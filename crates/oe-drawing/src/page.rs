//! Presentation only: the review page of a frozen expression graph over the neutral rendering of its source.
//! No classification is performed here. The graph and the rendering are embedded unchanged; the page says that
//! no independent reference is embedded, since classification comes from the source bytes only.
//! A raster image of the page (PNG) may be shown under the rendering, aligned to its drawing coordinates; it is
//! an HTML image outside the SVG, and without one the page bytes are exactly those of the plain page.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use quick_xml::events::Event;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::Path;

pub const SCHEMA: &str = "oev2.drawing-review-page/1";
pub const REQUEST_SCHEMA: &str = "oev2.drawing-review-page-request/1";
const TEMPLATE: &str = include_str!("../assets/expression-review.html");
/// Bound on the page's inputs, and on the page itself when it carries a raster: the route's readers and the
/// browser check apply the same 64 MiB. An A2 sheet at 300 dpi (about 4961 × 7016 px) is a PNG of some 5–8 MB,
/// 7–11 MB once base64-encoded, well within it; the bound is not raised here.
const LIMIT: usize = 64 * 1024 * 1024;
/// The raster token with its line break: removed entirely without a raster, so the plain page keeps its bytes.
const RASTER_TOKEN: &str = "@@RASTER@@\n";
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
/// The raster layer: the image under the rendering and the script that keeps it on the rendering's viewBox
/// mapping and adds the show/hide option. It is HTML beside the SVG, never inside it, so the refusal rules of
/// the rendering are untouched. Display only: nothing is read from the image.
const RASTER_BLOCK: &str = r##"<img id="raster" alt="像素底图" width="@@W@@" height="@@H@@" draggable="false" data-sha256="@@SHA@@" data-drawing-x0="@@X0@@" data-drawing-y0="@@Y0@@" data-drawing-x1="@@X1@@" data-drawing-y1="@@Y1@@" data-svg-x="@@SX@@" data-svg-y="@@SY@@" data-svg-width="@@SW@@" data-svg-height="@@SH@@" style="position:absolute;left:0;top:0;z-index:-1;max-width:none;transform-origin:0 0;pointer-events:none;user-select:none" src="data:image/png;base64,@@PNG@@"><script>
'use strict';
// The raster page image lies under the vector rendering, outside the SVG. Its rectangle (data-svg-*, in the rendering's
// user units) follows the viewBox mapping of the rendering (xMidYMid meet, as the page's own pan and zoom assume) on every
// view change and resize; the white background of the rendering is lifted while the raster shows.
document.addEventListener('DOMContentLoaded',()=>{
 const img=document.getElementById('raster'),canvas=document.getElementById('canvas'),svg=canvas.querySelector('svg'),d=img.dataset,rect=[d.svgX,d.svgY,d.svgWidth,d.svgHeight].map(Number),px=Number(img.getAttribute('width')),py=Number(img.getAttribute('height'));
 canvas.style.isolation='isolate';
 const background=[...svg.children].filter(n=>n.tagName==='rect'&&!n.hasAttribute('data-entity')&&/^(white|#fff|#ffffff)$/i.test(n.getAttribute('fill')||'')).map(n=>[n,n.getAttribute('fill')]);
 function place(){const v=svg.getAttribute('viewBox').split(/\s+/).map(Number),b=svg.getBoundingClientRect(),c=canvas.getBoundingClientRect();if(!(b.width>0&&b.height>0))return;const s=Math.min(b.width/v[2],b.height/v[3]),ox=b.left-c.left-canvas.clientLeft+(b.width-v[2]*s)/2-v[0]*s,oy=b.top-c.top-canvas.clientTop+(b.height-v[3]*s)/2-v[1]*s;img.style.transform=`matrix(${s*rect[2]/px},0,0,${s*rect[3]/py},${ox+rect[0]*s},${oy+rect[1]*s})`}
 function show(on){img.hidden=!on;for(const [n,fill] of background)n.setAttribute('fill',on?'none':fill);if(on)place()}
 const option=document.createElement('label');option.className='option';option.innerHTML='<input type="checkbox" id="rasterShown" checked>显示像素底图（页面光栅在矢量之下）';document.getElementById('dimOthers').closest('label').after(option);option.querySelector('input').onchange=e=>show(e.target.checked);
 new MutationObserver(place).observe(svg,{attributes:true,attributeFilter:['viewBox']});if(window.ResizeObserver)new ResizeObserver(place).observe(canvas);window.addEventListener('resize',place);
 document.getElementById('provenance').textContent+=`\n像素底图 SHA-256：${d.sha256}\n像素底图：${px}×${py} px，图纸坐标 [${d.drawingX0}, ${d.drawingY0}] – [${d.drawingX1}, ${d.drawingY1}]`;
 if(window.expressionReview)window.expressionReview.raster={place,get shown(){return !img.hidden},drawingRectangle:[d.drawingX0,d.drawingY0,d.drawingX1,d.drawingY1].map(Number),svgRectangle:rect,pixels:[px,py],sha256:d.sha256};
 show(true);
});
</script>"##;

pub struct ReviewPageRenderer<'a> {
    pub root: &'a SkillRoot,
}
impl MdReviewPagePort for ReviewPageRenderer<'_> {
    fn render_page(
        &self,
        input: &MdReviewPageRequest,
    ) -> std::result::Result<MdReviewPage, FailureRecord> {
        render(self.root, input)
            .map_err(|e| oe_engineering::artifacts::failure("drawing_review_page", e))
    }
}

