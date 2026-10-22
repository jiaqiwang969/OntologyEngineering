use anyhow::Result;
use oe_core::SkillRoot;
use oe_geometry::acceptance::verify_job_result;
use oe_geometry::protocol::{GeometryJob, JobResult, JobStatus, dispatch_in};
use std::path::{Path, PathBuf};

const ADAPTER: &str = r#"
import argparse,json,pathlib,sys,time
p=argparse.ArgumentParser()
p.add_argument('--step');p.add_argument('--out');p.add_argument('--mode',default='good')
p.add_argument('--original');a=p.parse_args();out=pathlib.Path(a.out)
doc={'schema':'ontology-engineering.geometry-evidence/v1','parts':{'part':{'cylinders':[],'planes':[],'cones':[]}},'missing_definitions':[],'extra_definitions':[]}
if a.mode=='timeout':
 print('partial',flush=True);time.sleep(30)
if a.mode=='limit':
 sys.stdout.buffer.write(b'x'*(33*1024*1024));sys.stdout.flush();time.sleep(30)
if a.mode=='node-limit':
 for i in range(4100):(out/str(i)).mkdir()
if a.mode!='no-artifact':(out/'geometry.json').write_text(json.dumps(doc))
if a.mode=='input-change':pathlib.Path(a.original).write_text('changed')
if a.mode=='script-change':pathlib.Path(__file__).write_text('changed')
if a.mode=='unicode':print('测'*1000,file=sys.stderr)
if a.mode=='nonzero':print('failed',file=sys.stderr);sys.exit(7)
if a.mode=='bad-json':print('not json')
elif a.mode=='duplicate-json':print('{"a":1,"a":2}')
elif a.mode=='empty':pass
else:print(json.dumps({'source':pathlib.Path(a.step).read_text()}))
"#;

