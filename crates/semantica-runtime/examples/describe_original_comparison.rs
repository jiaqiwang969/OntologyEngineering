//! Executable exact reference comparison. A difference exits 2; no expected value is replaced.
use oxigraph::io::{RdfFormat, RdfParser};
use semantica_runtime::graph;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn main() -> anyhow::Result<()> {
    let fixture = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("original_fixture_path_required"))?;
    let original: Value = serde_json::from_slice(&std::fs::read(&fixture)?)?;
    let namespaces: BTreeMap<String, String> =
        serde_json::from_value(original["namespaces"].clone())?;
    let mut observations = Vec::new();
    let mut different = 0;
    let mut original_errors = 0;
    for case in original["cases"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("original_cases_required"))?
    {
        if case["kind"] == "ORIGINAL_ERROR" {
            original_errors += 1;
            observations.push(json!({"id":case["id"],"comparison":"original_error_not_success_oracle","original":case}));
            continue;
        }
        let dataset = graph::load(
            case["data"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("original_data_required"))?
                .as_bytes(),
        )?;
        let mut bindings = BTreeMap::new();
        for (name, value) in case["bindings"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("original_bindings_required"))?
        {
            let line = format!(
                "<urn:binding> <urn:value> {} .",
                value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("original_binding_term_required"))?
            );
            let term = RdfParser::from_format(RdfFormat::NTriples)
                .for_reader(line.as_bytes())
                .next()
                .ok_or_else(|| anyhow::anyhow!("original_binding_term_missing"))??
                .object;
            bindings.insert(name.clone(), term);
        }
        let actual = graph::query_rdflib_with_options(
            &dataset,
            case["query"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("original_query_required"))?,
            &namespaces,
            &bindings,
        )?;
        let mut triples: Vec<String> = actual["triples"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("current_graph_result_required"))?
            .iter()
            .map(|term| format!("{} .", term.as_str().expect("triple text")))
            .collect();
        if case["blank_subject_label_projection"] == true {
            let labels: BTreeSet<String> = triples
                .iter()
                .flat_map(|triple| triple.split_whitespace())
                .filter(|term| term.starts_with("_:"))
                .map(str::to_owned)
                .collect();
            anyhow::ensure!(labels.len() == 1, "single_blank_original_domain_changed");
            let label = labels.into_iter().next().unwrap();
            triples = triples
                .into_iter()
                .map(|triple| triple.replace(&label, "_:target"))
                .collect();
        }
        triples.sort();
        let matched = serde_json::to_value(&triples)? == case["triples"];
        different += usize::from(!matched);
        observations.push(json!({"id":case["id"],"comparison":if matched {"matched"} else {"different"},"original_triples":case["triples"],"current_triples":triples,"complete_original_input":case}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":"private.describe-exact-original-comparison/1","fixture":fixture,"observations":observations,"different":different,"original_errors":original_errors,"scope":"Exact frozen original comparison only; differences and original errors remain visible, no qualification or adoption"})
        )?
    );
    if different != 0 {
        std::process::exit(2);
    }
    Ok(())
}
