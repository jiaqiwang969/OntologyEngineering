//! Original synthetic Python boundary cases captured as immutable data. The
//! executable tests have no Python interpreter or semantic-engine dependency.
use oe_cad::{evidence, handoff};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    packet: PathBuf,
}
impl Fixture {
    fn new(record: &Value) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("var/maintenance/rust-rewrite-20260927/phase12-cad-evidence-rust-01/test-temp")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.parent().unwrap()).unwrap();
        fs::create_dir(&root).unwrap();
        for (name, text) in record["files"].as_object().unwrap() {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let packet = root.join(record["packet_path"].as_str().unwrap());
        Self { root, packet }
    }
    fn write(&self, p: &Value) {
        fs::write(&self.packet, serde_json::to_vec(p).unwrap()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/cad-evidence-legacy-v1.json")).unwrap()
}
fn first(kind: &str) -> (Fixture, Value) {
    let data = oracle();
    let r = data["verification"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == kind && r["valid"] == true)
        .unwrap();
    (Fixture::new(r), r["packet"].clone())
}
#[test]
fn original_verification_boundaries_and_exact_sorted_abox_bytes() {
    let data = oracle();
    let mut good = 0;
    let mut rejected = 0;
    for (i, r) in data["verification"].as_array().unwrap().iter().enumerate() {
        let f = Fixture::new(r);
        let result = if r["kind"] == "cad" {
            evidence::verify(&f.packet, &f.root).map(|v| (v.audit().clone(), v.project_abox()))
        } else {
            handoff::verify(&f.packet, &f.root).map(|v| (v.audit().clone(), v.project_abox()))
        };
        if r["valid"] == true {
            let (audit, abox) = result.unwrap_or_else(|err| panic!("legacy case {i}: {err:#}"));
            assert_eq!(audit["packet_sha256"], r["packet_sha256"], "case {i}");
            assert_eq!(audit["entity_count"], r["entity_count"], "case {i}");
            assert_eq!(audit["semantic_execution"], "not_run");
            assert_eq!(audit["engineering_verdict"], "not_assessed");
            assert_eq!(
                String::from_utf8(abox).unwrap(),
                r["abox"].as_str().unwrap(),
                "case {i}"
            );
            good += 1;
        } else {
            assert!(
                result.is_err(),
                "legacy rejection {i} unexpectedly accepted: {}",
                r["error"]
            );
            rejected += 1;
        }
    }
    assert!(good >= 5 && rejected >= 15);
}
#[test]
fn original_dependency_impact_matches_exact_json() {
    for r in oracle()["comparison"].as_array().unwrap() {
        let out = if r["kind"] == "cad" {
            evidence::compare_packets(&r["previous"], &r["current"])
        } else {
            handoff::compare_packets(&r["previous"], &r["current"])
        }
        .unwrap();
        assert_eq!(out, r["impact"]);
    }
}
#[test]
fn closed_schema_unknown_fields_and_duplicate_json_keys_fail() {
    let (f, p) = first("cad");
    let mut bad = p.clone();
    bad["made_up_authority"] = json!(true);
    f.write(&bad);
    assert!(evidence::verify(&f.packet, &f.root).is_err());
    let mut bad = p.clone();
    bad["objects"][0]["status"] = json!("approved");
    f.write(&bad);
    assert!(evidence::verify(&f.packet, &f.root).is_err());
    let mut bad = p.clone();
    bad["assertions"][0]["source_ids"] = json!(["readback", "readback"]);
    f.write(&bad);
    assert!(evidence::verify(&f.packet, &f.root).is_err());
    let raw = serde_json::to_string(&p).unwrap();
    fs::write(
        &f.packet,
        raw.replacen('{', "{\"packet_id\":\"shadow\",", 1),
    )
    .unwrap();
    assert!(evidence::verify(&f.packet, &f.root).is_err());
}
#[test]
fn explicit_unknown_analysis_input_must_remain_null_and_measured_requires_measurement() {
    let data = oracle();
    let record = data["verification"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["valid"] == true
                && r["packet"]["sources"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|s| s["role"] == "analysis_record"))
        })
        .unwrap();
    let f = Fixture::new(record);
    let mut p = record["packet"].clone();
    let source = p["sources"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|s| s["role"] == "analysis_record")
        .unwrap();
    let path = f.root.join(source["path"].as_str().unwrap());
    let mut analysis: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    analysis["inputs"][0]["provenance"] = json!("unknown");
    analysis["inputs"][0]["value"] = Value::Null;
    let bytes = serde_json::to_vec(&analysis).unwrap();
    source["sha256"] = json!(oe_wire::sha256(&bytes));
    fs::write(&path, bytes).unwrap();
    f.write(&p);
    evidence::verify(&f.packet, &f.root).unwrap();
    analysis["inputs"][0]["provenance"] = json!("measured");
    let bytes = serde_json::to_vec(&analysis).unwrap();
    p["sources"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|s| s["role"] == "analysis_record")
        .unwrap()["sha256"] = json!(oe_wire::sha256(&bytes));
    fs::write(path, bytes).unwrap();
    f.write(&p);
    assert!(evidence::verify(&f.packet, &f.root).is_err());
}
#[test]
fn negative_and_invalid_json_pointers_and_path_aliases_fail_closed() {
    let (f, p) = first("cad");
    for locator in ["/occurrences/-1", "/occurrences/00", "/occurrences/~2"] {
        let mut bad = p.clone();
        bad["objects"][0]["source_locator"] = json!(locator);
        f.write(&bad);
        assert!(evidence::verify(&f.packet, &f.root).is_err());
    }
    for path in [
        "../assembly.step",
        "./assembly.step",
        "sub/../assembly.step",
        "sub//assembly.step",
        "C:\\assembly.step",
    ] {
        let mut bad = p.clone();
        bad["sources"][0]["path"] = json!(path);
        f.write(&bad);
        assert!(evidence::verify(&f.packet, &f.root).is_err());
    }
}
#[cfg(unix)]
#[test]
fn source_symlinks_rejected_in_both_adapters() {
    for kind in ["cad", "handoff"] {
        let (f, p) = first(kind);
        let source = p["sources"][0]["path"].as_str().unwrap();
        let path = f.root.join(source);
        let moved = f.root.join("original-source");
        fs::rename(&path, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &path).unwrap();
        if kind == "cad" {
            assert!(evidence::verify(&f.packet, &f.root).is_err());
        } else {
            assert!(handoff::verify(&f.packet, &f.root).is_err());
        }
    }
}
fn q21_fixture() -> (Fixture, Value) {
    let (f, mut p) = first("handoff");
    // This fixture may be the CAD-link case; use the standalone process case.
    if p["claims"].as_array().unwrap().is_empty() {
        drop(f);
        let data = oracle();
        let r = data["verification"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| {
                r["kind"] == "handoff"
                    && r["valid"] == true
                    && !r["packet"]["claims"].as_array().unwrap().is_empty()
            })
            .unwrap();
        let f = Fixture::new(r);
        p = r["packet"].clone();
        return q21_ready(f, p);
    }
    q21_ready(f, p)
}
fn q21_ready(f: Fixture, mut p: Value) -> (Fixture, Value) {
    p["coverage_assertions"] = json!([]);
    p["routes"].as_array_mut().unwrap().push(json!({"id":"baseline","name":"old route","status":"under_review","source_ids":["readback"],"operations":[]}));
    p["claims"][0]["baseline_route_id"] = json!("baseline");
    p["claims"][0]["target_route_id"] = json!("laser");
    f.write(&p);
    (f, p)
}
#[test]
fn q21_absent_completeness_and_coverage_stay_absent() {
    let (f, _) = q21_fixture();
    let v = handoff::verify(&f.packet, &f.root).unwrap();
    let q = v.project_q21("deliver").unwrap();
    let nt = String::from_utf8(q.abox).unwrap();
    assert!(!nt.contains("completenessEvidence"));
    assert!(!nt.contains("<urn:mfg:loop:FunctionCoverage>"));
    assert_eq!(q.focus, "urn:mfg:loop:demo.rev_a.A.nominal.claim.deliver");
    assert_eq!(q.audit["package_state"], "proposed_not_promoted");
    assert_eq!(q.audit["semantic_execution"], "not_run");
    for direction in [
        handoff::QuestionDirection::CadToProcess,
        handoff::QuestionDirection::ProcessToCad,
    ] {
        assert_eq!(v.requests(direction)["engineering_verdict"], "not_assessed");
    }
}
#[test]
fn q21_candidate_coverage_is_not_verified() {
    let (f, mut p) = q21_fixture();
    p["routes"][0]["operations"] = json!([{"id":"weld","process_type":"laser_welding","target_feature_ids":["hole_1"],"intended_function_ids":["seal_1"],"mechanism_statement":"candidate","source_ids":["readback"],"validation_source_ids":[]}]);
    p["coverage_assertions"] = json!([{"id":"cov","claim_id":"deliver","function_id":"seal_1","operation_id":"weld","status":"proposed","mechanism_statement":"unverified coverage","source_ids":["readback"],"validation_source_ids":[],"scope_checked_source_ids":[]}]);
    f.write(&p);
    let q = handoff::verify(&f.packet, &f.root)
        .unwrap()
        .project_q21("deliver")
        .unwrap();
    let nt = String::from_utf8(q.abox).unwrap();
    assert!(nt.contains("<urn:mfg:loop:status> \"proposed\""));
    assert!(!nt.contains("validatedBy"));
    assert!(!nt.contains("\"verified\""));
    p["coverage_assertions"][0]["status"] = json!("verified");
    f.write(&p);
    assert!(handoff::verify(&f.packet, &f.root).is_err());
}
#[test]
fn q21_requires_explicit_routes_and_required_function() {
    let (f, mut p) = q21_fixture();
    p["claims"][0]
        .as_object_mut()
        .unwrap()
        .remove("baseline_route_id");
    f.write(&p);
    assert!(
        handoff::verify(&f.packet, &f.root)
            .unwrap()
            .project_q21("deliver")
            .is_err()
    );
    p["claims"][0]["baseline_route_id"] = json!("baseline");
    p["functions"][0]["status"] = json!("candidate");
    f.write(&p);
    assert!(
        handoff::verify(&f.packet, &f.root)
            .unwrap()
            .project_q21("deliver")
            .is_err()
    );
}
#[test]
fn identity_changes_and_removed_dependencies_propagate_without_a_verdict() {
    let (_, p) = first("cad");
    let mut changed = p.clone();
    changed["project"]["model_revision"] = json!("B");
    let out = evidence::compare_packets(&p, &changed).unwrap();
    assert_eq!(out["identity_scope_changed"], true);
    assert_eq!(out["affected_assertion_ids"], json!(["fit_candidate"]));
    let (f, p) = q21_fixture();
    let mut changed = p.clone();
    changed["features"][0]["attributes"]["x_mm"] = json!(9);
    let out = handoff::compare_packets(&p, &changed).unwrap();
    assert_eq!(out["affected_output_ids"], json!(["quote"]));
    assert_eq!(
        out["engineering_verdict"],
        "re_review_required_not_assessed"
    );
    drop(f);
}

