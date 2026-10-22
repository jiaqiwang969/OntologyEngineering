//! Fixed SDK layout /3 CONTENT cases. Actual SDK is read only; all mutations are
//! synthetic private fixtures. No SDK compilation or owner authority is implied.
use oe_core::{SkillRoot, sha256};
use oe_evolution::{native_profile::verify_file, native_sdk::*, *};
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
        panic!("SDK alias fixture requires Unix");
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
        panic!("SDK alias fixture requires Unix");
    }
}
fn edge(role: &str, path: &Path, target: &Value) -> Value {
    let raw = std::fs::read_link(path).unwrap();
    let text = raw.to_str().unwrap();
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
struct Fixture {
    _temp: TempDir,
    root: SkillRoot,
    sdk: PathBuf,
    layout: Value,
}
impl Fixture {
    fn new(system_alias: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        write(&path.join("SKILL.md"), b"# finite SDK content fixture\n");
        write(&path.join("Cargo.toml"), b"[workspace]\nmembers=[]\n");
        let root = SkillRoot::new(&path).unwrap();
        let sdk = path.join("var/SDK");
        write(
            &sdk.join("SDKSettings.json"),
            b"{\"finiteSyntheticSDK\":true}\n",
        );
        let system = sdk.join(if system_alias {
            "usr/lib/libSystem.B.tbd"
        } else {
            "usr/lib/libSystem.tbd"
        });
        write(&system, b"syntheticSystemPayloadA\n");
        if system_alias {
            link(&sdk.join("usr/lib/libSystem.tbd"), "libSystem.B.tbd");
        }
        link(&sdk.join("usr/lib/libc.tbd"), "libSystem.tbd");
        link(&sdk.join("usr/lib/libm.tbd"), "libSystem.tbd");
        let target = pin(&system);
        let mut aliases = Vec::new();
        if system_alias {
            aliases.push(edge(
                "libSystem_tbd",
                &sdk.join("usr/lib/libSystem.tbd"),
                &target,
            ));
        }
        aliases.push(edge("libc_tbd", &sdk.join("usr/lib/libc.tbd"), &target));
        aliases.push(edge("libm_tbd", &sdk.join("usr/lib/libm.tbd"), &target));
        let layout = json!({"schema":"oev2.native-sdk-layout/3","canonical_root":sdk,"files":[{"role":"SDKSettings","file":pin(&sdk.join("SDKSettings.json"))},{"role":"libSystem_tbd","file":target}],"aliases":aliases});
        Self {
            _temp: temp,
            root,
            sdk,
            layout,
        }
    }
    fn inspect(&self) -> anyhow::Result<CodeNativeSdkLayout3> {
        inspect_sdk(&self.root, &private(&self.root, &self.layout))
    }
    fn rebind_link(&mut self, role: &str, text: &str) {
        let path = self.sdk.join(match role {
            "libc_tbd" => "usr/lib/libc.tbd",
            "libm_tbd" => "usr/lib/libm.tbd",
            _ => "usr/lib/libSystem.tbd",
        });
        link(&path, text);
        let target = self.layout["files"][1]["file"].clone();
        let index = self.layout["aliases"]
            .as_array()
            .unwrap()
            .iter()
            .position(|a| a["role"] == role)
            .unwrap();
        self.layout["aliases"][index] = edge(role, &path, &target);
    }
}
#[test]
fn actual_readonly_sdk_has_all_three_original_edges_and_regular_target() {
    let original = PathBuf::from(
        std::env::var("OE_NATIVE_ACTUAL_SDK_LAYOUT")
            .expect("explicit frozen actual SDK layout input required"),
    );
    let bytes = std::fs::read(&original).unwrap();
    let fixed: Value = serde_json::from_slice(&bytes).unwrap();
    let f = Fixture::new(false);
    let reference = private(&f.root, &fixed);
    let actual = NativeSdkInspector { root: &f.root }
        .inspect_sdk(&reference)
        .unwrap();
    assert_eq!(serde_json::to_value(&actual).unwrap(), fixed);
    assert_eq!(actual.files.len(), 2);
    assert_eq!(actual.aliases.len(), 3);
    assert_eq!(
        actual
            .aliases
            .iter()
            .map(|a| a.role.as_str())
            .collect::<Vec<_>>(),
        vec!["libSystem_tbd", "libc_tbd", "libm_tbd"]
    );
    for alias in &actual.aliases {
        assert_eq!(alias.target, actual.files[1].file);
    }
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    assert!(fixed.get("compilation_origin").is_none());
    assert!(fixed.get("native_available").is_none());
}
#[test]
fn explicit_regular_system_and_three_edge_system_layouts_both_work() {
    for alias in [false, true] {
        let f = Fixture::new(alias);
        let result = f.inspect().unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), f.layout);
    }
}
#[test]
fn strict_json_roles_types_and_no_caller_authority() {
    for case in 0..14 {
        let mut f = Fixture::new(true);
        match case {
            0 => f.layout["schema"] = json!("oev2.native-sdk-layout/2"),
            1 => {
                f.layout.as_object_mut().unwrap().remove("canonical_root");
            }
            2 => f.layout["owner_observed"] = json!(true),
            3 => f.layout["native_available"] = json!(true),
            4 => f.layout["files"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            5 => f.layout["files"][1]["role"] = json!("SDKSettings"),
            6 => f.layout["aliases"]
                .as_array_mut()
                .unwrap()
                .remove(0)
                .as_object()
                .map(|_| ())
                .unwrap(),
            7 => f.layout["aliases"][1]["role"] = json!("libm_tbd"),
            8 => f.layout["aliases"][1]["role"] = json!("unknown"),
            9 => f.layout["aliases"][1]["link_mode"] = json!("755"),
            10 => f.layout["aliases"][1]["link_text_sha256"] = json!("0".repeat(64)),
            11 => f.layout["aliases"][1]["target"]["bytes"] = json!(0),
            12 => f.layout["aliases"][1]["trusted"] = json!(true),
            _ => f.layout["files"][0]["file"]["sha256"] = json!("0".repeat(64)),
        }
        let failure = NativeSdkInspector { root: &f.root }
            .inspect_sdk(&private(&f.root, &f.layout))
            .unwrap_err();
        let partial = failure.partial_result.unwrap();
        assert_eq!(partial["execution"], "not_run", "case={case}");
        assert_eq!(partial["compilation_origin"], "unknown", "case={case}");
        assert_eq!(partial["sdk_closure"], "unknown", "case={case}");
        assert_eq!(
            partial["engineering_adoption"], "not_established",
            "case={case}"
        );
    }
    let f = Fixture::new(false);
    let raw = br#"{"schema":"oev2.native-sdk-layout/3","schema":"oev2.native-sdk-layout/3"}"#;
    let path = f.root.path().join("var/duplicate.json");
    write(&path, raw);
    assert!(
        inspect_sdk(
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
fn same_size_target_mutation_is_not_cached() {
    let f = Fixture::new(true);
    f.inspect().unwrap();
    let system = Path::new(f.layout["files"][1]["file"]["path"].as_str().unwrap());
    let before = std::fs::read(system).unwrap();
    let mut changed = before.clone();
    changed[0] ^= 1;
    std::fs::write(system, &changed).unwrap();
    assert_eq!(changed.len(), before.len());
    assert!(f.inspect().is_err());
}
#[test]
fn same_length_raw_link_replacement_and_mode_pin_are_rejected() {
    let f = Fixture::new(true);
    f.inspect().unwrap();
    let lib = f.sdk.join("usr/lib/libc.tbd");
    assert_eq!("libSystem.tbd".len(), "libSysten.tbd".len());
    link(&lib, "libSysten.tbd");
    assert!(f.inspect().is_err());
    let mut f = Fixture::new(true);
    f.inspect().unwrap();
    let old = f.layout["aliases"][0]["link_mode"].as_u64().unwrap();
    f.layout["aliases"][0]["link_mode"] = json!(old ^ 0o100);
    assert!(f.inspect().is_err());
}
#[test]
#[cfg(target_os = "macos")]
fn actual_synthetic_symlink_mode_mutation_is_rejected() {
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn lchmod(path: *const std::ffi::c_char, mode: u16) -> std::ffi::c_int;
    }
    let f = Fixture::new(true);
    f.inspect().unwrap();
    let path = f.sdk.join("usr/lib/libc.tbd");
    let before = mode(&path);
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { lchmod(c.as_ptr(), (before ^ 0o100) as u16) }, 0);
    assert_ne!(mode(&path), before);
    assert!(f.inspect().is_err());
}
#[test]
fn only_declared_finite_graph_can_resolve_to_fixed_regular_system() {
    for case in 0..7 {
        let mut f = Fixture::new(true);
        match case {
            0 => {
                let text = f
                    .sdk
                    .join("usr/lib/libSystem.B.tbd")
                    .to_str()
                    .unwrap()
                    .to_owned();
                f.rebind_link("libc_tbd", &text);
            }
            1 => f.rebind_link("libc_tbd", "../lib/libSystem.B.tbd"),
            2 => {
                link(&f.sdk.join("usr/lib/extra.tbd"), "libSystem.B.tbd");
                f.rebind_link("libc_tbd", "extra.tbd");
            }
            3 => {
                f.rebind_link("libSystem_tbd", "libc.tbd");
            }
            4 => {
                f.rebind_link("libc_tbd", "libm.tbd");
                f.rebind_link("libm_tbd", "libc.tbd");
            }
            5 => {
                let other = f.sdk.join("usr/lib/other.tbd");
                write(&other, b"unrelated regular target\n");
                f.layout["aliases"][1]["target"] = pin(&other);
            }
            _ => {
                let outside = f.root.path().join("var/outside.tbd");
                write(&outside, b"outsideSDK\n");
                f.layout["aliases"][1]["target"] = pin(&outside);
            }
        }
        assert!(f.inspect().is_err(), "case={case}");
    }
}
#[test]
fn fixed_sdk_paths_canonical_directory_and_parent_links_are_checked() {
    for case in 0..5 {
        let mut f = Fixture::new(true);
        match case {
            0 => f.layout["canonical_root"] = json!(format!("{}/.", f.sdk.display())),
            1 => {
                let alias = f.root.path().join("var/SDKAlias");
                std::os::unix::fs::symlink(&f.sdk, &alias).unwrap();
                f.layout["canonical_root"] = json!(alias);
            }
            2 => {
                f.layout["aliases"][1]["link_path"] =
                    json!(f.root.path().join("var/outside/libc.tbd"))
            }
            3 => {
                let moved = f.sdk.join("usr/lib-real");
                std::fs::rename(f.sdk.join("usr/lib"), &moved).unwrap();
                std::os::unix::fs::symlink("lib-real", f.sdk.join("usr/lib")).unwrap();
            }
            _ => {
                f.layout["files"][0]["file"]["path"] = json!(f.sdk.join("usr/lib/libSystem.B.tbd"))
            }
        }
        assert!(f.inspect().is_err(), "case={case}");
    }
}
#[test]
#[cfg(unix)]
fn non_utf8_raw_link_encoding_is_explicitly_uncovered() {
    use std::os::unix::ffi::OsStringExt;
    let f = Fixture::new(true);
    let path = f.sdk.join("usr/lib/libc.tbd");
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(std::ffi::OsString::from_vec(vec![0xff]), path).unwrap();
    let failure = f.inspect().unwrap_err();
    assert!(format!("{failure:#}").contains("sdk_link_encoding_uncovered"));
}
#[test]
fn sdk_alias_domain_does_not_weaken_generic_regular_file_pins() {
    let f = Fixture::new(true);
    let layout = f.inspect().unwrap();
    for alias in &layout.aliases {
        let bytes = std::fs::read(&alias.link_path).unwrap();
        let leaf = CodeNativeFileBinding {
            path: alias.link_path.clone(),
            sha256: sha256(&bytes),
            bytes: bytes.len() as u64,
            mode: mode(Path::new(&alias.link_path)),
        };
        assert!(verify_file(&leaf).is_err());
    }
    verify_file(&layout.files[1].file).unwrap();
}
