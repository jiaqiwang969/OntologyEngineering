//! Synthetic nonexecuted file pins exercise CONTENT boundaries only. No native
//! binary/SDK/archive semantics, actual debug execution or owner authority claim.
use oe_core::{SkillRoot, sha256};
use oe_evolution::{native_profile::*, *};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    root: SkillRoot,
    profile: Value,
    recipe: Value,
    prior: Value,
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
        write(
            &path.join("Cargo.toml"),
            b"[workspace]\nmembers=[\"crates/test-own\"]\n[workspace.package]\nversion=\"0.1.0\"\n",
            0o600,
        );
        write(
            &path.join("crates/test-own/Cargo.toml"),
            b"[package]\nname=\"test-own\"\nversion=\"0.1.0\"\n",
            0o600,
        );
        write(
            &path.join("crates/test-own/src/main.rs"),
            b"fn main() { let value: i32 = 1; }\n",
            0o600,
        );
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
        let sdk: Vec<Value> = [
            "SDKSettings.json",
            "usr/lib/libSystem.tbd",
            "usr/lib/libc.tbd",
            "usr/lib/libm.tbd",
        ]
        .iter()
        .map(|n| file(&root, &format!("sdk/{n}"), 0o600))
        .collect();
        let stdlib = file(
            &root,
            "toolchain/lib/rustlib/aarch64-apple-darwin/lib/libstd-fixture.rlib",
            0o600,
        );
        let cargo = ArtifactReference {
            path: path.join("Cargo.toml").to_str().unwrap().into(),
            sha256: sha256(std::fs::read(path.join("Cargo.toml")).unwrap()),
        };
        let recipe = json!({"schema":"oev2.native-build-recipe/2","algorithm":DIRECT_ALGORITHM,"input_domain":"own_objects_external_archives","target":"aarch64-apple-darwin","raw_encoded_rustflags":flags,"raw_encoded_rustflags_sha256":sha256(flags.as_bytes()),"debug":{"cargo_manifest":cargo,"cargo_profile":"dev","debug":2,"split_debuginfo":"unpacked","strip":"none","codegen_units":256,"incremental":true,"lto":false,"scope_packages":["test-own"]},"tools":tools,"native_parameter_preset":"darwin-reproducible-prefix-sdk/1","sdk_bindings":sdk,"external_archive_bindings":[stdlib]});
        let recipe_ref = private(&root, "recipe.json", &recipe);
        let source = ArtifactReference {
            path: path
                .join("crates/test-own/src/main.rs")
                .to_str()
                .unwrap()
                .into(),
            sha256: sha256(std::fs::read(path.join("crates/test-own/src/main.rs")).unwrap()),
        };
        let checks:Vec<Value>=["source_line","function","type_member","parameter","local"].iter().enumerate().map(|(i,k)|json!({"check_id":format!("E{i}"),"kind":k,"source":source,"function":"fixture::main","line":1,"name":"value","expected_type":if ["type_member","parameter","local"].contains(k){json!("i32")}else{Value::Null},"matcher":"exact"})).collect();
        let expectations = json!({"schema":"oev2.native-debug-expectations/2","scope":"synthetic content predicates, NOT actual DWARF","checks":checks,"all_required_oso":true,"signature_required":true});
        let expected_ref = private(&root, "expectations.json", &expectations);
        let profile = json!({"schema":"oev2.native-runtime-profile/2","profile":PROFILE,"baseline_profile":baseline,"recipe":recipe_ref,"debug_expectations":expected_ref});
        Self {
            _temp: temp,
            root,
            profile,
            recipe,
            prior,
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
    fn change_prior(&mut self, apply: impl FnOnce(&mut Value)) -> ArtifactReference {
        apply(&mut self.prior);
        self.profile["baseline_profile"] =
            serde_json::to_value(private(&self.root, "prior.json", &self.prior)).unwrap();
        self.profile_ref()
    }
    fn change_expectations(&mut self, apply: impl FnOnce(&mut Value)) -> ArtifactReference {
        apply(&mut self.expectations);
        self.profile["debug_expectations"] =
            serde_json::to_value(private(&self.root, "expectations.json", &self.expectations))
                .unwrap();
        self.profile_ref()
    }
    fn side(&self, p: &ArtifactReference) -> CodeNativeSideContext {
        let path = self.root.path();
        let builder = file(&self.root, "builder", 0o700);
        let mut bins = Vec::new();
        for n in [
            "toolchain/bin/cargo",
            "toolchain/bin/rustc",
            "toolchain/bin/ggen",
        ] {
            let f = file(&self.root, n, 0o700);
            bins.push(json!({"path":f["path"],"sha256":f["sha256"],"bytes":f["bytes"],"source_root":path,"source_manifest_sha256":digest(&public_manifest(path).unwrap()).unwrap()}));
        }
        let build = json!({"artifact_id":"finite","source_root":path,"evidence_path":path.join("var/fixture/evidence"),"cargo_path":bins[0]["path"],"rustc_path":bins[1]["path"],"ggen_path":bins[2]["path"],"cargo_home":path.join("var/fixture/cargo-home"),"target_dir":path.join("var/fixture/target"),"platform":"aarch64-apple-darwin"});
        let ctx = json!({"schema":"oev2.code-build-supervision-context/2","purpose":"observe_compilation_origin","binding_id":"one-owned-binding","revision":"r1","scope_id":"finite-content","builder":{"path":builder["path"],"sha256":builder["sha256"],"bytes":builder["bytes"],"source_root":path,"source_manifest_sha256":digest(&public_manifest(path).unwrap()).unwrap()},"build":build,"source_profile":p,"expected_tools":bins,"dispatch_budget":{"timeout_ms":1000,"output_bytes":4096}});
        let owner_context = private(&self.root, "context.json", &ctx);
        serde_json::from_value(json!({"schema":"oev2.native-side-context/2","source_profile":p,"recipe_sha256":self.profile["recipe"]["sha256"],"owner_context":owner_context,"side_id":"not-an-execution-proof","build":build,"stage_root":path.join("var/fixture/target-native-stage"),"body_budget":{"timeout_ms":500,"output_bytes":4096},"logical_debug_root":"oe-native-debug"})).unwrap()
    }
}
#[test]
fn current_profile_and_side_are_valid_content_only() {
    let f = Fixture::new();
    let p = f.profile_ref();
    let actual = NativeProfileInspector { root: &f.root }
        .inspect_native_profile(&p)
        .unwrap();
    assert_eq!(actual.profile, PROFILE);
    assert!(
        serde_json::to_value(&actual)
            .unwrap()
            .get("compilation_origin")
            .is_none()
    );
    let side = f.side(&p);
    inspect_side(&f.root, &p, &side).unwrap();
    let sr = private(&f.root, "side.json", &serde_json::to_value(&side).unwrap());
    assert_eq!(load_side(&f.root, &sr).unwrap(), side);
    assert_eq!(
        load_recipe(&f.root, &p).unwrap().algorithm,
        DIRECT_ALGORITHM
    );
}
#[test]
fn strict_schema_missing_unknown_and_no_caller_authority() {
    for (id, apply) in [
        ("schema", 0),
        ("profile", 1),
        ("missing", 2),
        ("unknown", 3),
        ("caller_authority", 4),
        ("wrong_type", 5),
    ] {
        let mut f = Fixture::new();
        match apply {
            0 => f.profile["schema"] = json!("oev2.native-runtime-profile/1"),
            1 => f.profile["profile"] = json!(runtime_identity::PROFILE),
            2 => {
                f.profile.as_object_mut().unwrap().remove("recipe");
            }
            3 => f.profile["unexpected"] = json!(true),
            4 => f.profile["compilation_origin"] = json!("owner_observed"),
            _ => f.profile["recipe"] = json!(false),
        };
        let failure = NativeProfileInspector { root: &f.root }
            .inspect_native_profile(&f.profile_ref())
            .unwrap_err();
        assert_eq!(
            failure.partial_result.as_ref().unwrap()["execution"],
            "not_run",
            "{id}"
        );
        assert_eq!(
            failure.partial_result.unwrap()["compilation_origin"],
            "unknown",
            "{id}"
        );
    }
    let f = Fixture::new();
    let bytes =
        br#"{"schema":"oev2.native-runtime-profile/2","schema":"oev2.native-runtime-profile/2"}"#;
    let path = f.root.path().join("var/fixture/duplicate.json");
    write(&path, bytes, 0o600);
    let reference = ArtifactReference {
        path: path.to_str().unwrap().into(),
        sha256: sha256(bytes),
    };
    assert!(inspect_profile(&f.root, &reference).is_err());
}
#[test]
fn recipe_domain_and_exact_flags_controls() {
    for id in 0..16 {
        let mut f = Fixture::new();
        let p = f.change_recipe(|r| match id {
            0 => r["schema"] = json!("wrong"),
            1 => r["algorithm"] = json!("darwin-owned-objects-and-archives/1"),
            2 => r["input_domain"] = json!("own_objects_and_archives"),
            3 => r["target"] = json!("x86_64-apple-darwin"),
            4 => r["raw_encoded_rustflags_sha256"] = json!("0".repeat(64)),
            5 => {
                r["raw_encoded_rustflags"] = json!("-C\u{1f}linker=/side-target/adapter");
                r["raw_encoded_rustflags_sha256"] =
                    json!(sha256(r["raw_encoded_rustflags"].as_str().unwrap()));
            }
            6 => r["native_parameter_preset"] = json!("pass_arbitrary_args"),
            7 => r["debug"]["debug"] = json!(0),
            8 => r["debug"]["strip"] = json!("debuginfo"),
            9 => r["debug"]["split_debuginfo"] = json!("off"),
            10 => r["debug"]["cargo_profile"] = json!("release"),
            11 => r["debug"]["incremental"] = json!(false),
            12 => r["debug"]["lto"] = json!(true),
            13 => r["debug"]["scope_packages"] = json!([]),
            14 => r["debug"]["codegen_units"] = json!(0),
            _ => {
                let flags = format!(
                    "{}\u{1f}-C\u{1f}debuginfo=0",
                    r["raw_encoded_rustflags"].as_str().unwrap()
                );
                r["raw_encoded_rustflags_sha256"] = json!(sha256(flags.as_bytes()));
                r["raw_encoded_rustflags"] = json!(flags);
            }
        });
        assert!(inspect_profile(&f.root, &p).is_err(), "recipe case {id}");
    }
}
#[test]
fn tool_sdk_archive_and_profile_file_identity_controls() {
    for id in 0..14 {
        let mut f = Fixture::new();
        let p = f.change_recipe(|r| match id {
            0 => {
                r["tools"].as_array_mut().unwrap().pop();
            }
            1 => r["tools"][1]["role"] = json!("linker_adapter"),
            2 => r["tools"][1]["role"] = json!("unknown"),
            3 => r["tools"][0]["file"]["sha256"] = json!("0".repeat(64)),
            4 => r["tools"][0]["file"]["bytes"] = json!(1),
            5 => r["tools"][0]["file"]["mode"] = json!(0o600),
            6 => r["sdk_bindings"] = json!([]),
            7 => {
                r["sdk_bindings"][1] = r["sdk_bindings"][0].clone();
            }
            8 => r["sdk_bindings"][0]["sha256"] = json!("0".repeat(64)),
            9 => r["external_archive_bindings"] = json!([]),
            10 => r["external_archive_bindings"][0]["sha256"] = json!("0".repeat(64)),
            11 => r["debug"]["cargo_manifest"]["sha256"] = json!("0".repeat(64)),
            12 => r["debug"]["scope_packages"] = json!(["not-current-package"]),
            _ => r["debug"]["cargo_manifest"]["path"] = r["tools"][0]["file"]["path"].clone(),
        });
        assert!(inspect_profile(&f.root, &p).is_err(), "pin case {id}");
    }
}
#[test]
fn original_profile_source_and_debug_expected_controls() {
    for id in 0..6 {
        let mut f = Fixture::new();
        let p = f.change_prior(|r| match id {
            0 => r["source_manifest"]["sha256"] = json!("0".repeat(64)),
            1 => r["expected_build_capture"] = r["source_manifest"].clone(),
            2 => r["expected_package_version"] = json!("0.2.0"),
            3 => r["source_kind"] = json!("git_checkout"),
            4 => r["expected_target"] = json!("x86_64-apple-darwin"),
            _ => r["schema"] = json!("oev2.rust-runtime-profile/2"),
        });
        assert!(inspect_profile(&f.root, &p).is_err(), "prior case {id}");
    }
    for id in 0..8 {
        let mut f = Fixture::new();
        let p = f.change_expectations(|e| match id {
            0 => e["checks"] = json!([]),
            1 => {
                e["checks"].as_array_mut().unwrap().pop();
            }
            2 => e["all_required_oso"] = json!(false),
            3 => e["signature_required"] = json!(false),
            4 => e["checks"][0]["line"] = json!(0),
            5 => e["checks"][1]["check_id"] = e["checks"][0]["check_id"].clone(),
            6 => e["checks"][3]["expected_type"] = Value::Null,
            _ => e["checks"][0]["source"]["sha256"] = json!("0".repeat(64)),
        });
        assert!(inspect_profile(&f.root, &p).is_err(), "expect case {id}");
    }
    let f = Fixture::new();
    let p = f.profile_ref();
    write(
        &f.root.path().join("crates/test-own/src/main.rs"),
        b"fn main() { let value: i32 = 2; }\n",
        0o600,
    );
    assert!(inspect_profile(&f.root, &p).is_err());
}
#[test]
fn derived_side_paths_source_tools_and_budget_controls() {
    for id in 0..15 {
        let f = Fixture::new();
        let p = f.profile_ref();
        let mut side = f.side(&p);
        match id {
            0 => side.schema = "wrong".into(),
            1 => side.source_profile.sha256 = "0".repeat(64),
            2 => side.recipe_sha256 = "0".repeat(64),
            3 => side.build.platform = "x86_64-apple-darwin".into(),
            4 => side.build.source_root = "/wrong/source".into(),
            5 => side.build.rustc_path = side.build.cargo_path.clone(),
            6 => side.stage_root = side.build.target_dir.clone(),
            7 => {
                side.stage_root = PathBuf::from(&side.build.target_dir)
                    .join("child")
                    .to_str()
                    .unwrap()
                    .into()
            }
            8 => side.logical_debug_root = "../escape".into(),
            9 => side.body_budget.timeout_ms = 0,
            10 => side.body_budget.timeout_ms = 751,
            11 => side.body_budget.output_bytes = 8192,
            12 => side.body_budget.output_bytes = 0,
            13 => side.owner_context.sha256 = "0".repeat(64),
            _ => side.side_id = String::new(),
        };
        assert!(inspect_side(&f.root, &p, &side).is_err(), "side case {id}");
    }
    let f = Fixture::new();
    let p = f.profile_ref();
    let mut side = f.side(&p);
    let raw = std::fs::read(&side.owner_context.path).unwrap();
    let mut ctx: Value = serde_json::from_slice(&raw).unwrap();
    ctx["schema"] = json!("oev2.code-build-supervision-context/1");
    side.owner_context = private(&f.root, "context.json", &ctx);
    assert!(inspect_side(&f.root, &p, &side).is_err());
    let mut f = Fixture::new();
    let own = file(&f.root, "target/libown.rlib", 0o600);
    let p = f.change_recipe(|r| r["external_archive_bindings"] = json!([own]));
    let side = f.side(&p);
    assert!(inspect_side(&f.root, &p, &side).is_err());
}
#[test]
fn same_size_file_mutation_and_mode_symlink_controls() {
    let f = Fixture::new();
    let p = f.profile_ref();
    let pin: CodeNativeFileBinding =
        serde_json::from_value(f.recipe["tools"][0]["file"].clone()).unwrap();
    let mut bytes = std::fs::read(&pin.path).unwrap();
    bytes[0] ^= 1;
    write(Path::new(&pin.path), &bytes, 0o700);
    assert_eq!(std::fs::metadata(&pin.path).unwrap().len(), pin.bytes);
    assert!(verify_file(&pin).is_err());
    assert!(inspect_profile(&f.root, &p).is_err());
    #[cfg(unix)]
    {
        let f = Fixture::new();
        let pin: CodeNativeFileBinding =
            serde_json::from_value(f.recipe["tools"][0]["file"].clone()).unwrap();
        let link = f.root.path().join("var/fixture/link");
        std::os::unix::fs::symlink(&pin.path, &link).unwrap();
        let mut lp = pin.clone();
        lp.path = link.to_str().unwrap().into();
        assert!(verify_file(&lp).is_err());
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pin.path, std::fs::Permissions::from_mode(0o701)).unwrap();
        assert!(verify_file(&pin).is_err());
    }
}

#[test]
fn reserved_archive_response_algorithm_is_explicitly_unsupported() {
    let mut f = Fixture::new();
    let p = f.change_recipe(|r| {
        r["algorithm"] = json!("darwin-archive-response/1");
        r["input_domain"] =
            json!("own_objects_own_regular_archives_external_toolchain_archives_gnu_response");
    });
    let error = inspect_profile(&f.root, &p).unwrap_err();
    assert!(format!("{error:#}").contains("native_algorithm_unsupported"));
    let failure = NativeProfileInspector { root: &f.root }
        .inspect_native_profile(&p)
        .unwrap_err();
    assert_eq!(failure.partial_result.unwrap()["execution"], "not_run");
}
