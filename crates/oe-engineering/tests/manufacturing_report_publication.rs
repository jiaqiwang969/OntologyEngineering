use oe_core::{SkillRoot, sha256};
use oe_engineering::manufacturing_report::publication as report;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: SkillRoot,
    path: PathBuf,
    input: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let real = SkillRoot::new(actual).unwrap();
        let relative = format!(
            "var/maintenance/rust-rewrite-20260927/phase13-manufacturing-report-publication-01/test-temp/{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let path = real.new_private_dir(Path::new(&relative)).unwrap();
        fs::write(path.join("SKILL.md"), "task-owned report fixture\n").unwrap();
        for (name, digest) in report::source_identity().as_object().unwrap() {
            let bytes = fs::read(actual.join(name)).unwrap();
            assert_eq!(sha256(&bytes), digest.as_str().unwrap());
            let dest = path.join(name);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::write(dest, bytes).unwrap();
        }
        let root = SkillRoot::new(&path).unwrap();
        fs::create_dir(path.join("var")).unwrap();
        let input = path.join("var/input.json");
        fs::copy(actual.join("skills/manufacturing-process-cost/assets/report-template/example/03-resource-cost.json"),&input).unwrap();
        Self { root, path, input }
    }
    fn generate(&self) -> Value {
        report::generate(&self.root, &self.input, Path::new("var/report")).unwrap()
    }
    fn output(&self) -> PathBuf {
        self.path.join("var/report")
    }
    fn manifest(&self) -> Value {
        oe_core::json::strict_json(&fs::read(self.output().join("manifest.json")).unwrap()).unwrap()
    }
    fn manifest_write(&self, v: &Value) {
        fs::write(
            self.output().join("manifest.json"),
            serde_json::to_vec_pretty(v).unwrap(),
        )
        .unwrap();
    }
    fn relock(&self, name: &str) {
        let mut m = self.manifest();
        m["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["path"] == name)
            .unwrap()["sha256"] = json!(sha256(fs::read(self.output().join(name)).unwrap()));
        self.manifest_write(&m);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
#[test]
fn generation_freezes_exact_input_and_rejects_overwrite() {
    let f = Fixture::new();
    let bytes = fs::read(&f.input).unwrap();
    let result = f.generate();
    assert_eq!(result["passed"], true);
    assert_eq!(result["status"], "tex_verified");
    assert_eq!(result["compiled"], false);
    assert!(result["pdf_sha256"].is_null());
    assert_eq!(fs::read(f.output().join("snapshot.json")).unwrap(), bytes);
    assert!(report::generate(&f.root, &f.input, Path::new("var/report")).is_err());
    assert_eq!(report::verify(&f.root, &f.output()).unwrap(), result);
}
#[test]
fn frozen_output_tamper_fails_and_is_not_repaired() {
    let f = Fixture::new();
    f.generate();
    let file = f.output().join("report.tex");
    let mut bytes = fs::read(&file).unwrap();
    bytes.extend(b"changed");
    fs::write(&file, &bytes).unwrap();
    let result = report::verify(&f.root, &f.output()).unwrap();
    assert_eq!(result["passed"], false);
    assert_eq!(fs::read(file).unwrap(), bytes);
}
#[test]
fn rehashed_tex_and_map_must_still_correspond_to_snapshot() {
    let f = Fixture::new();
    f.generate();
    let file = f.output().join("report.tex");
    let original = fs::read_to_string(&file).unwrap();
    fs::write(&file, original.replace("2088.00", "9999.00")).unwrap();
    f.relock("report.tex");
    let result = report::verify(&f.root, &f.output()).unwrap();
    assert_eq!(result["passed"], false);
    assert!(
        result["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("TeX does not correspond"))
    );
    fs::write(file, original).unwrap();
    f.relock("report.tex");
    let file = f.output().join("render-map.json");
    let mut map: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    map["bindings"]["R-EQUIP"]["source_ids"] = json!(["S1"]);
    fs::write(file, serde_json::to_vec(&map).unwrap()).unwrap();
    f.relock("render-map.json");
    assert_eq!(
        report::verify(&f.root, &f.output()).unwrap()["passed"],
        false
    );
}
#[test]
fn core_and_external_output_locations_and_aliases_rejected_before_creation() {
    let f = Fixture::new();
    let outside = f
        .path
        .parent()
        .unwrap()
        .join(format!("outside-{}", SEQ.fetch_add(1, Ordering::Relaxed)));
    assert!(!outside.exists());
    for output in [
        f.path.join("references/report"),
        outside.clone(),
        f.path.join("var/../escaped"),
        f.path.join("var//alias"),
        f.path.join("var/./alias"),
    ] {
        assert!(report::generate(&f.root, &f.input, &output).is_err());
    }
    if !cfg!(windows) {
        assert!(report::generate(&f.root, &f.input, &f.path.join("var\\alias")).is_err());
    }
    assert!(!outside.exists());
    assert!(!f.path.join("references/report").exists());
    assert!(!f.path.join("escaped").exists());
}
#[test]
fn duplicate_input_json_and_unsupported_semantic_status_make_no_output() {
    let f = Fixture::new();
    let bytes = fs::read_to_string(&f.input).unwrap();
    fs::write(
        &f.input,
        bytes.replacen('{', "{\"semantic_status\":\"passed\",", 1),
    )
    .unwrap();
    assert!(report::generate(&f.root, &f.input, Path::new("var/report")).is_err());
    assert!(!f.output().exists());
    let mut value: Value = serde_json::from_str(&bytes).unwrap();
    value["semantic_status"] = json!("passed");
    fs::write(&f.input, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(report::generate(&f.root, &f.input, Path::new("var/report")).is_err());
    assert!(!f.output().exists());
}
#[test]
fn missing_extra_duplicate_inventory_and_manifest_json_fail() {
    let f = Fixture::new();
    f.generate();
    let file = f.output().join("report.tex");
    let original = fs::read(&file).unwrap();
    fs::remove_file(&file).unwrap();
    assert_eq!(
        report::verify(&f.root, &f.output()).unwrap()["passed"],
        false
    );
    fs::write(file, original).unwrap();
    fs::write(f.output().join("extra.json"), "{}").unwrap();
    assert!(report::verify(&f.root, &f.output()).is_err());
    fs::remove_file(f.output().join("extra.json")).unwrap();
    let original = f.manifest();
    let mut duplicate = original.clone();
    duplicate["files"]
        .as_array_mut()
        .unwrap()
        .push(original["files"][0].clone());
    f.manifest_write(&duplicate);
    assert!(report::verify(&f.root, &f.output()).is_err());
    f.manifest_write(&original);
    let bytes = fs::read_to_string(f.output().join("manifest.json")).unwrap();
    fs::write(
        f.output().join("manifest.json"),
        bytes.replacen('{', "{\"compiled\":true,", 1),
    )
    .unwrap();
    assert!(report::verify(&f.root, &f.output()).is_err());
}
#[cfg(unix)]
#[test]
fn source_output_and_artifact_symlinks_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let linked = f.path.join("var/link.json");
    symlink(&f.input, &linked).unwrap();
    assert!(report::generate(&f.root, &linked, Path::new("var/report")).is_err());
    let elsewhere = f.path.join("var/other");
    fs::create_dir(&elsewhere).unwrap();
    let linked_dir = f.path.join("var/linked");
    symlink(&elsewhere, &linked_dir).unwrap();
    assert!(report::generate(&f.root, &f.input, &linked_dir.join("report")).is_err());
    f.generate();
    let file = f.output().join("snapshot.json");
    fs::remove_file(&file).unwrap();
    symlink(&f.input, file).unwrap();
    assert!(report::verify(&f.root, &f.output()).is_err());
}
#[test]
fn assets_and_own_execution_sources_must_match_compiled_identity() {
    for name in [
        "skills/manufacturing-process-cost/assets/report-template/preamble.tex",
        "skills/manufacturing-process-cost/assets/report-template/contract.json",
        "crates/oe-engineering/src/manufacturing_report/decimal.rs",
    ] {
        let f = Fixture::new();
        let p = f.path.join(name);
        let mut bytes = fs::read(&p).unwrap();
        bytes.extend(b"\nchanged");
        fs::write(p, bytes).unwrap();
        assert!(report::generate(&f.root, &f.input, Path::new("var/report")).is_err());
        assert!(!f.output().exists());
    }
}
#[test]
fn relocked_archived_template_and_forged_compiled_flag_are_rejected() {
    let f = Fixture::new();
    f.generate();
    let mut m = f.manifest();
    m["compiled"] = json!(true);
    f.manifest_write(&m);
    assert!(
        report::verify(&f.root, &f.output())
            .unwrap_err()
            .to_string()
            .contains("delivery_file_inventory_differs")
    );
    m["compiled"] = json!(false);
    f.manifest_write(&m);
    let file = f.output().join("template.tex");
    fs::write(file, "different template").unwrap();
    f.relock("template.tex");
    assert!(report::verify(&f.root, &f.output()).is_err());
}
#[test]
fn relocked_duplicate_snapshot_or_map_json_cannot_pass() {
    for name in ["snapshot.json", "render-map.json"] {
        let f = Fixture::new();
        f.generate();
        let p = f.output().join(name);
        let bytes = fs::read_to_string(&p).unwrap();
        let modified = if name == "snapshot.json" {
            bytes.replacen('{', "{\"format\":\"shadow\",", 1)
        } else {
            bytes.replacen('{', "{\"views\":[],", 1)
        };
        fs::write(p, modified).unwrap();
        f.relock(name);
        if name == "snapshot.json" {
            let mut m = f.manifest();
            m["input_sha256"] = json!(sha256(fs::read(f.output().join(name)).unwrap()));
            f.manifest_write(&m);
        }
        assert!(report::verify(&f.root, &f.output()).is_err());
    }
}
#[test]
fn metadata_and_stored_receipt_do_not_authorize_themselves() {
    let f = Fixture::new();
    f.generate();
    let mut m = f.manifest();
    m["revision"] = json!("forged");
    f.manifest_write(&m);
    assert!(report::verify(&f.root, &f.output()).is_err());
    m["revision"] = json!("R03");
    f.manifest_write(&m);
    let file = f.output().join("verification.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    receipt["compiled"] = json!(true);
    receipt["semantic_status"] = json!("passed");
    fs::write(file, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert_eq!(
        report::verify(&f.root, &f.output()).unwrap()["passed"],
        false
    );
}

#[test]
#[ignore = "requires XeLaTeX, Poppler and the report template fonts"]
fn actual_pdf_round_trip_and_byte_tamper_rejection() {
    let f = Fixture::new();
    let tools = oe_engineering::manufacturing_report::pdf::PdfTools::default();
    let result =
        report::generate_with_tools(&f.root, &f.input, Path::new("var/report"), Some(&tools))
            .unwrap();
    assert_eq!(result["passed"], true);
    assert_eq!(result["compiled"], true);
    assert!(result["pages"].as_u64().unwrap() > 0);
    assert_eq!(
        report::verify_with_tools(&f.root, &f.output(), &tools).unwrap(),
        result
    );
    let pdf = f.output().join("report.pdf");
    let mut changed = fs::read(&pdf).unwrap();
    changed[10] ^= 1;
    fs::write(pdf, changed).unwrap();
    let rejected = report::verify_with_tools(&f.root, &f.output(), &tools).unwrap();
    assert_eq!(rejected["passed"], false);
    assert!(
        rejected["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "file changed: report.pdf")
    );
}
