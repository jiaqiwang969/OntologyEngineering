//! Full3 CONTENT predicates; exact frozen actual SDK3 is read only, all tools
//! and archive pins remain synthetic nonexecuted fixtures. No native closure proof.
use oe_core::{SkillRoot, sha256};
use oe_evolution::{native_profile3::*, *};
use serde_json::{Value, json};
use std::path::Path;
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    root: SkillRoot,
    profile: Value,
    recipe: Value,
    expectations: Value,
}
fn write(path: &Path, bytes: &[u8], mode: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
}
fn private(root: &SkillRoot, name: &str, value: &Value) -> ArtifactReference {
    let path = root.path().join("var/fixture").join(name);
    let raw = serde_json::to_vec(value).unwrap();
    write(&path, &raw, 0o600);
    ArtifactReference {
        path: path.to_str().unwrap().into(),
        sha256: sha256(raw),
    }
}
fn file(root: &SkillRoot, name: &str, mode: u32) -> Value {
    let path = root.path().join("var/fixture").join(name);
    // A file pin is not an executable attestation. These bytes are never run.
    let bytes = format!("nonexecuted synthetic content fixture: {name}\n").into_bytes();
    write(&path, &bytes, mode);
    json!({"path":path,"sha256":sha256(&bytes),"bytes":bytes.len(),"mode":mode})
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        write(&path.join("SKILL.md"), b"# finite content fixture\n", 0o600);
        write(&path.join("Cargo.toml"),b"[workspace]\nmembers=[\"crates/oe-design\",\"crates/oe-code-runner\"]\n[workspace.package]\nversion=\"0.1.0\"\n",0o600);
        for package in ["oe-design", "oe-code-runner"] {
            write(
                &path.join("crates").join(package).join("Cargo.toml"),
                format!("[package]\nname=\"{package}\"\nversion=\"0.1.0\"\n").as_bytes(),
                0o600,
            );
            write(
                &path.join("crates").join(package).join("src/main.rs"),
                format!(
                    "fn main() {{ let {package_name}: i32 = 1; }}\n",
                    package_name = package.replace('-', "_")
                )
                .as_bytes(),
                0o600,
            );
        }
        let root = SkillRoot::new(&path).unwrap();
        let manifest = private(
            &root,
            "source-manifest.json",
            &public_manifest(root.path()).unwrap(),
        );
        let stages = private(
            &root,
            "stages.json",
            &json!({"schema":"oev2.rust-runtime-stages/1","stages":["build-normalizer","actual-ggen","build-runner","actual-ggen-check","runner-identity"]}),
        );
        let network = private(
            &root,
            "network.json",
            &json!({"schema":"oev2.rust-runtime-network/1","cargo":"locked_offline","model_assets":"no_fetch","providers":"not_run"}),
        );
        let prior = json!({"schema":"oev2.rust-runtime-profile/1","profile":runtime_identity::PROFILE,"language":"rust","source_kind":"frozen_export","source_manifest":manifest,"expected_target":"aarch64-apple-darwin","expected_package_version":"0.1.0","expected_git_commit":null,"git":null,"stage_expectations":stages,"network_policy":network,"expected_build_capture":null});
        let baseline = private(&root, "prior.json", &prior);
        let roles = [
            "linker_adapter",
            "clang_driver",
            "native_linker",
            "archive_reader",
            "debug_mapper",
            "debug_linker",
            "dwarf_reader",
            "signature_verifier",
        ];
        let tools:Vec<Value>=roles.iter().map(|r|json!({"role":r,"file":file(&root,&format!("tools/{r}"),0o700),"version_observation":null})).collect();
        let flags = format!(
            "-C\u{1f}linker={}",
            tools[0]["file"]["path"].as_str().unwrap()
        );
        let actual_input = std::env::var("OE_NATIVE_ACTUAL_SDK_LAYOUT")
            .expect("explicit fixed SDK3 input required");
        let sdk: Value = serde_json::from_slice(&std::fs::read(actual_input).unwrap()).unwrap();
        let sdk_reference = private(&root, "sdk-layout.json", &sdk);
        let stdlib = file(
            &root,
            "toolchain/lib/rustlib/aarch64-apple-darwin/lib/libstd-fixture.rlib",
            0o600,
        );
        let cargo = ArtifactReference {
            path: path.join("Cargo.toml").to_str().unwrap().into(),
            sha256: sha256(std::fs::read(path.join("Cargo.toml")).unwrap()),
        };
        let recipe = json!({"schema":"oev2.native-build-recipe/3","algorithm":FULL_ALGORITHM,"input_domain":FULL_INPUT_DOMAIN,"target":"aarch64-apple-darwin","raw_encoded_rustflags":flags,"raw_encoded_rustflags_sha256":sha256(flags.as_bytes()),"debug":{"cargo_manifest":cargo,"cargo_profile":"dev","debug":2,"split_debuginfo":"unpacked","strip":"none","codegen_units":256,"incremental":true,"lto":false,"scope_packages":["oe-design","oe-code-runner"]},"tools":tools,"native_parameter_preset":"darwin-reproducible-prefix-sdk/1","sdk_layout":sdk_reference,"external_archive_bindings":[stdlib]});
        let recipe_ref = private(&root, "recipe.json", &recipe);
        let source = ArtifactReference {
            path: path
                .join("crates/oe-code-runner/src/main.rs")
                .to_str()
                .unwrap()
                .into(),
            sha256: sha256(std::fs::read(path.join("crates/oe-code-runner/src/main.rs")).unwrap()),
        };
        let checks:Vec<Value>=["source_line","function","type_member","parameter","local"].iter().enumerate().map(|(i,k)|json!({"check_id":format!("E{i}"),"kind":k,"source":source,"function":"fixture::main","line":1,"name":"value","expected_type":if ["type_member","parameter","local"].contains(k){json!("i32")}else{Value::Null},"matcher":"exact"})).collect();
        let expectations = json!({"schema":"oev2.native-debug-expectations/2","scope":"synthetic content predicates, NOT actual DWARF","checks":checks,"all_required_oso":true,"signature_required":true});
        let expected_ref = private(&root, "expectations.json", &expectations);
        let profile = json!({"schema":"oev2.native-runtime-profile/3","profile":PROFILE,"baseline_profile":baseline,"recipe":recipe_ref,"debug_expectations":expected_ref});
        Self {
            _temp: temp,
            root,
            profile,
            recipe,
            expectations,
        }
    }
    fn profile_ref(&self) -> ArtifactReference {
        private(&self.root, "profile.json", &self.profile)
    }
    fn change_recipe(&mut self, apply: impl FnOnce(&mut Value)) -> ArtifactReference {
        apply(&mut self.recipe);
        self.profile["recipe"] =
            serde_json::to_value(private(&self.root, "recipe.json", &self.recipe)).unwrap();
        self.profile_ref()
    }
}

