//! Reading an existing engineering drawing through the current Project: the fixed route, source-only
//! discovery, the independent check, the review page and its browser checks. Paths are relative to the
//! project home unless absolute.
use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_core::SkillRoot;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum DrawingCommand {
    /// Run the fixed route on one drawing: a DWG through the pinned CADCraft, or a DXF with its rendering;
    /// classification in the isolated worker, independent check, review page and browser checks, one receipt.
    Route {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        object: PathBuf,
        #[arg(long, conflicts_with_all = ["dxf", "svg"])]
        dwg: Option<PathBuf>,
        #[arg(long, requires = "svg")]
        dxf: Option<PathBuf>,
        #[arg(long, requires = "dxf")]
        svg: Option<PathBuf>,
        /// Local record of the pinned converter (oev2.cadcraft-runtime/1); required with --dwg.
        #[arg(long)]
        cadcraft_runtime: Option<PathBuf>,
        /// New directory inside the project home.
        #[arg(long)]
        output_directory: PathBuf,
        /// Drawing named on the review page.
        #[arg(long)]
        title: String,
        /// Chrome for the offline page checks; without it the page is not checked and the route does not pass.
        #[arg(long)]
        chrome: Option<PathBuf>,
    },
    /// Classify the drawing named by the request in the isolated bytes-only worker under the current Project.
    Discover {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        request: PathBuf,
    },
    /// Check a discovery result independently against the bytes of its source.
    Check {
        /// Discovery request; its output directory holds graph.json and review.json.
        #[arg(long)]
        request: PathBuf,
        /// Neutral rendering of the same DXF.
        #[arg(long)]
        svg: PathBuf,
        /// Executable that ran the worker; this oev2 when omitted.
        #[arg(long)]
        worker: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Write the review page of a frozen graph over its rendering.
    Page {
        #[arg(long)]
        graph: PathBuf,
        #[arg(long)]
        svg: PathBuf,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "候选")]
        candidate: String,
        #[arg(long, default_value = "")]
        version: String,
        #[arg(long, default_value = "")]
        source_note: String,
        #[arg(long, default_value = "")]
        status_note: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Check a review page in headless Chrome in one display mode.
    CheckPage {
        #[arg(long)]
        page: PathBuf,
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long)]
        chrome: PathBuf,
        #[arg(long, default_value = "kind")]
        mode: String,
        /// Drawing-coordinate box x0,y0,x1,y1 for the screenshot.
        #[arg(long)]
        region: Option<String>,
        /// Label text of the expression focused in focus mode.
        #[arg(long)]
        focus_label: Option<String>,
    },
}

