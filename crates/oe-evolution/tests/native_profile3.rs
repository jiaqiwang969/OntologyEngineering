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
        let runner_ref = private(&root, "expectations.json", &expectations);
        let mut normalizer = expectations.clone();
        let normalizer_path = path.join("crates/oe-design/src/main.rs");
        for check in normalizer["checks"].as_array_mut().unwrap() {
            check["source"] = json!({"path":normalizer_path,"sha256":sha256(std::fs::read(&normalizer_path).unwrap())});
            check["function"] = json!("synthetic_oe_design::main");
        }
        let normalizer_ref = private(&root, "normalizer-expectations.json", &normalizer);
        let bundle = json!({"schema":"oev2.native-debug-expectations-bundle/3","scope":"finite synthetic package contents, NOT actual DWARF","images":[{"image_role":"normalizer","package":"oe-design","expectations":normalizer_ref},{"image_role":"runner","package":"oe-code-runner","expectations":runner_ref}]});
        let expected_ref = private(&root, "bundle3.json", &bundle);
        let profile = json!({"schema":"oev2.native-runtime-profile/3","profile":PROFILE,"baseline_profile":baseline,"recipe":recipe_ref,"debug_expectations":expected_ref});
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
        let bundle_path = self.profile["debug_expectations"]["path"].as_str().unwrap();
        let mut bundle: Value =
            serde_json::from_slice(&std::fs::read(bundle_path).unwrap()).unwrap();
        bundle["images"][1]["expectations"] =
            serde_json::to_value(private(&self.root, "expectations.json", &self.expectations))
                .unwrap();
        self.profile["debug_expectations"] =
            serde_json::to_value(private(&self.root, "bundle3.json", &bundle)).unwrap();
        self.profile_ref()
    }
    fn side(&self, p: &ArtifactReference) -> CodeNativeSideContext3 {
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
        serde_json::from_value(json!({"schema":"oev2.native-side-context/3","source_profile":p,"recipe_sha256":self.profile["recipe"]["sha256"],"owner_context":owner_context,"side_id":"not-an-execution-proof","build":build,"stage_root":path.join("var/fixture/target-native-stage"),"body_budget":{"timeout_ms":500,"output_bytes":4096},"logical_debug_root":"oe-native-debug"})).unwrap()
    }
}

