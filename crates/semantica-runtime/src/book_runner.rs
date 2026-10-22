//! Declarative chapter execution on caller-owned staging sessions. The Catalog
//! remains the only discovery source. Receipts observe execution and never
//! authenticate caller-supplied build identity or engineering adoption.
use crate::{
    books::{Catalog, SourceProvider},
    dataset::DatasetSession,
};
use anyhow::{Context, Result, bail, ensure};
use oxigraph::{io::RdfFormat, model::Term};
use semantica_contracts::{
    SemanticDatasetSnapshot, SemanticPackageAsset, SemanticPackageLoadResult,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[path = "providers/python_repr_data.rs"]
mod python_repr_data;

pub const RUNNER_CONTRACT: &str = "semantica.chapter_packages.SemanticPackageRunner/v1";
pub const PROFILE: &str = "semantica.chapter-runner-rust-bounded-observation/1";
const MAX_RECEIPTS: usize = 256;
const MAX_STEPS: usize = 256;

/// Original declared-contract failures have source-owned codes and messages.
/// Backend errors retain the separate safe, static runtime-boundary profile.
#[derive(Debug)]
struct ContractFailure {
    code: &'static str,
    message: String,
}
impl std::fmt::Display for ContractFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for ContractFailure {}
fn require_contract(passed: bool, code: &'static str, message: impl Into<String>) -> Result<()> {
    if !passed {
        return Err(ContractFailure {
            code,
            message: message.into(),
        }
        .into());
    }
    Ok(())
}

/// Original runtime.reason adapter for finite UTF-8 string assets, with its
/// normalization, source-rule details, and explicit convergence probe.
pub fn runtime_reason(facts: &str, rules: &str, max_iterations: u64) -> Result<Value> {
    crate::reasoner::forward_chain_text(facts, rules, max_iterations)
}

fn hash(value: &Value) -> Result<String> {
    Ok(crate::sha256(semantica_contracts::json::canonical_json(
        value,
    )?))
}
fn python_string(value: &Value) -> Result<String> {
    fn repr(value: &Value) -> Result<String> {
        Ok(match value {
            Value::String(text) => {
                let quote = if text.contains('\'') && !text.contains('"') {
                    '"'
                } else {
                    '\''
                };
                let mut output = String::new();
                output.push(quote);
                for c in text.chars() {
                    match c {
                        '\\' => output.push_str("\\\\"),
                        '\n' => output.push_str("\\n"),
                        '\r' => output.push_str("\\r"),
                        '\t' => output.push_str("\\t"),
                        c if c == quote => {
                            output.push('\\');
                            output.push(c);
                        }
                        c if {
                            let code = c as u32;
                            let index = python_repr_data::NONPRINTABLE
                                .partition_point(|(start, _)| *start <= code);
                            index > 0 && python_repr_data::NONPRINTABLE[index - 1].1 >= code
                        } =>
                        {
                            let code = c as u32;
                            output.push_str(&if code <= 255 {
                                format!("\\x{code:02x}")
                            } else if code <= 0xffff {
                                format!("\\u{code:04x}")
                            } else {
                                format!("\\U{code:08x}")
                            });
                        }
                        c => output.push(c),
                    }
                }
                output.push(quote);
                output
            }
            Value::Null => "None".into(),
            Value::Bool(value) => if *value { "True" } else { "False" }.into(),
            Value::Number(_) => {
                String::from_utf8(semantica_contracts::json::canonical_json(value)?)?
            }
            Value::Array(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(repr)
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ),
            Value::Object(values) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(|(key, value)| Ok(format!("{}: {}", repr(&json!(key))?, repr(value)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ),
        })
    }
    if let Value::String(value) = value {
        Ok(value.clone())
    } else {
        repr(value)
    }
}
fn check(id: &str, status: &str, expected: Value, actual: Value, message: &str) -> Value {
    json!({"check_id":id,"status":status,"expected":expected,"actual":actual,"message":message})
}
fn choose(passed: bool) -> &'static str {
    if passed { "passed" } else { "failed" }
}
fn aggregate<'a>(statuses: impl IntoIterator<Item = &'a str>) -> &'static str {
    let all: Vec<_> = statuses.into_iter().collect();
    if all.contains(&"blocked") {
        "blocked"
    } else if all.contains(&"failed") {
        "failed"
    } else {
        "passed"
    }
}
fn typename(value: Option<&Value>) -> &'static str {
    match value {
        None | Some(Value::Null) => "NoneType",
        Some(Value::String(_)) => "str",
        Some(Value::Array(_)) => "list",
        Some(Value::Object(_)) => "dict",
        Some(Value::Bool(_)) => "bool",
        Some(Value::Number(n)) if !n.to_string().contains(['.', 'e', 'E']) => "int",
        Some(_) => "float",
    }
}
fn counter(rows: &[Value]) -> Result<BTreeMap<Vec<u8>, usize>> {
    let mut result = BTreeMap::new();
    for row in rows {
        *result
            .entry(semantica_contracts::json::canonical_json(row)?)
            .or_default() += 1;
    }
    Ok(result)
}

