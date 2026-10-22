//! Original complete/1 purpose and independent source mutations; no renderer mock.
use oe_design::{composition, load};
use std::path::{Path, PathBuf};
fn original() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/composition-r2-foundation.toml")
}
fn fixture() -> (tempfile::TempDir, PathBuf, Vec<u8>) {
    let private = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../var")
        .canonicalize()
        .unwrap();
    std::fs::create_dir_all(&private).unwrap();
    let tmp = tempfile::tempdir_in(private).unwrap();
    let raw = std::fs::read(original()).unwrap();
    assert_eq!(
        oe_core::sha256(&raw),
        "5c6e03bc11739e504d96bca839e59ce0f22fedc5ba11e897f2cee975cd29cfef"
    );
    std::fs::write(tmp.path().join("foundation.toml"), &raw).unwrap();
    let d = load(&original()).unwrap();
    let root = tmp.path().join("root.toml");
    let manifest = format!(
        "schema = \"oev2.design/2\"\nversion = {}\nproblem = {}\npurpose = {}\n[[fragments]]\npath = \"foundation.toml\"\nsha256 = \"{}\"\nformat = \"oev2.design/1\"\n",
        serde_json::to_string(&d.version).unwrap(),
        serde_json::to_string(&d.problem).unwrap(),
        serde_json::to_string(&d.purpose).unwrap(),
        oe_core::sha256(&raw)
    );
    std::fs::write(&root, manifest).unwrap();
    (tmp, root, raw)
}
#[test]
fn composition2_preserves_complete_flat_semantics_and_original_field_locations() {
    let (tmp, root, raw) = fixture();
    let flat = load(&original()).unwrap();
    let composed = composition::load(tmp.path(), &root).unwrap();
    assert_eq!(
        flat.canonical().unwrap(),
        composed.design.canonical().unwrap()
    );
    assert_eq!(flat.rdf().unwrap(), composed.design.rdf().unwrap());
    assert_eq!(composed.snapshot.files.len(), 2);
    assert_eq!(composed.snapshot.files["foundation.toml"], raw);
    let normalized: serde_json::Value =
        serde_json::from_slice(&composed.normalized().unwrap()).unwrap();
    assert_eq!(normalized["schema"], "oev2.normalized-design/2");
    assert_eq!(normalized["input_schema"], "oev2.design/2");
    assert_eq!(normalized["design"], serde_json::to_value(&flat).unwrap());
    let file = composed.evidence["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["path"] == "foundation.toml")
        .unwrap();
    let locators = file["field_locators"].as_array().unwrap();
    assert!(locators.len() > 1000);
    for (index, locator) in locators.iter().enumerate() {
        let start = locator["byte_start"].as_u64().unwrap() as usize;
        let end = locator["byte_end"].as_u64().unwrap() as usize;
        assert!(start < end && end <= raw.len());
        let _ = std::str::from_utf8(&raw[start..end]).unwrap();
        if index % 97 == 0 || index + 1 == locators.len() {
            assert_eq!(
                locator["line_start"],
                1 + raw[..start].iter().filter(|b| **b == b'\n').count()
            );
        }
    }
    // Original source explicitly remains/1; no wrapper is resigned as that wire.
    assert_eq!(flat.schema, "oev2.design/1");
    assert_eq!(
        load(&root).unwrap().canonical().unwrap(),
        flat.canonical().unwrap()
    );
}

#[test]
fn composition3_preserves_ast_rdf_and_complete_source_reference() {
    let (tmp, root, raw) = fixture();
    let old = composition::load(tmp.path(), &root).unwrap();
    let old_normalized = old.normalized().unwrap();
    let manifest =
        std::fs::read_to_string(&root)
            .unwrap()
            .replacen("oev2.design/2", "oev2.design/3", 1);
    std::fs::write(&root, &manifest).unwrap();
    let current = composition::load(tmp.path(), &root).unwrap();
    assert_eq!(
        old.design.canonical().unwrap(),
        current.design.canonical().unwrap()
    );
    assert_eq!(old.design.rdf().unwrap(), current.design.rdf().unwrap());
    assert_eq!(
        old.normalized().unwrap(),
        old_normalized,
        "old loaded representation is not silently changed"
    );
    assert!(
        old.verify(tmp.path()).is_err(),
        "old root cannot be adopted as current"
    );
    let source = current.source_bytes().unwrap();
    let normalized: serde_json::Value =
        serde_json::from_slice(&current.normalized().unwrap()).unwrap();
    assert_eq!(normalized["schema"], "oev2.normalized-design/3");
    assert_eq!(normalized["input_schema"], "oev2.design/3");
    assert_eq!(normalized["semantic_ast_schema"], "oev2.design/1");
    assert_eq!(
        normalized["design"],
        serde_json::to_value(&old.design).unwrap()
    );
    assert!(normalized.get("source").is_none());
    assert_eq!(
        normalized["source_ref"]["source_map_product"],
        "design-composition-source.json"
    );
    assert_eq!(
        normalized["source_ref"]["source_map_sha256"],
        oe_core::sha256(&source)
    );
    assert_eq!(current.snapshot.files["foundation.toml"], raw);
    let before = old.evidence["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["path"] == "foundation.toml")
        .unwrap();
    let after = current.evidence["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["path"] == "foundation.toml")
        .unwrap();
    assert_eq!(
        before, after,
        "every original locator and byte identity retained"
    );
    current
        .verify_reference_source(&normalized, &source)
        .unwrap();
    assert_eq!(
        load(&root).unwrap().canonical().unwrap(),
        old.design.canonical().unwrap()
    );
}

#[test]
fn composition3_rejects_forged_reference_and_noncanonical_or_incomplete_source() {
    let (tmp, root, _) = fixture();
    let manifest =
        std::fs::read_to_string(&root)
            .unwrap()
            .replacen("oev2.design/2", "oev2.design/3", 1);
    std::fs::write(&root, manifest).unwrap();
    let current = composition::load(tmp.path(), &root).unwrap();
    let source = current.source_bytes().unwrap();
    let normalized: serde_json::Value =
        serde_json::from_slice(&current.normalized().unwrap()).unwrap();
    for (field, value) in [
        ("source_map_sha256", serde_json::json!("0".repeat(64))),
        ("source_map_product", serde_json::json!("../source.json")),
        ("files", serde_json::json!([])),
    ] {
        let mut changed = normalized.clone();
        changed["source_ref"][field] = value;
        assert!(current.verify_reference_source(&changed, &source).is_err());
    }
    let mut equivalent = source.clone();
    equivalent.push(b' ');
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&equivalent).unwrap(),
        current.evidence
    );
    assert!(
        current
            .verify_reference_source(&normalized, &equivalent)
            .is_err()
    );
    assert!(current.verify_reference_source(&normalized, b"").is_err());
    let mut missing = current.evidence.clone();
    missing["files"][0]["field_locators"]
        .as_array_mut()
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        current
            .verify_reference_source(
                &normalized,
                &oe_wire::json::canonical_json(&missing).unwrap()
            )
            .is_err()
    );
    let mut downgraded = normalized.clone();
    downgraded["schema"] = serde_json::json!("oev2.normalized-design/2");
    assert!(
        current
            .verify_reference_source(&downgraded, &source)
            .is_err()
    );
    std::fs::write(tmp.path().join("foundation.toml"), b"changed").unwrap();
    assert!(current.verify(tmp.path()).is_err());
}

