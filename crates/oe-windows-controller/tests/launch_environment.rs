use oe_windows_controller::launch_environment::worker_search_path;
use std::{ffi::OsString, path::PathBuf};

#[test]
fn nxbin_precedes_inherited_paths_without_modifying_the_parent() {
    let root = std::env::temp_dir().join("NX installation with spaces");
    let original = std::env::var_os("PATH");
    let inherited =
        std::env::join_paths([PathBuf::from("tool-a"), PathBuf::from("tool-b")]).unwrap();
    let child = worker_search_path(&root, Some(inherited)).unwrap();
    assert_eq!(
        std::env::split_paths(&child).collect::<Vec<_>>(),
        vec![
            root.join("NXBIN"),
            PathBuf::from("tool-a"),
            PathBuf::from("tool-b")
        ]
    );
    assert_eq!(std::env::var_os("PATH"), original);
    assert_eq!(
        std::env::split_paths(&worker_search_path(&root, None).unwrap()).collect::<Vec<_>>(),
        vec![root.join("NXBIN")]
    );
    assert!(worker_search_path(&root, Some(OsString::from(""))).is_ok());
}
