//! Bounded action histories over one existing DatalogReasoner per request.
//! Source operation order is explicit; no cross-request storage or Project writes.
use crate::datalog::{ContextGraphData, DatalogReasoner};
use crate::datalog_port::{
    MAX_INPUT_BYTES, MAX_OUTPUT_BYTES, check_serialized, failure, limits, request_identity,
};
use crate::runtime::RustSemanticRuntime;
use semantica_contracts::{
    SemanticDatalogAction, SemanticDatalogFact, SemanticDatalogSessionPort,
    SemanticDatalogSessionRequest, SemanticDatalogSessionResult, SemanticDatalogState,
    SemanticDatalogStep, SemanticRuntimeFailure,
};
use std::collections::BTreeSet;

pub const MAX_ACTIONS: usize = 32;
const MAX_RECORDS: usize = 4096;
const MAX_RULES: usize = 256;

fn state(engine: &DatalogReasoner) -> SemanticDatalogState {
    SemanticDatalogState {
        facts: engine
            .all_facts()
            .iter()
            .map(|f| SemanticDatalogFact {
                predicate: f.predicate.clone(),
                args: f.args.clone(),
            })
            .collect(),
        rule_count: engine.rules().len() as u64,
        derived: engine.is_derived(),
    }
}

fn admit(input: &SemanticDatalogSessionRequest) -> Result<(), SemanticRuntimeFailure> {
    check_serialized(input, MAX_INPUT_BYTES, false)?;
    input
        .validate()
        .map_err(|e| failure("invalid_contract", "contract", e.join("; ")))?;
    if input.actions.len() > MAX_ACTIONS {
        return Err(failure(
            "datalog_session_action_budget",
            "contract",
            "At most32 actions",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut records = 0usize;
    let mut rules = 0usize;
    for action in &input.actions {
        action
            .validate()
            .map_err(|e| failure("invalid_contract", "contract", e.join("; ")))?;
        if action.action_id.is_empty()
            || action.action_id.len() > 64
            || !action
                .action_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
            || !ids.insert(&action.action_id)
        {
            return Err(failure(
                "datalog_session_action_id",
                "contract",
                "Action ids must be unique ASCII identifiers of1..64 bytes",
            ));
        }
        let present = [
            action.fact.is_some(),
            action.rule.is_some(),
            action.graph.is_some(),
            action.query.is_some(),
        ];
        let expected = match action.operation.as_str() {
            "add_fact" => [true, false, false, false],
            "add_rule" => [false, true, false, false],
            "load_graph" => [false, false, true, false],
            "query" => [false, false, false, true],
            "derive" | "clear" | "snapshot" => [false; 4],
            _ => {
                return Err(failure(
                    "datalog_session_operation",
                    "contract",
                    "Unknown action",
                ));
            }
        };
        if present != expected {
            return Err(failure(
                "datalog_session_action_payload",
                "contract",
                "Only the declared operation payload is admitted",
            ));
        }
        if let Some(fact) = &action.fact {
            if !fact.is_string() && !fact.is_object() {
                return Err(failure(
                    "datalog_fact_kind_invalid",
                    "contract",
                    "Only string or dictionary facts",
                ));
            }
            records = records.saturating_add(1);
        }
        if action.rule.is_some() {
            rules = rules.saturating_add(1);
        }
        if let Some(graph) = &action.graph {
            if graph
                .nodes
                .iter()
                .chain(&graph.edges)
                .any(|v| !v.is_object())
            {
                return Err(failure(
                    "datalog_graph_record_invalid",
                    "contract",
                    "Only dictionary graph records",
                ));
            }
            records = records
                .saturating_add(graph.nodes.len())
                .saturating_add(graph.edges.len());
        }
    }
    if records > MAX_RECORDS || rules > MAX_RULES {
        return Err(failure(
            "datalog_session_input_count_budget",
            "contract",
            "Aggregate records/rules exceed4096/256; clear does not reset admission counts",
        ));
    }
    Ok(())
}

fn execute_action(
    engine: &mut DatalogReasoner,
    action: &SemanticDatalogAction,
    step: &mut SemanticDatalogStep,
) -> anyhow::Result<()> {
    match action.operation.as_str() {
        "add_fact" => {
            let fact = action.fact.as_ref().expect("admitted fact");
            step.inserted = Some(match fact.as_str() {
                Some(text) => engine.add_fact(text)?,
                None => engine.add_fact(fact)?,
            });
        }
        "add_rule" => engine.add_rule(action.rule.as_deref().expect("admitted rule"))?,
        "load_graph" => {
            let graph = action.graph.as_ref().expect("admitted graph");
            step.graph_facts_added = Some(engine.load_from_graph_incremental(&ContextGraphData {
                nodes: graph.nodes.clone(),
                edges: graph.edges.clone(),
            })? as u64);
        }
        "derive" => {
            engine.derive_all()?;
        }
        "query" => {
            let query = action.query.as_ref().expect("admitted query");
            step.query_rows =
                Some(engine.query_source_order(&query.pattern, Some(&query.bindings))?);
        }
        "clear" => engine.clear(),
        "snapshot" => (),
        _ => unreachable!("admitted operation"),
    }
    Ok(())
}

impl SemanticDatalogSessionPort for RustSemanticRuntime {
    fn datalog_session(
        &self,
        input: &SemanticDatalogSessionRequest,
    ) -> Result<SemanticDatalogSessionResult, SemanticRuntimeFailure> {
        admit(input)?;
        let mut engine = DatalogReasoner::with_limits(limits(&input.budget)?)
            .map_err(|e| failure("datalog_invalid_budget", "contract", e))?;
        let mut result = SemanticDatalogSessionResult {
            schema: "semantica.datalog-session-result/1".into(),
            profile: "semantica.bounded-source-datalog-session/1".into(),
            request_sha256: request_identity(input)?,
            steps: Vec::with_capacity(input.actions.len()),
            final_state: state(&engine),
            all_actions_succeeded: true,
        };
        let mut stopped = false;
        for action in &input.actions {
            let mut step = SemanticDatalogStep {
                action_id: action.action_id.clone(),
                operation: action.operation.clone(),
                status: if stopped { "skipped" } else { "completed" }.into(),
                state_before: state(&engine),
                state_after: state(&engine),
                inserted: None,
                graph_facts_added: None,
                query_rows: None,
                failure: None,
            };
            if !stopped {
                if let Err(error) = execute_action(&mut engine, action, &mut step) {
                    step.status = "failed".into();
                    step.failure = Some(failure(
                        "datalog_session_action_failed",
                        &action.operation,
                        error,
                    ));
                    result.all_actions_succeeded = false;
                    stopped = input.on_error == "stop";
                }
            }
            step.state_after = state(&engine);
            result.final_state = step.state_after.clone();
            result.steps.push(step);
            // Bound the complete accumulated history, including final state and newline.
            // Oversize histories fail as an envelope; they are never truncated or persisted.
            check_serialized(&result, MAX_OUTPUT_BYTES - 1, true)?;
        }
        result
            .validate()
            .map_err(|e| failure("result_contract_invalid", "result", e.join("; ")))?;
        check_serialized(&result, MAX_OUTPUT_BYTES - 1, true)?;
        Ok(result)
    }
}