#[test]
fn full3_profile_and_side_use_real_sdk_content_without_execution_authority() {
    let f = Fixture::new();
    let p = f.profile_ref();
    let result = NativeProfile3Inspector { root: &f.root }
        .inspect_native_profile3(&p)
        .unwrap();
    assert_eq!(result.profile, PROFILE);
    let value = serde_json::to_value(&result).unwrap();
    assert!(value.get("owner_observed").is_none());
    assert!(value.get("native_available").is_none());
    assert!(value.get("compilation_origin").is_none());
    assert_eq!(load_recipe(&f.root, &p).unwrap().algorithm, FULL_ALGORITHM);
    let side = f.side(&p);
    inspect_side(&f.root, &p, &side).unwrap();
    let r = private(&f.root, "side3.json", &serde_json::to_value(&side).unwrap());
    assert_eq!(load_side(&f.root, &r).unwrap(), side);
    assert!(!f.root.path().join("var/state").exists());
    assert!(!f.root.path().join("var/fixture/target").exists());
}
#[test]
fn explicit3_wire_schemas_do_not_upgrade2_or_accept_caller_authority() {
    for case in 0..7 {
        let mut f = Fixture::new();
        match case {
            0 => f.profile["schema"] = json!("oev2.native-runtime-profile/2"),
            1 => f.profile["profile"] = json!("semantica.rust-runtime-native-debug/2"),
            2 => {
                f.profile.as_object_mut().unwrap().remove("recipe");
            }
            3 => f.profile["compilation_origin"] = json!("owner_observed"),
            4 => f.profile["native_available"] = json!(true),
            5 => f.profile["baseline_profile"] = json!(true),
            _ => f.profile["trusted"] = json!(true),
        }
        let result = NativeProfile3Inspector { root: &f.root }
            .inspect_native_profile3(&f.profile_ref())
            .unwrap_err();
        let partial = result.partial_result.unwrap();
        assert_eq!(partial["execution"], "not_run");
        assert_eq!(partial["compilation_origin"], "unknown");
        assert_eq!(partial["sdk_closure"], "unknown");
        assert_eq!(partial["archive_closure"], "unknown");
        assert_eq!(partial["engineering_adoption"], "not_established");
    }
    let f = Fixture::new();
    assert!(oe_evolution::native_profile::inspect_profile(&f.root, &f.profile_ref()).is_err());
    let side = f.side(&f.profile_ref());
    assert!(
        serde_json::from_value::<CodeNativeSideContext>(serde_json::to_value(side).unwrap())
            .unwrap()
            .validate()
            .is_err()
    );
}
#[test]
fn raw_flags_debug_conditions_and_current_cargo_scope_keep_original_predicates() {
    for case in 0..18 {
        let mut f = Fixture::new();
        let p = f.change_recipe(|r| match case {
            0 => r["schema"] = json!("oev2.native-build-recipe/2"),
            1 => r["algorithm"] = json!("darwin-direct-objects/1"),
            2 => r["input_domain"] = json!("own_objects_external_archives"),
            3 => r["target"] = json!("x86_64-apple-darwin"),
            4 => r["native_parameter_preset"] = json!("arbitrary"),
            5 => r["raw_encoded_rustflags_sha256"] = json!("0".repeat(64)),
            6 => {
                r["raw_encoded_rustflags"] = json!("-C\u{1f}linker=/alternative");
                r["raw_encoded_rustflags_sha256"] =
                    json!(sha256(r["raw_encoded_rustflags"].as_str().unwrap()));
            }
            7 => r["debug"]["debug"] = json!(0),
            8 => r["debug"]["cargo_profile"] = json!("release"),
            9 => r["debug"]["split_debuginfo"] = json!("packed"),
            10 => r["debug"]["strip"] = json!("symbols"),
            11 => r["debug"]["codegen_units"] = json!(1),
            12 => r["debug"]["incremental"] = json!(false),
            13 => r["debug"]["lto"] = json!(true),
            14 => r["debug"]["scope_packages"] = json!([]),
            15 => r["debug"]["scope_packages"] = json!(["oe-design", "oe-design"]),
            16 => r["debug"]["scope_packages"] = json!(["absent-package"]),
            _ => r["debug"]["cargo_manifest"]["sha256"] = json!("0".repeat(64)),
        });
        assert!(inspect_profile(&f.root, &p).is_err(), "case={case}");
    }
}
#[test]
fn exact_sdk_artifact_and_alias_graph_cannot_be_bypassed_by3() {
    for case in 0..6 {
        let mut f = Fixture::new();
        let reference: ArtifactReference =
            serde_json::from_value(f.recipe["sdk_layout"].clone()).unwrap();
        let mut layout: Value =
            serde_json::from_slice(&std::fs::read(&reference.path).unwrap()).unwrap();
        match case {
            0 => layout["schema"] = json!("oev2.native-sdk-layout/2"),
            1 => layout["aliases"]
                .as_array_mut()
                .unwrap()
                .remove(0)
                .as_object()
                .map(|_| ())
                .unwrap(),
            2 => layout["aliases"][1]["link_text_sha256"] = json!("0".repeat(64)),
            3 => layout["files"][1]["file"]["sha256"] = json!("0".repeat(64)),
            4 => layout["owner_observed"] = json!(true),
            _ => layout["aliases"][1]["link_mode"] = json!(0),
        }
        let updated = private(&f.root, "sdk-layout.json", &layout);
        let p = f.change_recipe(|r| r["sdk_layout"] = serde_json::to_value(updated).unwrap());
        assert!(inspect_profile(&f.root, &p).is_err(), "case={case}");
    }
    let mut f = Fixture::new();
    let p = f.change_recipe(|r| r["sdk_layout"]["sha256"] = json!("0".repeat(64)));
    assert!(inspect_profile(&f.root, &p).is_err());
}
#[test]
fn tool_and_external_archive_pins_remain_complete_and_strict_regular() {
    for case in 0..8 {
        let mut f = Fixture::new();
        let p = f.change_recipe(|r| match case {
            0 => {
                r["tools"].as_array_mut().unwrap().pop();
            }
            1 => r["tools"][1]["role"] = json!("linker_adapter"),
            2 => r["tools"][0]["file"]["sha256"] = json!("0".repeat(64)),
            3 => r["tools"][0]["file"]["mode"] = json!(0o600),
            4 => r["external_archive_bindings"] = json!([]),
            5 => {
                let first = r["external_archive_bindings"][0].clone();
                r["external_archive_bindings"]
                    .as_array_mut()
                    .unwrap()
                    .push(first);
            }
            6 => r["external_archive_bindings"][0]["sha256"] = json!("0".repeat(64)),
            _ => {
                r["tools"][0]["version_observation"] =
                    json!({"path":"var/absent-version.json","sha256":"0".repeat(64)})
            }
        });
        assert!(inspect_profile(&f.root, &p).is_err(), "case={case}");
    }
    let f = Fixture::new();
    let path = Path::new(f.recipe["tools"][0]["file"]["path"].as_str().unwrap());
    let mut changed = std::fs::read(path).unwrap();
    changed[0] ^= 1;
    std::fs::write(path, changed).unwrap();
    assert!(inspect_profile(&f.root, &f.profile_ref()).is_err());
    #[cfg(unix)]
    {
        let f = Fixture::new();
        let path = Path::new(
            f.recipe["external_archive_bindings"][0]["path"]
                .as_str()
                .unwrap(),
        );
        let original = path.with_extension("regular");
        std::fs::rename(path, &original).unwrap();
        std::os::unix::fs::symlink(original, path).unwrap();
        assert!(inspect_profile(&f.root, &f.profile_ref()).is_err());
    }
}
#[test]
fn original_source1_and_shared_five_debug_kinds_are_still_required() {
    for case in 0..6 {
        let mut f = Fixture::new();
        let p = f.change_expectations(|e| match case {
            0 => {
                e["checks"].as_array_mut().unwrap().pop();
            }
            1 => e["checks"][1]["check_id"] = e["checks"][0]["check_id"].clone(),
            2 => e["checks"][0]["line"] = json!(0),
            3 => e["checks"][4]["expected_type"] = Value::Null,
            4 => e["checks"][0]["source"]["sha256"] = json!("0".repeat(64)),
            _ => e["passed"] = json!(true),
        });
        assert!(inspect_profile(&f.root, &p).is_err(), "case={case}");
    }
    let mut f = Fixture::new();
    let p = f.change_prior(|r| r["source_kind"] = json!("git"));
    assert!(inspect_profile(&f.root, &p).is_err());
    let f = Fixture::new();
    let p = f.profile_ref();
    inspect_profile(&f.root, &p).unwrap();
    std::fs::write(
        f.root.path().join("crates/oe-code-runner/src/main.rs"),
        b"fn main() { let value: i32 = 2; }\n",
    )
    .unwrap();
    assert!(inspect_profile(&f.root, &p).is_err());
}
#[test]
fn side3_context2_path_and_budget_content_never_create_elapsed_authority() {
    let f = Fixture::new();
    let p = f.profile_ref();
    let good = f.side(&p);
    for case in 0..13 {
        let mut side = good.clone();
        match case {
            0 => side.schema = "oev2.native-side-context/2".into(),
            1 => side.source_profile.sha256 = "0".repeat(64),
            2 => side.recipe_sha256 = "0".repeat(64),
            3 => side.owner_context.sha256 = "0".repeat(64),
            4 => side.build.artifact_id = "other".into(),
            5 => {
                side.stage_root = f
                    .root
                    .path()
                    .join("var/alternative-stage")
                    .to_str()
                    .unwrap()
                    .into()
            }
            6 => side.logical_debug_root = "caller-root".into(),
            7 => side.body_budget.timeout_ms = 0,
            8 => side.body_budget.timeout_ms = 751,
            9 => side.body_budget.output_bytes += 1,
            10 => {
                side.build.source_root = f
                    .root
                    .path()
                    .join("var/other-source")
                    .to_str()
                    .unwrap()
                    .into()
            }
            11 => side.build.evidence_path = side.build.target_dir.clone(),
            _ => side.side_id.clear(),
        };
        assert!(inspect_side(&f.root, &p, &side).is_err(), "case={case}");
    }
    for case in 0..3 {
        let mut ctx: Value =
            serde_json::from_slice(&std::fs::read(&good.owner_context.path).unwrap()).unwrap();
        match case {
            0 => ctx["schema"] = json!("oev2.code-build-supervision-context/1"),
            1 => ctx["purpose"] = json!("caller-authority"),
            _ => ctx["expected_tools"][0]["sha256"] = json!("0".repeat(64)),
        }
        let mut side = good.clone();
        side.owner_context = private(&f.root, &format!("bad-context{case}.json"), &ctx);
        assert!(
            inspect_side(&f.root, &p, &side).is_err(),
            "contextcase={case}"
        );
    }
    inspect_side(&f.root, &p, &good).unwrap();
    let mut value = serde_json::to_value(&good).unwrap();
    value["actor"] = json!("owner");
    assert!(serde_json::from_value::<CodeNativeSideContext3>(value).is_err());
}
#[test]
fn archive_pins_must_belong_to_selected_toolchain_before_side_content_accepts() {
    let mut f = Fixture::new();
    let other = file(&f.root, "outside-stdlib/libother.rlib", 0o600);
    let p = f.change_recipe(|r| r["external_archive_bindings"] = json!([other]));
    inspect_profile(&f.root, &p).unwrap();
    let side = f.side(&p);
    assert!(inspect_side(&f.root, &p, &side).is_err());
}
#[test]
fn duplicate_json_and_changed_side_bytes_are_not_accepted() {
    let f = Fixture::new();
    let raw =
        br#"{"schema":"oev2.native-runtime-profile/3","schema":"oev2.native-runtime-profile/3"}"#;
    let path = f.root.path().join("var/fixture/duplicate3.json");
    write(&path, raw, 0o600);
    assert!(
        inspect_profile(
            &f.root,
            &ArtifactReference {
                path: path.to_str().unwrap().into(),
                sha256: sha256(raw)
            }
        )
        .is_err()
    );
    let p = f.profile_ref();
    let side = f.side(&p);
    let r = private(&f.root, "side3.json", &serde_json::to_value(&side).unwrap());
    load_side(&f.root, &r).unwrap();
    std::fs::write(&r.path, b"{}\n").unwrap();
    assert!(load_side(&f.root, &r).is_err());
}
