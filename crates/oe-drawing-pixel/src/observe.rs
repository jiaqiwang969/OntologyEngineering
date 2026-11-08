//! Observation inputs of the pixel route: the tile index, the observation files written outside the
//! engine by the orchestration scripts (page layout L0, dimensions L1A, other objects L1B, text
//! verification L1C), and the proposals they contain, converted to page pixel coordinates and tagged
//! with their provenance.
//!
//! Everything read here is untrusted data: missing or odd fields make an item be skipped and reported
//! in `warnings`, never a panic. Only identity mismatches are fatal: a tile image whose digest differs
//! from the index, or an observation whose `image_sha256` differs from the tile it names, since the
//! provenance of every proposal rests on them.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A point in page pixel coordinates (x right, y down, pixel centres at integers).
pub type P = (f64, f64);

/// Lower-case hex SHA-256 of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One tile of a page: a `tile_px × tile_px` crop of the page image at offset `(x, y)`.
#[derive(Clone, Debug, PartialEq)]
pub struct TileEntry {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub sha256: String,
}

/// The tiles of one page with the page and thumbnail sizes.
#[derive(Clone, Debug, PartialEq)]
pub struct PageTiles {
    pub page_px: (u32, u32),
    pub small_px: (u32, u32),
    pub page_sha256: Option<String>,
    pub tile_px: f64,
    pub tiles: Vec<TileEntry>,
}

impl PageTiles {
    pub fn tile(&self, name: &str) -> Option<&TileEntry> {
        self.tiles.iter().find(|t| t.name == name)
    }

    /// Verify the bytes of a tile image against the index. An unknown tile name is an error too.
    pub fn verify_tile_png(&self, name: &str, png: &[u8]) -> Result<()> {
        let entry = self
            .tile(name)
            .with_context(|| format!("pixel_tile_unknown:{name}"))?;
        ensure!(
            sha256_hex(png) == entry.sha256,
            "pixel_tile_identity:{name}"
        );
        Ok(())
    }

    /// Verify the page image against the index when the index records its digest.
    pub fn verify_page_png(&self, png: &[u8]) -> Result<()> {
        if let Some(expected) = &self.page_sha256 {
            ensure!(&sha256_hex(png) == expected, "pixel_page_identity");
        }
        Ok(())
    }
}

/// The tile index (`terra-parity.tiles/1`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TileIndex {
    pub schema: String,
    pub dpi: f64,
    pub tile_px: f64,
    pub stride_px: f64,
    pub pages: BTreeMap<String, PageTiles>,
}

pub fn load_tile_index(bytes: &[u8]) -> Result<TileIndex> {
    let v: Value = serde_json::from_slice(bytes).context("pixel_tile_index_json")?;
    let tile_px = num(&v["tile"]).filter(|t| *t > 0.0).unwrap_or(1600.0);
    let mut out = TileIndex {
        schema: v["schema"].as_str().unwrap_or_default().to_owned(),
        dpi: num(&v["dpi"]).filter(|d| *d > 0.0).unwrap_or(300.0),
        tile_px,
        stride_px: num(&v["stride"]).filter(|s| *s > 0.0).unwrap_or(1400.0),
        pages: BTreeMap::new(),
    };
    let pages = v["pages"].as_object().context("pixel_tile_index_pages")?;
    for (name, page) in pages {
        let size = |key: &str| -> Option<(u32, u32)> {
            let a = page.get(key)?.as_array()?;
            let w = num(a.first()?)?;
            let h = num(a.get(1)?)?;
            (w >= 1.0 && h >= 1.0 && w < 1e6 && h < 1e6).then_some((w as u32, h as u32))
        };
        let page_px = size("page_px").with_context(|| format!("pixel_tile_index_page_px:{name}"))?;
        let small_px = size("small_px").unwrap_or(page_px);
        let mut tiles = Vec::new();
        for t in page["tiles"].as_array().into_iter().flatten() {
            let (Some(n), Some(x), Some(y), Some(sha)) = (
                t["name"].as_str(),
                num(&t["x"]),
                num(&t["y"]),
                t["sha256"].as_str(),
            ) else {
                continue;
            };
            if n.is_empty() || sha.len() != 64 || x < 0.0 || y < 0.0 {
                continue;
            }
            tiles.push(TileEntry {
                name: n.to_owned(),
                x,
                y,
                sha256: sha.to_ascii_lowercase(),
            });
        }
        out.pages.insert(
            name.clone(),
            PageTiles {
                page_px,
                small_px,
                page_sha256: page["page_sha256"]
                    .as_str()
                    .filter(|s| s.len() == 64)
                    .map(str::to_ascii_lowercase),
                tile_px,
                tiles,
            },
        );
    }
    Ok(out)
}

/// One observation file (`terra-parity.observation/1` or `terra-parity.verification/1`).
#[derive(Clone, Debug)]
pub struct ObservationFile {
    /// Path as given by the caller (recorded in provenance, never opened here).
    pub path: String,
    pub schema: String,
    /// L0, L1A, L1B or L1C.
    pub level: String,
    pub model: String,
    pub image: String,
    pub image_sha256: String,
    pub prompt_sha256: String,
    pub page: String,
    /// Tile image name (`t_<x>_<y>.png`) for tile-level files, from the file name.
    pub tile: Option<String>,
    pub parse_error: Option<String>,
    pub parsed: Value,
    pub raw: Value,
}

