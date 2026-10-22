//! Original package oracles, executed from a caller with the Windows CLI stack budget.
#![cfg(windows)]

#[test]
fn evidence_corpus_preserves_frozen_oracles_from_a_small_caller_stack() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let receipt = std::thread::Builder::new()
        .name("one-megabyte-caller".into())
        .stack_size(1024 * 1024)
        .spawn(move || oe_kernel::packages::regress(&root, "engineering-evidence-methods"))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(receipt["scenario_count"], 265);
    assert_eq!(receipt["passed"], 265);
    assert_eq!(receipt["failed"], 0);
    assert_eq!(receipt["unknown"], 0);
    assert_eq!(receipt["status"], "passed");
}
