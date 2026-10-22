//! Relocated report delivery with an empty PATH. No Python or TeX executable.
use oe_engineering::manufacturing_report::publication;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = source.join("var/maintenance/rust-tests").join(format!(
            "manufacturing-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("var/input")).unwrap();
        fs::write(root.join("SKILL.md"), "# Relocated report receiver\n").unwrap();
        for path in publication::source_identity().as_object().unwrap().keys() {
            let destination = root.join(path);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source.join(path), destination).unwrap();
        }
        let oracle: Value = serde_json::from_str(include_str!(
            "../../oe-engineering/tests/fixtures/manufacturing-report-legacy-v1.json"
        ))
        .unwrap();
        fs::write(
            root.join("var/input/report.json"),
            serde_json::to_vec_pretty(&oracle["examples"][2]["input"]).unwrap(),
        )
        .unwrap();
        Self {
            root: fs::canonicalize(root).unwrap(),
        }
    }
    fn call(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(&self.root)
            .args(["manufacturing", "report"])
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn report_round_trip_has_no_document_tools_and_detects_tampering() {
    let f = Fixture::new();
    let input = fs::read(f.root.join("var/input/report.json")).unwrap();
    let generated = success(f.call(&[
        "generate",
        "--input",
        "var/input/report.json",
        "--output",
        "var/delivery",
    ]));
    assert_eq!(generated["status"], "tex_verified");
    assert_eq!(generated["compiled"], false);
    assert_eq!(
        generated["semantic_status"],
        "not_executed_for_this_snapshot"
    );
    assert_eq!(
        fs::read(f.root.join("var/delivery/snapshot.json")).unwrap(),
        input
    );
    assert!(!f.root.join("var/delivery/report.pdf").exists());
    assert_eq!(
        success(f.call(&["verify", "--directory", "var/delivery"]))["passed"],
        true
    );
    assert_eq!(
        f.call(&[
            "generate",
            "--input",
            "var/input/report.json",
            "--output",
            "var/delivery"
        ])
        .status
        .code(),
        Some(2)
    );
    fs::write(f.root.join("var/delivery/report.tex"), b"altered report").unwrap();
    let failed = f.call(&["verify", "--directory", "var/delivery"]);
    assert_eq!(failed.status.code(), Some(1));
    let result: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(result["passed"], false);
    assert_eq!(result["status"], "tex_verification_failed");
}

#[test]
fn invalid_snapshot_and_nonprivate_output_fail_before_publication() {
    let f = Fixture::new();
    assert_eq!(
        f.call(&[
            "generate",
            "--input",
            "var/input/report.json",
            "--output",
            "sources/report"
        ])
        .status
        .code(),
        Some(2)
    );
    assert!(!f.root.join("sources/report").exists());
    fs::write(
        f.root.join("var/input/report.json"),
        br#"{"format":"one","format":"two"}"#,
    )
    .unwrap();
    assert_eq!(
        f.call(&[
            "generate",
            "--input",
            "var/input/report.json",
            "--output",
            "var/rejected"
        ])
        .status
        .code(),
        Some(2)
    );
    assert!(!f.root.join("var/rejected").exists());
}