pub fn load_observation(bytes: &[u8], path: &str) -> Result<ObservationFile> {
    let v: Value = serde_json::from_slice(bytes).with_context(|| format!("pixel_observation_json:{path}"))?;
    ensure!(v.is_object(), "pixel_observation_object:{path}");
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = base.strip_suffix(".json").unwrap_or(base);
    let (prefix, rest) = stem.split_once('-').unwrap_or((stem, ""));
    let level = v["level"]
        .as_str()
        .filter(|l| !l.is_empty())
        .unwrap_or(prefix)
        .to_owned();
    let tile = (rest.starts_with("t_")).then(|| format!("{rest}.png"));
    let text = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
    let image_sha256 = v["image_sha256"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let parsed = if v["parsed"].is_object() {
        v["parsed"].clone()
    } else {
        v.clone()
    };
    Ok(ObservationFile {
        path: path.to_owned(),
        schema: text("schema"),
        level,
        model: {
            let m = text("model");
            if m.is_empty() {
                v["response"]["model"].as_str().unwrap_or_default().to_owned()
            } else {
                m
            }
        },
        image: text("image"),
        image_sha256,
        prompt_sha256: text("prompt_sha256"),
        page: text("page"),
        tile,
        parse_error: v["parse_error"].as_str().map(str::to_owned),
        parsed,
        raw: v,
    })
}

// ------------------------------------------------------------------------------------------ proposals

/// Object category of a proposal (the observation list it came from).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Dimension,
    SurfaceTexture,
    GdtFrame,
    Datum,
    CentreLine,
    Hatch,
    Label,
    Note,
    Mark,
    BreakLine,
    /// One line of the technical requirements block (from L0).
    TechnicalLine,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Dimension => "dimension",
            Category::SurfaceTexture => "surface_texture",
            Category::GdtFrame => "gdt_frame",
            Category::Datum => "datum",
            Category::CentreLine => "centre_line",
            Category::Hatch => "hatch_region",
            Category::Label => "label",
            Category::Note => "note",
            Category::Mark => "mark",
            Category::BreakLine => "break_line",
            Category::TechnicalLine => "technical_requirement_line",
        }
    }
}

/// Proposed geometry of one member, page pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Segment(P, P),
    /// Arrow tip near `approx`, pointing along `along` (zero when unknown).
    Tip { approx: P, along: P },
    /// Text box `[x0, y0, x1, y1]`; `vertical` texts read bottom to top (rotation 90°).
    TextBox { bbox: [f64; 4], vertical: bool },
    /// A point feature (datum terminal).
    Point(P),
    Polyline(Vec<P>),
    Hatch { polygon: Vec<P>, angle_deg: f64, spacing_px: f64 },
}

impl Shape {
    /// Reference point: segment midpoint, tip, box centre, first polyline point, polygon centroid.
    pub fn reference(&self) -> P {
        match self {
            Shape::Segment(p, q) => ((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5),
            Shape::Tip { approx, .. } => *approx,
            Shape::TextBox { bbox, .. } => ((bbox[0] + bbox[2]) * 0.5, (bbox[1] + bbox[3]) * 0.5),
            Shape::Point(p) => *p,
            Shape::Polyline(v) => centroid(v),
            Shape::Hatch { polygon, .. } => centroid(polygon),
        }
    }

    /// Every point of the shape (box corners for boxes).
    pub fn points(&self) -> Vec<P> {
        match self {
            Shape::Segment(p, q) => vec![*p, *q],
            Shape::Tip { approx, .. } | Shape::Point(approx) => vec![*approx],
            Shape::TextBox { bbox, .. } => vec![(bbox[0], bbox[1]), (bbox[2], bbox[3])],
            Shape::Polyline(v) => v.clone(),
            Shape::Hatch { polygon, .. } => polygon.clone(),
        }
    }
}

fn centroid(v: &[P]) -> P {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let n = v.len() as f64;
    (
        v.iter().map(|p| p.0).sum::<f64>() / n,
        v.iter().map(|p| p.1).sum::<f64>() / n,
    )
}

/// One proposed member of an expression.
#[derive(Clone, Debug, PartialEq)]
pub struct ProposedMember {
    /// Member role of the graph vocabulary (`dimension_line`, `extension_line`, …).
    pub role: String,
    pub shape: Shape,
    /// Text of a text member (display form as observed).
    pub text: Option<String>,
}

/// Text verification of a proposal (L1C): the probability the model gave the proposed text among
/// systematic alternatives, and its top choice.
#[derive(Clone, Debug, PartialEq)]
pub struct Verification {
    pub item_text: String,
    pub p_proposal: f64,
    pub top_choice: String,
    pub predicate_p: Option<f64>,
    pub model: String,
    pub file: String,
}

impl Verification {
    /// Verified when the proposal is the top choice with probability at least 0.6.
    pub fn verified(&self) -> bool {
        self.p_proposal >= 0.6 && self.top_choice.replace(' ', "") == self.item_text.replace(' ', "")
    }
}

/// Where a proposal came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    pub tile: String,
    pub tile_sha256: String,
    pub level: String,
    pub model: String,
    pub prompt_sha256: String,
    pub file: String,
    pub item_index: usize,
    pub verification: Option<Verification>,
}

/// One proposed drawing object in page pixel coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub category: Category,
    /// Dimension kind, label role or empty.
    pub kind: String,
    /// Observed text (display form).
    pub text: String,
    pub centre: P,
    /// Distance from the proposal to the nearest edge of its tile, in pixels (large for page-level
    /// proposals).
    pub edge_distance: f64,
    pub members: Vec<ProposedMember>,
    pub provenance: Provenance,
    pub notes: Vec<String>,
}

/// One view region of the L0 layout, page pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct L0View {
    pub name: String,
    pub label_text: String,
    pub scale_note: String,
    pub bbox: [f64; 4],
}

/// The L0 page layout, page pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub file: String,
    pub image_sha256: String,
    pub model: String,
    pub frame: Option<[f64; 4]>,
    pub title_block: Option<[f64; 4]>,
    pub drawing_number: String,
    pub part_name: String,
    pub scale_note: Option<String>,
    pub technical_requirements: Option<([f64; 4], Vec<String>)>,
    pub views: Vec<L0View>,
    pub other_regions: Vec<(String, [f64; 4])>,
}

/// Record of one observation file as read.
#[derive(Clone, Debug, PartialEq)]
pub struct FileRecord {
    pub path: String,
    pub level: String,
    pub tile: Option<String>,
    pub model: String,
    pub image_sha256: String,
    pub prompt_sha256: String,
    pub parse_error: Option<String>,
    pub items: usize,
}

