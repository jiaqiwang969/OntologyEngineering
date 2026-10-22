use oe_core::{SkillRoot, sha256, write_new_bytes};
use oe_kernel::governance::{Event, transition};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: SkillRoot,
    candidate: String,
    binding: String,
    evidence: String,
}
impl Fixture {
    fn new(receipt: Value) -> Self {
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let actual = SkillRoot::new(original).unwrap();
        let path = actual
            .new_private_dir(Path::new(&format!(
                "var/maintenance/governance-tests/{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            )))
            .unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            "isolated package governance verification",
        )
        .unwrap();
        std::fs::create_dir_all(path.join("runtime/vendor")).unwrap();
        std::fs::copy(
            original.join("runtime/semantic-bundles.json"),
            path.join("runtime/semantic-bundles.json"),
        )
        .unwrap();
        std::fs::copy(
            original.join("runtime/semantica-source-lock.json"),
            path.join("runtime/semantica-source-lock.json"),
        )
        .unwrap();
        for file in [
            "engineering-judgment-intake-0.2.2.sem-package.zip",
            "engineering-evidence-methods-0.1.0.sem-package.zip",
            "manufacturing-process-cost-0.1.1.sem-package.zip",
        ] {
            std::fs::copy(
                original.join("runtime/vendor").join(file),
                path.join("runtime/vendor").join(file),
            )
            .unwrap();
        }
        let root = SkillRoot::new(path).unwrap();
        let bundle = oe_semantic::load_bundle(root.path(), "engineering-judgment-intake").unwrap();
        let candidate = bundle.spec["sha256"].as_str().unwrap().to_owned();
        let binding=sha256(oe_core::json::canonical_json(&json!({"package_id":bundle.spec["package_id"],"package_version":bundle.spec["package_version"],"archive_sha256":bundle.spec["sha256"]})).unwrap());
        let bytes = oe_core::json::canonical_json(&receipt).unwrap();
        let evidence = sha256(&bytes);
        std::fs::create_dir_all(root.path().join("var/ledger/evidence")).unwrap();
        write_new_bytes(
            &root
                .path()
                .join(format!("var/ledger/evidence/{evidence}.json")),
            &bytes,
        )
        .unwrap();
        std::fs::create_dir_all(root.path().join("var/state")).unwrap();
        let policy = json!({"schema":"oev2.governance-policy/1","ledger":"var/ledger","bundle":"engineering-judgment-intake","candidate_sha256":candidate,"package_binding_sha256":binding,"actor_scopes":{"author":["proposed","committed"],"independent_verifier":["regression_passed","release_complete"],"decision_owner":["promoted"]},"required_evidence":{"proposed":[evidence],"committed":[evidence],"regression_passed":[evidence]},"release_artifacts":[]});
        write_new_bytes(
            &root.path().join("var/state/governance-authority.json"),
            &oe_core::json::canonical_json(&policy).unwrap(),
        )
        .unwrap();
        Self {
            root,
            candidate,
            binding,
            evidence,
        }
    }
    fn event(&self, from: &str, to: &str, actor: &str, previous: Option<String>) -> Event {
        serde_json::from_value(json!({"candidate_sha256":self.candidate,"previous_sha256":previous,"from":from,"to":to,"actor":actor,"authority_scope":[to],"evidence_sha256":[self.evidence],"package_binding_sha256":self.binding,"event_sha256":""})).unwrap()
    }
    fn move_to(&self, from: &str, to: &str, actor: &str, previous: Option<String>) -> Value {
        transition(
            &self.root,
            Path::new("var/ledger"),
            self.event(from, to, actor, previous),
        )
        .unwrap()
    }
}
#[test]
fn self_asserted_authority_is_not_a_trusted_grant() {
    let f = Fixture::new(json!({"role":"source_bound_proposal"}));
    assert!(
        transition(
            &f.root,
            Path::new("var/ledger"),
            f.event("candidate", "proposed", "unknown_actor", None)
        )
        .is_err()
    );
}
#[test]
fn serialized_pass_cannot_replace_actual_regression() {
    let f = Fixture::new(json!({"engine_id":oe_kernel::ENGINE_ID,"status":"passed"}));
    let first = f.move_to("candidate", "proposed", "author", None);
    let next = f.move_to(
        "proposed",
        "committed",
        "author",
        Some(first["event"]["event_sha256"].as_str().unwrap().into()),
    );
    assert!(
        transition(
            &f.root,
            Path::new("var/ledger"),
            f.event(
                "committed",
                "regression_passed",
                "independent_verifier",
                Some(next["event"]["event_sha256"].as_str().unwrap().into())
            )
        )
        .is_err()
    );
}
#[test]
fn skips_duplicate_successors_and_changed_authority_baselines_are_rejected() {
    let f = Fixture::new(json!({"role":"source_bound_proposal"}));
    assert!(
        transition(
            &f.root,
            Path::new("var/ledger"),
            f.event("candidate", "committed", "author", None)
        )
        .is_err()
    );
    let first = f.move_to("candidate", "proposed", "author", None);
    let previous = first["event"]["event_sha256"].as_str().unwrap().to_owned();
    let policy_path = f.root.path().join("var/state/governance-authority.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&policy_path).unwrap()).unwrap();
    v["actor_scopes"]["author"] = json!(["proposed", "committed", "promoted"]);
    std::fs::write(policy_path, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(
        transition(
            &f.root,
            Path::new("var/ledger"),
            f.event("proposed", "committed", "author", Some(previous))
        )
        .is_err()
    );
}

#[test]
fn actual_package_regression_allows_a_bound_successor_but_not_incomplete_release() {
    let mut f = Fixture::new(json!({"role":"initial_fixture_only"}));
    let receipt =
        oe_kernel::packages::regress(f.root.path(), "engineering-judgment-intake").unwrap();
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["scenario_count"], 119);
    let bytes = oe_core::json::canonical_json(&receipt).unwrap();
    f.evidence = sha256(&bytes);
    write_new_bytes(
        &f.root
            .path()
            .join(format!("var/ledger/evidence/{}.json", f.evidence)),
        &bytes,
    )
    .unwrap();
    let p = f.root.path().join("var/state/governance-authority.json");
    let mut policy: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    for stage in [
        "proposed",
        "committed",
        "regression_passed",
        "release_complete",
    ] {
        policy["required_evidence"][stage] = json!([f.evidence]);
    }
    std::fs::write(p, oe_core::json::canonical_json(&policy).unwrap()).unwrap();
    let first = f.move_to("candidate", "proposed", "author", None);
    let next = f.move_to(
        "proposed",
        "committed",
        "author",
        Some(first["event"]["event_sha256"].as_str().unwrap().into()),
    );
    let parent = next["event"]["event_sha256"].as_str().unwrap().to_owned();
    let verified = f.move_to(
        "committed",
        "regression_passed",
        "independent_verifier",
        Some(parent.clone()),
    );
    assert_eq!(verified["gate"]["execution"], "completed");
    assert!(
        transition(
            &f.root,
            Path::new("var/ledger"),
            f.event(
                "committed",
                "regression_passed",
                "independent_verifier",
                Some(parent)
            )
        )
        .is_err()
    );
    let last = verified["event"]["event_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        transition(
            &f.root,
            Path::new("var/ledger"),
            f.event(
                "regression_passed",
                "release_complete",
                "independent_verifier",
                Some(last)
            )
        )
        .unwrap_err()
        .to_string()
        .contains("release_inventory_incomplete")
    );
    assert!(
        transition(
            &f.root,
            Path::new("var/ledger"),
            f.event("promoted", "published", "decision_owner", None)
        )
        .unwrap_err()
        .to_string()
        .contains("external_publication_verification_not_implemented")
    );
}