/// Identities and active content of a neutral rendering.
pub struct Rendering {
    pub entities: BTreeSet<String>,
    pub active: Vec<String>,
}

/// Every data-entity identity of an SVG, and any script, foreign object, image, event attribute or link in it.
pub fn rendering(svg: &[u8]) -> Result<Rendering> {
    let mut reader = quick_xml::Reader::from_reader(svg);
    let mut out = Rendering {
        entities: BTreeSet::new(),
        active: Vec::new(),
    };
    let mut buffer = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buffer)
            .context("review_page_svg_parse")?
        {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.local_name();
                let name = String::from_utf8_lossy(name.as_ref()).into_owned();
                if ["script", "foreignObject", "image"].contains(&name.as_str()) {
                    out.active.push(format!("element:{name}"));
                }
                for a in e.attributes() {
                    let a = a.context("review_page_svg_attribute")?;
                    let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
                    let local = String::from_utf8_lossy(a.key.local_name().as_ref()).into_owned();
                    if key.starts_with("on") || local == "href" {
                        out.active.push(format!("attribute:{key}"));
                    }
                    if key == "data-entity" {
                        let v = a
                            .unescape_value()
                            .context("review_page_svg_attribute_value")?;
                        if !v.is_empty() {
                            out.entities.insert(v.into_owned());
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(out)
}

/// A raster image of the drawing page, shown under the vector rendering.
#[derive(Clone, Copy, Debug)]
pub struct RasterBackground<'a> {
    /// PNG bytes; the IHDR dimensions must be `width_px` × `height_px`.
    pub png: &'a [u8],
    pub width_px: u32,
    pub height_px: u32,
    /// The drawing-coordinate rectangle the image covers, `[x_min, y_min, x_max, y_max]` with y up: the same
    /// coordinates as the rendering's `data-drawing-origin-x` and `data-drawing-top-y`.
    pub drawing_rectangle: [f64; 4],
}

/// The coordinate frame of a rendering: its viewBox and the drawing coordinates of its origin and top edge.
struct Frame {
    view_box: [f64; 4],
    origin_x: f64,
    top_y: f64,
}

/// The frame of the root `svg` element; the page's script reads the same three attributes.
fn frame(svg: &[u8]) -> Result<Frame> {
    let mut reader = quick_xml::Reader::from_reader(svg);
    let mut buffer = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buffer)
            .context("review_page_svg_parse")?
        {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"svg" => {
                let (mut view_box, mut origin_x, mut top_y) = (None, None, None);
                for a in e.attributes() {
                    let a = a.context("review_page_svg_attribute")?;
                    let v = a
                        .unescape_value()
                        .context("review_page_svg_attribute_value")?;
                    match a.key.as_ref() {
                        b"viewBox" => {
                            let numbers: Vec<f64> = v
                                .split_whitespace()
                                .filter_map(|x| x.parse().ok())
                                .collect();
                            view_box = <[f64; 4]>::try_from(numbers).ok();
                        }
                        b"data-drawing-origin-x" => origin_x = v.parse::<f64>().ok(),
                        b"data-drawing-top-y" => top_y = v.parse::<f64>().ok(),
                        _ => {}
                    }
                }
                let (Some(view_box), Some(origin_x), Some(top_y)) = (view_box, origin_x, top_y)
                else {
                    anyhow::bail!("review_page_svg_frame");
                };
                ensure!(
                    view_box.iter().all(|x| x.is_finite())
                        && view_box[2] > 0.
                        && view_box[3] > 0.
                        && origin_x.is_finite()
                        && top_y.is_finite(),
                    "review_page_svg_frame"
                );
                return Ok(Frame {
                    view_box,
                    origin_x,
                    top_y,
                });
            }
            Event::Eof => anyhow::bail!("review_page_svg_frame"),
            _ => {}
        }
        buffer.clear();
    }
}

