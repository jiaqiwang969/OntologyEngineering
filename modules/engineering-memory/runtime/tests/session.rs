//! Session/Project isolation and stale-context counterexamples through the real store and CLI.
use engineering_memory_runtime::contracts::*;
use engineering_memory_runtime::evidence::{EvidenceArgs, read_window};
use engineering_memory_runtime::session::verify_project_binding;
use engineering_memory_runtime::store::{Store, read_json, sha};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    store: Store,
    record: ExperienceRecord,
    project: SessionProject,
}

fn bind(path: &Path) -> SessionProject {
    let bytes = fs::read(path).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    SessionProject {
        path: path.canonicalize().unwrap().to_str().unwrap().into(),
        project_id: value["project_id"].as_str().unwrap().into(),
        revision: value["revision"].as_str().unwrap().into(),
        sha256: sha(&bytes),
        object: Some(SessionObject {
            object_id: "device".into(),
            object_revision: value["objects"]["device"].as_str().unwrap().into(),
            condition_scope: "inspection".into(),
            condition_sha256: sha(value["conditions"]["inspection"].as_str().unwrap()),
        }),
    }
}

fn project(path: &Path, project_id: &str) -> SessionProject {
    fs::write(
        path,
        serde_json::to_vec(&json!({
            "schema":"oev2.project/1","project_id":project_id,"revision":"r1",
            "purpose":"合成软件集成检查，非实际设备验收",
            "objects":{"device":"v1"},"conditions":{"inspection":"离散采样，连续过程未知"},
            "evidence":{},"decisions":[],"dependencies":{}
        }))
        .unwrap(),
    )
    .unwrap();
    bind(path)
}

fn fixture() -> Fixture {
    let temp = TempDir::new().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let original = root.join("source.jsonl");
    fs::write(
        &original,
        format!(
            "{}\n",
            json!({
                "type":"assistant","sessionId":"source-session","uuid":"source-message",
                "timestamp":"2026-10-11T00:00:00Z",
                "message":{"role":"assistant","content":[{"type":"text",
                "text":"先声称全部通过，随后纠正：只能支持离散采样。合成测试，不证明工程收益。"}]}
            })
        ),
    )
    .unwrap();
    let source = read_window(&EvidenceArgs {
        source: original,
        provider: "claude".into(),
        profile: Some("profile-a".into()),
        start_byte: 0,
        end_byte: None,
        max_record_bytes: 2 * 1024 * 1024,
        json: true,
    })
    .unwrap()
    .records[0]
        .clone();
    let record = ExperienceRecord {
        schema: "engineering-memory.experience/2".into(),
        id: "lesson-one".into(),
        revision: 1,
        supersedes: None,
        project_id: Some("project-one".into()),
        conditions: vec!["discrete-sampling".into()],
        purpose: "检验结论的边界".into(),
        status: "reviewed".into(),
        recorded_at: "2026-10-11T00:00:00Z".into(),
        timeline: vec![TimelineEvent {
            id: "claim".into(),
            kind: "assistant_claim".into(),
            occurred_at: source.event_at.clone(),
            description: "当时声称全部通过".into(),
            source_sha256: vec![source.sha256.clone()],
            corrects: vec![],
        }],
        sources: vec![source],
        causal_claims: vec![],
        method: Some(Method {
            purpose: "限制结论".into(),
            mechanism: "按实际覆盖范围".into(),
            validation: "独立反例".into(),
        }),
        unknowns: vec!["未检验连续路径".into()],
    };
    let store = Store::new(root.join("memory"));
    store.record(&record).unwrap();
    let project = project(&root.join("project.json"), "project-one");
    Fixture {
        _temp: temp,
        root,
        store,
        record,
        project,
    }
}

fn request(f: &Fixture, id: &str) -> SessionRequest {
    SessionRequest {
        schema: "engineering-memory.session-request/1".into(),
        id: id.into(),
        provider: "codex".into(),
        session_id: "current-session".into(),
        task: "确定检验覆盖的边界".into(),
        phase: "decision".into(),
        project: Some(f.project.clone()),
        conditions: vec!["discrete-sampling".into()],
        query: "".into(),
        include_other_projects: false,
        max_candidates: 5,
        max_context_bytes: 8192,
    }
}

fn export_request(context: &Value, record: &ExperienceRecord, name: &str) -> SessionExportRequest {
    SessionExportRequest {
        schema: "engineering-memory.session-export-request/1".into(),
        id: name.into(),
        context_id: context["session"]["request"]["id"].as_str().unwrap().into(),
        context_sha256: context["context_sha256"].as_str().unwrap().into(),
        record_id: record.id.clone(),
        revision: record.revision,
        max_bytes: 4 * 1024 * 1024,
    }
}