/// Original CQ comparison: variable order is exact, rows are a multiset.
pub fn compare_binding_multiset(declaration: &Value, result: &Value) -> Result<Value> {
    if !declaration.is_object() {
        return Ok(json!([check(
            "oracle.cq_exact",
            "blocked",
            json!({"oracle":"binding_multiset required"}),
            json!({"oracle":"missing"}),
            "CQ binding multiset oracle is missing"
        )]));
    }
    if declaration["kind"] != "binding_multiset" {
        return Ok(json!([check(
            "oracle.cq_exact",
            "blocked",
            json!({"kind":"binding_multiset"}),
            json!({"kind":declaration["kind"]}),
            "CQ oracle kind is unsupported"
        )]));
    }
    let (Some(variables), Some(rows)) = (
        declaration["variables"].as_array(),
        declaration["rows"].as_array(),
    ) else {
        return Ok(json!([check(
            "oracle.cq_exact",
            "blocked",
            json!({"variables":"list","rows":"list"}),
            json!({"variables":typename(declaration.get("variables")),"rows":typename(declaration.get("rows"))}),
            "CQ oracle variables/rows are malformed"
        )]));
    };
    let actual_vars = result["variables"]
        .as_array()
        .context("book_select_variables_invalid")?;
    let actual_rows = result["rows"]
        .as_array()
        .context("book_select_rows_invalid")?
        .iter()
        .map(|row| {
            let mut projected = serde_json::Map::new();
            for name in actual_vars {
                let name = name.as_str().context("book_select_variable_invalid")?;
                if let Some(value) = row.get(name) {
                    projected.insert(name.into(), value.clone());
                }
            }
            Ok(Value::Object(projected))
        })
        .collect::<Result<Vec<_>>>()?;
    let passed = variables == actual_vars && counter(rows)? == counter(&actual_rows)?;
    Ok(json!([check(
        "oracle.cq_exact",
        choose(passed),
        json!({"variables":variables,"rows":rows}),
        json!({"variables":actual_vars,"rows":actual_rows}),
        if passed {
            "exact SELECT variables and binding multiset match"
        } else {
            "SELECT variables or binding multiset differs from the exact oracle"
        }
    )]))
}
pub fn compare_ask_oracle(id: &str, declaration: &Value, result: &Value) -> Result<Value> {
    let expected = declaration.get("boolean").filter(|v| v.is_boolean());
    let Some(expected) = expected else {
        return Ok(check(
            id,
            "blocked",
            json!({"boolean":"required"}),
            json!({"boolean":result["boolean"]}),
            "ASK oracle boolean is missing",
        ));
    };
    let passed = result["boolean"].is_boolean() && result["boolean"] == *expected;
    Ok(check(
        id,
        choose(passed),
        json!({"boolean":expected}),
        json!({"boolean":result["boolean"]}),
        if passed {
            "ASK boolean matches"
        } else {
            "ASK boolean differs"
        },
    ))
}
fn original_integer(value: &Value) -> Option<Value> {
    if let Some(b) = value.as_bool() {
        Some(json!(u8::from(b)))
    } else if value
        .as_number()
        .is_some_and(|v| !v.to_string().contains(['.', 'e', 'E']))
    {
        Some(value.clone())
    } else {
        None
    }
}
pub fn compare_vol1_select_oracle(
    label: &str,
    declaration: &Value,
    result: Option<&Value>,
) -> Result<Value> {
    let id = format!("oracle.{label}");
    if !declaration.is_object() {
        return Ok(json!([check(
            &id,
            "blocked",
            json!({"oracle":"required"}),
            json!({"oracle":"missing"}),
            "SELECT oracle is missing"
        )]));
    }
    let Some(result) = result.filter(|v| v["kind"] == "select") else {
        return Ok(json!([check(
            &id,
            "blocked",
            declaration.clone(),
            json!({"result":"unavailable"}),
            "SELECT result is unavailable"
        )]));
    };
    let rows = result["rows"]
        .as_array()
        .context("book_select_rows_invalid")?;
    let empty = json!({});
    let bindings = declaration.get("bindings").unwrap_or(&empty);
    let mut actual_bindings = serde_json::Map::new();
    let mut expected_bindings = serde_json::Map::new();
    if let Some(bindings) = bindings.as_object() {
        for (name, values) in bindings {
            let mut actual: Vec<Value> = rows
                .iter()
                .filter_map(|row| row.get(name).map(|term| term["value"].clone()))
                .collect();
            ensure!(
                actual.iter().all(|v| v.is_string()),
                "book_select_nonstring_dto_uncovered"
            );
            actual.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            actual_bindings.insert(name.clone(), json!(actual));
            let values: Vec<Value> = match values {
                Value::Array(values) => values.clone(),
                Value::String(text) => text.chars().map(|c| json!(c.to_string())).collect(),
                Value::Object(values) => values.keys().map(|key| json!(key)).collect(),
                _ => bail!("book_oracle_noniterable_bindings"),
            };
            let mut expected = values
                .iter()
                .map(python_string)
                .collect::<Result<Vec<_>>>()?;
            expected.sort();
            expected_bindings.insert(name.clone(), json!(expected));
        }
    }
    let expected = json!({"row_count":declaration["row_count"],"bindings":expected_bindings});
    let actual =
        json!({"row_count":rows.len(),"bindings":actual_bindings,"variables":result["variables"]});
    if original_integer(&declaration["row_count"]).is_none() || !bindings.is_object() {
        return Ok(json!([check(
            &id,
            "blocked",
            declaration.clone(),
            actual,
            "SELECT oracle is malformed"
        )]));
    }
    let passed = original_integer(&declaration["row_count"]) == Some(json!(rows.len()))
        && expected["bindings"] == actual["bindings"];
    Ok(json!([check(
        &id,
        choose(passed),
        expected,
        actual,
        if passed {
            "exact SELECT row count and bindings match"
        } else {
            "SELECT row count or bindings differ from the oracle"
        }
    )]))
}
pub fn compare_validation_oracle(
    label: &str,
    declaration: &Value,
    result: Option<&Value>,
) -> Result<Value> {
    let id = format!("oracle.{label}");
    if !declaration.is_object() {
        return Ok(json!([check(
            &id,
            "blocked",
            json!({"oracle":"required"}),
            json!({"oracle":"missing"}),
            "validation oracle is missing"
        )]));
    }
    let Some(result) = result.filter(|v| v["kind"] == "validation") else {
        return Ok(json!([check(
            &id,
            "blocked",
            declaration.clone(),
            json!({"result":"unavailable"}),
            "SHACL validation result is unavailable"
        )]));
    };
    let mut valid = declaration["conforms"].is_boolean();
    let mut passed =
        valid && result["conforms"].is_boolean() && result["conforms"] == declaration["conforms"];
    let count = declaration
        .get("violation_count")
        .or_else(|| declaration.get("fault_count"));
    let message = declaration
        .get("message_contains")
        .or_else(|| declaration.get("required_message_contains"));
    let messages = result["messages"]
        .as_array()
        .context("book_validation_messages_invalid")?;
    let actual = json!({"conforms":result["conforms"],"violation_count":result["violation_count"],"messages":messages});
    let mut expected = json!({"conforms":declaration["conforms"]});
    if let Some(count) = count.filter(|v| !v.is_null()) {
        expected["violation_count"] = count.clone();
        valid &= original_integer(count).is_some();
        passed &= original_integer(count) == Some(result["violation_count"].clone());
    }
    if let Some(message) = message.filter(|v| !v.is_null()) {
        expected["message_contains"] = message.clone();
        valid &= message.is_string();
        let needle = python_string(message)?;
        passed &= messages
            .iter()
            .any(|v| v.as_str().is_some_and(|v| v.contains(&needle)));
    }
    let status = if !valid { "blocked" } else { choose(passed) };
    Ok(json!([check(
        &id,
        status,
        expected,
        actual,
        if status == "passed" {
            "SHACL result matches the declared oracle"
        } else if status == "blocked" {
            "SHACL oracle is malformed"
        } else {
            "SHACL result differs from the declared oracle"
        }
    )]))
}
pub fn compare_reason_oracle(
    id: &str,
    declaration: &Value,
    result: Option<&Value>,
) -> Result<Value> {
    let Some(declaration) = declaration.as_array() else {
        return Ok(check(
            id,
            "blocked",
            json!({"conclusions":"required list"}),
            json!({"result":"unavailable"}),
            "forward-chain oracle is missing or malformed",
        ));
    };
    let mut expected = declaration
        .iter()
        .map(python_string)
        .collect::<Result<Vec<_>>>()?;
    expected.sort();
    let Some(result) = result.filter(|v| v["kind"] == "reason") else {
        return Ok(check(
            id,
            "blocked",
            json!({"conclusions":expected}),
            json!({"result":"unavailable"}),
            "forward-chain result is unavailable",
        ));
    };
    let mut actual = result["conclusions"]
        .as_array()
        .context("book_reason_conclusions_invalid")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .context("book_reason_nonstring_dto_uncovered")
        })
        .collect::<Result<Vec<_>>>()?;
    actual.sort();
    let passed = expected == actual;
    Ok(check(
        id,
        choose(passed),
        json!({"conclusions":expected}),
        json!({"conclusions":actual}),
        if passed {
            "forward-chain conclusions match exactly"
        } else {
            "forward-chain conclusions differ from the exact oracle"
        },
    ))
}
pub fn compare_boolean(id: &str, declaration: &Value, result: Option<&Value>) -> Result<Value> {
    if !declaration.is_boolean() {
        return Ok(check(
            id,
            "blocked",
            json!({"boolean":"required"}),
            json!({"result":"unavailable"}),
            "boolean oracle is missing",
        ));
    }
    let Some(result) = result.filter(|v| v["kind"] == "ask") else {
        return Ok(check(
            id,
            "blocked",
            json!({"boolean":declaration}),
            json!({"result":"unavailable"}),
            "ASK result is unavailable",
        ));
    };
    let passed = result["boolean"].is_boolean() && result["boolean"] == *declaration;
    Ok(check(
        id,
        choose(passed),
        json!({"boolean":declaration}),
        json!({"boolean":result["boolean"]}),
        if passed {
            "ASK boolean matches"
        } else {
            "ASK boolean differs"
        },
    ))
}

