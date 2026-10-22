//! Independently fixed SDK4 content controls. Current SDK and R5 originals are
//! read only; mutations occur exclusively in temporary synthetic fixtures.
use oe_core::{SkillRoot, sha256};
use oe_evolution::{native_sdk4::*, *};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn mode(path: &Path) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        u64::from(
            std::fs::symlink_metadata(path)
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
        )
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        panic!("Unix SDK4 fixture required")
    }
}
fn pin(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap();
    json!({"path":path,"sha256":sha256(&bytes),"bytes":bytes.len(),"mode":mode(path)})
}
fn link(path: &Path, text: &str) {
    if std::fs::symlink_metadata(path).is_ok() {
        std::fs::remove_file(path).unwrap();
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(text, path).unwrap();
    #[cfg(not(unix))]
    {
        let _ = (path, text);
        panic!("Unix SDK4 fixture required")
    }
}
fn edge(role: &str, path: &Path, target: &Value) -> Value {
    let text = std::fs::read_link(path)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    json!({"role":role,"link_path":path,"link_text":text,"link_text_sha256":sha256(text.as_bytes()),"link_mode":mode(path),"target":target})
}
fn private(root: &SkillRoot, value: &Value) -> ArtifactReference {
    let path = root.path().join("var/layout.json");
    let bytes = serde_json::to_vec(value).unwrap();
    write(&path, &bytes);
    ArtifactReference {
        path: path.to_str().unwrap().into(),
        sha256: sha256(&bytes),
    }
}
fn tbd(install: &str, reexport: bool) -> String {
    format!(
        "--- !tapi-tbd\ntbd-version: 4\ntargets: [ x86_64-macos, arm64e-macos ]\ninstall-name: '{install}'\n{}exports:\n  - targets: [ x86_64-macos, arm64e-macos ]\n    symbols: [ _finite_independent_symbol ]\n...\n",
        if reexport {
            "reexported-libraries:\n  - targets: [ x86_64-macos, arm64e-macos ]\n    libraries: [ '/usr/lib/libcharset.1.dylib' ]\n"
        } else {
            ""
        }
    )
}
struct Fixture {
    _temp: TempDir,
    root: SkillRoot,
    sdk: PathBuf,
    layout: Value,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        write(
            &path.join("SKILL.md"),
            b"# independent finite SDK4 fixture\n",
        );
        write(&path.join("Cargo.toml"), b"[workspace]\nmembers=[]\n");
        let root = SkillRoot::new(&path).unwrap();
        let sdk = path.join("var/SDK");
        let mut files = Vec::new();
        for (role, rel, body) in [
            (
                "SDKSettings",
                "SDKSettings.json",
                "{\"synthetic\":true}\n".into(),
            ),
            (
                "libSystem_tbd",
                "usr/lib/libSystem.B.tbd",
                tbd("/usr/lib/libSystem.B.dylib", false),
            ),
            (
                "libiconv_tbd",
                "usr/lib/libiconv.2.tbd",
                tbd("/usr/lib/libiconv.2.dylib", true),
            ),
            (
                "libcharset_tbd",
                "usr/lib/libcharset.1.tbd",
                tbd("/usr/lib/libcharset.1.dylib", false),
            ),
        ] {
            let file = sdk.join(rel);
            write(&file, body.as_bytes());
            files.push(json!({"role":role,"file":pin(&file)}));
        }
        let mut aliases = Vec::new();
        for (role, name, raw, target) in [
            ("libSystem_tbd", "libSystem.tbd", "libSystem.B.tbd", 1),
            ("libc_tbd", "libc.tbd", "libSystem.tbd", 1),
            ("libm_tbd", "libm.tbd", "libSystem.tbd", 1),
            ("libproc_tbd", "libproc.tbd", "libSystem.tbd", 1),
            ("libiconv_tbd", "libiconv.tbd", "libiconv.2.tbd", 2),
            ("libcharset_tbd", "libcharset.tbd", "libcharset.1.tbd", 3),
        ] {
            let file = sdk.join("usr/lib").join(name);
            link(&file, raw);
            aliases.push(edge(role, &file, &files[target]["file"]));
        }
        let layout = json!({"schema":"oev2.native-sdk-layout/4","canonical_root":sdk,"files":files,"aliases":aliases});
        Self {
            _temp: temp,
            root,
            sdk,
            layout,
        }
    }
    fn inspect(&self) -> anyhow::Result<CodeNativeSdkLayout4> {
        inspect_sdk4(&self.root, &private(&self.root, &self.layout))
    }
    fn rebind_file(&mut self, ordinal: usize, body: &[u8]) {
        let path = PathBuf::from(
            self.layout["files"][ordinal]["file"]["path"]
                .as_str()
                .unwrap(),
        );
        let old = self.layout["files"][ordinal]["file"].clone();
        write(&path, body);
        let new = pin(&path);
        self.layout["files"][ordinal]["file"] = new.clone();
        for alias in self.layout["aliases"].as_array_mut().unwrap() {
            if alias["target"] == old {
                alias["target"] = new.clone();
            }
        }
    }
    fn rebind_edge(&mut self, ordinal: usize, raw: &str) {
        let path = PathBuf::from(
            self.layout["aliases"][ordinal]["link_path"]
                .as_str()
                .unwrap(),
        );
        link(&path, raw);
        let role = self.layout["aliases"][ordinal]["role"].as_str().unwrap();
        self.layout["aliases"][ordinal] =
            edge(role, &path, &self.layout["aliases"][ordinal]["target"]);
    }
}
#[test]
fn actual_readonly_sdk4_and_complete_original_tbd_documents() {
    let path = PathBuf::from(
        std::env::var("OE_NATIVE_ACTUAL_SDK4_LAYOUT")
            .expect("explicit frozen actual SDK4 source required"),
    );
    let bytes = std::fs::read(&path).unwrap();
    let expected: Value = serde_json::from_slice(&bytes).unwrap();
    let f = Fixture::new();
    let layout = NativeSdkInspector4 { root: &f.root }
        .inspect_sdk4(&private(&f.root, &expected))
        .unwrap();
    assert_eq!(serde_json::to_value(&layout).unwrap(), expected);
    assert_eq!(layout.files.len(), 4);
    assert_eq!(layout.aliases.len(), 6);
    let metadata = inspect_tbd_metadata4(&layout).unwrap();
    assert_eq!(metadata.len(), 42);
    for (role, count) in [
        ("libSystem_tbd", 40),
        ("libiconv_tbd", 1),
        ("libcharset_tbd", 1),
    ] {
        let documents: Vec<_> = metadata.iter().filter(|d| d.file_role == role).collect();
        assert_eq!(documents.len(), count);
        let source = &documents[0].source;
        let raw = std::fs::read(&source.path).unwrap();
        let mut recovered = Vec::new();
        for (i, d) in documents.iter().enumerate() {
            assert_eq!(d.document_ordinal, i as u64);
            assert_eq!(d.byte_start, recovered.len() as u64);
            let slice = &raw[d.byte_start as usize..d.byte_end as usize];
            assert_eq!(sha256(slice), d.raw_document_sha256);
            recovered.extend_from_slice(slice);
        }
        assert_eq!(recovered, raw);
        assert_eq!(
            documents[0].targets,
            vec![
                "x86_64-macos",
                "x86_64-maccatalyst",
                "arm64e-macos",
                "arm64e-maccatalyst"
            ]
        );
        assert!(!documents[0].targets.iter().any(|t| t == "arm64-macos"));
    }
    assert_eq!(
        metadata[0].content["reexported-libraries"][0]["libraries"]
            .as_array()
            .unwrap()
            .len(),
        39
    );
    let kernel = metadata
        .iter()
        .find(|d| {
            d.file_role == "libSystem_tbd"
                && d.install_name == "/usr/lib/system/libsystem_kernel.dylib"
        })
        .unwrap();
    for symbol in ["_proc_pidpath", "_proc_pidinfo", "_proc_name"] {
        assert!(
            kernel.content["exports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|g| g["targets"]
                    .as_array()
                    .is_some_and(|ts| ts.iter().any(|t| t == "arm64-macos"))
                    && g["symbols"]
                        .as_array()
                        .is_some_and(|ss| ss.iter().any(|s| s == symbol)))
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    for name in [
        "execution",
        "native_available",
        "compilation_origin",
        "actual_runtime_load",
        "owner_observed",
    ] {
        assert!(expected.get(name).is_none());
    }
}
#[test]
fn synthetic_four_files_six_aliases_and_ordered_library_occurrences() {
    let f = Fixture::new();
    let layout = f.inspect().unwrap();
    assert_eq!(serde_json::to_value(&layout).unwrap(), f.layout);
    let argv = [
        "object.o",
        "-lproc",
        "-liconv",
        "-lSystem",
        "-lc",
        "-lm",
        "-lproc",
        "-lcharset",
    ]
    .map(str::to_owned);
    let selected = bind_selected_libraries4(&layout, &argv).unwrap();
    assert_eq!(selected.len(), 7);
    for (ordinal, row) in selected.iter().enumerate() {
        assert_eq!(row.argument_ordinal, (ordinal + 1) as u64);
        assert_eq!(row.argument, argv[ordinal + 1]);
    }
    assert_eq!(
        selected[0].alias_roles,
        vec!["libproc_tbd", "libSystem_tbd"]
    );
    assert_eq!(selected[0].target, layout.files[1].file);
    assert_eq!(selected[1].alias_roles, vec!["libiconv_tbd"]);
    assert_eq!(selected[1].target, layout.files[2].file);
    assert_eq!(
        selected[1].tbd_dependencies,
        vec![layout.files[3].file.clone()]
    );
    for (i, row) in selected.iter().enumerate() {
        if i != 1 {
            assert!(row.tbd_dependencies.is_empty());
        }
    }
    assert_eq!(selected[5].argument, selected[0].argument);
    assert_ne!(selected[5].argument_ordinal, selected[0].argument_ordinal);
    assert_eq!(selected[6].target, layout.files[3].file);
    assert!(
        bind_selected_libraries4(&layout, &["-o".into(), "-lproc".into()])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn strict_sdk4_json_roles_and_authority_fields() {
    for case in 0..20 {
        let mut f = Fixture::new();
        match case {
            0 => f.layout["schema"] = json!("oev2.native-sdk-layout/3"),
            1 => {
                f.layout.as_object_mut().unwrap().remove("files");
            }
            2 => f.layout["owner_observed"] = json!(true),
            3 => f.layout["native_available"] = json!(true),
            4 => {
                f.layout["files"].as_array_mut().unwrap().pop();
            }
            5 => f.layout["files"][3]["role"] = json!("libSystem_tbd"),
            6 => {
                f.layout["aliases"].as_array_mut().unwrap().remove(0);
            }
            7 => f.layout["aliases"][3]["role"] = json!("libc_tbd"),
            8 => f.layout["aliases"][3]["role"] = json!("unknown"),
            9 => f.layout["aliases"][3]["link_mode"] = json!("755"),
            10 => f.layout["aliases"][3]["link_text_sha256"] = json!("0".repeat(64)),
            11 => f.layout["files"][2]["file"]["bytes"] = json!(0),
            12 => f.layout["aliases"][3]["trusted"] = json!(true),
            13 => f.layout["files"][2]["file"]["sha256"] = json!("0".repeat(64)),
            14 => f.layout["files"][2]["file"]["mode"] = json!(0),
            15 => f.layout["files"][2]["role"] = json!("libproc_tbd"),
            16 => f.layout["aliases"][4]["target"] = f.layout["files"][1]["file"].clone(),
            17 => f.layout["aliases"][5]["target"] = f.layout["files"][2]["file"].clone(),
            18 => f.layout["compilation_origin"] = json!(null),
            _ => f.layout["aliases"] = json!(null),
        }
        let failure = NativeSdkInspector4 { root: &f.root }
            .inspect_sdk4(&private(&f.root, &f.layout))
            .unwrap_err();
        let v = failure.partial_result.unwrap();
        assert_eq!(v["execution"], "not_run", "case={case}");
        assert_eq!(v["compilation_origin"], "unknown");
        assert_eq!(v["actual_runtime_load"], "not_observed");
        assert_eq!(v["sdk_closure"], "unknown");
        assert_eq!(v["engineering_adoption"], "not_established");
    }
    let f = Fixture::new();
    let path = f.root.path().join("var/duplicate.json");
    let raw = br#"{"schema":"oev2.native-sdk-layout/4","schema":"oev2.native-sdk-layout/4"}"#;
    write(&path, raw);
    assert!(
        inspect_sdk4(
            &f.root,
            &ArtifactReference {
                path: path.to_str().unwrap().into(),
                sha256: sha256(raw)
            }
        )
        .is_err()
    );
}
#[test]
fn same_size_regular_target_mutation_is_not_cached() {
    for ordinal in 0..4 {
        let f = Fixture::new();
        f.inspect().unwrap();
        let path = PathBuf::from(f.layout["files"][ordinal]["file"]["path"].as_str().unwrap());
        let mut raw = std::fs::read(&path).unwrap();
        let len = raw.len();
        raw[0] ^= 1;
        write(&path, &raw);
        assert_eq!(raw.len(), len);
        assert!(f.inspect().is_err());
    }
}
#[test]
fn wrong_raw_edge_mode_target_and_graph_are_rejected() {
    for case in 0..10 {
        let mut f = Fixture::new();
        match case {
            0 => f.rebind_edge(3, "libiconv.tbd"),
            1 => f.rebind_edge(4, "libSystem.tbd"),
            2 => f.rebind_edge(5, "libiconv.2.tbd"),
            3 => f.rebind_edge(0, "libproc.tbd"),
            4 => f.rebind_edge(3, "/usr/lib/libSystem.tbd"),
            5 => f.rebind_edge(3, "../lib/libSystem.tbd"),
            6 => f.layout["aliases"][3]["link_mode"] = json!(0),
            7 => f.layout["aliases"][3]["link_path"] = json!(f.sdk.join("usr/lib/libunknown.tbd")),
            8 => link(&f.sdk.join("usr/lib/libproc.tbd"), "libSysten.tbd"),
            _ => {
                let path = f.sdk.join("usr/lib/libproc.tbd");
                std::fs::remove_file(&path).unwrap();
                write(&path, b"regular is not leaf alias");
            }
        }
        assert!(f.inspect().is_err(), "case={case}");
    }
}
#[test]
fn noncanonical_paths_and_symlink_parents_are_rejected() {
    let mut f = Fixture::new();
    f.layout["canonical_root"] = json!(f.sdk.join("."));
    assert!(f.inspect().is_err());
    let mut f = Fixture::new();
    let copied = f.sdk.join("usr/lib/other-iconv.2.tbd");
    write(
        &copied,
        &std::fs::read(f.sdk.join("usr/lib/libiconv.2.tbd")).unwrap(),
    );
    f.layout["files"][2]["file"] = pin(&copied);
    assert!(f.inspect().is_err());
    let f = Fixture::new();
    let lib = f.sdk.join("usr/lib");
    let relocated = f.sdk.join("usr/moved-lib");
    std::fs::rename(&lib, &relocated).unwrap();
    link(&lib, "moved-lib");
    assert!(f.inspect().is_err());
}
#[test]
fn all_six_names_and_three_shadow_extensions_are_rejected() {
    let f = Fixture::new();
    let search = f.root.path().join("var/search");
    std::fs::create_dir(&search).unwrap();
    check_library_search_paths4(&[search.clone()]).unwrap();
    for name in ["System", "c", "m", "proc", "iconv", "charset"] {
        for extension in ["a", "dylib", "tbd"] {
            let path = search.join(format!("lib{name}.{extension}"));
            for dangling in [false, true] {
                if dangling {
                    link(&path, "missing");
                } else {
                    write(&path, b"shadow");
                }
                assert!(
                    check_library_search_paths4(&[search.clone()]).is_err(),
                    "{name}.{extension} dangling={dangling}"
                );
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    check_library_search_paths4(&[search]).unwrap();
    assert!(check_library_search_paths4(&[PathBuf::from("relative")]).is_err());
}
#[test]
fn unselected_and_forwarded_library_arguments_are_rejected() {
    let f = Fixture::new();
    let layout = f.inspect().unwrap();
    for argv in [
        vec!["-lssl"],
        vec!["-l"],
        vec!["-l", "proc"],
        vec!["-l:libproc.tbd"],
        vec!["-Wl,-lproc"],
        vec!["-Xlinker", "-liconv"],
        vec!["-o"],
    ] {
        assert!(
            bind_selected_libraries4(
                &layout,
                &argv.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            )
            .is_err(),
            "{argv:?}"
        );
    }
}
#[test]
fn tbd_duplicate_version_install_and_charset_relation_are_rejected() {
    for case in 0..6 {
        let mut f = Fixture::new();
        let original = tbd("/usr/lib/libiconv.2.dylib", true);
        let changed = match case {
            0 => original.replace("tbd-version: 4", "tbd-version: 4\ntbd-version: 4"),
            1 => original.replace("tbd-version: 4", "tbd-version: 3"),
            2 => original.replace(
                "'/usr/lib/libiconv.2.dylib'",
                "'/usr/lib/libSystem.B.dylib'",
            ),
            3 => original.replace(
                "'/usr/lib/libcharset.1.dylib'",
                "'/usr/lib/libSystem.B.dylib'",
            ),
            4 => original.replace("--- !tapi-tbd", "--- !different"),
            _ => original.replace(
                "targets: [ x86_64-macos, arm64e-macos ]",
                "targets: [ 1, arm64e-macos ]",
            ),
        };
        f.rebind_file(2, changed.as_bytes());
        let layout = f.inspect().unwrap();
        assert!(inspect_tbd_metadata4(&layout).is_err(), "case={case}");
        assert!(
            bind_selected_libraries4(&layout, &["-liconv".into()]).is_err(),
            "case={case}"
        );
    }
}
#[test]
fn sdk3_and_generic_regular_guards_keep_their_original_domain() {
    let f = Fixture::new();
    assert!(native_sdk::inspect_sdk(&f.root, &private(&f.root, &f.layout)).is_err());
    for item in f.layout["aliases"].as_array().unwrap() {
        let path = PathBuf::from(item["link_path"].as_str().unwrap());
        let pin: CodeNativeFileBinding = serde_json::from_value(pin(&path)).unwrap();
        assert!(native_profile::verify_file(&pin).is_err());
    }
}
#[test]
fn original_five_R5_argv_bindings_remain_distinct_from_loading() {
    let p = PathBuf::from(
        std::env::var("OE_NATIVE_SDK4_R5_ROOT").expect("explicit original R5 source root required"),
    );
    let path = PathBuf::from(std::env::var("OE_NATIVE_ACTUAL_SDK4_LAYOUT").unwrap());
    let raw = std::fs::read(&path).unwrap();
    let f = Fixture::new();
    let layout = inspect_sdk4(
        &f.root,
        &private(&f.root, &serde_json::from_slice(&raw).unwrap()),
    )
    .unwrap();
    for (name, flags) in [
        ("system-baseline", vec!["-lSystem"]),
        ("proc-only", vec!["-lproc"]),
        ("iconv-only", vec!["-liconv"]),
        ("proc-iconv", vec!["-lproc", "-liconv"]),
        ("iconv-charset-reexport", vec!["-liconv"]),
    ] {
        let source = p.join(format!(
            "ACTUAL-SELECTED-CLANG-LINK-{name}-01-FROZEN-INPUTS.json"
        ));
        let original = std::fs::read(&source).unwrap();
        let value: Value = serde_json::from_slice(&original).unwrap();
        let argv: Vec<String> = serde_json::from_value(value["argv"].clone()).unwrap();
        let rows = bind_selected_libraries4(&layout, &argv).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.argument.as_str()).collect::<Vec<_>>(),
            flags
        );
        for row in &rows {
            assert_eq!(argv[row.argument_ordinal as usize], row.argument);
        }
        if name == "iconv-charset-reexport" {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].tbd_dependencies, vec![layout.files[3].file.clone()]);
        }
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
}