/// All proposals of one page with the layout and the records of what was read.
#[derive(Clone, Debug)]
pub struct PageObservations {
    pub page: String,
    pub page_px: (u32, u32),
    pub layout: Option<Layout>,
    pub proposals: Vec<Proposal>,
    pub files: Vec<FileRecord>,
    pub warnings: Vec<String>,
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

fn point(v: &Value) -> Option<P> {
    let a = v.as_array()?;
    if a.len() < 2 {
        return None;
    }
    Some((num(&a[0])?, num(&a[1])?))
}

fn bbox(v: &Value) -> Option<[f64; 4]> {
    let a = v.as_array()?;
    if a.len() != 4 {
        return None;
    }
    let b = [num(&a[0])?, num(&a[1])?, num(&a[2])?, num(&a[3])?];
    Some([b[0].min(b[2]), b[1].min(b[3]), b[0].max(b[2]), b[1].max(b[3])])
}

fn segment(v: &Value) -> Option<(P, P)> {
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    Some((point(&a[0])?, point(&a[1])?))
}

fn polyline(v: &Value) -> Option<Vec<P>> {
    let pts: Vec<P> = v.as_array()?.iter().filter_map(point).collect();
    (pts.len() >= 2).then_some(pts)
}

fn string(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Coordinate transform of one tile into the page.
struct TileFrame<'a> {
    entry: &'a TileEntry,
    tile_px: f64,
    page_px: (u32, u32),
}

impl TileFrame<'_> {
    fn to_page(&self, p: P) -> P {
        let x = (p.0 + self.entry.x).clamp(0.0, f64::from(self.page_px.0) - 1.0);
        let y = (p.1 + self.entry.y).clamp(0.0, f64::from(self.page_px.1) - 1.0);
        (x, y)
    }

    fn bbox(&self, b: [f64; 4]) -> [f64; 4] {
        let p = self.to_page((b[0], b[1]));
        let q = self.to_page((b[2], b[3]));
        [p.0, p.1, q.0, q.1]
    }

    /// Distance of a tile-local point from the nearest tile edge.
    fn edge_distance(&self, p: P) -> f64 {
        p.0.min(p.1).min(self.tile_px - p.0).min(self.tile_px - p.1).max(0.0)
    }

    fn shape(&self, s: &Shape) -> Shape {
        match s {
            Shape::Segment(p, q) => Shape::Segment(self.to_page(*p), self.to_page(*q)),
            Shape::Tip { approx, along } => Shape::Tip {
                approx: self.to_page(*approx),
                along: *along,
            },
            Shape::TextBox { bbox, vertical } => Shape::TextBox {
                bbox: self.bbox(*bbox),
                vertical: *vertical,
            },
            Shape::Point(p) => Shape::Point(self.to_page(*p)),
            Shape::Polyline(v) => Shape::Polyline(v.iter().map(|p| self.to_page(*p)).collect()),
            Shape::Hatch {
                polygon,
                angle_deg,
                spacing_px,
            } => Shape::Hatch {
                polygon: polygon.iter().map(|p| self.to_page(*p)).collect(),
                angle_deg: *angle_deg,
                spacing_px: *spacing_px,
            },
        }
    }
}

fn unit(v: P) -> P {
    let n = v.0.hypot(v.1);
    if n > 1e-9 { (v.0 / n, v.1 / n) } else { (0.0, 0.0) }
}

/// A direction within about 3° of an axis is that axis: observed endpoints are off by pixels, and
/// an arrow search along a slightly tilted axis drifts along the crossing extension line.
pub(crate) fn snap_axis(d: P) -> P {
    if d.0.abs() >= 0.9986 {
        (d.0.signum(), 0.0)
    } else if d.1.abs() >= 0.9986 {
        (0.0, d.1.signum())
    } else {
        d
    }
}

fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Direction of an arrow at `tip` of a dimension line or leader: away from the dimension line's
/// midpoint, or away from the nearest other vertex of the leader.
fn arrow_direction(tip: P, dimension_line: Option<(P, P)>, leader: &[P], extension: &[(P, P)], text_centre: Option<P>) -> P {
    if let Some((p, q)) = dimension_line {
        let m = ((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5);
        let d = unit((tip.0 - m.0, tip.1 - m.1));
        if d != (0.0, 0.0) {
            return snap_axis(d);
        }
    }
    let mut best: Option<(f64, P)> = None;
    for v in leader {
        let d = dist(*v, tip);
        if d > 3.0 && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, *v));
        }
    }
    if let Some((_, v)) = best {
        return snap_axis(unit((tip.0 - v.0, tip.1 - v.1)));
    }
    // Angular dimensions: the arrow runs perpendicular to the nearest extension line, away from the text.
    let mut nearest: Option<(f64, (P, P))> = None;
    for &(p, q) in extension {
        let d = dist(p, tip).min(dist(q, tip));
        if nearest.is_none_or(|(b, _)| d < b) {
            nearest = Some((d, (p, q)));
        }
    }
    if let (Some((_, (p, q))), Some(t)) = (nearest, text_centre) {
        let d = unit((q.0 - p.0, q.1 - p.1));
        let n = (-d.1, d.0);
        let away = (tip.0 - t.0, tip.1 - t.1);
        return if n.0 * away.0 + n.1 * away.1 >= 0.0 { n } else { (-n.0, -n.1) };
    }
    (0.0, 0.0)
}

/// Member role of an L1B label by its role word.
fn label_role(role: &str) -> (&'static str, &'static str) {
    match role {
        "view_name" => ("view_title", "view_name"),
        "callout_letter" | "section_letter" => ("view_letter", role_static(role)),
        "scale" => ("scale_text", "scale"),
        _ => ("note_text", "other"),
    }
}

fn role_static(role: &str) -> &'static str {
    match role {
        "callout_letter" => "callout_letter",
        "section_letter" => "section_letter",
        _ => "other",
    }
}

