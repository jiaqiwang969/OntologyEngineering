//! Port ledger gate over the real frozen Semantica scope: items are verified only by source-bound
//! recorded port tests, and the whole scope completes only with this build's compiled pins.
use oe_design::{migration, migration_ledger};
use serde_json::{Value, json};
use std::{fs, path::Path};

const SCOPE: &[u8] = include_bytes!("../../../runtime/semantica-scope.json");
const LOCK: &[u8] = include_bytes!("../../../runtime/semantica-scope-lock.json");
const SCOPE_SHA: &str = "b1470d9980755811c815be24cdc041740de5f55e28232c327f0845d160455226";
const SYMBOL: &str = "semantica/change_management/change_log.py|class|ChangeCategory|139";
const TEST_ITEM: &str = "tests/_smoke_review_fixes.py|original_test_function|test_bug001_semantic_neighborhood_uses_top_k|186";
const SOURCE: &str = "semantica-rs/crates/sem-demo/src/lib.rs";
const LOG: &str = "     Running unittests src/lib.rs (target/debug/deps/sem_demo-0123456789abcdef)\n\
running 2 tests\n\
test tests::change_category_round_trips ... ok\n\
test tests::neighborhood_uses_top_k ... ok\n\
test tests::broken ... FAILED\n\
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("runtime")).unwrap();
    fs::create_dir(root.path().join("specs")).unwrap();
    fs::write(root.path().join("runtime/semantica-scope.json"), SCOPE).unwrap();
    fs::write(root.path().join("runtime/semantica-scope-lock.json"), LOCK).unwrap();
    for dir in [
        "semantica-rs/ledger",
        "semantica-rs/evidence",
        "semantica-rs/crates/sem-demo/src",
    ] {
        fs::create_dir_all(root.path().join(dir)).unwrap();
    }
    fs::write(
        root.path().join(SOURCE),
        "pub enum ChangeCategory { Added }\npub struct Neighborhood;\nimpl Neighborhood { pub fn top_k(&self) -> usize { 3 } }\n",
    )
    .unwrap();
    root
}

fn ledger(root: &Path, module: &str, entries: Value) {
    fs::write(
        root.join(format!("semantica-rs/ledger/{module}.json")),
        serde_json::to_vec_pretty(&json!({"schema":"semantica.migration-ledger/1","scope_sha256":SCOPE_SHA,"module":module,"entries":entries})).unwrap(),
    )
    .unwrap();
}

/// Record a receipt for `log` bound to the current port sources.
fn record(root: &Path, log: &str) {
    let (source, _) = migration_ledger::port_source_digest(root).unwrap();
    fs::write(root.join("semantica-rs/evidence/test-run.log"), log).unwrap();
    fs::write(
        root.join("semantica-rs/evidence/test-receipt.json"),
        serde_json::to_vec_pretty(
            &json!({"schema":"semantica.port-test-receipt/1","source_sha256":source,
            "log":"test-run.log","log_sha256":oe_core::sha256(log.as_bytes()),"toolchain":"test"}),
        )
        .unwrap(),
    )
    .unwrap();
}

fn status<'a>(report: &'a Value, id: &str) -> &'a str {
    report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .unwrap()["migration_status"]
        .as_str()
        .unwrap()
}

fn migrated_symbol(test: &str) -> Value {
    json!({"id":SYMBOL,"disposition":"migrated","rust":[format!("{SOURCE}::ChangeCategory")],"tests":[test]})
}

#[test]
fn recorded_passing_port_test_verifies_one_item_without_completing_the_scope() {
    let root = fixture();
    ledger(
        root.path(),
        "change_management",
        json!([migrated_symbol(
            "sem_demo::tests::change_category_round_trips"
        )]),
    );
    ledger(
        root.path(),
        "tests",
        json!([{"id":TEST_ITEM,"disposition":"ported_test",
        "rust":[format!("{SOURCE}::Neighborhood::top_k")],"tests":["sem_demo::tests::neighborhood_uses_top_k"]}]),
    );
    record(root.path(), LOG);
    let r = migration::audit(root.path()).unwrap();
    assert_eq!(status(&r, SYMBOL), "verified");
    assert_eq!(status(&r, TEST_ITEM), "verified");
    assert_eq!(r["port"]["verified"], 2);
    assert_eq!(r["port"]["missing"], 13492);
    assert_eq!(r["port"]["evidence"]["status"], "fresh");
    assert_eq!(r["port"]["evidence"]["failed"], 1);
    assert_eq!(r["migration_status"], "incomplete");
    assert_eq!(r["behavior_status"], "unknown");
}

