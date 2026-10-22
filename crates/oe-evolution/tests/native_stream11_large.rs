#![cfg(unix)]
//! Controlled real pipe/file captures. The temporary executable is a fixture,
//! never actual Cargo/native acceptance or permission to execute a Project.
use oe_core::process::{SpoolLimits, SpoolProcessObservation, run_command_spooled};
use oe_core::{SkillRoot, fingerprint_file, sha256, write_new, write_new_bytes};
use oe_evolution::native_loader11_content::*;
use oe_evolution::native_stream11::*;
use oe_evolution::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ID: &str = "5FB385BE-2B4C-31AE-AE92-ED6355A92C2D";
fn expected_text() -> Vec<u8> {
    let row = [vec![b'q'; 4095], vec![b'\n']].concat();
    let mut bytes = row.repeat(1280);
    assert_eq!(bytes.len(), 5 * 1024 * 1024);
    bytes.extend_from_slice(format!("dyld[27182]: <{ID}> /after/old/limit\ndyld[27182]: move loaded to delayed: tail\n末行 未理解").as_bytes());
    bytes
}
#[test]
fn large_stream_fixture() {
    let Some(kind) = std::env::var_os("STREAM11_LARGE_FIXTURE_KIND") else {
        return;
    };
    match kind.to_str().unwrap() {
        "complete" => std::io::stderr()
            .lock()
            .write_all(&expected_text())
            .unwrap(),
        "overflow" => {
            let block = vec![b'z'; 1024 * 1024];
            let mut output = std::io::stderr().lock();
            for _ in 0..65 {
                if output.write_all(&block).is_err() {
                    return;
                }
            }
        }
        _ => panic!("unknown fixture"),
    }
}