struct RetainedRun {
    receipt: Value,
    snapshot: SemanticDatasetSnapshot,
    declared_release: String,
    scenario_status: String,
    runtime_identity: Value,
}
/// Caller-owned execution proofs. There is no graph, provider registry or
/// deserialization constructor here; only run can create a retained proof.
#[derive(Default)]
pub struct BookRunnerState {
    contexts: BTreeMap<String, RetainedRun>,
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            ContractFailure {
                code: "missing_field",
                message: format!("declared operation is missing {key}"),
            }
            .into()
        })
}
fn asset<'a>(loaded: &'a SemanticPackageLoadResult, id: &str) -> Result<&'a SemanticPackageAsset> {
    loaded
        .assets
        .iter()
        .find(|v| v.asset_id == id)
        .context("missing_asset")
}
fn structured(
    provider: &dyn SourceProvider,
    loaded: &SemanticPackageLoadResult,
    id: &str,
) -> Result<Value> {
    let asset = asset(loaded, id)?;
    let value = provider.metadata_at(&asset.content, id)?;
    ensure!(value.is_object(), "invalid_registry");
    Ok(value)
}
fn contract(value: &Value) -> Result<()> {
    require_contract(
        value == RUNNER_CONTRACT
            || (value.is_object()
                && value["requested_api"] == "SemanticPackageRunner.run_scenario"
                && value["fail_closed"] == true),
        if value.is_string() || value.is_object() {
            "unsupported_runner_contract"
        } else {
            "missing_runner_contract"
        },
        if value.is_string() || value.is_object() {
            "scenario runner contract is unsupported"
        } else {
            "scenario runner contract is missing"
        },
    )
}
fn select_scenario<'a>(
    values: &'a [Value],
    key: &str,
    requested: Option<&str>,
) -> Result<&'a Value> {
    let all: Vec<_> = values.iter().filter(|v| v.is_object()).collect();
    if let Some(id) = requested {
        all.into_iter()
            .find(|v| v[key] == id)
            .context("missing_scenario")
    } else {
        require_contract(
            all.len() == 1,
            "ambiguous_scenario",
            "scenario_id is required when a package has multiple scenarios",
        )?;
        Ok(all[0])
    }
}
fn resolve(
    catalog: &Catalog,
    provider: &dyn SourceProvider,
    loaded: &SemanticPackageLoadResult,
    requested: Option<&str>,
) -> Result<(Value, Value, String, bool)> {
    let manifest = catalog.read_manifest(&loaded.identity.package_id)?;
    if let Some(scenarios) = manifest["scenarios"]
        .as_array()
        .filter(|v| !v.is_empty() && v[0].is_object() && v[0].get("scenario_id").is_some())
    {
        let scenario = select_scenario(scenarios, "scenario_id", requested)?.clone();
        let id = text(&scenario, "scenario_id")?.to_owned();
        let registry = structured(provider, loaded, text(&scenario, "cq_registry_asset")?)?;
        contract(&registry["runner_contract"])?;
        let cq = registry["competency_questions"]
            .as_array()
            .context("missing_cq")?
            .iter()
            .find(|v| v.is_object() && v["scenario_id"] == id)
            .context("missing_cq")?
            .clone();
        return Ok((scenario, cq, id, true));
    }
    let execution = &manifest["execution"];
    ensure!(execution.is_object(), "missing_scenario_registry");
    let registry = structured(
        provider,
        loaded,
        text(execution, "scenario_registry_asset_id")?,
    )?;
    contract(&registry["runner_contract"])?;
    let scenario = select_scenario(
        registry["scenarios"]
            .as_array()
            .context("missing_scenario")?,
        "id",
        requested,
    )?
    .clone();
    let id = text(&scenario, "id")?.to_owned();
    let cqs = structured(provider, loaded, text(execution, "cq_registry_asset_id")?)?;
    let ids: Vec<Value> = cqs["competency_questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| {
            v["scenario_ids"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v == &id))
        })
        .map(|v| v["id"].clone())
        .collect();
    Ok((scenario, json!({"ids":ids}), id, false))
}
fn report(kind: &str, status: &str, payload: Value) -> Result<Value> {
    let value = json!({"kind":kind,"status":status,"payload":payload});
    let sha = hash(&value)?;
    let mut value = value;
    value["sha256"] = json!(sha);
    Ok(value)
}
fn operation_report(id: &str, operation: &str, status: &str, payload: Value) -> Result<Value> {
    let mut value =
        json!({"operation_id":id,"operation":operation,"status":status,"payload":payload});
    value["sha256"] = json!(hash(&value)?);
    Ok(value)
}
#[derive(Clone)]
struct Outcome {
    operation: String,
    value: Option<Value>,
}
fn last<'a>(outcomes: &'a [Outcome], operation: &str) -> Option<&'a Value> {
    outcomes
        .iter()
        .rev()
        .find(|v| v.operation == operation)
        .and_then(|v| v.value.as_ref())
}
fn term(value: &Term) -> Value {
    match value {
        Term::NamedNode(n) => {
            json!({"value":n.as_str(),"term_type":"iri","datatype":null,"language":null})
        }
        Term::BlankNode(n) => {
            json!({"value":n.as_str(),"term_type":"blank_node","datatype":null,"language":null})
        }
        Term::Literal(n) => {
            let datatype = n.datatype().as_str();
            json!({"value":n.value(),"term_type":"literal","datatype":if datatype=="http://www.w3.org/2001/XMLSchema#string"||n.language().is_some(){None}else{Some(datatype)},"language":n.language()})
        }
    }
}
fn execute(
    catalog: &Catalog,
    provider: &dyn SourceProvider,
    loaded: &SemanticPackageLoadResult,
    session: &mut DatasetSession,
    step: &Value,
    required: &mut BTreeSet<String>,
) -> Result<Value> {
    let operation = text(step, "operation")?;
    match operation {
        "load_package" => {
            ensure!(
                step["package_id"] == loaded.identity.package_id
                    && step["version"] == loaded.identity.version,
                "package_identity_mismatch"
            );
            required.insert("semantic.package.load".into());
            Ok(
                json!({"package_id":loaded.identity.package_id,"version":loaded.identity.version,"digest":loaded.identity.digest}),
            )
        }
        "load" | "load_asset" => {
            let asset = asset(loaded, text(step, "asset_id")?)?;
            let format = step["format"]
                .as_str()
                .filter(|v| !v.is_empty())
                .or(asset.format.as_deref());
            let format = crate::packages::rdf_format(format)?;
            let result = session.load(
                &asset.content,
                format,
                None,
                step["graph_name"].as_str().filter(|v| !v.is_empty()),
            )?;
            required.insert("rdf.dataset.load".into());
            Ok(
                json!({"asset_id":asset.asset_id,"asset_sha256":asset.sha256,"format":step["format"].as_str().or(asset.format.as_deref()),"added":result.added,"quad_count":result.quad_count,"revision":result.revision}),
            )
        }
        "select" => {
            let asset = asset(loaded, text(step, "asset_id")?)?;
            ensure!(asset.role == "sparql", "asset_role_mismatch");
            let query = std::str::from_utf8(&asset.content)?;
            let (variables, rows) = crate::graph::select_with_columns(session.dataset(), query)?;
            let rows: Vec<Value> = rows
                .into_iter()
                .map(|row| Value::Object(row.into_iter().map(|(k, v)| (k, term(&v))).collect()))
                .collect();
            required.insert("sparql.select".into());
            Ok(
                json!({"kind":"select","asset_id":asset.asset_id,"asset_sha256":asset.sha256,"variables":variables,"row_count":rows.len(),"rows":rows,"revision":session.revision()}),
            )
        }
        "ask" => {
            let asset = asset(loaded, text(step, "asset_id")?)?;
            ensure!(asset.role == "sparql", "asset_role_mismatch");
            let (kind, value) = crate::graph::query_with_kind(
                session.dataset(),
                std::str::from_utf8(&asset.content)?,
            )?;
            ensure!(kind == "ask", "book_expected_ask");
            let boolean = value["value"]
                .as_bool()
                .context("book_ask_boolean_invalid")?;
            required.insert("sparql.ask".into());
            Ok(
                json!({"kind":"ask","asset_id":asset.asset_id,"asset_sha256":asset.sha256,"boolean":boolean,"revision":session.revision()}),
            )
        }
        "validate" => {
            let asset = asset(loaded, text(step, "asset_id")?)?;
            ensure!(
                ["shapes", "shape", "shacl"].contains(&asset.role.as_str()),
                "asset_role_mismatch"
            );
            ensure!(
                step["inference"].as_str().unwrap_or("none") == "none",
                "book_shacl_inference_uncovered"
            );
            ensure!(
                crate::packages::rdf_format(step["format"].as_str().or(asset.format.as_deref()))?
                    == RdfFormat::Turtle,
                "book_shacl_shapes_format_uncovered"
            );
            let data = session.serialize(RdfFormat::Turtle, None)?;
            let mut value = crate::shacl::validate(data.as_bytes(), &asset.content)?;
            if step["abort_on_first"] == true {
                if let Some(findings) = value["findings"].as_array_mut() {
                    findings.truncate(1);
                }
                value["violation_count"] = json!(value["findings"].as_array().map_or(0, Vec::len));
            }
            let messages: Vec<Value> = value["findings"]
                .as_array()
                .context("book_shacl_findings_invalid")?
                .iter()
                .map(|v| v["message"].clone())
                .collect();
            value["kind"] = json!("validation");
            value["messages"] = json!(messages);
            value["asset_id"] = json!(asset.asset_id);
            value["asset_sha256"] = json!(asset.sha256);
            value["revision"] = json!(session.revision());
            value["inference"] = json!("none");
            value["advanced"] = json!(step["advanced"].as_bool().unwrap_or(false));
            required.insert("shacl.validate".into());
            Ok(value)
        }
        "reason" => {
            let facts = asset(loaded, text(step, "facts_asset_id")?)?;
            let rules = asset(loaded, text(step, "rules_asset_id")?)?;
            ensure!(
                facts.format.as_deref() == Some("semantica-fact")
                    && rules.format.as_deref() == Some("semantica-rule"),
                "unsupported_reasoning_profile"
            );
            let max = step
                .get("max_iterations")
                .map(|v| v.as_u64().context("book_reason_iteration_invalid"))
                .transpose()?
                .unwrap_or(50);
            let mut value = runtime_reason(
                std::str::from_utf8(&facts.content)?,
                std::str::from_utf8(&rules.content)?,
                max,
            )?;
            let conclusions: Vec<_> = value["inferences"]
                .as_array()
                .context("book_inferences_invalid")?
                .iter()
                .map(|v| v["conclusion"].clone())
                .collect();
            value["conclusions"] = json!(conclusions);
            value["kind"] = json!("reason");
            value["facts_asset_id"] = json!(facts.asset_id);
            value["facts_sha256"] = json!(facts.sha256);
            value["rules_asset_id"] = json!(rules.asset_id);
            value["rules_sha256"] = json!(rules.sha256);
            value["semantics"] = json!("semantica.textual-forward-chain.v1");
            value["complete"] = json!(true);
            value["implementation_profile"] =
                json!("semantica.runtime-reason-string-assets-bounded/1");
            required.insert("rule.forward_chain".into());
            Ok(value)
        }
        _ => {
            let _ = (catalog, provider);
            bail!("unsupported_operation")
        }
    }
}
fn steps(
    catalog: &Catalog,
    provider: &dyn SourceProvider,
    loaded: &SemanticPackageLoadResult,
    session: &mut DatasetSession,
    label: &str,
    raw: &Value,
    operations: &mut Vec<Value>,
    required: &mut BTreeSet<String>,
) -> Result<Vec<Outcome>> {
    let raw = raw
        .as_array()
        .filter(|v| !v.is_empty())
        .context("missing_steps")?;
    ensure!(raw.len() <= MAX_STEPS, "book_step_limit");
    let mut outcomes = Vec::new();
    for (index, step) in raw.iter().enumerate() {
        let id = format!("{label}.{:03}", index + 1);
        if !step.is_object() {
            operations.push(operation_report(&id,"invalid","blocked",json!({"error_code":"invalid_step","message":"declared execution step is not a mapping"}))?);
            break;
        }
        let operation = step["operation"].as_str().unwrap_or("");
        let capability = match operation {
            "load_package" => Some("semantic.package.load"),
            "load" | "load_asset" => Some("rdf.dataset.load"),
            "select" => Some("sparql.select"),
            "ask" => Some("sparql.ask"),
            "validate" => Some("shacl.validate"),
            "reason" => Some("rule.forward_chain"),
            _ => None,
        };
        if let Some(capability) = capability {
            required.insert(capability.into());
        }
        match execute(catalog, provider, loaded, session, step, required) {
            Ok(value) => {
                let mut payload = value.clone();
                if let Some(payload) = payload.as_object_mut() {
                    payload.remove("kind");
                }
                operations.push(operation_report(&id, operation, "passed", payload)?);
                outcomes.push(Outcome {
                    operation: operation.into(),
                    value: Some(value),
                });
            }
            Err(error) => {
                // Classification never copies a backend's raw error text into
                // the receipt. Invalid input and unsupported profiles differ.
                let detail = error.to_string();
                let uncovered = detail.contains("_uncovered")
                    || detail.contains("_unsupported")
                    || detail.contains("unsupported_operation")
                    || detail.contains("execution_not_migrated");
                let payload = if let Some(failure) = error.downcast_ref::<ContractFailure>() {
                    json!({"error_code":failure.code,"message":failure.message,"failure_domain":"declared_contract","capability":capability})
                } else {
                    json!({"error_code":"stable_runtime_rejected_operation","message":"declared semantic operation could not be executed","failure_domain":if uncovered{"backend_profile_uncovered"}else{"operation_rejected"},"capability":capability})
                };
                operations.push(operation_report(
                    &id,
                    if operation.is_empty() {
                        "missing"
                    } else {
                        operation
                    },
                    "blocked",
                    payload,
                )?);
                outcomes.push(Outcome {
                    operation: operation.into(),
                    value: None,
                });
                break;
            }
        }
    }
    Ok(outcomes)
}
fn add_checks(checks: &mut Vec<Value>, value: Value) {
    if let Value::Array(values) = value {
        checks.extend(values)
    } else {
        checks.push(value)
    }
}
fn forward_ids(loaded: &SemanticPackageLoadResult, inputs: &[Value]) -> Result<(String, String)> {
    let mut facts = Vec::new();
    let mut rules = Vec::new();
    for input in inputs {
        let asset = asset(loaded, input.as_str().context("book_input_id_uncovered")?)?;
        match asset.format.as_deref() {
            Some("semantica-fact") => facts.push(asset.asset_id.clone()),
            Some("semantica-rule") => rules.push(asset.asset_id.clone()),
            _ => {}
        }
    }
    ensure!(
        facts.len() == 1 && rules.len() == 1,
        "unsupported_reasoning_profile"
    );
    Ok((facts.remove(0), rules.remove(0)))
}
fn execute_scenario(
    catalog: &Catalog,
    provider: &dyn SourceProvider,
    loaded: &SemanticPackageLoadResult,
    session: &mut DatasetSession,
    base: &DatasetSession,
    scenario: &Value,
    cq: &Value,
    vol2: bool,
    operations: &mut Vec<Value>,
    required: &mut BTreeSet<String>,
) -> Result<Vec<Value>> {
    let mut checks = Vec::new();
    if vol2 {
        require_contract(
            cq["execution"].is_object(),
            "missing_execution",
            "CQ execution is missing",
        )?;
        let primary = steps(
            catalog,
            provider,
            loaded,
            session,
            "primary",
            &cq["execution"]["steps"],
            operations,
            required,
        )?;
        let oracles = &cq["oracles"];
        require_contract(
            oracles.is_object(),
            "missing_oracle",
            "CQ exact oracles are missing",
        )?;
        if let Some(result) = last(&primary, "select") {
            add_checks(
                &mut checks,
                compare_binding_multiset(&oracles["cq_exact"], result)?,
            );
        } else if let Some(result) = last(&primary, "ask") {
            checks.push(compare_ask_oracle(
                "oracle.cq_exact",
                &oracles["cq_exact"],
                result,
            )?);
        } else {
            checks.push(check(
                "oracle.cq_exact",
                "blocked",
                oracles["cq_exact"].clone(),
                json!({"query_result":"unavailable"}),
                "CQ execution produced no SELECT or ASK result",
            ));
        }
        for label in ["positive_path", "single_fault_negative"] {
            let declaration = &oracles[label];
            if !declaration.is_object() {
                checks.push(check(
                    &format!("oracle.{label}"),
                    "blocked",
                    json!({"oracle":"required"}),
                    json!({"oracle":"missing"}),
                    &format!("{label} oracle is missing"),
                ));
                continue;
            }
            let raw = if declaration.get("execution").is_some_and(|v| !v.is_null()) {
                declaration["execution"].clone()
            } else {
                let shapes = text(declaration, "shapes_asset")?;
                let mut steps = Vec::new();
                if let Some(dataset) = declaration["dataset_asset"]
                    .as_str()
                    .filter(|v| !v.is_empty())
                {
                    steps.push(json!({"operation":"load_asset","asset_id":dataset}));
                } else {
                    ensure!(
                        declaration["dataset"] == "loaded-package",
                        "missing_dataset"
                    );
                }
                steps.push(json!({"operation":"validate","asset_id":shapes}));
                json!(steps)
            };
            let mut branch = base.clone();
            let outcomes = steps(
                catalog,
                provider,
                loaded,
                &mut branch,
                label,
                &raw,
                operations,
                required,
            )?;
            add_checks(
                &mut checks,
                compare_validation_oracle(label, declaration, last(&outcomes, "validate"))?,
            );
        }
        return Ok(checks);
    }
    let execution = &scenario["execution"];
    let oracle = &scenario["oracle"];
    require_contract(
        execution.is_object(),
        "missing_execution",
        "scenario execution is missing",
    )?;
    require_contract(
        oracle.is_object() && ["ready", "native"].iter().any(|v| oracle["status"] == *v),
        "missing_oracle",
        "scenario has no machine-checkable ready oracle",
    )?;
    require_contract(
        scenario["inputs"].is_array(),
        "missing_inputs",
        "scenario inputs are missing",
    )?;
    let inputs = scenario["inputs"].as_array().expect("checked array");
    match text(execution, "operation")? {
        "select_exact_multiset" | "validate" => {
            let validating = execution["operation"] == "validate";
            require_contract(
                inputs.len() == 2,
                "invalid_inputs",
                if validating {
                    "validate scenario requires two dataset assets"
                } else {
                    "select_exact_multiset requires two dataset assets"
                },
            )?;
            let target = text(
                execution,
                if validating {
                    "shape_asset_id"
                } else {
                    "query_asset_id"
                },
            )?;
            for (index, label) in ["positive", "single_fault_negative"].iter().enumerate() {
                let raw = json!([{"operation":"load_asset","asset_id":inputs[index]},{"operation":if validating{"validate"}else{"select"},"asset_id":target,"advanced":true}]);
                let mut branch = base.clone();
                let active = if index == 0 {
                    &mut *session
                } else {
                    &mut branch
                };
                let outcomes = steps(
                    catalog, provider, loaded, active, label, &raw, operations, required,
                )?;
                add_checks(
                    &mut checks,
                    if validating {
                        compare_validation_oracle(
                            label,
                            &oracle[*label],
                            last(&outcomes, "validate"),
                        )?
                    } else {
                        compare_vol1_select_oracle(
                            label,
                            &oracle[*label],
                            last(&outcomes, "select"),
                        )?
                    },
                );
            }
        }
        "forward_chain" | "composite" => {
            let (facts, rules) = forward_ids(loaded, inputs)?;
            let mut raw =
                vec![json!({"operation":"reason","facts_asset_id":facts,"rules_asset_id":rules})];
            let composite = execution["operation"] == "composite";
            if composite {
                let rdf: Vec<_> = inputs
                    .iter()
                    .filter(|id| {
                        asset(loaded, id.as_str().unwrap_or("")).is_ok_and(|asset| {
                            asset.format.as_deref().is_some_and(|v| {
                                ["turtle", "nt", "xml", "trig", "nquads"].contains(&v)
                            })
                        })
                    })
                    .collect();
                let queries: Vec<_> = inputs
                    .iter()
                    .filter(|id| {
                        asset(loaded, id.as_str().unwrap_or(""))
                            .is_ok_and(|asset| asset.role == "sparql")
                    })
                    .collect();
                ensure!(rdf.len() == 1 && queries.len() == 2, "invalid_inputs");
                raw.push(json!({"operation":"load_asset","asset_id":rdf[0]}));
                for query in queries {
                    raw.push(json!({"operation":"ask","asset_id":query}));
                }
            }
            let outcomes = steps(
                catalog,
                provider,
                loaded,
                session,
                "primary",
                &json!(raw),
                operations,
                required,
            )?;
            if composite {
                let expected = &oracle["expected"];
                require_contract(
                    expected.is_object(),
                    "missing_oracle",
                    "composite oracle is missing",
                )?;
                checks.push(compare_reason_oracle(
                    "oracle.inferred",
                    &expected["inferred"],
                    last(&outcomes, "reason"),
                )?);
                let asks: Vec<_> = outcomes.iter().filter(|v| v.operation == "ask").collect();
                for (index, key) in ["ask_owa_evidence", "ask_cwa_naf"].iter().enumerate() {
                    checks.push(compare_boolean(
                        &format!("oracle.{key}"),
                        &expected[*key],
                        asks.get(index).and_then(|v| v.value.as_ref()),
                    )?);
                }
            } else {
                checks.push(compare_reason_oracle(
                    "oracle.expected_conclusions",
                    &oracle["expected_conclusions"],
                    last(&outcomes, "reason"),
                )?);
            }
        }
        "select_then_cross_package_validate" => {
            require_contract(
                inputs.len() == 1,
                "invalid_inputs",
                "cross-package scenario requires one dataset asset",
            )?;
            let outcomes = steps(
                catalog,
                provider,
                loaded,
                session,
                "primary",
                &json!([{"operation":"load_asset","asset_id":inputs[0]},{"operation":"select","asset_id":text(execution,"query_asset_id")?}]),
                operations,
                required,
            )?;
            add_checks(
                &mut checks,
                compare_vol1_select_oracle("select", &oracle["select"], last(&outcomes, "select"))?,
            );
            let (package, id) = text(execution, "shape_dependency")?
                .rsplit_once(':')
                .context("invalid_dependency")?;
            catalog.load_package(provider, package, session)?;
            let dependency = session.package_identity(
                package,
                catalog.read_manifest(package)?["version"]
                    .as_str()
                    .context("book_dependency_version_invalid")?,
            )?;
            let loaded_dependency = SemanticPackageLoadResult {
                identity: dependency,
                assets: Vec::new(),
                rdf_added: 0,
                revision: session.revision(),
            };
            let asset = session
                .package_asset(package, &loaded_dependency.identity.version, id)?
                .clone();
            let mut loaded_dependency = loaded_dependency;
            loaded_dependency.assets.push(asset);
            let validation = steps(
                catalog,
                provider,
                &loaded_dependency,
                session,
                "cross_package_validation",
                &json!([{"operation":"validate","asset_id":id,"advanced":true}]),
                operations,
                required,
            )?;
            add_checks(
                &mut checks,
                compare_validation_oracle(
                    "validation",
                    &oracle["validation"],
                    last(&validation, "validate"),
                )?,
            );
        }
        _ => bail!("unsupported_operation"),
    }
    Ok(checks)
}

