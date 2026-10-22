//! Independent current-Project cases; actual native fixtures remain private.
use anyhow::{Context, Result, ensure};
use oe_contracts::{EngineeringAssertion, ObjectIdentity};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oe_kernel::{engineering, project::Project};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn fixture() -> (Project, ObjectIdentity, EngineeringAssertion) {
    let object: ObjectIdentity = serde_json::from_value(json!({"object_id":"part","revision":"r1","condition_scope":"service","source_sha256":"a".repeat(64)})).unwrap();
    let a: EngineeringAssertion = serde_json::from_value(json!({"assertion_id":"clear","kind":"cad_native_motion","subject":"part",
        "requirements":{"schema":"oev2.cad-native-motion-assertion/1","object":object,"receive_request":{"path":"var/missing/request.json","sha256":"b".repeat(64)},"request_ref":"request","claim":"sampled_no_interference","applicability":"append-only-project/1"},"source_refs":["request"],"affected_decisions":["design"]})).unwrap();
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"part","object_revision":"r1","purpose":"check all required conditions","acceptance":["necessary manufacturing check"],"facts":[],"assertions":[a]});
    let p: Project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"test","revision":"r2","purpose":"check all required conditions","objects":{"part":"r1"},"conditions":{"service":basis.to_string()},"evidence":{"part":"a".repeat(64),"request":"b".repeat(64)},
        "decisions":[{"id":"design","object_id":"part","object_revision":"r1","condition_scope":"service","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["clear","request"]},{"id":"manufacture","object_id":"part","object_revision":"r1","condition_scope":"service","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["design"]}],"dependencies":{"design":["request","clear"],"manufacture":["design"]}})).unwrap();
    (p, object, a)
}

#[test]
fn memory_only_declaration_never_supplies_native_proof() {
    let (p, o, a) = fixture();
    let r = oe_kernel::motion::declared(&p, &o, &a);
    assert_eq!(r.conclusion, "unknown");
    assert_eq!(
        r.failures,
        ["native_motion_original_sources_not_recomputed"]
    );
    assert_eq!(
        r.result["details"]["affected_decisions"],
        json!(["design", "manufacture"])
    );
    assert!(!p.decisions.iter().any(|d| d.adopted));
}

#[test]
fn missing_original_cannot_be_replaced_by_asserted_success() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("var")).unwrap();
    fs::write(
        dir.path().join("SKILL.md"),
        "# isolated native assertion fixture\n",
    )
    .unwrap();
    let root = SkillRoot::new(dir.path().canonicalize().unwrap()).unwrap();
    let (p, o, a) = fixture();
    let r = oe_kernel::motion::current(&root, &p, &o, &a);
    assert_eq!(r.conclusion, "unknown");
    let checks = engineering::checks_current(&root, &p, &o);
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.id == "engineering:clear")
            .count(),
        1
    );
    assert_eq!(
        checks
            .iter()
            .find(|c| c.id == "engineering:clear")
            .unwrap()
            .conclusion,
        "unknown"
    );
}

#[test]
fn unrecorded_or_source_unbound_claims_do_not_enter_current_review() {
    let (p, o, mut a) = fixture();
    a.requirements["claim"] = json!("discrete_pair_checks");
    assert_eq!(
        oe_kernel::motion::declared(&p, &o, &a).failures,
        ["native_motion_assertion_not_in_current_project"]
    );
    let (mut p, o, a) = fixture();
    p.dependencies.remove("design");
    p.decisions[0].basis.retain(|id| id != "request");
    assert_eq!(
        oe_kernel::motion::declared(&p, &o, &a).failures,
        ["native_motion_dependency_missing"]
    );
}

fn pinned(root: &SkillRoot, r: &Value) -> Result<Vec<u8>> {
    let p = root.private_path(Path::new(r["path"].as_str().context("fixture path")?))?;
    let bytes = read_regular_bytes(&p, 16 * 1024 * 1024)?;
    ensure!(
        r["bytes"].as_u64() == Some(bytes.len() as u64) && r["sha256"] == sha256(&bytes),
        "fixture identity differs"
    );
    Ok(bytes)
}

