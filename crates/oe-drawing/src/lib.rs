//! Reading an existing engineering drawing: the reverse side of the CAD route.
//!
//! Every source entity is kept with its native members; source uses are nonexclusive and carry bounded
//! intervals; complete expressions are classified on four independent axes (discipline kind, line type,
//! native primitive, view); structure topology gives every free outline end a typed cause and joins
//! connected pieces into structural lines. Part feature targets stay unknown. Everything is derived from
//! the drawing bytes only, in an isolated worker under the current Project.
pub(crate) mod discipline_discovery;
pub mod engineering_expression;
pub mod expression_text;
pub(crate) mod native_layers;
pub(crate) mod native_readback;
pub mod page;
pub mod review;
pub mod route;
pub mod usage_discovery;

/// The invariants of an expression graph, checked on its bytes: the typed contract, source identity,
/// member domains and coverage, structure topology and structural-line ownership. For graphs written
/// outside the classifier (the pixel route) before they are frozen.
pub fn check_graph_bytes(bytes: &[u8]) -> anyhow::Result<()> {
    let graph: oe_contracts::MdEngineeringExpressionGraph =
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("engineering_expression_graph_parse: {e}"))?;
    // The contract failures by element first, so a graph written outside the classifier learns which
    // element and which rule failed; `check` itself reports the contract as one label.
    let mut failures: Vec<String> = Vec::new();
    let mut note = |label: String, r: std::result::Result<(), Vec<String>>| {
        if let Err(f) = r {
            failures.push(format!("{label}: {}", f.join(",")));
        }
    };
    note("graph".into(), graph.validate());
    for m in &graph.members {
        note(format!("member {}", m.id), m.validate());
    }
    for e in &graph.expressions {
        note(format!("expression {}", e.id), e.validate());
    }
    for v in &graph.views {
        note(format!("view {}", v.id), v.validate());
    }
    for s in &graph.line_styles {
        note(format!("line_style {}", s.source_handle), s.validate());
    }
    for e in &graph.structure_ends {
        note(format!("structure_end {}", e.id), e.validate());
    }
    for s in &graph.implied_spans {
        note(format!("implied_span {}", s.id), s.validate());
    }
    for l in &graph.structural_lines {
        note(format!("structural_line {}", l.id), l.validate());
    }
    anyhow::ensure!(
        failures.is_empty(),
        "engineering_expression_contract: {} failure(s): {}",
        failures.len(),
        failures.iter().take(12).cloned().collect::<Vec<_>>().join("; ")
    );
    engineering_expression::check(&graph)
}