#[test]
fn failed_unlisted_or_stale_tests_do_not_verify() {
    let root = fixture();
    ledger(
        root.path(),
        "change_management",
        json!([migrated_symbol("sem_demo::tests::broken")]),
    );
    record(root.path(), LOG);
    let r = migration::audit(root.path()).unwrap();
    assert_eq!(status(&r, SYMBOL), "unverified");
    assert!(
        r["port"]["problems"][0]
            .as_str()
            .unwrap()
            .contains("test_not_passed")
    );

    ledger(
        root.path(),
        "change_management",
        json!([migrated_symbol("sem_demo::tests::never_ran")]),
    );
    let r = migration::audit(root.path()).unwrap();
    assert_eq!(status(&r, SYMBOL), "unverified");

    // A passing test recorded before the sources changed is stale evidence.
    ledger(
        root.path(),
        "change_management",
        json!([migrated_symbol(
            "sem_demo::tests::change_category_round_trips"
        )]),
    );
    assert_eq!(
        status(&migration::audit(root.path()).unwrap(), SYMBOL),
        "verified"
    );
    fs::write(
        root.path()
            .join("semantica-rs/crates/sem-demo/src/extra.rs"),
        "pub fn more() {}\n",
    )
    .unwrap();
    let r = migration::audit(root.path()).unwrap();
    assert_eq!(r["port"]["evidence"]["status"], "stale");
    assert_eq!(status(&r, SYMBOL), "unverified");
    assert!(
        r["port"]["problems"][0]
            .as_str()
            .unwrap()
            .contains("evidence_stale")
    );
}

#[test]
fn edited_raw_log_and_listed_results_are_refused() {
    let root = fixture();
    ledger(
        root.path(),
        "change_management",
        json!([migrated_symbol("sem_demo::tests::broken")]),
    );
    record(root.path(), LOG);
    // Turning FAILED into ok after recording breaks the receipt's log digest.
    fs::write(
        root.path().join("semantica-rs/evidence/test-run.log"),
        LOG.replace("broken ... FAILED", "broken ... ok"),
    )
    .unwrap();
    assert!(
        migration::audit(root.path())
            .unwrap_err()
            .to_string()
            .contains("migration_receipt_log_digest_mismatch")
    );
}

#[test]
fn undeclared_symbols_and_unsafe_references_do_not_verify() {
    let root = fixture();
    record(root.path(), LOG);
    for (rust, problem) in [
        (format!("{SOURCE}::ChangeKind"), "rust_symbol_not_declared"),
        (
            format!("{SOURCE}::Missing::top_k"),
            "rust_owner_not_declared",
        ),
        (
            "semantica-rs/crates/sem-demo/src/absent.rs".to_owned(),
            "rust_reference_missing_file",
        ),
        (
            "crates/oe-core/src/lib.rs::sha256".to_owned(),
            "migration_ledger_reference_outside_port",
        ),
        (
            "semantica-rs/target/debug/x.rs".to_owned(),
            "migration_ledger_reference_to_build_or_evidence",
        ),
        (
            "semantica-rs/crates/../evidence/test-run.log".to_owned(),
            "migration_ledger_unsafe_reference",
        ),
    ] {
        ledger(
            root.path(),
            "change_management",
            json!([{"id":SYMBOL,"disposition":"migrated","rust":[rust],
            "tests":["sem_demo::tests::change_category_round_trips"]}]),
        );
        let r = migration::audit(root.path()).unwrap();
        assert_eq!(status(&r, SYMBOL), "unverified", "{problem}");
        assert!(
            r["port"]["problems"][0].as_str().unwrap().contains(problem),
            "{}",
            r["port"]["problems"][0]
        );
    }
}

