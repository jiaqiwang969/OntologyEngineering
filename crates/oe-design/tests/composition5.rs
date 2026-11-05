//! Application-fragment capacity is versioned; old identities/budgets stay fixed.
use oe_design::composition::{self, transport};
use std::path::Path;

fn fixture(count: usize, schema: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../var");
    std::fs::create_dir_all(&base).unwrap();
    let tmp = tempfile::tempdir_in(base.canonicalize().unwrap()).unwrap();
    let foundation = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/composition-r2-foundation.toml"),
    )
    .unwrap();
    std::fs::write(tmp.path().join("foundation.toml"), &foundation).unwrap();
    let original = oe_design::load(&tmp.path().join("foundation.toml")).unwrap();
    let mut root = format!(
        "schema={schema:?}\nversion={}\nproblem={}\npurpose={}\nvalidation_encoding=\"oev2.validation-groups/1\"\n[[fragments]]\npath=\"foundation.toml\"\nsha256=\"{}\"\nformat=\"oev2.design/1\"\n",
        serde_json::to_string(&original.version).unwrap(),
        serde_json::to_string(&original.problem).unwrap(),
        serde_json::to_string(&original.purpose).unwrap(),
        oe_core::sha256(foundation)
    );
    for i in 2..count {
        let text = "schema=\"oev2.design-fragment/2\"\n";
        std::fs::write(tmp.path().join(format!("extra{i}.toml")), text).unwrap();
        root.push_str(&format!("\n[[fragments]]\npath=\"extra{i}.toml\"\nsha256=\"{}\"\nformat=\"oev2.design-fragment/2\"\n",oe_core::sha256(text)));
    }
    let path = tmp.path().join("root.toml");
    std::fs::write(&path, root).unwrap();
    (tmp, path)
}

#[test]
fn composition5_capacity_is_versioned_and_all_original_bytes_bound() {
    for (schema, count, ok) in [
        ("oev2.design/4", 32, true),
        ("oev2.design/4", 33, false),
        ("oev2.design/5", 64, true),
        ("oev2.design/5", 65, false),
    ] {
        let (tmp, path) = fixture(count, schema);
        let value = transport::snapshot(tmp.path(), &path);
        assert_eq!(value.is_ok(), ok, "{schema} {count}");
        if ok {
            assert_eq!(value.unwrap().files.len(), count);
        } else {
            assert!(value.unwrap_err().contains("file_count_budget"));
        }
    }
    let (tmp, path) = fixture(33, "oev2.design/5");
    let loaded = composition::load(tmp.path(), &path).unwrap();
    assert_eq!(loaded.normalized_schema(), "oev2.normalized-design/5");
    assert_eq!(loaded.generation_schema(), "oev2.generation/5");
    assert_eq!(loaded.evidence["file_count_limit"], 64);
    let mut normalized: serde_json::Value =
        serde_json::from_slice(&loaded.normalized().unwrap()).unwrap();
    loaded
        .verify_reference_source(&normalized, &loaded.source_bytes().unwrap())
        .unwrap();
    normalized["schema"] = serde_json::json!("oev2.normalized-design/4");
    assert!(
        loaded
            .verify_reference_source(&normalized, &loaded.source_bytes().unwrap())
            .is_err()
    );
    std::fs::write(tmp.path().join("extra32.toml"), "changed").unwrap();
    assert!(loaded.verify(tmp.path()).is_err());
    assert!(
        transport::snapshot(tmp.path(), &path)
            .unwrap_err()
            .contains("sha_mismatch")
    );
}

#[test]
fn composition5_keeps_size_link_and_encoding_guards() {
    let (tmp, path) = fixture(2, "oev2.design/5");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        original.replace("oev2.validation-groups/1", "unknown"),
    )
    .unwrap();
    assert!(
        transport::snapshot(tmp.path(), &path)
            .unwrap_err()
            .contains("validation_encoding")
    );
    std::fs::write(
        &path,
        original.replace("foundation.toml", "../foundation.toml"),
    )
    .unwrap();
    assert!(
        transport::snapshot(tmp.path(), &path)
            .unwrap_err()
            .contains("unsafe_path")
    );
    std::fs::write(&path, original).unwrap();
    let mut large = b"schema=\"oev2.design/1\"\n".to_vec();
    large.resize(4 * 1024 * 1024 + 1, b' ');
    std::fs::write(tmp.path().join("foundation.toml"), large).unwrap();
    assert!(transport::snapshot(tmp.path(), &path).is_err());
    assert_eq!(transport::FILE_BYTES, 4 * 1024 * 1024);
    assert_eq!(transport::TOTAL_BYTES, 8 * 1024 * 1024);
}

#[test]
fn composition5_total_size_extension_does_not_change_legacy_budget() {
    let (tmp, path) = fixture(2, "oev2.design/5");
    let mut root = std::fs::read_to_string(&path).unwrap();
    let mut padding = b"schema=\"oev2.design-fragment/2\"\n".to_vec();
    padding.resize(4 * 1024 * 1024, b' ');
    for i in 0..3 {
        std::fs::write(tmp.path().join(format!("padding{i}.toml")), &padding).unwrap();
        root.push_str(&format!("\n[[fragments]]\npath=\"padding{i}.toml\"\nsha256=\"{}\"\nformat=\"oev2.design-fragment/2\"\n",oe_core::sha256(&padding)));
        if i == 1 {
            std::fs::write(&path, &root).unwrap();
            let current = transport::snapshot(tmp.path(), &path).unwrap();
            assert!(
                current.total_bytes > 8 * 1024 * 1024 && current.total_bytes <= 12 * 1024 * 1024
            );
            std::fs::write(&path, root.replace("oev2.design/5", "oev2.design/4")).unwrap();
            assert!(
                transport::snapshot(tmp.path(), &path)
                    .unwrap_err()
                    .contains("total_byte_budget")
            );
        }
    }
    std::fs::write(&path, root).unwrap();
    assert!(
        transport::snapshot(tmp.path(), &path)
            .unwrap_err()
            .contains("total_byte_budget")
    );
}