/// Explicit opt-in: this executes the installed v2 program, never a fake receiver.
#[test]
#[ignore = "requires ENGINEERING_MEMORY_TEST_V2_ROOT pointing to a registered native v2 installation"]
fn native_receiver_keeps_both_provenance_sides_current() {
    let receiver_root = PathBuf::from(
        std::env::var_os("ENGINEERING_MEMORY_TEST_V2_ROOT")
            .expect("explicit native v2 root required"),
    )
    .canonicalize()
    .unwrap();
    let installation = receiver_root.join("var/install/INSTALLATION.json");
    let digest = if installation.exists() {
        let state: Value = read_json(&installation).unwrap();
        state["programs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|program| program["name"] == "oev2")
            .unwrap()["sha256"]
            .clone()
    } else {
        let state: Value = read_json(&receiver_root.join("var/maintenance/status.json")).unwrap();
        state["current_cli"]["default"]["sha256"].clone()
    };
    let receiver = V2Receiver {
        root: receiver_root.to_str().unwrap().into(),
        cli_sha256: digest.as_str().unwrap().into(),
    };
    for provider in ["claude", "codex"] {
        let mut f = fixture();
        let original = f.root.join(format!("native-{provider}.jsonl"));
        let texts = [
            "声称连续路径通过。",
            "纠正：只观察了离散采样，连续路径未知。",
        ];
        let message = if provider == "claude" {
            json!({"type":"assistant","sessionId":"native-source","uuid":"native-message",
                "timestamp":"2026-10-11T00:00:00Z","message":{"role":"assistant",
                "content":texts.iter().map(|text|json!({"type":"text","text":text})).collect::<Vec<_>>()}})
        } else {
            json!({"type":"response_item","timestamp":"2026-10-11T00:00:00Z",
                "payload":{"type":"message","role":"assistant",
                "content":texts.iter().map(|text|json!({"type":"output_text","text":text})).collect::<Vec<_>>()}})
        };
        let header = if provider == "codex" {
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":"native-source"}})
            )
        } else {
            String::new()
        };
        fs::write(&original, format!("{header}{message}\n")).unwrap();
        let source = read_window(&EvidenceArgs {
            source: original,
            provider: provider.into(),
            profile: Some("test".into()),
            start_byte: 0,
            end_byte: None,
            max_record_bytes: 2 * 1024 * 1024,
            json: true,
        })
        .unwrap()
        .records[0]
            .clone();
        f.record.id = format!("native-{provider}");
        f.record.sources = vec![source.clone()];
        f.record.timeline[0].source_sha256 = vec![source.sha256];
        f.store.record(&f.record).unwrap();
        let directory = receiver_root
            .join("var/engineering-memory-tests")
            .join(format!(
                "{}-{}-{provider}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap()
            ));
        fs::create_dir_all(&directory).unwrap();
        let project_path = directory.join("project.json");
        project(&project_path, "project-one");
        let mut project: Value = read_json(&project_path).unwrap();
        let mut basis = json!({"schema":"oev2.engineering-basis/1","object_id":"device",
            "object_revision":"v1","purpose":"synthetic native receiver integration",
            "acceptance":["preserve source and unknowns; no engineering adoption"],"facts":[],"assertions":[]});
        project["conditions"]["inspection"] = json!(basis.to_string());
        project["evidence"]["design"] = json!(sha(b"test design"));
        fs::write(&project_path, serde_json::to_vec(&project).unwrap()).unwrap();
        f.project = bind(&project_path);
        let before = fs::read(&project_path).unwrap();
        let context = f
            .store
            .session_prepare(&request(&f, "native-before"))
            .unwrap();
        let prepare = V2PrepareRequest {
            schema: "engineering-memory.v2-prepare-request/1".into(),
            id: "native-prepare".into(),
            export: export_request(&context, &f.record, "native-before-export"),
            receiver: receiver.clone(),
            fact_property: "memory.test-method".into(),
            object_source_sha256: sha(b"test design"),
            max_seconds: 60,
        };
        let mut forged = prepare.clone();
        forged.receiver.cli_sha256 = "0".repeat(64);
        error(
            f.store.v2_prepare(&forged),
            "receiver_selected_identity_mismatch",
        );
        let prepared = f.store.v2_prepare(&prepare).unwrap();
        let again = f.store.v2_prepare(&prepare).unwrap();
        assert_eq!(prepared["preparation"], again["preparation"]);
        assert_ne!(prepared["run"], again["run"]);
        assert_eq!(fs::read(&project_path).unwrap(), before);
        let expected_entries = if provider == "claude" { 2 } else { 1 };
        assert_eq!(
            prepared["preparation"]["intakes"][0]["entry_ids"]
                .as_array()
                .unwrap()
                .len(),
            expected_entries
        );
        let mut verify = V2VerifyRequest {
            schema: "engineering-memory.v2-verify-request/1".into(),
            id: "native-verify".into(),
            preparation_id: prepare.id.clone(),
            export: prepare.export.clone(),
            previous: None,
            work: None,
            sources: vec![],
            max_seconds: 60,
        };
        error(
            f.store.v2_verify(&verify),
            "receiver_method_fact_missing_or_ambiguous",
        );

        // The receiving agent owns this edit; neither runtime command writes the Project.
        basis["facts"] = json!([prepared["preparation"]["proposal"]["fact"]]);
        project["revision"] = json!("r2");
        project["conditions"]["inspection"] = json!(basis.to_string());
        project["evidence"].as_object_mut().unwrap().extend(
            prepared["preparation"]["proposal"]["evidence"]
                .as_object()
                .unwrap()
                .clone(),
        );
        fs::write(&project_path, serde_json::to_vec(&project).unwrap()).unwrap();
        f.project = bind(&project_path);
        let context = f
            .store
            .session_prepare(&request(&f, "native-current"))
            .unwrap();
        verify.export = export_request(&context, &f.record, "native-current-export");
        let current_bytes = fs::read(&project_path).unwrap();
        let verified = f.store.v2_verify(&verify).unwrap();
        assert_eq!(verified["status"], "matched");
        assert_eq!(verified["selected_entries"], expected_entries);
        assert_eq!(
            verified["native_freshness"]["proof_scope"],
            "selected_message_provenance_only"
        );
        assert_eq!(verified["engineering_adoption"], "not_granted");
        assert_eq!(fs::read(&project_path).unwrap(), current_bytes);
        let rerun = f.store.v2_verify(&verify).unwrap();
        assert_eq!(rerun["status"], "matched");
        assert_ne!(verified["run"], rerun["run"]);

        let intake_path = receiver_root.join(
            prepared["preparation"]["intakes"][0]["intake"]["path"]
                .as_str()
                .unwrap(),
        );
        let intake_bytes = fs::read(&intake_path).unwrap();
        fs::write(&intake_path, b"{}").unwrap();
        error(f.store.v2_verify(&verify), "receiver_intake_digest_changed");
        fs::write(&intake_path, intake_bytes).unwrap();

        let mut stripped = project.clone();
        let mut stripped_basis = basis.clone();
        stripped_basis["facts"][0]["value"]["experience"]["unknowns"] = json!([]);
        stripped["conditions"]["inspection"] = json!(stripped_basis.to_string());
        fs::write(&project_path, serde_json::to_vec(&stripped).unwrap()).unwrap();
        f.project = bind(&project_path);
        let context = f
            .store
            .session_prepare(&request(&f, "native-stripped"))
            .unwrap();
        let mut invalid = verify.clone();
        invalid.id = "native-stripped-check".into();
        invalid.export = export_request(&context, &f.record, "native-stripped-export");
        error(
            f.store.v2_verify(&invalid),
            "receiver_complete_experience_fact_mismatch",
        );

        // A source remains listed in memory but is no longer evidence for the current
        // native object. Real task-view must reject it and leave a failed run receipt.
        let mut missing_design = project.clone();
        missing_design["evidence"]
            .as_object_mut()
            .unwrap()
            .remove("design");
        fs::write(&project_path, serde_json::to_vec(&missing_design).unwrap()).unwrap();
        f.project = bind(&project_path);
        let context = f
            .store
            .session_prepare(&request(&f, "native-missing-design"))
            .unwrap();
        invalid.id = "native-missing-design-check".into();
        invalid.export = export_request(&context, &f.record, "native-missing-design-export");
        error(f.store.v2_verify(&invalid), "v2_verification_failed");
        let namespace = sha(f
            .store
            .directory
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .as_bytes());
        let failed_parent = receiver_root
            .join("var/engineering-memory")
            .join(&namespace[..16])
            .join(&invalid.id);
        let failed: Vec<_> = fs::read_dir(failed_parent)
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect();
        assert_eq!(failed.len(), 1);
        let failure: Value = read_json(&failed[0].join("result.json")).unwrap();
        assert_eq!(failure["execution"]["status"], "failed");
        assert_eq!(failure["execution"]["old_success_reused"], false);

        fs::write(&project_path, &current_bytes).unwrap();
        f.project = bind(&project_path);
        assert_eq!(f.store.v2_verify(&verify).unwrap()["status"], "matched");
        f.store.record(&corrected(&f.record)).unwrap();
        error(f.store.v2_verify(&verify), "session_experience_advanced");
    }
}