#[test]
fn retained_files_must_be_byte_identical_to_the_original() {
    let root = fixture();
    let scope: Value = serde_json::from_slice(SCOPE).unwrap();
    let file = scope["tracked_files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "LICENSE")
        .unwrap()
        .clone();
    let id = "LICENSE|tracked_file||0";
    let original = root.path().join("var-original-license");
    fs::write(&original, b"MIT License (changed)").unwrap();
    fs::create_dir_all(root.path().join("semantica-rs/retained")).unwrap();
    fs::copy(&original, root.path().join("semantica-rs/retained/LICENSE")).unwrap();
    let changed = oe_core::sha256(&fs::read(&original).unwrap());
    ledger(
        root.path(),
        "repo",
        json!([{"id":id,"disposition":"retained","retained":[{"path":"semantica-rs/retained/LICENSE","sha256":changed}]}]),
    );
    record(root.path(), LOG);
    let r = migration::audit(root.path()).unwrap();
    assert_ne!(changed, file["sha256"].as_str().unwrap());
    assert_eq!(status(&r, id), "unverified");
    assert!(
        r["port"]["problems"][0]
            .as_str()
            .unwrap()
            .contains("retained_not_byte_identical_to_original")
    );
}

#[test]
fn ported_test_original_result_must_match_the_recorded_baseline() {
    let root = fixture();
    let entry = |result: &str| json!([{"id":TEST_ITEM,"disposition":"ported_test","tests":["sem_demo::tests::neighborhood_uses_top_k"],"original_result":result}]);
    ledger(root.path(), "tests", entry("failed"));
    record(root.path(), LOG);
    let r = migration::audit(root.path()).unwrap();
    assert!(
        r["port"]["problems"][0]
            .as_str()
            .unwrap()
            .contains("original_result_without_baseline")
    );
    fs::write(
        root.path().join("semantica-rs/evidence/original-baseline.json"),
        serde_json::to_vec(&json!({"schema":"semantica.original-test-baseline/1","scope_sha256":SCOPE_SHA,"results":{TEST_ITEM:"passed"}})).unwrap(),
    )
    .unwrap();
    let r = migration::audit(root.path()).unwrap();
    assert!(
        r["port"]["problems"][0]
            .as_str()
            .unwrap()
            .contains("original_result_mismatch:passed")
    );
    ledger(root.path(), "tests", entry("passed"));
    assert_eq!(
        status(&migration::audit(root.path()).unwrap(), TEST_ITEM),
        "verified"
    );
}

#[test]
fn duplicate_unknown_or_unbound_ledger_rows_are_refused() {
    let root = fixture();
    record(root.path(), LOG);
    let row = migrated_symbol("sem_demo::tests::change_category_round_trips");
    ledger(root.path(), "a", json!([row.clone()]));
    ledger(root.path(), "b", json!([row]));
    assert!(
        migration::audit(root.path())
            .unwrap_err()
            .to_string()
            .contains("migration_ledger_duplicate_scope_id")
    );
    fs::remove_file(root.path().join("semantica-rs/ledger/b.json")).unwrap();
    ledger(
        root.path(),
        "a",
        json!([{"id":"semantica/invented.py|function|f|1","disposition":"migrated"}]),
    );
    assert!(
        migration::audit(root.path())
            .unwrap_err()
            .to_string()
            .contains("migration_ledger_unknown_scope_id")
    );
    fs::write(
        root.path().join("semantica-rs/ledger/a.json"),
        serde_json::to_vec(&json!({"schema":"semantica.migration-ledger/1","scope_sha256":"0".repeat(64),"module":"a","entries":[]})).unwrap(),
    )
    .unwrap();
    assert!(
        migration::audit(root.path())
            .unwrap_err()
            .to_string()
            .contains("migration_ledger_binding_mismatch")
    );
}

#[test]
fn completion_requires_pinned_fresh_port_evidence() {
    // Every one of the 13,494 items verified by fresh evidence is still not complete unless the
    // ledger and evidence digests equal the pins compiled into this program.
    let root = fixture();
    let report = migration::audit(root.path()).unwrap();
    let entries: Vec<Value> = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| json!({"id":i["id"],"disposition":"superseded","by":[SOURCE],"note":"synthetic gate fixture only"}))
        .collect();
    ledger(root.path(), "all", Value::Array(entries));
    record(
        root.path(),
        LOG.replace("test tests::broken ... FAILED\n", "").as_str(),
    );
    let r = migration::audit(root.path()).unwrap();
    assert_eq!(r["port"]["verified"], 13494);
    assert_eq!(r["port"]["missing"], 0);
    assert_eq!(r["port"]["evidence"]["status"], "fresh");
    assert_eq!(r["port"]["pinned"], false);
    assert_eq!(r["migration_status"], "incomplete");
    assert_eq!(r["behavior_status"], "unknown");
}
