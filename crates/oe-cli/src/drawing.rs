//! Reading an existing engineering drawing through the current Project: the fixed route, source-only
//! discovery, the independent check, the review page and its browser checks. Paths are relative to the
//! project home unless absolute.
use anyhow::{Context, Result, ensure};
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
    /// Evaluate the drafting rules of the ontology on a discovery result: facts from the source bytes and the
    /// frozen graph in three layers, the shapes on each, one report.
    Rules {
        /// Discovery request; its output directory holds graph.json.
        #[arg(long)]
        request: PathBuf,
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
    /// Developer command of the pixel route: synthesise the expression graph of one page image from
    /// its tile index and observation files (no model calls), write graph, provenance, refined
    /// members, calibration, overlay rendering and review page into a new directory, check the graph.
    PixelSynthesize {
        /// Page image (PNG at the tile index's resolution); its `tiles/` sibling directory, when
        /// present, provides the tile images for digest verification.
        #[arg(long)]
        page: PathBuf,
        /// Tile index (terra-parity.tiles/1).
        #[arg(long)]
        tiles: PathBuf,
        /// Directory of the L0-, L1A-, L1B- and L1C- observation files.
        #[arg(long)]
        observations: PathBuf,
        /// New directory for the outputs.
        #[arg(long)]
        output_directory: PathBuf,
        /// Title-block scale note (1:1.5) overriding the one of the page layout observation.
        #[arg(long)]
        scale_note: Option<String>,
        /// Page key of the tile index; the page file's parent directory name when omitted.
        #[arg(long)]
        page_name: Option<String>,
    },
}

/// Read a directory's files with one of the given name prefixes and the `.json` or `.png` extension,
/// sorted by name, as `(file name, bytes)`.
fn read_named(dir: &Path, prefixes: &[&str], extension: &str, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .with_context(|| format!("pixel_directory:{}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| n.ends_with(extension) && prefixes.iter().any(|p| n.starts_with(p)))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|n| Ok((n.clone(), oe_core::read_regular_bytes(&dir.join(&n), limit)?)))
        .collect()
}

fn pixel_synthesize(
    page: &Path,
    tiles: &Path,
    observations: &Path,
    output_directory: &Path,
    scale_note: Option<String>,
    page_name: Option<String>,
) -> Result<serde_json::Value> {
    const LIMIT: usize = 64 * 1024 * 1024;
    let page_png = oe_core::read_regular_bytes(page, LIMIT)?;
    let tile_index = oe_core::read_regular_bytes(tiles, LIMIT)?;
    let name = match page_name {
        Some(n) => n,
        None => page
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .context("pixel_page_name")?,
    };
    let files = read_named(observations, &["L0-", "L1A-", "L1B-", "L1C-"], ".json", LIMIT)?;
    let tile_dir = page.parent().map(|p| p.join("tiles"));
    let tile_pngs: std::collections::BTreeMap<String, Vec<u8>> = match tile_dir.filter(|d| d.is_dir()) {
        Some(d) => read_named(&d, &["t_"], ".png", LIMIT)?.into_iter().collect(),
        None => Default::default(),
    };
    let out = oe_drawing_pixel::synthesize_page(&oe_drawing_pixel::PageInput {
        page: &name,
        page_png: &page_png,
        tile_index: &tile_index,
        observations: &files,
        tile_pngs: &tile_pngs,
        scale_note: scale_note.as_deref(),
        threshold: 128,
    })?;
    std::fs::create_dir(output_directory)
        .with_context(|| format!("pixel_output_directory_exists:{}", output_directory.display()))?;
    let mut outputs = Vec::new();
    let mut write = |file: &str, bytes: &[u8]| -> Result<()> {
        let path = output_directory.join(file);
        oe_core::write_new_bytes(&path, bytes)?;
        outputs.push(serde_json::json!({"path": text(&path), "bytes": bytes.len(), "sha256": oe_core::sha256(bytes)}));
        Ok(())
    };
    write("graph.json", &out.graph)?;
    write("provenance.json", &out.provenance)?;
    write("refined-members.json", &out.refined_members)?;
    write("calibration.json", &out.calibration)?;
    write("source.svg", out.overlay_svg.as_bytes())?;
    let check = match oe_drawing::check_graph_bytes(&out.graph) {
        Ok(()) => serde_json::json!({"status": "passed"}),
        Err(e) => {
            let message = e.to_string();
            // While the generated contract lacks the pixel profile in its allowed values, the same
            // graph is checked again with the DXF profile string, which no invariant reads, so the
            // report tells the contract gap apart from a graph defect.
            let pending = message.ends_with("graph: MdEngineeringExpressionGraph.profile:allowed_values");
            let other_invariants = if pending {
                let mut g: serde_json::Value = serde_json::from_slice(&out.graph)?;
                g["profile"] = serde_json::json!("dxf-expression-rules/2");
                match oe_drawing::check_graph_bytes(&serde_json::to_vec(&g)?) {
                    Ok(()) => serde_json::json!("passed"),
                    Err(e) => serde_json::json!(format!("failed: {e}")),
                }
            } else {
                serde_json::Value::Null
            };
            serde_json::json!({"status": "failed", "error": message, "profile_pending_in_generated_contract": pending,
                "other_invariants_with_dxf_profile_string": other_invariants})
        }
    };
    let q = oe_contracts::MdReviewPageRequest {
        schema: oe_drawing::page::REQUEST_SCHEMA.into(),
        graph: text(&output_directory.join("graph.json")),
        svg: text(&output_directory.join("source.svg")),
        title: name.clone(),
        candidate: "pixel-route/1".into(),
        version: "pixel-synthesize developer r1".into(),
        source_note: "page image with observation files; no DXF".into(),
        status_note: "developer synthesis; not a route receipt and not an engineering acceptance".into(),
        output: text(&output_directory.join("page.html")),
    };
    let raster = oe_drawing::page::RasterBackground {
        png: &page_png,
        width_px: out.page_px.0,
        height_px: out.page_px.1,
        drawing_rectangle: out.drawing_rectangle,
    };
    let page_result = match oe_drawing::page::page_with_raster(&out.graph, &out.overlay_svg, &q, Some(&raster)) {
        Ok((html, result)) => {
            write("page.html", html.as_bytes())?;
            serde_json::json!({"status": "written", "result": result})
        }
        Err(e) => serde_json::json!({"status": "failed", "error": e.to_string()}),
    };
    let mut summary = out.summary;
    summary["check"] = check;
    summary["page_html"] = page_result;
    summary["outputs"] = serde_json::Value::Array(outputs);
    oe_core::write_new_bytes(
        &output_directory.join("synthesis-summary.json"),
        &serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(summary)
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
        DrawingCommand::Rules { request, output } => {
            crate::v2::require_execution(root, "drawing_rules")?;
            let q = oe_contracts::MdDraftingRuleRequest {
                schema: oe_drawing_check::drafting::REQUEST_SCHEMA.into(),
                discovery_request: text(&request),
                output: text(&output),
            };
            Ok(serde_json::to_value(oe_drawing_check::drafting::check_rules(
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
        DrawingCommand::PixelSynthesize {
            page,
            tiles,
            observations,
            output_directory,
            scale_note,
            page_name,
        } => pixel_synthesize(
            &page,
            &tiles,
            &observations,
            &output_directory,
            scale_note,
            page_name,
        ),
    }
}
