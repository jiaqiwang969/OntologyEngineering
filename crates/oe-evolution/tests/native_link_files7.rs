use oe_contracts::*;
use oe_core::{SkillRoot, write_new_bytes};
use oe_evolution::native_archive::{ALGORITHM, bind_file, prepare_link_inputs};
use oe_evolution::native_link_files7::*;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../var/maintenance/link-file-content7-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let p = parent.canonicalize().unwrap().join(format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        write_new_bytes(
            &p.join("SKILL.md"),
            b"# Isolated finite linker source content test\n",
        )
        .unwrap();
        std::fs::create_dir(p.join("var")).unwrap();
        std::fs::create_dir(p.join("var/target")).unwrap();
        Self {
            root: SkillRoot::new(p).unwrap(),
        }
    }
    fn path(&self, s: &str) -> PathBuf {
        self.root.path().join(s)
    }
    fn write(&self, s: &str, b: &[u8]) -> PathBuf {
        let p = self.path(s);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        write_new_bytes(&p, b).unwrap();
        p
    }
    fn request(&self, mut args: Vec<String>) -> CodeNativeLinkFileInputRequest7 {
        args.extend([
            "-nodefaultlibs".into(),
            "-o".into(),
            self.path("var/target/result").to_str().unwrap().into(),
        ]);
        CodeNativeLinkFileInputRequest7 {
            schema: "oev2.native-link-file-input-request/7".into(),
            received_arguments: args,
            caller_cwd: self.root.path().to_str().unwrap().into(),
            target_root: self.path("var/target").to_str().unwrap().into(),
            capture_root: self.path("var/captured").to_str().unwrap().into(),
        }
    }
    fn old(
        &self,
        r: &CodeNativeLinkFileInputRequest7,
        args: Vec<String>,
    ) -> CodeNativeInputPreparationRequest {
        CodeNativeInputPreparationRequest {
            schema: "oev2.native-input-preparation-request/3".into(),
            algorithm: ALGORITHM.into(),
            received_arguments: args,
            caller_cwd: r.caller_cwd.clone(),
            target_root: r.target_root.clone(),
            stage_root: self.path("var/stage").to_str().unwrap().into(),
            evidence_root: self.path("var/evidence").to_str().unwrap().into(),
            external_archives: vec![],
            index_executable: bind_file(Path::new("/usr/bin/true")).unwrap(),
            index_environment: vec![],
        }
    }
}

