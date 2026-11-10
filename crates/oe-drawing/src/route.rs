//! The drawing route: one DWG (or a DXF with its rendering) to a checked review page, with one receipt.
//!
//! DWG -> pinned CADCraft: 2018 DXF (converted without fonts), neutral SVG and preview -> current Project
//! context -> discovery request -> isolated worker that receives only the DXF bytes -> independent check of the
//! graph from the DXF bytes -> review page -> offline browser checks in every mode -> receipt. CADCraft converts
//! and renders; the worker makes every engineering interpretation; nothing else classifies. No reference,
//! answer, region, feedback, model or screenshot can be passed in. The first failing step stops the route and
//! every later step is recorded as not run. Passing says the route ran as pinned and its outputs keep the source
//! and the contracts; it is not an engineering acceptance of the classification.
use crate::{engineering_expression, page, review, usage_discovery};
#[path = "route_correction.rs"]
mod corrections;
use anyhow::{Context, Result, anyhow, ensure};
use oe_contracts::*;
use oe_core::process::{
    CommandSpec, ProcessObservation, run_command, run_command_with_stdin_file_deadline,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const REQUEST_SCHEMA: &str = "oev2.drawing-route-request/1";
pub const RECEIPT_SCHEMA: &str = "oev2.drawing-route-receipt/1";
pub const RUNTIME_SCHEMA: &str = "oev2.cadcraft-runtime/1";
pub const LOCK_SCHEMA: &str = "oev2.cadcraft-source-lock/1";
pub const STEPS: [&str; 17] = [
    "verify-runtime",
    "read-source",
    "probe-sandbox",
    "convert",
    "render",
    "context",
    "request",
    "classify",
    "check-graph",
    "write-corrected",
    "read-corrected",
    "render-corrected",
    "check-corrected",
    "page",
    "check-page",
    "check-rules",
    "verify-source",
];
pub const MODES: [&str; 7] = [
    "kind", "line", "view", "geometry", "focus", "complete", "asdrawn",
];
/// Source lock of the only converter the route accepts, compiled into the executable.
pub const CADCRAFT_LOCK: &str =
    include_str!("../../../integrations/runtime-locks/cadcraft-source.json");
/// Two LINE records: enough for the worker to return a graph; no real drawing content.
const PROBE_DXF: &[u8] = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n1\n8\n0\n330\nM\n10\n0\n20\n0\n11\n10\n21\n0\n0\nLINE\n5\n2\n8\n0\n330\nM\n10\n10\n20\n0\n11\n10\n21\n5\n0\nENDSEC\n0\nEOF\n";
const SCOPE: &str = "Route execution, source conservation and output-contract checks only; no engineering adoption, no evidence about other drawings.";
const PAGE_STATUS: &str = "本页由固定路线生成并经离线浏览器检查；分类未经人工评价，不是工程验收。";
const LIMIT: usize = 512 * 1024 * 1024;

pub struct DrawingRoute<'a> {
    pub root: &'a SkillRoot,
}
impl MdDrawingRoutePort for DrawingRoute<'_> {
    fn run_route(
        &self,
        input: &MdDrawingRouteRequest,
    ) -> std::result::Result<MdDrawingRouteReceipt, FailureRecord> {
        route(self.root, input).map_err(|e| oe_engineering::artifacts::failure("drawing_route", e))
    }
}

/// The compiled CADCraft source lock.
pub fn lock() -> Result<Value> {
    let lock = oe_core::json::strict_json(CADCRAFT_LOCK.as_bytes())?;
    ensure!(
        lock["schema"] == LOCK_SCHEMA
            && ["repository", "commit", "version"]
                .iter()
                .all(|k| lock[*k].as_str().is_some_and(|v| !v.is_empty())),
        "cadcraft_lock_contract"
    );
    Ok(lock)
}

