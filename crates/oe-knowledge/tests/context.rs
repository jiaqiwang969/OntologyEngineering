use oe_core::json::{canonical_digest, strict_json};
use oe_core::{SkillRoot, sha256};
use oe_knowledge::context;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    path: PathBuf,
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("var/maintenance/rust-tests");
        let path = base.join(format!(
            "knowledge-context-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("var/projects/p")).unwrap();
        fs::write(path.join("SKILL.md"), b"test").unwrap();
        let root = SkillRoot::new(&path).unwrap();
        Self { path, root }
    }
    fn write(&self, value: &Value) {
        fs::write(
            self.path.join("var/projects/p/source.json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    fn project(&self) -> Value {
        self.write(&source());
        context::project(&self.root, &spec()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
fn source() -> Value {
    json!({"project":{"id":"actual-project","revision":"r1"},"object":{"id":"轴","revision":"A"},"decision_id":"choice","goal":"选择😀与e\u{301}的条件","unknowns":["unknown"],"statements":[{"id":"claim","text":"reported","status":"source_report","source_refs":[]}],"a/b":{"~key":["escaped"]}})
}
fn spec() -> Value {
    let fields = [
        "/project/id",
        "/project/revision",
        "/object/id",
        "/object/revision",
        "/decision_id",
        "/goal",
        "/unknowns",
        "/statements",
    ]
    .map(|p| json!({"source":"var/projects/p/source.json","pointer":p,"target":p}));
    json!({"schema":context::PROJECTION_SCHEMA,"base":{"schema":context::SCHEMA,"task_id":"turn-1"},"fields":fields})
}

#[test]
fn old_optional_identity_and_private_model_references_are_preserved() {
    let v=context::validate(&json!({"schema":context::SCHEMA,"task_id":"t","record_refs":["/private/ledger"],"statements":[{"id":"h","text":"unknown","status":"hypothesis","source_refs":["/private/source"]}]})).unwrap();
    assert!(v.get("decision_criteria").is_none());
    assert!(v.get("source_bindings").is_none());
    assert_eq!(v, context::validate(&v).unwrap());
    assert_eq!(v["object"]["id"], "");
    let model = context::model_view(&v).unwrap();
    assert!(!model.to_string().contains("/private/"));
    assert_eq!(model["context_sha256"], canonical_digest(&v).unwrap());
    assert_eq!(model["statements"][0]["status"], "hypothesis");
}
#[test]
fn projection_exact_bytes_unicode_and_escaped_pointers() {
    let f = Fixture::new();
    let c = f.project();
    let raw = fs::read(f.path.join("var/projects/p/source.json")).unwrap();
    assert_eq!(c["source_bindings"][0]["sha256"], sha256(&raw));
    assert_eq!(c["goal"], source()["goal"]);
    assert_eq!(
        context::verify_sources(&f.root, &c).unwrap()["status"],
        "current"
    );
    let mut s = spec();
    s["fields"] =
        json!([{"source":"var/projects/p/source.json","pointer":"/a~1b/~0key/0","target":"/goal"}]);
    assert_eq!(context::project(&f.root, &s).unwrap()["goal"], "escaped");
    s["fields"][0]["pointer"] = json!("/a~1b/~0key/00");
    assert!(context::project(&f.root, &s).is_err());
    assert_eq!(
        fs::read(f.path.join("var/projects/p/source.json")).unwrap(),
        raw
    );
}
#[test]
fn whole_file_drift_and_coherently_rehashed_value_are_stale() {
    let f = Fixture::new();
    let mut c = f.project();
    c["decision_id"] = json!("other");
    c["source_bindings"][0]["mappings"][4]["value_sha256"] =
        json!(canonical_digest(&c["decision_id"]).unwrap());
    let check = context::verify_sources(&f.root, &c).unwrap();
    assert_eq!(check["status"], "stale");
    assert!(
        check["sources"][0]["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("source_value_changed"))
    );
    let c = f.project();
    let mut updated = source();
    updated["unselected"] = json!("changed");
    f.write(&updated);
    assert_eq!(
        context::verify_sources(&f.root, &c).unwrap()["sources"][0]["reasons"],
        json!(["source_changed"])
    );
}
#[test]
fn unmapped_facts_stale_but_call_metadata_and_references_allowed() {
    let f = Fixture::new();
    let mut c = f.project();
    c["task_id"] = json!("later-call");
    c["record_refs"] = json!(["var/projects/p/note.json"]);
    assert_eq!(
        context::verify_sources(&f.root, &c).unwrap()["status"],
        "current"
    );
    c["object"]["name"] = json!("invented");
    assert_eq!(
        context::verify_sources(&f.root, &c).unwrap()["unmapped_context_fields"],
        json!(["/object/name"])
    );
}
#[test]
fn selected_comparison_sources_may_cross_directories_without_inferred_identity() {
    let f = Fixture::new();
    f.write(&source());
    fs::create_dir_all(f.path.join("var/projects/comparison")).unwrap();
    fs::write(
        f.path.join("var/projects/comparison/record.json"),
        br#"{"alternatives":["reference design"]}"#,
    )
    .unwrap();
    let mut s = spec();
    s["fields"].as_array_mut().unwrap().push(json!({"source":"var/projects/comparison/record.json","pointer":"/alternatives","target":"/alternatives"}));
    let c = context::project(&f.root, &s).unwrap();
    assert_eq!(c["project"]["id"], "actual-project");
    assert_eq!(
        context::verify_sources(&f.root, &c).unwrap()["status"],
        "current"
    );
}
#[test]
fn duplicate_keys_nonfinite_and_normalization_changes_rejected() {
    let f = Fixture::new();
    let mut s = spec();
    s["fields"] = json!([{"source":"var/projects/p/source.json","pointer":"/statements","target":"/statements"}]);
    f.write(&json!({"statements":[{"id":"s","text":"not normalized","status":"unknown"}]}));
    assert!(
        context::project(&f.root, &s)
            .unwrap_err()
            .to_string()
            .contains("normalization_changed")
    );
    for raw in [
        b"{\"goal\":\"x\",\"goal\":\"y\"}".as_slice(),
        b"{\"goal\":NaN}",
    ] {
        fs::write(f.path.join("var/projects/p/source.json"), raw).unwrap();
        assert!(context::project(&f.root, &spec()).is_err());
    }
    assert!(strict_json(b"{\"x\":1e400}").is_err());
}
#[test]
fn bounds_unknown_verdicts_and_credentials_rejected() {
    for bad in [
        json!({"statements":[{"id":"x","status":"passed"}]}),
        json!({"extra":"fact"}),
        json!({"functions":vec!["x";17]}),
    ] {
        let mut c = json!({"schema":context::SCHEMA,"task_id":"t"});
        c.as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        assert!(context::validate(&c).is_err());
    }
    let token = ["apikey_", "synthetic_test"].concat();
    assert!(
        context::validate(&json!({"schema":context::SCHEMA,"task_id":"t","goal":token})).is_err()
    );
    let large =
        json!({"schema":context::SCHEMA,"task_id":"t","functions":vec!["字".repeat(600);16]});
    assert!(
        context::validate(&large)
            .unwrap_err()
            .to_string()
            .contains("too_large")
    );
}
#[test]
fn invalid_paths_targets_pointers_and_missing_file_do_not_project() {
    let f = Fixture::new();
    f.write(&source());
    for path in [
        "/tmp/x",
        "../x",
        "var/projects/../x",
        "var//projects/p/x",
        "var/projects/./p/x",
        "var/cache/x",
    ] {
        let mut s = spec();
        s["fields"][0]["source"] = json!(path);
        assert!(context::project(&f.root, &s).is_err());
    }
    for ptr in ["/a~wrong", "x", "/x/~"] {
        let mut s = spec();
        s["fields"][0]["pointer"] = json!(ptr);
        assert!(context::project(&f.root, &s).is_err());
    }
    let c = f.project();
    fs::remove_file(f.path.join("var/projects/p/source.json")).unwrap();
    assert_eq!(
        context::verify_sources(&f.root, &c).unwrap()["sources"][0]["reasons"],
        json!(["source_missing"])
    );
}
#[cfg(unix)]
#[test]
fn symlinks_and_fifo_never_follow_or_hang() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let c = f.project();
    let p = f.path.join("var/projects/p/source.json");
    fs::rename(&p, p.with_extension("original")).unwrap();
    symlink(p.with_extension("original"), &p).unwrap();
    assert!(context::project(&f.root, &spec()).is_err());
    assert_eq!(
        context::verify_sources(&f.root, &c).unwrap()["status"],
        "stale"
    );
    fs::remove_file(&p).unwrap();
    let name = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(context::project(&f.root, &spec()).is_err());
}
#[test]
fn copied_project_tree_retains_identity() {
    let f = Fixture::new();
    let c = f.project();
    let g = Fixture::new();
    fs::copy(
        f.path.join("var/projects/p/source.json"),
        g.path.join("var/projects/p/source.json"),
    )
    .unwrap();
    assert_eq!(context::project(&g.root, &spec()).unwrap(), c);
    assert_eq!(
        context::verify_sources(&g.root, &c).unwrap()["status"],
        "current"
    );
}
