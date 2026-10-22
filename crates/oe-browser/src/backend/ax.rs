//! Explicit Peekaboo CLI background observations. No DOM capability or account
//! verification is inferred from a window snapshot or screenshot.
use super::{BackendConfig, focus};
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, time::Duration};

fn command(
    root: &SkillRoot,
    config: &BackendConfig,
    arguments: Vec<String>,
    output: &Path,
) -> Result<Value> {
    let raw = oe_core::process::run_command_memory(
        &oe_core::process::CommandSpec {
            executable: config.peekaboo_executable.clone(),
            arguments: arguments.into_iter().map(Into::into).collect(),
            cwd: root.path().to_owned(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout: config.timeout.min(Duration::from_secs(30)),
            output_limit: 16 * 1024 * 1024,
        },
        output,
    )?;
    ensure!(
        raw.status == "completed" && raw.exit_code == Some(0),
        "peekaboo_ax_observation_unconfirmed"
    );
    let value = oe_core::json::strict_json(&raw.stdout)?;
    ensure!(
        value["success"] == true,
        "peekaboo_ax_observation_unconfirmed"
    );
    Ok(value)
}

pub fn select_window(inventory: &Value, wanted: u64) -> Result<&Value> {
    ensure!(
        inventory["success"] == true
            && inventory["data"]["targetApplication"]["bundleIdentifier"] == "com.google.Chrome",
        "peekaboo_chrome_window_inventory_required"
    );
    let candidates: Vec<_> = inventory["data"]["windows"]
        .as_array()
        .context("peekaboo_window_inventory_required")?
        .iter()
        .filter(|v| v["window_id"] == wanted)
        .collect();
    ensure!(
        wanted > 0 && candidates.len() == 1,
        "peekaboo_exact_window_identity_required"
    );
    let window = candidates[0];
    ensure!(
        window["layer"] == 0
            && window["title"]
                .as_str()
                .is_some_and(|v| !v.trim().is_empty())
            && window["isMinimized"] != true,
        "peekaboo_normal_browser_window_not_observed"
    );
    Ok(window)
}

/// Default inspection: actual application/window facts. An absent normal
/// window remains not_observed; no window is opened, focused or guessed.
pub fn observe(root: &SkillRoot, config: &BackendConfig, window_id: Option<u64>) -> Result<Value> {
    super::peekaboo::check_executable(root, config)?;
    let output = root.new_private_dir(&config.output)?;
    let before = focus::active_application(
        &config.peekaboo_executable,
        root.path(),
        Duration::from_secs(5),
        &output.join("before"),
    )?;
    let inventory = command(
        root,
        config,
        [
            "list",
            "windows",
            "--app",
            "com.google.Chrome",
            "--json",
            "--no-remote",
        ]
        .map(str::to_owned)
        .to_vec(),
        &output.join("windows-before"),
    )?;
    ensure!(
        inventory["data"]["targetApplication"]["bundleIdentifier"] == "com.google.Chrome",
        "peekaboo_chrome_window_inventory_required"
    );
    let chrome_pid = inventory["data"]["targetApplication"]["processIdentifier"]
        .as_u64()
        .filter(|v| *v > 0 && *v < i32::MAX as u64)
        .context("peekaboo_chrome_pid_required")? as u32;
    let chrome_instance = focus::process_start(chrome_pid)?;
    write_new(&output.join("window-inventory.json"), &inventory)?;
    let mut result = json!({"schema":"oev2.peekaboo-ax-observation/1","status":"window_inventory_observed",
        "backend":"peekaboo_background","observation_kind":"native_ax_window_inventory","chrome_pid":chrome_pid,
        "window_id":window_id,"page_observation":"not_run","dom_connection":"not_available",
        "dom_reason":"verified_existing_cdp_endpoint_not_supplied","connection_verified":false,
        "transport_process_started":false,"profile_directory_accessed":false,
        "window_inventory":inventory["data"]["windows"],
        "account_verified":false,"browser_actions":0,"model_calls":0,"focus_actions":0,"storage_accessed":false,
        "browser_started":false,"settings_changed":false,"semantic_execution":"not_run","engineering_acceptance":"not_evaluated"});
    let eligible: Vec<_> = inventory["data"]["windows"]
        .as_array()
        .context("peekaboo_window_inventory_required")?
        .iter()
        .filter_map(|w| w["window_id"].as_u64())
        .filter(|id| select_window(&inventory, *id).is_ok())
        .collect();
    result["eligible_normal_window_ids"] = json!(eligible);
    if let Some(id) = window_id {
        let observed = select_window(&inventory, id)?;
        let image = output.join("window.png");
        let value = command(
            root,
            config,
            vec![
                "see".into(),
                "--pid".into(),
                chrome_pid.to_string(),
                "--window-id".into(),
                id.to_string(),
                "--mode".into(),
                "window".into(),
                "--no-web-focus".into(),
                "--json".into(),
                "--no-remote".into(),
                "--path".into(),
                image.to_string_lossy().into_owned(),
            ],
            &output.join("see"),
        )?;
        let after_inventory = command(
            root,
            config,
            [
                "list",
                "windows",
                "--app",
                "com.google.Chrome",
                "--json",
                "--no-remote",
            ]
            .map(str::to_owned)
            .to_vec(),
            &output.join("windows-after"),
        )?;
        ensure!(
            after_inventory["data"]["targetApplication"]["processIdentifier"] == chrome_pid,
            "peekaboo_window_changed_during_observation"
        );
        ensure!(
            select_window(&after_inventory, id)? == observed,
            "peekaboo_window_changed_during_observation"
        );
        let bytes = read_regular_bytes(&image, 64 * 1024 * 1024)?;
        ensure!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "peekaboo_target_image_not_png"
        );
        let artifact = oe_contracts::ArtifactReference {
            path: image
                .strip_prefix(root.path())?
                .to_string_lossy()
                .into_owned(),
            sha256: sha256(&bytes),
        };
        ensure!(
            artifact.validate().is_ok(),
            "peekaboo_image_reference_invalid"
        );
        write_new(&output.join("ax-observation.json"), &value)?;
        result["status"] = json!("ax_window_observed");
        result["observation_kind"] = json!("native_ax_window");
        result["page_observation"] = json!("background_window_ax_only");
        result["image"] = serde_json::to_value(artifact)?;
        result["snapshot_sha256"] = json!(oe_core::json::canonical_digest(&value)?);
        result["dom_target_or_account_identity"] = json!("not_verified_by_ax_snapshot");
    }
    let after = focus::active_application(
        &config.peekaboo_executable,
        root.path(),
        Duration::from_secs(5),
        &output.join("after"),
    )?;
    ensure!(
        before == after && focus::process_start(chrome_pid)? == chrome_instance,
        "peekaboo_active_application_or_chrome_changed"
    );
    result["focus_before"] = before;
    result["focus_after"] = after;
    result["active_application_unchanged"] = json!(true);
    write_new(&output.join("result.json"), &result)?;
    Ok(result)
}
