use oe_contracts::{NxAssemblySourceClosure, NxAssemblySourceFile};
use oe_nx_worker::motion_source::*;
fn file(path: &str) -> NxAssemblySourceFile {
    NxAssemblySourceFile {
        path: path.into(),
        bytes: 3,
        sha256: oe_nx_protocol::BlobIdentity::of(b"CAD")
            .sha256
            .as_str()
            .to_owned(),
    }
}
#[test]
fn closure_rejects_aliases_missing_root_and_changed_bytes() {
    let good = NxAssemblySourceClosure {
        schema: "oev2.nx-assembly-source-closure/1".into(),
        root_part: "root.prt".into(),
        files: vec![file("root.prt"), file("sub/part.prt")],
        max_total_bytes: 6,
    };
    validate_source_closure(&good).unwrap();
    verify_source_file(&good.files[0], b"CAD").unwrap();
    assert!(verify_source_file(&good.files[0], b"BAD").is_err());
    assert!(verify_source_file(&good.files[0], b"CAD!").is_err());
    for fault in 0..6 {
        let mut c = good.clone();
        match fault {
            0 => c.files[1].path = "ROOT.PRT".into(),
            1 => {
                c.files.push(file("sub"));
                c.max_total_bytes = 9;
            }
            2 => c.root_part = "missing.prt".into(),
            3 => c.max_total_bytes = 5,
            4 => c.files[1].path = "../escape.prt".into(),
            5 => c.files[1].sha256 = "wrong".into(),
            _ => unreachable!(),
        }
        assert!(validate_source_closure(&c).is_err(), "{fault}");
    }
}

#[test]
fn nested_directory_plan_is_parent_first_and_rejects_aliases() {
    let mut c = NxAssemblySourceClosure {
        schema: "oev2.nx-assembly-source-closure/1".into(),
        root_part: "root.prt".into(),
        files: vec![
            file("root.prt"),
            file("B/deep/part.prt"),
            file("A/leaf.prt"),
        ],
        max_total_bytes: 9,
    };
    validate_source_closure(&c).unwrap();
    assert_eq!(source_directories(&c).unwrap(), vec!["A", "B", "B/deep"]);
    c.files.push(file("b/other.prt"));
    c.max_total_bytes = 12;
    assert!(
        validate_source_closure(&c)
            .unwrap_err()
            .to_string()
            .contains("motion_source_directory_alias")
    );
}
