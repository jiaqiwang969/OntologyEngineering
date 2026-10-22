use std::path::PathBuf;

/// Original native receipts are private and must be explicitly supplied. A
/// skipped default run is not evidence of native or Windows qualification.
#[test]
#[ignore = "requires original saved/reopened native receipt directories"]
fn retirement_real_receipts_and_independence_controls() {
    let saved = PathBuf::from(std::env::var_os("OE_RETIREMENT_SAVED").expect("saved receipts"));
    let reopened =
        PathBuf::from(std::env::var_os("OE_RETIREMENT_REOPENED").expect("reopen receipts"));
    let report = oe_cad::drawing::retirement::compare(&saved, &reopened).unwrap();
    assert_eq!(report["conclusion"], "pass");
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_ne!(report["saved_process"], report["reopened_process"]);
    assert_eq!(
        oe_cad::drawing::retirement::compare(&saved, &saved)
            .unwrap_err()
            .to_string(),
        "retirement_reopen_process_or_tool_identity_mismatch"
    );
}
