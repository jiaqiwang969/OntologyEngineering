use oe_core::json::canonical_digest;
use oe_core::{SkillRoot, sha256};
use oe_knowledge::{context, feedback, packet};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    path: PathBuf,
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("var/maintenance/rust-tests");
        let path = base.join(format!(
            "knowledge-packet-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("var/projects/p")).unwrap();
        fs::create_dir_all(path.join("sources")).unwrap();
        fs::write(path.join("SKILL.md"), b"synthetic test").unwrap();
        fs::write(
            path.join("sources/source.pdf"),
            b"synthetic source bytes; no PDF parser invoked without locator",
        )
        .unwrap();
        let root = SkillRoot::new(&path).unwrap();
        Self { path, root }
    }
    fn context(&self, revision: &str) -> Value {
        let v = json!({"project":{"id":"project","revision":revision},"object":{"id":"object","revision":revision},"decision_id":"decision","goal":"Compare exact original conditions"});
        self.write("var/projects/p/source.json", &v);
        let fields = [
            "/project/id",
            "/project/revision",
            "/object/id",
            "/object/revision",
            "/decision_id",
            "/goal",
        ]
        .map(|p| json!({"source":"var/projects/p/source.json","pointer":p,"target":p}));
        context::project(&self.root,&json!({"schema":context::PROJECTION_SCHEMA,"base":{"schema":context::SCHEMA,"task_id":"turn"},"fields":fields})).unwrap()
    }
    fn write(&self, path: &str, value: &Value) {
        let p = self.path.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
    fn report(&self, c: &Value) -> Value {
        let body = "条件😀 e\u{301}：只适用于A。边界未知。";
        let selected = body.chars().skip(2).take(5).collect::<String>();
        json!({"schema":"ontology-engineering.knowledge-search-result/v1","status":"partial","query":"Applicable?","context_sha256":canonical_digest(c).unwrap(),"parameter_verification":"not_run","run_id":"fixture-query-1","results":[{"id":"book:1","catalog":"fixture","catalog_title":"Synthetic","printed_page":null,"pdf_page":1,"source_pdf":self.path.join("sources/source.pdf").to_str().unwrap(),"source_relative_path":"source.pdf","source_url":null,"source_sha256":sha256(fs::read(self.path.join("sources/source.pdf")).unwrap()),"native_text":body,"text_sha256":sha256(body),"judgment":{"relation":"related","origin":"fixture"},"knowledge_units":[{"id":"u1","page_id":"book:1","source_text_sha256":sha256(body),"context_sha256":canonical_digest(c).unwrap(),"source_span":{"start":2,"end":7,"text":selected,"sha256":sha256(&selected)},"status":"source_passage_candidate","applicability":"not_assessed"}]}],"errors":[{"code":"budget_exhausted"}],"query_budget":{"max_requests":1,"network_requests":1,"remaining_requests":0,"denied_requests":1,"status":"exhausted"},"elapsed_seconds":0.3,"timing":{"stage_seconds":{"retrieve":0.1}},"calls":[{"origin":"fixture","dispatched":false}],"execution_summary":{"partial":1}})
    }
    fn packet(&self, c: &Value, path: &str) -> Value {
        let p = packet::make(&self.root, &self.report(c), c).unwrap();
        self.write(path, &p);
        p
    }
    fn request(&self, c: &Value) -> Value {
        json!({"context":c,"packet_path":"var/projects/p/packet.json","candidate_id":"book:1","disposition":"deferred","reason":"Need measurement","next_check":"Read original conditions","output":"var/projects/p/feedback/first.json"})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
fn attach_review(report: &mut Value, c: &Value) {
    let mut bundle = json!({"schema":"ontology-engineering.supplier-method-bundle/v1","policy_version":"synthetic","pattern_contract":{"identity":{"fixture":true},"question_ids":["pattern_p","pattern_q"],"patterns":[{"id":"p","method":"fixture-p"},{"id":"q","method":"fixture-q"}]}});
    let sha = canonical_digest(&bundle).unwrap();
    bundle["sha256"] = json!(sha);
    report["method_bundle"] = bundle;
    report["method_bundle_sha256"] = json!(sha);
    report["knowledge_policy_sha256"] = json!(sha);
    let result = &report["results"][0];
    let mut span = result["knowledge_units"][0]["source_span"].clone();
    span["unit_id"] = json!("u1");
    span["parent_span_sha256"] = span["sha256"].clone();
    report["review_plan"] = json!({"schema":"ontology-engineering.supplier-review-plan/v1","status":"partial","context_sha256":canonical_digest(c).unwrap(),"method_bundle_sha256":sha,"policy_sha256":sha,"adoption":"pending_agent_review","execution":"not_run","evidence_input":{"pages":[{"page_id":"book:1","source_sha256":result["source_sha256"],"text_sha256":result["text_sha256"],"spans":[span]}]},"answers":{"check":{"choice":"not_established"}},"actions":[{"id":"check","choice":"not_established","status":"candidate","adoption":"pending_agent_review","execution":"not_run"}],"selected_actions":[],"uncertain_actions":["check"],"missing_action_ids":["other"],"pattern_identity":{"fixture":true},"pattern_answers":{"pattern_p":{"choice":"not_established"}},"pattern_refs":[{"id":"p","method":"fixture-p","choice":"not_established","status":"candidate","adoption":"pending_agent_review","execution":"not_run"}],"pattern_relevance":"partial","missing_pattern_question_ids":["pattern_q"],"semantic_review":"not_run","ontology_promotion":"not_run"});
}

#[test]
fn unicode_spans_partial_budget_and_unknowns_survive_handoff() {
    let f = Fixture::new();
    let c = f.context("r1");
    let r = f.report(&c);
    let p = packet::make(&f.root, &r, &c).unwrap();
    let check = packet::verify(&f.root, &p, &c).unwrap();
    assert_eq!(check["status"], "ready_for_agent_review");
    assert_eq!(check["source_query_status"], "partial");
    assert_eq!(p["query_budget"], r["query_budget"]);
    assert_eq!(
        p["evidence_candidates"][0]["knowledge_units"][0]["source_span"]["text"],
        "😀 e\u{301}："
    );
    assert_eq!(p["semantic_review"], "not_run");
    assert_eq!(p["adoption"], "pending_agent_review");
}
#[test]
fn source_span_identity_and_caller_verdict_tampering_rejected() {
    let f = Fixture::new();
    let c = f.context("r1");
    for kind in 0..6 {
        let mut r = f.report(&c);
        match kind {
            0 => r["results"][0]["source_sha256"] = json!("0".repeat(64)),
            1 => r["results"][0]["knowledge_units"][0]["source_span"]["end"] = json!(true),
            2 => r["results"][0]["knowledge_units"][0]["source_span"]["text"] = json!("other"),
            3 => r["results"][0]["knowledge_units"][0]["context_sha256"] = json!("0".repeat(64)),
            4 => r["results"][0]["knowledge_units"][0]["applicability"] = json!("passed"),
            _ => r["parameter_verification"] = json!("passed"),
        };
        assert!(packet::make(&f.root, &r, &c).is_err());
    }
    let mut p = f.packet(&c, "var/projects/p/packet.json");
    p["evidence_candidates"][0]["engineering_verdict"] = json!("passed");
    assert!(packet::verify(&f.root, &p, &c).is_err());
}
#[test]
fn stale_changed_and_unbound_are_distinct_from_no_candidates() {
    let f = Fixture::new();
    let c = f.context("r1");
    let p = f.packet(&c, "var/projects/p/packet.json");
    let c2 = f.context("r2");
    assert_eq!(
        packet::verify(&f.root, &p, &c2).unwrap()["status"],
        "changed"
    );
    assert_eq!(
        packet::verify(&f.root, &p, &c).unwrap()["status"],
        "stale_context"
    );
    let unbound =
        context::validate(&json!({"schema":context::SCHEMA,"task_id":"concept"})).unwrap();
    let mut r = f.report(&unbound);
    r["results"] = json!([]);
    let p = packet::make(&f.root, &r, &unbound).unwrap();
    let check = packet::verify(&f.root, &p, &unbound).unwrap();
    assert_eq!(check["status"], "unbound");
    assert_eq!(check["evidence_availability"], "no_returned_candidates");
}
#[test]
fn partial_review_preserves_real_choices_without_implying_semantic_execution() {
    let f = Fixture::new();
    let c = f.context("r1");
    let mut r = f.report(&c);
    attach_review(&mut r, &c);
    let p = packet::make(&f.root, &r, &c).unwrap();
    assert_eq!(p["review_plan"], r["review_plan"]);
    assert_eq!(
        packet::verify(&f.root, &p, &c).unwrap()["review_plan_status"],
        "partial"
    );
    for fault in 0..6 {
        let mut bad = r.clone();
        match fault {
            0 => bad["review_plan"]["missing_action_ids"] = json!(["check"]),
            1 => bad["review_plan"]["status"] = json!("completed"),
            2 => bad["review_plan"]["missing_pattern_question_ids"] = json!([]),
            3 => bad["review_plan"]["pattern_refs"][0]["method"] = json!("invented"),
            4 => {
                bad["review_plan"]["evidence_input"]["pages"][0]["spans"][0]["text"] =
                    json!("outside")
            }
            _ => bad["review_plan"]["execution"] = json!("completed"),
        };
        assert!(packet::make(&f.root, &bad, &c).is_err());
    }
}
#[test]
fn bad_observation_shapes_and_budget_counters_cannot_pass() {
    let f = Fixture::new();
    let c = f.context("r1");
    for (key, value) in [
        ("elapsed_seconds", json!(true)),
        ("elapsed_seconds", json!(-1)),
        ("timing", json!({"stage_seconds":{"retrieve":-1}})),
        (
            "query_budget",
            json!({"max_requests":1,"network_requests":2,"remaining_requests":0,"denied_requests":0,"status":"exhausted"}),
        ),
        ("calls", json!({})),
        ("run_id", json!(" ")),
    ] {
        let mut r = f.report(&c);
        r[key] = value;
        assert!(packet::make(&f.root, &r, &c).is_err());
    }
}
#[test]
fn exact_adoption_feedback_is_private_append_only_and_no_fact_adoption() {
    let f = Fixture::new();
    let c = f.context("r1");
    let p = f.packet(&c, "var/projects/p/packet.json");
    let packet_bytes = fs::read(f.path.join("var/projects/p/packet.json")).unwrap();
    let source_bytes = fs::read(f.path.join("var/projects/p/source.json")).unwrap();
    let mut request = f.request(&c);
    request["disposition"] = json!("adopted");
    let record = feedback::append(&f.root, &request).unwrap();
    assert_eq!(record["packet"]["sha256"], sha256(&packet_bytes));
    assert_eq!(
        record["candidate"]["spans"][0]["source_span"],
        p["evidence_candidates"][0]["knowledge_units"][0]["source_span"]
    );
    assert_eq!(record["engineering_verdict"], "not_run");
    let written = fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap();
    assert!(feedback::append(&f.root, &request).is_err());
    assert_eq!(
        written,
        fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap()
    );
    assert_eq!(
        packet_bytes,
        fs::read(f.path.join("var/projects/p/packet.json")).unwrap()
    );
    assert_eq!(
        source_bytes,
        fs::read(f.path.join("var/projects/p/source.json")).unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.path.join("var/projects/p/feedback/first.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn explicit_history_load_is_hash_pinned_and_has_no_local_paths_in_model_view() {
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    feedback::append(&f.root, &f.request(&c)).unwrap();
    let raw = fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap();
    let reference = json!({"path":"var/projects/p/feedback/first.json","sha256":sha256(raw)});
    let loaded = feedback::load_reference(&f.root, &reference, &c).unwrap();
    assert_eq!(loaded["status"], "history_verified");
    assert_eq!(loaded["model_observation"]["historical_only"], true);
    assert!(
        !loaded["model_observation"]
            .to_string()
            .contains("var/projects")
    );
    assert_eq!(loaded["model_observation"]["semantic_review"], "not_run");
    let mut wrong = reference.clone();
    wrong["sha256"] = json!("0".repeat(64));
    assert!(feedback::load_reference(&f.root, &wrong, &c).is_err());
}
#[test]
fn new_revision_requires_new_packet_but_can_link_immutable_old_feedback() {
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    let first = feedback::append(&f.root, &f.request(&c)).unwrap();
    let prior_bytes = fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap();
    let c2 = f.context("r2");
    let mut request = f.request(&c2);
    request["output"] = json!("var/projects/p/feedback/second.json");
    request["previous"] = json!("var/projects/p/feedback/first.json");
    assert!(feedback::append(&f.root, &request).is_err());
    f.packet(&c2, "var/projects/p/packet-r2.json");
    request["packet_path"] = json!("var/projects/p/packet-r2.json");
    let second = feedback::append(&f.root, &request).unwrap();
    assert_eq!(
        second["previous_feedback"]["feedback_id"],
        first["feedback_id"]
    );
    assert_eq!(second["previous_binding_changed"], true);
    assert!(
        second["previous_changed_fields"]
            .as_array()
            .unwrap()
            .contains(&json!("object.revision"))
    );
    let loaded = feedback::load_reference(
        &f.root,
        &json!({"path":"var/projects/p/feedback/first.json","sha256":sha256(&prior_bytes)}),
        &c2,
    )
    .unwrap();
    assert_eq!(
        loaded["historical_context_source_freshness"]["status"],
        "stale"
    );
    assert_eq!(loaded["model_observation"]["historical_only"], true);
    assert_eq!(
        prior_bytes,
        fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap()
    );
}
#[test]
fn result_evidence_exact_pointer_and_file_hash_rechecked_by_history_loader() {
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    f.write(
        "var/projects/p/result.json",
        &json!({"a/b":{"observed":0.2}}),
    );
    let mut request = f.request(&c);
    request["result_evidence"] = json!(["var/projects/p/result.json#/a~1b"]);
    let record = feedback::append(&f.root, &request).unwrap();
    assert_eq!(
        record["result_evidence"][0]["value_sha256"],
        canonical_digest(&json!({"observed":0.2})).unwrap()
    );
    let reference = json!({"path":"var/projects/p/feedback/first.json","sha256":sha256(fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap())});
    feedback::load_reference(&f.root, &reference, &c).unwrap();
    f.write(
        "var/projects/p/result.json",
        &json!({"a/b":{"observed":0.3}}),
    );
    assert!(feedback::load_reference(&f.root, &reference, &c).is_err());
}
#[test]
fn cross_project_paths_and_coherently_rehashed_other_object_history_rejected() {
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    for (key, value) in [
        ("output", json!("var/projects/other/new.json")),
        ("result_evidence", json!(["var/projects/other/result.json"])),
        ("unit_ids", json!(["u1", "u1"])),
        ("disposition", json!("passed")),
        ("reason", json!(" ")),
    ] {
        let mut r = f.request(&c);
        r[key] = value;
        assert!(feedback::append(&f.root, &r).is_err());
    }
    assert!(!f.path.join("var/projects/p/feedback").exists());
    let mut prior = feedback::append(&f.root, &f.request(&c)).unwrap();
    prior["object"]["id"] = json!("other");
    prior.as_object_mut().unwrap().remove("feedback_sha256");
    prior["feedback_sha256"] = json!(canonical_digest(&prior).unwrap());
    f.write("var/projects/p/feedback/tampered.json", &prior);
    let mut r = f.request(&c);
    r["previous"] = json!("var/projects/p/feedback/tampered.json");
    r["output"] = json!("var/projects/p/feedback/later.json");
    assert!(feedback::append(&f.root, &r).is_err());
}
#[test]
fn packet_reference_tampering_rejected_even_when_feedback_self_hash_updated() {
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    let mut record = feedback::append(&f.root, &f.request(&c)).unwrap();
    record["candidate"]["spans"][0]["source_span"]["text"] = json!("different source assertion");
    record.as_object_mut().unwrap().remove("feedback_sha256");
    record["feedback_sha256"] = json!(canonical_digest(&record).unwrap());
    f.write("var/projects/p/feedback/tampered.json", &record);
    let reference = json!({"path":"var/projects/p/feedback/tampered.json","sha256":sha256(fs::read(f.path.join("var/projects/p/feedback/tampered.json")).unwrap())});
    assert!(feedback::load_reference(&f.root, &reference, &c).is_err());
}
#[cfg(unix)]
#[test]
fn packet_source_and_output_parent_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    symlink(
        f.path.join("var/projects/p"),
        f.path.join("var/projects/p/feedback"),
    )
    .unwrap();
    assert!(feedback::append(&f.root, &f.request(&c)).is_err());
    fs::remove_file(f.path.join("var/projects/p/feedback")).unwrap();
    let source = f.path.join("sources/source.pdf");
    fs::rename(&source, source.with_extension("original")).unwrap();
    symlink(source.with_extension("original"), &source).unwrap();
    assert!(
        packet::verify(
            &f.root,
            &serde_json::from_slice(&fs::read(f.path.join("var/projects/p/packet.json")).unwrap())
                .unwrap(),
            &c
        )
        .is_err()
    );
}

#[test]
fn root_relative_packet_and_feedback_move_without_reading_producer_paths() {
    let producer = Fixture::new();
    let current = producer.context("r1");
    let mut report = producer.report(&current);
    report["results"][0]["source_pdf"] = json!("sources/source.pdf");
    let packet = packet::make(&producer.root, &report, &current).unwrap();
    producer.write("var/projects/p/packet.json", &packet);
    feedback::append(&producer.root, &producer.request(&current)).unwrap();
    let receiver = Fixture::new();
    for name in ["source.json", "packet.json", "feedback/first.json"] {
        let rel = format!("var/projects/p/{name}");
        let dst = receiver.path.join(&rel);
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::copy(producer.path.join(&rel), dst).unwrap();
    }
    let reference = json!({"path":"var/projects/p/feedback/first.json","sha256":sha256(fs::read(receiver.path.join("var/projects/p/feedback/first.json")).unwrap())});
    fs::write(
        producer.path.join("sources/source.pdf"),
        b"producer changed after receiver copy",
    )
    .unwrap();
    assert!(feedback::load_reference(&producer.root, &reference, &current).is_err());
    assert_eq!(
        feedback::load_reference(&receiver.root, &reference, &current).unwrap()["status"],
        "history_verified"
    );
    let mut old = packet.clone();
    old["evidence_candidates"][0]["source"]["source_pdf"] =
        json!(producer.path.join("sources/source.pdf"));
    assert!(packet::verify(&receiver.root, &old, &current).is_err());
}

fn source_pack_report(f: &Fixture, current: &Value) -> Value {
    let raw = fs::read(f.path.join("sources/source.pdf")).unwrap();
    let source_sha = sha256(&raw);
    let manifest = json!({"schema":"ontology-engineering.misumi-source-pack/v1","index":{"path":"catalog.sqlite3","bytes":0,"sha256":"0".repeat(64)},"segments":[{"id":"s1","path":"segments/s1.tar.xz","bytes":0,"sha256":"0".repeat(64),"files":[{"path":"book/pages/1.pdf","sha256":source_sha,"bytes":raw.len()}]}],"assembled_pdfs":[{"catalog":"fixture","path":"book/output/full.pdf","sha256":"1".repeat(64),"bytes":10}]});
    let name = "sources/.indexes/fixture/source-pack.json";
    f.write(name, &manifest);
    let manifest_sha = sha256(fs::read(f.path.join(name)).unwrap());
    let source = format!("var/cache/knowledge/source-pages/{manifest_sha}/book/pages/1.pdf");
    let path = f.path.join(&source);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, raw).unwrap();
    let mut report = f.report(current);
    let result = &mut report["results"][0];
    result["source_pdf"] = json!(source);
    result["source_relative_path"] = json!("book/pages/1.pdf");
    result["source_pack_manifest"] = json!(name);
    result["source_pack_manifest_sha256"] = json!(manifest_sha);
    result["assembled_pdf"] = json!("sources/.indexes/fixture/cache/archive/book/output/full.pdf");
    result["assembled_relative_path"] = json!("book/output/full.pdf");
    result["assembled_pdf_sha256_at_index_build"] = json!("1".repeat(64));
    result["assembled_pdf_exists"] = json!(false);
    result["assembled_pdf_status"] = json!("not_downloaded");
    result["assembled_pdf_uri"] = Value::Null;
    result["pdf_uri"] = json!(format!(
        "{}#page=1",
        oe_knowledge::citations::file_uri(&path).unwrap()
    ));
    report
}

#[test]
fn source_pack_manifest_binds_cache_and_assembly_across_distinct_physical_roots() {
    let f = Fixture::new();
    let current = f.context("r1");
    let report = source_pack_report(&f, &current);
    let p = packet::make(&f.root, &report, &current).unwrap();
    let source = &p["evidence_candidates"][0]["source"];
    assert_eq!(source["assembled_pdf_status"], "not_downloaded");
    assert!(source.get("pdf_uri").is_none());
    assert!(source.get("assembled_pdf_uri").is_none());
    assert_eq!(
        source["source_pack_manifest_sha256"],
        report["results"][0]["source_pack_manifest_sha256"]
    );
    packet::verify(&f.root, &p, &current).unwrap();
    for fault in 0..3 {
        let mut bad = report.clone();
        match fault {
            0 => bad["results"][0]["source_pack_manifest_sha256"] = json!("0".repeat(64)),
            1 => bad["results"][0]["assembled_pdf_sha256_at_index_build"] = json!("2".repeat(64)),
            _ => bad["results"][0]["source_relative_path"] = json!("book/pages/2.pdf"),
        };
        assert!(packet::make(&f.root, &bad, &current).is_err());
    }
    let name = f.path.join("sources/.indexes/fixture/source-pack.json");
    let mut raw = fs::read(&name).unwrap();
    raw.push(b'\n');
    fs::write(name, raw).unwrap();
    assert!(packet::verify(&f.root, &p, &current).is_err());
}

#[test]
fn coherent_history_hash_cannot_claim_result_evidence_was_evaluated() {
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    let mut record = feedback::append(&f.root, &f.request(&c)).unwrap();
    record["result_evidence_status"] = json!("verified");
    record.as_object_mut().unwrap().remove("feedback_sha256");
    record["feedback_sha256"] = json!(canonical_digest(&record).unwrap());
    f.write("var/projects/p/feedback/tampered.json", &record);
    let reference = json!({"path":"var/projects/p/feedback/tampered.json","sha256":sha256(fs::read(f.path.join("var/projects/p/feedback/tampered.json")).unwrap())});
    assert!(feedback::load_reference(&f.root, &reference, &c).is_err());
}

#[test]
fn fixture_budget_is_preserved_without_counting_it_as_network() {
    let f = Fixture::new();
    let c = f.context("r1");
    let mut r = f.report(&c);
    r["query_budget"] = json!({"max_requests":2,"reserved_attempts":2,"network_requests":0,"fixture_requests":2,"remaining_requests":0,"denied_requests":0,"status":"exhausted","unit":"transport_attempt_including_retries"});
    let p = packet::make(&f.root, &r, &c).unwrap();
    assert_eq!(p["query_budget"]["network_requests"], 0);
    assert_eq!(p["query_budget"]["fixture_requests"], 2);
    packet::verify(&f.root, &p, &c).unwrap();
    r["query_budget"]["reserved_attempts"] = json!(1);
    assert!(packet::make(&f.root, &r, &c).is_err());
    r["query_budget"]["reserved_attempts"] = json!(2);
    r["query_budget"]["fixture_requests"] = json!(-1);
    assert!(packet::make(&f.root, &r, &c).is_err());
}

#[test]
fn explicit_current_registration_requirement_rejects_unbound_and_invalid_boolean_without_writes() {
    let f = Fixture::new();
    let c = f.context("r1");
    f.packet(&c, "var/projects/p/packet.json");
    let mut request = f.request(&c);
    request["require_current_registration"] = json!("true");
    assert!(
        feedback::append(&f.root, &request)
            .unwrap_err()
            .to_string()
            .contains("registration_requirement_invalid")
    );
    request["require_current_registration"] = json!(true);
    assert!(
        feedback::append(&f.root, &request)
            .unwrap_err()
            .to_string()
            .contains("current_source_registration_required")
    );
    assert!(!f.path.join("var/projects/p/feedback/first.json").exists());
    request["require_current_registration"] = json!(false);
    let value = feedback::append(&f.root, &request).unwrap();
    assert_eq!(value["require_current_registration"], false);
    assert_ne!(value["source_registration_freshness"]["status"], "current");
}
