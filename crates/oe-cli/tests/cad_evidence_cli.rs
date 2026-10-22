//! Relocated source projections with an empty PATH; no Python/NX/Semantica.
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture {
    root: PathBuf,
    packet: String,
}
impl Fixture {
    fn new(kind: &str, claim: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../var/maintenance/rust-tests")
            .join(format!(
                "cad-evidence-cli-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.join("var/project")).unwrap();
        fs::write(root.join("SKILL.md"), "# Relocated synthetic receiver\n").unwrap();
        let data: Value = serde_json::from_str(include_str!(
            "../../oe-cad/tests/fixtures/cad-evidence-legacy-v1.json"
        ))
        .unwrap();
        let record = data["verification"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| {
                r["kind"] == kind
                    && r["valid"] == true
                    && (!claim || !r["packet"]["claims"].as_array().unwrap().is_empty())
            })
            .unwrap();
        for (name, text) in record["files"].as_object().unwrap() {
            let path = root.join("var/project").join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let packet = format!("var/project/{}", record["packet_path"].as_str().unwrap());
        if claim {
            let mut p = record["packet"].clone();
            p["coverage_assertions"] = json!([]);
            p["routes"].as_array_mut().unwrap().push(json!({"id":"baseline","name":"old route","status":"under_review","source_ids":["readback"],"operations":[]}));
            p["claims"][0]["baseline_route_id"] = json!("baseline");
            p["claims"][0]["target_route_id"] = json!("laser");
            fs::write(root.join(&packet), serde_json::to_vec(&p).unwrap()).unwrap();
        }
        Self {
            root: fs::canonicalize(root).unwrap(),
            packet,
        }
    }
    fn call(&self, route: &str, action: &str, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(&self.root)
            .args([
                "cad",
                route,
                action,
                "--packet",
                &self.packet,
                "--project-root",
                "var/project",
            ])
            .args(extra)
            .output()
            .unwrap()
    }
    fn json(&self, name: &str) -> Value {
        serde_json::from_slice(&fs::read(self.root.join(name)).unwrap()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn success(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn cad_packet_projects_without_python_and_retains_verified_input_and_impact() {
    let f = Fixture::new("cad", false);
    let before = fs::read(f.root.join(&f.packet)).unwrap();
    let audit = success(f.call("evidence", "verify", &[]));
    assert_eq!(audit["semantic_execution"], "not_run");
    let result = success(f.call(
        "evidence",
        "project",
        &["--output", "var/projected", "--previous-packet", &f.packet],
    ));
    assert_eq!(result["status"], "projected");
    assert_eq!(
        fs::read(f.root.join("var/projected/packet.json")).unwrap(),
        before
    );
    assert_eq!(
        f.json("var/projected/impact.json")["changed_ids"],
        json!([])
    );
    let verified =
        oe_cad::evidence::verify(&f.root.join(&f.packet), &f.root.join("var/project")).unwrap();
    assert_eq!(
        fs::read(f.root.join("var/projected/evidence.nt")).unwrap(),
        verified.project_abox()
    );
    assert!(
        !f.call("evidence", "project", &["--output", "var/projected"])
            .status
            .success()
    );
    assert_eq!(
        fs::read(f.root.join("var/projected/packet.json")).unwrap(),
        before
    );
}

#[test]
fn handoff_exports_both_request_directions_and_proposed_q21_only() {
    let f = Fixture::new("handoff", true);
    success(f.call("handoff", "verify", &[]));
    success(f.call(
        "handoff",
        "project",
        &["--output", "var/handoff", "--previous-packet", &f.packet],
    ));
    for (file, direction) in [
        ("cad-requests.json", "process_to_cad"),
        ("process-requests.json", "cad_to_process"),
    ] {
        let request = f.json(&format!("var/handoff/{file}"));
        assert_eq!(request["direction"], direction);
        assert_eq!(request["engineering_verdict"], "not_assessed");
    }
    success(f.call(
        "handoff",
        "q21",
        &["--claim-id", "deliver", "--output", "var/q21"],
    ));
    let audit = f.json("var/q21/q21.json");
    let source_audit = f.json("var/q21/source-integrity.json");
    assert_eq!(source_audit["packet_sha256"], audit["packet_sha256"]);
    assert!(
        !source_audit["source_results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(audit["package_state"], "proposed_not_promoted");
    assert_eq!(audit["semantic_execution"], "not_run");
    let graph = fs::read_to_string(f.root.join("var/q21/q21.ttl")).unwrap();
    assert!(!graph.contains("completenessEvidence"));
    assert!(!graph.contains("<urn:mfg:loop:FunctionCoverage>"));
    assert!(
        !f.call(
            "handoff",
            "q21",
            &["--claim-id", "absent", "--output", "var/missing-claim"]
        )
        .status
        .success()
    );
    assert!(!f.root.join("var/missing-claim").exists());
}

#[test]
fn invalid_sources_and_nonprivate_output_never_create_a_projection() {
    let f = Fixture::new("cad", false);
    assert!(
        !f.call("evidence", "project", &["--output", "references/report"])
            .status
            .success()
    );
    assert!(!f.root.join("references/report").exists());
    fs::write(f.root.join("var/project/readback.json"), "{}").unwrap();
    assert!(
        !f.call("evidence", "project", &["--output", "var/invalid"])
            .status
            .success()
    );
    assert!(!f.root.join("var/invalid").exists());
}
