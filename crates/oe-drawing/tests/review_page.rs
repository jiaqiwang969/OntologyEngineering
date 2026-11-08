//! The review page embeds the frozen graph and the rendering unchanged, and refuses renderings with active
//! content or identities absent from the graph. Graph from the classifier on the original public I1 drawing.
use oe_contracts::MdReviewPageRequest;
use oe_drawing::page::RasterBackground;
use serde_json::{Value, json};
use std::path::Path;

fn example(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/drawing-understanding/i1-stepped-shaft")
            .join(name),
    )
    .unwrap()
}

fn request() -> MdReviewPageRequest {
    MdReviewPageRequest {
        schema: oe_drawing::page::REQUEST_SCHEMA.into(),
        graph: "graph.json".into(),
        svg: "source.svg".into(),
        title: "I1 stepped shaft".into(),
        candidate: "test".into(),
        version: "test".into(),
        source_note: String::new(),
        status_note: String::new(),
        output: "page.html".into(),
    }
}

/// A small authored graph and rendering with no drawing content: two abstract entities (a line and a circle) in
/// one view. The page text for it is fully determined by this file, which keeps the snapshot below stable.
fn authored() -> (Vec<u8>, String) {
    let graph = json!({
        "schema": oe_drawing::engineering_expression::SCHEMA,
        "profile": "authored-fixture",
        "source_sha256": "0".repeat(64),
        "layer_convention": "unverified",
        "layer_convention_evidence": [],
        "views": [{"id": "view:1", "label": "视图 1", "kind": "principal"}],
        "line_styles": [
            {"source_handle": "A1", "pattern": "continuous", "line_type": "thick_continuous", "basis": "width"},
            {"source_handle": "B2", "pattern": "continuous", "line_type": "thin_continuous", "basis": "width"}
        ],
        "members": [
            {"id": "m1", "expression_id": "x1", "source_handle": "A1", "interval": [0.0, 1.0], "member_role": "contour_segment",
             "line_type": "thick_continuous", "view_id": "view:1", "state": "observed", "evidence_ids": []},
            {"id": "m2", "expression_id": "x2", "source_handle": "B2", "interval": [0.0, 1.0], "member_role": "contour_segment",
             "line_type": "thin_continuous", "view_id": "view:1", "state": "observed", "evidence_ids": []}
        ],
        "expressions": [
            {"id": "x1", "kind": "visible_contour", "label": "轮廓 1", "view_id": "view:1", "state": "observed", "rule": "authored",
             "knowledge_ids": [], "qualifiers": [], "evidence_ids": [], "unknowns": [], "concept_ids": []},
            {"id": "x2", "kind": "visible_contour", "label": "轮廓 2", "view_id": "view:1", "state": "observed", "rule": "authored",
             "knowledge_ids": [], "qualifiers": [], "evidence_ids": [], "unknowns": [], "concept_ids": []}
        ],
        "unknowns": [],
        "source": {
            "entities": [
                {"id": "A1", "handle": "A1", "kind": "LINE", "layer": "0", "geometry_kind": "line", "coordinates": [0.0, 0.0, 100.0, 0.0]},
                {"id": "B2", "handle": "B2", "kind": "CIRCLE", "layer": "0", "geometry_kind": "circle", "coordinates": [50.0, 30.0, 10.0]}
            ],
            "relations": [],
            "members": []
        }
    });
    // Drawing origin (-10, -10) at the bottom left, top edge y = 70: drawing (x, y) is SVG (x + 10, 70 - y).
    let svg = concat!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 120 80\" data-drawing-origin-x=\"-10\" data-drawing-top-y=\"70\" width=\"1200\" height=\"800\">",
        "<rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
        "<g data-entity=\"A1\"><polyline fill=\"none\" stroke=\"#000000\" stroke-width=\"0.5\" points=\"10,70 110,70\"/></g>",
        "<g data-entity=\"B2\"><circle fill=\"none\" stroke=\"#000000\" stroke-width=\"0.25\" cx=\"60\" cy=\"40\" r=\"10\"/></g>",
        "</svg>"
    );
    (serde_json::to_vec(&graph).unwrap(), svg.to_owned())
}

fn authored_request() -> MdReviewPageRequest {
    MdReviewPageRequest {
        title: "authored fixture".into(),
        candidate: "fixture".into(),
        version: "fixture".into(),
        ..request()
    }
}

/// The page of the authored fixture as the code before the raster layer rendered it: `page()` at commit 07e194e
/// (template without the raster token), run once over `authored()` and `authored_request()` and recorded here as
/// its length and SHA-256. A page without a raster must still be exactly these bytes.
const PLAIN_PAGE_BYTES: usize = 50111;
const PLAIN_PAGE_SHA256: &str = "0e93707fd95e3701d3c215e27bafbc63399aa608b7030db29f50c94978101fde";