#[test]
fn export_preserves_original_bytes_and_rejects_drift() {
    let f = fixture();
    let context = f
        .store
        .session_prepare(&request(&f, "export-source"))
        .unwrap();
    let input = export_request(&context, &f.record, "for-v2");
    let result = f.store.session_export(&input).unwrap();
    let mapping = &result["manifest"]["archives"][0];
    let derived = Path::new(mapping["archive_path"].as_str().unwrap());
    let raw = fs::read(&f.record.sources[0].source_path).unwrap();
    assert_eq!(fs::read(derived).unwrap(), raw);
    assert_eq!(
        mapping["original"],
        serde_json::to_value(&f.record.sources[0]).unwrap()
    );
    assert!(mapping["original_session_header"].is_null());
    let handoff: Value = read_json(Path::new(
        result["manifest"]["handoff_path"].as_str().unwrap(),
    ))
    .unwrap();
    assert_eq!(
        handoff["candidate"]["record"],
        serde_json::to_value(&f.record).unwrap()
    );
    assert_eq!(result["manifest"]["source_count"], 1);
    assert_eq!(f.store.session_export(&input).unwrap(), result);
    f.store
        .session_feedback(&feedback(&f, &context, "later-feedback"))
        .unwrap();
    assert_eq!(f.store.session_export_verify(&input.id).unwrap(), result);
    assert_eq!(f.store.session_export(&input).unwrap(), result);

    let mut changed_request = input.clone();
    changed_request.max_bytes += 1;
    error(
        f.store.session_export(&changed_request),
        "export_request_conflict",
    );
    fs::write(derived, b"changed derived record\n").unwrap();
    error(
        f.store.session_export_verify(&input.id),
        "export_file_changed",
    );
    error(f.store.session_export(&input), "export_file_changed");
    fs::write(derived, &raw).unwrap();
    assert_eq!(f.store.session_export_verify(&input.id).unwrap(), result);
    fs::write(&f.record.sources[0].source_path, b"changed original\n").unwrap();
    assert!(f.store.session_export_verify(&input.id).is_err());
    fs::write(&f.record.sources[0].source_path, &raw).unwrap();
    let path = Path::new(&f.project.path);
    let original_project = fs::read(path).unwrap();
    fs::write(path, [original_project.as_slice(), b"\n"].concat()).unwrap();
    assert!(f.store.session_export_verify(&input.id).is_err());
    let mut next = request(&f, "next-project-context");
    next.project = Some(bind(path));
    let next = f.store.session_prepare(&next).unwrap();
    let next_export = f
        .store
        .session_export(&export_request(&next, &f.record, "next-project-export"))
        .unwrap();
    assert_eq!(
        next_export["manifest"]["archives"],
        result["manifest"]["archives"]
    );
    fs::write(path, original_project).unwrap();
    f.store.record(&corrected(&f.record)).unwrap();
    assert!(f.store.session_export_verify(&input.id).is_err());
}

#[test]
fn export_rejects_partial_budget_and_recovers_interrupted_publication() {
    let f = fixture();
    let context = f
        .store
        .session_prepare(&request(&f, "export-budget"))
        .unwrap();
    let mut input = export_request(&context, &f.record, "budget");
    input.max_bytes = 1;
    error(f.store.session_export(&input), "export_total_budget");
    assert!(
        !f.store
            .directory
            .join("session-exports/budget/intent.json")
            .exists()
    );
    input.max_bytes = 65536;
    let done = f.store.session_export(&input).unwrap();
    let folder = f.store.directory.join("session-exports/budget");
    // Simulate a crash after durable intent and one archive, before the handoff
    // and manifest appeared. Retain the original artifacts under separate names.
    for name in ["manifest.json", "handoff.json"] {
        fs::rename(folder.join(name), folder.join(format!("retained-{name}"))).unwrap();
    }
    assert!(f.store.session_export_verify(&input.id).is_err());
    assert_eq!(f.store.session_export(&input).unwrap(), done);
    assert_eq!(
        fs::read(folder.join("manifest.json")).unwrap(),
        fs::read(folder.join("retained-manifest.json")).unwrap()
    );
}

