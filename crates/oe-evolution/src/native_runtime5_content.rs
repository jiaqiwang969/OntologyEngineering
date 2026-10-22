//! Runtime /5 diagnostic CONTENT only. Neither paths nor textual UUID reports
//! authorize a loader environment, qualify a process, or establish image use.
//! Legacy stage3/context4, DYLD guards and native SDK3 execution are unchanged.
use anyhow::{Context, Result, ensure};
use oe_contracts::{CodeNativeLoaderDiagnosticLine5, CodeNativeLoaderDiagnosticReport5};
use oe_core::sha256;

const RAW_LIMIT: usize = 4 * 1024 * 1024;
const LINE_LIMIT: usize = 64 * 1024;
const LINE_COUNT_LIMIT: usize = 100_000;
const DOMAINS: &[&str] = &[
    "host_compiler_version",
    "host_cargo_version",
    "native_tool_version",
    "host_compilation",
    "host_proc_macro",
    "target_linker",
    "final_runner",
];
pub(crate) fn unknown() -> Vec<String> {
    [
        "declared_process_domain_unverified",
        "process_budget_and_original_cargo_source_binding_unverified",
        "reported_uuid_path_is_not_image_bytes_used_or_load_completion",
        "delayed_and_unrecognized_states_retained_not_interpreted",
        "cargo_compiler_proc_macro_final_runner_runtime_closure_required_unknown",
        "os_cache_sdk_resources_and_all_platform_closure_required_unknown",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'A'..=b'F').contains(&b)
            }
        })
}
pub(crate) fn line(
    ordinal: usize,
    byte_offset: usize,
    raw_text: &str,
) -> Result<CodeNativeLoaderDiagnosticLine5> {
    ensure!(
        raw_text.len() <= LINE_LIMIT,
        "native5_diagnostic_line_limit"
    );
    let mut value = CodeNativeLoaderDiagnosticLine5 {
        ordinal: u64::try_from(ordinal)?,
        byte_offset: u64::try_from(byte_offset)?,
        raw_text: raw_text.into(),
        event: "unrecognized".into(),
        reported_pid: None,
        reported_uuid: None,
        reported_path: None,
    };
    let body = raw_text.strip_suffix('\n').unwrap_or(raw_text);
    let parsed = body
        .strip_prefix("dyld[")
        .and_then(|s| s.split_once("]: "))
        .and_then(|(pid, text)| {
            (!pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
                .then(|| pid.parse::<u32>().ok())
                .flatten()
                .filter(|pid| *pid > 0)
                .map(|pid| (u64::from(pid), text))
        });
    if let Some((pid, text)) = parsed {
        if let Some((id, path)) = text.strip_prefix('<').and_then(|s| s.split_once("> ")) {
            if uuid(id) && path.starts_with('/') && !path.chars().any(char::is_control) {
                value.event = "uuid_path_report".into();
                value.reported_pid = Some(pid);
                value.reported_uuid = Some(id.into());
                value.reported_path = Some(path.into());
            }
        } else if text
            .strip_prefix("move loaded to delayed: ")
            .is_some_and(|s| !s.is_empty() && !s.chars().any(char::is_control))
        {
            // No basename-to-path inference: several images may share a name.
            value.event = "delayed_state_report".into();
            value.reported_pid = Some(pid);
        }
    }
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("native5_diagnostic_line_contract:{e:?}"))?;
    Ok(value)
}
/// Preserve complete bounded raw diagnostic bytes and every original line.
/// `declared_domain` is a caller label, never inferred execution authority.
pub fn inspect_loader_diagnostic_bytes(
    declared_domain: &str,
    raw: &[u8],
) -> Result<CodeNativeLoaderDiagnosticReport5> {
    ensure!(
        DOMAINS.contains(&declared_domain),
        "native5_diagnostic_domain_unsupported"
    );
    ensure!(
        raw.len() <= RAW_LIMIT && !raw.contains(&0),
        "native5_diagnostic_raw_limit_or_nul"
    );
    let text = std::str::from_utf8(raw).context("native5_diagnostic_utf8")?;
    let mut lines = Vec::new();
    let mut byte_offset = 0usize;
    for original in text.split_inclusive('\n') {
        ensure!(
            lines.len() < LINE_COUNT_LIMIT,
            "native5_diagnostic_line_count_limit"
        );
        lines.push(line(lines.len(), byte_offset, original)?);
        byte_offset = byte_offset
            .checked_add(original.len())
            .context("native5_diagnostic_offset_overflow")?;
    }
    ensure!(
        byte_offset == raw.len(),
        "native5_diagnostic_full_raw_not_retained"
    );
    let report = CodeNativeLoaderDiagnosticReport5 {
        schema: "oev2.native-loader-diagnostic-content/5".into(),
        declared_domain: declared_domain.into(),
        raw_sha256: sha256(raw),
        raw_bytes: u64::try_from(raw.len())?,
        lines,
        qualification: "diagnostic_content_only".into(),
        unknown: unknown(),
    };
    report
        .validate()
        .map_err(|e| anyhow::anyhow!("native5_diagnostic_report_contract:{e:?}"))?;
    Ok(report)
}
/// Exact content replay checks line bytes/order/coordinates and unknown state.
/// Passing this check still grants no process or loader execution qualification.
pub fn verify_loader_diagnostic_bytes(
    declared_domain: &str,
    raw: &[u8],
    report: &CodeNativeLoaderDiagnosticReport5,
) -> Result<()> {
    let replay = inspect_loader_diagnostic_bytes(declared_domain, raw)?;
    ensure!(
        serde_json::to_value(&replay)? == serde_json::to_value(report)?,
        "native5_diagnostic_content_changed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "5FB385BE-2B4C-31AE-AE92-ED6355A92C2D";
    #[test]
    fn original_reports_delayed_states_and_other_lines_keep_exact_order() {
        let text = format!(
            "dyld[16110]: <{ID}> /exact/image\ndyld[16110]: move loaded to delayed: image\noriginal error text\n"
        );
        let r = inspect_loader_diagnostic_bytes("host_compiler_version", text.as_bytes()).unwrap();
        assert_eq!(
            r.lines.iter().map(|l| l.event.as_str()).collect::<Vec<_>>(),
            ["uuid_path_report", "delayed_state_report", "unrecognized"]
        );
        assert_eq!(r.lines[0].reported_uuid.as_deref(), Some(ID));
        assert_eq!(r.lines[0].reported_path.as_deref(), Some("/exact/image"));
        assert!(r.lines[1].reported_uuid.is_none() && r.lines[1].reported_path.is_none());
        assert_eq!(
            r.lines
                .iter()
                .map(|l| l.raw_text.as_str())
                .collect::<String>(),
            text
        );
        verify_loader_diagnostic_bytes("host_compiler_version", text.as_bytes(), &r).unwrap();
    }
    #[test]
    fn malformed_pid_uuid_or_path_are_retained_as_unrecognized() {
        let original = format!(
            "dyld[0]: <{ID}> /a\ndyld[4294967296]: <{ID}> /b\ndyld[x]: <{ID}> /c\ndyld[1]: <bad> /d\ndyld[1]: <{ID}> relative\ndyld[1]: move loaded to delayed: \n"
        );
        let r = inspect_loader_diagnostic_bytes("target_linker", original.as_bytes()).unwrap();
        assert_eq!(r.lines.len(), 6);
        assert!(
            r.lines
                .iter()
                .all(|l| l.event == "unrecognized" && l.reported_pid.is_none())
        );
        assert_eq!(
            r.lines
                .iter()
                .map(|l| l.raw_text.as_str())
                .collect::<String>(),
            original
        );
    }
    #[test]
    fn multiple_reported_process_ids_do_not_gain_process_authority() {
        let original = format!("dyld[2]: <{ID}> /a\ndyld[3]: <{ID}> /b\n");
        let r = inspect_loader_diagnostic_bytes("final_runner", original.as_bytes()).unwrap();
        assert_eq!(r.lines[0].reported_pid, Some(2));
        assert_eq!(r.lines[1].reported_pid, Some(3));
        assert_eq!(r.qualification, "diagnostic_content_only");
        assert_eq!(r.unknown, unknown());
    }
    #[test]
    fn unicode_byte_offsets_and_missing_last_newline_are_preserved() {
        let original = "未理解\nlast line";
        let r = inspect_loader_diagnostic_bytes("host_cargo_version", original.as_bytes()).unwrap();
        assert_eq!(r.lines[1].byte_offset, 10);
        assert_eq!(r.lines[1].raw_text, "last line");
        assert_eq!(r.raw_bytes, original.len() as u64);
        assert_eq!(r.raw_sha256, sha256(original));
    }
    #[test]
    fn empty_raw_still_preserves_all_required_unknowns() {
        let r = inspect_loader_diagnostic_bytes("host_proc_macro", b"").unwrap();
        assert!(r.lines.is_empty());
        assert_eq!(r.unknown, unknown());
        assert_eq!(r.qualification, "diagnostic_content_only");
    }
    #[test]
    fn malformed_raw_and_size_limits_are_real_denials() {
        for input in [
            vec![0xff],
            vec![0],
            vec![b'a'; RAW_LIMIT + 1],
            vec![b'a'; LINE_LIMIT + 1],
            vec![b'\n'; LINE_COUNT_LIMIT + 1],
        ] {
            assert!(inspect_loader_diagnostic_bytes("host_compilation", &input).is_err());
        }
        assert!(inspect_loader_diagnostic_bytes("qualified_runtime", b"").is_err());
    }
    #[test]
    fn content_replay_denies_reordered_changed_or_qualified_reports() {
        let raw = format!("dyld[1]: <{ID}> /a\nother\n");
        let base = inspect_loader_diagnostic_bytes("native_tool_version", raw.as_bytes()).unwrap();
        let mut r = base.clone();
        r.lines.swap(0, 1);
        assert!(verify_loader_diagnostic_bytes("native_tool_version", raw.as_bytes(), &r).is_err());
        let mut r = base.clone();
        r.lines[1].byte_offset += 1;
        assert!(verify_loader_diagnostic_bytes("native_tool_version", raw.as_bytes(), &r).is_err());
        let mut r = base.clone();
        r.unknown.clear();
        r.qualification = "qualified".into();
        assert!(verify_loader_diagnostic_bytes("native_tool_version", raw.as_bytes(), &r).is_err());
        assert!(verify_loader_diagnostic_bytes("final_runner", raw.as_bytes(), &base).is_err());
        assert!(
            verify_loader_diagnostic_bytes("native_tool_version", b"changed raw\n", &base).is_err()
        );
    }
}