fn read(root: &SkillRoot, path: &Path) -> Result<serde_json::Value> {
    Ok(oe_core::json::strict_json(&oe_core::read_regular_bytes(
        &root.private_path(path)?,
        16 * 1024 * 1024,
    )?)?)
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A failure line without paths or drawing content: the error code and, for a failed check, the kind of the
/// check that failed (its label up to the first colon, without handles or identifiers).
pub fn failure(error: &anyhow::Error) -> serde_json::Value {
    let message = error.to_string();
    let mut parts = message.splitn(3, ':');
    let code = parts.next().unwrap_or_default();
    let identifier = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    let reason = if !identifier(code) {
        "unclassified".to_owned()
    } else if code == "drawing_check_failed" {
        format!("{code}:{}", parts.next().unwrap_or_default().trim())
    } else {
        code.to_owned()
    };
    serde_json::json!({"schema":"oev2.command-failure/1","command":"drawing","execution":"failed",
        "conclusion":"unknown","engineering_adoption":"blocked","error":{"reason":reason}})
}

/// The result and the exit code: a route that did not pass exits 1.
pub fn run(root: &SkillRoot, command: DrawingCommand) -> Result<(serde_json::Value, u8)> {
    let value = execute(root, command)?;
    let code = u8::from(
        value["schema"] == oe_drawing::route::RECEIPT_SCHEMA && value["status"] != "passed",
    );
    Ok((value, code))
}

fn execute(root: &SkillRoot, command: DrawingCommand) -> Result<serde_json::Value> {
    match command {
        DrawingCommand::Route {
            project,
            object,
            dwg,
            dxf,
            svg,
            cadcraft_runtime,
            output_directory,
            title,
            chrome,
        } => {
            crate::v2::require_execution(root, "drawing_route")?;
            let expand = |p: Option<PathBuf>| -> Result<Option<String>> {
                p.map(|p| root.data_expand(&p).map(|p| text(&p)))
                    .transpose()
            };
            let q = oe_contracts::MdDrawingRouteRequest {
                schema: oe_drawing::route::REQUEST_SCHEMA.into(),
                project: text(&project),
                object: text(&object),
                dwg: expand(dwg)?,
                dxf: expand(dxf)?,
                svg: expand(svg)?,
                cadcraft_runtime: expand(cadcraft_runtime)?,
                output_directory: text(&output_directory),
                title,
                chrome: expand(chrome)?,
            };
            Ok(serde_json::to_value(oe_drawing::route::route(root, &q)?)?)
        }
        DrawingCommand::Discover { project, request } => {
            crate::v2::require_execution(root, "drawing_understanding")?;
            let project: oe_kernel::project::Project =
                serde_json::from_value(read(root, &project)?)?;
            let raw = read(root, &request)?;
            ensure!(
                raw.as_object().is_some_and(|o| o.keys().all(|k| [
                    "schema",
                    "subject",
                    "project_context",
                    "source",
                    "profile",
                    "output_directory"
                ]
                .contains(&k.as_str()))),
                "expression_unexpected_input"
            );
            let q: oe_contracts::MdUsageDiscoveryRequest = serde_json::from_value(raw)?;
            Ok(serde_json::to_value(oe_drawing::review::review(
                root, &project, &q,
            )?)?)
        }
        DrawingCommand::Check {
            request,
            svg,
            worker,
            output,
        } => {
            crate::v2::require_execution(root, "drawing_check")?;
            let worker = match worker {
                Some(w) => text(&root.data_expand(&w)?),
                None => text(&std::env::current_exe()?),
            };
            let q = oe_contracts::MdExpressionGraphCheckRequest {
                schema: oe_drawing_check::graph::REQUEST_SCHEMA.into(),
                discovery_request: text(&request),
                svg: text(&svg),
                worker_executable: worker,
                pinned: Vec::new(),
                output: text(&output),
            };
            Ok(serde_json::to_value(oe_drawing_check::graph::check(
                root, &q,
            )?)?)
        }
        DrawingCommand::Page {
            graph,
            svg,
            title,
            candidate,
            version,
            source_note,
            status_note,
            output,
        } => {
            crate::v2::require_execution(root, "drawing_page")?;
            let q = oe_contracts::MdReviewPageRequest {
                schema: oe_drawing::page::REQUEST_SCHEMA.into(),
                graph: text(&graph),
                svg: text(&svg),
                title,
                candidate,
                version,
                source_note,
                status_note,
                output: text(&output),
            };
            Ok(serde_json::to_value(oe_drawing::page::render(root, &q)?)?)
        }
        DrawingCommand::CheckPage {
            page,
            output_directory,
            chrome,
            mode,
            region,
            focus_label,
        } => {
            crate::v2::require_execution(root, "drawing_page_check")?;
            let region = region
                .map(|r| {
                    r.split(',')
                        .map(|x| x.trim().parse::<f64>())
                        .collect::<std::result::Result<Vec<_>, _>>()
                })
                .transpose()?;
            let q = oe_contracts::MdReviewPageCheckRequest {
                schema: oe_drawing_check::browser::REQUEST_SCHEMA.into(),
                page: text(&page),
                output_directory: text(&output_directory),
                chrome: text(&root.data_expand(&chrome)?),
                mode,
                region,
                focus_label,
            };
            Ok(serde_json::to_value(
                oe_drawing_check::browser::check_page(root, &q)?,
            )?)
        }
    }
}