#[test]
fn export_from_large_codex_source_keeps_real_header_and_original_offsets() {
    use std::io::{Seek, SeekFrom};
    let mut f = fixture();
    let path = f.root.join("large-codex.jsonl");
    let before_header = b"{\"type\":\"ignored_record\"}\n";
    let header = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"actual-session\",\"cwd\":\"/original/cwd\"}}\n";
    let first = b"{\"type\":\"response_item\",\"timestamp\":\"2026-10-11T00:00:00Z\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"earlier claim\"}]}}\n";
    let second = b"{\"type\":\"response_item\",\"timestamp\":\"2026-10-11T00:01:00Z\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"correction: unknown\"}]}}\n";
    let start = 700 * 1024 * 1024;
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(before_header).unwrap();
    file.write_all(header).unwrap();
    file.seek(SeekFrom::Start(start - 1)).unwrap();
    file.write_all(b"\n").unwrap();
    file.write_all(first).unwrap();
    file.write_all(second).unwrap();
    file.sync_all().unwrap();
    let window = read_window(&EvidenceArgs {
        source: path,
        provider: "codex".into(),
        profile: Some("codex-profile".into()),
        start_byte: start,
        end_byte: None,
        max_record_bytes: 2 * 1024 * 1024,
        json: true,
    })
    .unwrap();
    f.record.id = "large-case".into();
    f.record.sources = window.records;
    f.record.timeline[0].source_sha256 = vec![f.record.sources[0].sha256.clone()];
    f.record.timeline.push(TimelineEvent {
        id: "correction".into(),
        kind: "correction".into(),
        occurred_at: f.record.sources[1].event_at.clone(),
        description: "The claim is not established".into(),
        source_sha256: vec![f.record.sources[1].sha256.clone()],
        corrects: vec![f.record.timeline[0].id.clone()],
    });
    f.store.record(&f.record).unwrap();
    let context = f
        .store
        .session_prepare(&request(&f, "large-export"))
        .unwrap();
    let input = export_request(&context, &f.record, "large-for-v2");
    let exported = f.store.session_export(&input).unwrap();
    assert_eq!(exported["manifest"]["source_count"], 2);
    let mappings = exported["manifest"]["archives"].as_array().unwrap();
    for (index, raw) in [first.as_slice(), second.as_slice()]
        .into_iter()
        .enumerate()
    {
        let mapping = &mappings[index];
        assert_eq!(
            fs::read(mapping["archive_path"].as_str().unwrap()).unwrap(),
            [header.as_slice(), raw].concat()
        );
        assert_eq!(
            mapping["original_session_header"]["byte_start"],
            before_header.len()
        );
        assert_eq!(mapping["original_session_header"]["sha256"], sha(header));
        assert_eq!(mapping["derived"]["byte_start"], header.len());
        assert_eq!(
            mapping["original"]["byte_start"],
            start + if index == 0 { 0 } else { first.len() as u64 }
        );
        assert_eq!(mapping["derived"]["session_id"], "actual-session");
    }
    assert!(exported["manifest"]["archive_bytes"].as_u64().unwrap() < 1024);
    assert_eq!(f.store.session_export_verify(&input.id).unwrap(), exported);
}

#[test]
fn export_requires_complete_records_within_receiver_guard() {
    for (name, content, newline, expected) in [
        (
            "unterminated",
            "two text blocks".to_string(),
            false,
            "export_unfinished_source_record",
        ),
        (
            "too-large",
            "x".repeat(2 * 1024 * 1024),
            true,
            "export_receiver_record_budget",
        ),
    ] {
        let mut f = fixture();
        let path = f.root.join(format!("{name}.jsonl"));
        let mut raw = serde_json::to_vec(&json!({"type":"assistant", "sessionId":"two-blocks",
            "message":{"role":"assistant","content":[{"type":"text","text":"first block"},{"type":"text","text":"unknown"},
            {"type":"image","source":{"type":"base64","data":content}}]}})).unwrap();
        if newline {
            raw.push(b'\n');
        }
        fs::write(&path, raw).unwrap();
        f.record.id = name.into();
        f.record.sources = read_window(&EvidenceArgs {
            source: path,
            provider: "claude".into(),
            profile: None,
            start_byte: 0,
            end_byte: None,
            max_record_bytes: 16 * 1024 * 1024,
            json: true,
        })
        .unwrap()
        .records;
        f.record.timeline[0].source_sha256 = vec![f.record.sources[0].sha256.clone()];
        f.store.record(&f.record).unwrap();
        let mut req = request(&f, name);
        req.max_context_bytes = 131072;
        // Image bytes belong to the complete source record but are not copied
        // into visible text. The candidate fits; the receiver's guard still fails.
        let context = f.store.session_prepare(&req).unwrap();
        let input = export_request(&context, &f.record, name);
        let result = f.store.session_export(&input);
        error(result, expected);
        assert!(
            !f.store
                .directory
                .join("session-exports")
                .join(name)
                .exists()
        );
    }
}

fn feedback(f: &Fixture, context: &Value, id: &str) -> SessionFeedback {
    SessionFeedback {
        schema: "engineering-memory.session-feedback/1".into(),
        context_id: context["session"]["request"]["id"].as_str().unwrap().into(),
        context_sha256: context["context_sha256"].as_str().unwrap().into(),
        feedback: UsageFeedback {
            schema: "engineering-memory.feedback/1".into(),
            id: id.into(),
            record_id: f.record.id.clone(),
            revision: 1,
            project_id: Some(f.project.project_id.clone()),
            used: false,
            outcome: "not_used".into(),
            result: "只验证软件往返，未实际用于工程".into(),
            sources: vec![],
            elapsed_ms: None,
            unknowns: vec!["未做实际效果对照".into()],
        },
        result_project: None,
    }
}

