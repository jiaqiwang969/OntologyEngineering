#![cfg(unix)]
//! Controlled real pipe/file captures. The temporary executable is a fixture,
//! never actual Cargo/native acceptance or permission to execute a Project.
use oe_core::process::{CommandSpec, SpoolLimits, SpoolProcessObservation, run_command_spooled};
use oe_core::{SkillRoot, fingerprint_file, sha256, write_new, write_new_bytes};
use oe_evolution::native_loader11_content::*;
use oe_evolution::native_stream11::*;
use oe_evolution::*;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ID: &str = "5FB385BE-2B4C-31AE-AE92-ED6355A92C2D";

fn original_text() -> Vec<u8> {
    let mut bytes = Vec::new();
    let row = [vec![b'u'; 63], vec![b'\n']].concat();
    for _ in 0..16383 {
        bytes.extend_from_slice(&row);
    }
    bytes.extend_from_slice(&vec![b'x'; 62]);
    bytes.push(b'\n');
    assert_eq!(bytes.len(), 1024 * 1024 - 1);
    bytes.extend_from_slice(format!("中\ndyld[31415]: <{ID}> /original/image\ndyld[31415]: move loaded to delayed: image\noriginal warning\nlast 未理解").as_bytes());
    bytes.push(b'\n');
    for _ in 0..16383 {
        bytes.extend_from_slice(&row);
    }
    bytes.extend_from_slice(b"final original without newline");
    bytes
}

#[test]
fn selected_stream_fixture() {
    let Some(kind) = std::env::var_os("STREAM11_FIXTURE_KIND") else {
        return;
    };
    let bytes = match kind.to_str().unwrap() {
        "complete" => original_text(),
        "binary" => vec![b'a', 0, 0xff, b'\n'],
        "nul" => vec![b'a', 0, b'\n'],
        "invalid" => vec![0xff, b'\n'],
        "line_limit" => vec![b'a'; 64 * 1024 + 1],
        "empty" => vec![],
        "fail" => {
            std::io::stderr().write_all(b"original nonzero\n").unwrap();
            std::process::exit(7);
        }
        "sleep" => {
            std::io::stderr().write_all(b"partial original\n").unwrap();
            std::thread::sleep(Duration::from_secs(3));
            return;
        }
        _ => panic!("unknown controlled fixture"),
    };
    std::io::stderr().write_all(&bytes).unwrap();
}