/// Width and height from the IHDR chunk, which a PNG starts with.
fn png_dimensions(png: &[u8]) -> Result<(u32, u32)> {
    ensure!(
        png.len() >= 33
            && png[..8] == PNG_SIGNATURE
            && png[8..16] == [0, 0, 0, 13, b'I', b'H', b'D', b'R'],
        "review_page_raster_png"
    );
    let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    Ok((be(16), be(20)))
}

/// Standard base64 with padding (RFC 4648 §4) for the image data URI.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n =
            chunk.iter().fold(0u32, |acc, b| acc << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The raster layer of the page and its facts for the payload. The image must be the PNG it is declared to be,
/// cover a proper rectangle that meets the rendering, and leave the page within `LIMIT`.
fn raster_layer(r: &RasterBackground, svg: &str) -> Result<(String, Value)> {
    let (w, h) = png_dimensions(r.png)?;
    ensure!(
        w > 0 && h > 0 && w == r.width_px && h == r.height_px,
        "review_page_raster_dimensions:{w}x{h}"
    );
    let [x0, y0, x1, y1] = r.drawing_rectangle;
    ensure!(
        r.drawing_rectangle.iter().all(|v| v.is_finite()) && x1 > x0 && y1 > y0,
        "review_page_raster_rectangle"
    );
    let f = frame(svg.as_bytes())?;
    // The image rectangle in the rendering's user units: x right of the origin, y down from the top edge.
    let (sx, sy, sw, sh) = (x0 - f.origin_x, f.top_y - y1, x1 - x0, y1 - y0);
    let [vx, vy, vw, vh] = f.view_box;
    ensure!(
        sx < vx + vw && sx + sw > vx && sy < vy + vh && sy + sh > vy,
        "review_page_raster_outside_rendering"
    );
    ensure!(
        r.png.len().div_ceil(3) * 4 + TEMPLATE.len() + svg.len() <= LIMIT,
        "review_page_size_budget"
    );
    let sha = sha256(r.png);
    let block = RASTER_BLOCK
        .replace("@@W@@", &w.to_string())
        .replace("@@H@@", &h.to_string())
        .replace("@@SHA@@", &sha)
        .replace("@@X0@@", &x0.to_string())
        .replace("@@Y0@@", &y0.to_string())
        .replace("@@X1@@", &x1.to_string())
        .replace("@@Y1@@", &y1.to_string())
        .replace("@@SX@@", &sx.to_string())
        .replace("@@SY@@", &sy.to_string())
        .replace("@@SW@@", &sw.to_string())
        .replace("@@SH@@", &sh.to_string())
        .replace("@@PNG@@", &base64(r.png));
    let facts = json!({"sha256":sha,"bytes":r.png.len(),"width_px":w,"height_px":h,
        "drawing_rectangle":r.drawing_rectangle,"svg_rectangle":[sx,sy,sw,sh]});
    Ok((block, facts))
}

/// The page text and what it embeds.
pub fn page(graph: &[u8], svg: &str, q: &MdReviewPageRequest) -> Result<(String, MdReviewPage)> {
    page_with_raster(graph, svg, q, None)
}

/// The page text and what it embeds, with a raster page image under the rendering when one is given.
pub fn page_with_raster(
    graph: &[u8],
    svg: &str,
    q: &MdReviewPageRequest,
    raster: Option<&RasterBackground>,
) -> Result<(String, MdReviewPage)> {
    let g: Value = oe_core::json::strict_json(graph)?;
    ensure!(
        g["schema"] == crate::engineering_expression::SCHEMA,
        "review_page_graph_schema"
    );
    let src = &g["source"];
    let entities = src["entities"].as_array().context("review_page_entities")?;
    let mut roots = BTreeSet::new();
    let mut geometry = std::collections::HashMap::new();
    for e in entities {
        let id = e["id"].as_str().context("review_page_entity_id")?;
        roots.insert(id.to_owned());
        geometry.insert(
            id.to_owned(),
            e["geometry_kind"].as_str().unwrap_or_default().to_owned(),
        );
    }
    let members = g["members"].as_array().context("review_page_members")?;
    // Display contract: partial member intervals exist only on qualified LINE sources.
    let mut sources = BTreeSet::new();
    for m in members {
        let h = m["source_handle"]
            .as_str()
            .context("review_page_member_source")?;
        sources.insert(h.to_owned());
        let interval: Vec<f64> = m["interval"]
            .as_array()
            .context("review_page_member_interval")?
            .iter()
            .filter_map(Value::as_f64)
            .collect();
        ensure!(
            geometry.get(h).is_some_and(|k| k == "line") || interval == [0., 1.],
            "review_page_partial_interval_on_unqualified_source:{h}"
        );
    }
    ensure!(sources == roots, "review_page_member_roots");
    let mut tally: Map<String, Value> = Map::new();
    let mut open_ends = Vec::new();
    for r in src["relations"]
        .as_array()
        .context("review_page_relations")?
    {
        let kind = r["kind"].as_str().unwrap_or_default();
        let from = r["from_id"].as_str().unwrap_or_default();
        let mut count = |id: &str| {
            let entry = tally.entry(id.to_owned()).or_insert_with(|| json!({}));
            let n = entry[kind].as_u64().unwrap_or(0);
            entry[kind] = json!(n + 1);
        };
        count(from);
        if let Some(to) = r["to_id"].as_str().filter(|to| *to != from) {
            count(to);
        }
        if kind == "open_endpoint" {
            let p = &r["points"];
            open_ends.push(json!([from, r["from_parameter"], p[0], p[1]]));
        }
    }
    let mut subset = Map::new();
    for k in [
        "schema",
        "profile",
        "source_sha256",
        "layer_convention",
        "layer_convention_evidence",
        "views",
        "line_styles",
        "members",
        "expressions",
        "unknowns",
    ] {
        subset.insert(
            k.into(),
            g.get(k)
                .cloned()
                .with_context(|| format!("review_page_graph_field:{k}"))?,
        );
    }
    for k in ["structure_ends", "implied_spans", "structural_lines"] {
        subset.insert(k.into(), g.get(k).cloned().unwrap_or_else(|| json!([])));
    }
    subset.insert("entities".into(), json!(entities));
    subset.insert(
        "native_member_count".into(),
        json!(src["members"].as_array().map_or(0, Vec::len)),
    );
    subset.insert("relation_tally".into(), Value::Object(tally));
    subset.insert("open_ends".into(), json!(open_ends));
    let shown = rendering(svg.as_bytes())?;
    ensure!(
        shown.entities.is_subset(&roots),
        "review_page_svg_roots_absent_from_graph"
    );
    ensure!(
        shown.active.is_empty(),
        "review_page_svg_active_content:{}",
        shown.active.join(",")
    );
    let nonrendered: Vec<&String> = roots.difference(&shown.entities).collect();
    let graph_sha256 = sha256(graph);
    let svg_sha256 = sha256(svg.as_bytes());
    // Names of the drafting concepts the expressions are instances of (catalogue cards).
    let used: BTreeSet<&str> = g["expressions"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| e["concept_ids"].as_array().into_iter().flatten().filter_map(Value::as_str))
        .collect();
    let mut concepts = serde_json::Map::new();
    for c in crate::engineering_expression::drawing_concepts()?.iter().flat_map(|x| x.concepts.iter()) {
        if used.contains(c.id.as_str()) {
            concepts.insert(c.id.clone(), json!(c.name));
        }
    }
    let raster = raster.map(|r| raster_layer(r, svg)).transpose()?;
    let mut payload = json!({"graph":Value::Object(subset),"concepts":Value::Object(concepts),"candidate":q.candidate,"version":q.version,"title":q.title,
        "source_note":q.source_note,"status_note":q.status_note,"graph_sha256":graph_sha256,"svg_sha256":svg_sha256,
        "rendered_roots":shown.entities.len(),"nonrendered_roots":nonrendered});
    if let Some((_, facts)) = &raster {
        payload["raster"] = facts.clone();
    }
    let data = serde_json::to_string(&payload)?.replace('<', "\\u003c");
    let text = TEMPLATE
        .replace(RASTER_TOKEN, raster.as_ref().map_or("", |(block, _)| block))
        .replace("@@DATA@@", &data)
        .replace("@@SVG@@", svg);
    if raster.is_some() {
        ensure!(text.len() <= LIMIT, "review_page_size_budget");
    }
    let result = MdReviewPage {
        schema: SCHEMA.into(),
        page: MdDrawingRouteFile {
            path: q.output.clone(),
            sha256: sha256(text.as_bytes()),
            bytes: text.len() as u64,
        },
        graph_sha256,
        svg_sha256,
        root_count: roots.len() as u64,
        rendered_root_count: shown.entities.len() as u64,
        expression_count: g["expressions"].as_array().map_or(0, Vec::len) as u64,
    };
    Ok((text, result))
}

/// Write one new page from a frozen graph and its rendering.
pub fn render(root: &SkillRoot, q: &MdReviewPageRequest) -> Result<MdReviewPage> {
    render_with_raster(root, q, None)
}

/// Write one new page from a frozen graph, its rendering and, when given, a raster page image under it.
pub fn render_with_raster(
    root: &SkillRoot,
    q: &MdReviewPageRequest,
    raster: Option<&RasterBackground>,
) -> Result<MdReviewPage> {
    ensure!(
        q.validate().is_ok() && q.schema == REQUEST_SCHEMA,
        "review_page_request_contract"
    );
    let graph = read_regular_bytes(&root.private_path(Path::new(&q.graph))?, LIMIT)?;
    let svg = String::from_utf8(read_regular_bytes(
        &root.private_path(Path::new(&q.svg))?,
        LIMIT,
    )?)
    .context("review_page_svg_utf8")?;
    let (text, result) = page_with_raster(&graph, &svg, q, raster)?;
    oe_core::write_new_bytes(&root.private_path(Path::new(&q.output))?, text.as_bytes())?;
    ensure!(result.validate().is_ok(), "review_page_result_contract");
    Ok(result)
}
