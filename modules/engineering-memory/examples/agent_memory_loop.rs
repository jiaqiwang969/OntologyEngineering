//! Public fixture orchestration through installed CLIs; no fake tool execution.
use anyhow::{Context, Result, ensure};
use clap::Parser;
use engineering_memory_runtime::evidence::{EvidenceArgs, read_window};
use engineering_memory_runtime::store::sha;
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
struct Args {
    /// Real installed repository root, containing var/install/INSTALLATION.json.
    #[arg(long)]
    root: PathBuf,
    /// A new relative directory under var/examples; existing output is refused.
    #[arg(long, default_value = "var/examples/agent-memory-loop")]
    output: PathBuf,
    /// Write source-bound requests only; do not execute an installed program.
    #[arg(long)]
    prepare_only: bool,
}

fn save(path: &Path, value: &Value) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn binding(path: &Path) -> Result<Value> {
    let raw = fs::read(path)?;
    let project: Value = serde_json::from_slice(&raw)?;
    Ok(
        json!({"path":path,"project_id":project["project_id"],"revision":project["revision"],
        "sha256":sha(&raw),"object":{"object_id":"path","object_revision":"v1",
        "condition_scope":"inspection","condition_sha256":sha(project["conditions"]["inspection"].as_str().context("condition")?)}}),
    )
}

fn session(path: &Path, id: &str, session_id: &str) -> Result<Value> {
    Ok(
        json!({"schema":"engineering-memory.session-request/1","id":id,"provider":"codex",
        "session_id":session_id,"task":"在公开合成案例中保留离散观察与连续结论的边界",
        "phase":"decision","project":binding(path)?,"conditions":["公开合成案例","discrete-sampling"],
        "query":"连续路径","include_other_projects":false,"max_candidates":5,"max_context_bytes":16384}),
    )
}

struct Runner {
    root: PathBuf,
    out: PathBuf,
    program: PathBuf,
    home: String,
    sequence: usize,
}

impl Runner {
    fn call(&mut self, name: &str, args: Vec<String>) -> Result<Value> {
        self.sequence += 1;
        let stem = format!("{:02}-{name}", self.sequence);
        let output = Command::new(&self.program)
            .arg("--root")
            .arg(&self.root)
            .args(&args)
            .current_dir(&self.root)
            .output()?;
        fs::write(self.out.join(format!("{stem}.stdout.json")), &output.stdout)?;
        fs::write(self.out.join(format!("{stem}.stderr.txt")), &output.stderr)?;
        save(
            &self.out.join(format!("{stem}.execution.json")),
            &json!({
            "program":self.program,"program_sha256":sha(fs::read(&self.program)?),
            "args":args,"root":self.root,"exit_code":output.status.code(),
            "scope":"Actual CLI execution against public synthetic sources; no external specialist invocation"}),
        )?;
        ensure!(
            output.status.success(),
            "{stem} failed; inspect retained stdout/stderr"
        );
        let value: Value = serde_json::from_slice(&output.stdout).context(stem)?;
        if value.get("ok").is_some() {
            ensure!(value["ok"] == true, "memory command rejected");
            Ok(value["data"].clone())
        } else {
            Ok(value)
        }
    }
    fn memory(&mut self, name: &str, args: &[&str]) -> Result<Value> {
        let mut all = vec!["memory".into(), "--home".into(), self.home.clone()];
        all.extend(args.iter().map(|s| s.to_string()));
        self.call(name, all)
    }
    fn project(&mut self, name: &str, args: &[&str]) -> Result<Value> {
        let mut all = vec!["project".into()];
        all.extend(args.iter().map(|s| s.to_string()));
        self.call(name, all)
    }
}