/// The converter record checked against the compiled lock and the program bytes, before it may run.
pub fn verify_cadcraft(record: &MdCadcraftRuntime, lock: &Value) -> Result<PathBuf> {
    ensure!(
        record.validate().is_ok() && record.schema == RUNTIME_SCHEMA,
        "cadcraft_runtime_contract"
    );
    ensure!(
        lock["repository"] == record.repository.as_str()
            && lock["commit"] == record.source_commit.as_str(),
        "cadcraft_commit_not_locked:{}@{}",
        record.repository,
        record.source_commit
    );
    let program = PathBuf::from(&record.program_path);
    ensure!(program.is_absolute(), "cadcraft_program_path_not_absolute");
    let actual =
        sha256(read_regular_bytes(&program, LIMIT).context("cadcraft_program_unreadable")?);
    ensure!(
        actual == record.program_sha256,
        "cadcraft_program_digest_mismatch:{actual}"
    );
    match (&record.fonts_manifest_path, &record.fonts_manifest_sha256) {
        (Some(path), Some(digest)) => ensure!(
            sha256(read_regular_bytes(Path::new(path), 16 * 1024 * 1024)?) == *digest,
            "cadcraft_font_manifest_changed"
        ),
        (None, None) => {}
        _ => anyhow::bail!("cadcraft_font_manifest_incomplete"),
    }
    Ok(program)
}

fn utc_now() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:06}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_micros()
    )
}

/// ASCII whitespace as a byte-level DXF reader trims it.
fn trim(b: &[u8]) -> &[u8] {
    let ws = |c: &u8| b" \t\n\r\x0b\x0c".contains(c);
    let start = b.iter().position(|c| !ws(c)).unwrap_or(b.len());
    let end = b.iter().rposition(|c| !ws(c)).map_or(start, |i| i + 1);
    &b[start..end.max(start)]
}

/// One group code and its value.
type Pair<'a> = (&'a [u8], &'a [u8]);

/// Model-space entities of a DXF, counted from its group pairs.
fn entity_count(dxf: &[u8]) -> usize {
    let lines: Vec<&[u8]> = dxf.split(|b| *b == b'\n').collect();
    let mut section: Option<&[u8]> = None;
    let mut count = 0;
    let mut previous: Option<Pair> = None;
    let mut i = 0;
    while i + 1 < lines.len() {
        let (code, value) = (trim(lines[i]), trim(lines[i + 1]));
        if code == b"2" && previous == Some((b"0", b"SECTION")) {
            section = Some(value);
        }
        if section == Some(b"ENTITIES")
            && code == b"0"
            && ![&b"SECTION"[..], b"ENDSEC", b"VERTEX", b"SEQEND", b"ATTRIB"].contains(&value)
        {
            count += 1;
        }
        if code == b"0" && value == b"ENDSEC" {
            section = None;
        }
        previous = Some((code, value));
        i += 2;
    }
    count
}

struct Run<'a> {
    root: &'a SkillRoot,
    q: &'a MdDrawingRouteRequest,
    home: PathBuf,
    out: PathBuf,
    work: PathBuf,
    lock: Value,
    exe: PathBuf,
    exe_sha256: String,
    program: Option<PathBuf>,
    runtime: Option<MdCadcraftRuntime>,
    given: Vec<(PathBuf, String)>,
    dxf: Option<PathBuf>,
    svg: Option<PathBuf>,
    project: Option<oe_kernel::project::Project>,
    context: Option<ProjectReviewContext>,
    request: Option<MdUsageDiscoveryRequest>,
    page: Option<PathBuf>,
    correction: Option<(MdDrawingCorrectionPlan, MdDrawingCorrectionReceipt)>,
    correction_plan: Option<PathBuf>,
}

pub fn route(root: &SkillRoot, q: &MdDrawingRouteRequest) -> Result<MdDrawingRouteReceipt> {
    run_route(root,q,None)
}

