use oe_core::{SkillRoot, write_new};
use oe_evolution::runtime_identity::{inspect, observe_identity, reproduce};
use oe_evolution::*;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .canonicalize()
        .unwrap()
}
fn private() -> PathBuf {
    let p = root().join(format!(
        "var/runtime-identity-tests/fixture space {}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn reference(p: &Path) -> ArtifactReference {
    ArtifactReference {
        path: p.to_str().unwrap().into(),
        sha256: oe_core::sha256(std::fs::read(p).unwrap()),
    }
}
fn profile(dir: &Path) -> ArtifactReference {
    let snapshot = public_manifest(&root()).unwrap();
    let manifest = dir.join("manifest.json");
    write_new(&manifest, &snapshot).unwrap();
    let stages = dir.join("stages.json");
    write_new(&stages,&json!({"schema":"oev2.rust-runtime-stages/1","stages":["build-normalizer","actual-ggen","build-runner","actual-ggen-check","runner-identity"]})).unwrap();
    let network = dir.join("network.json");
    write_new(&network,&json!({"schema":"oev2.rust-runtime-network/1","cargo":"locked_offline","model_assets":"no_fetch","providers":"not_run"})).unwrap();
    let p = CodeRustRuntimeProfile {
        schema: "oev2.rust-runtime-profile/1".into(),
        profile: runtime_identity::PROFILE.into(),
        language: "rust".into(),
        source_kind: "frozen_export".into(),
        source_manifest: reference(&manifest),
        expected_target: "aarch64-apple-darwin".into(),
        expected_package_version: "2.0.0-dev.3".into(),
        expected_build_capture: None,
        expected_git_commit: None,
        git: None,
        stage_expectations: reference(&stages),
        network_policy: reference(&network),
    };
    let path = dir.join("profile.json");
    write_new(&path, &serde_json::to_value(p).unwrap()).unwrap();
    reference(&path)
}
#[test]
fn source_schema_path_and_input_pins_fail_before_dispatch() {
    let dir = private();
    let p = profile(&dir);
    let base = CodeRustRuntimeInspectRequest {
        schema: "oev2.rust-runtime-inspect-request/1".into(),
        profile: p.clone(),
        source_root: root().to_str().unwrap().into(),
        evidence_path: dir.join("inspect").to_str().unwrap().into(),
        budget: CodeBudget {
            timeout_ms: 2000,
            output_bytes: 65536,
        },
    };
    let proof = inspect(&base).unwrap();
    assert_eq!(proof.identity, "bound");
    assert_eq!(proof.proof["build_dispatched"], false);
    let mut wrong = base.clone();
    wrong.schema = "unknown".into();
    wrong.evidence_path = dir.join("bad-schema").to_str().unwrap().into();
    assert!(format!("{:#}", inspect(&wrong).unwrap_err()).contains("rust_runtime_inspect_schema"));
    assert!(!Path::new(&wrong.evidence_path).exists());
    wrong = base.clone();
    wrong.budget.timeout_ms = 0;
    wrong.evidence_path = dir.join("bad-budget").to_str().unwrap().into();
    assert!(format!("{:#}", inspect(&wrong).unwrap_err()).contains("budget"));
    assert!(!Path::new(&wrong.evidence_path).exists());
    wrong = base.clone();
    wrong.evidence_path = "var/../escape".into();
    assert!(inspect(&wrong).is_err());
    let mut raw: Value = serde_json::from_slice(&std::fs::read(&p.path).unwrap()).unwrap();
    raw["expected_package_version"] = json!("9.9.9+staging.1");
    let bad = dir.join("bad-version.json");
    write_new(&bad, &raw).unwrap();
    wrong = base.clone();
    wrong.profile = reference(&bad);
    wrong.evidence_path = dir.join("bad-version").to_str().unwrap().into();
    assert!(format!("{:#}", inspect(&wrong).unwrap_err()).contains("source_version_mismatch"));
    assert!(!Path::new(&wrong.evidence_path).exists());
    raw["language"] = json!("python");
    let bad = dir.join("bad-language.json");
    write_new(&bad, &raw).unwrap();
    wrong.profile = reference(&bad);
    assert!(inspect(&wrong).is_err());
    assert!(
        serde_json::from_value::<CodeRustRuntimeProfile>(
            json!({"passed":true,"profile":runtime_identity::PROFILE})
        )
        .is_err()
    );
}
#[test]
fn actual_probe_budget_stream_and_capture_keep_distinct_states() {
    let dir = private();
    let compiler = std::env::var_os("RUSTC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("rustc"));
    let worker = dir.join("worker-stdout");
    let status = Command::new(compiler)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/runtime_worker.rs"))
        .args(["--edition", "2024", "-o"])
        .arg(&worker)
        .status()
        .unwrap();
    assert!(status.success());
    let cap = CodeBudget {
        timeout_ms: 2000,
        output_bytes: 65536,
    };
    let binary = binary_reference(&worker, &root()).unwrap();
    let result = observe_identity(&binary, &cap, &dir.join("positive")).unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout, b"123456789");
    assert_eq!(result.report["capture_complete"], true);
    assert_eq!(result.report["semantic_execution"], "not_inferred");
    let overflow = observe_identity(
        &binary,
        &CodeBudget {
            timeout_ms: 2000,
            output_bytes: 8,
        },
        &dir.join("overflow"),
    )
    .unwrap();
    assert_eq!(overflow.status, "output_limit");
    assert_ne!(overflow.report["capture_complete"], true);
    for mode in ["stderr", "sleep", "parent", "malformed", "exit37"] {
        std::fs::copy(&worker, dir.join(format!("worker-{mode}"))).unwrap();
    }
    let stderr = binary_reference(&dir.join("worker-stderr"), &root()).unwrap();
    let result = observe_identity(
        &stderr,
        &CodeBudget {
            timeout_ms: 2000,
            output_bytes: 8,
        },
        &dir.join("stderr-overflow"),
    )
    .unwrap();
    assert_eq!(result.status, "output_limit");
    let malformed = binary_reference(&dir.join("worker-malformed"), &root()).unwrap();
    let result = observe_identity(&malformed, &cap, &dir.join("malformed")).unwrap();
    assert!(oe_core::json::strict_json(&result.stdout).is_err());
    for mode in ["sleep", "parent"] {
        let b = binary_reference(&dir.join(format!("worker-{mode}")), &root()).unwrap();
        let start = Instant::now();
        let result = observe_identity(
            &b,
            &CodeBudget {
                timeout_ms: 80,
                output_bytes: 65536,
            },
            &dir.join(format!("timeout-{mode}")),
        )
        .unwrap();
        assert_eq!(result.status, "timed_out");
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    assert!(
        observe_identity(
            &binary,
            &CodeBudget {
                timeout_ms: 0,
                output_bytes: 65536
            },
            &dir.join("zero")
        )
        .is_err()
    );
    assert!(!dir.join("zero").exists());
    let fail = binary_reference(&dir.join("worker-exit37"), &root()).unwrap();
    let result = observe_identity(&fail, &cap, &dir.join("exit37")).unwrap();
    assert_eq!(result.status, "process_failed");
    assert_eq!(result.exit_code, Some(37));
    assert_eq!(result.stderr, b"fixture failure\n");
    assert_eq!(result.report["capture_complete"], true);
}
#[test]
fn reproduction_never_infers_eligibility_or_old_profile_success() {
    let dir = private();
    let p = profile(&dir);
    let root = root();
    let build = CodeBuildRequest {
        artifact_id: "native-first".into(),
        source_root: root.to_str().unwrap().into(),
        evidence_path: dir.join("first").to_str().unwrap().into(),
        cargo_path: "cargo".into(),
        rustc_path: "rustc".into(),
        ggen_path: "ggen".into(),
        cargo_home: "fixture-not-dispatched".into(),
        target_dir: dir.join("target-a").to_str().unwrap().into(),
        platform: "aarch64-apple-darwin".into(),
    };
    let mut second = build.clone();
    second.artifact_id = "native-second".into();
    second.evidence_path = dir.join("second").to_str().unwrap().into();
    second.target_dir = build.target_dir.clone();
    let r = CodeRustRuntimeReproductionRequest {
        schema: "oev2.rust-runtime-reproduction-request/1".into(),
        profile: p,
        first: build,
        second,
        budget: CodeBudget {
            timeout_ms: 2000,
            output_bytes: 65536,
        },
        evidence_path: dir.join("reproduction").to_str().unwrap().into(),
    };
    assert!(format!("{:#}", reproduce(&r).unwrap_err()).contains("build_paths_overlap"));
    assert!(!Path::new(&r.evidence_path).exists());
    let unknown = CodeRustRuntimeReproductionObservation {
        schema: "oev2.rust-runtime-reproduction-observation/1".into(),
        profile_sha256: "a".repeat(64),
        first: None,
        second: None,
        first_capture: None,
        second_capture: None,
        execution: "not_run".into(),
        reproduction: "unknown".into(),
        published_descriptor: None,
        uncovered: vec!["full13494 unknown; original11 still retired failed; no selection".into()],
    };
    unknown.validate().unwrap();
    let mut raw = serde_json::to_value(unknown).unwrap();
    raw["eligibility"] = json!("eligible");
    assert!(serde_json::from_value::<CodeRustRuntimeReproductionObservation>(raw).is_err());
    let _ = SkillRoot::new(root).unwrap();
}

fn small_source() -> (PathBuf, PathBuf) {
    let top = private();
    let root = top.join("source");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        "independent native source inspection fixture\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion='2.0.0-dev.3'\n",
    )
    .unwrap();
    std::fs::write(root.join(".gitignore"), "var/\n").unwrap();
    for file in [
        "Cargo.lock",
        "specs/conditions.toml",
        "templates/object.tmpl",
        "vendor/algorithm.rs",
        "build_support/normalizer.rs",
    ] {
        let p = root.join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "fixed independent source\n").unwrap();
    }
    let state = root.join("var/profile");
    std::fs::create_dir_all(&state).unwrap();
    (root, state)
}
fn small_profile(
    root: &Path,
    state: &Path,
    git: Option<(String, CodeBinaryRef)>,
    label: &str,
) -> ArtifactReference {
    let manifest = state.join(format!("{label}-manifest.json"));
    write_new(&manifest, &public_manifest(root).unwrap()).unwrap();
    let stages = state.join(format!("{label}-stages.json"));
    write_new(&stages,&json!({"schema":"oev2.rust-runtime-stages/1","stages":["build-normalizer","actual-ggen","build-runner","actual-ggen-check","runner-identity"]})).unwrap();
    let network = state.join(format!("{label}-network.json"));
    write_new(&network,&json!({"schema":"oev2.rust-runtime-network/1","cargo":"locked_offline","model_assets":"no_fetch","providers":"not_run"})).unwrap();
    let p = CodeRustRuntimeProfile {
        schema: "oev2.rust-runtime-profile/1".into(),
        profile: runtime_identity::PROFILE.into(),
        language: "rust".into(),
        source_kind: if git.is_some() {
            "git_checkout"
        } else {
            "frozen_export"
        }
        .into(),
        source_manifest: reference(&manifest),
        expected_target: "aarch64-apple-darwin".into(),
        expected_package_version: "2.0.0-dev.3".into(),
        expected_build_capture: None,
        expected_git_commit: git.as_ref().map(|x| x.0.clone()),
        git: git.map(|x| x.1),
        stage_expectations: reference(&stages),
        network_policy: reference(&network),
    };
    let file = state.join(format!("{label}-profile.json"));
    write_new(&file, &serde_json::to_value(p).unwrap()).unwrap();
    reference(&file)
}
fn small_inspect(root: &Path, p: ArtifactReference, label: &str) -> CodeRustRuntimeInspectRequest {
    CodeRustRuntimeInspectRequest {
        schema: "oev2.rust-runtime-inspect-request/1".into(),
        profile: p,
        source_root: root.to_str().unwrap().into(),
        evidence_path: root.join("var").join(label).to_str().unwrap().into(),
        budget: CodeBudget {
            timeout_ms: 30000,
            output_bytes: 65536,
        },
    }
}
#[test]
fn source_lock_spec_template_vendor_and_normalizer_pins_are_actual_preflight_boundaries() {
    for file in [
        "Cargo.lock",
        "specs/conditions.toml",
        "templates/object.tmpl",
        "vendor/algorithm.rs",
        "build_support/normalizer.rs",
    ] {
        let (root, state) = small_source();
        let p = small_profile(&root, &state, None, "locked");
        let r = small_inspect(&root, p, "reject");
        std::fs::write(root.join(file), "different real source bytes\n").unwrap();
        let e = format!("{:#}", inspect(&r).unwrap_err());
        assert!(e.contains("source_manifest_mismatch"), "{e}");
        assert!(!Path::new(&r.evidence_path).exists());
    }
    for mode in ["missing", "extra"] {
        let (root, state) = small_source();
        let p = small_profile(&root, &state, None, "locked");
        let r = small_inspect(&root, p, "reject");
        if mode == "missing" {
            std::fs::remove_file(root.join("Cargo.lock")).unwrap()
        } else {
            std::fs::write(root.join("unexpected-public.rs"), "extra").unwrap()
        };
        assert!(format!("{:#}", inspect(&r).unwrap_err()).contains("source_manifest_mismatch"));
        assert!(!Path::new(&r.evidence_path).exists());
    }
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let v = Command::new("/usr/bin/git")
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(v.status.success(), "{}", String::from_utf8_lossy(&v.stderr));
    v.stdout
}
#[test]
fn actual_git_head_sparse_promisor_and_dirty_checks_precede_build_dispatch() {
    let (root, state) = small_source();
    git(&root, &["init", "--quiet"]);
    git(&root, &["add", "."]);
    git(
        &root,
        &[
            "-c",
            "user.name=Frozen Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "frozen native source",
        ],
    );
    let head = String::from_utf8(git(&root, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_owned();
    let tool = binary_reference(Path::new("/usr/bin/git"), &root).unwrap();
    let p = small_profile(&root, &state, Some((head.clone(), tool.clone())), "correct");
    let r = small_inspect(&root, p, "positive");
    let result = inspect(&r).unwrap();
    assert_eq!(result.identity, "bound");
    assert_eq!(result.proof["git_processes"].as_array().unwrap().len(), 7);
    assert_eq!(result.proof["build_dispatched"], false);
    let p = small_profile(
        &root,
        &state,
        Some(("0".repeat(40), tool.clone())),
        "wrong-head",
    );
    assert!(
        format!(
            "{:#}",
            inspect(&small_inspect(&root, p, "wrong-head-evidence")).unwrap_err()
        )
        .contains("git_commit_mismatch")
    );
    git(&root, &["config", "core.sparseCheckout", "true"]);
    let p = small_profile(&root, &state, Some((head.clone(), tool.clone())), "sparse");
    assert!(
        format!(
            "{:#}",
            inspect(&small_inspect(&root, p, "sparse-evidence")).unwrap_err()
        )
        .contains("git_sparse")
    );
    git(&root, &["config", "--unset", "core.sparseCheckout"]);
    git(&root, &["config", "remote.origin.promisor", "true"]);
    let p = small_profile(&root, &state, Some((head.clone(), tool)), "promisor");
    assert!(
        format!(
            "{:#}",
            inspect(&small_inspect(&root, p, "promisor-evidence")).unwrap_err()
        )
        .contains("git_partial_clone")
    );
    git(&root, &["config", "--unset", "remote.origin.promisor"]);
    std::fs::write(root.join("Cargo.lock"), "modified actual worktree bytes\n").unwrap();
    let tool = binary_reference(Path::new("/usr/bin/git"), &root).unwrap();
    let p = small_profile(&root, &state, Some((head, tool)), "dirty");
    assert!(
        format!(
            "{:#}",
            inspect(&small_inspect(&root, p, "dirty-evidence")).unwrap_err()
        )
        .contains("git_dirty")
    );
    assert!(!root.join("var/build-normalizer").exists());
}
#[test]
fn duplicate_unknown_and_forged_capture_inputs_do_not_authorize_native_identity() {
    let duplicated = br#"{"schema":"oev2.rust-runtime-profile/1","schema":"replacement"}"#;
    assert!(oe_core::json::strict_json(duplicated).is_err());
    let (root, state) = small_source();
    let p = small_profile(&root, &state, None, "valid");
    let mut raw: Value = serde_json::from_slice(&std::fs::read(&p.path).unwrap()).unwrap();
    raw["passed"] = json!(true);
    assert!(serde_json::from_value::<CodeRustRuntimeProfile>(raw).is_err());
    let forged = json!({"status":"completed","exit_code":0,"capture_complete":true,"compiled":{"language":"rust","passed":true}});
    assert!(serde_json::from_value::<CodeRustRuntimeObservation>(forged).is_err());
    let mut raw: Value = serde_json::from_slice(&std::fs::read(&p.path).unwrap()).unwrap();
    raw["network_policy"]["sha256"] = json!("0".repeat(64));
    let file = state.join("forged-policy.json");
    write_new(&file, &raw).unwrap();
    assert!(
        format!(
            "{:#}",
            inspect(&small_inspect(
                &root,
                reference(&file),
                "wrong-policy-evidence"
            ))
            .unwrap_err()
        )
        .contains("reference_changed")
    );
}

#[test]
fn independent_driver_stage_exit37_retains_raw_output_and_stops_publication() {
    let dir = private();
    let compiler = std::env::var_os("RUSTC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("rustc"));
    let worker = dir.join("cargo-exit37");
    assert!(
        Command::new(compiler)
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/runtime_worker.rs"))
            .args(["--edition", "2024", "-o"])
            .arg(&worker)
            .status()
            .unwrap()
            .success()
    );
    let p = profile(&dir);
    let root = root();
    let build = CodeBuildRequest {
        artifact_id: "failed-native-first".into(),
        source_root: root.to_str().unwrap().into(),
        evidence_path: dir.join("first").to_str().unwrap().into(),
        cargo_path: worker.to_str().unwrap().into(),
        rustc_path: String::from_utf8(
            Command::new("rustup")
                .args(["which", "--toolchain", "1.95.0", "rustc"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .into(),
        ggen_path: root
            .join("var/builds/ggen-target/debug/ggen")
            .to_str()
            .unwrap()
            .into(),
        cargo_home: "unused-native-exit37-fixture-cache".into(),
        target_dir: dir.join("target-a").to_str().unwrap().into(),
        platform: "aarch64-apple-darwin".into(),
    };
    let mut second = build.clone();
    second.artifact_id = "failed-native-second".into();
    second.evidence_path = dir.join("second").to_str().unwrap().into();
    second.target_dir = dir.join("target-b").to_str().unwrap().into();
    let r = CodeRustRuntimeReproductionRequest {
        schema: "oev2.rust-runtime-reproduction-request/1".into(),
        profile: p,
        first: build,
        second,
        budget: CodeBudget {
            timeout_ms: 30000,
            output_bytes: 65536,
        },
        evidence_path: dir.join("reproduction").to_str().unwrap().into(),
    };
    let input = dir.join("request.json");
    write_new(&input, &serde_json::to_value(&r).unwrap()).unwrap();
    let actual = Command::new(env!("CARGO_BIN_EXE_oe-evolution-driver"))
        .arg("runtime-build")
        .arg(&input)
        .current_dir(&root)
        .output()
        .unwrap();
    std::fs::write(dir.join("observer.stdout"), &actual.stdout).unwrap();
    std::fs::write(dir.join("observer.stderr"), &actual.stderr).unwrap();
    assert!(!actual.status.success());
    let result: CodeRustRuntimeReproductionObservation =
        serde_json::from_value(oe_core::json::strict_json(&actual.stdout).unwrap()).unwrap();
    assert_eq!(result.execution, "failed");
    assert_eq!(result.reproduction, "unknown");
    assert!(result.published_descriptor.is_none());
    let stage = dir.join("first/build-normalizer");
    let observed =
        oe_core::json::strict_json(&std::fs::read(stage.join("process.json")).unwrap()).unwrap();
    assert_eq!(observed["status"], "process_failed");
    assert_eq!(observed["exit_code"], 37);
    assert_eq!(
        std::fs::read(stage.join("stdout")).unwrap(),
        b"native fixture prior output\n"
    );
    assert_eq!(
        std::fs::read(stage.join("stderr")).unwrap(),
        b"fixture failure\n"
    );
    assert!(!dir.join("first/actual-ggen").exists());
    assert!(!dir.join("second").exists());
    assert!(!dir.join("reproduction/staging-descriptor.json").exists());
}

#[test]
fn absent_original_independent_capture_is_unknown_before_binary_execution() {
    let dir = private();
    let p = profile(&dir);
    let file = dir.join("copied-user-data.json");
    write_new(
        &file,
        &json!({"compiled":{"passed":true},"build_processes":[{"passed":true}]}),
    )
    .unwrap();
    let r = CodeRustRuntimeProbeRequest {
        schema: "oev2.rust-runtime-probe-request/1".into(),
        profile: p,
        artifact: reference(&file),
        source_root: root().to_str().unwrap().into(),
        budget: CodeBudget {
            timeout_ms: 30000,
            output_bytes: 65536,
        },
        phase: "probe".into(),
        evidence_path: dir.join("observer").to_str().unwrap().into(),
    };
    assert!(
        format!("{:#}", runtime_identity::probe(&r).unwrap_err())
            .contains("original_build_capture_missing")
    );
    assert!(!Path::new(&r.evidence_path).exists());
}

#[test]
fn caller_selected_capture_never_becomes_compilation_authority() {
    let unknown = CodeRustRuntimeObservation {
        schema: "oev2.rust-runtime-observation/1".into(),
        profile_sha256: "a".repeat(64),
        artifact_sha256: "b".repeat(64),
        language: "rust".into(),
        execution: "completed".into(),
        identity: "unknown".into(),
        observed_match: "matched".into(),
        capture_trust: "untrusted".into(),
        compilation_origin: "unknown".into(),
        process: ArtifactReference {
            path: "var/actual/process.json".into(),
            sha256: "c".repeat(64),
        },
        stdout: ArtifactReference {
            path: "var/actual/stdout".into(),
            sha256: "d".repeat(64),
        },
        stderr: ArtifactReference {
            path: "var/actual/stderr".into(),
            sha256: "e".repeat(64),
        },
        compiled: Some(json!({"package_name":"oe-code-runner"})),
        semantic_execution: "not_run".into(),
        uncovered: vec![
            "prior experiment/owner selection and capture authority not_connected".into(),
        ],
    };
    unknown.validate().unwrap();
    let mut invalid = unknown.clone();
    invalid.identity = "bound".into();
    assert!(invalid.validate().is_err());
    let mut invalid = unknown.clone();
    invalid.capture_trust = "trusted".into();
    assert!(invalid.validate().is_err());
    invalid = unknown;
    invalid.compilation_origin = "verified".into();
    assert!(invalid.validate().is_err());
}