#[test]
fn complete_stream_beyond_old_preview_retains_exact_raw_and_final_lines() {
    let fixture = Fixture::new("complete");
    let (actual, reference) = fixture.capture();
    assert_eq!(actual.status, "completed");
    assert_eq!(actual.exit_code, Some(0));
    assert_eq!(actual.stderr.len(), 4 * 1024 * 1024);
    assert_eq!(actual.report["capture_complete"], true);
    let expected = expected_text();
    assert!(expected.len() > 4 * 1024 * 1024);
    assert!(
        native_runtime5_content::inspect_loader_diagnostic_bytes("host_compilation", &expected)
            .is_err()
    );
    assert_eq!(actual.report["stderr"]["bytes"], expected.len());
    assert_eq!(actual.report["stderr"]["sha256"], sha256(&expected));
    assert_eq!(
        actual.report["stderr"]["segments"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let content = inspect_capture_content11(&fixture.root, &reference).unwrap();
    let mut raw = Vec::new();
    content
        .visit_stream("stderr", |_, bytes| {
            raw.extend_from_slice(bytes);
            Ok(())
        })
        .unwrap();
    assert_eq!(raw, expected);
    let report = prepare_loader_stream_content11(&fixture.root, &reference).unwrap();
    assert_eq!(report.line_count, 1283);
    assert!(report.pages.len() >= 2);
    let mut lines = Vec::new();
    for page in &report.pages {
        let bytes = std::fs::read(&page.path).unwrap();
        assert_eq!(sha256(&bytes), page.sha256);
        let page: CodeNativeLoaderPage11 = serde_json::from_slice(&bytes).unwrap();
        lines.extend(page.lines);
    }
    let expected_lines = std::str::from_utf8(&expected)
        .unwrap()
        .split_inclusive('\n')
        .collect::<Vec<_>>();
    let mut offset = 0;
    for (ordinal, (line, text)) in lines.iter().zip(&expected_lines).enumerate() {
        assert_eq!(line.ordinal, ordinal as u64);
        assert_eq!(line.byte_offset, offset);
        assert_eq!(&line.raw_text, text);
        offset += text.len() as u64;
        match ordinal {
            1280 => {
                assert_eq!(line.event, "uuid_path_report");
                assert_eq!(line.reported_pid, Some(27182));
                assert_eq!(line.reported_uuid.as_deref(), Some(ID));
                assert_eq!(line.reported_path.as_deref(), Some("/after/old/limit"));
            }
            1281 => assert_eq!(line.event, "delayed_state_report"),
            _ => assert_eq!(line.event, "unrecognized"),
        }
    }
    assert_eq!(lines.len(), expected_lines.len());
    assert_eq!(offset, expected.len() as u64);
    assert_eq!(lines.last().unwrap().raw_text, "末行 未理解");
    assert_eq!(
        reported_line_ordinals11(&fixture.root, &report, 27182).unwrap(),
        vec![1280]
    );
    assert_eq!(report.qualification, "diagnostic_content_only");
    assert!(!report.unknown.is_empty());
    verify_loader_stream_content11(&fixture.root, &report).unwrap();
    content.verify_current().unwrap();
    actual.verify_retained().unwrap();
}

#[test]
fn raw_overflow_preserves_prefix_and_refuses_complete_capture() {
    let fixture = Fixture::new("overflow");
    let (actual, reference) = fixture.capture();
    assert_eq!(actual.status, "output_limit");
    assert_eq!(actual.report["capture_complete"], false);
    assert_eq!(actual.report["stderr"]["bytes"], 64 * 1024 * 1024);
    assert_eq!(actual.report["stderr"]["truncated"], true);
    assert_eq!(
        actual.report["stderr"]["observed_bytes_at_least"],
        64 * 1024 * 1024 + 1
    );
    assert_eq!(actual.stderr.len(), 4 * 1024 * 1024);
    let segments = actual.report["stderr"]["segments"].as_array().unwrap();
    assert_eq!(segments.len(), 64);
    for (ordinal, segment) in segments.iter().enumerate() {
        assert_eq!(segment["ordinal"], ordinal);
        let bytes = std::fs::read(segment["path"].as_str().unwrap()).unwrap();
        assert_eq!(bytes, vec![b'z'; 1024 * 1024]);
    }
    actual.verify_retained().unwrap();
    assert!(inspect_capture_content11(&fixture.root, &reference).is_err());
    assert!(prepare_loader_stream_content11(&fixture.root, &reference).is_err());
    assert!(!fixture.stage.join("loader-content11.json").exists());
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
            format!("#!/bin/sh\nexec '{quoted}' --exact large_stream_fixture --nocapture\n")
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
        let deadline = epoch + 180_000;
        let environment =
            BTreeMap::from([("STREAM11_LARGE_FIXTURE_KIND".to_owned(), kind.to_owned())]);
        let mut removed = std::env::vars_os()
            .map(|(k, _)| k)
            .filter(|k| k != "STREAM11_LARGE_FIXTURE_KIND")
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
            "timeout_seconds":120.0,"output_bytes":4*1024*1024,"prepared_epoch_ms":epoch,"deadline_epoch_ms":deadline})).unwrap();
        let side = inputs.join("side.json");
        write_new(&side, &json!({"schema":"controlled-original-side-content-fixture", "build":{
            "source_root":folder,"cargo_path":executable,"evidence_path":stage.parent().unwrap(),"platform":"aarch64-apple-darwin"},
            "body_budget":{"output_bytes":4*1024*1024,"timeout_ms":180_000},"stage_root":context_folder.parent().unwrap()})).unwrap();
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
            body_timeout_ms: 180_000,
            deadline_epoch_ms: deadline,
            resources: selected_resources11(),
        };
        write_new(
            &stage.join("stream-plan11.json"),
            &serde_json::to_value(&plan).unwrap(),
        )
        .unwrap();
        Self {
            _temp: temp,
            root,
            stage,
            plan,
        }
    }
    fn capture(&self) -> (SpoolProcessObservation, ArtifactReference) {
        let mut spec = declared_spec_content11(&self.root, &self.plan).unwrap();
        spec.output_limit = self.plan.resources.raw_stream_bytes as usize;
        let observation = run_command_spooled(
            &spec,
            &self.stage.join("raw-stream11"),
            Instant::now() + Duration::from_secs(120),
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
}