fn corrected(old: &ExperienceRecord) -> ExperienceRecord {
    let mut next = old.clone();
    next.revision += 1;
    next.supersedes = Some(old.revision);
    next.timeline.push(TimelineEvent {
        id: "correction".into(),
        kind: "correction".into(),
        occurred_at: None,
        description: "原判断过宽，只能支持离散采样".into(),
        source_sha256: vec![old.sources[0].sha256.clone()],
        corrects: vec!["claim".into()],
    });
    next
}

fn error(result: anyhow::Result<impl std::fmt::Debug>, expected: &str) {
    let actual = format!("{:#}", result.unwrap_err());
    assert!(
        actual.contains(expected),
        "expected {expected}, got {actual}"
    );
}

fn handoff(f: &Fixture, context: &Value) -> anyhow::Result<Value> {
    f.store.session_handoff(
        context["session"]["request"]["id"].as_str().unwrap(),
        context["context_sha256"].as_str().unwrap(),
        &f.record.id,
        1,
    )
}

#[test]
fn independent_tasks_share_memory_without_mixing_projects_or_mutating_owners() {
    let f = fixture();
    let original_project = fs::read(&f.project.path).unwrap();
    let mut another = f.record.clone();
    another.id = "lesson-two".into();
    another.project_id = Some("project-two".into());
    another.sources[0].profile = Some("profile-b".into());
    f.store.record(&another).unwrap();
    let a = request(&f, "task-one");
    let first = f.store.session_prepare(&a).unwrap();
    assert_eq!(first, f.store.session_prepare(&a).unwrap());
    let mut b = a.clone();
    b.id = "task-two".into();
    b.provider = "claude".into();
    b.project = Some(project(&f.root.join("second.json"), "project-two"));
    let second = Store::new(f.store.directory.clone())
        .session_prepare(&b)
        .unwrap();
    assert_eq!(
        first["session"]["memory_context"]["candidates"][0]["id"],
        "lesson-one"
    );
    assert_eq!(
        second["session"]["memory_context"]["candidates"][0]["id"],
        "lesson-two"
    );
    assert_eq!(
        second["session"]["memory_context"]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    b.id = a.id.clone();
    error(
        f.store.session_prepare(&b),
        "session_request_identity_conflict",
    );
    let exchange = handoff(&f, &first).unwrap();
    assert_eq!(exchange["project_write"], false);
    assert_eq!(exchange["engineering_adoption"], "not_granted");
    assert_eq!(fs::read(&f.project.path).unwrap(), original_project);
    assert_eq!(f.store.get(&f.record.id, Some(1)).unwrap(), f.record);
}

#[test]
fn project_drift_and_experience_correction_invalidate_handoff() {
    let f = fixture();
    let first = f.store.session_prepare(&request(&f, "before")).unwrap();
    handoff(&f, &first).unwrap();
    let mut value: Value = read_json(Path::new(&f.project.path)).unwrap();
    value["conditions"]["inspection"] = json!("条件已改变，但 revision 字符串未变");
    fs::write(&f.project.path, serde_json::to_vec(&value).unwrap()).unwrap();
    error(handoff(&f, &first), "session_project_content_changed");
    let mut second = request(&f, "after-project-change");
    second.project = Some(bind(Path::new(&f.project.path)));
    let next = f.store.session_prepare(&second).unwrap();
    f.store.record(&corrected(&f.record)).unwrap();
    error(handoff(&f, &next), "session_experience_advanced");
    error(
        f.store.session_prepare(&second),
        "session_experience_advanced",
    );
    assert_eq!(
        f.store.session_get("before").unwrap()["session"]["memory_context"]["candidates"][0]["revision"],
        1
    );
    let mut fresh = second;
    fresh.id = "after-correction".into();
    let fresh = f.store.session_prepare(&fresh).unwrap();
    let candidate = &fresh["session"]["memory_context"]["candidates"][0];
    assert_eq!(candidate["revision"], 2);
    assert_eq!(candidate["timeline"].as_array().unwrap().len(), 2);
    assert_eq!(candidate["timeline"][1]["corrects"], json!(["claim"]));
    let mut bad_binding = bind(Path::new(&f.project.path));
    bad_binding.object.as_mut().unwrap().object_revision = "invented".into();
    error(
        verify_project_binding(&bad_binding),
        "session_object_revision_mismatch",
    );
    bad_binding = bind(Path::new(&f.project.path));
    bad_binding.project_id = "other".into();
    error(
        verify_project_binding(&bad_binding),
        "session_project_identity_mismatch",
    );
}

#[test]
fn feedback_requires_the_provided_revision_context_and_real_use_status() {
    let f = fixture();
    let context = f.store.session_prepare(&request(&f, "provided")).unwrap();
    let good = feedback(&f, &context, "use-one");
    let mut bad = good.clone();
    bad.context_sha256 = "f".repeat(64);
    error(
        f.store.session_feedback(&bad),
        "session_context_digest_mismatch",
    );
    bad = good.clone();
    bad.feedback.record_id = "not-presented".into();
    error(
        f.store.session_feedback(&bad),
        "experience_not_provided_to_session",
    );
    bad = good.clone();
    bad.feedback.project_id = Some("foreign-project".into());
    error(
        f.store.session_feedback(&bad),
        "session_feedback_project_mismatch",
    );
    bad = good.clone();
    bad.feedback.outcome = "helped".into();
    error(
        f.store.session_feedback(&bad),
        "feedback_use_outcome_conflict",
    );
    assert!(
        !f.store
            .directory
            .join("session-feedback-intents/use-one.json")
            .exists()
    );
    let receipt = f.store.session_feedback(&good).unwrap();
    assert_eq!(receipt["result_project_observation"], "not_provided");
    assert_eq!(receipt["used"], false);
    assert_eq!(receipt["rrsi_validation"], "not_run");
    assert_eq!(f.store.session_feedback(&good).unwrap(), receipt);
}

#[test]
fn correction_preserves_old_use_and_interrupted_feedback_replays_exact_intent() {
    let f = fixture();
    let context = f
        .store
        .session_prepare(&request(&f, "used-context"))
        .unwrap();
    f.store.record(&corrected(&f.record)).unwrap();
    let mut input = feedback(&f, &context, "historical-use");
    input.feedback.used = true;
    input.feedback.outcome = "no_delta".into();
    error(
        f.store.session_feedback(&input),
        "session_feedback_result_project_required",
    );
    input.result_project = Some(f.project.clone());
    let obstructed = f.store.directory.join("session-feedback");
    fs::write(&obstructed, b"simulated interrupted receipt destination").unwrap();
    assert!(f.store.session_feedback(&input).is_err());
    assert!(
        f.store
            .directory
            .join("session-feedback-intents/historical-use.json")
            .is_file()
    );
    assert!(
        f.store
            .directory
            .join("feedback/historical-use.json")
            .is_file()
    );
    fs::rename(
        &obstructed,
        f.store.directory.join("retained-interruption-fixture"),
    )
    .unwrap();
    // The result has since advanced. Retry must finish the frozen historical observation,
    // not pretend that its earlier Project observation describes the current Project.
    let mut value: Value = read_json(Path::new(&f.project.path)).unwrap();
    value["revision"] = json!("r2");
    fs::write(&f.project.path, serde_json::to_vec(&value).unwrap()).unwrap();
    let receipt = f.store.session_feedback(&input).unwrap();
    assert_eq!(receipt["revision"], 1);
    assert_eq!(
        receipt["result_project_observation"],
        "verified_before_durable_intent"
    );
    let mut changed = input.clone();
    changed.feedback.result = "another result".into();
    error(
        f.store.session_feedback(&changed),
        "session_feedback_intent_conflict",
    );
    assert_eq!(f.store.get(&f.record.id, None).unwrap().revision, 2);
}

#[test]
fn unreadable_history_is_excluded_without_hiding_valid_context() {
    let f = fixture();
    fs::write(
        f.root.join("memory").join("records").join("broken.json"),
        b"{truncated",
    )
    .unwrap();
    let prepared = f
        .store
        .session_prepare(&request(&f, "with-broken"))
        .unwrap();
    assert_eq!(
        prepared["session"]["memory_context"]["candidates"][0]["id"],
        f.record.id
    );
    let excluded = prepared["session"]["excluded"].as_array().unwrap();
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0]["id"], "broken");
    assert_eq!(excluded[0]["reason"], "record_unreadable");
    assert!(excluded[0]["revision"].is_null());
    f.store.session_verify("with-broken").unwrap();
}

