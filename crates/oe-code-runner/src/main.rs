//! Internal replay adapter. It receives inputs, never oracle expectations.
use anyhow::{Result, ensure};
mod semantic_outcome;
use oe_contracts::{CodeRdfLibSnapshotInput, CodeReplayInvocation, CodeRunnerObservation};
use oe_core::{read_regular_bytes, sha256};
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};
use std::path::Path;
fn compiled() -> Result<Value> {
    Ok(strict_json(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/code-source.json"
    )))?)
}
fn input_text<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    input[key]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("runner_input_{key}"))
}
fn fields(input: &Value, expected: &[&str]) -> Result<()> {
    let object = input
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("runner_input_object"))?;
    ensure!(
        object.len() == expected.len() && object.keys().all(|k| expected.contains(&k.as_str())),
        "runner_input_unknown_or_missing_fields"
    );
    Ok(())
}
fn execute(invocation: &CodeReplayInvocation) -> Result<Option<Value>> {
    if invocation.profile == "local-semantic-replay/2" {
        return match invocation.capability.as_str() {
            "semantic.reason-metadata/1" => Ok(Some(serde_json::to_value(
                semantic_outcome::execute(&invocation.input)?,
            )?)),
            _ => Ok(None),
        };
    }
    ensure!(
        invocation.profile == "local-semantic-replay/1",
        "runner_profile_unsupported"
    );
    match invocation.capability.as_str() {
        "rdf.snapshot-rdflib/1" => {
            fields(
                &invocation.input,
                &["data", "format", "graph_name", "created_at"],
            )?;
            let input: CodeRdfLibSnapshotInput = serde_json::from_value(invocation.input.clone())?;
            input
                .validate()
                .map_err(|e| anyhow::anyhow!("runner_snapshot_input_invalid:{e:?}"))?;
            let format = match input.format.as_str() {
                "ntriples" => oxigraph::io::RdfFormat::NTriples,
                "nquads" => oxigraph::io::RdfFormat::NQuads,
                _ => anyhow::bail!("runner_snapshot_format_unsupported"),
            };
            let mut session = semantica_runtime::dataset::DatasetSession::new();
            session.load_rdflib(input.data.as_bytes(), format, input.graph_name.as_deref())?;
            Ok(Some(serde_json::to_value(
                session.snapshot_rdflib(&input.created_at)?,
            )?))
        }
        "rdf.query/1" => {
            fields(&invocation.input, &["turtle", "query"])?;
            let graph = semantica_runtime::graph::load(
                input_text(&invocation.input, "turtle")?.as_bytes(),
            )?;
            Ok(Some(semantica_runtime::graph::query(
                &graph,
                input_text(&invocation.input, "query")?,
            )?))
        }
        "rdf.query-with-bindings/1" => {
            fields(
                &invocation.input,
                &["turtle", "query", "namespaces", "bindings"],
            )?;
            let graph = semantica_runtime::graph::load(
                input_text(&invocation.input, "turtle")?.as_bytes(),
            )?;
            let namespaces: std::collections::BTreeMap<String, String> =
                serde_json::from_value(invocation.input["namespaces"].clone())?;
            let mut bindings = std::collections::BTreeMap::new();
            for (key, term) in invocation.input["bindings"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("runner_bindings_object"))?
            {
                fields(term, &["kind", "value"])?;
                ensure!(term["kind"] == "iri", "runner_binding_profile_only_iri");
                bindings.insert(
                    key.clone(),
                    oxigraph::model::Term::NamedNode(oxigraph::model::NamedNode::new(input_text(
                        term, "value",
                    )?)?),
                );
            }
            Ok(Some(semantica_runtime::graph::query_with_options(
                &graph,
                input_text(&invocation.input, "query")?,
                &namespaces,
                &bindings,
            )?))
        }
        "shacl.conforms/3" => {
            fields(&invocation.input, &["turtle", "shapes"])?;
            let report = semantica_runtime::shacl::validate(
                input_text(&invocation.input, "turtle")?.as_bytes(),
                input_text(&invocation.input, "shapes")?.as_bytes(),
            )?;
            Ok(Some(json!({"conforms":report["conforms"]})))
        }
        "datalog.query/1" => {
            fields(&invocation.input, &["facts", "rules", "query"])?;
            let mut reasoner = semantica_runtime::datalog::DatalogReasoner::new();
            for fact in invocation.input["facts"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("runner_fact_list"))?
            {
                reasoner.add_fact(
                    fact.as_str()
                        .ok_or_else(|| anyhow::anyhow!("runner_fact_text"))?,
                )?;
            }
            for rule in invocation.input["rules"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("runner_rule_list"))?
            {
                reasoner.add_rule(
                    rule.as_str()
                        .ok_or_else(|| anyhow::anyhow!("runner_rule_text"))?,
                )?;
            }
            Ok(Some(serde_json::to_value(
                reasoner.query(input_text(&invocation.input, "query")?, None)?,
            )?))
        }
        _ => Ok(None),
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 2 && args[1] == "identity" {
        println!("{}", String::from_utf8(canonical_json(&compiled()?)?)?);
        return Ok(());
    }
    ensure!(args.len() == 3 && args[1] == "run", "runner_arguments");
    let invocation: CodeReplayInvocation = serde_json::from_value(strict_json(
        &read_regular_bytes(Path::new(&args[2]), 1024 * 1024)?,
    )?)?;
    invocation
        .validate()
        .map_err(|e| anyhow::anyhow!("runner_protocol_invalid:{e:?}"))?;
    ensure!(
        sha256(canonical_json(&invocation.input)?) == invocation.input_sha256,
        "runner_input_identity_changed"
    );
    let result = execute(&invocation);
    let (execution, value, error) = match result {
        Ok(Some(value)) => ("completed", value, None),
        Ok(None) => (
            "unsupported",
            Value::Null,
            Some("capability_not_implemented_in_this_runner_profile".into()),
        ),
        Err(error) if error.to_string() == "query_initial_binding_minus_scope_unsupported" => {
            ("unsupported", Value::Null, Some(error.to_string()))
        }
        Err(error) => ("failed", Value::Null, Some(error.to_string())),
    };
    let observation = CodeRunnerObservation {
        schema: "oev2.code-runner-observation/1".into(),
        case_id: invocation.case_id,
        input_sha256: invocation.input_sha256,
        conditions_sha256: invocation.conditions_sha256,
        compiled_manifest_sha256: compiled()?["manifest_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("runner_compiled_identity"))?
            .into(),
        execution: execution.into(),
        result: value,
        coverage: if execution == "completed" {
            vec![invocation.capability]
        } else {
            vec![]
        },
        unknown: vec!["engineering_effectiveness_not_evaluated".into()],
        error,
    };
    println!(
        "{}",
        String::from_utf8(canonical_json(&serde_json::to_value(observation)?)?)?
    );
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/q40_outcome.rs"]
mod q40_outcome;
