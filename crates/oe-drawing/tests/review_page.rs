//! The review page embeds the frozen graph and the rendering unchanged, and refuses renderings with active
//! content or identities absent from the graph. Graph from the classifier on the original public I1 drawing.
use oe_contracts::MdReviewPageRequest;
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