fn release_check(id: &str, passed: bool, message: &str) -> Value {
    json!({"check_id":id,"passed":passed,"message":message})
}
fn verdict(receipt: &Value, checks: Vec<Value>) -> Value {
    let reasons: Vec<_> = checks
        .iter()
        .filter(|v| v["passed"] != true)
        .map(|v| v["check_id"].clone())
        .collect();
    json!({"schema_version":"1.0","status":if reasons.is_empty(){"complete"}else{"blocked"},"receipt_sha256":receipt["receipt_sha256"],"checked_at":receipt["created_at"],"checks":checks,"reasons":reasons})
}
fn verify_retained(
    context: &RetainedRun,
    receipt: &Value,
    current: &SemanticDatasetSnapshot,
    runtime_identity: &Value,
) -> Result<Value> {
    let mut content = receipt.clone();
    let digest = content
        .as_object_mut()
        .and_then(|v| v.remove("receipt_sha256"));
    let intact = digest == Some(json!(hash(&content)?)) && receipt == &context.receipt;
    let mut checks = vec![
        release_check(
            "runner.execution_context",
            true,
            "receipt was produced by this runner instance",
        ),
        release_check(
            "receipt.integrity",
            intact,
            "exact retained receipt and report bytes must match",
        ),
        release_check(
            "dataset.integrity",
            current == &context.snapshot,
            "receipt dataset must match the execution session",
        ),
        release_check(
            "runtime.observation",
            runtime_identity == &context.runtime_identity,
            "observed runtime identity must match the executing boundary",
        ),
        release_check(
            "runtime.origin",
            false,
            "caller-supplied build fields are observations; authenticated compilation origin remains unknown",
        ),
        release_check(
            "package.source_authenticity",
            false,
            "matching declared asset hashes does not authenticate source origin",
        ),
        release_check(
            "package.declared_release_status",
            context.declared_release == "complete",
            "manifest release_status must be complete",
        ),
        release_check(
            "runner.scenario_execution",
            context.scenario_status == "passed",
            "scenario execution and exact oracle checks must pass",
        ),
    ];
    for kind in ["capability", "cq", "shacl", "oracle"] {
        let report = &receipt[format!("{kind}_report")];
        let mut content = report.clone();
        let digest = content.as_object_mut().and_then(|v| v.remove("sha256"));
        checks.push(release_check(
            &format!("report.{kind}"),
            report["kind"] == kind
                && report["status"] == "passed"
                && digest == Some(json!(hash(&content)?)),
            "required reports must have the exact kind, intact content hash, and passed status",
        ));
    }
    Ok(verdict(receipt, checks))
}
/// Execute a registry-bound scenario on an empty, isolated caller-owned staging
/// session. Branches clone the registered base using the same DatasetSession
/// algorithm. No project session is cleared, restored, or silently replaced.
pub fn run(
    state: &mut BookRunnerState,
    catalog: &Catalog,
    provider: &dyn SourceProvider,
    session: &mut DatasetSession,
    package_id: &str,
    scenario_id: Option<&str>,
    runtime_identity: &Value,
) -> Result<Value> {
    ensure!(
        session.quad_count() == 0 && session.revision() == 0 && session.packages().is_empty(),
        "book_runner_requires_empty_staging_session"
    );
    ensure!(
        state.contexts.len() < MAX_RECEIPTS,
        "book_runner_receipt_limit"
    );
    let manifest = catalog.read_manifest(package_id)?.clone();
    let loaded_value = catalog.load_package(provider, package_id, session)?;
    let loaded: SemanticPackageLoadResult = serde_json::from_value(loaded_value)?;
    let base = session.clone();
    let mut operations = Vec::new();
    let mut required = BTreeSet::from_iter(
        [
            "semantic.package.load",
            "rdf.dataset.snapshot",
            "semantic.package.receipt",
            "provenance.bundle",
            "semantic.release.verify",
        ]
        .map(str::to_owned),
    );
    let mut selected = scenario_id.unwrap_or("unresolved").to_owned();
    let mut cq_ids = json!([]);
    let mut contract_status = "passed";
    let mut contract_reason = Value::Null;
    let executed = (|| {
        let (scenario, cq, id, vol2) = resolve(catalog, provider, &loaded, scenario_id)?;
        selected = id;
        cq_ids = if cq["id"].is_string() {
            json!([cq["id"]])
        } else {
            cq["ids"].clone()
        };
        execute_scenario(
            catalog,
            provider,
            &loaded,
            session,
            &base,
            &scenario,
            &cq,
            vol2,
            &mut operations,
            &mut required,
        )
    })();
    let mut checks = match executed {
        Ok(checks) => checks,
        Err(error) => {
            contract_status = "blocked";
            if let Some(failure) = error.downcast_ref::<ContractFailure>() {
                contract_reason = json!(failure.message);
                vec![check(
                    "oracle.contract",
                    "blocked",
                    json!({"runner_contract":RUNNER_CONTRACT}),
                    json!({"error_code":failure.code}),
                    &failure.message,
                )]
            } else {
                contract_reason =
                    json!("scenario execution was rejected at the stable runtime boundary");
                vec![check(
                    "oracle.execution",
                    "blocked",
                    json!({"executable":true}),
                    json!({"error_code":"stable_runtime_rejected_execution"}),
                    "scenario execution was rejected at the stable runtime boundary",
                )]
            }
        }
    };
    if checks.is_empty() {
        checks.push(check(
            "oracle.required",
            "blocked",
            json!({"declared_exact_oracle":true}),
            json!({"declared_exact_oracle":false}),
            "scenario has no machine-checkable exact oracle",
        ));
    }
    let operation_status = aggregate(
        operations
            .iter()
            .map(|v| v["status"].as_str().unwrap_or("blocked")),
    );
    let cq_status = aggregate([contract_status, operation_status]);
    let validation: Vec<_> = operations
        .iter()
        .filter(|v| v["operation"] == "validate")
        .cloned()
        .collect();
    let shacl_status = if validation.is_empty() {
        if cq_status == "blocked" {
            "blocked"
        } else {
            "passed"
        }
    } else {
        aggregate(
            validation
                .iter()
                .map(|v| v["status"].as_str().unwrap_or("blocked")),
        )
    };
    let oracle_status = aggregate(
        checks
            .iter()
            .map(|v| v["status"].as_str().unwrap_or("blocked")),
    );
    let status = aggregate([cq_status, shacl_status, oracle_status]);
    let hashes: BTreeMap<_, _> = operations
        .iter()
        .map(|v| {
            (
                v["operation_id"].as_str().unwrap().to_owned(),
                v["sha256"].clone(),
            )
        })
        .collect();
    let cq_report = report(
        "cq",
        cq_status,
        json!({"scenario_id":selected,"competency_question_ids":cq_ids,"status":cq_status,"operation_hashes":hashes,"reason":contract_reason}),
    )?;
    let shacl_report = report(
        "shacl",
        shacl_status,
        json!({"scenario_id":selected,"applicable":!validation.is_empty(),"operations":validation}),
    )?;
    let oracle_report = report(
        "oracle",
        oracle_status,
        json!({"scenario_id":selected,"checks":checks}),
    )?;
    let mut unknown = BTreeSet::from([
        "provenance.bundle".to_owned(),
        "semantic.release.verify".to_owned(),
    ]);
    for operation in &operations {
        if operation["payload"]["failure_domain"] == "backend_profile_uncovered" {
            if let Some(capability) = operation["payload"]["capability"].as_str() {
                unknown.insert(capability.into());
            }
        }
    }
    let capability_report = report(
        "capability",
        "blocked",
        json!({"profile":PROFILE,"required":required,"unknown":unknown,"missing":[],"supported_profiles":{"semantic.package.load":"actual-byte manifest/asset identity; source authenticity unknown","rdf.dataset.load":"bounded Turtle/NTriples/RDFXML/TriG/NQuads; JSONLD/TriX execution uncovered","sparql.select":"bounded local SELECT; no SERVICE/remote dataset","sparql.ask":"bounded local ASK; same graph result encoding","shacl.validate":crate::shacl::supported(),"rule.forward_chain":"original string-asset normalization, same-pass chaining, premise merge, explanation and convergence; <=1000 passes and reasoner resource limits; Python Rule objects/Unicode variable IDs uncovered","rdf.dataset.snapshot":"same bounded DatasetSession RDFC10 algorithm","semantic.package.receipt":"actual content-hashed execution observation"},"unknown_profiles":{"provenance.bundle":"execution-binding observation exists; complete original PROV bundle model is not migrated","semantic.release.verify":"local content/instance/report/declaration checks exist; authenticated compilation/source origin and full original release model remain unknown"},"operation_rejections":operations.iter().filter(|v|v["status"]=="blocked").map(|v|json!({"operation_id":v["operation_id"],"failure_domain":v["payload"]["failure_domain"],"capability":v["payload"]["capability"]})).collect::<Vec<_>>() }),
    )?;
    let snapshot = session.snapshot()?;
    let asset_hashes: BTreeMap<_, _> = loaded
        .assets
        .iter()
        .map(|v| (v.asset_id.clone(), v.sha256.clone()))
        .collect();
    let contracts: Vec<_> = loaded
        .assets
        .iter()
        .filter(|v| v.role == "chapter_contract")
        .collect();
    let contract_hash = if contracts.len() == 1 {
        contracts[0].sha256.as_str()
    } else {
        ""
    };
    let mut receipt = json!({"schema_version":"1.0","receipt_profile":PROFILE,"created_at":runtime_identity.get("created_at").unwrap_or(&Value::Null),"runtime_version":runtime_identity.get("runtime_version").unwrap_or(&Value::Null),"runtime_commit":runtime_identity.get("runtime_commit").unwrap_or(&Value::Null),"runtime_artifact_sha256":runtime_identity.get("runtime_artifact_sha256").unwrap_or(&Value::Null),"runtime_identity_observation":runtime_identity,"runtime_origin":"unknown","source_authenticity":"unknown","package_id":loaded.identity.package_id,"package_version":loaded.identity.version,"package_digest":loaded.identity.digest,"asset_hashes":asset_hashes,"chapter_contract_sha256":contract_hash,"dataset_sha256":snapshot.dataset_sha256,"dataset_quad_count":snapshot.quad_count,"dataset_revision":snapshot.revision,"snapshot_profile":snapshot.canonicalization,"capability_report":capability_report,"cq_report":cq_report,"shacl_report":shacl_report,"oracle_report":oracle_report,"output_hashes":hashes,"provenance_bundle":{"profile":"semantica.chapter-execution-bindings-observation/1","status":"observed","package_digest":loaded.identity.digest,"dataset_sha256":snapshot.dataset_sha256,"report_hashes":{"cq":cq_report["sha256"],"shacl":shacl_report["sha256"],"oracle":oracle_report["sha256"]},"engineering_adoption":"unknown"}});
    receipt["receipt_sha256"] = json!(hash(&receipt)?);
    let context = RetainedRun {
        receipt: receipt.clone(),
        snapshot: snapshot.clone(),
        declared_release: manifest["release_status"].as_str().unwrap_or("").into(),
        scenario_status: status.into(),
        runtime_identity: runtime_identity.clone(),
    };
    let verdict = verify_retained(&context, &receipt, &snapshot, runtime_identity)?;
    let reasons = verdict["reasons"].clone();
    let result = json!({"schema_version":"1.0","runner_contract":RUNNER_CONTRACT,"execution_profile":PROFILE,"package_id":loaded.identity.package_id,"package_version":loaded.identity.version,"package_digest":loaded.identity.digest,"scenario_id":selected,"status":status,"created_at":receipt["created_at"],"operations":operations,"oracle_checks":checks,"capability_report":capability_report,"cq_report":cq_report,"shacl_report":shacl_report,"oracle_report":oracle_report,"receipt":receipt,"release_verdict":verdict,"reasons":reasons});
    state.contexts.insert(
        result["receipt"]["receipt_sha256"].as_str().unwrap().into(),
        context,
    );
    Ok(result)
}
/// Re-verify only an actual receipt produced by this state instance. A matching
/// hash supplied by a caller cannot create an execution capability.
pub fn verify(
    state: &BookRunnerState,
    session: &mut DatasetSession,
    value: &Value,
    runtime_identity: &Value,
) -> Result<Value> {
    let receipt = value.get("receipt").unwrap_or(value);
    let sha = receipt["receipt_sha256"].as_str().unwrap_or("");
    let Some(context) = state.contexts.get(sha) else {
        return Ok(verdict(
            &json!({"receipt_sha256":if sha.is_empty(){"0".repeat(64)}else{sha.into()},"created_at":receipt["created_at"]}),
            vec![release_check(
                "runner.execution_context",
                false,
                "receipt was not produced by this runner instance",
            )],
        ));
    };
    let current = session.snapshot()?;
    verify_retained(context, receipt, &current, runtime_identity)
}

