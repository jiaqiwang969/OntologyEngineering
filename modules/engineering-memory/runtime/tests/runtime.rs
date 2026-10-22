use engineering_memory_runtime::contracts::*;
use engineering_memory_runtime::evidence::{EvidenceArgs, read_window};
use engineering_memory_runtime::store::{Store, atomic_json, read_json, validate_experience};
use engineering_memory_runtime::worker::Extractor;
use engineering_memory_runtime::workflow::{IgnoredSource, Resolution};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn line(message: &str, text: &str) -> String {
    format!(
        "{}\n",
        json!({"type":"assistant","sessionId":"session-one","uuid":message,
        "timestamp":"2026-10-11T00:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":text}]}})
    )
}
fn source(path: &Path) -> SourceAnchor {
    read_window(&EvidenceArgs {
        source: path.into(),
        provider: "claude".into(),
        profile: Some("profile-a".into()),
        start_byte: 0,
        end_byte: None,
        max_record_bytes: 2 * 1024 * 1024,
        json: true,
    })
    .unwrap()
    .records[0]
        .clone()
}
fn lesson(anchor: SourceAnchor, id: &str) -> ExperienceRecord {
    ExperienceRecord {
        schema: "engineering-memory.experience/2".into(),
        id: id.into(),
        revision: 1,
        supersedes: None,
        project_id: Some("project-one".into()),
        conditions: vec!["discrete-sampling".into()],
        purpose: "检查碰撞结论的边界".into(),
        status: "reviewed".into(),
        recorded_at: "2026-10-11T00:00:00Z".into(),
        timeline: vec![TimelineEvent {
            id: "old-claim".into(),
            kind: "assistant_claim".into(),
            occurred_at: anchor.event_at.clone(),
            description: "当时声称连续成立".into(),
            source_sha256: vec![anchor.sha256.clone()],
            corrects: vec![],
        }],
        sources: vec![anchor],
        causal_claims: vec![],
        method: Some(Method {
            purpose: "限制结论".into(),
            mechanism: "按检验证据范围".into(),
            validation: "独立反例".into(),
        }),
        unknowns: vec!["没有连续路径检验".into()],
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
        description: "只能支持离散采样通过".into(),
        source_sha256: vec![old.sources[0].sha256.clone()],
        corrects: vec!["old-claim".into()],
    });
    next
}
fn fixture() -> (TempDir, Store, ExperienceRecord) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("original.jsonl");
    fs::write(&path, line("m1", "旧判断；后来明确仅采样通过")).unwrap();
    let record = lesson(source(&path), "lesson-one");
    let store = Store::new(temp.path().join("memory"));
    (temp, store, record)
}
fn registration(path: &Path, id: &str) -> SourceRegistration {
    SourceRegistration {
        id: id.into(),
        path: path.to_str().unwrap().into(),
        provider: "claude".into(),
        profile: Some(id.into()),
        project_id: None,
        conditions: vec![],
        start_byte: 0,
    }
}
fn budget() -> SyncBudget {
    SyncBudget {
        max_bytes: 2 * 1024 * 1024,
        max_verify_bytes: 16 * 1024 * 1024,
        max_windows: 8,
        max_seconds: 10,
    }
}
fn packet_ids(store: &Store) -> Vec<String> {
    store.pending(100).unwrap()["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().into())
        .collect()
}

#[cfg(unix)]
#[test]
fn source_alias_keeps_one_cursor_and_preserves_history() {
    let (temp, store, _) = fixture();
    let path = temp.path().join("original.jsonl");
    let first = registration(&path, "manual");
    store.register(first.clone(), 100_000).unwrap();
    let owner_file = store.directory.join("sources/manual.json");
    let owner_initial = fs::read(&owner_file).unwrap();
    let other = registration(&path, "profile-b");
    let linked = store.register(other.clone(), 100_000).unwrap();
    assert_eq!(linked["alias"]["source_id"], "manual");
    assert_eq!(linked["alias"]["registration"]["profile"], "profile-b");
    assert_eq!(fs::read(&owner_file).unwrap(), owner_initial);
    assert!(!store.directory.join("sources/profile-b.json").exists());
    #[cfg(target_os = "macos")]
    {
        let archive = temp.path().join("alias-conflict-archive");
        error(
            store.capture_source(&CaptureRequest {
                schema: "engineering-memory.capture-request/1".into(),
                registration: other.clone(),
                archive_root: archive.to_str().unwrap().into(),
                max_logical_bytes: 100_000,
            }),
            "source_registration_conflict",
        );
        assert!(!archive.exists());
    }
    assert_eq!(store.register(other, 100_000).unwrap()["effect"], false);
    let scan = store.sync(&budget()).unwrap();
    assert_eq!(scan["windows"].as_array().unwrap().len(), 1);
    assert_eq!(packet_ids(&store).len(), 1);
    let packet = packet_ids(&store)[0].clone();
    store.resolve(&no_delta(&store, &packet)).unwrap();
    let packet_file = store
        .directory
        .join("packets")
        .join(format!("{packet}.json"));
    let packet_before = fs::read(&packet_file).unwrap();

    // Reproduce an actual pre-0.10 pristine duplicate without rewriting the owner.
    let mut legacy: Value = serde_json::from_slice(&owner_initial).unwrap();
    legacy["registration"]["id"] = json!("old-hook");
    legacy["registration"]["profile"] = json!("old-profile");
    let legacy_file = store.directory.join("sources/old-hook.json");
    atomic_json(&legacy_file, &legacy).unwrap();
    let legacy_bytes = fs::read(&legacy_file).unwrap();
    let request = SourceAliasRequest {
        schema: "engineering-memory.source-alias-request/1".into(),
        alias_id: "old-hook".into(),
        source_id: "manual".into(),
        max_verify_bytes: 100_000,
    };
    assert_eq!(store.alias_source(&request).unwrap()["effect"], true);
    assert_eq!(store.alias_source(&request).unwrap()["effect"], false);
    assert_eq!(fs::read(&legacy_file).unwrap(), legacy_bytes);
    assert_eq!(fs::read(&packet_file).unwrap(), packet_before);
    assert_eq!(store.status().unwrap()["active_scan_sources"], 1);
    let mut append = fs::OpenOptions::new().append(true).open(&path).unwrap();
    append
        .write_all(line("m2", "新纠正必须在已有游标后继续读取").as_bytes())
        .unwrap();
    let scan = store.sync(&budget()).unwrap();
    assert!(scan["problems"].as_array().unwrap().is_empty());
    assert_eq!(scan["windows"].as_array().unwrap().len(), 1);
    assert_eq!(scan["windows"][0]["registration_id"], "manual");
    let pending = packet_ids(&store);
    assert_eq!(pending.len(), 1);
    assert_eq!(
        store.packet(&pending[0]).unwrap().sources[0]
            .message_id
            .as_deref(),
        Some("m2")
    );
    assert_eq!(fs::read(&legacy_file).unwrap(), legacy_bytes);
    assert_eq!(fs::read(&packet_file).unwrap(), packet_before);
    let mut chain = request.clone();
    chain.source_id = "profile-b".into();
    error(store.alias_source(&chain), "source_alias_chain");
    let alias_path = store.directory.join("source-aliases/old-hook.json");
    let saved: Value = read_json(&alias_path).unwrap();
    let mut tampered = saved.clone();
    tampered["source_registration"]["profile"] = json!("forged-owner");
    atomic_json(&alias_path, &tampered).unwrap();
    error(store.sync(&budget()), "source_alias_owner_changed");
    atomic_json(&alias_path, &saved).unwrap();
    fs::write(&legacy_file, b"{}").unwrap();
    error(store.status(), "source_alias_retired_state_changed");
}

#[test]
fn source_alias_rejects_drift_different_scope_and_started_scan() {
    for case in ["budget", "prefix", "scope", "started", "issued"] {
        let (temp, store, _) = fixture();
        let path = temp.path().join("original.jsonl");
        store
            .register(registration(&path, "manual"), 100_000)
            .unwrap();
        let mut old: Value = read_json(&store.directory.join("sources/manual.json")).unwrap();
        old["registration"]["id"] = json!("old-hook");
        old["registration"]["profile"] = json!("other-profile");
        if case == "scope" {
            old["registration"]["project_id"] = json!("different-project");
        }
        if case == "started" {
            old["backfill_cursor"] = old["baseline_end"].clone();
        }
        if case == "prefix" {
            fs::write(&path, line("m1", "原件被改写")).unwrap();
        }
        let legacy_file = store.directory.join("sources/old-hook.json");
        atomic_json(&legacy_file, &old).unwrap();
        let bytes = fs::read(&legacy_file).unwrap();
        if case == "issued" {
            atomic_json(
                &store.directory.join("scans/interrupted.json"),
                &json!({"registration_id":"old-hook"}),
            )
            .unwrap();
        }
        let req = SourceAliasRequest {
            schema: "engineering-memory.source-alias-request/1".into(),
            alias_id: "old-hook".into(),
            source_id: "manual".into(),
            max_verify_bytes: if case == "budget" { 1 } else { 100_000 },
        };
        assert!(store.alias_source(&req).is_err(), "{case}");
        assert!(
            !store
                .directory
                .join("source-aliases/old-hook.json")
                .exists()
        );
        assert_eq!(fs::read(&legacy_file).unwrap(), bytes);
    }
    let (temp, store, _) = fixture();
    let path = temp.path().join("original.jsonl");
    store
        .register(registration(&path, "manual"), 100_000)
        .unwrap();
    let mut different = registration(&path, "different");
    different.conditions = vec!["different-condition".into()];
    let result = store.register(different, 100_000).unwrap();
    assert!(result.get("alias").is_none());
    assert_eq!(store.status().unwrap()["active_scan_sources"], 2);
}
fn no_delta(store: &Store, packet_id: &str) -> Resolution {
    let p = store.packet(packet_id).unwrap();
    Resolution {
        schema: "engineering-memory.resolution/1".into(),
        packet_id: p.id,
        input_sha256: p.input_sha256,
        action: "no_delta".into(),
        actor: "test-reviewer".into(),
        reason: "该合成输入只验证流程，无工程结论".into(),
        records: vec![],
        ignored: p
            .sources
            .iter()
            .map(|s| IgnoredSource {
                sha256: s.sha256.clone(),
                reason: "合成运行样本".into(),
            })
            .collect(),
    }
}
fn error(result: anyhow::Result<impl std::fmt::Debug>, expected: &str) {
    assert!(
        format!("{:#}", result.unwrap_err()).contains(expected),
        "expected {expected}"
    );
}

#[cfg(target_os = "macos")]
fn capture_request(temp: &TempDir, path: &Path, id: &str) -> CaptureRequest {
    CaptureRequest {
        schema: "engineering-memory.capture-request/1".into(),
        registration: registration(path, id),
        archive_root: temp.path().join("archives").to_str().unwrap().into(),
        max_logical_bytes: 1024 * 1024 * 1024,
    }
}

#[test]
#[cfg(target_os = "macos")]
fn fixed_capture_scans_large_history_and_preserves_origin() {
    use std::io::{Seek, SeekFrom};
    let temp = TempDir::new().unwrap();
    let original = temp.path().join("large.jsonl");
    let first = line("large-first", "捕获时的原始判断");
    let mut file = fs::File::create(&original).unwrap();
    file.write_all(first.as_bytes()).unwrap();
    // Sparse unexamined history exercises the file-size limit without allocating 600 MiB RAM.
    file.set_len(600 * 1024 * 1024).unwrap();
    file.seek(SeekFrom::End(-1)).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
    let store = Store::new(temp.path().join("memory"));
    let request = capture_request(&temp, &original, "large-capture");
    error(
        store.register(request.registration.clone(), 512 * 1024 * 1024),
        "registration_range_or_verification_budget",
    );
    let captured = store.capture_source(&request).unwrap();
    assert!(captured["whole_file_sha256"].is_null());
    assert_eq!(captured["verified_prefix_bytes"], 0);
    let snapshot = Path::new(captured["capture"]["snapshot_path"].as_str().unwrap());
    assert!(!snapshot.starts_with(&store.directory));
    let mut limits = budget();
    limits.max_verify_bytes = 1;
    limits.max_windows = 1;
    let scanned = store.sync(&limits).unwrap();
    assert_eq!(scanned["problems"], json!([]));
    assert_eq!(scanned["read_bytes"], first.len());
    assert_eq!(scanned["verified_prefix_bytes"], 0);
    assert_eq!(scanned["curated_by_scan"], 0);
    assert_eq!(scanned["whole_history_reviewed"], false);
    let packet = store.packet(&packet_ids(&store)[0]).unwrap();
    assert_eq!(packet.schema, "engineering-memory.packet/2");
    assert_eq!(
        packet.capture.as_ref().unwrap().original_path,
        original.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(packet.sources[0].text, "捕获时的原始判断");
    let record = lesson(packet.sources[0].clone(), "captured-lesson");
    store.record(&record).unwrap();
    // Later writes to the original do not rewrite the captured historical version.
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(line("large-first", "原件已修改").as_bytes())
        .unwrap();
    file.sync_all().unwrap();
    store.verify_record(&record.id, Some(1)).unwrap();
    let handoff = store
        .handoff(&record.id, 1, Some("project-one"), &record.conditions)
        .unwrap();
    assert_eq!(
        handoff["source_captures"][0]["capture"]["original_path"],
        original.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        handoff["source_captures"][0]["original_current_state"],
        "not_asserted"
    );
    assert_eq!(handoff["engineering_adoption"], "not_granted");
    assert!(handoff["source_captures"][0]["whole_file_sha256"].is_null());
    let recalled = store
        .recall("", Some("project-one"), &record.conditions, 5, false)
        .unwrap();
    assert_eq!(
        recalled["candidates"][0]["source_captures"],
        handoff["source_captures"]
    );
    let context = store
        .session_prepare(&SessionRequest {
            schema: "engineering-memory.session-request/1".into(),
            id: "captured-context".into(),
            provider: "codex".into(),
            session_id: "current-session".into(),
            task: "核历史判断".into(),
            phase: "decision".into(),
            project: None,
            conditions: record.conditions.clone(),
            query: "".into(),
            include_other_projects: true,
            max_candidates: 5,
            max_context_bytes: 32768,
        })
        .unwrap();
    assert_eq!(
        context["session"]["memory_context"]["candidates"][0]["source_captures"],
        handoff["source_captures"]
    );
    store.session_verify("captured-context").unwrap();
    let before = store.status().unwrap()["sources"].clone();
    fs::rename(&original, temp.path().join("original-moved.jsonl")).unwrap();
    assert_eq!(store.capture_source(&request).unwrap()["effect"], false);
    assert_eq!(store.status().unwrap()["sources"], before);
    store.session_verify("captured-context").unwrap();
    let mut conflict = request.clone();
    conflict.registration.conditions.push("new".into());
    error(store.capture_source(&conflict), "capture_request_conflict");
    assert!(
        store.status().unwrap()["sources"][0]["historical_bytes_remaining"]
            .as_u64()
            .unwrap()
            > 500 * 1024 * 1024
    );
}

#[test]
#[cfg(target_os = "macos")]
fn capture_versions_keep_forks_and_receipt_only_recovery() {
    let temp = TempDir::new().unwrap();
    let original = temp.path().join("changing.jsonl");
    fs::write(&original, line("m1", "最初的说法")).unwrap();
    let store = Store::new(temp.path().join("memory"));
    let request = capture_request(&temp, &original, "version-one");
    store.capture_source(&request).unwrap();
    store.sync(&budget()).unwrap();
    fs::write(&original, line("m1", "同身份的另一段内容")).unwrap();
    let mut next = request.clone();
    next.registration.id = "version-two".into();
    let captured = store.capture_source(&next).unwrap();
    let scan = store.sync(&budget()).unwrap();
    assert_eq!(scan["windows"][0]["forks"], 1);
    assert_eq!(packet_ids(&store).len(), 2);
    // Simulate durable capture commit followed by interruption before index/cursor publication.
    let recovered = Store::new(temp.path().join("recovery"));
    atomic_json(
        &recovered.directory.join("captures/version-two.json"),
        &captured["capture"],
    )
    .unwrap();
    fs::rename(&original, temp.path().join("origin-retained.jsonl")).unwrap();
    assert_eq!(recovered.capture_source(&next).unwrap()["effect"], true);
    assert_eq!(recovered.capture_source(&next).unwrap()["effect"], false);
    recovered.sync(&budget()).unwrap();
    let p = recovered.packet(&packet_ids(&recovered)[0]).unwrap();
    assert_eq!(p.sources[0].text, "同身份的另一段内容");
    recovered.resolve(&no_delta(&recovered, &p.id)).unwrap();
    assert_eq!(recovered.status().unwrap()["packet_states"]["no_delta"], 1);
}

#[test]
#[cfg(target_os = "macos")]
fn snapshot_mutation_replacement_truncation_and_restored_mtime_are_refused() {
    use std::os::unix::fs::PermissionsExt;
    for change in ["rewrite", "replacement", "truncation", "symlink"] {
        let temp = TempDir::new().unwrap();
        let original = temp.path().join("original.jsonl");
        let first = line("m1", "前面的消息保持原样");
        fs::write(&original, format!("{first}{}", line("m2", "后面的历史"))).unwrap();
        let store = Store::new(temp.path().join("memory"));
        let request = capture_request(&temp, &original, "fixed");
        let captured = store.capture_source(&request).unwrap();
        let snapshot = Path::new(captured["capture"]["snapshot_path"].as_str().unwrap());
        let record = lesson(source(snapshot), "fixed-lesson");
        store.record(&record).unwrap();
        let metadata = snapshot.metadata().unwrap();
        match change {
            "rewrite" => {
                use std::io::{Seek, SeekFrom};
                fs::set_permissions(snapshot, fs::Permissions::from_mode(0o600)).unwrap();
                let mut file = fs::OpenOptions::new().write(true).open(snapshot).unwrap();
                file.seek(SeekFrom::Start(first.len() as u64 + 1)).unwrap();
                file.write_all(b"X").unwrap();
                file.set_times(fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
                    .unwrap();
                fs::set_permissions(snapshot, fs::Permissions::from_mode(0o400)).unwrap();
                assert_eq!(
                    snapshot.metadata().unwrap().modified().unwrap(),
                    metadata.modified().unwrap()
                );
            }
            "truncation" => {
                fs::set_permissions(snapshot, fs::Permissions::from_mode(0o600)).unwrap();
                fs::OpenOptions::new()
                    .write(true)
                    .open(snapshot)
                    .unwrap()
                    .set_len(first.len() as u64)
                    .unwrap();
                fs::set_permissions(snapshot, fs::Permissions::from_mode(0o400)).unwrap();
            }
            "replacement" | "symlink" => {
                fs::rename(snapshot, snapshot.with_extension("preserved")).unwrap();
                if change == "symlink" {
                    std::os::unix::fs::symlink(&original, snapshot).unwrap();
                } else {
                    fs::copy(&original, snapshot).unwrap();
                    fs::set_permissions(snapshot, fs::Permissions::from_mode(0o400)).unwrap();
                }
            }
            _ => unreachable!(),
        }
        assert!(store.verify_record(&record.id, None).is_err(), "{change}");
        assert!(store.handoff(&record.id, 1, None, &[]).is_err(), "{change}");
        let scan = store.sync(&budget()).unwrap();
        assert_eq!(scan["windows"], json!([]), "{change}");
        assert_eq!(scan["problems"].as_array().unwrap().len(), 1, "{change}");
        assert_eq!(store.status().unwrap()["sources"][0]["backfill_cursor"], 0);
    }
}

#[test]
#[cfg(target_os = "macos")]
fn capture_rejects_budget_path_and_identity_conflicts_before_publication() {
    let temp = TempDir::new().unwrap();
    let original = temp.path().join("original.jsonl");
    fs::write(&original, line("m1", "原件")).unwrap();
    let store = Store::new(temp.path().join("memory"));
    let request = capture_request(&temp, &original, "capture");
    let mut small = request.clone();
    small.max_logical_bytes = 1;
    error(store.capture_source(&small), "capture_logical_byte_budget");
    assert!(!Path::new(&request.archive_root).exists());
    let mut traversal = request.clone();
    traversal.archive_root = temp
        .path()
        .join("not-created/../memory")
        .to_str()
        .unwrap()
        .into();
    error(
        store.capture_source(&traversal),
        "capture_paths_or_budget_invalid",
    );
    assert!(!temp.path().join("not-created").exists());
    let mut overlap = request.clone();
    overlap.archive_root = store.directory.to_str().unwrap().into();
    error(store.capture_source(&overlap), "archive_directory_overlap");
    overlap.archive_root = temp.path().to_str().unwrap().into();
    error(store.capture_source(&overlap), "archive_directory_overlap");
    let link = temp.path().join("linked.jsonl");
    std::os::unix::fs::symlink(&original, &link).unwrap();
    let mut linked = request.clone();
    linked.registration.path = link.to_str().unwrap().into();
    error(store.capture_source(&linked), "capture_source_not_regular");
    store
        .register(request.registration.clone(), 1024 * 1024)
        .unwrap();
    error(
        store.capture_source(&request),
        "source_registration_conflict",
    );
    assert!(!store.directory.join("captures/capture.json").exists());
}

#[test]
fn recall_prioritizes_subject_over_incidental_history() {
    let (_temp, store, mut focused) = fixture();
    focused.project_id = None;
    focused.id = "zz-focused".into();
    focused.purpose = "shared profile memory".into();
    focused.method = None;
    focused.conditions.clear();
    focused.unknowns.clear();
    store.record(&focused).unwrap();
    let mut broad = focused.clone();
    broad.id = "aa-incidental".into();
    broad.purpose = "unrelated deployment chronology".into();
    broad.timeline[0].description = "shared profile memory history clock endpoint archive transfer command dependency window document queue worker checksum duration result ".into();
    store.record(&broad).unwrap();
    let first = store
        .recall("shared profile memory", None, &[], 2, false)
        .unwrap();
    assert_eq!(first["candidates"][0]["record"]["id"], focused.id);
    let broad_score = first["candidates"][1]["score"].clone();
    // A separate record measures repetition; rewriting an existing historical
    // timeline without an appended correction is correctly prohibited.
    broad.id = "ab-repeated".into();
    broad.timeline[0].description = broad.timeline[0].description.repeat(30);
    store.record(&broad).unwrap();
    let repeated = store
        .recall("shared profile memory", None, &[], 3, false)
        .unwrap();
    assert_eq!(repeated["candidates"][0]["record"]["id"], focused.id);
    assert_eq!(repeated["candidates"][1]["score"], broad_score);
    assert_eq!(repeated["candidates"][2]["record"]["id"], "ab-repeated");
    assert_eq!(repeated["candidates"][2]["score"], broad_score);
    let mut body_only = focused;
    body_only.id = "body-only".into();
    body_only.purpose = "other topic".into();
    body_only.timeline[0].description = "singulararchiveterm".into();
    store.record(&body_only).unwrap();
    let found = store
        .recall("singulararchiveterm", None, &[], 1, false)
        .unwrap();
    assert_eq!(found["candidates"][0]["record"]["id"], "body-only");
    assert_eq!(
        found["candidates"][0]["engineering_adoption"],
        "not_granted"
    );
}

#[test]
fn recall_skips_unreadable_history_but_get_stays_strict() {
    let (temp, store, mut record) = fixture();
    record.project_id = None;
    store.record(&record).unwrap();
    let records = temp.path().join("memory").join("records");
    fs::write(records.join("broken.json"), b"{not a history").unwrap();
    fs::write(
        records.join("credential.json"),
        serde_json::to_vec(&json!({"schema":"engineering-memory.history/1","revisions":[]}))
            .unwrap(),
    )
    .unwrap();
    let found = store.recall("碰撞结论", None, &[], 5, false).unwrap();
    assert_eq!(found["candidates"][0]["record"]["id"], "lesson-one");
    let unreadable: Vec<_> = found["unreadable_records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(unreadable, ["broken", "credential"]);
    assert!(
        found["unreadable_records"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| !item["error"].as_str().unwrap().is_empty())
    );
    assert!(store.get("broken", None).is_err());
    assert!(store.get("credential", None).is_err());
    assert_eq!(store.get("lesson-one", None).unwrap(), record);
}

#[test]
fn natural_language_recall_preserves_scope_and_sources() {
    let (temp, store, mut record) = fixture();
    record.project_id = None;
    record.purpose = "汇报工作进展：先交代完成了什么、剩余任务和真实证据".into();
    store.record(&record).unwrap();
    let question = "请用一句话说清楚目前的工作进展和剩余任务";
    let found = store.recall(question, None, &[], 5, false).unwrap();
    assert_eq!(found["candidates"][0]["record"]["id"], "lesson-one");
    assert_ne!(found["candidates"][0]["scope_status"], "scope_matched");
    assert_eq!(
        found["candidates"][0]["engineering_adoption"],
        "not_granted"
    );
    assert_eq!(
        store.recall("description", None, &[], 5, false).unwrap()["total_matches"],
        0
    );
    assert_eq!(
        store.recall("？！……", None, &[], 5, false).unwrap()["total_matches"],
        0
    );

    let mut rust = record.clone();
    rust.id = "rust-normalization".into();
    rust.purpose = "Ｒｕｓｔ café".into();
    store.record(&rust).unwrap();
    let mut trust = rust.clone();
    trust.id = "trust-boundary".into();
    trust.purpose = "trust the reviewed decision".into();
    store.record(&trust).unwrap();
    for query in ["Rust", "cafe\u{301}"] {
        let found = store.recall(query, None, &[], 5, false).unwrap();
        assert_eq!(found["total_matches"], 1);
        assert_eq!(found["candidates"][0]["record"]["id"], rust.id);
    }
    let mut punctuation = rust.clone();
    punctuation.id = "punctuation-boundary".into();
    punctuation.purpose = "进，展".into();
    store.record(&punctuation).unwrap();
    let found = store.recall("进展", None, &[], 5, false).unwrap();
    assert_eq!(found["total_matches"], 1);
    assert_eq!(store.get(&rust.id, None).unwrap().purpose, "Ｒｕｓｔ café");

    let mut other = record.clone();
    other.id = "another-project".into();
    other.project_id = Some("other-project".into());
    store.record(&other).unwrap();
    assert_eq!(
        store.recall(question, None, &[], 5, false).unwrap()["total_matches"],
        1
    );
    assert_eq!(
        store.recall(question, None, &[], 5, true).unwrap()["total_matches"],
        2
    );
    fs::write(
        temp.path().join("original.jsonl"),
        line("m1", "原件后来发生变化"),
    )
    .unwrap();
    let changed = store.recall(question, None, &[], 5, false).unwrap();
    assert!(changed["candidates"][0]["source_error"].is_string());
    assert_ne!(changed["candidates"][0]["scope_status"], "scope_matched");
}

fn tick_policy() -> TickPolicy {
    TickPolicy {
        schema: "engineering-memory.tick-policy/1".into(),
        sync: budget(),
        max_hook_sources: 8,
        max_registration_bytes: 16 * 1024 * 1024,
        max_pending_packets: 1,
        max_curation_packets: 2,
    }
}

fn queued_sources(
    temp: &TempDir,
    store: &Store,
    count: usize,
) -> Vec<(String, std::path::PathBuf)> {
    let root = temp.path().canonicalize().unwrap();
    let policy = HookPolicy {
        schema: "engineering-memory.hook-policy/1".into(),
        source_roots: vec![root.to_str().unwrap().into()],
        skill_path: root.join("SKILL.md").to_str().unwrap().into(),
        max_candidates: 3,
        max_context_bytes: 8192,
        include_other_projects: false,
    };
    let mut result = Vec::new();
    for i in 0..count {
        let path = root.join(format!("source-{i}.jsonl"));
        fs::write(
            &path,
            line(&format!("m{i}"), "有限测试原件，保留本次观察与未知"),
        )
        .unwrap();
        let receipt = store
            .queue_hook_source(
                &policy,
                "claude",
                Some("test-profile"),
                &json!({"session_id":"session-one","transcript_path":path}),
            )
            .unwrap();
        result.push((receipt["id"].as_str().unwrap().to_owned(), path));
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[test]
fn background_tick_caps_pending_and_resumes_after_review() {
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    queued_sources(&temp, &store, 3);
    let policy = tick_policy();
    let first = store.tick(&policy).unwrap();
    assert_eq!(first["status"], "ran");
    assert_eq!(
        first["result"]["intake"]["registered"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(packet_ids(&store).len(), 1);
    assert_eq!(store.status().unwrap()["experiences"], 0);
    assert_eq!(first["result"]["work"]["new_model_calls"], 0);
    let before = store.status().unwrap()["sources"].clone();
    let second = store.tick(&policy).unwrap();
    assert_eq!(second["result"]["work"]["sync"]["backpressure"], true);
    assert_eq!(second["result"]["work"]["sync"]["read_bytes"], 0);
    assert_eq!(store.status().unwrap()["sources"], before);
    store
        .resolve(&no_delta(&store, &packet_ids(&store)[0]))
        .unwrap();
    let third = store.tick(&policy).unwrap();
    assert!(
        third["result"]["work"]["sync"]["read_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(packet_ids(&store).len(), 1);
    assert_eq!(
        fs::read_dir(store.directory.join("tick-runs"))
            .unwrap()
            .count(),
        3
    );
    assert_eq!(
        read_json::<Value>(&store.directory.join("last-tick.json")).unwrap()["id"],
        third["id"]
    );

    let lease = Store::new(store.directory.join("tick-lease"));
    let guard = lease.lock().unwrap();
    assert_eq!(store.tick(&policy).unwrap()["status"], "busy");
    assert_eq!(
        fs::read_dir(store.directory.join("tick-runs"))
            .unwrap()
            .count(),
        3
    );
    drop(guard);
    let mut invalid = policy.clone();
    invalid.max_pending_packets = 0;
    error(store.tick(&invalid), "tick_policy_budget");
}

#[test]
fn hook_registration_budget_is_aggregate_and_bad_sources_do_not_starve_others() {
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    let originals = queued_sources(&temp, &store, 3);
    let max_bytes = originals
        .iter()
        .map(|(_, p)| p.metadata().unwrap().len())
        .max()
        .unwrap();
    let first = store.ingest_hook_sources(3, max_bytes).unwrap();
    assert_eq!(first["registered"].as_array().unwrap().len(), 1);
    assert_eq!(first["waiting"].as_array().unwrap().len(), 2);
    assert!(first["verification_bytes_reserved"].as_u64().unwrap() <= max_bytes);
    for _ in 0..2 {
        store.ingest_hook_sources(3, max_bytes).unwrap();
    }
    assert_eq!(
        store.status().unwrap()["sources"].as_array().unwrap().len(),
        3
    );

    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    let originals = queued_sources(&temp, &store, 2);
    fs::write(&originals[0].1, "not a session\n").unwrap();
    assert_eq!(
        store.ingest_hook_sources(1, 1_000_000).unwrap()["waiting"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let next = store.ingest_hook_sources(1, 1_000_000).unwrap();
    assert_eq!(next["registered"][0]["id"], originals[1].0);
}

#[test]
fn failed_prefix_verification_still_consumes_the_aggregate_budget() {
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    let a = temp.path().join("a.jsonl");
    let b = temp.path().join("b.jsonl");
    let original = line("m1", "one observation");
    fs::write(&a, &original).unwrap();
    fs::write(&b, line("m2", "two observation")).unwrap();
    store.register(registration(&a, "a"), 100_000).unwrap();
    store.register(registration(&b, "b"), 100_000).unwrap();
    fs::write(&a, original.replace("one", "bad")).unwrap();
    let mut limit = budget();
    limit.max_verify_bytes = original.len() as u64;
    let result = store.sync(&limit).unwrap();
    assert_eq!(result["verified_prefix_bytes"], original.len());
    assert!(result["windows"].as_array().unwrap().is_empty());
    assert!(
        result["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["reason"] == "source_prefix_changed")
    );
    assert!(
        result["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["reason"] == "prefix_verification_budget")
    );
}

#[test]
fn live_appends_can_register_and_scan_without_accepting_prefix_rewrites() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    let path = temp.path().join("live.jsonl");
    let initial = (0..24)
        .map(|i| line(&format!("old-{i}"), &"x".repeat(512 * 1024)))
        .collect::<String>();
    fs::write(&path, &initial).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let writer_path = path.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(writer_path)
            .unwrap();
        file.write_all(line("ready", "writer started").as_bytes())
            .unwrap();
        ready_tx.send(()).unwrap();
        let mut count = 0;
        while !done.load(Ordering::Acquire) && count < 100_000 {
            file.write_all(line(&format!("live-{count}"), "new observation").as_bytes())
                .unwrap();
            count += 1;
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
        count
    });
    ready_rx.recv().unwrap();
    let registered = store.register(registration(&path, "live"), 64 * 1024 * 1024);
    let mut limits = budget();
    limits.max_verify_bytes = 128 * 1024 * 1024;
    limits.max_windows = 1;
    let scanned = registered.as_ref().ok().map(|_| store.sync(&limits));
    stop.store(true, Ordering::Release);
    assert!(writer.join().unwrap() > 0);
    let registered = registered.unwrap();
    assert!(
        registered["verified_prefix_bytes"].as_u64().unwrap()
            >= registered["baseline_end"].as_u64().unwrap()
    );
    let scanned = scanned.unwrap().unwrap();
    assert!(
        scanned["problems"].as_array().unwrap().is_empty(),
        "{scanned}"
    );
    assert_eq!(scanned["windows"].as_array().unwrap().len(), 1);
    assert!(scanned["verified_prefix_bytes"].as_u64().unwrap() <= limits.max_verify_bytes);
    let bytes = fs::read(&path).unwrap();
    for packet in packet_ids(&store) {
        for anchor in store.packet(&packet).unwrap().sources {
            assert_eq!(
                engineering_memory_runtime::store::sha(
                    &bytes[anchor.byte_start as usize..anchor.byte_end as usize]
                ),
                anchor.sha256
            );
        }
    }
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all(b"!")
        .unwrap();
    let changed = store.sync(&limits).unwrap();
    assert!(
        changed["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["reason"] == "source_prefix_changed")
    );
}

#[test]
fn revisions_preserve_original_corrections_and_concurrency() {
    let (_temp, store, record) = fixture();
    assert_eq!(store.record(&record).unwrap()["effect"], true);
    assert_eq!(store.record(&record).unwrap()["effect"], false);
    let next = corrected(&record);
    let mut bad = next.clone();
    bad.timeline[0].description = "悄悄改掉旧判断".into();
    error(store.record(&bad), "timeline_rewrite");
    let mut fork = next.clone();
    fork.purpose = "另一解释".into();
    let directory = store.directory.clone();
    let directory2 = directory.clone();
    let a = std::thread::spawn(move || Store::new(directory).record(&next));
    let b = std::thread::spawn(move || Store::new(directory2).record(&fork));
    let results = [a.join().unwrap(), b.join().unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(store.get(&record.id, Some(1)).unwrap(), record);
    assert_eq!(store.get(&record.id, None).unwrap().timeline.len(), 2);
}

#[test]
fn structural_and_source_forgeries_are_rejected() {
    let (_temp, store, record) = fixture();
    let mut raw = serde_json::to_value(&record).unwrap();
    raw.as_object_mut().unwrap().remove("supersedes");
    assert!(serde_json::from_value::<ExperienceRecord>(raw).is_err());
    let mut bad = record.clone();
    bad.status = "proved".into();
    error(store.record(&bad), "invalid_ExperienceRecord_status");
    let mut bad = record.clone();
    bad.sources[0].text = "伪造".into();
    error(store.record(&bad), "source_anchor_mismatch");
    let mut bad = corrected(&record);
    bad.timeline[1].corrects = vec!["future".into()];
    error(validate_experience(&bad), "correction_target_missing");
    assert!(store.get(&record.id, None).is_err());
}

#[test]
fn scope_requires_verified_source_and_conditions() {
    let (_temp, store, record) = fixture();
    store.record(&record).unwrap();
    let recall = |project, conditions: Vec<String>, other| {
        store.recall("", project, &conditions, 5, other).unwrap()
    };
    let matched = recall(Some("project-one"), record.conditions.clone(), false);
    assert_eq!(matched["candidates"][0]["scope_status"], "scope_matched");
    assert_eq!(
        matched["candidates"][0]["semantic_review"]["execution"]["conclusions"],
        json!(["scope_matched(record)"])
    );
    assert_eq!(
        matched["candidates"][0]["engineering_adoption"],
        "not_granted"
    );
    assert_eq!(
        recall(Some("project-one"), vec![], false)["candidates"][0]["scope_status"],
        "review_required"
    );
    assert_eq!(
        recall(Some("other"), record.conditions.clone(), false)["candidates"],
        json!([])
    );
    assert_eq!(
        recall(None, record.conditions.clone(), true)["candidates"][0]["scope_status"],
        "review_required"
    );
    fs::write(&record.sources[0].source_path, line("m1", "原文已改")).unwrap();
    let drift = recall(Some("project-one"), record.conditions.clone(), false);
    assert_eq!(drift["candidates"][0]["scope_status"], "review_required");
    assert_eq!(
        drift["candidates"][0]["semantic_review"]["execution"]["conclusions"],
        json!([])
    );
    error(
        store.handoff(&record.id, 1, Some("project-one"), &record.conditions),
        "source_",
    );
}

#[test]
fn feedback_requires_use_version_and_evidence() {
    let (_temp, store, record) = fixture();
    store.record(&record).unwrap();
    let mut feedback = UsageFeedback {
        schema: "engineering-memory.feedback/1".into(),
        id: "use-one".into(),
        record_id: record.id,
        revision: 1,
        project_id: None,
        used: false,
        outcome: "helped".into(),
        result: "只有召回".into(),
        sources: vec![],
        elapsed_ms: None,
        unknowns: vec!["未知".into()],
    };
    error(store.feedback(&feedback), "feedback_use_outcome_conflict");
    feedback.used = true;
    error(store.feedback(&feedback), "feedback_evidence_missing");
    feedback.outcome = "no_delta".into();
    feedback.revision = 99;
    error(store.feedback(&feedback), "experience_revision_missing");
    feedback.revision = 1;
    assert_eq!(store.feedback(&feedback).unwrap()["effect"], true);
    assert_eq!(store.feedback(&feedback).unwrap()["effect"], false);
}

#[test]
fn append_partial_alias_fork_and_drift() {
    let (temp, store, _) = fixture();
    let path = temp.path().join("original.jsonl");
    store
        .register(registration(&path, "a"), 16 * 1024 * 1024)
        .unwrap();
    let first = store.sync(&budget()).unwrap();
    assert_eq!(first["windows"][0]["new_messages"], 1);
    assert_eq!(packet_ids(&store).len(), 1);
    assert_eq!(store.sync(&budget()).unwrap()["windows"], json!([]));
    let alias = temp.path().join("alias.jsonl");
    fs::copy(&path, &alias).unwrap();
    store
        .register(registration(&alias, "b"), 16 * 1024 * 1024)
        .unwrap();
    assert_eq!(store.sync(&budget()).unwrap()["windows"][0]["aliases"], 1);
    assert_eq!(packet_ids(&store).len(), 1);
    let fork = temp.path().join("fork.jsonl");
    fs::write(&fork, line("m1", "同一消息ID的不同分支")).unwrap();
    store
        .register(registration(&fork, "c"), 16 * 1024 * 1024)
        .unwrap();
    assert_eq!(store.sync(&budget()).unwrap()["windows"][0]["forks"], 1);
    let next = line("m2", "新增纠正");
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(next.trim_end().as_bytes()).unwrap();
    file.sync_all().unwrap();
    let partial = store.sync(&budget()).unwrap();
    assert_eq!(partial["windows"], json!([]));
    assert!(
        partial["waiting"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["reason"] == "partial_tail_waiting")
    );
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
    let appended = store.sync(&budget()).unwrap();
    assert_eq!(appended["windows"][0]["lane"], "live");
    assert_eq!(appended["windows"][0]["new_messages"], 1);
    fs::write(
        &path,
        line("m1", "发生修改") + &line("m2", "新增纠正") + &line("m3", "补长避免只因截断失败"),
    )
    .unwrap();
    assert!(
        store.sync(&budget()).unwrap()["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["reason"] == "source_prefix_changed")
    );
    assert_eq!(store.status().unwrap()["experiences"], 0);
}

#[test]
fn live_tail_precedes_old_backlog_and_cursor_replays() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("old.jsonl");
    fs::write(
        &path,
        (0..20)
            .map(|i| line(&format!("m{i}"), "历史"))
            .collect::<String>(),
    )
    .unwrap();
    let store = Store::new(temp.path().join("memory"));
    store
        .register(registration(&path, "source"), 16 * 1024 * 1024)
        .unwrap();
    let mut small = budget();
    small.max_bytes = 500;
    small.max_windows = 1;
    let first = store.sync(&small).unwrap();
    assert_eq!(first["windows"][0]["lane"], "backfill");
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(line("new", "新消息先处理").as_bytes())
        .unwrap();
    file.sync_all().unwrap();
    let live = store.sync(&small).unwrap();
    assert_eq!(live["windows"][0]["lane"], "live");
    let state_file = store.directory.join("sources/source.json");
    let mut state: Value = read_json(&state_file).unwrap();
    state["backfill_cursor"] = json!(0);
    atomic_json(&state_file, &state).unwrap();
    let before = packet_ids(&store).len();
    store.sync(&small).unwrap();
    assert_eq!(packet_ids(&store).len(), before);
    let mut no_verify = budget();
    no_verify.max_verify_bytes = 0;
    assert_eq!(
        store.sync(&no_verify).unwrap()["problems"][0]["reason"],
        "prefix_verification_budget"
    );
}

#[test]
fn packet_resolution_accounts_for_all_sources_and_is_recoverable() {
    let (temp, store, _) = fixture();
    let path = temp.path().join("original.jsonl");
    fs::write(&path, line("m1", "第一项") + &line("m2", "第二项")).unwrap();
    store
        .register(registration(&path, "a"), 16 * 1024 * 1024)
        .unwrap();
    store.sync(&budget()).unwrap();
    let pid = packet_ids(&store)[0].clone();
    let packet = store.packet(&pid).unwrap();
    let one = lesson(packet.sources[0].clone(), "one");
    let two = lesson(packet.sources[1].clone(), "two");
    let mut resolution = no_delta(&store, &pid);
    resolution.action = "commit".into();
    resolution.records = vec![one.clone()];
    resolution.ignored = vec![];
    error(store.resolve(&resolution), "packet_source_unaccounted");
    assert!(store.get("one", None).is_err());
    resolution.records.push(two.clone());
    // Simulate a crash after durable intent plus first history publication.
    atomic_json(
        &store
            .directory
            .join("resolutions")
            .join(format!("{pid}.json")),
        &resolution,
    )
    .unwrap();
    store.record(&one).unwrap();
    let recovered = store.run(&budget(), 1).unwrap();
    assert_eq!(
        recovered["resolutions_recovered"][0]["result"]["state"],
        "committed"
    );
    assert_eq!(store.resolve(&resolution).unwrap()["effect"], false);
    assert_eq!(store.get("two", None).unwrap(), two);
    let mut conflict = resolution;
    conflict.reason = "改写同一次整理".into();
    error(store.resolve(&conflict), "resolution_conflict");
}

#[test]
fn gaps_cannot_be_declared_curated_and_sources_cannot_be_invented() {
    let (temp, store, _) = fixture();
    let path = temp.path().join("original.jsonl");
    fs::write(&path, line("m1", "可读") + "{broken}\n").unwrap();
    store
        .register(registration(&path, "a"), 16 * 1024 * 1024)
        .unwrap();
    store.sync(&budget()).unwrap();
    let pid = packet_ids(&store)[0].clone();
    let mut resolution = no_delta(&store, &pid);
    error(store.resolve(&resolution), "packet_has_unresolved_gaps");
    resolution.action = "reject".into();
    assert_eq!(store.resolve(&resolution).unwrap()["state"], "rejected");
    assert_eq!(store.status().unwrap()["retained_gaps"], 1);
}

#[test]
fn worker_without_provider_leaves_pending() {
    let (temp, store, _) = fixture();
    store
        .register(
            registration(&temp.path().join("original.jsonl"), "a"),
            16 * 1024 * 1024,
        )
        .unwrap();
    let run = store.run(&budget(), 1).unwrap();
    assert_eq!(run["extractor"], "unconfigured");
    assert_eq!(run["curated"], 0);
    assert_eq!(packet_ids(&store).len(), 1);
}

#[test]
fn large_image_message_keeps_source_identity_and_resumes_after_budget_wait() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("image.jsonl");
    let original = format!(
        "{}\n",
        json!({"type":"user","sessionId":"image-session","uuid":"image-request",
        "message":{"role":"user","content":[
            {"type":"text","text":"图里的旧模块已由新的审阅流程替换"},
            {"type":"image","source":{"type":"base64","data":"A".repeat(3 * 1024 * 1024)}}
        ]}})
    );
    fs::write(
        &path,
        original.clone() + &line("after-image", "后续纠正仍须保留"),
    )
    .unwrap();
    let store = Store::new(temp.path().join("memory"));
    store
        .register(registration(&path, "image"), 32 * 1024 * 1024)
        .unwrap();
    let waiting = store.sync(&budget()).unwrap();
    assert!(waiting["problems"].as_array().unwrap().is_empty());
    assert!(waiting["windows"].as_array().unwrap().is_empty());
    assert_eq!(
        waiting["waiting"][0]["reason"],
        "window_budget_below_next_record"
    );
    assert_eq!(store.status().unwrap()["sources"][0]["backfill_cursor"], 0);
    assert!(packet_ids(&store).is_empty());

    let mut larger = budget();
    larger.max_bytes = 8 * 1024 * 1024;
    let read = store.sync(&larger).unwrap();
    assert!(read["problems"].as_array().unwrap().is_empty());
    assert!(read["read_bytes"].as_u64().unwrap() <= larger.max_bytes);
    assert!(read["window_probe_bytes"].as_u64().unwrap() <= larger.max_bytes);
    assert_eq!(read["windows"][0]["byte_end"], original.len());
    let packet_id = read["windows"][0]["id"].as_str().unwrap();
    let packet = store.packet(packet_id).unwrap();
    assert!(packet.gaps.is_empty());
    assert_eq!(packet.sources.len(), 1);
    assert_eq!(packet.sources[0].text, "图里的旧模块已由新的审阅流程替换");
    assert_eq!(packet.sources[0].byte_end, original.len() as u64);
    assert_eq!(
        packet.sources[0].sha256,
        engineering_memory_runtime::store::sha(&original)
    );
    let mut record = lesson(packet.sources[0].clone(), "large-source");
    record.timeline[0].kind = "user_report".into();
    record.timeline[0].description = packet.sources[0].text.clone();
    store.record(&record).unwrap();
    store.resolve(&no_delta(&store, packet_id)).unwrap();
    let next = store.sync(&larger).unwrap();
    assert_eq!(next["windows"][0]["byte_start"], original.len());
    assert_eq!(next["windows"][0]["new_messages"], 1);
    assert_eq!(
        store.status().unwrap()["sources"][0]["historical_bytes_remaining"],
        0
    );

    // Image bytes belong to the original identity even though only visible text
    // was interpreted. Altering one of them must invalidate the stored source.
    use std::io::{Seek, SeekFrom};
    let offset = original.find(&"A".repeat(64)).unwrap() as u64;
    let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(b"B").unwrap();
    file.sync_all().unwrap();
    error(
        engineering_memory_runtime::store::verify_sources(&record.sources),
        "source_anchor_mismatch",
    );
}

#[test]
fn oversized_record_and_partial_tail_do_not_skip_history() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("oversized.jsonl");
    fs::write(
        &path,
        line("too-large", &"x".repeat(16 * 1024 * 1024)) + &line("later", "不能跳到这里"),
    )
    .unwrap();
    let store = Store::new(temp.path().join("memory"));
    store
        .register(registration(&path, "oversized"), 32 * 1024 * 1024)
        .unwrap();
    let mut large = budget();
    large.max_bytes = 32 * 1024 * 1024;
    large.max_verify_bytes = 64 * 1024 * 1024;
    let result = store.sync(&large).unwrap();
    assert!(result["problems"].as_array().unwrap().is_empty());
    assert_eq!(
        result["waiting"][0]["reason"],
        "next_record_exceeds_window_limit"
    );
    assert_eq!(store.status().unwrap()["sources"][0]["backfill_cursor"], 0);
    assert!(packet_ids(&store).is_empty());

    let tail = temp.path().join("partial.jsonl");
    let head = line("before", "已有的完整消息");
    let partial = line("large-tail", &"p".repeat(3 * 1024 * 1024));
    fs::write(&tail, head.clone() + partial.trim_end()).unwrap();
    let other = Store::new(temp.path().join("partial-memory"));
    let registered = other
        .register(registration(&tail, "partial"), 16 * 1024 * 1024)
        .unwrap();
    assert_eq!(registered["baseline_end"], head.len());
    let scan = other.sync(&large).unwrap();
    assert_eq!(scan["windows"][0]["new_messages"], 1);
    assert!(
        scan["waiting"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["reason"] == "partial_tail_waiting")
    );
    let mut file = fs::OpenOptions::new().append(true).open(&tail).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
    let resumed = other.sync(&large).unwrap();
    assert_eq!(resumed["windows"][0]["lane"], "live");
    assert_eq!(resumed["windows"][0]["byte_start"], head.len());
    assert_eq!(resumed["windows"][0]["new_messages"], 1);
}

#[test]
fn packets_stay_within_source_review_limit_without_losing_following_messages() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("many.jsonl");
    fs::write(
        &path,
        (0..130)
            .map(|i| line(&format!("m-{i}"), "每条都应可核验"))
            .collect::<String>(),
    )
    .unwrap();
    let store = Store::new(temp.path().join("memory"));
    store
        .register(registration(&path, "many"), 16 * 1024 * 1024)
        .unwrap();
    let first = store.sync(&budget()).unwrap();
    let first_id = first["windows"][0]["id"].as_str().unwrap();
    assert_eq!(store.packet(first_id).unwrap().sources.len(), 128);
    store.resolve(&no_delta(&store, first_id)).unwrap();
    let second = store.sync(&budget()).unwrap();
    let second_id = second["windows"][0]["id"].as_str().unwrap();
    assert_eq!(store.packet(second_id).unwrap().sources.len(), 2);
    store.resolve(&no_delta(&store, second_id)).unwrap();
    assert_eq!(
        store.status().unwrap()["sources"][0]["historical_bytes_remaining"],
        0
    );
}

#[test]
fn explicit_extractor_commits_no_delta_and_failure_stays_pending() {
    let (temp, store, _) = fixture();
    store
        .register(
            registration(&temp.path().join("original.jsonl"), "a"),
            16 * 1024 * 1024,
        )
        .unwrap();
    store.sync(&budget()).unwrap();
    let pid = packet_ids(&store)[0].clone();
    let resolution = no_delta(&store, &pid);
    let response = temp.path().join("response.json");
    atomic_json(&response, &resolution).unwrap();
    // An explicit deterministic fixture, not a claim that an LLM was used or evaluated.
    store
        .configure_extractor(&Extractor {
            program: "/bin/cat".into(),
            args: vec![response.to_str().unwrap().into()],
            timeout_seconds: 2,
            max_output_bytes: 1_000_000,
        })
        .unwrap();
    assert_eq!(
        store.run(&budget(), 1).unwrap()["results"][0]["result"]["state"],
        "no_delta"
    );
    assert!(packet_ids(&store).is_empty());
    let path = temp.path().join("more.jsonl");
    fs::write(&path, line("other", "失败保留")).unwrap();
    store
        .register(registration(&path, "b"), 16 * 1024 * 1024)
        .unwrap();
    store
        .configure_extractor(&Extractor {
            program: "/usr/bin/false".into(),
            args: vec![],
            timeout_seconds: 2,
            max_output_bytes: 1_000_000,
        })
        .unwrap();
    assert!(
        store.run(&budget(), 1).unwrap()["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("extractor_exit_failed")
    );
    assert_eq!(packet_ids(&store).len(), 1);
}

#[test]
fn extractor_timeout_and_self_review_do_not_complete_work() {
    let (temp, store, _) = fixture();
    store
        .register(
            registration(&temp.path().join("original.jsonl"), "a"),
            16 * 1024 * 1024,
        )
        .unwrap();
    store.sync(&budget()).unwrap();
    let pid = packet_ids(&store)[0].clone();
    let packet = store.packet(&pid).unwrap();
    let mut resolution = no_delta(&store, &pid);
    resolution.action = "commit".into();
    resolution.ignored.clear();
    resolution
        .records
        .push(lesson(packet.sources[0].clone(), "automatically-reviewed"));
    let response = temp.path().join("response.json");
    atomic_json(&response, &resolution).unwrap();
    store
        .configure_extractor(&Extractor {
            program: "/bin/cat".into(),
            args: vec![response.to_str().unwrap().into()],
            timeout_seconds: 2,
            max_output_bytes: 1_000_000,
        })
        .unwrap();
    assert!(
        store.run(&budget(), 1).unwrap()["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("extractor_cannot_self_review")
    );
    assert!(store.get("automatically-reviewed", None).is_err());
    store
        .configure_extractor(&Extractor {
            program: "/bin/sleep".into(),
            args: vec!["5".into()],
            timeout_seconds: 1,
            max_output_bytes: 1024,
        })
        .unwrap();
    assert!(
        store.run(&budget(), 1).unwrap()["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("extractor_timeout")
    );
    assert_eq!(packet_ids(&store).len(), 1);
}

#[test]
fn aliases_within_one_window_keep_every_original_location() {
    let (temp, store, _) = fixture();
    let path = temp.path().join("original.jsonl");
    let raw = line("same-id", "同一原记录");
    fs::write(&path, format!("{raw}{raw}")).unwrap();
    store
        .register(registration(&path, "a"), 16 * 1024 * 1024)
        .unwrap();
    store.sync(&budget()).unwrap();
    let packet = store.packet(&packet_ids(&store)[0]).unwrap();
    assert_eq!(packet.sources.len(), 1);
    let shard = fs::read_dir(store.directory.join("messages"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let file = fs::read_dir(shard).unwrap().next().unwrap().unwrap().path();
    let identity: Value = read_json(&file).unwrap();
    assert_eq!(
        identity["variants"][0]["anchors"].as_array().unwrap().len(),
        2
    );
    assert_eq!(store.sync(&budget()).unwrap()["windows"], json!([]));
}

#[test]
fn tick_large_prefix_budget_is_streamed_and_bounded() {
    let limit = engineering_memory_runtime::worker::MAX_TICK_VERIFY_BYTES;
    assert_eq!(limit, 8 << 30);
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    queued_sources(&temp, &store, 1);
    let mut policy = tick_policy();
    policy.max_registration_bytes = limit;
    policy.sync.max_verify_bytes = limit;
    let ran = store.tick(&policy).unwrap();
    assert_eq!(ran["status"], "ran");
    assert_eq!(
        ran["result"]["intake"]["registered"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut over = policy.clone();
    over.max_registration_bytes = limit + 1;
    error(store.tick(&over), "tick_policy_budget");
    let mut over = policy.clone();
    over.sync.max_verify_bytes = limit + 1;
    error(store.tick(&over), "tick_policy_budget");

    // A registration budget below the source keeps it queued with a reason.
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    queued_sources(&temp, &store, 1);
    let mut small = tick_policy();
    small.max_registration_bytes = 1;
    let waiting = store.tick(&small).unwrap();
    assert_eq!(
        waiting["result"]["intake"]["waiting"][0]["reason"],
        "hook_registration_byte_budget"
    );
    assert!(
        store.status().unwrap()["sources"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "writes a 600 MiB transcript; run explicitly for the large-file path"]
fn tick_registers_and_scans_a_600_mib_session() {
    let temp = TempDir::new().unwrap();
    let store = Store::new(temp.path().join("memory"));
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("large.jsonl");
    {
        let mut file = fs::File::create(&path).unwrap();
        let text = "x".repeat(1024 * 1024 - 256);
        for i in 0..600 {
            file.write_all(line(&format!("m{i}"), &text).as_bytes())
                .unwrap();
        }
    }
    let policy = HookPolicy {
        schema: "engineering-memory.hook-policy/1".into(),
        source_roots: vec![root.to_str().unwrap().into()],
        skill_path: root.join("SKILL.md").to_str().unwrap().into(),
        max_candidates: 3,
        max_context_bytes: 8192,
        include_other_projects: false,
    };
    store
        .queue_hook_source(
            &policy,
            "claude",
            Some("test-profile"),
            &json!({"session_id":"session-one","transcript_path":path}),
        )
        .unwrap();
    let mut tick = tick_policy();
    tick.max_registration_bytes = 3 << 30;
    tick.sync.max_verify_bytes = 3 << 30;
    tick.sync.max_bytes = 64 * 1024 * 1024;
    tick.sync.max_seconds = 60;
    let ran = store.tick(&tick).unwrap();
    assert_eq!(
        ran["result"]["intake"]["registered"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        ran["result"]["work"]["sync"]["read_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn process_drafts_rank_after_curated_records_without_hiding_them() {
    let (_temp, store, mut curated) = fixture();
    curated.project_id = None;
    curated.id = "zz-curated".into();
    curated.purpose = "游标卡尺 读数的边界".into();
    curated.status = "draft".into();
    store.record(&curated).unwrap();
    // Machine process drafts that match the query far more strongly.
    for n in 0..3 {
        let mut draft = curated.clone();
        draft.id = format!("process-{n:064x}");
        draft.purpose = "游标卡尺 读数 游标卡尺 读数".into();
        draft.method = None;
        store.record(&draft).unwrap();
    }
    let found = store.recall("游标卡尺 读数", None, &[], 4, false).unwrap();
    let ids: Vec<&str> = found["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["record"]["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids[0], "zz-curated", "a curated draft stays ahead: {ids:?}");
    assert!(ids[1..].iter().all(|id| id.starts_with("process-")));
    assert_eq!(found["total_matches"], 4);
    assert_eq!(found["process_drafts"], "ranked_after_curated");
    // Drafts still fill the remaining slots when nothing curated matches.
    let only = store.recall("游标卡尺", None, &[], 1, false).unwrap();
    assert_eq!(only["candidates"][0]["record"]["id"], "zz-curated");
    // Once a host reviews a process record, it ranks as curated experience.
    let mut reviewed = store.get(&format!("process-{:064x}", 2), None).unwrap();
    reviewed.revision = 2;
    reviewed.supersedes = Some(1);
    reviewed.status = "reviewed".into();
    store.record(&reviewed).unwrap();
    let found = store.recall("游标卡尺 读数", None, &[], 4, false).unwrap();
    assert_eq!(found["candidates"][0]["record"]["id"], reviewed.id.as_str());
    assert_eq!(found["candidates"][1]["record"]["id"], "zz-curated");
}

#[test]
fn recall_scores_every_curated_record_and_only_the_newest_process_drafts() {
    let (_temp, store, mut curated) = fixture();
    curated.project_id = None;
    curated.id = "zz-curated".into();
    curated.purpose = "游标卡尺 读数的边界".into();
    store.record(&curated).unwrap();
    let draft_id = |n: u32| format!("process-{n:064x}");
    for n in 0..5 {
        let mut draft = curated.clone();
        draft.id = draft_id(n);
        draft.status = "draft".into();
        draft.method = None;
        draft.purpose = "游标卡尺 读数".into();
        draft.recorded_at = format!("2026-10-2{n}T00:00:00+00:00");
        store.record(&draft).unwrap();
    }
    let ids = |found: &Value| -> Vec<String> {
        found["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["record"]["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let recall = || {
        store
            .recall_with_draft_window("游标卡尺 读数", None, &[], 10, false, 2)
            .unwrap()
    };
    // Only the tick maintains the manifest; until it runs, every history is parsed.
    let found = recall();
    assert_eq!(found["draft_window"]["older_drafts_not_scored"], 0);
    assert_eq!(ids(&found).len(), 6);
    assert_eq!(store.recall_manifest_refresh(100).unwrap()["noted"], 6);
    let found = recall();
    assert_eq!(found["draft_window"]["scored_drafts"], 2);
    assert_eq!(found["draft_window"]["older_drafts_not_scored"], 3);
    let mut got = ids(&found);
    assert_eq!(got.remove(0), "zz-curated");
    got.sort();
    assert_eq!(got, vec![draft_id(3), draft_id(4)]);
    // A reviewed process record is curated experience: its changed history is
    // parsed at once, and the next refresh notes it as curated.
    let mut reviewed = store.get(&draft_id(0), None).unwrap();
    reviewed.revision = 2;
    reviewed.supersedes = Some(1);
    reviewed.status = "reviewed".into();
    store.record(&reviewed).unwrap();
    let found = recall();
    assert!(ids(&found).contains(&draft_id(0)), "{found:#}");
    assert_eq!(found["draft_window"]["older_drafts_not_scored"], 2);
    assert_eq!(store.recall_manifest_refresh(100).unwrap()["noted"], 1);
    let found = recall();
    assert!(ids(&found).contains(&draft_id(0)), "{found:#}");
    assert_eq!(found["draft_window"]["older_drafts_not_scored"], 2);
    // A record write leaves no side file; only the refresh writes the manifest.
    let manifest = store.directory.join("recall-manifest.jsonl");
    let before = fs::read(&manifest).unwrap();
    let mut another = curated.clone();
    another.id = "zz-another".into();
    store.record(&another).unwrap();
    assert_eq!(fs::read(&manifest).unwrap(), before);
    // The default window keeps all five drafts beside the two curated records.
    let all = store.recall("游标卡尺 读数", None, &[], 10, false).unwrap();
    assert_eq!(ids(&all).len(), 7);
}
