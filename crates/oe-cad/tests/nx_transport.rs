#![cfg(all(unix, feature = "nx-test-fixtures"))]
use base64::{Engine, engine::general_purpose::STANDARD};
use oe_cad::nx::client::{Client, DispatchUncertain};
use oe_cad::nx::transport::*;
use oe_core::SkillRoot;
use oe_nx_protocol::BlobIdentity;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Fixture {
    directory: PathBuf,
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("var/tests/nx")
            .join(format!(
                "transport-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(directory.parent().unwrap()).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("SKILL.md"), "synthetic source boundary").unwrap();
        fs::create_dir(directory.join("var")).unwrap();
        let root = SkillRoot::new(&directory).unwrap();
        Self { directory, root }
    }
    fn run(&self, args: &[&str], input: &[u8], limit: usize, timeout: Duration) -> Observation {
        let mut argv = vec![env!("CARGO_BIN_EXE_nx-transport-test-fixture").to_owned()];
        argv.extend(args.iter().map(|s| (*s).to_owned()));
        exchange(
            &self.root,
            Path::new("var/observation"),
            &Exchange {
                argv: &argv,
                input,
                timeout,
                output_limit: limit,
            },
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn profile() -> RemoteProfile {
    RemoteProfile {
        schema: "cad-agent.nx-remote-profile/v2".into(),
        ssh_command: vec!["ssh".into(), "fixture-host".into()],
        remote_root: "%USERPROFILE%\\work\\cad-agent-jobs".into(),
        nx_root: "C:\\Program Files\\Siemens\\NX 2412".into(),
        controller_path: "C:\\worker\\controller.exe".into(),
        controller: BlobIdentity::of(b"not-a-real-executable"),
        expected_root: None,
        legacy_review: None,
        dispatch_writer_review: None,
    }
}

#[test]
fn dispatch_writer_review_is_optional_and_separate_from_legacy_admission() {
    let old = serde_json::to_vec(&profile()).unwrap();
    assert!(!String::from_utf8_lossy(&old).contains("dispatch_writer_review"));
    let mut p: RemoteProfile = oe_nx_protocol::decode(&old).unwrap();
    assert!(p.dispatch_writer_review.is_none());
    assert!(
        !p.powershell_bootstrap()
            .unwrap()
            .contains("--dispatch-writer-review")
    );
    p.expected_root = Some(oe_nx_protocol::RootIdentity {
        host_id: "windows-test-host".into(),
        volume_id: "volume".into(),
        directory_id: "root".into(),
    });
    p.legacy_review = Some(LegacyReviewPin {
        remote_path: "C:/control/legacy.json".into(),
        sha256: oe_nx_protocol::Digest::of(b"legacy"),
    });
    p.dispatch_writer_review = Some(DispatchWriterReviewPin {
        remote_path: "C:/control/operator's review.json".into(),
        sha256: oe_nx_protocol::Digest::of(b"one dispatch"),
    });
    let script = p.powershell_bootstrap().unwrap();
    assert!(script.contains(
        "'--legacy-review', ([Environment]::ExpandEnvironmentVariables('C:/control/legacy.json'))"
    ));
    assert!(script.contains("'--dispatch-writer-review', ([Environment]::ExpandEnvironmentVariables('C:/control/operator''s review.json'))"));
    assert!(script.contains(&format!(
        "'--dispatch-writer-review-sha256', '{}'",
        p.dispatch_writer_review.as_ref().unwrap().sha256.as_str()
    )));
    p.expected_root = None;
    assert!(p.powershell_bootstrap().is_err());
}

#[test]
fn malformed_dispatch_writer_review_is_rejected_before_transport() {
    for pin in [
        serde_json::json!({"remote_path":"C:/review.json"}),
        serde_json::json!({"remote_path":"C:/review.json","sha256":"not-a-digest"}),
        serde_json::json!({"remote_path":"C:/review.json","sha256":oe_nx_protocol::Digest::of(b"review"),"allow_retry":true}),
    ] {
        let mut value = serde_json::to_value(profile()).unwrap();
        value["dispatch_writer_review"] = pin;
        assert!(
            oe_nx_protocol::decode::<RemoteProfile>(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
}

#[test]
fn full_duplex_bounded_payload_and_exact_raw_evidence() {
    let f = Fixture::new();
    let input = (0..1024 * 1024)
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    let o = f.run(
        &["duplex"],
        &input,
        2 * 1024 * 1024,
        Duration::from_secs(10),
    );
    assert!(o.transport_completed(), "{}", o.report);
    assert_eq!(o.stdout, input);
    assert_eq!(o.stderr, vec![b'e'; 256 * 1024]);
    assert_eq!(
        fs::read(f.directory.join("var/observation/request.bin")).unwrap(),
        input
    );
    assert_eq!(
        fs::read(f.directory.join("var/observation/stdout.bin")).unwrap(),
        input
    );
    assert_eq!(o.report["automatic_retry"], false);
    assert_eq!(o.report["input_complete"], true);
    assert!(f.directory.join("var/observation/intent.json").exists());
}

#[test]
fn closed_stdin_and_nonzero_exit_remain_transport_observations() {
    let f = Fixture::new();
    let o = f.run(&["stdin-closed"], &[], 1024, Duration::from_secs(5));
    assert!(o.transport_completed());
    assert_eq!(o.stdout, b"0\n");
    let f = Fixture::new();
    let o = f.run(&["fail"], &[], 1024, Duration::from_secs(5));
    assert_eq!(o.exit_code, Some(17));
    assert_eq!(o.status, "process_failed");
    assert_eq!(o.stdout, b"partial response");
    assert!(!o.transport_completed());
}

#[test]
fn flooding_output_is_bounded_and_never_a_complete_response() {
    let f = Fixture::new();
    let o = f.run(&["flood"], &[], 4096, Duration::from_secs(5));
    assert_eq!(o.status, "output_limit");
    assert_eq!(o.stdout.len(), 4096);
    assert_eq!(o.report["stdout"]["truncated"], true);
    assert!(!o.transport_completed());
}

#[test]
fn observation_timeout_preserves_independent_execution_and_never_restarts_it() {
    let f = Fixture::new();
    let marker = f.directory.join("var/independent-result");
    let o = f.run(
        &["start-independent", marker.to_str().unwrap()],
        &[],
        4096,
        Duration::from_millis(150),
    );
    assert_eq!(o.status, "observation_timeout");
    assert!(!o.transport_completed());
    assert!(String::from_utf8(o.stdout).unwrap().starts_with("started "));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read(&marker).unwrap(),
        b"one detached execution completed"
    );
    assert_eq!(o.report["automatic_retry"], false);
    assert_eq!(
        o.report["remote_execution"],
        "requires_bound_response_or_same_job_reconciliation"
    );
}

#[test]
fn process_intent_cannot_be_reused_or_written_outside_private_root() {
    let f = Fixture::new();
    f.run(&["stdin-closed"], &[], 1024, Duration::from_secs(5));
    let argv = vec![
        env!("CARGO_BIN_EXE_nx-transport-test-fixture").to_owned(),
        "stdin-closed".into(),
    ];
    let request = Exchange {
        argv: &argv,
        input: &[],
        timeout: Duration::from_secs(1),
        output_limit: 1024,
    };
    assert!(exchange(&f.root, Path::new("var/observation"), &request).is_err());
    assert!(exchange(&f.root, Path::new("outside-var"), &request).is_err());
}

#[test]
fn powershell_launch_quotes_unicode_without_interpolation_and_pins_companion() {
    let mut p = profile();
    p.controller_path = "C:\\试验\\'$notExecuted;controller.exe".into();
    let source = p.powershell_bootstrap().unwrap();
    assert!(source.contains("'C:\\试验\\''$notExecuted;controller.exe'"));
    assert!(source.contains(p.controller.sha256.as_str()));
    assert!(source.contains("[IO.FileShare]::Read"));
    assert!(!source.contains("ReadToEnd"));
    let argv = invocation(&p, &p.ssh_command).unwrap();
    let raw = STANDARD.decode(argv.last().unwrap()).unwrap();
    let units = raw
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), source);
    p.remote_root.push('\n');
    assert!(p.validate().is_err());
    assert!(invocation(&profile(), &["sh".into(), "host".into()]).is_err());
}

#[test]
fn finite_frame_roundtrip_and_trailing_text_cannot_masquerade_as_receipt() {
    let mut bytes = Vec::new();
    oe_nx_protocol::write_request(&mut bytes, &oe_nx_protocol::Request::Probe {}, &[]).unwrap();
    let f = Fixture::new();
    let o = f.run(&["framed"], &bytes, 4096, Duration::from_secs(5));
    assert_eq!(
        o.response_frame().unwrap().record["request"]["command"],
        "probe"
    );
    let f = Fixture::new();
    let o = f.run(&["framed-trailing"], &bytes, 4096, Duration::from_secs(5));
    assert!(o.transport_completed());
    assert!(o.response_frame().is_err());
}

#[test]
fn non_probe_requires_pinned_root_before_any_remote_invocation() {
    let f = Fixture::new();
    let mut frame = Vec::new();
    oe_nx_protocol::write_request(
        &mut frame,
        &oe_nx_protocol::Request::Status {
            job_id: oe_nx_protocol::JobId::new("same-original-job").unwrap(),
        },
        &[],
    )
    .unwrap();
    assert!(
        invoke(
            &f.root,
            &profile(),
            &frame,
            Path::new("var/remote"),
            Duration::from_secs(1)
        )
        .is_err()
    );
    assert!(!f.directory.join("var/remote").exists());
}

#[test]
fn fleet_resolution_has_closed_stdin_and_preserves_quoted_argv() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let fleet = f.directory.join("var/fleet");
    fs::write(&fleet, b"#!/bin/sh\nif read item; then exit 18; fi\ncase \"$*\" in *--print-only) ;; *) exit 19;; esac\nprintf '%s\\n' \"ssh -o 'ControlPath=/literal space' fixture-host\"\n").unwrap();
    fs::set_permissions(&fleet, fs::Permissions::from_mode(0o700)).unwrap();
    let mut p = profile();
    p.ssh_command = vec![
        fleet.to_str().unwrap().into(),
        "ssh".into(),
        "fixture-host".into(),
    ];
    let route = resolve_route(&f.root, &p, Path::new("var/resolution")).unwrap();
    assert_eq!(
        route,
        ["ssh", "-o", "ControlPath=/literal space", "fixture-host"]
    );
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(f.directory.join("var/resolution/observation.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["input_bytes_sent"], 0);
    assert_eq!(report["exit_code"], 0);
}

#[test]
fn binary_channel_rejects_tty_stdin_background_and_extra_remote_commands() {
    for option in [
        "-t",
        "-tt",
        "-vvt",
        "-n",
        "-f",
        "-N",
        "-G",
        "-V",
        "-s",
        "-oRequestTTY=force",
        "-oStdinNull=yes",
        "-oRemoteCommand=whoami",
        "-oForkAfterAuthentication=yes",
        "-oSessionType=none",
        "-oBatchMode=no",
    ] {
        assert!(
            binary_ssh_route(&["ssh".into(), option.into(), "host".into()]).is_err(),
            "{option}"
        );
    }
    assert!(binary_ssh_route(&["ssh".into(), "host".into(), "whoami".into()]).is_err());
    let route = vec![
        "ssh".into(),
        "-vvT".into(),
        "-o".into(),
        "ControlPath=/literal -t space".into(),
        "-i".into(),
        "/path with spaces/key".into(),
        "user@host".into(),
    ];
    let safe = binary_ssh_route(&route).unwrap();
    assert_eq!(&safe[1..4], ["-T", "-o", "RequestTTY=no"]);
    assert_eq!(&safe[safe.len() - route.len() + 1..], &route[1..]);
}

#[test]
fn actual_openssh_config_cannot_force_tty_or_drop_binary_stdin() {
    let f = Fixture::new();
    let config = f.directory.join("var/ssh_config");
    fs::write(&config, "Host *\n RequestTTY force\n StdinNull yes\n ForkAfterAuthentication yes\n RemoteCommand ignored-command\n BatchMode no\n").unwrap();
    let route = vec![
        "/usr/bin/ssh".into(),
        "-F".into(),
        config.to_str().unwrap().into(),
        "example.invalid".into(),
    ];
    let mut argv = binary_ssh_route(&route).unwrap();
    argv.insert(1, "-G".into()); // read configuration only; no network connection
    let o = exchange(
        &f.root,
        Path::new("var/ssh-config-observation"),
        &Exchange {
            argv: &argv,
            input: &[],
            timeout: Duration::from_secs(5),
            output_limit: 64 * 1024,
        },
    )
    .unwrap();
    assert!(o.transport_completed(), "{}", o.report);
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(
        text.lines()
            .any(|s| s == "requesttty false" || s == "requesttty no")
    );
    assert!(text.lines().any(|s| s == "stdinnull no"));
    assert!(text.lines().any(|s| s == "forkafterauthentication no"));
    assert!(text.lines().any(|s| s == "batchmode yes"));
    assert!(!text.contains("ignored-command"));
}

#[test]
fn complete_zero_exit_remote_error_retains_bytes_without_a_successful_status() {
    let f = Fixture::new();
    let ssh = f.directory.join("var/ssh");
    fs::copy(env!("CARGO_BIN_EXE_nx-transport-test-fixture"), &ssh).unwrap();
    let mut p = profile();
    p.ssh_command = vec![ssh.to_str().unwrap().into(), "fixture-error-host".into()];
    p.expected_root = Some(oe_nx_protocol::RootIdentity {
        host_id: "fixture-host".into(),
        volume_id: "fixture-volume".into(),
        directory_id: "fixture-root".into(),
    });
    let client = oe_cad::nx::client::Client {
        root: &f.root,
        profile: &p,
    };
    let error = client
        .status(
            &oe_nx_protocol::JobId::new("same-job").unwrap(),
            Path::new("var/status-call"),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("synthetic remote observation unavailable")
    );
    let raw = fs::read(f.directory.join("var/status-call/request/stdout.bin")).unwrap();
    let frame = oe_nx_protocol::read_frame(&mut std::io::Cursor::new(raw)).unwrap();
    assert_eq!(frame.record["automatic_retry"], false);
    let observation: serde_json::Value = serde_json::from_slice(
        &fs::read(f.directory.join("var/status-call/request/observation.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(observation["exit_code"], 0);
    assert!(
        !f.directory
            .join("var/status-call/status-observation.json")
            .exists()
    );
}

fn run_job() -> oe_nx_protocol::JobManifest {
    use oe_nx_protocol::*;
    JobManifest {
        capacity: None,
        auxiliary_capacity: None,
        schema: JOB_SCHEMA.into(),
        job_id: JobId::new("single-run").unwrap(),
        root: RootIdentity {
            host_id: "fixture-host".into(),
            volume_id: "fixture-volume".into(),
            directory_id: "fixture-root".into(),
        },
        worker: WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: "fixture-never-launched".into(),
            version: "test".into(),
            target: "x86_64-pc-windows-gnu".into(),
            executable: FileEntry::from_bytes("worker/native.exe", b"fixture").unwrap(),
            runtime_files: vec![],
            source_files: vec![FileEntry::from_bytes("sources/main.rs", b"fixture").unwrap()],
            build_recipe: FileEntry::from_bytes("build/recipe.txt", b"fixture").unwrap(),
            toolchain: "fixture-no-compiler".into(),
            required_runtime: RuntimeIdentity {
                product: "NX".into(),
                product_label: "fixture-not-loaded".into(),
                architecture: "x64".into(),
                libraries: vec![FileEntry::from_bytes("NXBIN/libufun.dll", b"fixture").unwrap()],
            },
            capabilities: vec![Capability {
                action: "inspect".into(),
                params_schema: "fixture.inspect/v1".into(),
            }],
        },
        action: "inspect".into(),
        params_schema: "fixture.inspect/v1".into(),
        params: FileEntry::from_bytes("params.json", br#"{"schema":"fixture.inspect/v1"}"#)
            .unwrap(),
        inputs: vec![],
    }
}

fn run_profile(f: &Fixture, mode: &str) -> RemoteProfile {
    let ssh = f.directory.join("var/ssh");
    fs::copy(env!("CARGO_BIN_EXE_nx-transport-test-fixture"), &ssh).unwrap();
    let mut p = profile();
    p.ssh_command = vec![ssh.to_str().unwrap().into(), mode.into()];
    p.expected_root = Some(run_job().root);
    p
}

#[test]
fn run_transport_parse_binding_and_persist_failures_are_typed_uncertain_without_resend() {
    for mode in [
        "fixture-run-exit",
        "fixture-run-malformed",
        "fixture-run-error",
        "fixture-run-binding",
        "fixture-run-persist",
    ] {
        let f = Fixture::new();
        let p = run_profile(&f, mode);
        let client = Client {
            root: &f.root,
            profile: &p,
        };
        let job = run_job();
        let error = client.run(&job, 1, Path::new("var/run")).unwrap_err();
        let uncertain = error.downcast_ref::<DispatchUncertain>().unwrap();
        assert_eq!(uncertain.job_id, job.job_id);
        assert_eq!(uncertain.manifest_digest, job.digest().unwrap());
        assert_eq!(uncertain.evidence, f.directory.join("var/run"));
        assert_eq!(uncertain.reason, "run_response_unconfirmed");
        assert!(!error.to_string().contains("private synthetic peer detail"));
        if mode == "fixture-run-error" {
            assert!(format!("{error:#}").contains("private synthetic peer detail"));
        }
        assert_eq!(
            fs::read(f.directory.join("var/request-count")).unwrap(),
            b"1"
        );
        for name in [
            "intent.json",
            "request.bin",
            "stdout.bin",
            "stderr.bin",
            "observation.json",
        ] {
            assert!(f.directory.join("var/run/request").join(name).is_file());
        }
        assert!(!f.directory.join("var/run/run-observation.json").is_file());
    }
}

#[test]
fn valid_unknown_remains_a_remote_observation_instead_of_a_local_uncertainty_error() {
    let f = Fixture::new();
    let p = run_profile(&f, "fixture-run-unknown");
    let client = Client {
        root: &f.root,
        profile: &p,
    };
    let result = client.run(&run_job(), 1, Path::new("var/run")).unwrap();
    assert!(matches!(
        result,
        oe_nx_protocol::StatusObservation::Unknown(_)
    ));
    assert!(f.directory.join("var/run/run-observation.json").is_file());
    assert_eq!(
        fs::read(f.directory.join("var/request-count")).unwrap(),
        b"1"
    );
}

#[test]
fn local_rejections_and_failed_spawn_are_not_reported_as_possible_remote_dispatch() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [
        "wrong-root",
        "outside-output",
        "invalid-wait",
        "used-output",
        "spawn-failure",
        "fleet-failure",
    ] {
        let f = Fixture::new();
        let mut p = run_profile(&f, "fixture-run-unknown");
        let mut job = run_job();
        let mut wait = 1;
        let mut output = PathBuf::from("var/run");
        match mode {
            "wrong-root" => job.root.directory_id = "other-root".into(),
            "outside-output" => output = PathBuf::from("outside-run"),
            "invalid-wait" => wait = 0,
            "used-output" => fs::create_dir(f.directory.join("var/run")).unwrap(),
            "spawn-failure" => {
                fs::write(&p.ssh_command[0], b"#!/nonexistent/nx-test-interpreter\n").unwrap();
                fs::set_permissions(&p.ssh_command[0], fs::Permissions::from_mode(0o700)).unwrap();
            }
            "fleet-failure" => {
                let fleet = f.directory.join("var/fleet");
                fs::write(&fleet, b"#!/bin/sh\nexit 19\n").unwrap();
                fs::set_permissions(&fleet, fs::Permissions::from_mode(0o700)).unwrap();
                p.ssh_command = vec![
                    fleet.to_str().unwrap().into(),
                    "ssh".into(),
                    "synthetic-host".into(),
                ];
            }
            _ => unreachable!(),
        }
        let client = Client {
            root: &f.root,
            profile: &p,
        };
        let error = client.run(&job, wait, &output).unwrap_err();
        assert!(
            error.downcast_ref::<DispatchUncertain>().is_none(),
            "{mode}: {error:#}"
        );
        assert!(!f.directory.join("var/request-count").exists());
    }
}