#[test]
fn composition3_keeps_complete_source_budget_and_input_limits() {
    let (tmp, root, _) = fixture();
    let manifest =
        std::fs::read_to_string(&root)
            .unwrap()
            .replacen("oev2.design/2", "oev2.design/3", 1);
    std::fs::write(&root, &manifest).unwrap();
    let mut current = composition::load(tmp.path(), &root).unwrap();
    current.evidence["large_control"] = serde_json::json!("x".repeat(16 * 1024 * 1024));
    assert!(
        current
            .source_bytes()
            .unwrap_err()
            .to_string()
            .contains("composition_source_map_artifact_budget")
    );
    assert!(
        current
            .normalized()
            .unwrap_err()
            .to_string()
            .contains("composition_source_map_artifact_budget")
    );
    std::fs::write(
        &root,
        manifest.replacen("oev2.design/3", "oev2.design/4", 1),
    )
    .unwrap();
    assert!(composition::load(tmp.path(), &root).is_err());
    assert_eq!(composition::transport::FILE_BYTES, 4 * 1024 * 1024);
    assert_eq!(composition::transport::TOTAL_BYTES, 8 * 1024 * 1024);
    assert_eq!(composition::transport::FILE_COUNT, 32);
    assert_eq!(oe_core::source_limits::PUBLIC_FILE_BYTES, 16 * 1024 * 1024);
}
#[test]
fn composition2_rejects_missing_hash_path_link_duplicate_nested_and_budgets() {
    let (tmp, root, raw) = fixture();
    let manifest = std::fs::read_to_string(&root).unwrap();
    for mutated in [
        manifest.replace("foundation.toml", "missing.toml"),
        manifest.replace(&oe_core::sha256(&raw), &"a".repeat(64)),
        manifest.replace("foundation.toml", "../foundation.toml"),
        manifest.replace("foundation.toml", "/outside.toml"),
        manifest.replace("foundation.toml", "file://foundation.toml"),
        manifest.replace("format = \"oev2.design/1\"", "format = \"oev2.design/2\""),
        format!(
            "{manifest}\n{}",
            manifest[manifest.find("[[fragments]]").unwrap()..].to_owned()
        ),
        format!("{manifest}\nunread_input = true\n"),
    ] {
        std::fs::write(&root, mutated).unwrap();
        assert!(composition::load(tmp.path(), &root).is_err());
    }
    std::fs::write(&root, &manifest).unwrap();
    let loaded = composition::load(tmp.path(), &root).unwrap();
    std::fs::write(
        tmp.path().join("foundation.toml"),
        [raw.as_slice(), b"\n"].concat(),
    )
    .unwrap();
    assert!(loaded.verify(tmp.path()).is_err());
    std::fs::write(tmp.path().join("foundation.toml"), &raw).unwrap();
    let mut old = raw.clone();
    old.extend_from_slice(b"\nimports = []\n");
    let legacy = tmp.path().join("legacy-imports.toml");
    std::fs::write(&legacy, &old).unwrap();
    assert!(load(&legacy).is_err());
    let mut large = raw.clone();
    large.resize(4 * 1024 * 1024 + 1, b' ');
    std::fs::write(&legacy, &large).unwrap();
    assert!(load(&legacy).is_err());
    std::fs::write(tmp.path().join("foundation.toml"), &large).unwrap();
    assert!(composition::load(tmp.path(), &root).is_err());
    std::fs::write(tmp.path().join("foundation.toml"), &raw).unwrap();
    // Complete total/count boundaries are independent from each4MiB file cap.
    let empty = "schema=\"oev2.design-fragment/2\"\n";
    let mut count_manifest = manifest.clone();
    for i in 0..30 {
        let name = format!("empty{i}.toml");
        std::fs::write(tmp.path().join(&name), empty).unwrap();
        count_manifest.push_str(&format!(
            "\n[[fragments]]\npath={:?}\nsha256=\"{}\"\nformat=\"oev2.design-fragment/2\"\n",
            name,
            oe_core::sha256(empty)
        ));
    }
    std::fs::write(&root, &count_manifest).unwrap();
    assert_eq!(
        composition::load(tmp.path(), &root)
            .unwrap()
            .snapshot
            .files
            .len(),
        32
    );
    count_manifest.push_str(&format!("\n[[fragments]]\npath=\"empty30.toml\"\nsha256=\"{}\"\nformat=\"oev2.design-fragment/2\"\n",oe_core::sha256(empty)));
    std::fs::write(&root, count_manifest).unwrap();
    assert!(
        composition::load(tmp.path(), &root)
            .err()
            .unwrap()
            .to_string()
            .contains("file_count_budget")
    );
    let mut padding = empty.as_bytes().to_vec();
    padding.resize(4 * 1024 * 1024, b' ');
    std::fs::write(tmp.path().join("padding.toml"), &padding).unwrap();
    let smaller = [empty.as_bytes(), vec![b' '; 65536].as_slice()].concat();
    std::fs::write(tmp.path().join("extra-padding.toml"), &smaller).unwrap();
    let extended = format!(
        "{manifest}\n[[fragments]]\npath=\"padding.toml\"\nsha256=\"{}\"\nformat=\"oev2.design-fragment/2\"\n",
        oe_core::sha256(&padding)
    );
    std::fs::write(&root, &extended).unwrap();
    assert!(composition::load(tmp.path(), &root).is_ok());
    std::fs::write(&root,format!("{extended}\n[[fragments]]\npath=\"extra-padding.toml\"\nsha256=\"{}\"\nformat=\"oev2.design-fragment/2\"\n",oe_core::sha256(&smaller))).unwrap();
    assert!(
        composition::load(tmp.path(), &root)
            .err()
            .unwrap()
            .to_string()
            .contains("total_byte_budget")
    );
    std::fs::write(&root, &manifest).unwrap();
    // An actually repeated definition cannot be resolved by last-writer-wins.
    let duplicate = "schema = \"oev2.design-fragment/2\"\n[[requirements]]\nid=\"req_delivery\"\nfunction=\"duplicate\"\ncondition=\"duplicate\"\nacceptance=\"duplicate\"\n";
    let extra = tmp.path().join("duplicate.toml");
    std::fs::write(&extra, duplicate).unwrap();
    std::fs::write(&root,format!("{manifest}\n[[fragments]]\npath=\"duplicate.toml\"\nsha256=\"{}\"\nformat=\"oev2.design-fragment/2\"\n",oe_core::sha256(duplicate))).unwrap();
    assert!(composition::load(tmp.path(), &root).is_err());
    std::fs::write(&root, &manifest).unwrap();
    #[cfg(unix)]
    {
        std::fs::rename(
            tmp.path().join("foundation.toml"),
            tmp.path().join("real.toml"),
        )
        .unwrap();
        std::os::unix::fs::symlink("real.toml", tmp.path().join("foundation.toml")).unwrap();
        assert!(composition::load(tmp.path(), &root).is_err());
        std::fs::remove_file(tmp.path().join("foundation.toml")).unwrap();
        std::fs::hard_link(
            tmp.path().join("real.toml"),
            tmp.path().join("foundation.toml"),
        )
        .unwrap();
        assert!(composition::load(tmp.path(), &root).is_err());
    }
}