#[test]
fn exact_original_duplicate_bytes_and_args_borrow_old_kernel() {
    let f = Fixture::new();
    let content = b"# literal order stays\n_symbol2\n_symbol1\n_symbol2\n";
    let file = f.write("var/target/rustc/list", content);
    let path = file.to_str().unwrap();
    let r = f.request(vec![
        "-Wl,-exported_symbols_list".into(),
        format!("-Wl,{path}"),
        "-dynamiclib".into(),
        "-exported_symbols_list".into(),
        path.into(),
    ]);
    let old = f.old(&r, r.received_arguments.clone());
    let error = prepare_link_inputs(&f.root, &old, &mut |_| {
        panic!("no archive callback permitted")
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("native_input_hidden_file_option_unsupported"));
    assert!(!f.path("var/stage").exists());
    assert!(!f.path("var/evidence").exists());
    let facts = BorrowedNativeLinkFiles7 { root: &f.root }
        .prepare_link_file_inputs7(&r)
        .unwrap();
    assert_eq!(facts.request, r);
    assert_eq!(facts.files.len(), 2);
    assert_eq!(facts.base_argument_ordinals, vec![2, 5, 6, 7]);
    assert_eq!(facts.files[0].original_file, facts.files[1].original_file);
    assert_ne!(
        facts.files[0].captured_file.path,
        facts.files[1].captured_file.path
    );
    for input in &facts.files {
        assert_eq!(std::fs::read(&input.captured_file.path).unwrap(), content);
    }
    assert_eq!(std::fs::read(&file).unwrap(), content);
    let old = f.old(&r, facts.base_arguments.clone());
    let base = prepare_link_inputs(&f.root, &old, &mut |_| {
        panic!("no archive callback permitted")
    })
    .unwrap();
    let assembled = execution_arguments7(&f.root, &facts, &base).unwrap();
    let mut expected = r.received_arguments.clone();
    expected[1] = format!("-Wl,{}", facts.files[0].captured_file.path);
    expected[4] = facts.files[1].captured_file.path.clone();
    assert_eq!(assembled, expected);
    let mut changed = base;
    changed.request.received_arguments[0] = "-arbitrary".into();
    assert!(execution_arguments7(&f.root, &facts, &changed).is_err());
}

#[test]
fn current_source_change_and_removal_never_restore_availability() {
    let f = Fixture::new();
    let file = f.write("var/target/list", b"_original\n");
    let r = f.request(vec![
        "-exported_symbols_list".into(),
        file.to_str().unwrap().into(),
    ]);
    let facts = prepare_link_file_inputs7(&f.root, &r).unwrap();
    std::fs::write(&file, b"_changed_\n").unwrap();
    assert!(verify_live7(&f.root, &facts).is_err());
    verify_retained7(&f.root, &facts).unwrap();
    std::fs::remove_file(&file).unwrap();
    assert!(verify_live7(&f.root, &facts).is_err());
    verify_retained7(&f.root, &facts).unwrap();
}

#[test]
fn complete_capture_byte_and_mode_drift_are_refused() {
    let f = Fixture::new();
    let file = f.write("var/target/list", b"_stable\n");
    let r = f.request(vec![
        "-exported_symbols_list".into(),
        file.to_str().unwrap().into(),
    ]);
    let facts = prepare_link_file_inputs7(&f.root, &r).unwrap();
    std::fs::write(&facts.files[0].captured_file.path, b"_other_\n").unwrap();
    assert!(verify_retained7(&f.root, &facts).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&facts.files[0].captured_file.path, b"_stable\n").unwrap();
        std::fs::set_permissions(
            &facts.files[0].captured_file.path,
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(verify_retained7(&f.root, &facts).is_err());
    }
}

#[test]
fn malformed_option_and_missing_file_refuse_before_capture() {
    let table = [
        vec!["-exported_symbols_list".into()],
        vec![
            "-Wl,-exported_symbols_list".into(),
            "var/target/list".into(),
        ],
        vec!["-Wl,-exported_symbols_list,var/target/list".into()],
        vec!["-exported_symbols_list=var/target/list".into()],
        vec!["-exported_symbols_list".into(), "@response".into()],
        vec!["-exported_symbols_list".into(), "var/target/missing".into()],
    ];
    for args in table {
        let f = Fixture::new();
        f.write("var/target/list", b"_plain\n");
        let r = f.request(args);
        assert!(prepare_link_file_inputs7(&f.root, &r).is_err());
        assert!(!f.path("var/captured").exists());
    }
}

#[test]
fn target_escape_traversal_and_symlink_never_capture() {
    for value in ["var/outside", "var/target/../outside", "var/target/./list"] {
        let f = Fixture::new();
        f.write("var/outside", b"_outside\n");
        f.write("var/target/list", b"_inside\n");
        let r = f.request(vec!["-exported_symbols_list".into(), value.into()]);
        assert!(prepare_link_file_inputs7(&f.root, &r).is_err(), "{value}");
        assert!(!f.path("var/captured").exists());
    }
    #[cfg(unix)]
    {
        let f = Fixture::new();
        f.write("var/target/list", b"_inside\n");
        std::os::unix::fs::symlink(f.path("var/target/list"), f.path("var/target/alias")).unwrap();
        let r = f.request(vec![
            "-exported_symbols_list".into(),
            "var/target/alias".into(),
        ]);
        assert!(prepare_link_file_inputs7(&f.root, &r).is_err());
        assert!(!f.path("var/captured").exists());
    }
}

#[test]
fn original_occurrences_and_scope_cannot_be_rewritten() {
    let f = Fixture::new();
    let file = f.write("var/target/list", b"_inside\n");
    let r = f.request(vec![
        "-exported_symbols_list".into(),
        file.to_str().unwrap().into(),
    ]);
    let facts = prepare_link_file_inputs7(&f.root, &r).unwrap();
    let mut changed = facts.clone();
    changed.files[0].value_argument_ordinal += 1;
    assert!(verify_retained7(&f.root, &changed).is_err());
    let mut changed = facts.clone();
    changed.base_argument_ordinals.reverse();
    assert!(verify_retained7(&f.root, &changed).is_err());
    let mut changed = facts.clone();
    changed.uncovered.clear();
    assert!(verify_retained7(&f.root, &changed).is_err());
    let mut changed = facts;
    changed.files[0].raw_bytes.path = "var/target/list".into();
    assert!(verify_retained7(&f.root, &changed).is_err());
    assert!(prepare_link_file_inputs7(&f.root, &r).is_err());
}

#[test]
fn empty_symbol_list_is_complete_content_and_not_a_missing_source() {
    let f = Fixture::new();
    let file = f.write("var/target/empty", b"");
    let r = f.request(vec![
        "-exported_symbols_list".into(),
        file.to_str().unwrap().into(),
    ]);
    let facts = prepare_link_file_inputs7(&f.root, &r).unwrap();
    assert_eq!(facts.files[0].original_file.bytes, 0);
    assert_eq!(facts.files[0].captured_file.bytes, 0);
    verify_live7(&f.root, &facts).unwrap();
}

#[test]
fn file_count_and_full_byte_budgets_refuse_without_capture() {
    let f = Fixture::new();
    let file = f.write("var/target/list", b"_x\n");
    let args = (0..129)
        .flat_map(|_| {
            vec![
                "-exported_symbols_list".into(),
                file.to_str().unwrap().into(),
            ]
        })
        .collect();
    assert!(prepare_link_file_inputs7(&f.root, &f.request(args)).is_err());
    assert!(!f.path("var/captured").exists());
    let large = f.write("var/target/large", &vec![b'x'; 16 * 1024 * 1024 + 1]);
    let r = f.request(vec![
        "-exported_symbols_list".into(),
        large.to_str().unwrap().into(),
    ]);
    assert!(prepare_link_file_inputs7(&f.root, &r).is_err());
    assert!(!f.path("var/captured").exists());
}