// Existing runner entry points and their original assertions remain unchanged.
pub(crate) fn python_value_string(value: &Value) -> Result<String> {
    python_string(value)
}
pub(crate) fn python_value_repr(value: &Value) -> Result<String> {
    let wrapped = python_string(&json!([value]))?;
    Ok(wrapped[1..wrapped.len() - 1].into())
}

/// Produce a full storage-free execution bundle from this instance's actual
/// retained run. Ordinary JSON cannot insert an execution into BookRunnerState.
/// Rust source observations remain separate from authenticated compilation,
/// package authenticity, release authority and engineering adoption.
pub fn execution_provenance(
    state: &BookRunnerState,
    session: &mut DatasetSession,
    value: &Value,
    runtime_identity: &Value,
) -> Result<semantica_contracts::SemanticExecutionProvenanceObservation> {
    use crate::execution_provenance as prov;
    let receipt = value.get("receipt").unwrap_or(value);
    let context = state
        .contexts
        .get(receipt["receipt_sha256"].as_str().unwrap_or(""))
        .context("execution_provenance_producer_not_retained")?;
    ensure!(
        receipt == &context.receipt,
        "execution_provenance_retained_receipt_changed"
    );
    ensure!(
        runtime_identity == &context.runtime_identity,
        "execution_provenance_runtime_observation_changed"
    );
    let current = session.snapshot()?;
    ensure!(
        current == context.snapshot,
        "execution_provenance_current_dataset_changed"
    );
    let source = runtime_identity["runtime_commit"]
        .as_str()
        .and_then(|s| s.strip_prefix("semantica.rust-source/"))
        .context("execution_provenance_requires_rust_source_profile")?;
    ensure!(
        source.len() == 64
            && source
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "execution_provenance_rust_source_digest_invalid"
    );
    ensure!(
        receipt["created_at"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty()),
        "execution_provenance_timestamp_missing"
    );
    let mut input = receipt.clone();
    let mut capability = input["capability_report"]["payload"].clone();
    if let Some(unknown) = capability["unknown"].as_array_mut() {
        unknown.retain(|v| v != "provenance.bundle");
    }
    if let Some(unknown) = capability["unknown_profiles"].as_object_mut() {
        unknown.remove("provenance.bundle");
    }
    capability["supported_profiles"]["provenance.bundle"] = json!(
        "complete original storage-free Entry/Bundle checksum, insertion chain, canonical content and receipt binding; manager storage and release authority separate"
    );
    input["capability_report"] = serde_json::to_value(prov::execution_report(
        "capability",
        &capability,
        receipt["capability_report"]["status"]
            .as_str()
            .context("execution_provenance_capability_status_missing")?,
    )?)?;
    let full = prov::build_execution_receipt(&input)?;
    let raw = serde_json::to_value(&full)?;
    ensure!(
        prov::verify_receipt(&raw)? && prov::verify_receipt_bindings(&raw)?,
        "execution_provenance_produced_receipt_invalid"
    );
    Ok(
        semantica_contracts::SemanticExecutionProvenanceObservation {
            profile: prov::CURRENT_PROFILE.into(),
            receipt: full,
            snapshot_profile: current.canonicalization,
            runtime_origin: "unknown".into(),
            source_authenticity: "unknown".into(),
            engineering_adoption: "unknown".into(),
            release_status: "blocked".into(),
        },
    )
}

