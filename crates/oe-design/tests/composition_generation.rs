//! Executed source/publish rejection; actual ggen equality also has private
//! fixed-producer receipts. Test name is part of the annotated validation map.
use oe_core::SkillRoot;
use oe_design::composition;
use std::path::Path;
#[test]
fn composition2_actual_ggen_products_bind_all_sources_and_atomic_refusal() {
    let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let tmp = tempfile::tempdir_in(actual.join("var")).unwrap();
    let w = tmp.path();
    std::fs::write(w.join("SKILL.md"), "source capture test").unwrap();
    // Current executing code can be present while an independent /2 source is
    // absent from its build capture; that source must not gain formal authority.
    for name in oe_design::provenance::compiled().unwrap()["files"]
        .as_object()
        .unwrap()
        .keys()
    {
        let q = w.join(name);
        std::fs::create_dir_all(q.parent().unwrap()).unwrap();
        std::fs::copy(actual.join(name), q).unwrap();
    }
    let r = SkillRoot::new(w).unwrap();
    oe_design::provenance::require(&r).unwrap();
    let legacy =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/composition-r2-foundation.toml");
    let d = oe_design::load(&legacy).unwrap();
    let raw = std::fs::read(&legacy).unwrap();
    let s = w.join("specs");
    std::fs::create_dir_all(&s).unwrap();
    std::fs::write(s.join("foundation.toml"), &raw).unwrap();
    let text = format!(
        "schema=\"oev2.design/2\"\nversion={}\nproblem={}\npurpose={}\n[[fragments]]\npath=\"foundation.toml\"\nsha256=\"{}\"\nformat=\"oev2.design/1\"\n",
        serde_json::to_string(&d.version).unwrap(),
        serde_json::to_string(&d.problem).unwrap(),
        serde_json::to_string(&d.purpose).unwrap(),
        oe_core::sha256(&raw)
    );
    let root = s.join("composed.toml");
    std::fs::write(&root, text).unwrap();
    let loaded = composition::load(w, &root).unwrap();
    assert_eq!(
        composition::binding(&r, &loaded).unwrap()["status"],
        "unmatched_or_uncaptured"
    );
    assert!(composition::require_current(&r, &loaded).is_err());
    std::fs::create_dir(w.join("generated")).unwrap();
    std::fs::write(w.join("generated/previous.bin"), b"original version").unwrap();
    let before = oe_design::generation::snapshot(&w.join("generated")).unwrap();
    assert!(
        oe_design::generation::run_spec(&r, &root, &actual.join("unavailable-ggen"))
            .unwrap_err()
            .to_string()
            .contains("design_composition_uncompiled_or_changed_input")
    );
    assert_eq!(
        before,
        oe_design::generation::snapshot(&w.join("generated")).unwrap()
    );
}
