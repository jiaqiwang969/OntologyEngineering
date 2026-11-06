//! Presentation only: the review page of a frozen expression graph over the neutral rendering of its source.
//! No classification is performed here. The graph and the rendering are embedded unchanged; the page says that
//! no independent reference is embedded, since classification comes from the source bytes only.
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
const LIMIT: usize = 64 * 1024 * 1024;

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

/// The page text and what it embeds.
pub fn page(graph: &[u8], svg: &str, q: &MdReviewPageRequest) -> Result<(String, MdReviewPage)> {
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
    let payload = json!({"graph":Value::Object(subset),"concepts":Value::Object(concepts),"candidate":q.candidate,"version":q.version,"title":q.title,
        "source_note":q.source_note,"status_note":q.status_note,"graph_sha256":graph_sha256,"svg_sha256":svg_sha256,
        "rendered_roots":shown.entities.len(),"nonrendered_roots":nonrendered});
    let data = serde_json::to_string(&payload)?.replace('<', "\\u003c");
    let text = TEMPLATE.replace("@@DATA@@", &data).replace("@@SVG@@", svg);
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
    let (text, result) = page(&graph, &svg, q)?;
    oe_core::write_new_bytes(&root.private_path(Path::new(&q.output))?, text.as_bytes())?;
    ensure!(result.validate().is_ok(), "review_page_result_contract");
    Ok(result)
}