/// Convert the observation files of one page into proposals in page pixel coordinates.
///
/// `tile_pngs` are the tile images available for verification by name; every provided image must
/// match the index digest, and every tile-level observation must name a tile whose index digest
/// equals its own `image_sha256`.
pub fn page_observations(
    page: &str,
    tiles: &PageTiles,
    files: &[ObservationFile],
    tile_pngs: &BTreeMap<String, Vec<u8>>,
) -> Result<PageObservations> {
    for (name, png) in tile_pngs {
        tiles.verify_tile_png(name, png)?;
    }
    let mut out = PageObservations {
        page: page.to_owned(),
        page_px: tiles.page_px,
        layout: None,
        proposals: Vec::new(),
        files: Vec::new(),
        warnings: Vec::new(),
    };
    // Verifications by (tile, item index), read before the L1A files are converted.
    let mut verifications: BTreeMap<(String, usize), Verification> = BTreeMap::new();
    for f in files.iter().filter(|f| f.level == "L1C") {
        let Some(tile) = &f.tile else {
            out.warnings.push(format!("{}: verification without tile", f.path));
            continue;
        };
        let items = f.raw["items"].as_array().cloned().unwrap_or_default();
        let answers = f.raw["response"]["answers"].as_array().cloned().unwrap_or_default();
        let mut cursor = 0usize;
        let mut count = 0usize;
        for item in &items {
            let text = string(&item["text"]);
            let alternatives = item["alternatives"].as_array().map_or(0, Vec::len);
            let n_questions = item["n_questions"]
                .as_u64()
                .map_or(if alternatives > 0 { 2 } else { 1 }, |n| n as usize);
            let slice: Vec<&Value> = answers.iter().skip(cursor).take(n_questions).collect();
            cursor += n_questions;
            let Some(index) = item["index"].as_u64() else {
                continue;
            };
            let mut predicate_p = None;
            let mut choice: Option<(f64, String)> = None;
            for a in slice {
                match a["type"].as_str() {
                    Some("predicate") => predicate_p = num(&a["probability"]),
                    Some("choice") => {
                        let top = string(&a["choice"]);
                        let p = a["probabilities"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|x| string(&x["value"]).replace(' ', "") == text.replace(' ', ""))
                            .and_then(|x| num(&x["probability"]))
                            .unwrap_or(0.0);
                        choice = Some((p, top));
                    }
                    _ => {}
                }
            }
            let Some((p_proposal, top_choice)) = choice else {
                continue;
            };
            count += 1;
            verifications.insert(
                (tile.clone(), index as usize),
                Verification {
                    item_text: text,
                    p_proposal,
                    top_choice,
                    predicate_p,
                    model: f.model.clone(),
                    file: f.path.clone(),
                },
            );
        }
        out.files.push(FileRecord {
            path: f.path.clone(),
            level: f.level.clone(),
            tile: f.tile.clone(),
            model: f.model.clone(),
            image_sha256: f.image_sha256.clone(),
            prompt_sha256: f.prompt_sha256.clone(),
            parse_error: f.parse_error.clone(),
            items: count,
        });
    }
    for f in files.iter().filter(|f| f.level != "L1C") {
        let mut record = FileRecord {
            path: f.path.clone(),
            level: f.level.clone(),
            tile: f.tile.clone(),
            model: f.model.clone(),
            image_sha256: f.image_sha256.clone(),
            prompt_sha256: f.prompt_sha256.clone(),
            parse_error: f.parse_error.clone(),
            items: 0,
        };
        if f.parse_error.is_some() {
            out.warnings.push(format!("{}: parse_error, skipped", f.path));
            out.files.push(record);
            continue;
        }
        if f.level == "L0" {
            if out.layout.is_some() {
                out.warnings.push(format!("{}: second L0 layout ignored", f.path));
            } else {
                let (layout, proposals) = layout_from(f, tiles);
                record.items = proposals.len() + layout.views.len();
                out.proposals.extend(proposals);
                out.layout = Some(layout);
            }
            out.files.push(record);
            continue;
        }
        let Some(tile) = &f.tile else {
            out.warnings.push(format!("{}: tile-level observation without tile name", f.path));
            out.files.push(record);
            continue;
        };
        let Some(entry) = tiles.tile(tile) else {
            bail!("pixel_observation_tile_unknown:{}", f.path);
        };
        ensure!(
            f.image_sha256.is_empty() || f.image_sha256 == entry.sha256,
            "pixel_observation_identity:{}",
            f.path
        );
        let frame = TileFrame {
            entry,
            tile_px: tiles.tile_px,
            page_px: tiles.page_px,
        };
        let before = out.proposals.len();
        match f.level.as_str() {
            "L1A" => dimensions_from(f, &frame, &verifications, &mut out),
            "L1B" => objects_from(f, &frame, &mut out),
            other => out.warnings.push(format!("{}: unknown level {other}", f.path)),
        }
        record.items = out.proposals.len() - before;
        out.files.push(record);
    }
    Ok(out)
}

fn provenance(f: &ObservationFile, frame: &TileFrame, index: usize) -> Provenance {
    Provenance {
        tile: frame.entry.name.clone(),
        tile_sha256: frame.entry.sha256.clone(),
        level: f.level.clone(),
        model: f.model.clone(),
        prompt_sha256: f.prompt_sha256.clone(),
        file: f.path.clone(),
        item_index: index,
        verification: None,
    }
}

/// Assemble a proposal from tile-local members: centre from the text box or the member points, edge
/// distance from the tile-local points.
fn finish(
    category: Category,
    kind: &str,
    text: &str,
    members_local: Vec<ProposedMember>,
    frame: &TileFrame,
    provenance: Provenance,
    notes: Vec<String>,
) -> Option<Proposal> {
    if members_local.is_empty() {
        return None;
    }
    let mut pts: Vec<P> = Vec::new();
    for m in &members_local {
        pts.extend(m.shape.points());
    }
    let centre_local = members_local
        .iter()
        .find(|m| matches!(m.shape, Shape::TextBox { .. }))
        .map(|m| m.shape.reference())
        .unwrap_or_else(|| centroid(&pts));
    let edge_distance = pts
        .iter()
        .map(|p| frame.edge_distance(*p))
        .fold(f64::INFINITY, f64::min);
    let members = members_local
        .into_iter()
        .map(|m| ProposedMember {
            role: m.role,
            shape: frame.shape(&m.shape),
            text: m.text,
        })
        .collect();
    Some(Proposal {
        category,
        kind: kind.to_owned(),
        text: text.to_owned(),
        centre: frame.to_page(centre_local),
        edge_distance: if edge_distance.is_finite() { edge_distance } else { 0.0 },
        members,
        provenance,
        notes,
    })
}