/// A 3 × 2 RGBA PNG (84 bytes) written for this test with Python's zlib and struct modules; no drawing content.
const FIXTURE_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x9d, 0x74, 0x66,
    0x1a, 0x00, 0x00, 0x00, 0x1b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x60, 0x60, 0x38, 0xf1,
    0x5f, 0x03, 0x88, 0x03, 0x80, 0x98, 0x81, 0x21, 0x00, 0xc8, 0x01, 0xe2, 0x00, 0x20, 0x06, 0x00,
    0x91, 0x15, 0x0c, 0x8b, 0xa8, 0x0c, 0x24, 0x19, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];
/// Standard base64 of `FIXTURE_PNG` (Python `base64.b64encode`), independent of the crate's own encoder.
const FIXTURE_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAG0lEQVR42mNgYDjxXwOIA4CYgSEAyAHiACAGAJEVDIuoDCQZAAAAAElFTkSuQmCC";

fn raster() -> RasterBackground<'static> {
    RasterBackground {
        png: FIXTURE_PNG,
        width_px: 3,
        height_px: 2,
        // Drawing rectangle (0, 0)–(60, 40): SVG x 10, y 30 (top edge 70 − 40), 60 wide, 40 high.
        drawing_rectangle: [0.0, 0.0, 60.0, 40.0],
    }
}

#[test]
fn a_page_without_raster_keeps_the_bytes_of_the_plain_page() {
    let (graph, svg) = authored();
    let q = authored_request();
    let (plain, result) = oe_drawing::page::page(&graph, &svg, &q).unwrap();
    assert_eq!(plain.len(), PLAIN_PAGE_BYTES);
    assert_eq!(oe_core::sha256(plain.as_bytes()), PLAIN_PAGE_SHA256);
    assert_eq!(result.page.bytes as usize, plain.len());
    let (same, _) = oe_drawing::page::page_with_raster(&graph, &svg, &q, None).unwrap();
    assert_eq!(same.as_bytes(), plain.as_bytes());
    assert!(
        !plain.contains("@@RASTER@@") && !plain.contains("<img") && !plain.contains("rasterShown")
    );
    assert!(payload(&plain).get("raster").is_none());
}

#[test]
fn a_raster_lies_under_the_svg_on_the_drawing_coordinates_of_the_rendering() {
    let (graph, svg) = authored();
    let q = authored_request();
    let (text, result) =
        oe_drawing::page::page_with_raster(&graph, &svg, &q, Some(&raster())).unwrap();
    // An HTML image inside the canvas before the untouched SVG, never inside it.
    let canvas = text.find("<div class=\"canvas\" id=\"canvas\">").unwrap();
    let img = text.find("<img id=\"raster\"").unwrap();
    let svg_at = text.find(&svg).unwrap();
    assert!(canvas < img && img < svg_at, "{canvas} {img} {svg_at}");
    assert_eq!(text.matches("<img").count(), 1);
    assert_eq!(result.svg_sha256, oe_core::sha256(svg.as_bytes()));
    let tag = &text[img..text[img..].find('>').unwrap() + img];
    assert!(tag.contains(&format!(
        "src=\"data:image/png;base64,{FIXTURE_PNG_BASE64}\""
    )));
    for attribute in [
        "width=\"3\"",
        "height=\"2\"",
        "data-drawing-x0=\"0\"",
        "data-drawing-y0=\"0\"",
        "data-drawing-x1=\"60\"",
        "data-drawing-y1=\"40\"",
        "data-svg-x=\"10\"",
        "data-svg-y=\"30\"",
        "data-svg-width=\"60\"",
        "data-svg-height=\"40\"",
        "transform-origin:0 0",
        "pointer-events:none",
    ] {
        assert!(tag.contains(attribute), "{attribute} in {tag}");
    }
    assert!(tag.contains(&format!("data-sha256=\"{}\"", oe_core::sha256(FIXTURE_PNG))));
    // The option to hide it, and the facts beside the graph; the graph itself is that of the plain page.
    assert!(text.contains("id=\"rasterShown\" checked"));
    let data = payload(&text);
    assert_eq!(
        data["raster"]["svg_rectangle"],
        json!([10.0, 30.0, 60.0, 40.0])
    );
    assert_eq!(
        data["raster"]["drawing_rectangle"],
        json!([0.0, 0.0, 60.0, 40.0])
    );
    assert_eq!(data["raster"]["sha256"], oe_core::sha256(FIXTURE_PNG));
    assert_eq!(data["raster"]["bytes"], FIXTURE_PNG.len());
    let (plain, _) = oe_drawing::page::page(&graph, &svg, &q).unwrap();
    assert_eq!(data["graph"], payload(&plain)["graph"]);
    // The refusal rules of the rendering are unchanged with a raster: an image inside the SVG is refused.
    let with_image = svg.replacen("<rect", "<image href=\"x.png\"/><rect", 1);
    let error = oe_drawing::page::page_with_raster(&graph, &with_image, &q, Some(&raster()))
        .unwrap_err()
        .to_string();
    assert!(
        error.starts_with("review_page_svg_active_content"),
        "{error}"
    );
    // The PNG must be what it is declared to be and cover a proper rectangle meeting the rendering.
    for (bad, reason) in [
        (
            RasterBackground {
                width_px: 4,
                ..raster()
            },
            "review_page_raster_dimensions",
        ),
        (
            RasterBackground {
                png: &FIXTURE_PNG[..20],
                ..raster()
            },
            "review_page_raster_png",
        ),
        (
            RasterBackground {
                drawing_rectangle: [0.0, 0.0, 0.0, 40.0],
                ..raster()
            },
            "review_page_raster_rectangle",
        ),
        (
            RasterBackground {
                drawing_rectangle: [500.0, 500.0, 560.0, 540.0],
                ..raster()
            },
            "review_page_raster_outside_rendering",
        ),
    ] {
        let error = oe_drawing::page::page_with_raster(&graph, &svg, &q, Some(&bad))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with(reason), "{error}");
    }
    // Without the drawing frame on the rendering there is nothing to align the image with.
    let unframed = svg.replace(" data-drawing-origin-x=\"-10\"", "");
    let error = oe_drawing::page::page_with_raster(&graph, &unframed, &q, Some(&raster()))
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("review_page_svg_frame"), "{error}");
}

