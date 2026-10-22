use oe_contracts::{NxAssemblySourceClosure, NxMotionEvidenceBudget};
use oe_nx_worker::geometry::motion_task::validate_delivery;
use serde_json::json;
fn fixture() -> (NxAssemblySourceClosure, NxMotionEvidenceBudget) {
    let source=serde_json::from_value(json!({"schema":"oev2.nx-assembly-source-closure/1","root_part":"ROOT.prt","files":[{"path":"ROOT.prt","sha256":"0".repeat(64),"bytes":104857600}],"max_total_bytes":104857600})).unwrap();
    let evidence=serde_json::from_value(json!({"max_events":1000000,"max_bytes":209715200,"max_event_bytes":4096,"segment_bytes":67108864,"max_segments":4})).unwrap();
    (source, evidence)
}
#[test]
fn motion_delivery_uses_whole_job_capacity_and_keeps_legacy_bound() {
    let (source, evidence) = fixture();
    assert!(validate_delivery(&evidence, &source, (536870912, 134217728, 100)).is_ok());
    assert!(validate_delivery(&evidence, &source, (104857600, 104857600, 100)).is_err());
    assert!(validate_delivery(&evidence, &source, (314572800, 134217728, 100)).is_err());
    assert!(validate_delivery(&evidence, &source, (536870912, 33554432, 100)).is_err());
    assert!(validate_delivery(&evidence, &source, (536870912, 83886080, 100)).is_err());
    assert!(validate_delivery(&evidence, &source, (536870912, 134217728, 5)).is_err());
    assert!(validate_delivery(&evidence, &source, (536870912, 134217728, 6)).is_ok());
    let mut overflow = evidence.clone();
    overflow.max_bytes = u64::MAX;
    assert!(validate_delivery(&overflow, &source, (u64::MAX, 134217728, 100)).is_err());
}
