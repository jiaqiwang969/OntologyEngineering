use anyhow::Result;
use oe_core::sha256;
use oe_geometry::{acceptance::verify_job_output, evidence::load_evidence};
use serde_json::json;

#[test]
fn changed_output_is_not_accepted() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().canonicalize()?;
    let original = b"{\"radius\":1}";
    std::fs::write(path.join("geometry.json"), b"{\"radius\":999}")?;
    oe_core::write_new(
        &path.join("test.job.json"),
        &json!({
            "schema":"ontology-engineering.geometry-job/v1",
            "identity":{"job_id":"test"},"status":"completed","exit_code":0,
            "stdout_sha256":sha256(b"{}"),
            "outputs":[{"path":"geometry.json","sha256":sha256(original),"bytes":original.len()}]
        }),
    )?;
    assert!(!verify_job_output(&path, "test")?.passed());
    Ok(())
}

#[test]
fn malformed_surface_collection_is_not_an_empty_observation() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().canonicalize()?.join("geometry.json");
    oe_core::write_new(
        &path,
        &json!({"schema":oe_geometry::evidence::EVIDENCE_SCHEMA,"parts":{"part":{"cylinders":"bad","planes":[],"cones":[]}},"missing_definitions":[],"extra_definitions":[]}),
    )?;
    let error = load_evidence(&path, &"a".repeat(64))
        .err()
        .expect("malformed collection must fail");
    assert!(error.to_string().contains("cylinders_must_be_array"));
    Ok(())
}
