//! Read-only, pinned collection for an explicit writer review. Never releases
//! a hold, rewrites a job, dispatches a process, or loads a CAD library.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use oe_nx_protocol as p;
    use oe_windows_controller::{adapter, dispatch_review::DispatchSnapshotGuard, windows::StableRoot};
    use std::{fs::OpenOptions, io::Write, path::Path};
    let args:Vec<_>=std::env::args().collect();
    ensure!(args.len()==3,"usage: dispatch-review-observe explicit-input.json new-output.json");
    let raw=std::fs::read(&args[1])?;
    let input:serde_json::Value=p::decode(&raw)?;
    ensure!(input["schema"]=="oev2.dispatch-review-observation-request/1","observation_schema");
    let root=StableRoot::open(Path::new(input["root_path"].as_str().context("root_path")?),None)?;
    let expected:p::RootIdentity=serde_json::from_value(input["root"].clone())?;
    ensure!(adapter::root_identity(&root)==expected,"observation_root_identity_differs");
    let id:p::JobId=serde_json::from_value(input["job_id"].clone())?;
    let worker:p::ProcessIdentity=serde_json::from_value(input["worker"].clone())?;
    let supervisor:p::ProcessIdentity=serde_json::from_value(input["supervisor"].clone())?;
    let lock=root.lock()?;
    let observed=DispatchSnapshotGuard::collect_for_review(&lock,&id,&worker,&supervisor)?;
    observed.verify(&root,&id)?;
    let result=serde_json::json!({"schema":"oev2.dispatch-review-observed-facts/1","request_identity":p::BlobIdentity::of(&raw),"snapshot":observed.facts.snapshot,"records":observed.facts.records,"worker":observed.facts.worker,"supervisor":observed.facts.supervisor,"writer_release":"not_granted","native_execution":"not_started_by_observer"});
    let bytes=serde_json::to_vec_pretty(&result)?;
    ensure!(bytes.len()<=p::MAX_RECORD_BYTES,"observation_record_budget");
    let mut output=OpenOptions::new().write(true).create_new(true).open(&args[2])?;
    output.write_all(&bytes)?;output.sync_all()?;
    observed.verify(&root,&id)?;
    println!("{}",serde_json::json!({"output":args[2],"identity":p::BlobIdentity::of(&bytes),"writer_release":"not_granted"}));
    Ok(())
}
#[cfg(not(windows))]
fn main() { eprintln!("Windows OS observation required");std::process::exit(2); }