fn reference(path: &Path) -> ArtifactReference {
    ArtifactReference {
        path: path.to_str().unwrap().into(),
        sha256: sha256(std::fs::read(path).unwrap()),
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: SkillRoot,
    stage: PathBuf,
    plan: CodeNativeCargoStreamPlan11,
    spec: CommandSpec,
}
impl Fixture {
    fn new(kind: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().canonicalize().unwrap();
        std::fs::write(
            folder.join("SKILL.md"),
            b"controlled stream content fixture; native unknown",
        )
        .unwrap();
        let root = SkillRoot::new(&folder).unwrap();
        let stage = root
            .new_private_dir(Path::new("var/evidence/build-runner"))
            .unwrap();
        let inputs = root.new_private_dir(Path::new("var/inputs")).unwrap();
        let context_folder = root
            .new_private_dir(Path::new("var/stages/build-runner"))
            .unwrap();
        let executable = inputs.join("cargo-fixture.sh");
        let quoted = std::env::current_exe()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\'', "'\\''");
        write_new_bytes(
            &executable,
            format!("#!/bin/sh\nexec '{quoted}' --exact selected_stream_fixture --nocapture\n")
                .as_bytes(),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let identity = fingerprint_file(&executable, 512 * 1024 * 1024).unwrap();
        let binding = CodeNativeFileBinding {
            path: executable.to_str().unwrap().into(),
            sha256: identity.sha256,
            bytes: identity.bytes,
            mode: 0o700,
        };
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let deadline = epoch + 60_000;
        let environment = BTreeMap::from([("STREAM11_FIXTURE_KIND".to_owned(), kind.to_owned())]);
        let mut removed = std::env::vars_os()
            .map(|(k, _)| k)
            .filter(|k| k != "STREAM11_FIXTURE_KIND")
            .collect::<Vec<_>>();
        removed.sort();
        removed.dedup();
        let arguments: Vec<String> = [
            "build",
            "--locked",
            "--offline",
            "-j4",
            "--target",
            "aarch64-apple-darwin",
            "-p",
            "oe-code-runner",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let command = stage.with_extension("command.json");
        write_new(&command, &json!({"schema":"oev2.native-command-before-dispatch/1", "executable":binding,
            "arguments":arguments,"cwd":folder,"environment":environment,
            "removed_environment":removed.iter().map(|s|s.to_str().unwrap()).collect::<Vec<_>>(),
            "timeout_seconds":10.0,"output_bytes":4*1024*1024,"prepared_epoch_ms":epoch,"deadline_epoch_ms":deadline})).unwrap();
        let side = inputs.join("side.json");
        write_new(&side, &json!({"schema":"controlled-original-side-content-fixture", "build":{
            "source_root":folder,"cargo_path":executable,"evidence_path":stage.parent().unwrap(),"platform":"aarch64-apple-darwin"},
            "body_budget":{"output_bytes":4*1024*1024,"timeout_ms":60_000},"stage_root":context_folder.parent().unwrap()})).unwrap();
        let manifest = public_manifest(&folder).unwrap();
        let context = context_folder.join("stage-context.json");
        write_new(&context, &json!({"schema":"oev2.native-build-stage/6","source_root":folder,
            "source_manifest":manifest,"side_context":reference(&side),"parent_stage":"build-runner",
            "output_bytes":4*1024*1024,"deadline_epoch_ms":deadline})).unwrap();
        let plan = CodeNativeCargoStreamPlan11 {
            schema: "oev2.native-cargo-stream-plan/11".into(),
            source_root: folder.to_str().unwrap().into(),
            source_manifest_sha256: digest(&manifest).unwrap(),
            side_context: reference(&side),
            stage_context: reference(&context),
            parent_stage: "build-runner".into(),
            declared_command: reference(&command),
            body_output_bytes: 4 * 1024 * 1024,
            body_timeout_ms: 60_000,
            deadline_epoch_ms: deadline,
            resources: selected_resources11(),
        };
        write_new(
            &stage.join("stream-plan11.json"),
            &serde_json::to_value(&plan).unwrap(),
        )
        .unwrap();
        let spec = declared_spec_content11(&root, &plan).unwrap();
        Self {
            _temp: temp,
            root,
            stage,
            plan,
            spec,
        }
    }
    fn capture(&self) -> (SpoolProcessObservation, ArtifactReference) {
        let mut spec = declared_spec_content11(&self.root, &self.plan).unwrap();
        spec.output_limit = self.plan.resources.raw_stream_bytes as usize;
        let observation = run_command_spooled(
            &spec,
            &self.stage.join("raw-stream11"),
            Instant::now() + Duration::from_secs(10),
            SpoolLimits {
                preview_bytes: self.plan.resources.preview_bytes as usize,
                segment_bytes: self.plan.resources.segment_bytes as usize,
            },
        )
        .unwrap();
        observation.verify_retained().unwrap();
        let capture = CodeNativeCargoStreamCapture11 {
            schema: "oev2.native-cargo-stream-capture/11".into(),
            plan: reference(&self.stage.join("stream-plan11.json")),
            process: reference(&self.stage.join("raw-stream11/process.json")),
            stdout_sha256: observation.report["stdout"]["sha256"]
                .as_str()
                .unwrap()
                .into(),
            stdout_bytes: observation.report["stdout"]["bytes"].as_u64().unwrap(),
            stderr_sha256: observation.report["stderr"]["sha256"]
                .as_str()
                .unwrap()
                .into(),
            stderr_bytes: observation.report["stderr"]["bytes"].as_u64().unwrap(),
            qualification: "complete_bounded_stream_content_only".into(),
        };
        let path = self.stage.join("stream-capture11.json");
        write_new(&path, &serde_json::to_value(capture).unwrap()).unwrap();
        (observation, reference(&path))
    }
    fn replace_process(&self, mutator: impl FnOnce(&mut Value)) -> ArtifactReference {
        let path = self.stage.join("raw-stream11/process.json");
        let mut v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        mutator(&mut v);
        std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
        let c = self.stage.join("stream-capture11.json");
        let mut value: Value = serde_json::from_slice(&std::fs::read(&c).unwrap()).unwrap();
        value["process"] = serde_json::to_value(reference(&path)).unwrap();
        std::fs::write(&c, serde_json::to_vec(&value).unwrap()).unwrap();
        reference(&c)
    }
}

#[test]
fn complete_segmented_utf8_lines_replay_original_rules() {
    let f = Fixture::new("complete");
    let (actual, capture) = f.capture();
    assert_eq!(actual.status, "completed");
    let content = inspect_capture_content11(&f.root, &capture).unwrap();
    let mut raw = Vec::new();
    content
        .visit_stream("stderr", |_, bytes| {
            raw.extend_from_slice(bytes);
            Ok(())
        })
        .unwrap();
    assert_eq!(raw, original_text());
    let old =
        native_runtime5_content::inspect_loader_diagnostic_bytes("host_compilation", &raw).unwrap();
    let report = prepare_loader_stream_content11(&f.root, &capture).unwrap();
    assert!(report.pages.len() > 1);
    let mut lines = Vec::new();
    for r in &report.pages {
        let page: CodeNativeLoaderPage11 =
            serde_json::from_slice(&std::fs::read(&r.path).unwrap()).unwrap();
        lines.extend(page.lines);
    }
    assert_eq!(
        serde_json::to_value(lines).unwrap(),
        serde_json::to_value(&old.lines).unwrap()
    );
    assert_eq!(report.raw_sha256, sha256(&raw));
    assert_eq!(report.raw_bytes, raw.len() as u64);
    verify_loader_stream_content11(&f.root, &report).unwrap();
    let dto: CodeNativeCargoStreamCapture11 =
        serde_json::from_slice(&std::fs::read(&capture.path).unwrap()).unwrap();
    let port = CargoStreamContent11Inspector {
        root: SkillRoot::new(f.root.path()).unwrap(),
    };
    assert_eq!(
        serde_json::to_value(port.inspect_cargo_stream_content(&dto).unwrap()).unwrap(),
        serde_json::to_value(&report).unwrap()
    );
    let empty = Fixture::new("empty");
    let (_, cap) = empty.capture();
    let report = prepare_loader_stream_content11(&empty.root, &cap).unwrap();
    assert_eq!(report.raw_bytes, 0);
    assert!(report.pages.is_empty());
    assert_eq!(report.line_count, 0);
}

#[test]
fn binary_raw_is_retained_while_loader_parse_refuses_nul_and_utf8() {
    for kind in ["binary", "nul", "invalid"] {
        let f = Fixture::new(kind);
        let (actual, capture) = f.capture();
        assert_eq!(actual.status, "completed");
        inspect_capture_content11(&f.root, &capture).unwrap();
        assert!(prepare_loader_stream_content11(&f.root, &capture).is_err());
        assert!(!f.stage.join("loader-content11.json").exists());
        actual.verify_retained().unwrap();
    }
}

#[test]
fn line_and_derived_limits_refuse_complete_report() {
    let f = Fixture::new("line_limit");
    let (_, capture) = f.capture();
    assert!(prepare_loader_stream_content11(&f.root, &capture).is_err());
    assert!(!f.stage.join("loader-content11.json").exists());
    let f = Fixture::new("complete");
    let (_, capture) = f.capture();
    let mut report = prepare_loader_stream_content11(&f.root, &capture).unwrap();
    let page = Path::new(&report.pages[0].path);
    std::fs::write(page, vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
    report.pages[0] = reference(page);
    assert!(verify_loader_stream_content11(&f.root, &report).is_err());
}

#[test]
fn missing_extra_changed_reordered_or_swapped_inode_segments_refuse() {
    let f = Fixture::new("complete");
    let (_, capture) = f.capture();
    let segment = f
        .stage
        .join("raw-stream11/stderr-segments/segment-00000.bin");
    let saved = f.root.path().join("var/saved-segment.bin");
    std::fs::rename(&segment, &saved).unwrap();
    assert!(inspect_capture_content11(&f.root, &capture).is_err());
    std::fs::rename(&saved, &segment).unwrap();
    inspect_capture_content11(&f.root, &capture).unwrap();
    let extra = segment.parent().unwrap().join("extra.bin");
    write_new_bytes(&extra, b"extra").unwrap();
    assert!(inspect_capture_content11(&f.root, &capture).is_err());
    std::fs::remove_file(extra).unwrap();
    let bytes = std::fs::read(&segment).unwrap();
    let mut changed = bytes.clone();
    changed[0] ^= 1;
    std::fs::write(&segment, changed).unwrap();
    assert!(inspect_capture_content11(&f.root, &capture).is_err());
    std::fs::write(&segment, &bytes).unwrap();
    std::fs::rename(&segment, &saved).unwrap();
    write_new_bytes(&segment, &bytes).unwrap();
    let error = inspect_capture_content11(&f.root, &capture).err().unwrap();
    assert!(format!("{error:#}").contains("physical_content"));
    std::fs::remove_file(&segment).unwrap();
    std::fs::rename(&saved, &segment).unwrap();
    std::fs::rename(&segment, &saved).unwrap();
    std::os::unix::fs::symlink(&saved, &segment).unwrap();
    assert!(inspect_capture_content11(&f.root, &capture).is_err());
    std::fs::remove_file(&segment).unwrap();
    std::fs::rename(&saved, &segment).unwrap();
    let changed = f.replace_process(|v| v["stderr"]["segments"].as_array_mut().unwrap().swap(0, 1));
    assert!(inspect_capture_content11(&f.root, &changed).is_err());
}

#[test]
fn wrong_plan_stage_side_source_or_command_refuse_content() {
    let f = Fixture::new("empty");
    for change in ["stage", "source", "side", "body", "deadline"] {
        let mut plan = f.plan.clone();
        match change {
            "stage" => plan.parent_stage = "build-normalizer".into(),
            "source" => plan.source_manifest_sha256 = "0".repeat(64),
            "side" => plan.side_context.sha256 = "0".repeat(64),
            "body" => plan.body_output_bytes += 1,
            "deadline" => plan.deadline_epoch_ms += 1,
            _ => unreachable!(),
        }
        assert!(verify_plan_content11(&f.root, &plan).is_err());
    }
    let command = Path::new(&f.plan.declared_command.path);
    let mut value: Value = serde_json::from_slice(&std::fs::read(command).unwrap()).unwrap();
    value["arguments"][0] = json!("test");
    std::fs::write(command, serde_json::to_vec(&value).unwrap()).unwrap();
    let mut plan = f.plan.clone();
    plan.declared_command = reference(command);
    assert!(verify_plan_content11(&f.root, &plan).is_err());
    std::fs::write(f.root.path().join("SKILL.md"), b"changed current source").unwrap();
    assert!(verify_plan_content11(&f.root, &f.plan).is_err());
}

#[test]
fn expired_deadline_or_nonzero_process_never_yields_complete_capture() {
    let f = Fixture::new("fail");
    let (actual, capture) = f.capture();
    assert_eq!(actual.status, "process_failed");
    assert_eq!(actual.exit_code, Some(7));
    assert!(inspect_capture_content11(&f.root, &capture).is_err());
    let f = Fixture::new("sleep");
    let mut spec = f.spec;
    spec.output_limit = 64 * 1024 * 1024;
    let actual = run_command_spooled(
        &spec,
        &f.stage.join("raw-stream11"),
        Instant::now() + Duration::from_millis(150),
        SpoolLimits {
            preview_bytes: 4 * 1024 * 1024,
            segment_bytes: 1024 * 1024,
        },
    )
    .unwrap();
    assert_eq!(actual.status, "timed_out");
    assert_eq!(actual.report["capture_complete"], false);
    actual.verify_retained().unwrap();
    assert!(!f.stage.join("stream-capture11.json").exists());
    let f = Fixture::new("empty");
    assert!(
        run_command_spooled(
            &f.spec,
            &f.stage.join("raw-stream11"),
            Instant::now() - Duration::from_millis(1),
            SpoolLimits {
                preview_bytes: 4 * 1024 * 1024,
                segment_bytes: 1024 * 1024
            }
        )
        .is_err()
    );
    assert!(!f.stage.join("raw-stream11").exists());
}

#[test]
fn reported_pids_remain_content_only() {
    let f = Fixture::new("complete");
    let (_, capture) = f.capture();
    let report = prepare_loader_stream_content11(&f.root, &capture).unwrap();
    let ordinals = reported_line_ordinals11(&f.root, &report, 31415).unwrap();
    assert_eq!(ordinals.len(), 1);
    assert!(
        reported_line_ordinals11(&f.root, &report, 99999)
            .unwrap()
            .is_empty()
    );
    assert_eq!(report.qualification, "diagnostic_content_only");
    assert!(report.unknown.iter().any(|s| s.contains("live_owner")));
    let mut changed = report.clone();
    changed.qualification = "native_complete".into();
    assert!(verify_loader_stream_content11(&f.root, &changed).is_err());
    let mut changed = report;
    changed.unknown.clear();
    assert!(verify_loader_stream_content11(&f.root, &changed).is_err());
}

#[test]
fn strict_contract_rejects_legacy_or_unselected_resources() {
    let original = selected_resources11();
    verify_resources11(&original).unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    value.as_object_mut().unwrap().remove("raw_stream_bytes");
    assert!(serde_json::from_value::<CodeNativeCargoStreamResources11>(value).is_err());
    let mut value = serde_json::to_value(&original).unwrap();
    value["legacy_output_bytes"] = json!(4 * 1024 * 1024);
    assert!(serde_json::from_value::<CodeNativeCargoStreamResources11>(value).is_err());
    for change in ["legacy", "raw", "preview", "segment", "derived"] {
        let mut resource = original.clone();
        match change {
            "legacy" => resource.schema = "oev2.native-cargo-stream-resources/10".into(),
            "raw" => resource.raw_stream_bytes += 1,
            "preview" => resource.preview_bytes += 1,
            "segment" => resource.segment_bytes /= 2,
            "derived" => resource.maximum_derived_bytes += 1,
            _ => unreachable!(),
        };
        assert!(verify_resources11(&resource).is_err());
    }
}

#[test]
fn borrowed_stage_adapter_does_not_publish_full_native_execution() {
    let f = Fixture::new("empty");
    let input = CodeNativeBudgetedBuildRequest10 {
        schema: "oev2.native-budgeted-build-request/10".into(),
        build: CodeBuildRequest {
            artifact_id: "controlled-no-native-permission".into(),
            source_root: f.plan.source_root.clone(),
            evidence_path: f.stage.to_str().unwrap().into(),
            target_dir: f.root.path().join("var/target").to_str().unwrap().into(),
            cargo_home: f
                .root
                .path()
                .join("var/cargo-home")
                .to_str()
                .unwrap()
                .into(),
            cargo_path: f.spec.executable.to_str().unwrap().into(),
            rustc_path: "/usr/bin/true".into(),
            ggen_path: "/usr/bin/true".into(),
            platform: "aarch64-apple-darwin".into(),
        },
        budget: CodeBudget {
            timeout_ms: 60_000,
            output_bytes: selected_resources11().raw_stream_bytes,
        },
        side_context: f.plan.side_context.clone(),
    };
    let error = native_runtime10::build_full_native_budgeted(&input).unwrap_err();
    assert!(format!("{error:#}").contains("schema_or_bounds"));
    assert!(!f.root.path().join("var/target").exists());
    assert!(!f.stage.join("native-probe10-capture.json").exists());
}
