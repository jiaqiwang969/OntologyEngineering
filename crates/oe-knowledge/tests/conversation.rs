use oe_knowledge::conversation::{self, Entry};
use serde_json::{Value, json};
use std::fs;

#[test]
fn wrapped_home_paths_are_masked_and_a_bare_prefix_cannot_hide_a_later_path() {
    // Construct the platform-shaped synthetic input; no concrete personal home
    // directory belongs in the distributed test source.
    let home = std::path::Path::new("/").join("Users");
    let text = format!(
        "at {0}/\noperator/.config/tool; bare {0}/ then {0}/other/project; correction: acceptance was withdrawn",
        home.display()
    );
    let projected = conversation::model_text(&text).unwrap();
    assert!(!projected.contains("operator"));
    assert!(!projected.contains("/Users/"));
    assert!(!projected.contains("other/project"));
    assert!(projected.contains("acceptance was withdrawn"));
    assert!(oe_core::privacy::inspect_text(&serde_json::to_string(&projected).unwrap()).is_empty());
}

fn file(records: &[Value]) -> (tempfile::TempDir, std::path::PathBuf) {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("var/maintenance/rust-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("history.jsonl");
    let raw = records.iter().map(|v| format!("{v}\n")).collect::<String>();
    fs::write(&path, raw).unwrap();
    (temp, path)
}

#[test]
fn claude_visible_source_excludes_reasoning_and_preserves_pair_identity() {
    let (_temp, path) = file(&[
        json!({"type":"assistant","uuid":"a","parentUuid":"p","timestamp":"t","message":{"role":"assistant","content":[{"type":"thinking","thinking":"never projected"},{"type":"text","text":"可检验的条件😀"},{"type":"tool_use","id":"call","name":"NX","input":{"secret":"not projected"}}]}}),
        json!({"type":"user","uuid":"b","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":[{"type":"text","text":"reported result"}]}]}}),
    ]);
    let result = conversation::read_archive(&path, 100_000).unwrap();
    assert_eq!(result["status"], "complete");
    let entries: Vec<Entry> = serde_json::from_value(result["entries"].clone()).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "可检验的条件😀");
    assert_eq!(entries[1].call_id, entries[2].call_id);
    assert!(!result.to_string().contains("never projected"));
    assert!(!result.to_string().contains("not projected"));
    for entry in &entries {
        conversation::verify_entry(&path, entry).unwrap();
    }
    let mut changed = entries[0].clone();
    changed.text = "invented".into();
    assert!(conversation::verify_entry(&path, &changed).is_err());
    changed = entries[0].clone();
    changed.timestamp = Some("different".into());
    assert!(conversation::verify_entry(&path, &changed).is_err());
}

