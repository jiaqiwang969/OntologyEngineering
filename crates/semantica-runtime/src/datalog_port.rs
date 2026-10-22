//! Stateless transport for the existing DatalogReasoner, never a second engine.
use crate::datalog::{ContextGraphData, DatalogLimits, DatalogReasoner};
use crate::runtime::RustSemanticRuntime;
use semantica_contracts::{
    SemanticDatalogBudget, SemanticDatalogFact, SemanticDatalogGraph, SemanticDatalogGraphPort,
    SemanticDatalogGraphRequest, SemanticDatalogGraphResult, SemanticDatalogPort,
    SemanticDatalogQuery, SemanticDatalogRequest, SemanticDatalogResult, SemanticRuntimeFailure,
};
use serde::Serialize;
use std::io::{self, Write};

pub const PROFILE: &str = "semantica.positive-function-free-datalog/1";
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_FACT_INPUTS: usize = 4096;
const MAX_RULE_INPUTS: usize = 256;

pub(crate) fn failure(code: &str, phase: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: code.into(),
        phase: phase.into(),
        message: message.to_string(),
        retryable: false,
    }
}

/// Count actual serialized bytes without first allocating an over-budget buffer.
struct ByteCounter(usize);
impl Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_sub(bytes.len()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "datalog_serialized_byte_budget")
        })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn check_serialized(
    value: &impl Serialize,
    max: usize,
    pretty: bool,
) -> Result<(), SemanticRuntimeFailure> {
    let mut sink = ByteCounter(max);
    let result = if pretty {
        serde_json::to_writer_pretty(&mut sink, value)
    } else {
        serde_json::to_writer(&mut sink, value)
    };
    result.map_err(|_| {
        failure(
            if pretty {
                "datalog_output_byte_budget"
            } else {
                "datalog_input_byte_budget"
            },
            if pretty { "result" } else { "contract" },
            "Complete serialized Datalog envelope exceeds its declared bound",
        )
    })
}

pub(crate) fn limits(b: &SemanticDatalogBudget) -> Result<DatalogLimits, SemanticRuntimeFailure> {
    let defaults = DatalogLimits::default();
    for (actual, maximum) in [
        (b.max_iterations, defaults.max_iterations),
        (b.max_match_attempts, defaults.max_match_attempts),
        (b.max_facts, defaults.max_facts),
        (b.max_query_rows, defaults.max_query_rows),
    ] {
        if actual == 0 || actual > maximum as u64 {
            return Err(failure(
                "datalog_invalid_budget",
                "contract",
                "Limits must be positive and cannot exceed the existing algorithm ceilings",
            ));
        }
    }
    Ok(DatalogLimits {
        max_iterations: b.max_iterations as usize,
        max_match_attempts: b.max_match_attempts as usize,
        max_facts: b.max_facts as usize,
        max_query_rows: b.max_query_rows as usize,
        ..defaults
    })
}

// Both transports use one import/inference implementation and retain separate
// generated wire identities. There is no second graph or Project owner here.
struct Execution {
    input_inserted: Vec<bool>,
    graph_facts_added: u64,
    seed_fact_count: u64,
    derived_fact_count: u64,
    closure: Vec<SemanticDatalogFact>,
    query_rows: Option<Vec<std::collections::BTreeMap<String, String>>>,
    iterations: u64,
    inference_match_attempts: u64,
}

fn execute(
    facts: &[serde_json::Value],
    rules: &[String],
    query: Option<&SemanticDatalogQuery>,
    budget: &SemanticDatalogBudget,
    graph: Option<&SemanticDatalogGraph>,
) -> Result<Execution, SemanticRuntimeFailure> {
    let graph_count = graph.map_or(0, |g| g.nodes.len().saturating_add(g.edges.len()));
    if facts.len().saturating_add(graph_count) > MAX_FACT_INPUTS || rules.len() > MAX_RULE_INPUTS {
        return Err(failure(
            "datalog_input_count_budget",
            "contract",
            "Too many combined fact/graph records or rule inputs",
        ));
    }
    if graph.is_some_and(|g| g.edges.iter().chain(&g.nodes).any(|v| !v.is_object())) {
        return Err(failure(
            "datalog_graph_record_invalid",
            "contract",
            "Graph snapshots require dictionary node and edge records",
        ));
    }
    let mut engine = DatalogReasoner::with_limits(limits(budget)?)
        .map_err(|e| failure("datalog_invalid_budget", "contract", e))?;
    let mut input_inserted = Vec::with_capacity(facts.len());
    for fact in facts {
        let added = match fact {
            serde_json::Value::String(text) => engine.add_fact(text),
            serde_json::Value::Object(_) => engine.add_fact(fact),
            _ => {
                return Err(failure(
                    "datalog_fact_kind_invalid",
                    "facts",
                    "Only original string or dictionary facts are admitted",
                ));
            }
        }
        .map_err(|e| failure("datalog_fact_failed", "facts", e))?;
        input_inserted.push(added);
    }
    let graph_facts_added = match graph {
        None => 0,
        Some(graph) => engine
            .load_from_graph(&ContextGraphData {
                nodes: graph.nodes.clone(),
                edges: graph.edges.clone(),
            })
            .map_err(|e| failure("datalog_graph_failed", "graph", e))?
            as u64,
    };
    let seed_fact_count = engine.all_facts().len() as u64;
    for rule in rules {
        engine
            .add_rule(rule)
            .map_err(|e| failure("datalog_rule_failed", "rules", e))?;
    }
    engine
        .derive_all()
        .map_err(|e| failure("datalog_inference_failed", "derive", e))?;
    let query_rows = query
        .map(|q| {
            engine
                .query(&q.pattern, Some(&q.bindings))
                .map_err(|e| failure("datalog_query_failed", "query", e))
        })
        .transpose()?;
    if !engine.is_derived() {
        return Err(failure(
            "datalog_completion_missing",
            "result",
            "No fixed point was reached",
        ));
    }
    let stats = engine.last_stats().ok_or_else(|| {
        failure(
            "datalog_completion_missing",
            "result",
            "Completed inference statistics are absent",
        )
    })?;
    Ok(Execution {
        input_inserted,
        graph_facts_added,
        seed_fact_count,
        derived_fact_count: engine.all_facts().len() as u64 - seed_fact_count,
        closure: engine
            .all_facts()
            .iter()
            .map(|f| SemanticDatalogFact {
                predicate: f.predicate.clone(),
                args: f.args.clone(),
            })
            .collect(),
        query_rows,
        iterations: stats.iterations as u64,
        inference_match_attempts: stats.match_attempts as u64,
    })
}