#[test]
#[ignore = "requires explicitly frozen private original native Project cases"]
fn independently_frozen_current_native_cases() -> Result<()> {
    let root = SkillRoot::new(std::env::var("OEV2_NATIVE_ASSERTION_ROOT")?)?;
    let path = root.private_path(Path::new(&std::env::var("OEV2_NATIVE_ASSERTION_CASES")?))?;
    let raw = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&raw) == std::env::var("OEV2_NATIVE_ASSERTION_SHA256")?,
        "fixture manifest changed"
    );
    let manifest = oe_core::json::strict_json(&raw)?;
    let output =
        root.new_private_dir(Path::new(&std::env::var("OEV2_NATIVE_ASSERTION_OUTPUT")?))?;
    let object: ObjectIdentity = serde_json::from_value(manifest["object"].clone())?;
    let mut rows = vec![];
    let mut failures = vec![];
    for case in manifest["cases"].as_array().context("cases missing")? {
        let p_raw = pinned(&root, &case["project"])?;
        let project: Project = serde_json::from_value(oe_core::json::strict_json(&p_raw)?)?;
        let before = serde_json::to_value(&project)?;
        // Expected conclusions are loaded only after these real owner calls.
        let pure = engineering::checks(&project, &object);
        let actual = engineering::checks_current(&root, &project, &object);
        let expected = oe_core::json::strict_json(&pinned(&root, &case["expected"])?)?;
        let mut matched = serde_json::to_value(&project)? == before;
        for wanted in expected.as_array().context("expectations missing")? {
            let found = actual
                .iter()
                .find(|r| r.id == wanted["id"].as_str().unwrap_or(""));
            matched &=
                found.is_some_and(|r| r.conclusion == wanted["conclusion"].as_str().unwrap_or(""));
            if let Some(f) = found {
                for pointer in wanted["details"].as_array().into_iter().flatten() {
                    matched &= f.result.pointer(pointer["pointer"].as_str().unwrap_or(""))
                        == pointer.get("value");
                }
            }
        }
        matched &= pure
            .iter()
            .filter(|r| r.id.starts_with("engineering:motion36-"))
            .all(|r| r.conclusion == "unknown");
        if !matched {
            failures.push(case["id"].clone());
        }
        let row = json!({"case":case["id"],"actual":actual,"pure":pure,"matched":matched,"project_unchanged":serde_json::to_value(&project)?==before,"native_dispatches":0});
        let file = output.join(format!("{:03}.json", rows.len()));
        fs::write(&file, serde_json::to_vec_pretty(&row)?)?;
        rows.push(json!({"case":case["id"],"actual":file,"sha256":sha256(fs::read(&file)?),"matched":matched}));
        ensure!(
            pinned(&root, &case["project"])? == p_raw,
            "fixture project changed during check"
        );
    }
    fs::write(
        output.join("SUMMARY.json"),
        serde_json::to_vec_pretty(
            &json!({"cases":rows,"failures":failures,"native_dispatches":0,"engineering_adoption":"not_granted"}),
        )?,
    )?;
    ensure!(
        read_regular_bytes(&path, 16 * 1024 * 1024)? == raw,
        "fixture manifest changed after execution"
    );
    ensure!(
        failures.is_empty(),
        "independent expectations did not match: {failures:?}"
    );
    Ok(())
}

#[test]
fn a_claim_failure_reopens_its_dependents_without_declaring_the_object_changed() {
    let (mut p, o, a) = fixture();
    let mut peer = p.decisions[0].clone();
    peer.id = "observed-samples".into();
    peer.basis = vec!["request".into()];
    p.decisions.push(peer);
    let r = oe_kernel::motion::declared(&p, &o, &a);
    assert_eq!(
        r.result["details"]["affected_decisions"],
        json!(["design", "manufacture"])
    );
    let source_changed =
        oe_kernel::project::impact(&p, &std::collections::BTreeSet::from(["request".into()]))
            .unwrap();
    assert!(
        source_changed["affected"]
            .as_array()
            .unwrap()
            .contains(&json!("observed-samples"))
    );
}