fn dimensions_from(
    f: &ObservationFile,
    frame: &TileFrame,
    verifications: &BTreeMap<(String, usize), Verification>,
    out: &mut PageObservations,
) {
    let Some(dims) = f.parsed["dimensions"].as_array() else {
        out.warnings.push(format!("{}: no dimensions list", f.path));
        return;
    };
    for (i, d) in dims.iter().enumerate() {
        let text = string(&d["text"]);
        let dimension_line = segment(&d["dimension_line"]);
        let leader = polyline(&d["leader"]).unwrap_or_default();
        let mut kind = string(&d["kind"]);
        if !["linear", "diameter", "radius", "angular", "chamfer", "leader"].contains(&kind.as_str()) {
            kind = if dimension_line.is_some() { "linear".into() } else { "leader".into() };
        }
        let mut members = Vec::new();
        let mut notes = Vec::new();
        let text_box = bbox(&d["text_bbox"]);
        if let Some(b) = text_box {
            members.push(ProposedMember {
                role: "dimension_text".into(),
                shape: Shape::TextBox {
                    bbox: b,
                    vertical: d["text_orientation"].as_str() == Some("vertical"),
                },
                text: Some(text.clone()),
            });
        } else if !text.is_empty() {
            notes.push("text box missing in the observation".into());
        }
        if let Some((p, q)) = dimension_line {
            members.push(ProposedMember {
                role: "dimension_line".into(),
                shape: Shape::Segment(p, q),
                text: None,
            });
        }
        let extension: Vec<(P, P)> = d["extension_lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(segment)
            .collect();
        for &(p, q) in &extension {
            members.push(ProposedMember {
                role: "extension_line".into(),
                shape: Shape::Segment(p, q),
                text: None,
            });
        }
        for w in leader.windows(2) {
            members.push(ProposedMember {
                role: "leader_line".into(),
                shape: Shape::Segment(w[0], w[1]),
                text: None,
            });
        }
        let text_centre = text_box.map(|b| ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5));
        for tip in d["arrowheads"].as_array().into_iter().flatten().filter_map(point) {
            members.push(ProposedMember {
                role: "arrowhead".into(),
                shape: Shape::Tip {
                    approx: tip,
                    along: arrow_direction(tip, dimension_line, &leader, &extension, text_centre),
                },
                text: None,
            });
        }
        if let Some(n) = d["note"].as_str().filter(|n| !n.trim().is_empty()) {
            notes.push(format!("observer note: {}", n.trim()));
        }
        let mut prov = provenance(f, frame, i);
        prov.verification = verifications.get(&(frame.entry.name.clone(), i)).cloned();
        if let Some(v) = &prov.verification
            && v.item_text.replace(' ', "") != text.replace(' ', "")
        {
            notes.push(format!("verification item text differs: {}", v.item_text));
        }
        match finish(Category::Dimension, &kind, &text, members, frame, prov, notes) {
            Some(p) => out.proposals.push(p),
            None => out.warnings.push(format!("{}#{i}: dimension without usable geometry", f.path)),
        }
    }
}

/// Legs of a surface texture symbol from its tip and bounding box: the tip sits on one edge of the
/// box; both legs leave it at 60° to that edge towards the far side, the long leg twice the short one.
/// Both chiralities are returned; the refinement keeps the better supported pair.
fn roughness_legs(tip: P, b: [f64; 4]) -> Vec<[(P, P); 2]> {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    // Edge the tip sits on: distances to bottom, top, left, right.
    let d = [(b[3] - tip.1).abs(), (tip.1 - b[1]).abs(), (tip.0 - b[0]).abs(), (b[2] - tip.0).abs()];
    let edge = (0..4).min_by(|&a, &c| d[a].total_cmp(&d[c])).unwrap_or(0);
    // Up: away from the tip edge; right: perpendicular (both chiralities below).
    let (up, extent): (P, f64) = match edge {
        0 => ((0.0, -1.0), h),
        1 => ((0.0, 1.0), h),
        2 => ((1.0, 0.0), w),
        _ => ((-1.0, 0.0), w),
    };
    let right = (-up.1, up.0);
    let (s30, c30) = (0.5f64, 0.866_025_4f64);
    let long = extent / c30;
    let short = long * 0.5;
    let leg = |side: f64, len: f64| -> (P, P) {
        let dir = (up.0 * c30 + right.0 * s30 * side, up.1 * c30 + right.1 * s30 * side);
        (tip, (tip.0 + dir.0 * len, tip.1 + dir.1 * len))
    };
    vec![[leg(-1.0, short), leg(1.0, long)], [leg(1.0, short), leg(-1.0, long)]]
}