#[test]
fn a_raster_page_stays_within_the_read_limit_of_the_route() {
    // 64 MiB of PNG would pass the 64 MiB page limit once base64-encoded; it is refused before encoding.
    let mut png = vec![0u8; 64 * 1024 * 1024];
    png[..33].copy_from_slice(&FIXTURE_PNG[..33]);
    let (graph, svg) = authored();
    let error = oe_drawing::page::page_with_raster(
        &graph,
        &svg,
        &authored_request(),
        Some(&RasterBackground {
            png: &png,
            ..raster()
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(error.starts_with("review_page_size_budget"), "{error}");
}

fn payload(page: &str) -> Value {
    let start = page
        .find("<script type=\"application/json\" id=\"payload\">")
        .unwrap()
        + "<script type=\"application/json\" id=\"payload\">".len();
    let end = start + page[start..].find("</script>").unwrap();
    serde_json::from_str(&page[start..end]).unwrap()
}

#[test]
fn the_review_page_keeps_source_identities_and_refuses_active_svg() {
    let dxf = example("source.dxf");
    let graph =
        serde_json::to_vec(&oe_drawing::engineering_expression::discover(&dxf).unwrap()).unwrap();
    let svg = String::from_utf8(example("source.svg")).unwrap();
    let (text, result) = oe_drawing::page::page(&graph, &svg, &request()).unwrap();
    assert!(text.contains(&svg) && !text.contains("@@DATA@@") && !text.contains("@@SVG@@"));
    let data = payload(&text);
    let g: Value = serde_json::from_slice(&graph).unwrap();
    assert_eq!(data["graph"]["members"], g["members"]);
    assert_eq!(data["graph"]["expressions"], g["expressions"]);
    assert_eq!(data["graph"]["entities"], g["source"]["entities"]);
    assert_eq!(data["graph_sha256"], oe_core::sha256(&graph));
    let rendered = oe_drawing::page::rendering(svg.as_bytes())
        .unwrap()
        .entities;
    assert_eq!(result.rendered_root_count as usize, rendered.len());
    assert_eq!(
        result.root_count as usize,
        g["source"]["entities"].as_array().unwrap().len()
    );
    assert!(!rendered.is_empty());
    // A rendering with active content, or with an identity the graph does not have, is refused.
    for (bad, reason) in [
        (
            svg.replacen("<rect", "<script>alert(1)</script><rect", 1),
            "review_page_svg_active_content",
        ),
        (
            svg.replacen("<rect", "<rect onload=\"x()\"", 1),
            "review_page_svg_active_content",
        ),
        (
            svg.replacen("<rect", "<a href=\"https://example.invalid\"></a><rect", 1),
            "review_page_svg_active_content",
        ),
        (
            svg.replacen("<rect", "<g data-entity=\"NOT-A-ROOT\"></g><rect", 1),
            "review_page_svg_roots_absent_from_graph",
        ),
    ] {
        let error = oe_drawing::page::page(&graph, &bad, &request())
            .unwrap_err()
            .to_string();
        assert!(error.starts_with(reason), "{error}");
    }
    // Partial intervals are shown only on qualified LINE sources.
    let mut changed = g.clone();
    let other = g["source"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["geometry_kind"] != "line")
        .map(|e| e["id"].clone());
    if let Some(id) = other {
        let i = g["members"]
            .as_array()
            .unwrap()
            .iter()
            .position(|m| m["source_handle"] == id)
            .unwrap();
        changed["members"][i]["interval"] = json!([0.0, 0.5]);
        let error =
            oe_drawing::page::page(&serde_json::to_vec(&changed).unwrap(), &svg, &request())
                .unwrap_err()
                .to_string();
        assert!(
            error.starts_with("review_page_partial_interval_on_unqualified_source"),
            "{error}"
        );
    }
}
