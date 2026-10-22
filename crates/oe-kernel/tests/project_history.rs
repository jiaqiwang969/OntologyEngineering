use oe_kernel::project::{Project, reconcile, validate};
use serde_json::Value;
use std::path::Path;

#[test]
fn unchanged_and_single_factor_changes_match_frozen_independent_oracles() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/v2-project/history-cases");
    let oracles: Value =
        serde_json::from_slice(&std::fs::read(root.join("oracles.json")).unwrap()).unwrap();
    let mut mismatches = Vec::new();
    for case in oracles["cases"].as_array().unwrap() {
        let label = case["label"].as_str().unwrap();
        let load = |suffix: &str| -> Project {
            serde_json::from_slice(
                &std::fs::read(root.join(format!("{label}.{suffix}.json"))).unwrap(),
            )
            .unwrap()
        };
        let before = load("before");
        let after = load("after");
        let old_bytes = serde_json::to_vec(&before).unwrap();
        let result = reconcile(&before, &after).unwrap();
        let mut review: Vec<_> = result["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["decision_id"].as_str().unwrap())
            .collect();
        let mut stale: Vec<_> = result["stale_adoptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d.as_str().unwrap())
            .collect();
        review.sort();
        stale.sort();
        if serde_json::to_value(&review).unwrap() != case["expected_requires_review"]
            || serde_json::to_value(&stale).unwrap() != case["expected_stale_adoptions"]
        {
            mismatches.push(format!("{label}: review={review:?}; stale={stale:?}"));
        }
        assert_eq!(
            serde_json::to_vec(&before).unwrap(),
            old_bytes,
            "history must remain immutable"
        );
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn current_unknown_does_not_replace_the_historical_conclusion() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/v2-project/history-cases");
    let load = |suffix: &str| -> Project {
        serde_json::from_slice(
            &std::fs::read(root.join(format!("all-changes-pending-review.{suffix}.json"))).unwrap(),
        )
        .unwrap()
    };
    let result = reconcile(&load("before"), &load("after")).unwrap();
    let decision = result["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["decision_id"] == "adopt-bracket")
        .unwrap();
    assert_eq!(decision["prior_conclusion"], "supported");
    assert_eq!(decision["current_conclusion"], "unknown");
}

#[test]
fn incomplete_identity_and_malformed_evidence_are_rejected() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/v2-project/history-cases");
    let baseline: Project = serde_json::from_slice(
        &std::fs::read(root.join("unchanged-stable-source-ids.before.json")).unwrap(),
    )
    .unwrap();
    assert!(validate(&baseline).is_ok());
    type Fault = (&'static str, fn(&mut Project));
    let faults: [Fault; 9] = [
        ("missing project revision", |p| p.revision.clear()),
        ("missing object revision", |p| {
            p.objects
                .insert("installation-bracket".into(), String::new());
        }),
        ("empty condition", |p| {
            p.conditions
                .insert("operating-temperature".into(), " ".into());
        }),
        ("invalid evidence digest", |p| {
            p.evidence
                .insert("supplier-measurement-report".into(), "not-a-sha256".into());
        }),
        ("missing decision identity", |p| p.decisions[0].id.clear()),
        ("missing decision revision", |p| {
            p.decisions[0].object_revision.clear()
        }),
        ("missing decision scope", |p| {
            p.decisions[0].condition_scope.clear()
        }),
        ("invalid decision digest", |p| {
            p.decisions[0].source_sha256 = "a".repeat(63)
        }),
        ("empty adoption basis", |p| {
            p.decisions[0].basis = vec![String::new()]
        }),
    ];
    let mut accepted = Vec::new();
    for (name, change) in faults {
        let mut p = baseline.clone();
        change(&mut p);
        if validate(&p).is_ok() {
            accepted.push(name);
        }
    }
    assert!(
        accepted.is_empty(),
        "invalid records accepted: {}",
        accepted.join(", ")
    );
}