#[test]
fn codex_event_mirrors_runtime_instructions_and_analysis_do_not_become_experience() {
    let (_temp, path) = file(&[
        json!({"type":"session_meta","payload":{"id":"session","cwd":"project"}}),
        json!({"type":"event_msg","payload":{"type":"user_message","message":"same"}}),
        json!({"type":"response_item","payload":{"type":"message","id":"1","role":"user","content":[{"type":"input_text","text":"same"}]}}),
        json!({"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"runtime policy"}]}}),
        json!({"type":"response_item","payload":{"type":"message","role":"assistant","channel":"analysis","content":[{"type":"output_text","text":"private reasoning"}]}}),
        json!({"type":"response_item","payload":{"type":"message","role":"assistant","channel":"final","content":[{"type":"output_text","text":"public answer"}]}}),
    ]);
    let result = conversation::read_archive(&path, 100_000).unwrap();
    assert_eq!(result["entries"].as_array().unwrap().len(), 2);
    assert_eq!(result["metadata"]["format"], "codex");
    assert!(!result.to_string().contains("private reasoning"));
    assert!(!result.to_string().contains("runtime policy"));
}

#[test]
fn corruption_and_unfinished_records_remain_partial_and_original_change_is_detected() {
    let (_temp, path) =
        file(&[json!({"type":"user","uuid":"a","message":{"role":"user","content":"original"}})]);
    let mut raw = fs::read(&path).unwrap();
    raw.extend_from_slice(b"not JSON\n{unfinished");
    fs::write(&path, &raw).unwrap();
    let result = conversation::read_archive(&path, 100_000).unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["issues"].as_array().unwrap().len(), 2);
    let entry: Entry = serde_json::from_value(result["entries"][0].clone()).unwrap();
    conversation::verify_entry(&path, &entry).unwrap();
    fs::write(
        &path,
        String::from_utf8(raw)
            .unwrap()
            .replace("original", "modified"),
    )
    .unwrap();
    assert!(conversation::verify_entry(&path, &entry).is_err());
}

#[test]
fn byte_budget_never_publishes_a_cut_record_as_complete() {
    let (_temp, path) =
        file(&[json!({"type":"user","message":{"role":"user","content":"unicode😀"}})]);
    let result = conversation::read_archive(&path, 17).unwrap();
    assert_eq!(result["status"], "partial");
    assert!(result["entries"].as_array().unwrap().is_empty());
    assert_eq!(result["scanned_bytes"], 17);
}

#[test]
fn equal_text_in_distinct_turns_is_retained_but_exact_identified_mirrors_are_deduplicated() {
    let a = json!({"type":"user","uuid":"a","message":{"role":"user","content":"yes"}});
    let b = json!({"type":"user","uuid":"b","message":{"role":"user","content":"yes"}});
    let (_temp, path) = file(&[a.clone(), a, b]);
    assert_eq!(
        conversation::read_archive(&path, 100_000).unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn oversized_record_is_bounded_and_not_reinterpreted_as_valid_empty_history() {
    let (_temp, path) = file(&[
        json!({"type":"user","message":{"role":"user","content":"x".repeat(conversation::MAX_RECORD_BYTES)}}),
    ]);
    let result = conversation::read_archive(&path, 10_000_000).unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["issues"][0]["code"], "record_exceeds_memory_guard");
}

#[test]
fn model_projection_redacts_home_component_and_blocks_known_credential_patterns() {
    assert_eq!(
        conversation::model_text(&format!(
            "查看 {}/alice/project 和 {}/bob/file",
            "/Users", "/home"
        ))
        .unwrap(),
        "查看 [private-root]/project 和 [private-root]/file"
    );
    let secret = format!("{}{}", "sk-", "a".repeat(30));
    assert!(conversation::model_text(&secret).is_err());
    assert_eq!(
        conversation::model_text("保留条件、反例、未知😀").unwrap(),
        "保留条件、反例、未知😀"
    );
}

#[test]
fn system_injected_user_shaped_notification_stays_context_with_a_source_locator() {
    let (_temp, path) = file(&[
        json!({"type":"user","uuid":"notification","promptSource":"system",
        "origin":{"kind":"task-notification"},"message":{"role":"user","content":"Archived runtime notice"}}),
    ]);
    let result = conversation::read_archive(&path, 100_000).unwrap();
    let entry: Entry = serde_json::from_value(result["entries"][0].clone()).unwrap();
    assert_eq!(entry.kind, "runtime_context");
    assert_eq!(entry.role, "context");
    conversation::verify_entry(&path, &entry).unwrap();
}

#[test]
fn important_project_method_requirements_survive_reuse_filter_without_becoming_facts() {
    // The live pilot classified the book-kernel and infrastructure-order
    // requirements as project_specific while identifying architecture content.
    assert_eq!(
        conversation::review_disposition(Some("project_specific"), Some("architecture")).unwrap(),
        "source_review"
    );
    assert_eq!(
        conversation::review_disposition(Some("project_specific"), Some("neither")).unwrap(),
        "project_history"
    );
    assert_eq!(
        conversation::review_disposition(None, Some("architecture")).unwrap(),
        "needs_context"
    );
    assert_eq!(
        conversation::review_disposition(Some("unrelated"), Some("not_established")).unwrap(),
        "needs_context"
    );
    assert!(conversation::review_disposition(Some("verified"), Some("architecture")).is_err());
}

#[test]
fn stream_reads_large_visible_interventions_and_the_later_withdrawal_without_collecting() {
    let (_temp, path) = file(&[
        json!({"type":"response_item","payload":{"type":"function_call","name":"solver",
            "arguments":"{\"time_step\":0.001}","call_id":"c"}}),
        json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"c",
            "output":"x".repeat(conversation::MAX_RECORD_BYTES+1)}}),
        json!({"type":"response_item","payload":{"type":"message","role":"user",
            "content":[{"type":"input_text","text":"此前通过已撤回，仍需验证接触力。"}]}}),
    ]);
    let mut entries = Vec::new();
    let report = conversation::visit_archive(
        &path,
        conversation::ScanOptions {
            max_bytes: u64::MAX,
            max_record_bytes: 4 * 1024 * 1024,
            include_tool_arguments: true,
        },
        |event| {
            if let conversation::ScanEvent::Entry(entry) = event {
                entries.push(entry);
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(report["status"], "complete");
    assert_eq!(report["entry_count"], 4);
    assert_eq!(report["scanned_bytes"], fs::metadata(&path).unwrap().len());
    assert!(report.get("entries").is_none());
    assert_eq!(entries[1].kind, "tool_call_arguments");
    assert_eq!(entries[1].call_id, entries[2].call_id);
    assert!(entries[3].text.contains("撤回"));
    for entry in entries {
        conversation::verify_entry(&path, &entry).unwrap();
    }
}

#[test]
fn stream_keeps_every_gap_locator_and_propagates_a_failed_consumer() {
    let (_temp, path) =
        file(&[json!({"type":"user","message":{"role":"user","content":"condition"}})]);
    let err = conversation::visit_archive(
        &path,
        conversation::ScanOptions {
            max_bytes: u64::MAX,
            max_record_bytes: conversation::MAX_RECORD_BYTES,
            include_tool_arguments: false,
        },
        |_| anyhow::bail!("consumer_failed"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "consumer_failed");
    assert!(
        conversation::visit_archive(
            &path,
            conversation::ScanOptions {
                max_bytes: u64::MAX,
                max_record_bytes: 1,
                include_tool_arguments: false,
            },
            |_| Ok(())
        )
        .is_err()
    );
}

#[test]
fn privacy_gaps_keep_safe_context_and_do_not_expose_a_private_key_body() {
    let token = format!("{}{}", "sk-", "a".repeat(30));
    let source = format!("条件：改变步长\n凭据={token}\n结果：误差增大\n");
    let p = conversation::project_visible_text(&source).unwrap();
    assert!(p["text"].as_str().unwrap().contains("误差增大"));
    assert!(!p.to_string().contains(&token));
    assert_eq!(p["omissions"].as_array().unwrap().len(), 1);
    assert!(oe_core::privacy::inspect_text(p["text"].as_str().unwrap()).is_empty());
    let source = [
        "-----BEGIN ",
        "PRIVATE KEY-----\nunmarked body\n-----END PRIVATE KEY-----",
    ]
    .concat();
    let p = conversation::project_visible_text(&source).unwrap();
    assert!(!p.to_string().contains("unmarked body"));
    assert_eq!(p["omissions"][0]["byte_end"], source.len());
}

#[test]
fn optional_argument_projection_cannot_reintroduce_nonpublic_roles_or_record_types() {
    for (record_type, role) in [("system", "assistant"), ("user", "developer")] {
        let record = json!({"type":record_type,"message":{"role":role,"content":[
            {"type":"tool_use","id":"runtime","name":"runtime setup","input":{"policy":"never projected"}}
        ]}});
        assert!(conversation::visible_record_with_arguments(&record).is_empty());
    }
}

#[test]
fn window_retains_absolute_locators_and_rejects_unaligned_boundaries() {
    let (_temp, path) = file(&[
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"prior purpose"}]}}),
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"correct the physical mechanism"}]}}),
        json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"retain unknown engineering outcome"}]}}),
    ]);
    let raw = fs::read(&path).unwrap();
    let start = (raw.iter().position(|b| *b == b'\n').unwrap() + 1) as u64;
    let window = conversation::read_window(&path, start, raw.len() as u64).unwrap();
    assert_eq!(window["status"], "window_complete");
    assert_eq!(window["window"]["line_origin"], "range_relative");
    assert!(window["scanned_prefix_sha256"].is_null());
    let entries: Vec<Entry> = serde_json::from_value(window["entries"].clone()).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].byte_start, start);
    assert_eq!(entries[0].line, 1);
    conversation::verify_entry(&path, &entries[0]).unwrap();
    assert!(conversation::read_window(&path, start + 1, raw.len() as u64).is_err());
    let mut changed = raw;
    changed[start as usize + 20] ^= 1;
    fs::write(&path, changed).unwrap();
    assert!(
        conversation::verify_entry(&path, &entries[0]).is_err(),
        "exact raw source changes invalidate the projected evidence"
    );
}

#[test]
fn codex_user_shaped_runtime_context_is_located_but_is_not_a_user_requirement() {
    let records=["# AGENTS.md instructions\nRuntime rules","<environment_context>machine context</environment_context>",
        "<codex_internal_context>automatic goal reminder</codex_internal_context>","请保持加工孔的对称性"]
        .into_iter().map(|text|json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":text}]}})).collect::<Vec<_>>();
    let (_t, path) = file(&records);
    let r = conversation::read_archive(&path, 100_000).unwrap();
    let entries: Vec<Entry> = serde_json::from_value(r["entries"].clone()).unwrap();
    assert_eq!(entries.len(), 4);
    for e in &entries[..3] {
        assert_eq!(e.role, "context");
        assert_eq!(e.kind, "runtime_context");
        conversation::verify_entry(&path, e).unwrap();
    }
    assert_eq!(entries[3].role, "user");
    assert!(entries[3].text.contains("对称性"));
}
