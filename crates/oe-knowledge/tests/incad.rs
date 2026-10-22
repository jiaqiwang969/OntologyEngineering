#[path = "support/incad_fixture.rs"]
mod fixture;
use fixture::*;
use oe_knowledge::incad::verify;
use serde_json::{Value, json};

#[test]
fn incad_original_sources_and_exact_task_are_bound_without_answer_leak() {
    let t = tempfile::tempdir().unwrap();
    let f = Fixture::new(t.path());
    let v = verify(&f.request).unwrap();
    assert_eq!(
        v.specifications_bytes(),
        std::fs::read(f.projection.join("input.json")).unwrap()
    );
    assert_eq!(
        v.task_bytes(),
        std::fs::read(f.root.join("task.json")).unwrap()
    );
    assert_eq!(
        v.design_window()["specifications"]["sections"][0]["text"],
        "force 10 N\n"
    );
    assert!(!v.design_window().to_string().contains("answer-secret"));
    assert_eq!(
        v.audit()["archive_status"]["complete_for_requested_scope"],
        false
    );
    assert_eq!(v.audit()["baseline_exposure"], "development_replay");
    assert_eq!(v.audit()["cad_files_revalidated"], false);
    std::fs::write(f.projection.join("reference.json"), b"changed answer").unwrap();
    assert!(v.ensure_current().is_err());
    assert!(verify(&f.request).is_err());
}
#[test]
fn changed_specs_missing_sources_and_claimed_holdout_are_rejected() {
    for defect in [
        "specification",
        "case",
        "holdout",
        "adoption",
        "extra_answer",
        "missing",
    ] {
        let t = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(t.path());
        match defect {
            "specification" => f.task["specifications"]["sections"][0]["text"] = json!("20 N"),
            "case" => f.task["case_number"] = json!("000020"),
            "holdout" => f.task["baseline_exposure"] = json!("unseen"),
            "adoption" => f.task["engineering_adoption"] = json!("pass"),
            "extra_answer" => f.task["answer"] = json!("not allowed"),
            "missing" => {
                std::fs::remove_file(f.root.join("cases/cn/000019/attempts/fixed/source.html"))
                    .unwrap();
            }
            _ => unreachable!(),
        }
        f.write_task();
        assert!(verify(&f.request).is_err(), "{defect}");
    }
}
#[test]
fn source_inventory_path_duplicates_and_symlinks_cannot_escape() {
    for defect in ["path", "duplicate"] {
        let t = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(t.path());
        let mp = f.projection.join("manifest.json");
        let mut m: Value = serde_json::from_slice(&std::fs::read(&mp).unwrap()).unwrap();
        if defect == "path" {
            m["files"][0]["path"] = json!("../../outside");
        } else {
            let first = m["files"][0].clone();
            m["files"].as_array_mut().unwrap().push(first);
        }
        f.request.projection_manifest.sha256 = save(&mp, &m);
        assert!(verify(&f.request).is_err());
    }
    #[cfg(unix)]
    {
        let t = tempfile::tempdir().unwrap();
        let f = Fixture::new(t.path());
        let path = f.projection.join("input.json");
        let other = t.path().join("other.json");
        std::fs::rename(&path, &other).unwrap();
        std::os::unix::fs::symlink(other, path).unwrap();
        assert!(verify(&f.request).is_err());
    }
}
#[test]
fn answer_fields_are_rejected_even_after_every_affected_hash_is_rebound() {
    for position in ["input", "section", "source", "locator", "title_type"] {
        let t = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(t.path());
        let ip = f.projection.join("input.json");
        let mp = f.projection.join("manifest.json");
        let mut input: Value = serde_json::from_slice(&std::fs::read(&ip).unwrap()).unwrap();
        match position {
            "input" => input["reference_answer"] = json!("answer-secret"),
            "title_type" => input["title"] = json!({"answer":"answer-secret"}),
            "section" => input["sections"][0]["bom"] = json!(["answer-secret"]),
            "source" => input["source"]["answer"] = json!("answer-secret"),
            "locator" => input["sections"][0]["locator"]["answer"] = json!("answer-secret"),
            _ => unreachable!(),
        }
        save(&ip, &input);
        let mut m: Value = serde_json::from_slice(&std::fs::read(&mp).unwrap()).unwrap();
        m["files"][0] = entry(&f.projection, "input.json");
        if position == "source" {
            m["source"] = input["source"].clone();
        }
        f.request.projection_manifest.sha256 = save(&mp, &m);
        f.task["title"] = input["title"].clone();
        f.task["specifications"] = input;
        f.write_task();
        assert!(verify(&f.request).is_err(), "{position}");
    }
}