pub(crate) fn request_identity(input: &impl Serialize) -> Result<String, SemanticRuntimeFailure> {
    let value =
        serde_json::to_value(input).map_err(|e| failure("invalid_contract", "contract", e))?;
    let canonical = semantica_json::canonical_json(&value)
        .map_err(|e| failure("invalid_contract", "contract", e))?;
    if canonical.len() > MAX_INPUT_BYTES {
        return Err(failure(
            "datalog_input_byte_budget",
            "contract",
            "Complete canonical request exceeds its bound",
        ));
    }
    Ok(crate::sha256(canonical))
}

impl SemanticDatalogPort for RustSemanticRuntime {
    fn datalog(
        &self,
        input: &SemanticDatalogRequest,
    ) -> Result<SemanticDatalogResult, SemanticRuntimeFailure> {
        check_serialized(input, MAX_INPUT_BYTES, false)?;
        input
            .validate()
            .map_err(|e| failure("invalid_contract", "contract", e.join("; ")))?;
        let request_sha256 = request_identity(input)?;
        let r = execute(
            &input.facts,
            &input.rules,
            input.query.as_ref(),
            &input.budget,
            None,
        )?;
        let response = SemanticDatalogResult {
            schema: "semantica.datalog-result/1".into(),
            profile: PROFILE.into(),
            request_sha256,
            complete: true,
            input_inserted: r.input_inserted,
            seed_fact_count: r.seed_fact_count,
            derived_fact_count: r.derived_fact_count,
            closure: r.closure,
            query_rows: r.query_rows,
            iterations: r.iterations,
            inference_match_attempts: r.inference_match_attempts,
        };
        response
            .validate()
            .map_err(|e| failure("result_contract_invalid", "result", e.join("; ")))?;
        check_serialized(&response, MAX_OUTPUT_BYTES - 1, true)?;
        Ok(response)
    }
}

impl SemanticDatalogGraphPort for RustSemanticRuntime {
    fn datalog_graph(
        &self,
        input: &SemanticDatalogGraphRequest,
    ) -> Result<SemanticDatalogGraphResult, SemanticRuntimeFailure> {
        check_serialized(input, MAX_INPUT_BYTES, false)?;
        input
            .validate()
            .map_err(|e| failure("invalid_contract", "contract", e.join("; ")))?;
        let request_sha256 = request_identity(input)?;
        let r = execute(
            &input.facts,
            &input.rules,
            input.query.as_ref(),
            &input.budget,
            Some(&input.graph),
        )?;
        let response = SemanticDatalogGraphResult {
            schema: "semantica.datalog-graph-result/1".into(),
            profile: "semantica.positive-function-free-datalog-graph/1".into(),
            request_sha256,
            complete: true,
            input_inserted: r.input_inserted,
            graph_node_count: input.graph.nodes.len() as u64,
            graph_edge_count: input.graph.edges.len() as u64,
            graph_facts_added: r.graph_facts_added,
            seed_fact_count: r.seed_fact_count,
            derived_fact_count: r.derived_fact_count,
            closure: r.closure,
            query_rows: r.query_rows,
            iterations: r.iterations,
            inference_match_attempts: r.inference_match_attempts,
        };
        response
            .validate()
            .map_err(|e| failure("result_contract_invalid", "result", e.join("; ")))?;
        check_serialized(&response, MAX_OUTPUT_BYTES - 1, true)?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn full_result_byte_guard_refuses_without_truncation() {
        let value = serde_json::json!({"facts":["a", "b"]});
        let exact = serde_json::to_vec_pretty(&value).unwrap().len();
        assert!(super::check_serialized(&value, exact, true).is_ok());
        let e = super::check_serialized(&value, exact - 1, true).unwrap_err();
        assert_eq!(e.code, "datalog_output_byte_budget");
        assert_eq!(e.phase, "result");
    }
}
