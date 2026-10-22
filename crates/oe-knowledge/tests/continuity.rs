use oe_core::{SkillRoot, sha256};
use oe_knowledge::continuity::mapped_sources;
use serde_json::json;
#[test]
fn mapped_sources_read_actual_bytes_and_detect_drift() {
    let actual = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let temp = tempfile::tempdir_in(actual.join("var/builds")).unwrap();
    let root = temp.path();
    std::fs::write(root.join("SKILL.md"), "private source fixture").unwrap();
    std::fs::create_dir_all(root.join("references")).unwrap();
    let path = "references/ontology-engineering-book/handbook/chapters/ch02.tex";
    let copy = root.join("sources/book-authoring").join(path);
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::write(&copy, "bounded chapter text").unwrap();
    std::fs::write(root.join("references/judgment-pattern-source-map.json"),serde_json::to_vec(&json!({"sources":{"vol1-ch02":{"path":path,"sha256":sha256("bounded chapter text"),"role":"methodological_source_not_project_fact"}}})).unwrap()).unwrap();
    let r = SkillRoot::new(root).unwrap();
    assert_eq!(mapped_sources(&r).unwrap()["status"], "verified");
    std::fs::write(&copy, "changed chapter text").unwrap();
    let result = mapped_sources(&r).unwrap();
    assert_eq!(result["status"], "incomplete");
    assert_eq!(result["sources"][0]["status"], "drift");
    assert_eq!(
        result["sources"][0]["expected_sha256"],
        sha256("bounded chapter text")
    );
}

#[test]
fn method_copies_preserve_originals_and_collisions_do_not_overwrite() {
    let actual = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let old = tempfile::tempdir_in(actual.join("var/builds")).unwrap();
    let new = tempfile::tempdir_in(actual.join("var/builds")).unwrap();
    for p in [
        "SKILL.md",
        "skills/cad-agent/SKILL.md",
        "skills/cad-agent/CONTRIBUTING.md",
        "skills/cad-agent/AGENTS.md",
        "references/supplier-knowledge.md",
        "skills/cad-agent/references/nx-execution.md",
    ] {
        let file = old.path().join(p);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            file,
            if p == "skills/cad-agent/SKILL.md" {
                "[NX](references/nx-execution.md)"
            } else {
                "historical method fixture"
            },
        )
        .unwrap();
    }
    std::fs::write(new.path().join("SKILL.md"), "current method fixture").unwrap();
    let root = SkillRoot::new(new.path()).unwrap();
    let report = oe_knowledge::continuity::receive_methods(&root, old.path()).unwrap();
    assert_eq!(report["added_files"], 6);
    assert_eq!(report["links"][0]["status"], "verified");
    assert_eq!(
        std::fs::read(
            old.path()
                .join("skills/cad-agent/references/nx-execution.md")
        )
        .unwrap(),
        b"historical method fixture"
    );
    let copy = new
        .path()
        .join("sources/history/skills/cad-agent/references/nx-execution.md");
    std::fs::write(&copy, "local drift").unwrap();
    assert!(
        oe_knowledge::continuity::receive_methods(&root, old.path())
            .unwrap_err()
            .to_string()
            .contains("method_existing_copy_drift")
    );
    assert_eq!(std::fs::read(&copy).unwrap(), b"local drift");
    assert_eq!(
        std::fs::read(
            old.path()
                .join("skills/cad-agent/references/nx-execution.md")
        )
        .unwrap(),
        b"historical method fixture"
    );
}
