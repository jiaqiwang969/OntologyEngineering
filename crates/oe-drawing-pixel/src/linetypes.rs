//! Line-type knowledge of the pixel recognizer, read from `knowledge/line-types.json`: the families of discontinuous
//! lines, the bounds of their elements, how one drawing keeps them consistent, how CAD constructs and renders them and
//! how they meet other lines, each with its source. Every line-type bound the classifier uses is a field here.

use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow, ensure};
use serde_json::Value;

/// The knowledge file, embedded at build time.
pub const KNOWLEDGE: &str = include_str!("../knowledge/line-types.json");

/// The bounds of the line-type classifier (statements and sources in the knowledge file).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineTypeBounds {
    pub blank_gap_max_dark: f64,
    pub max_dash_gap_paper_mm: f64,
    pub dashed_max_dash_paper_mm: f64,
    pub dashed_min_gap_to_dash: f64,
    pub chain_max_long_paper_mm: f64,
    pub chain_dot_max_paper_mm: f64,
    pub chain_max_short_to_long: f64,
    pub chain_max_short_to_gap: f64,
    pub page_gap_tolerance: f64,
    pub two_dash_end_equal_tolerance: f64,
    pub two_dash_min_end_to_dash: f64,
    pub two_dash_max_end_to_dash: f64,
    pub closure_repeat_slack: f64,
    pub closure_join_paper_mm: f64,
    pub text_glyph_max_paper_mm: f64,
    pub element_short_spread: f64,
    pub element_spread_slack_paper_mm: f64,
    pub element_long_share: f64,
    pub element_group_share: f64,
    pub dash_spread_cv: f64,
    pub gap_spread_cv: f64,
    pub piece_spread: f64,
    pub crossing_reach_paper_mm: f64,
}

/// The names of the bounds, as the knowledge file keys them.
pub const BOUND_NAMES: [&str; 23] = [
    "blank_gap_max_dark",
    "max_dash_gap_paper_mm",
    "dashed_max_dash_paper_mm",
    "dashed_min_gap_to_dash",
    "chain_max_long_paper_mm",
    "chain_dot_max_paper_mm",
    "chain_max_short_to_long",
    "chain_max_short_to_gap",
    "page_gap_tolerance",
    "two_dash_end_equal_tolerance",
    "two_dash_min_end_to_dash",
    "two_dash_max_end_to_dash",
    "closure_repeat_slack",
    "closure_join_paper_mm",
    "text_glyph_max_paper_mm",
    "element_short_spread",
    "element_spread_slack_paper_mm",
    "element_long_share",
    "element_group_share",
    "dash_spread_cv",
    "gap_spread_cv",
    "piece_spread",
    "crossing_reach_paper_mm",
];

/// Where a bound's value comes from (the file's `basis_values`).
pub const BASIS_VALUES: [&str; 4] = ["standard", "standard_margin", "measured", "development"];

/// Parse the knowledge file: every bound present with a finite positive value, at least one source and the basis of its
/// value, and no bound the classifier does not read.
pub fn parse(text: &str) -> Result<LineTypeBounds> {
    let v: Value = serde_json::from_str(text).context("line_types_json")?;
    let bounds = v["bounds"].as_object().ok_or_else(|| anyhow!("line_types_bounds_missing"))?;
    for key in bounds.keys() {
        ensure!(BOUND_NAMES.contains(&key.as_str()), "line_type_bound_unread:{key}");
    }
    let get = |k: &str| -> Result<f64> {
        let e = &v["bounds"][k];
        let value = e["value"].as_f64().ok_or_else(|| anyhow!("line_type_bound_missing:{k}"))?;
        ensure!(value.is_finite() && value > 0.0, "line_type_bound_not_positive:{k}");
        ensure!(e["sources"].as_array().is_some_and(|s| !s.is_empty()), "line_type_bound_without_source:{k}");
        ensure!(e["basis"].as_str().is_some_and(|b| BASIS_VALUES.contains(&b)), "line_type_bound_without_basis:{k}");
        Ok(value)
    };
    Ok(LineTypeBounds {
        blank_gap_max_dark: get("blank_gap_max_dark")?,
        max_dash_gap_paper_mm: get("max_dash_gap_paper_mm")?,
        dashed_max_dash_paper_mm: get("dashed_max_dash_paper_mm")?,
        dashed_min_gap_to_dash: get("dashed_min_gap_to_dash")?,
        chain_max_long_paper_mm: get("chain_max_long_paper_mm")?,
        chain_dot_max_paper_mm: get("chain_dot_max_paper_mm")?,
        chain_max_short_to_long: get("chain_max_short_to_long")?,
        chain_max_short_to_gap: get("chain_max_short_to_gap")?,
        page_gap_tolerance: get("page_gap_tolerance")?,
        two_dash_end_equal_tolerance: get("two_dash_end_equal_tolerance")?,
        two_dash_min_end_to_dash: get("two_dash_min_end_to_dash")?,
        two_dash_max_end_to_dash: get("two_dash_max_end_to_dash")?,
        closure_repeat_slack: get("closure_repeat_slack")?,
        closure_join_paper_mm: get("closure_join_paper_mm")?,
        text_glyph_max_paper_mm: get("text_glyph_max_paper_mm")?,
        element_short_spread: get("element_short_spread")?,
        element_spread_slack_paper_mm: get("element_spread_slack_paper_mm")?,
        element_long_share: get("element_long_share")?,
        element_group_share: get("element_group_share")?,
        dash_spread_cv: get("dash_spread_cv")?,
        gap_spread_cv: get("gap_spread_cv")?,
        piece_spread: get("piece_spread")?,
        crossing_reach_paper_mm: get("crossing_reach_paper_mm")?,
    })
}