fn bundle_fixture() -> Fixture {
    let mut f = Fixture::new();
    let mut normalizer = f.expectations.clone();
    let path = f.root.path().join("crates/oe-design/src/main.rs");
    for check in normalizer["checks"].as_array_mut().unwrap() {
        check["source"] = json!({"path":path,"sha256":sha256(std::fs::read(&path).unwrap())});
        check["function"] = json!("synthetic_oe_design::main");
    }
    let n = private(&f.root, "normalizer-expectations.json", &normalizer);
    let r = private(&f.root, "runner-expectations.json", &f.expectations);
    let bundle = json!({"schema":"oev2.native-debug-expectations-bundle/3","scope":"finite synthetic package contents, NOT actual DWARF","images":[{"image_role":"normalizer","package":"oe-design","expectations":n},{"image_role":"runner","package":"oe-code-runner","expectations":r}]});
    f.profile["debug_expectations"] =
        serde_json::to_value(private(&f.root, "bundle3.json", &bundle)).unwrap();
    f
}
fn bundle(f: &Fixture) -> Value {
    let p = f.profile["debug_expectations"]["path"].as_str().unwrap();
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}
fn with_bundle(f: &mut Fixture, b: &Value) -> ArtifactReference {
    f.profile["debug_expectations"] =
        serde_json::to_value(private(&f.root, "bundle3.json", b)).unwrap();
    f.profile_ref()
}
#[test]
fn both_independent_package_image_expectations_are_required_content() {
    let f = bundle_fixture();
    let p = f.profile_ref();
    assert!(
        inspect_profile(&f.root, &p).is_ok(),
        "complete normalizer+runner bundle must be content-valid"
    );
    assert!(!f.root.path().join("var/state").exists());
}
#[test]
fn old_flat_runner_expectations_do_not_qualify_full3_bundle() {
    let f = Fixture::new();
    assert!(
        inspect_profile(&f.root, &f.profile_ref()).is_err(),
        "flat old2 must be rejected for full3"
    );
}
#[test]
fn roles_packages_scope_and_unknown_fields_are_closed() {
    for case in 0..10 {
        let mut f = bundle_fixture();
        let mut b = bundle(&f);
        match case {
            0 => {
                b["images"].as_array_mut().unwrap().pop();
            }
            1 => b["images"][0]["image_role"] = json!("runner"),
            2 => b["images"][0]["image_role"] = json!("other"),
            3 => b["images"][0]["package"] = json!("oe-code-runner"),
            4 => b["schema"] = json!("oev2.native-debug-expectations/2"),
            5 => b["trusted"] = json!(true),
            6 => b["images"][0]["passed"] = json!(true),
            7 => b["images"][0]
                .as_object_mut()
                .unwrap()
                .remove("package")
                .map(|_| ())
                .unwrap(),
            8 => b["scope"] = json!(""),
            _ => {
                let third = b["images"][0].clone();
                b["images"].as_array_mut().unwrap().push(third);
            }
        };
        let p = with_bundle(&mut f, &b);
        assert!(inspect_profile(&f.root, &p).is_err(), "case={case}");
    }
    let mut f = bundle_fixture();
    let p = f.change_recipe(|r| r["debug"]["scope_packages"] = json!(["oe-code-runner"]));
    assert!(inspect_profile(&f.root, &p).is_err());
    let f = bundle_fixture();
    std::fs::write(
        f.root.path().join("crates/oe-design/Cargo.toml"),
        b"[package]\nname=\"renamed\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    assert!(inspect_profile(&f.root, &f.profile_ref()).is_err());
}
#[test]
fn runner_reference_or_sources_cannot_be_relabeled_normalizer() {
    let mut f = bundle_fixture();
    let mut b = bundle(&f);
    b["images"][0]["expectations"] = b["images"][1]["expectations"].clone();
    let p = with_bundle(&mut f, &b);
    assert!(inspect_profile(&f.root, &p).is_err());
    let mut f = bundle_fixture();
    let mut b = bundle(&f);
    let normal_path = b["images"][0]["expectations"]["path"].as_str().unwrap();
    let mut n: Value = serde_json::from_slice(&std::fs::read(normal_path).unwrap()).unwrap();
    let rpath = f.root.path().join("crates/oe-code-runner/src/main.rs");
    n["checks"][0]["source"] =
        json!({"path":rpath,"sha256":sha256(std::fs::read(&rpath).unwrap())});
    b["images"][0]["expectations"] =
        serde_json::to_value(private(&f.root, "normalizer-expectations.json", &n)).unwrap();
    let p = with_bundle(&mut f, &b);
    assert!(inspect_profile(&f.root, &p).is_err());
}
#[test]
fn independent_source_and_expectation_reference_mutations_are_rejected() {
    let f = bundle_fixture();
    let p = f.profile_ref();
    let path = f.root.path().join("crates/oe-design/src/main.rs");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[0] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    assert!(inspect_profile(&f.root, &p).is_err());
    let mut f = bundle_fixture();
    let mut b = bundle(&f);
    b["images"][0]["expectations"]["sha256"] = json!("0".repeat(64));
    let p = with_bundle(&mut f, &b);
    assert!(inspect_profile(&f.root, &p).is_err());
}

#[test]
fn typed_image_helpers_recheck_each_real_package_reference() {
    let f = bundle_fixture();
    let p = f.profile_ref();
    let images = load_debug_images(&f.root, &p).unwrap();
    let recipe = load_recipe(&f.root, &p).unwrap();
    assert_eq!(images.images.len(), 2);
    assert_ne!(images.images[0].expectations, images.images[1].expectations);
    for image in &images.images {
        let expected = inspect_debug_image(&f.root, &recipe, image).unwrap();
        assert_eq!(expected.checks.len(), 5);
        assert!(
            serde_json::to_value(&expected)
                .unwrap()
                .get("owner_observed")
                .is_none()
        );
    }
    let normalizer = images
        .images
        .iter()
        .find(|i| i.image_role == "normalizer")
        .unwrap();
    let mut excluded = recipe.clone();
    excluded.debug.scope_packages = vec!["oe-code-runner".into()];
    assert!(inspect_debug_image(&f.root, &excluded, normalizer).is_err());
    let source = f.root.path().join("crates/oe-design/src/main.rs");
    let mut bytes = std::fs::read(&source).unwrap();
    bytes[0] ^= 1;
    std::fs::write(&source, bytes).unwrap();
    assert!(inspect_debug_image(&f.root, &recipe, normalizer).is_err());
    assert!(load_debug_images(&f.root, &p).is_err());
}