#[test]
fn embedded_json_literals_sort_keys_and_reuse_python_number_formatting() {
    let (f, mut p) = first("cad");
    let mut readback: Value =
        serde_json::from_slice(&fs::read(f.root.join("readback.json")).unwrap()).unwrap();
    let expected =
        oe_wire::json::strict_json(br#"{"z":1e-6,"a":[1e20,"quoted \\\" : ,",{"y":2,"b":1}]}"#)
            .unwrap();
    readback["occurrences"][0] = expected.clone();
    let bytes = serde_json::to_vec(&readback).unwrap();
    fs::write(f.root.join("readback.json"), &bytes).unwrap();
    p["sources"][1]["sha256"] = json!(oe_wire::sha256(&bytes));
    p["objects"][0]["expected"] = expected;
    f.write(&p);
    let nt =
        String::from_utf8(evidence::verify(&f.packet, &f.root).unwrap().project_abox()).unwrap();
    let line = nt
        .lines()
        .find(|line| {
            line.starts_with(
                "<urn:ontology-engineering:project:demo:assembly_a:A:nominal:cad_object:plate_occ>",
            ) && line.contains("expectedJson")
        })
        .unwrap();
    let literal = line
        .split("expectedJson> ")
        .nth(1)
        .unwrap()
        .strip_suffix(" .")
        .unwrap();
    let value: String = serde_json::from_str(literal).unwrap();
    assert!(value.starts_with("{\"a\": [1e+20,"));
    assert!(value.ends_with("\"z\": 1e-06}"));
    assert!(value.contains("{\"b\": 1, \"y\": 2}"));
}
#[test]
fn native_number_equivalence_preserves_precision_and_rejects_boolean_coercion() {
    let (f, mut p) = first("cad");
    for (actual, expected, valid) in [
        ("1", "1.0", true),
        ("9007199254740993", "9007199254740992.0", false),
        ("9007199254740992", "9007199254740992.0", true),
        ("true", "1", false),
    ] {
        let mut readback: Value =
            serde_json::from_slice(&fs::read(f.root.join("readback.json")).unwrap()).unwrap();
        readback["occurrences"][0] =
            json!({"value":oe_wire::json::strict_json(actual.as_bytes()).unwrap()});
        let bytes = serde_json::to_vec(&readback).unwrap();
        fs::write(f.root.join("readback.json"), &bytes).unwrap();
        p["sources"][1]["sha256"] = json!(oe_wire::sha256(&bytes));
        p["objects"][0]["expected"] =
            json!({"value":oe_wire::json::strict_json(expected.as_bytes()).unwrap()});
        f.write(&p);
        assert_eq!(
            evidence::verify(&f.packet, &f.root).is_ok(),
            valid,
            "{actual} / {expected}"
        );
    }
}
#[test]
fn duplicate_cross_kind_ids_and_indirect_assertion_cycles_are_rejected() {
    let (f, p) = first("cad");
    let mut bad = p.clone();
    bad["objects"][0]["id"] = json!("model");
    f.write(&bad);
    assert!(evidence::verify(&f.packet, &f.root).is_err());
    let mut bad = p.clone();
    let mut extra = bad["assertions"][0].clone();
    extra["id"] = json!("other_claim");
    extra["depends_on_ids"] = json!(["fit_candidate"]);
    bad["assertions"][0]["depends_on_ids"] = json!(["other_claim"]);
    bad["assertions"].as_array_mut().unwrap().push(extra);
    f.write(&bad);
    assert!(evidence::verify(&f.packet, &f.root).is_err());
}
#[test]
fn verified_snapshot_cannot_mutate_its_captured_packet_through_public_accessors() {
    let (f, p) = first("cad");
    let verified = evidence::verify(&f.packet, &f.root).unwrap();
    let before = verified.project_abox();
    let mut edited = verified.packet().clone();
    edited["project"]["model_revision"] = json!("changed");
    f.write(&edited);
    assert_eq!(verified.packet(), &p);
    assert_eq!(verified.project_abox(), before);
    assert_ne!(
        verified.audit()["packet_sha256"],
        json!(oe_wire::sha256(fs::read(&f.packet).unwrap()))
    );
}

#[test]
fn q21_legacy_views_change_only_to_state_bound_identity() {
    for r in oracle()["q21"].as_array().unwrap() {
        let f = Fixture::new(r);
        let v = handoff::verify(&f.packet, &f.root).unwrap();
        let q = v.project_q21(r["claim_id"].as_str().unwrap()).unwrap();
        // Preserve the original oracle bytes. This is an intentional identity
        // migration; the source assertions and proposed vocabulary are unchanged.
        let old = "urn:mfg:loop:demo.rev_a.A.";
        let new = "urn:mfg:loop:demo.rev_a.A.nominal.";
        assert!(r["focus"].as_str().unwrap().starts_with(old));
        assert_eq!(
            String::from_utf8(q.abox).unwrap(),
            r["abox"].as_str().unwrap().replace(old, new)
        );
        assert_eq!(q.focus, r["focus"].as_str().unwrap().replace(old, new));
        assert_eq!(q.audit["packet_sha256"], r["packet_sha256"]);
        assert_eq!(
            q.audit["identity_scheme"],
            "project-configuration-revision-state/dot-escaped-v2"
        );
    }
}

#[test]
fn q21_component_boundaries_and_product_states_have_distinct_identities() {
    let (f, original) = q21_fixture();
    let cases = [
        (["a.b", "c", "A", "nominal"], "a%2Eb.c.A.nominal"),
        (["a", "b.c", "A", "nominal"], "a.b%2Ec.A.nominal"),
        (["a.b", "c", "A", "as_built"], "a%2Eb.c.A.as_built"),
        (["a", "b", "c.d", "e"], "a.b.c%2Ed.e"),
        (["a", "b", "c", "d.e"], "a.b.c.d%2Ee"),
    ];
    let mut identities = std::collections::BTreeSet::new();
    for (identity, expected) in cases {
        let mut packet = original.clone();
        for (field, value) in [
            "project_id",
            "configuration_id",
            "model_revision",
            "product_state",
        ]
        .into_iter()
        .zip(identity)
        {
            packet["project"][field] = json!(value);
        }
        f.write(&packet);
        let q = handoff::verify(&f.packet, &f.root)
            .unwrap()
            .project_q21("deliver")
            .unwrap();
        assert_eq!(q.focus, format!("urn:mfg:loop:{expected}.claim.deliver"));
        assert!(identities.insert(q.focus));
        let abox = String::from_utf8(q.abox).unwrap();
        assert!(abox.contains(&format!("<urn:mfg:loop:{expected}.snapshot.")));
        assert_eq!(q.audit["semantic_execution"], "not_run");
    }
}
