use oe_design::{inspection, load};
use std::{fs, path::Path};
#[test]
fn interface_owner_and_real_production_impl_are_required_beyond_names_and_comments() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let d = load(&root.join("specs/engineering.toml")).unwrap();
    let original = inspection::inspect(root, &d).unwrap();
    assert!(!original["findings"].as_array().unwrap().iter().any(|f| {
        f["code"]
            .as_str()
            .is_some_and(|s| s.starts_with("interface_implementation"))
    }));
    let index = d
        .interfaces
        .iter()
        .position(|i| i.name == "EngineeringAdoptionPort")
        .unwrap();
    let mut outside = d.clone();
    outside.interfaces[index].module = "kernel".into();
    assert!(
        outside
            .validate()
            .unwrap_err()
            .to_string()
            .contains("interface_implementation_outside_owner")
    );
    let temp = tempfile::tempdir_in(root.join("var/builds")).unwrap();
    let path = temp
        .path()
        .join(d.interfaces[index].rust_path.as_ref().unwrap());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path,"// impl EngineeringAdoptionPort for AdoptionExecutor {}\n#[cfg(test)] mod test_only { impl EngineeringAdoptionPort for AdoptionExecutor {} }\n").unwrap();
    let report = inspection::inspect(temp.path(), &d).unwrap();
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "interface_implementation_missing"
                && f["subject"] == "interface_EngineeringAdoptionPort")
    );
}