/// The same route with an explicit source-bound plan. The plan is never passed to classification.
/// Automatic plans derived from rule findings are a separate producer of this execution capability.
pub fn route_with_correction(root:&SkillRoot,q:&MdDrawingRouteRequest,plan:&Path)->Result<MdDrawingRouteReceipt> {
    run_route(root,q,Some(plan.to_path_buf()))
}
fn run_route(root:&SkillRoot,q:&MdDrawingRouteRequest,correction_plan:Option<PathBuf>)->Result<MdDrawingRouteReceipt> {
    ensure!(
        q.validate().is_ok() && q.schema == REQUEST_SCHEMA,
        "drawing_route_contract"
    );
    ensure!(
        match (&q.dwg, &q.dxf, &q.svg) {
            (Some(_), None, None) => q.cadcraft_runtime.is_some(),
            (None, Some(_), Some(_)) => true,
            _ => false,
        },
        "drawing_route_source:give a DWG with the CADCraft runtime record, or a DXF with its rendering"
    );
    let out = root.private_path(Path::new(&q.output_directory))?;
    std::fs::create_dir(&out).context("drawing_route_output_directory_must_be_new")?;
    let work = out.join("work");
    std::fs::create_dir(&work)?;
    std::fs::create_dir(work.join("tmp"))?;
    let exe = std::env::current_exe()?;
    let exe_sha256 = sha256(read_regular_bytes(&exe, LIMIT)?);
    let mut run = Run {
        root,
        q,
        home: root.data_path().to_owned(),
        out,
        work,
        lock: lock()?,
        exe,
        exe_sha256,
        program: None,
        runtime: None,
        given: Vec::new(),
        dxf: None,
        svg: None,
        project: None,
        context: None,
        request: None,
        page: None,
        correction: None,
        correction_plan,
    };
    let started_at = utc_now();
    let mut steps: Vec<MdDrawingRouteStep> = STEPS
        .iter()
        .filter(|id|run.correction_plan.is_some() || !["write-corrected","read-corrected","render-corrected","check-corrected"].contains(id))
        .map(|id| MdDrawingRouteStep {
            id: (*id).into(),
            status: "not_run".into(),
            seconds: 0.,
            inputs: Vec::new(),
            outputs: Vec::new(),
            external_processes: Vec::new(),
            facts: json!({}),
            failure: None,
        })
        .collect();
    let (mut status, mut failure) = ("passed", None);
    for step in &mut steps {
        let t = Instant::now();
        let result = run.step(step);
        step.seconds = (t.elapsed().as_secs_f64() * 1000.).round() / 1000.;
        match result {
            Ok(outcome) => {
                step.status = outcome.into();
                if outcome == "skipped" {
                    status = "page_not_checked";
                }
            }
            Err(error) => {
                let message = format!("{error:#}");
                step.status = "failed".into();
                step.failure = Some(message.clone());
                failure = Some(json!({"step": step.id, "message": message}));
                status = "failed";
                break;
            }
        }
    }
    let receipt = MdDrawingRouteReceipt {
        schema: RECEIPT_SCHEMA.into(),
        status: status.into(),
        request: q.clone(),
        cadcraft_lock: run.lock.clone(),
        executable_sha256: run.exe_sha256.clone(),
        started_at,
        finished_at: utc_now(),
        failure,
        steps,
        scope: SCOPE.into(),
    };
    ensure!(receipt.validate().is_ok(), "drawing_route_receipt_contract");
    oe_core::write_new_bytes(
        &run.out.join("route-receipt.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

impl Run<'_> {
    fn step(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        match s.id.as_str() {
            "verify-runtime" => self.verify_runtime(s),
            "read-source" => self.read_source(s),
            "probe-sandbox" => self.probe_sandbox(s),
            "convert" => self.convert(s),
            "render" => self.render(s),
            "context" => self.context(s),
            "request" => self.write_request(s),
            "classify" => self.classify(s),
            "check-graph" => self.check_graph(s),
            "write-corrected" => self.write_corrected(s),
            "read-corrected" => self.read_corrected(s),
            "render-corrected" => self.render_corrected(s),
            "check-corrected" => self.check_corrected(s),
            "page" => self.page(s),
            "check-page" => self.check_page(s),
            "check-rules" => self.check_rules(s),
            "verify-source" => self.verify_source(s),
            other => Err(anyhow!("drawing_route_step_unknown:{other}")),
        }
    }

    fn rel(&self, p: &Path) -> String {
        match p.strip_prefix(&self.home) {
            Ok(r) if !r.as_os_str().is_empty() => r.to_string_lossy().into_owned(),
            _ => p.to_string_lossy().into_owned(),
        }
    }

    fn file(&self, p: &Path) -> Result<MdDrawingRouteFile> {
        oe_drawing_check::file(Some(&self.home), p)
    }

    /// The fixed environment of an external program: only the named variables, nothing inherited.
    fn environment(vars: &[(&str, OsString)]) -> (BTreeMap<OsString, OsString>, Vec<OsString>) {
        let set: BTreeMap<OsString, OsString> = vars
            .iter()
            .map(|(k, v)| (OsString::from(k), v.clone()))
            .collect();
        let remove = std::env::vars_os()
            .map(|(k, _)| k)
            .filter(|k| !set.contains_key(k))
            .collect();
        (set, remove)
    }

    fn cadcraft_environment(&self, fonts: bool) -> Vec<(&'static str, OsString)> {
        // No inherited CADCraft settings. The conversion runs without the SHX fonts: for justified TEXT CADCraft
        // writes its own baseline start (group 10) computed from the font whenever the stored one is far from it,
        // so the DXF bytes would depend on which fonts are present. Rendering uses the recorded fonts.
        let mut vars = vec![
            ("PATH", OsString::from("/usr/bin:/bin:/usr/sbin:/sbin")),
            (
                "HOME",
                std::env::var_os("HOME").unwrap_or_else(|| "/".into()),
            ),
            ("TMPDIR", self.work.join("tmp").into_os_string()),
            (
                "CADCRAFT_CONFIG_DIR",
                self.work.join("cadcraft-config").into_os_string(),
            ),
        ];
        if fonts
            && let Some(dir) = self
                .runtime
                .as_ref()
                .and_then(|r| r.fonts_directory.clone())
        {
            vars.push(("CADCRAFT_SHX_PATH", OsString::from(dir)));
        }
        vars
    }

    #[allow(clippy::too_many_arguments)]
    fn call(
        &self,
        s: &mut MdDrawingRouteStep,
        name: &str,
        program: &Path,
        arguments: Vec<OsString>,
        vars: &[(&str, OsString)],
        cwd: &Path,
        seconds: u64,
        stdin: Option<&Path>,
        expect_success: bool,
    ) -> Result<ProcessObservation> {
        let (environment, remove_environment) = Self::environment(vars);
        let spec = CommandSpec {
            executable: program.to_owned(),
            arguments,
            cwd: cwd.to_owned(),
            environment,
            remove_environment,
            timeout: Duration::from_secs(seconds),
            output_limit: 64 * 1024 * 1024,
        };
        let evidence = self.work.join(name);
        let observed = match stdin {
            Some(input) => {
                let identity = oe_core::fingerprint_file(input, 16 * 1024 * 1024)?;
                run_command_with_stdin_file_deadline(
                    &spec,
                    &evidence,
                    Instant::now() + Duration::from_secs(seconds + 60),
                    input,
                    &identity,
                )?
            }
            None => run_command(&spec, &evidence)?,
        };
        let mut record = observed.report.clone();
        record["name"] = json!(name);
        record["evidence"] = json!(self.rel(&evidence));
        s.external_processes.push(record);
        if expect_success {
            let tail = String::from_utf8_lossy(
                &observed.stderr[observed.stderr.len().saturating_sub(2000)..],
            )
            .into_owned();
            ensure!(
                observed.status == "completed" && observed.exit_code == Some(0),
                "{name} exited {:?} ({}): {}",
                observed.exit_code,
                observed.status,
                tail.trim()
            );
        }
        Ok(observed)
    }

    fn verify_runtime(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        s.facts["executable_sha256"] = json!(self.exe_sha256);
        s.facts["cadcraft_lock"] = json!({"repository": self.lock["repository"], "commit": self.lock["commit"], "version": self.lock["version"]});
        let Some(record) = &self.q.cadcraft_runtime else {
            s.facts["cadcraft"] = json!("not used: DXF and its rendering given");
            return Ok("passed");
        };
        let record_path = PathBuf::from(record);
        s.inputs.push(self.file(&record_path)?);
        let runtime: MdCadcraftRuntime = serde_json::from_value(oe_core::json::strict_json(
            &read_regular_bytes(&record_path, 1024 * 1024)?,
        )?)?;
        let program = verify_cadcraft(&runtime, &self.lock)?;
        self.runtime = Some(runtime);
        let vars = self.cadcraft_environment(false);
        let observed = self.call(
            s,
            "cadcraft-version",
            &program,
            vec!["--version".into()],
            &vars,
            &self.work,
            60,
            None,
            true,
        )?;
        let version = String::from_utf8_lossy(&observed.stdout).trim().to_owned();
        ensure!(
            self.lock["version"] == version.as_str(),
            "cadcraft_version_mismatch:{version}"
        );
        s.facts["verified"] = json!([
            "CADCraft repository and commit equal the compiled lock",
            "CADCraft program digest equals its runtime record",
            "CADCraft reports the locked version",
            "font manifest unchanged when recorded"
        ]);
        self.program = Some(program);
        Ok("passed")
    }

    fn read_source(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        if let Some(dwg) = &self.q.dwg {
            let dwg = std::fs::canonicalize(dwg).context("drawing_route_dwg_missing")?;
            ensure!(dwg.is_file(), "drawing_route_dwg_not_a_file");
            let f = self.file(&dwg)?;
            self.given.push((dwg, f.sha256.clone()));
            s.inputs.push(f);
            s.facts["source_kind"] = json!("dwg");
            return Ok("passed");
        }
        // A DXF with its rendering: both copied into the output directory, which the request then names.
        for (given, name) in [
            (self.q.dxf.as_ref(), "source.dxf"),
            (self.q.svg.as_ref(), "source.svg"),
        ] {
            let given = std::fs::canonicalize(given.context("drawing_route_source_missing")?)?;
            let bytes = read_regular_bytes(&given, 64 * 1024 * 1024)?;
            let target = self.out.join(name);
            oe_core::write_new_bytes(&target, &bytes)?;
            s.inputs.push(self.file(&given)?);
            s.outputs.push(self.file(&target)?);
            self.given.push((given, sha256(&bytes)));
            if name == "source.dxf" {
                self.dxf = Some(target);
            } else {
                self.svg = Some(target);
            }
        }
        s.facts["source_kind"] = json!("dxf");
        Ok("passed")
    }

    fn probe_sandbox(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        ensure!(cfg!(target_os = "macos"), "drawing_route_sandbox_platform");
        let probe = self.work.join("probe");
        std::fs::create_dir(&probe)?;
        let input = probe.join("synthetic.dxf");
        oe_core::write_new_bytes(&input, PROBE_DXF)?;
        let sandbox = Path::new("/usr/bin/sandbox-exec");
        let worker = |extra: &[&str]| -> Result<Vec<OsString>> {
            let mut a: Vec<OsString> = vec![
                "-p".into(),
                usage_discovery::worker_profile(&self.exe)?.into(),
                self.exe.clone().into_os_string(),
                review::WORKER_FLAG.into(),
            ];
            a.extend(extra.iter().map(OsString::from));
            Ok(a)
        };
        let root = Path::new("/");
        let observed = self.call(
            s,
            "probe-worker",
            sandbox,
            worker(&[])?,
            &[],
            root,
            120,
            Some(&input),
            true,
        )?;
        let graph: Value = serde_json::from_slice(&observed.stdout)
            .context("drawing_route_probe_worker_output")?;
        ensure!(
            graph["schema"] == engineering_expression::SCHEMA
                && graph["source"]["entities"].as_array().map(Vec::len) == Some(2),
            "the sandboxed worker did not return the expression graph of the synthetic bytes"
        );
        let canary = probe.join("forbidden-canary.txt");
        oe_core::write_new_bytes(&canary, b"Boundary probe only, not engineering data.\n")?;
        let canary = canary.into_os_string();
        for (name, program, rest) in [
            ("probe-file-read", "/bin/cat", vec![canary]),
            (
                "probe-network",
                "/usr/bin/nc",
                ["-vz", "-w", "1", "127.0.0.1", "9"]
                    .iter()
                    .map(OsString::from)
                    .collect(),
            ),
        ] {
            let mut a: Vec<OsString> = vec![
                "-p".into(),
                usage_discovery::worker_profile(Path::new(program))?.into(),
                program.into(),
            ];
            a.extend(rest);
            let observed = self.call(s, name, sandbox, a, &[], root, 60, None, false)?;
            ensure!(
                observed.exit_code != Some(0)
                    && String::from_utf8_lossy(&observed.stderr)
                        .contains("Operation not permitted"),
                "{name}: the deny-default rules did not deny it"
            );
        }
        let observed = self.call(
            s,
            "probe-arguments",
            sandbox,
            worker(&["unexpected-path"])?,
            &[],
            root,
            60,
            Some(&input),
            false,
        )?;
        ensure!(
            observed.exit_code != Some(0)
                && String::from_utf8_lossy(&observed.stderr)
                    .contains("expression_worker_no_extra_arguments"),
            "the worker accepted an extra argument"
        );
        s.facts["probes"] = json!([
            "synthetic bytes give a graph",
            "file read denied",
            "network denied",
            "extra worker argument rejected"
        ]);
        s.outputs
            .push(self.file(&self.work.join("probe-worker/stdout"))?);
        Ok("passed")
    }

    fn convert(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let Some(program) = self.program.clone() else {
            s.facts["reason"] = json!("DXF given");
            return Ok("not_applicable");
        };
        let dwg = self.given[0].0.clone();
        let dxf = self.out.join("source.dxf");
        let command = format!(
            "saveas {}",
            serde_json::to_string(&json!({"path": dxf, "version": 2018}))?
        );
        s.inputs.push(self.file(&dwg)?);
        let vars = self.cadcraft_environment(false);
        s.facts["environment"] = json!(vars.iter().map(|(k, _)| *k).collect::<Vec<_>>());
        self.call(
            s,
            "convert",
            &program,
            vec![
                "run".into(),
                dwg.into_os_string(),
                "--cmd".into(),
                command.into(),
            ],
            &vars,
            &self.work,
            900,
            None,
            true,
        )?;
        let bytes = read_regular_bytes(&dxf, 64 * 1024 * 1024).context("CADCraft wrote no DXF")?;
        ensure!(!bytes.is_empty(), "CADCraft wrote no DXF");
        let entities = entity_count(&bytes);
        ensure!(entities > 0, "the DXF written by CADCraft has no entities");
        s.facts["entities"] = json!(entities);
        s.outputs.push(self.file(&dxf)?);
        self.dxf = Some(dxf);
        Ok("passed")
    }

    fn render(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let Some(program) = self.program.clone() else {
            s.facts["reason"] = json!("rendering given");
            return Ok("not_applicable");
        };
        let dxf = self.dxf.clone().context("drawing_route_dxf_missing")?;
        let (svg, png) = (
            self.out.join("source.svg"),
            self.out.join("source-preview.png"),
        );
        let mut plot = json!({"path": png});
        if let Some(fonts) = self
            .runtime
            .as_ref()
            .and_then(|r| r.fonts_directory.clone())
        {
            plot["fontDirectory"] = json!(fonts);
        }
        s.inputs.push(self.file(&dxf)?);
        let vars = self.cadcraft_environment(true);
        s.facts["environment"] = json!(vars.iter().map(|(k, _)| *k).collect::<Vec<_>>());
        let arguments = vec![
            "run".into(),
            dxf.into_os_string(),
            "--cmd".into(),
            format!("reviewplot {}", serde_json::to_string(&plot)?).into(),
            "--export".into(),
            svg.clone().into_os_string(),
        ];
        self.call(
            s, "render", &program, arguments, &vars, &self.work, 900, None, true,
        )?;
        for f in [&svg, &png] {
            ensure!(
                std::fs::metadata(f).is_ok_and(|m| m.is_file() && m.len() > 0),
                "CADCraft wrote no {}",
                f.display()
            );
        }
        let shown = page::rendering(&read_regular_bytes(&svg, 64 * 1024 * 1024)?)?;
        ensure!(
            shown.active.is_empty(),
            "the SVG carries active content: {}",
            shown.active.join(",")
        );
        s.facts["rendered_roots"] = json!(shown.entities.len());
        s.outputs.push(self.file(&svg)?);
        s.outputs.push(self.file(&png)?);
        self.svg = Some(svg);
        Ok("passed")
    }

    fn context(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let project_path = self.root.private_path(Path::new(&self.q.project))?;
        let object_path = self.root.private_path(Path::new(&self.q.object))?;
        s.inputs.push(self.file(&project_path)?);
        s.inputs.push(self.file(&object_path)?);
        let project: oe_kernel::project::Project = serde_json::from_value(
            oe_core::json::strict_json(&read_regular_bytes(&project_path, 64 * 1024 * 1024)?)?,
        )?;
        let object: ObjectIdentity = serde_json::from_value(oe_core::json::strict_json(
            &read_regular_bytes(&object_path, 1024 * 1024)?,
        )?)?;
        let context = oe_kernel::project::context(&project, &object)?;
        let target = self.out.join("project-context.json");
        oe_core::write_new_bytes(&target, &serde_json::to_vec_pretty(&context)?)?;
        s.outputs.push(self.file(&target)?);
        self.project = Some(project);
        self.context = Some(context);
        Ok("passed")
    }

    fn write_request(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let c = self
            .context
            .clone()
            .context("drawing_route_context_missing")?;
        let dxf = self.dxf.clone().context("drawing_route_dxf_missing")?;
        let q = MdUsageDiscoveryRequest {
            schema: "oev2.drawing-discovery-request/1".into(),
            subject: ObjectIdentity {
                object_id: c.object_id.clone(),
                revision: c.object_revision.clone(),
                condition_scope: c.condition_scope.clone(),
                source_sha256: c.source_sha256.clone(),
            },
            project_context: c,
            source: ArtifactReference {
                path: self.root.portable_data_path(&dxf)?,
                sha256: sha256(read_regular_bytes(&dxf, 64 * 1024 * 1024)?),
            },
            profile: engineering_expression::PROFILE.into(),
            output_directory: self.root.portable_data_path(&self.out.join("classified"))?,
        };
        let target = self.out.join("request.json");
        oe_core::write_new_bytes(&target, &serde_json::to_vec_pretty(&q)?)?;
        s.inputs.push(self.file(&dxf)?);
        s.outputs.push(self.file(&target)?);
        self.request = Some(q);
        Ok("passed")
    }

    fn classify(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let q = self
            .request
            .clone()
            .context("drawing_route_request_missing")?;
        let project = self
            .project
            .as_ref()
            .context("drawing_route_project_missing")?;
        s.inputs.push(self.file(&self.out.join("request.json"))?);
        review::review(self.root, project, &q)?;
        for name in ["graph.json", "review.json", "request.json"] {
            s.outputs
                .push(self.file(&self.out.join("classified").join(name))?);
        }
        Ok("passed")
    }

    fn pinned(&self) -> Result<Vec<MdPinnedFile>> {
        let mut pinned = vec![MdPinnedFile {
            name: "Route executable".into(),
            path: self.exe.to_string_lossy().into_owned(),
            sha256: self.exe_sha256.clone(),
        }];
        if let (Some(r), Some(_)) = (&self.runtime, &self.program) {
            pinned.push(MdPinnedFile {
                name: "CADCraft program".into(),
                path: r.program_path.clone(),
                sha256: r.program_sha256.clone(),
            });
            if let (Some(p), Some(h)) = (&r.fonts_manifest_path, &r.fonts_manifest_sha256) {
                pinned.push(MdPinnedFile {
                    name: "CADCraft font manifest".into(),
                    path: p.clone(),
                    sha256: h.clone(),
                });
            }
        }
        Ok(pinned)
    }

    fn check_graph(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let svg = self.svg.clone().context("drawing_route_svg_missing")?;
        let output = self.out.join("graph-check.json");
        let q = MdExpressionGraphCheckRequest {
            schema: oe_drawing_check::graph::REQUEST_SCHEMA.into(),
            discovery_request: self
                .root
                .portable_data_path(&self.out.join("request.json"))?,
            svg: self.root.portable_data_path(&svg)?,
            worker_executable: self.exe.to_string_lossy().into_owned(),
            pinned: self.pinned()?,
            output: self.root.portable_data_path(&output)?,
        };
        for f in ["classified/graph.json", "source.dxf"] {
            s.inputs.push(self.file(&self.out.join(f))?);
        }
        s.inputs.push(self.file(&svg)?);
        let report = oe_drawing_check::graph::check(self.root, &q)?;
        s.facts["assertions"] = json!(report.count);
        s.facts["source_derived_assertions"] = json!(report.source_derived_count);
        s.outputs.push(self.file(&output)?);
        Ok("passed")
    }

    fn page(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let svg = self.svg.clone().context("drawing_route_svg_missing")?;
        let graph = self.out.join("classified/graph.json");
        let target = self.out.join("review-page.html");
        let source_note = if self.program.is_some() {
            format!(
                "源 DXF 由固定 {} 从 DWG 转出",
                self.lock["version"].as_str().unwrap_or_default()
            )
        } else {
            "源 DXF 与其渲染由调用方给出".into()
        };
        let q = MdReviewPageRequest {
            schema: page::REQUEST_SCHEMA.into(),
            graph: self.root.portable_data_path(&graph)?,
            svg: self.root.portable_data_path(&svg)?,
            title: self.q.title.clone(),
            candidate: "固定路线".into(),
            version: "oev2 drawing route".into(),
            source_note,
            status_note: PAGE_STATUS.into(),
            output: self.root.portable_data_path(&target)?,
        };
        s.inputs.push(self.file(&graph)?);
        s.inputs.push(self.file(&svg)?);
        let result = page::render(self.root, &q)?;
        s.facts["roots"] = json!(result.root_count);
        s.facts["rendered_roots"] = json!(result.rendered_root_count);
        s.facts["expressions"] = json!(result.expression_count);
        s.outputs.push(self.file(&target)?);
        self.page = Some(target);
        Ok("passed")
    }

    fn check_page(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let Some(chrome) = self.q.chrome.clone() else {
            s.facts["reason"] = json!("no Chrome given: the page is not checked");
            return Ok("skipped");
        };
        let page = self.page.clone().context("drawing_route_page_missing")?;
        s.inputs.push(self.file(&page)?);
        let mut runs = serde_json::Map::new();
        for mode in MODES {
            let q = MdReviewPageCheckRequest {
                schema: oe_drawing_check::browser::REQUEST_SCHEMA.into(),
                page: self.root.portable_data_path(&page)?,
                output_directory: self
                    .root
                    .portable_data_path(&self.out.join(format!("browser-{mode}")))?,
                chrome: chrome.clone(),
                mode: mode.into(),
                region: None,
                focus_label: None,
            };
            let report = oe_drawing_check::browser::check_page(self.root, &q)?;
            s.facts["browser"] = json!(report.browser);
            runs.insert(mode.into(), json!(report.count));
            s.outputs.push(report.result);
            s.outputs.push(report.screenshot);
        }
        s.facts["runs"] = Value::Object(runs);
        Ok("passed")
    }

    /// The drafting rules of the ontology on the frozen graph and the source bytes: findings to review, never a
    /// failure of the route.
    fn check_rules(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let output = self.out.join("drafting-rules.json");
        let q = MdDraftingRuleRequest {
            schema: oe_drawing_check::drafting::REQUEST_SCHEMA.into(),
            discovery_request: self
                .root
                .portable_data_path(&self.out.join("request.json"))?,
            output: self.root.portable_data_path(&output)?,
        };
        for f in ["classified/graph.json", "source.dxf"] {
            s.inputs.push(self.file(&self.out.join(f))?);
        }
        let report = oe_drawing_check::drafting::check_rules(self.root, &q)?;
        s.facts["rules"] = json!(report.rule_count);
        s.facts["findings"] = json!(report.findings.len());
        s.facts["by_layer"] = json!(
            report
                .layers
                .iter()
                .map(|l| (l.layer.clone(), json!(l.finding_count)))
                .collect::<serde_json::Map<_, _>>()
        );
        s.outputs.push(self.file(&output)?);
        for layer in &report.layers {
            s.outputs
                .push(self.file(&self.root.data_expand(Path::new(&layer.facts_file))?)?);
        }
        Ok("passed")
    }

    fn verify_source(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        for (path, before) in &self.given {
            let f = self.file(path)?;
            ensure!(
                f.sha256 == *before,
                "the source changed while the route ran: {}",
                path.display()
            );
            s.inputs.push(f);
        }
        Ok("passed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_now_is_an_iso_timestamp() {
        let t = utc_now();
        assert!(
            t.len() == 27 && t.ends_with('Z') && t.as_bytes()[10] == b'T',
            "{t}"
        );
    }
    #[test]
    fn entities_are_counted_in_the_entities_section_only() {
        let dxf = b"0\nSECTION\n2\nBLOCKS\n0\nLINE\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n0\nPOLYLINE\n0\nVERTEX\n0\nSEQEND\n0\nENDSEC\n0\nEOF\n";
        assert_eq!(entity_count(dxf), 2);
    }
}
