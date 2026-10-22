//! Execute immutable v1 assets with their unchanged, independently stored oracles.
use anyhow::{Context, Result, bail, ensure};
use oe_semantic::{LoadedBundle, load_bundle};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn asset<'a>(bundle: &'a LoadedBundle, id: &str) -> Result<&'a [u8]> {
    let a = bundle.manifest["assets"]
        .as_array()
        .context("package_assets_missing")?
        .iter()
        .find(|a| a["asset_id"] == id)
        .context("package_asset_unknown")?;
    bundle
        .payload
        .get(a["path"].as_str().context("asset_path_missing")?)
        .map(Vec::as_slice)
        .context("asset_bytes_missing")
}
fn base(bundle: &LoadedBundle) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for a in bundle.manifest["assets"]
        .as_array()
        .context("package_assets_missing")?
    {
        if a["load_into_dataset"] == true {
            ensure!(a["kind"] == "rdf", "unsupported_loaded_asset_kind");
            out.extend_from_slice(asset(
                bundle,
                a["asset_id"].as_str().context("asset_id_missing")?,
            )?);
            out.push(b'\n');
        }
    }
    Ok(out)
}
pub(crate) fn lexical(term: &oxigraph::model::Term) -> String {
    match term {
        oxigraph::model::Term::NamedNode(n) => n.as_str().into(),
        oxigraph::model::Term::BlankNode(n) => n.as_str().into(),
        oxigraph::model::Term::Literal(l) => l.value().into(),
    }
}
fn expected_text(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => bail!("oracle_value_unsupported"),
    }
}
pub(crate) fn matches(actual: &Value, expected: &Value) -> Result<bool> {
    ensure!(expected.is_object(), "oracle_expectation_invalid");
    for (k, v) in expected.as_object().expect("checked") {
        if k == "bindings" {
            let actual = actual["bindings"]
                .as_object()
                .context("query_bindings_missing")?;
            for (column, values) in v.as_object().context("oracle_bindings_invalid")? {
                let mut expected = values
                    .as_array()
                    .context("oracle_multiset_invalid")?
                    .iter()
                    .map(expected_text)
                    .collect::<Result<Vec<_>>>()?;
                expected.sort();
                let mut got = actual
                    .get(column)
                    .and_then(Value::as_array)
                    .context("query_column_missing")?
                    .iter()
                    .map(expected_text)
                    .collect::<Result<Vec<_>>>()?;
                got.sort();
                if got != expected {
                    return Ok(false);
                }
            }
        } else if k == "message_contains" {
            let needle = v.as_str().context("oracle_message_invalid")?;
            if !actual["findings"]
                .as_array()
                .context("shacl_findings_missing")?
                .iter()
                .any(|f| f["message"].as_str().is_some_and(|m| m.contains(needle)))
            {
                return Ok(false);
            }
        } else {
            ensure!(
                ["row_count", "conforms", "violation_count"].contains(&k.as_str()),
                "oracle_key_unsupported:{k}"
            );
            if actual[k] != *v {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn execute_input(bundle: &LoadedBundle, scenario: &Value, input: &str) -> Result<Value> {
    let mut data = base(bundle)?;
    data.extend_from_slice(asset(bundle, input)?);
    let operation = scenario["execution"]["operation"]
        .as_str()
        .context("scenario_operation_missing")?;
    match operation {
        "select_exact_multiset" => {
            let id = scenario["execution"]["query_asset_id"]
                .as_str()
                .context("scenario_query_missing")?;
            let query = std::str::from_utf8(asset(bundle, id)?)?;
            let store = crate::graph::load(&data)?;
            let (variables, rows) = crate::graph::select_with_columns(&store, query)?;
            let mut columns: BTreeMap<String, Vec<String>> =
                variables.into_iter().map(|v| (v, Vec::new())).collect();
            for row in &rows {
                for (k, v) in row {
                    columns.entry(k.clone()).or_default().push(lexical(v));
                }
            }
            Ok(
                json!({"row_count":rows.len(),"bindings":columns,"terms":rows.iter().map(|r|r.iter().map(|(k,v)|(k.clone(),crate::graph::term_value(v))).collect::<BTreeMap<_,_>>()).collect::<Vec<_>>()}),
            )
        }
        "validate" => {
            let id = scenario["execution"]["shape_asset_id"]
                .as_str()
                .context("scenario_shape_missing")?;
            crate::shacl::validate(&data, asset(bundle, id)?)
        }
        _ => bail!("scenario_operation_unsupported:{operation}"),
    }
}
fn execute_scenario(bundle: &LoadedBundle, s: &Value) -> Result<Value> {
    ensure!(s["oracle"]["status"] == "ready", "oracle_not_ready");
    let inputs = s["inputs"].as_array().context("scenario_inputs_missing")?;
    ensure!(inputs.len() == 2, "scenario_input_arity_unsupported");
    if s["execution"]["operation"] == "forward_chain" {
        let a = inputs[0].as_str().context("input_id_invalid")?;
        let b = inputs[1].as_str().context("input_id_invalid")?;
        let result = crate::rules::forward_chain(
            std::str::from_utf8(asset(bundle, a)?)?,
            std::str::from_utf8(asset(bundle, b)?)?,
        )?;
        let mut expected = s["oracle"]["expected_conclusions"]
            .as_array()
            .context("rule_oracle_missing")?
            .clone();
        expected.sort_by_key(Value::to_string);
        let mut got = result["conclusions"]
            .as_array()
            .context("rule_conclusions_missing")?
            .clone();
        got.sort_by_key(Value::to_string);
        return Ok(
            json!({"regression":if expected==got{"pass"}else{"failed"},"result":result,"oracle":s["oracle"],"inputs":crate::source_identities(&[(a.into(),asset(bundle,a)?.to_vec()),(b.into(),asset(bundle,b)?.to_vec())].into_iter().collect())}),
        );
    }
    let mut cases = Vec::new();
    for (index, key) in ["positive", "single_fault_negative"]
        .into_iter()
        .enumerate()
    {
        let input = inputs[index].as_str().context("input_id_invalid")?;
        let result = execute_input(bundle, s, input)?;
        let okay = matches(&result, &s["oracle"][key])?;
        cases.push(json!({"role":key,"input_id":input,"input_sha256":oe_core::sha256(asset(bundle,input)?),"expected":s["oracle"][key],"regression":if okay{"pass"}else{"failed"},"result":result}));
    }
    Ok(
        json!({"regression":if cases.iter().all(|c|c["regression"]=="pass"){"pass"}else{"failed"},"cases":cases,"oracle":s["oracle"]}),
    )
}
pub fn regress(root: &Path, name: &str) -> Result<Value> {
    let bundle = load_bundle(root, name)?;
    let scenarios: Value = oe_wire::json::strict_json(asset(&bundle, "scenario-registry")?)?;
    let scenarios = scenarios["scenarios"]
        .as_array()
        .context("scenario_registry_missing")?;
    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;
    let mut unknown = 0;
    for s in scenarios {
        let id = s["id"].as_str().context("scenario_identity_missing")?;
        match execute_scenario(&bundle, s) {
            Ok(v) => {
                if v["regression"] == "pass" {
                    passed += 1;
                } else {
                    failed += 1;
                }
                results.push(json!({"scenario_id":id,"execution":"completed","verification":v}));
            }
            Err(error) => {
                unknown += 1;
                results.push(json!({"scenario_id":id,"execution":"failed","regression":"unknown","error":format!("{error:#}"),"oracle_retained":s["oracle"]}));
            }
        }
    }
    Ok(
        json!({"schema":"oev2.package-regression/1","engine_id":crate::ENGINE_ID,"engine_version":crate::ENGINE_VERSION,"executable_sha256":crate::executable_digest()?,"package":bundle.spec,"status":if failed==0&&unknown==0{"passed"}else{"incomplete"},"scenario_count":scenarios.len(),"passed":passed,"failed":failed,"unknown":unknown,"results":results,"proof_scope":"new Rust execution reproduces unchanged package oracles; no project adoption or native engineering acceptance implied","historical_receipts":"not_relabelled"}),
    )
}
/// Project review uses exact selected assets, not scenario author-test results.
pub fn project_checks(
    root: &Path,
    name: &str,
    evidence: &[u8],
    query_ids: &[String],
    shape_ids: &[String],
) -> Result<Vec<crate::CheckResult>> {
    let bundle = load_bundle(root, name)?;
    let mut data = base(&bundle)?;
    data.extend_from_slice(evidence);
    let mut out = Vec::new();
    for id in query_ids {
        let result = (|| -> Result<Value> {
            crate::graph::query(
                &crate::graph::load(&data)?,
                std::str::from_utf8(asset(&bundle, id)?)?,
            )
        })();
        out.push(match result {
            Ok(result) => {
                // Queries must explicitly classify status. Finding queries pass only on no findings.
                let rows = result["rows"]
                    .as_array()
                    .context("project_review_requires_select")?;
                let statuses: Vec<_> = rows
                    .iter()
                    .filter_map(|r| r["status"]["value"].as_str())
                    .collect();
                let evidence_present = !crate::graph::load(evidence)?.is_empty();
                let conclusion = if !evidence_present {
                    "unknown"
                } else if statuses.contains(&"violated") {
                    "violated"
                } else if (!statuses.is_empty()
                    && statuses.len() == rows.len()
                    && statuses.iter().all(|s| ["satisfied", "pass"].contains(s)))
                    || (id.starts_with("findings-") && rows.is_empty())
                {
                    "pass"
                } else {
                    "unknown"
                };
                crate::CheckResult {
                    id: id.clone(),
                    execution: "completed".into(),
                    conclusion: conclusion.into(),
                    coverage: vec![format!("query:{id}")],
                    failures: Vec::new(),
                    result,
                }
            }
            Err(e) => crate::CheckResult {
                id: id.clone(),
                execution: "failed".into(),
                conclusion: "unknown".into(),
                coverage: Vec::new(),
                failures: vec![format!("{e:#}")],
                result: Value::Null,
            },
        });
    }
    for id in shape_ids {
        out.push(match crate::shacl::validate(&data, asset(&bundle, id)?) {
            Ok(result) => crate::CheckResult {
                id: id.clone(),
                execution: "completed".into(),
                conclusion: if result["focus_count"] == 0 {
                    "unknown"
                } else if result["conforms"] == true {
                    "pass"
                } else {
                    "violated"
                }
                .into(),
                coverage: vec![format!("shape:{id}")],
                failures: Vec::new(),
                result,
            },
            Err(e) => crate::CheckResult {
                id: id.clone(),
                execution: "failed".into(),
                conclusion: "unknown".into(),
                coverage: Vec::new(),
                failures: vec![format!("{e:#}")],
                result: Value::Null,
            },
        });
    }
    Ok(out)
}