#[test]
fn bounded_context_preserves_whole_corrections_and_reports_source_drift() {
    let f = fixture();
    let mut large = corrected(&f.record);
    large.timeline[1].description = "完整纠正不能被截掉。".repeat(800);
    f.store.record(&large).unwrap();
    let mut small = request(&f, "small");
    small.max_context_bytes = 2048;
    let prepared = f.store.session_prepare(&small).unwrap();
    assert!(
        prepared["session"]["memory_context"]["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        prepared["session"]["excluded"][0]["reason"],
        "context_budget_whole_candidate_omitted"
    );
    let mut enough = small;
    enough.id = "enough".into();
    enough.max_context_bytes = 65536;
    let prepared = f.store.session_prepare(&enough).unwrap();
    assert_eq!(
        prepared["session"]["memory_context"]["candidates"][0]["timeline"][1]["description"],
        large.timeline[1].description
    );
    fs::write(&f.record.sources[0].source_path, b"changed original\n").unwrap();
    assert!(f.store.session_verify("enough").is_err());
    enough.id = "drifted".into();
    let drifted = f.store.session_prepare(&enough).unwrap();
    assert!(
        drifted["session"]["memory_context"]["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        drifted["session"]["excluded"][0]["reason"],
        "source_reverification_failed"
    );
}

#[test]
fn no_project_remains_independent_and_cli_rpc_share_the_same_owner() {
    let f = fixture();
    let mut independent = request(&f, "no-v2");
    independent.project = None;
    independent.include_other_projects = true;
    let input_path = f.root.join("input.json");
    fs::write(&input_path, serde_json::to_vec(&independent).unwrap()).unwrap();
    let binary = env!("CARGO_BIN_EXE_engineering-memory");
    let run = Command::new(binary)
        .arg("--home")
        .arg(&f.store.directory)
        .args(["session", "prepare", "--input"])
        .arg(input_path)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    let context: Value = serde_json::from_slice(&run.stdout).unwrap();
    let context = &context["data"];
    assert!(context["session"]["request"]["project"].is_null());
    error(handoff(&f, context), "v2_handoff_requires_actual_project");
    let mut rpc = Command::new(binary)
        .arg("--home")
        .arg(&f.store.directory)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        rpc.stdin.take().unwrap(),
        "{}",
        json!({"op":"session_verify","id":"no-v2"})
    )
    .unwrap();
    let response = rpc.wait_with_output().unwrap();
    assert!(response.status.success());
    let response: Value = serde_json::from_slice(&response.stdout).unwrap();
    assert_eq!(response["data"]["source_and_project_binding"], "matched");
    let mut result = feedback(&f, context, "independent-feedback");
    result.feedback.project_id = None;
    f.store.session_feedback(&result).unwrap();
    result.feedback.id = "invented-project".into();
    result.result_project = Some(f.project.clone());
    error(
        f.store.session_feedback(&result),
        "session_feedback_unbound_project",
    );
}

#[test]
fn session_skips_whole_oversized_candidates_and_fills_from_later_records() {
    let f = fixture();
    for (id, large) in [
        ("aa-large", true),
        ("ab-large", true),
        ("yy-small", false),
        ("zz-small", false),
    ] {
        let mut record = f.record.clone();
        record.id = id.into();
        record.purpose = "contextfill".into();
        if large {
            record
                .unknowns
                .push("完整保留的未知，不能为预算而删除。".repeat(600));
        }
        f.store.record(&record).unwrap();
    }
    let mut req = request(&f, "bounded-context-fill");
    req.query = "contextfill".into();
    req.max_candidates = 2;
    let context = f.store.session_prepare(&req).unwrap();
    let list = context["session"]["memory_context"]["candidates"]
        .as_array()
        .unwrap();
    assert_eq!(
        list.iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["yy-small", "zz-small"]
    );
    let excluded = context["session"]["excluded"].as_array().unwrap();
    assert_eq!(excluded.len(), 2);
    assert!(
        excluded
            .iter()
            .all(|v| v["reason"] == "context_budget_whole_candidate_omitted")
    );
    assert!(
        serde_json::to_vec(&context["session"]["memory_context"])
            .unwrap()
            .len()
            <= req.max_context_bytes as usize
    );
    assert_eq!(f.store.get("aa-large", None).unwrap().unknowns.len(), 2);
    f.store.session_verify(&req.id).unwrap();
}

#[test]
fn continuation_hook_keeps_active_task_but_new_requests_take_precedence() {
    let f = fixture();
    let skill = f.root.join("continuation-skill.md");
    fs::write(&skill, "Synthetic hook fixture").unwrap();
    let policy = HookPolicy {
        schema: "engineering-memory.hook-policy/1".into(),
        source_roots: vec![f.root.to_string_lossy().into_owned()],
        skill_path: skill.to_string_lossy().into_owned(),
        max_candidates: 3,
        max_context_bytes: 8192,
        include_other_projects: true,
    };
    let mut binding = request(&f, "continuation-binding");
    binding.query = "检验结论的边界".into();
    let context = f.store.session_prepare(&binding).unwrap();
    f.store
        .session_activate(&binding.id, context["context_sha256"].as_str().unwrap())
        .unwrap();
    for (prompt, continuation) in [
        ("好的，继续", true),
        ("继续吧。", true),
        ("OK, continue.", true),
        ("好的，继续优化", true),
        ("继续优化", true),
        ("好，继续优化", true),
        ("可以继续优化", true),
        ("继续优化吧", true),
        ("好的，可以继续优化", true),
        ("继续分析共享 profile 的记忆隔离", false),
        ("不要继续", false),
        ("不要继续优化", false),
        ("好的，继续优化另一项目的数据库", false),
        ("继续优化，但预算改为 10 秒", false),
        ("停止优化", false),
        ("不要，继续优化", false),
    ] {
        let event = json!({"hook_event_name":"UserPromptSubmit", "session_id":binding.session_id,
            "prompt":prompt, "cwd":"/do-not-infer-project"});
        let result = f.store.client_hook(&policy, "codex", None, &event).unwrap();
        let text = result["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        let provided: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(
            provided["request"]["task"],
            if continuation { &binding.task } else { prompt }
        );
        assert_eq!(
            provided["request"]["query"],
            if continuation { &binding.query } else { prompt }
        );
        assert_eq!(
            provided["request"]["project"]["sha256"],
            binding.project.as_ref().unwrap().sha256
        );
        assert_eq!(provided["request"]["conditions"], json!(binding.conditions));
        assert_eq!(provided["request"]["max_candidates"], policy.max_candidates);
        assert_eq!(
            provided["request"]["max_context_bytes"],
            policy.max_context_bytes
        );
        assert_eq!(
            provided["request"]["include_other_projects"],
            policy.include_other_projects
        );
    }
    let event = json!({"hook_event_name":"UserPromptSubmit", "session_id":"unbound-session",
        "prompt":"好的，继续优化", "cwd":f.root});
    let result = f.store.client_hook(&policy, "codex", None, &event).unwrap();
    let provided: Value = serde_json::from_str(
        result["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(provided["request"]["task"], "好的，继续优化");
    assert_eq!(provided["request"]["query"], "好的，继续优化");
    assert!(provided["request"]["project"].is_null());
}

#[test]
fn hook_context_and_intake_keep_project_and_feedback_separate() {
    let f = fixture();
    let skill = f.root.join("SKILL.md");
    fs::write(&skill, b"Synthetic skill instruction fixture").unwrap();
    let policy = HookPolicy {
        schema: "engineering-memory.hook-policy/1".into(),
        source_roots: vec![f.root.to_string_lossy().into_owned()],
        skill_path: skill.to_string_lossy().into_owned(),
        max_candidates: 3,
        max_context_bytes: 8192,
        include_other_projects: true,
    };
    let mut event = json!({"hook_event_name":"SessionStart","session_id":"source-session",
        "source":"resume","cwd":"/this-is-not-a-project-binding","transcript_path":f.record.sources[0].source_path});
    let initial = f
        .store
        .client_hook(&policy, "claude", Some("profile-a"), &event)
        .unwrap();
    assert_eq!(initial["continue"], true);
    let additional = initial["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    let provided: Value = serde_json::from_str(additional.lines().last().unwrap()).unwrap();
    assert!(provided["request"]["project"].is_null());
    assert_eq!(
        provided["memory_context"]["candidates"][0]["id"],
        "lesson-one"
    );
    let mut binding = request(&f, "explicit-hook-project");
    binding.provider = "claude".into();
    binding.session_id = "source-session".into();
    let context = f.store.session_prepare(&binding).unwrap();
    f.store
        .session_activate(&binding.id, context["context_sha256"].as_str().unwrap())
        .unwrap();
    let bound = f
        .store
        .client_hook(&policy, "claude", Some("profile-a"), &event)
        .unwrap();
    let provided: Value = serde_json::from_str(
        bound["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(provided["request"]["project"]["project_id"], "project-one");
    let intake = f.store.ingest_hook_sources(8, 4 * 1024 * 1024).unwrap();
    assert_eq!(intake["registered"].as_array().unwrap().len(), 1);
    assert_eq!(intake["curated"], false);
    assert_eq!(
        f.store.ingest_hook_sources(8, 4 * 1024 * 1024).unwrap()["attempts"],
        0
    );
    event["hook_event_name"] = json!("Stop");
    event["last_assistant_message"] = json!("自称本次大幅改进，不能因此成为帮助反馈");
    assert_eq!(
        f.store
            .client_hook(&policy, "claude", Some("profile-a"), &event)
            .unwrap(),
        json!({"continue":true})
    );
    assert!(!f.store.directory.join("feedback").exists());
    // A different provider with the same session string must not inherit the Project.
    event["hook_event_name"] = json!("SessionStart");
    event["transcript_path"] = Value::Null;
    let separate = f.store.client_hook(&policy, "codex", None, &event).unwrap();
    let provided: Value = serde_json::from_str(
        separate["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert!(provided["request"]["project"].is_null());
    let outside = TempDir::new().unwrap();
    let foreign = outside.path().join("foreign.jsonl");
    fs::copy(&f.record.sources[0].source_path, &foreign).unwrap();
    event["transcript_path"] = json!(foreign);
    error(
        f.store.queue_hook_source(&policy, "claude", None, &event),
        "hook_source_outside_configured_roots",
    );
    event["transcript_path"] = json!(f.record.sources[0].source_path);
    event["session_id"] = json!("forged-session");
    f.store
        .queue_hook_source(&policy, "claude", Some("other-profile"), &event)
        .unwrap();
    assert_eq!(
        f.store.ingest_hook_sources(8, 4 * 1024 * 1024).unwrap()["waiting"][0]["reason"],
        "hook_source_session_mismatch"
    );
}

#[test]
fn credential_prompts_do_not_enter_hook_context_or_saved_tasks() {
    let f = fixture();
    let skill = f.root.join("SKILL.md");
    fs::write(&skill, b"Synthetic skill fixture").unwrap();
    let policy = HookPolicy {
        schema: "engineering-memory.hook-policy/1".into(),
        source_roots: vec![f.root.to_string_lossy().into_owned()],
        skill_path: skill.to_string_lossy().into_owned(),
        max_candidates: 3,
        max_context_bytes: 8192,
        include_other_projects: false,
    };
    let binding = request(&f, "safe-active-task");
    let context = f.store.session_prepare(&binding).unwrap();
    f.store
        .session_activate(&binding.id, context["context_sha256"].as_str().unwrap())
        .unwrap();
    let token = format!("sk-proj-{}", "SYNTHETIC_ONLY_".repeat(4));
    let event = json!({"hook_event_name":"UserPromptSubmit", "session_id":binding.session_id,
        "prompt":format!("请试这把 {token}")});
    let response = f.store.client_hook(&policy, "codex", None, &event).unwrap();
    let additional = response["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(!additional.contains(&token));
    let provided: Value = serde_json::from_str(additional.lines().last().unwrap()).unwrap();
    assert_eq!(provided["request"]["task"], binding.task);
    assert_eq!(provided["request"]["query"], binding.query);
    assert_eq!(provided["request"]["project"]["project_id"], "project-one");
    for file in fs::read_dir(f.store.directory.join("sessions")).unwrap() {
        assert!(
            !fs::read_to_string(file.unwrap().path())
                .unwrap()
                .contains(&token)
        );
    }
    let mut direct = binding;
    direct.id = "unsafe-direct-task".into();
    direct.query = token;
    error(
        f.store.session_prepare(&direct),
        "session_request_contains_credential",
    );
    assert!(
        !f.store
            .directory
            .join("sessions/unsafe-direct-task.json")
            .exists()
    );
}

#[test]
fn hook_cli_returns_native_json_and_failures_do_not_stop_the_user() {
    let f = fixture();
    let skill = f.root.join("SKILL.md");
    fs::write(&skill, b"Synthetic skill fixture").unwrap();
    let policy = f.root.join("policy.json");
    fs::write(&policy,serde_json::to_vec(&json!({"schema":"engineering-memory.hook-policy/1",
        "source_roots":[f.root],"skill_path":skill,"max_candidates":3,"max_context_bytes":8192,"include_other_projects":true})).unwrap()).unwrap();
    let run = |event: Value| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_engineering-memory"))
            .arg("--home")
            .arg(&f.store.directory)
            .args(["hook", "--provider", "codex", "--policy"])
            .arg(&policy)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{event}").unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success());
        serde_json::from_slice::<Value>(&result.stdout).unwrap()
    };
    let response = run(
        json!({"hook_event_name":"UserPromptSubmit","session_id":"s1","prompt":"检验",
        "transcript_path":null,"cwd":"/unknown","turn_id":"t1","permission_mode":"default","model":"wire-fixture"}),
    );
    assert!(response.get("ok").is_none());
    assert_eq!(
        response["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    let invalid = run(json!({"hook_event_name":"unknown-event","session_id":"s1"}));
    assert_eq!(invalid["continue"], true);
    assert!(
        invalid["systemMessage"]
            .as_str()
            .unwrap()
            .contains("hook_event_unsupported")
    );
}

#[test]
fn many_budget_omissions_are_counted_after_the_first_few_are_named() {
    let f = fixture();
    for n in 0..12 {
        let mut large = f.record.clone();
        large.id = format!("large-{n:02}");
        large.timeline[0].description = "完整内容不能被截掉。".repeat(400);
        f.store.record(&large).unwrap();
    }
    let mut small = request(&f, "crowded");
    small.max_context_bytes = 2048;
    let prepared = f.store.session_prepare(&small).unwrap();
    let excluded = prepared["session"]["excluded"].as_array().unwrap();
    let named = excluded
        .iter()
        .filter(|e| e["reason"] == "context_budget_whole_candidate_omitted")
        .count();
    assert_eq!(
        named,
        engineering_memory_runtime::session::MAX_LISTED_OMISSIONS
    );
    let rest = excluded.last().unwrap();
    assert_eq!(rest["reason"], "context_budget_more_candidates_omitted");
    assert!(rest["id"].is_null());
    let candidates = prepared["session"]["memory_context"]["candidates"]
        .as_array()
        .unwrap()
        .len() as u64;
    // Every recalled record either fits or is omitted: named or counted.
    assert_eq!(
        named as u64 + rest["count"].as_u64().unwrap() + candidates,
        13
    );
    f.store.session_verify("crowded").unwrap();
}