/// Evaluation steps of the line-type mechanisms (S-DRAWING-TERRA-PARITY-R4, criterion N8): the chain line with a short
/// dash (scope item 3), the two-dash rule (item 4) and the hidden closure (item 5), each with the mechanisms that serve
/// it. The stage runs all of them (`C`); `OEV2_PIXEL_LINE_STEPS` set to `0`, `A` or `B` turns the later ones off — 0: the
/// round-3 reading, A: item 3, B: items 3 and 4 — so that every class change can be located at the step that makes it.
/// For evaluation only; the graph's line-pattern conditions name a step other than `C`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineSteps {
    pub chain_short_dash: bool,
    pub two_dash: bool,
    pub closure: bool,
    pub label: &'static str,
}

/// Parse an evaluation step (`None`: the stage, `C`).
pub fn parse_steps(value: Option<&str>) -> Result<LineSteps> {
    let (chain_short_dash, two_dash, closure, label) = match value.unwrap_or("C") {
        "0" => (false, false, false, "0"),
        "A" => (true, false, false, "A"),
        "B" => (true, true, false, "B"),
        "C" => (true, true, true, "C"),
        other => anyhow::bail!("line_steps_unknown:{other}"),
    };
    Ok(LineSteps { chain_short_dash, two_dash, closure, label })
}

/// The evaluation step of this process (`OEV2_PIXEL_LINE_STEPS`, read once; unset: the stage).
pub fn steps() -> LineSteps {
    static STEPS: OnceLock<LineSteps> = OnceLock::new();
    *STEPS.get_or_init(|| {
        parse_steps(std::env::var("OEV2_PIXEL_LINE_STEPS").ok().as_deref()).expect("OEV2_PIXEL_LINE_STEPS is one of 0, A, B, C")
    })
}

/// The bounds of the embedded knowledge file, parsed once.
pub fn bounds() -> &'static LineTypeBounds {
    static BOUNDS: OnceLock<LineTypeBounds> = OnceLock::new();
    BOUNDS.get_or_init(|| parse(KNOWLEDGE).expect("knowledge/line-types.json is valid (checked by the crate's tests)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_type_bound_comes_from_the_knowledge_file_with_a_source() {
        let b = parse(KNOWLEDGE).expect("knowledge parses");
        assert_eq!(&b, bounds());
        let v: Value = serde_json::from_str(KNOWLEDGE).expect("json");
        let keys: Vec<&str> = v["bounds"].as_object().expect("bounds").keys().map(String::as_str).collect();
        let mut names = BOUND_NAMES.to_vec();
        names.sort_unstable();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, names, "the classifier reads exactly the bounds of the file");
        for family in ["dashed", "chain", "double_chain"] {
            let f = &v["families"][family];
            assert!(f["concepts"].as_array().is_some_and(|c| !c.is_empty()), "{family} names its concept cards");
            assert!(f["sources"].as_array().is_some_and(|s| !s.is_empty()), "{family} has sources");
        }
        for statement in [
            "equal_elements_in_one_drawing",
            "cad_dashed_construction",
            "meet_on_dashes",
            "cad_intersections_in_gaps",
            "cad_short_segment_continuous",
            "views_only",
            "erased_crossing_inked_gap",
            "gap_between_walls",
            "chain_ends_long",
            "rescued_pieces_candidates_only",
            "closure_junctions",
            "merge_confirmed_on_ink",
            "linetype_scales_as_modes",
        ] {
            assert!(v["statements"][statement]["sources"].as_array().is_some_and(|s| !s.is_empty()), "{statement} has sources");
        }
        assert!(matches!(v["statements"]["cad_short_segment_continuous"]["status"].as_str(), Some("confirmed" | "refuted")));
    }

    #[test]
    fn evaluation_steps_add_one_item_each_and_default_to_the_stage() {
        let c = parse_steps(None).expect("stage");
        assert!(c.chain_short_dash && c.two_dash && c.closure && c.label == "C");
        let z = parse_steps(Some("0")).expect("0");
        assert!(!z.chain_short_dash && !z.two_dash && !z.closure);
        let a = parse_steps(Some("A")).expect("A");
        assert!(a.chain_short_dash && !a.two_dash && !a.closure);
        let b = parse_steps(Some("B")).expect("B");
        assert!(b.chain_short_dash && b.two_dash && !b.closure);
        assert!(parse_steps(Some("D")).is_err());
    }

    #[test]
    fn a_bound_without_a_source_or_one_the_classifier_does_not_read_is_refused() {
        let mut v: Value = serde_json::from_str(KNOWLEDGE).expect("json");
        v["bounds"]["chain_max_short_to_gap"]["sources"] = Value::Array(vec![]);
        assert!(parse(&v.to_string()).is_err());
        let mut v: Value = serde_json::from_str(KNOWLEDGE).expect("json");
        v["bounds"]["an_unread_bound"] = serde_json::json!({"value": 1.0, "sources": ["x"], "basis": "development"});
        assert!(parse(&v.to_string()).is_err());
        let mut v: Value = serde_json::from_str(KNOWLEDGE).expect("json");
        v["bounds"]["piece_spread"]["basis"] = Value::String("tuned".into());
        assert!(parse(&v.to_string()).is_err());
    }
}