fn path_string(path: &Path) -> Result<String> {
    Ok(path.to_str().context("path must be UTF-8")?.into())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.root.canonicalize()?;
    ensure!(
        args.output.starts_with("var/examples")
            && args
                .output
                .components()
                .all(|p| matches!(p, Component::Normal(_))),
        "output must be a normal relative path under var/examples"
    );
    let out = root.join(&args.output);
    ensure!(
        !out.exists(),
        "example output already exists; choose a new directory"
    );
    fs::create_dir_all(&out)?;
    let source = out.join("source.claude.jsonl");
    fs::write(&source, include_str!("source.claude.jsonl"))?;
    fs::write(out.join("AGENT-REVIEW.md"), include_str!("AGENT-REVIEW.md"))?;
    let window = read_window(&EvidenceArgs {
        source: source.clone(),
        provider: "claude".into(),
        profile: Some("public-example".into()),
        start_byte: 0,
        end_byte: None,
        max_record_bytes: 2 * 1024 * 1024,
        json: true,
    })?;
    ensure!(
        window.records.len() == 3 && window.complete_visible_window,
        "fixture source mismatch"
    );
    let sources = serde_json::to_value(&window.records)?;
    let record = json!({"schema":"engineering-memory.experience/2","id":"sampling-boundary","revision":1,
        "supersedes":null,"project_id":"public-memory-example","conditions":["公开合成案例","discrete-sampling"],
        "purpose":"保留有限离散观察不能直接扩大为连续路径通过的讨论与纠正",
        "status":"reviewed","recorded_at":"2026-01-01T00:03:00Z","sources":sources,
        "timeline":[
          {"id":"purpose","kind":"user_report","occurred_at":"2026-01-01T00:00:00Z","description":window.records[0].text,"source_sha256":[window.records[0].sha256],"corrects":[]},
          {"id":"claim","kind":"assistant_claim","occurred_at":"2026-01-01T00:01:00Z","description":window.records[1].text,"source_sha256":[window.records[1].sha256],"corrects":[]},
          {"id":"correction","kind":"correction","occurred_at":"2026-01-01T00:02:00Z","description":window.records[2].text,"source_sha256":[window.records[2].sha256],"corrects":["claim"]}],
        "causal_claims":[],"method":{"purpose":"限制结论范围","mechanism":"把实际证据覆盖与待核范围分别记录","validation":"真实任务另取覆盖所需范围的证据"},
        "unknowns":["合成案例没有真实采样或工具运行","没有连续路径验证","没有工程采用或收益证明"]});
    save(&out.join("experience.json"), &record)?;
    let project = json!({"schema":"oev2.project/1","project_id":"public-memory-example","revision":"r1",
        "purpose":"公开合成安装案例；核验候选回交，不进行设备验收",
        "objects":{"path":"v1"},"conditions":{"inspection":"公开合成案例；离散采样与连续结论分别保留"},
        "evidence":{"source-fixture":sha(fs::read(&source)?)},"decisions":[],"dependencies":{}});
    let project1 = out.join("project-r1.json");
    save(&project1, &project)?;
    save(
        &out.join("session-first.json"),
        &session(&project1, "first-context", "public-session-first")?,
    )?;
    save(&out.join("source-anchors.json"), &sources)?;
    if args.prepare_only {
        println!(
            "{}",
            json!({"status":"prepared_only","output":out,"commands_executed":0,"next":"Review AGENT-REVIEW.md and the source-bound inputs; rerun without --prepare-only into another new output directory."})
        );
        return Ok(());
    }
    let installation: Value =
        serde_json::from_slice(&fs::read(root.join("var/install/INSTALLATION.json"))?)?;
    ensure!(
        installation["schema"] == "oev2.installation/1"
            && installation["root"].as_str() == root.to_str(),
        "installation identity mismatch"
    );
    let programs = installation["programs"]
        .as_array()
        .context("installation programs")?;
    let selected: Vec<_> = programs.iter().filter(|p| p["name"] == "oev2").collect();
    ensure!(selected.len() == 1, "oev2 program selection must be unique");
    let program = PathBuf::from(selected[0]["path"].as_str().context("installed program")?);
    ensure!(
        sha(fs::read(&program)?) == selected[0]["sha256"],
        "installed program changed"
    );
    let mut run = Runner {
        root,
        out: out.clone(),
        program,
        home: path_string(&out.join("memory"))?,
        sequence: 0,
    };
    let p1 = path_string(&project1)?;
    let p2 = path_string(&out.join("project-r2.json"))?;
    let experience = path_string(&out.join("experience.json"))?;
    let first = path_string(&out.join("session-first.json"))?;
    run.project("project-validate", &["validate", "--input", &p1])?;
    let actual = run.memory(
        "evidence",
        &[
            "evidence",
            "--provider",
            "claude",
            "--profile",
            "public-example",
            "--source",
            &path_string(&source)?,
        ],
    )?;
    ensure!(
        actual["records"] == sources,
        "installed reader returned different anchors"
    );
    run.memory("record", &["record", "--input", &experience])?;
    let context = run.memory("prepare-first", &["session", "prepare", "--input", &first])?;
    let digest = context["context_sha256"]
        .as_str()
        .context("context digest")?;
    run.memory(
        "activate-first",
        &[
            "session",
            "activate",
            "--id",
            "first-context",
            "--context-sha256",
            digest,
        ],
    )?;
    let handoff = run.memory(
        "handoff",
        &[
            "session",
            "handoff",
            "--context-id",
            "first-context",
            "--context-sha256",
            digest,
            "--id",
            "sampling-boundary",
            "--revision",
            "1",
        ],
    )?;
    let handoff_path = out.join("handoff.json");
    save(&handoff_path, &handoff)?;
    let hp = path_string(&handoff_path)?;
    let verified = run.project(
        "candidate-verify",
        &[
            "memory-candidate",
            "verify",
            "--project",
            &p1,
            "--input",
            &hp,
        ],
    )?;
    ensure!(verified["status"] == "accepted", "candidate not verified");
    let registered = run.project(
        "candidate-register",
        &[
            "memory-candidate",
            "register",
            "--project",
            &p1,
            "--input",
            &hp,
            "--revision",
            "r2",
            "--output",
            &p2,
        ],
    )?;
    ensure!(
        registered["status"] == "registered",
        "candidate not registered"
    );
    let next: Value = serde_json::from_slice(&fs::read(&p2)?)?;
    ensure!(
        next["decisions"] == json!([]),
        "candidate registration created a decision"
    );
    let feedback = path_string(&out.join("feedback.json"))?;
    run.project(
        "feedback-prepare",
        &[
            "memory-feedback",
            "--project",
            &p2,
            "--input",
            &hp,
            "--id",
            "sampling-feedback",
            "--outcome",
            "no_delta",
            "--result",
            "候选已用于保留证据范围并登记到同一Project；没有对照证明收益。",
            "--unknown",
            "没有工程收益对照",
            "--output",
            &feedback,
        ],
    )?;
    run.memory(
        "feedback-record",
        &["session", "feedback", "--input", &feedback],
    )?;
    let second = out.join("session-second.json");
    save(
        &second,
        &session(Path::new(&p2), "second-context", "public-session-second")?,
    )?;
    let recalled = run.memory(
        "prepare-second",
        &["session", "prepare", "--input", &path_string(&second)?],
    )?;
    let candidates = recalled["session"]["memory_context"]["candidates"]
        .as_array()
        .context("recalled candidates")?;
    let lesson = candidates
        .iter()
        .find(|c| c["id"] == "sampling-boundary")
        .context("experience not recalled")?;
    ensure!(
        lesson["timeline"] == record["timeline"] && lesson["unknowns"] == record["unknowns"],
        "recall lost correction or uncertainty"
    );
    let digest = recalled["context_sha256"]
        .as_str()
        .context("second context digest")?;
    run.memory(
        "activate-second",
        &[
            "session",
            "activate",
            "--id",
            "second-context",
            "--context-sha256",
            digest,
        ],
    )?;
    run.memory(
        "verify-second",
        &["session", "verify", "--id", "second-context"],
    )?;
    let second_handoff = run.memory(
        "handoff-second",
        &[
            "session",
            "handoff",
            "--context-id",
            "second-context",
            "--context-sha256",
            digest,
            "--id",
            "sampling-boundary",
            "--revision",
            "1",
        ],
    )?;
    ensure!(
        second_handoff["candidate"]["record"] == record,
        "new session handoff lost original messages, conditions or correction history"
    );
    ensure!(
        second_handoff["engineering_adoption"] == "not_granted"
            && second_handoff["rrsi_validation"] == "not_run"
            && second_handoff["project_write"] == false,
        "handoff crossed the candidate-only boundary"
    );
    ensure!(
        second_handoff["candidate"]["feedback"]
            .as_array()
            .is_some_and(|a| a.iter().any(|f| f["id"] == "sampling-feedback")),
        "actual-use feedback missing"
    );
    let result = json!({"schema":"oev2.public-memory-example/1","status":"matched","project":p2,
        "source_sha256":sha(fs::read(&source)?),"cli_calls":run.sequence,"new_session_recalled":true,
        "complete_record_preserved":true,"correction_and_unknowns_preserved":true,"feedback_retained":true,"external_tools_executed":false,
        "engineering_adoption":"not_granted","rrsi_gain":"not_measured","scope":"Public synthetic installation and source-bound workflow check"});
    save(&out.join("RESULT.json"), &result)?;
    println!("{result}");
    Ok(())
}
