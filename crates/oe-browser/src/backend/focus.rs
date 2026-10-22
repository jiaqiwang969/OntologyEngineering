//! Fixed macOS process/focus observations. No activation, tab creation, storage
//! reads or credentials are part of these native programs.
use super::BackendConfig;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, create_new_private_dir, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChromeInstance {
    pid: u32,
    started_seconds: u64,
    started_microseconds: u64,
}
impl ChromeInstance {
    pub(super) fn peekaboo_session(&self, worker: &str) -> String {
        format!(
            "peekaboo-background:{worker}:{}:{}:{}",
            self.pid, self.started_seconds, self.started_microseconds
        )
    }
    pub(super) fn session(&self) -> String {
        format!(
            "chrome-apple-events:{}:{}:{}",
            self.pid, self.started_seconds, self.started_microseconds
        )
    }
}

/// Independently movable into the supplier's bounded sampling thread. Each
/// sample is timestamped by its process receipt; sampling is not continuous
/// proof of foreground invariance between observations.
pub struct FocusProbe {
    output: PathBuf,
    cwd: PathBuf,
    peekaboo: PathBuf,
    timeout: Duration,
    serial: u64,
}
impl FocusProbe {
    pub fn new(root: &SkillRoot, config: &BackendConfig) -> Result<Self> {
        ensure!(
            cfg!(target_os = "macos"),
            "focus_probe_unsupported_platform"
        );
        ensure!(
            !config.timeout.is_zero() && config.timeout <= Duration::from_secs(5),
            "focus_probe_timeout_out_of_range"
        );
        let output = root.private_path(&config.output)?;
        create_new_private_dir(&output)?;
        Ok(Self {
            output,
            cwd: root.path().to_owned(),
            peekaboo: config.peekaboo_executable.clone(),
            timeout: config.timeout,
            serial: 0,
        })
    }
    pub fn sample(&mut self) -> Result<Value> {
        self.serial += 1;
        active_application(
            &self.peekaboo,
            &self.cwd,
            self.timeout,
            &self.output.join(format!("{:06}", self.serial)),
        )
    }
}

/// Peekaboo OS observation only. Browser page focus is sampled separately by the transport.
pub(super) fn active_application(
    executable: &Path,
    cwd: &Path,
    timeout: Duration,
    output: &Path,
) -> Result<Value> {
    create_new_private_dir(output)?;
    let result = oe_core::process::run_command_memory(
        &oe_core::process::CommandSpec {
            executable: executable.to_owned(),
            arguments: ["list", "apps", "--json", "--no-remote"]
                .map(Into::into)
                .to_vec(),
            cwd: cwd.to_owned(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout,
            output_limit: 8 * 1024 * 1024,
        },
        &output.join("process"),
    )?;
    ensure!(
        result.status == "completed" && result.exit_code == Some(0),
        "peekaboo_focus_observation_unconfirmed"
    );
    let raw = oe_core::json::strict_json(&result.stdout)?;
    let sample = parse_active_application(&raw)?;
    oe_core::write_new(&output.join("observation.json"), &sample)?;
    Ok(sample)
}

pub fn parse_active_application(raw: &Value) -> Result<Value> {
    ensure!(
        raw["success"] == true,
        "peekaboo_focus_observation_unconfirmed"
    );
    let apps = raw["data"]["applications"]
        .as_array()
        .context("peekaboo_applications_required")?;
    let active: Vec<_> = apps.iter().filter(|v| v["isActive"] == true).collect();
    ensure!(active.len() == 1, "peekaboo_active_application_ambiguous");
    let app = active[0]["bundleIdentifier"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 512 && !v.chars().any(char::is_control))
        .context("peekaboo_application_identity_required")?;
    let pid = active[0]["processIdentifier"]
        .as_u64()
        .filter(|v| *v > 0 && *v < i32::MAX as u64)
        .context("peekaboo_application_pid_required")?;
    Ok(
        json!({"app":app,"pid":pid,"observer":"peekaboo_list_apps","foreground_window_tab_identity":"not_observed"}),
    )
}

pub(super) fn chrome_instance(
    osascript: &Path,
    cwd: &Path,
    timeout: Duration,
    output: &Path,
) -> Result<ChromeInstance> {
    ensure!(
        cfg!(target_os = "macos"),
        "chrome_process_identity_unsupported_platform"
    );
    let text = run_apple(osascript, cwd, timeout, output, CHROME_PID)?;
    let pid = text
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|v| *v > 0 && *v < i32::MAX as u32)
        .context("chrome_process_identity_invalid")?;
    process_start(pid)
}

#[cfg(target_os = "macos")]
pub(super) fn process_start(pid: u32) -> Result<ChromeInstance> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as libc::c_int;
    let result = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    ensure!(
        result == size
            && info.pbi_pid == pid
            && info.pbi_uid == unsafe { libc::geteuid() }
            && info.pbi_start_tvsec > 0,
        "chrome_process_start_unconfirmed"
    );
    Ok(ChromeInstance {
        pid,
        started_seconds: info.pbi_start_tvsec,
        started_microseconds: info.pbi_start_tvusec,
    })
}
#[cfg(not(target_os = "macos"))]
pub(super) fn process_start(_pid: u32) -> Result<ChromeInstance> {
    anyhow::bail!("chrome_process_identity_unsupported_platform")
}

pub(super) fn run_apple(
    osascript: &Path,
    cwd: &Path,
    timeout: Duration,
    output: &Path,
    script: &str,
) -> Result<String> {
    create_new_private_dir(output)?;
    let input = output.join("input.applescript");
    write_new_bytes(&input, script.as_bytes())?;
    let result = oe_core::process::run_command(
        &oe_core::process::CommandSpec {
            executable: osascript.to_owned(),
            arguments: vec![input.into_os_string()],
            cwd: cwd.to_owned(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout,
            output_limit: 4096,
        },
        &output.join("process"),
    )?;
    ensure!(
        result.status == "completed" && result.exit_code == Some(0),
        "native_apple_observation_unconfirmed"
    );
    String::from_utf8(result.stdout).context("native_apple_observation_invalid")
}
const CHROME_PID: &str = r#"if application "Google Chrome" is not running then error "Chrome is not running"
tell application "System Events"
 set observed_pids to unix id of every application process whose bundle identifier is "com.google.Chrome"
 if (count of observed_pids) is not 1 then error "Chrome process identity is ambiguous"
 return (item 1 of observed_pids) as text
end tell
"#;