fn fixture() -> Result<(tempfile::TempDir, SkillRoot, GeometryJob)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().canonicalize()?;
    std::fs::write(path.join("SKILL.md"), "fixture")?;
    std::fs::write(path.join("adapter.py"), ADAPTER)?;
    std::fs::create_dir(path.join("var"))?;
    let input = path.join("var/input.step");
    std::fs::write(&input, "original")?;
    let root = SkillRoot::new(&path)?;
    let job = GeometryJob::new(
        PathBuf::from("adapter.py"),
        input,
        PathBuf::from("var/output"),
    )?;
    Ok((dir, root, job))
}
fn run(root: &SkillRoot, job: &GeometryJob) -> Result<JobResult> {
    let python = oe_core::process::resolve_executable(Path::new("python3"))?;
    dispatch_in(root, &python, job)
}
#[test]
fn empty_output_directories_cannot_bypass_resource_limits() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    let result = run(&root, &job.with_arg("--mode", "node-limit"))?;
    assert_eq!(result.status, JobStatus::InvalidArtifacts);
    assert!(verify_job_result(&result).is_err());
    Ok(())
}
#[test]
fn actual_dispatch_and_bound_readback_roundtrip() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    let result = run(&root, &job)?;
    assert_eq!(result.status, JobStatus::Completed);
    assert_eq!(result.output["source"], "original");
    let readback = verify_job_result(&result)?;
    assert!(readback.passed(), "{:?}", readback.errors);
    let evidence =
        oe_geometry::evidence::load_verified_evidence(&result, Path::new("geometry.json"))?;
    assert_eq!(
        evidence.to_json()["source_binding"],
        "exact_dispatch_readback"
    );
    assert_eq!(
        readback.to_json()["engineering_acceptance"],
        "not_determined"
    );
    Ok(())
}
#[test]
fn caller_mutations_of_the_result_projection_cannot_inherit_acceptance() -> Result<()> {
    for field in ["output", "elapsed"] {
        let (_dir, root, job) = fixture()?;
        let mut result = run(&root, &job)?;
        if field == "output" {
            result.output["source"] = "forged".into();
        } else {
            result.elapsed_seconds += 1.0;
        }
        assert!(verify_job_result(&result).is_err(), "{field}");
    }
    Ok(())
}
#[test]
fn altered_added_or_deleted_output_is_rejected() -> Result<()> {
    for alteration in ["change", "add", "delete"] {
        let (_dir, root, job) = fixture()?;
        let result = run(&root, &job)?;
        let output = root.path().join("var/output");
        match alteration {
            "change" => std::fs::write(output.join("geometry.json"), b"{\"changed\":true}")?,
            "add" => std::fs::write(output.join("extra.json"), b"{}")?,
            _ => std::fs::remove_file(output.join("geometry.json"))?,
        }
        assert!(!verify_job_result(&result)?.passed(), "{alteration}");
        assert!(
            oe_geometry::evidence::load_verified_evidence(&result, Path::new("geometry.json"))
                .is_err()
        );
    }
    Ok(())
}
#[test]
fn changed_receipt_stdout_or_input_snapshot_is_rejected() -> Result<()> {
    for file in [
        "record",
        "_oe/process/stdout",
        "_oe/process/stderr",
        "_oe/process/process.json",
        "_oe/input/input.step",
    ] {
        let (_dir, root, job) = fixture()?;
        let result = run(&root, &job)?;
        let output = root.path().join("var/output");
        let path = if file == "record" {
            output.join(format!("{}.job.json", result.job_id))
        } else {
            output.join(file)
        };
        std::fs::write(path, "{}")?;
        assert!(verify_job_result(&result).is_err(), "{file}");
    }
    Ok(())
}
#[test]
fn protocol_or_process_failure_never_completes() -> Result<()> {
    for (mode, status) in [
        ("bad-json", JobStatus::ProtocolError),
        ("duplicate-json", JobStatus::ProtocolError),
        ("empty", JobStatus::ProtocolError),
        ("nonzero", JobStatus::ProcessFailed),
        ("no-artifact", JobStatus::InvalidArtifacts),
    ] {
        let (_dir, root, job) = fixture()?;
        let job = job.with_arg("--mode", mode);
        let result = run(&root, &job)?;
        assert_eq!(result.status, status, "{mode}");
        assert!(!verify_job_result(&result)?.passed());
        assert!(
            root.path()
                .join("var/output/_oe/process/process.json")
                .is_file()
        );
    }
    Ok(())
}
#[test]
fn timeout_is_enforced_and_partial_evidence_is_retained() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    let result = run(&root, &job.with_arg("--mode", "timeout").with_timeout(1))?;
    assert_eq!(result.status, JobStatus::TimedOut);
    let process: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.path().join("var/output/_oe/process/process.json"),
    )?)?;
    assert_eq!(process["status"], "timed_out");
    assert!(
        std::fs::read(root.path().join("var/output/_oe/process/stdout"))?.starts_with(b"partial")
    );
    assert!(!verify_job_result(&result)?.passed());
    Ok(())
}
#[test]
fn streaming_output_limit_is_a_failure_with_receipt() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    let result = run(&root, &job.with_arg("--mode", "limit").with_timeout(10))?;
    assert_eq!(result.status, JobStatus::OutputLimit);
    assert!(!verify_job_result(&result)?.passed());
    Ok(())
}
#[test]
fn unicode_stderr_does_not_panic_at_excerpt_boundary() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    let result = run(&root, &job.with_arg("--mode", "unicode"))?;
    assert!(verify_job_result(&result)?.passed());
    Ok(())
}
#[test]
fn changed_sources_cannot_reuse_the_old_identity() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    std::fs::write(root.path().join("var/input.step"), "changed before")?;
    assert!(run(&root, &job).is_err());
    assert!(!root.path().join("var/output").exists());
    for (mode, status) in [
        ("input-change", JobStatus::InputChanged),
        ("script-change", JobStatus::ScriptChanged),
    ] {
        let (_dir, root, job) = fixture()?;
        let original = root
            .path()
            .join("var/input.step")
            .to_string_lossy()
            .into_owned();
        let result = run(
            &root,
            &job.with_arg("--mode", mode)
                .with_arg("--original", original),
        )?;
        assert_eq!(result.status, status);
        assert!(!verify_job_result(&result)?.passed());
    }
    Ok(())
}
#[test]
fn existing_output_and_reserved_input_flags_are_rejected() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    std::fs::create_dir(root.path().join("var/output"))?;
    assert!(run(&root, &job).is_err());
    let (_dir, root, job) = fixture()?;
    assert!(run(&root, &job.with_arg("--step", "other.step")).is_err());
    assert!(!root.path().join("var/output").exists());
    assert!(
        GeometryJob::new(
            PathBuf::from("../adapter.py"),
            root.path().join("var/input.step"),
            PathBuf::from("var/unused")
        )
        .is_err()
    );
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlinked_output_is_rejected() -> Result<()> {
    let (_dir, root, job) = fixture()?;
    let result = run(&root, &job)?;
    let output = root.path().join("var/output/geometry.json");
    let alternate = root.path().join("var/alternate.json");
    std::fs::rename(&output, &alternate)?;
    std::os::unix::fs::symlink(alternate, output)?;
    assert!(verify_job_result(&result).is_err());
    Ok(())
}