pub const RELEASE_CONTENT_PROFILE: &str = "semantica.release-content-retained-rust/1";

/// Check content produced by the same retained execution and current Dataset.
/// Neither a complete content verdict nor observed source hashes publish a
/// release, authenticate compilation or authorize engineering adoption.
pub fn release_content(
    state: &BookRunnerState,
    session: &mut DatasetSession,
    value: &Value,
    runtime_identity: &Value,
    source: &Value,
) -> Result<semantica_contracts::SemanticReleaseContentObservation> {
    use crate::{execution_provenance as prov, release_validation as release};
    let observation = execution_provenance(state, session, value, runtime_identity)?;
    let original = value.get("receipt").unwrap_or(value);
    let source_run_receipt_sha256 = original["receipt_sha256"]
        .as_str()
        .context("release_content_source_receipt_missing")?;
    let context = state
        .contexts
        .get(source_run_receipt_sha256)
        .context("release_content_execution_not_retained")?;
    let runtime_source: semantica_contracts::SemanticRefineryRustSource =
        serde_json::from_value(source.clone())?;
    runtime_source.validate().map_err(|errors| {
        anyhow::anyhow!(
            "release_content_source_observation_invalid: {}",
            errors.join("; ")
        )
    })?;

    let mut input = serde_json::to_value(observation.receipt)?;
    let mut capability = input["capability_report"]["payload"].clone();
    capability["profile"] = json!(RELEASE_CONTENT_PROFILE);
    capability["unknown"]
        .as_array_mut()
        .context("release_content_unknown_capabilities_missing")?
        .retain(|v| v != "semantic.release.verify");
    capability["unknown_profiles"]
        .as_object_mut()
        .context("release_content_unknown_profiles_missing")?
        .remove("semantic.release.verify");
    capability["supported_profiles"]["semantic.release.verify"] = json!(
        "original fifteen content checks plus retained manifest/scenario checks; current Rust source observation and RDFC snapshot; compilation authentication, full library release and engineering adoption separate"
    );
    let known = capability["unknown"]
        .as_array()
        .context("release_content_unknown_capabilities_missing")?
        .is_empty()
        && capability["missing"]
            .as_array()
            .context("release_content_missing_capabilities_missing")?
            .is_empty();
    input["capability_report"] = serde_json::to_value(prov::execution_report(
        "capability",
        &capability,
        if known { "passed" } else { "blocked" },
    )?)?;
    let package = session
        .registered_package(
            input["package_id"]
                .as_str()
                .context("release_content_package_missing")?,
            input["package_version"]
                .as_str()
                .context("release_content_version_missing")?,
        )
        .context("release_content_package_not_registered")?;
    input["chapter_contract_sha256"] =
        json!(release::chapter_contract_hash(&package.assets).unwrap_or(""));
    let receipt = prov::build_execution_receipt(&input)?;
    let raw = serde_json::to_value(&receipt)?;
    let checked_at = original["created_at"]
        .as_str()
        .context("release_content_timestamp_missing")?;
    let content_verdict = release::with_runner_checks(
        release::verify_rust_content(
            &raw,
            session,
            &json!(RELEASE_CONTENT_PROFILE),
            checked_at,
            source,
        )?,
        &context.declared_release,
        context.scenario_status == "passed",
    )?;
    Ok(semantica_contracts::SemanticReleaseContentObservation {
        profile: RELEASE_CONTENT_PROFILE.into(),
        source_run_receipt_sha256: source_run_receipt_sha256.into(),
        receipt,
        content_verdict,
        runtime_source,
        snapshot_profile: observation.snapshot_profile,
        runner_execution: "retained".into(),
        runtime_origin: "unknown".into(),
        source_authenticity: "unknown".into(),
        engineering_adoption: "unknown".into(),
        formal_release: "not_published".into(),
    })
}

/// Rebuild from the private retained run rather than accepting caller checks,
/// hashes, a source observation or a serialized `runner_execution` claim.
pub fn verify_release_content(
    state: &BookRunnerState,
    session: &mut DatasetSession,
    observation: &semantica_contracts::SemanticReleaseContentObservation,
    runtime_identity: &Value,
    source: &Value,
) -> Result<()> {
    let context = state
        .contexts
        .get(&observation.source_run_receipt_sha256)
        .context("release_content_execution_not_retained")?;
    let expected = release_content(state, session, &context.receipt, runtime_identity, source)?;
    ensure!(
        observation == &expected,
        "release_content_retained_observation_changed"
    );
    Ok(())
}