fn objects_from(f: &ObservationFile, frame: &TileFrame, out: &mut PageObservations) {
    let p = &f.parsed;
    let list = |key: &str| -> Vec<Value> { p[key].as_array().cloned().unwrap_or_default() };
    for (i, s) in list("surface_texture").iter().enumerate() {
        let value = string(&s["value"]);
        let (Some(b), Some(tip)) = (bbox(&s["symbol_bbox"]), point(&s["tip"])) else {
            out.warnings.push(format!("{}#surface_texture:{i}: box or tip missing", f.path));
            continue;
        };
        let mut members = Vec::new();
        for (k, legs) in roughness_legs(tip, b).into_iter().enumerate() {
            for (j, (a, c)) in legs.into_iter().enumerate() {
                members.push(ProposedMember {
                    role: format!("roughness_symbol#{k}{j}"),
                    shape: Shape::Segment(a, c),
                    text: None,
                });
            }
        }
        // The value sits in the open half of the symbol, away from the tip.
        let (w, h) = (b[2] - b[0], b[3] - b[1]);
        let value_box = if (b[3] - tip.1).abs() <= h * 0.25 {
            [b[0], b[1], b[2], b[1] + h * 0.55]
        } else if (tip.1 - b[1]).abs() <= h * 0.25 {
            [b[0], b[3] - h * 0.55, b[2], b[3]]
        } else if (tip.0 - b[0]).abs() <= w * 0.25 {
            [b[2] - w * 0.55, b[1], b[2], b[3]]
        } else {
            [b[0], b[1], b[0] + w * 0.55, b[3]]
        };
        if !value.is_empty() {
            members.push(ProposedMember {
                role: "roughness_value".into(),
                shape: Shape::TextBox {
                    bbox: value_box,
                    vertical: false,
                },
                text: Some(value.clone()),
            });
        }
        let notes = vec![format!("attached_to: {}", string(&s["attached_to"]))];
        if let Some(pr) = finish(Category::SurfaceTexture, "", &value, members, frame, provenance(f, frame, i), notes) {
            out.proposals.push(pr);
        }
    }
    for (i, g) in list("gdt_frames").iter().enumerate() {
        let cells: Vec<String> = g["cells"]
            .as_array()
            .into_iter()
            .flatten()
            .map(string)
            .collect();
        let Some(b) = bbox(&g["bbox"]) else {
            out.warnings.push(format!("{}#gdt_frames:{i}: box missing", f.path));
            continue;
        };
        let mut members = Vec::new();
        let corners = [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])];
        for k in 0..4 {
            members.push(ProposedMember {
                role: "frame_cell".into(),
                shape: Shape::Segment(corners[k], corners[(k + 1) % 4]),
                text: None,
            });
        }
        // Cells: the first (the characteristic symbol) is square, the rest share the remaining width.
        let (w, h) = (b[2] - b[0], b[3] - b[1]);
        let n = cells.len();
        if n > 0 {
            let first = h.min(w);
            let rest = ((w - first) / (n.saturating_sub(1)).max(1) as f64).max(1.0);
            let mut x = b[0];
            for (k, c) in cells.iter().enumerate() {
                let cw = if k == 0 { first } else { rest };
                let (role, keep) = match k {
                    0 => ("symbol_glyph", !c.is_empty()),
                    1 => ("tolerance_value", !c.is_empty()),
                    _ => ("datum_reference", !c.is_empty()),
                };
                if keep && c != "?" {
                    members.push(ProposedMember {
                        role: role.into(),
                        shape: Shape::TextBox {
                            bbox: [x, b[1], x + cw, b[3]],
                            vertical: false,
                        },
                        text: Some(c.clone()),
                    });
                }
                x += cw;
            }
        }
        let leader = polyline(&g["leader"]).unwrap_or_default();
        for w in leader.windows(2) {
            members.push(ProposedMember {
                role: "leader_line".into(),
                shape: Shape::Segment(w[0], w[1]),
                text: None,
            });
        }
        if let Some(tip) = point(&g["arrow_tip"]) {
            members.push(ProposedMember {
                role: "arrowhead".into(),
                shape: Shape::Tip {
                    approx: tip,
                    along: arrow_direction(tip, None, &leader, &[], None),
                },
                text: None,
            });
        }
        let text = cells.join(" | ");
        if let Some(pr) = finish(Category::GdtFrame, "", &text, members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        }
    }
    for (i, d) in list("datums").iter().enumerate() {
        let letter = string(&d["letter"]);
        let Some(b) = bbox(&d["bbox"]) else {
            out.warnings.push(format!("{}#datums:{i}: box missing", f.path));
            continue;
        };
        let mut members = Vec::new();
        if !letter.is_empty() {
            let (w, h) = (b[2] - b[0], b[3] - b[1]);
            members.push(ProposedMember {
                role: "datum_letter".into(),
                shape: Shape::TextBox {
                    bbox: [b[0] + w * 0.2, b[1] + h * 0.2, b[2] - w * 0.2, b[3] - h * 0.2],
                    vertical: false,
                },
                text: Some(letter.clone()),
            });
        }
        if let Some((p, q)) = segment(&d["base_line"]) {
            members.push(ProposedMember {
                role: "datum_frame".into(),
                shape: Shape::Segment(p, q),
                text: None,
            });
            members.push(ProposedMember {
                role: "datum_terminal".into(),
                shape: Shape::Point(((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5)),
                text: None,
            });
        }
        if let Some(pr) = finish(Category::Datum, "", &letter, members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        }
    }
    for (i, c) in list("centre_lines").iter().enumerate() {
        let members: Vec<ProposedMember> = c["segments"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(segment)
            .map(|(p, q)| ProposedMember {
                role: "center_line".into(),
                shape: Shape::Segment(p, q),
                text: None,
            })
            .collect();
        if let Some(pr) = finish(Category::CentreLine, "", "", members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        } else {
            out.warnings.push(format!("{}#centre_lines:{i}: no segments", f.path));
        }
    }
    for (i, h) in list("hatch_regions").iter().enumerate() {
        let Some(polygon) = polyline(&h["polygon"]).filter(|p| p.len() >= 3) else {
            out.warnings.push(format!("{}#hatch_regions:{i}: polygon missing", f.path));
            continue;
        };
        let members = vec![ProposedMember {
            role: "hatch_fill".into(),
            shape: Shape::Hatch {
                polygon,
                angle_deg: num(&h["angle_deg"]).unwrap_or(45.0),
                spacing_px: num(&h["spacing_px"]).filter(|s| *s > 0.0).unwrap_or(0.0),
            },
            text: None,
        }];
        if let Some(pr) = finish(Category::Hatch, "", "", members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        }
    }
    for (i, l) in list("labels").iter().enumerate() {
        let text = string(&l["text"]);
        let Some(b) = bbox(&l["bbox"]) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let (role, kind) = label_role(&string(&l["role"]));
        let members = vec![ProposedMember {
            role: role.into(),
            shape: Shape::TextBox {
                bbox: b,
                vertical: false,
            },
            text: Some(text.clone()),
        }];
        if let Some(pr) = finish(Category::Label, kind, &text, members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        }
    }
    for (i, n) in list("notes").iter().enumerate() {
        let text = string(&n["text"]);
        let Some(b) = bbox(&n["bbox"]) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let mut members = vec![ProposedMember {
            role: "note_text".into(),
            shape: Shape::TextBox {
                bbox: b,
                vertical: false,
            },
            text: Some(text.clone()),
        }];
        let leader = polyline(&n["leader"]).unwrap_or_default();
        for w in leader.windows(2) {
            members.push(ProposedMember {
                role: "leader_line".into(),
                shape: Shape::Segment(w[0], w[1]),
                text: None,
            });
        }
        if let Some(tip) = point(&n["arrow_tip"]) {
            members.push(ProposedMember {
                role: "arrowhead".into(),
                shape: Shape::Tip {
                    approx: tip,
                    along: arrow_direction(tip, None, &leader, &[], None),
                },
                text: None,
            });
        }
        if let Some(pr) = finish(Category::Note, "", &text, members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        }
    }
    for (i, m) in list("marks").iter().enumerate() {
        let text = string(&m["text"]);
        let Some(b) = bbox(&m["bbox"]) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let members = vec![ProposedMember {
            role: "special_mark".into(),
            shape: Shape::TextBox {
                bbox: b,
                vertical: false,
            },
            text: Some(text.clone()),
        }];
        if let Some(pr) = finish(Category::Mark, "", &text, members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        }
    }
    for (i, b) in list("break_lines").iter().enumerate() {
        let Some(pl) = polyline(&b["polyline"]) else {
            out.warnings.push(format!("{}#break_lines:{i}: polyline missing", f.path));
            continue;
        };
        let members = vec![ProposedMember {
            role: "break_curve".into(),
            shape: Shape::Polyline(pl),
            text: None,
        }];
        if let Some(pr) = finish(Category::BreakLine, "", "", members, frame, provenance(f, frame, i), vec![]) {
            out.proposals.push(pr);
        }
    }
}

/// The L0 layout in page pixels, and the technical-requirement lines as page-level proposals.
fn layout_from(f: &ObservationFile, tiles: &PageTiles) -> (Layout, Vec<Proposal>) {
    let p = &f.parsed;
    let sx = f64::from(tiles.page_px.0) / f64::from(tiles.small_px.0.max(1));
    let sy = f64::from(tiles.page_px.1) / f64::from(tiles.small_px.1.max(1));
    let scale = |b: [f64; 4]| -> [f64; 4] {
        let w = f64::from(tiles.page_px.0) - 1.0;
        let h = f64::from(tiles.page_px.1) - 1.0;
        [
            (b[0] * sx).clamp(0.0, w),
            (b[1] * sy).clamp(0.0, h),
            (b[2] * sx).clamp(0.0, w),
            (b[3] * sy).clamp(0.0, h),
        ]
    };
    let views = p["views"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let b = bbox(&v["bbox"])?;
            Some(L0View {
                name: string(&v["name"]),
                label_text: string(&v["label_text"]),
                scale_note: string(&v["scale_note"]),
                bbox: scale(b),
            })
        })
        .filter(|v| !v.name.is_empty() || !v.label_text.is_empty())
        .collect();
    let technical = bbox(&p["technical_requirements"]["bbox"]).map(|b| {
        let lines: Vec<String> = p["technical_requirements"]["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .map(string)
            .filter(|s| !s.is_empty())
            .collect();
        (scale(b), lines)
    });
    let scale_note = Some(string(&p["scale_in_title_block"])).filter(|s| !s.is_empty());
    let layout = Layout {
        file: f.path.clone(),
        image_sha256: f.image_sha256.clone(),
        model: f.model.clone(),
        frame: bbox(&p["frame"]).map(scale),
        title_block: bbox(&p["title_block"]).map(scale),
        drawing_number: string(&p["drawing_number"]),
        part_name: string(&p["part_name"]),
        scale_note,
        technical_requirements: technical.clone(),
        views,
        other_regions: p["other_regions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| Some((string(&r["kind"]), scale(bbox(&r["bbox"])?))))
            .collect(),
    };
    let mut proposals = Vec::new();
    if let Some((b, lines)) = technical {
        let n = lines.len().max(1) as f64;
        let row = (b[3] - b[1]) / n;
        for (i, line) in lines.iter().enumerate() {
            let y0 = b[1] + row * i as f64;
            proposals.push(Proposal {
                category: Category::TechnicalLine,
                kind: String::new(),
                text: line.clone(),
                centre: ((b[0] + b[2]) * 0.5, y0 + row * 0.5),
                edge_distance: f64::MAX,
                members: vec![ProposedMember {
                    role: "note_text".into(),
                    shape: Shape::TextBox {
                        bbox: [b[0], y0, b[2], y0 + row],
                        vertical: false,
                    },
                    text: Some(line.clone()),
                }],
                provenance: Provenance {
                    tile: "page-small".into(),
                    tile_sha256: f.image_sha256.clone(),
                    level: f.level.clone(),
                    model: f.model.clone(),
                    prompt_sha256: f.prompt_sha256.clone(),
                    file: f.path.clone(),
                    item_index: i,
                    verification: None,
                },
                notes: vec!["row box split evenly from the technical-requirements block".into()],
            });
        }
    }
    (layout, proposals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tiles() -> PageTiles {
        PageTiles {
            page_px: (3000, 2000),
            small_px: (1500, 1000),
            page_sha256: None,
            tile_px: 1600.0,
            tiles: vec![
                TileEntry { name: "t_0_0.png".into(), x: 0.0, y: 0.0, sha256: "a".repeat(64) },
                TileEntry { name: "t_1400_0.png".into(), x: 1400.0, y: 0.0, sha256: "b".repeat(64) },
            ],
        }
    }

    fn file(level: &str, tile: &str, sha: &str, parsed: Value) -> ObservationFile {
        let path = format!("obs/{level}-{}.json", tile.trim_end_matches(".png"));
        let v = json!({"schema": "terra-parity.observation/1", "model": "m", "image": format!("work/p/tiles/{tile}"),
            "image_sha256": sha, "prompt_sha256": "p", "parsed": parsed, "parse_error": null, "page": "p", "level": level});
        load_observation(&serde_json::to_vec(&v).unwrap(), &path).unwrap()
    }

    #[test]
    fn dimensions_are_moved_into_page_coordinates_with_provenance() {
        let a = file("L1A", "t_1400_0.png", &"b".repeat(64), json!({"dimensions": [
            {"text": "Ø40", "kind": "diameter", "text_bbox": [100, 100, 160, 130], "text_orientation": "horizontal",
             "dimension_line": [[50, 140], [250, 140]], "arrowheads": [[50, 140], [250, 140]],
             "extension_lines": [[[50, 100], [50, 150]], [[250, 100], [250, 150]]], "leader": []},
            {"text": "odd", "kind": "linear", "text_bbox": "nope", "dimension_line": null, "arrowheads": [], "extension_lines": [], "leader": []}
        ]}));
        let obs = page_observations("p", &tiles(), &[a], &BTreeMap::new()).unwrap();
        assert_eq!(obs.proposals.len(), 1, "{:?}", obs.warnings);
        let p = &obs.proposals[0];
        assert_eq!(p.category, Category::Dimension);
        assert_eq!(p.kind, "diameter");
        assert_eq!(p.centre, (1530.0, 115.0));
        assert_eq!(p.members.len(), 6);
        assert_eq!(p.provenance.tile, "t_1400_0.png");
        assert_eq!(p.provenance.item_index, 0);
        let tips: Vec<&ProposedMember> = p.members.iter().filter(|m| m.role == "arrowhead").collect();
        assert!(matches!(tips[0].shape, Shape::Tip { approx: (1450.0, 140.0), along } if along.0 < -0.99));
        assert!(matches!(tips[1].shape, Shape::Tip { approx: (1650.0, 140.0), along } if along.0 > 0.99));
        assert!((p.edge_distance - 50.0).abs() < 1e-9);
        assert_eq!(obs.warnings.len(), 1);
    }

    #[test]
    fn identity_mismatch_is_fatal_and_verification_is_attached_by_index() {
        let bad = file("L1A", "t_0_0.png", &"c".repeat(64), json!({"dimensions": []}));
        assert!(page_observations("p", &tiles(), &[bad], &BTreeMap::new()).is_err());
        let a = file("L1A", "t_0_0.png", &"a".repeat(64), json!({"dimensions": [
            {"text": "30", "kind": "linear", "text_bbox": [10, 10, 40, 20], "dimension_line": [[0, 30], [100, 30]], "arrowheads": [], "extension_lines": [], "leader": []},
            {"text": "Ø19 +0.05/+0.02", "kind": "diameter", "text_bbox": [200, 10, 300, 20], "dimension_line": [[200, 30], [300, 30]], "arrowheads": [], "extension_lines": [], "leader": []}
        ]}));
        let c = json!({"schema": "terra-parity.verification/1", "ok": true, "level": "L1C", "model": "luna", "image_sha256": "a".repeat(64),
            "items": [{"index": 1, "text": "Ø19 +0.05/+0.02", "alternatives": ["Ø19 +0.05/+0.03"], "n_questions": 2}],
            "response": {"answers": [{"type": "predicate", "probability": 0.1},
                {"type": "choice", "choice": "Ø19 +0.05/+0.02", "probabilities": [{"value": "Ø19 +0.05/+0.02", "probability": 0.7}, {"value": "Ø19 +0.05/+0.03", "probability": 0.3}]}]}});
        let c = load_observation(&serde_json::to_vec(&c).unwrap(), "obs/L1C-t_0_0.json").unwrap();
        let obs = page_observations("p", &tiles(), &[a, c], &BTreeMap::new()).unwrap();
        assert!(obs.proposals[0].provenance.verification.is_none());
        let v = obs.proposals[1].provenance.verification.as_ref().unwrap();
        assert!((v.p_proposal - 0.7).abs() < 1e-9 && v.verified());
        assert_eq!(v.predicate_p, Some(0.1));
    }

    #[test]
    fn layout_scales_from_the_thumbnail_and_tile_png_digests_are_checked() {
        let l0 = json!({"schema": "x", "level": "L0", "model": "m", "image_sha256": "", "prompt_sha256": "", "parsed": {
            "frame": [10, 10, 1400, 900], "scale_in_title_block": "1:2",
            "technical_requirements": {"bbox": [1000, 500, 1400, 600], "lines": ["a", "b"]},
            "views": [{"name": "主视图", "label_text": "", "scale_note": "", "bbox": [100, 100, 700, 500]}]}});
        let l0 = load_observation(&serde_json::to_vec(&l0).unwrap(), "obs/L0-page.json").unwrap();
        assert_eq!(l0.level, "L0");
        assert!(l0.tile.is_none());
        let obs = page_observations("p", &tiles(), &[l0], &BTreeMap::new()).unwrap();
        let layout = obs.layout.unwrap();
        assert_eq!(layout.frame, Some([20.0, 20.0, 2800.0, 1800.0]));
        assert_eq!(layout.views[0].bbox, [200.0, 200.0, 1400.0, 1000.0]);
        assert_eq!(layout.scale_note.as_deref(), Some("1:2"));
        assert_eq!(obs.proposals.len(), 2);
        assert_eq!(obs.proposals[1].category, Category::TechnicalLine);
        let mut pngs = BTreeMap::new();
        pngs.insert("t_0_0.png".to_owned(), b"not the tile".to_vec());
        assert!(page_observations("p", &tiles(), &[], &pngs).is_err());
    }
}
