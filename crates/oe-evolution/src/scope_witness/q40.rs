//! Fixed source-derived catalog. Scope evidence is derived by the existing owner.
use super::*;
use oe_contracts::{CodeExperiment, CodeScopeCaseRelation, CodeScopeWitnessReport2};
use oe_wire::json::canonical_json;

pub const ORACLE_SHA: &str = "fb556d112c0302389240f83b536c82aa7e5079a36be7c2cec2645a9d8a5b51ce";
pub const CATALOG_SHA: &str = "31a5ac5f7b95b976c75d809293f613fbd0fb946d0a330fd48d4064550981bbc4";
pub const CONDITIONS_SHA: &str = "5d96bb44c4da300d2e677de54331535b3082d0e08c89edc923f714325f290f59";
const CATALOG: &[u8] = include_bytes!("../../../../runtime/q40-catalog.json");
const ORACLE: &[u8] = include_bytes!("../../../../runtime/q40-replay-cases.json");

pub fn catalog() -> Result<Value> {
    ensure!(
        oe_core::sha256(CATALOG) == CATALOG_SHA && oe_core::sha256(ORACLE) == ORACLE_SHA,
        "q40_compiled_catalog_changed"
    );
    let value = strict_json(CATALOG)?;
    ensure!(
        value["schema"] == "oev2.code-replay-catalog/2"
            && value["oracle_sha256"] == ORACLE_SHA
            && value["source_fixture_sha256"]
                == "273f849482ac2212ba7c1865ef69546c16e30d39d3e884c3fae6affb8bc8dea3",
        "q40_catalog_identity"
    );
    Ok(value)
}

pub fn validate_experiment(experiment: &CodeExperiment) -> Result<()> {
    let fixed = catalog()?;
    ensure!(
        experiment.oracle_sha256 == ORACLE_SHA
            && experiment.conditions_sha256 == CONDITIONS_SHA
            && experiment.scope_sha256 == SCOPE_SHA
            && experiment.protocol_expectations_sha256 == super::PROTOCOL_SHA
            && experiment.protocol_case_ids.len() == 19
            && experiment.cases.len() == 40
            && canonical_json(&serde_json::to_value(&experiment.cases)?)?
                == canonical_json(&strict_json(ORACLE)?)?
            && fixed["case_ids"]
                == serde_json::to_value(
                    experiment
                        .cases
                        .iter()
                        .map(|c| &c.case_id)
                        .collect::<Vec<_>>()
                )?,
        "q40_fixed_experiment_changed"
    );
    Ok(())
}

pub(super) fn permitted(scope_id: &str, case_id: &str) -> bool {
    catalog()
        .ok()
        .and_then(|value| {
            value["relations"].as_array().map(|rows| {
                rows.iter()
                    .any(|row| row["scope_id"] == scope_id && row["case_id"] == case_id)
            })
        })
        .unwrap_or(false)
}

pub fn bindings(root: &Path) -> Result<Vec<CodeScopeBinding>> {
    let mut rows = unknown_bindings(root)?;
    let fixed = catalog()?;
    for relation in fixed["relations"].as_array().context("q40_relations")? {
        let row = rows
            .iter_mut()
            .find(|row| relation["scope_id"] == row.scope_id)
            .context("q40_original_identity_absent")?;
        row.case_ids
            .push(relation["case_id"].as_str().context("q40_case_id")?.into());
    }
    Ok(rows)
}

pub fn validate_bindings(root: &Path, rows: &[CodeScopeBinding]) -> Result<()> {
    super::validate_bindings_profile(root, rows, Profile::Q40)?;
    let expected = bindings(root)?;
    let actual: std::collections::BTreeMap<_, _> = rows
        .iter()
        .map(|row| (&row.scope_id, &row.case_ids))
        .collect();
    let expected: std::collections::BTreeMap<_, _> = expected
        .iter()
        .map(|row| (&row.scope_id, &row.case_ids))
        .collect();
    ensure!(actual == expected, "q40_catalog_bindings_changed");
    Ok(())
}

pub fn relations(report: &CodeScopeWitnessReport) -> Result<Vec<CodeScopeCaseRelation>> {
    let catalog = catalog()?;
    let mut result = Vec::new();
    for row in catalog["relations"].as_array().context("q40_relations")? {
        let case_id = row["case_id"].as_str().context("q40_case_id")?;
        let status = if report.witness.execution == "completed" {
            report
                .witness
                .comparison
                .as_ref()
                .and_then(|comparison| {
                    comparison
                        .slots
                        .iter()
                        .find(|slot| slot["case_id"] == case_id && slot["run_id"] == 0)
                })
                .and_then(|slot| slot["candidate"]["status"].as_str())
                .unwrap_or("unknown")
        } else {
            "unknown"
        };
        result.push(CodeScopeCaseRelation {
            scope_id: row["scope_id"].as_str().context("q40_scope_id")?.into(),
            case_id: case_id.into(),
            role: row["role"].as_str().context("q40_role")?.into(),
            observation: match status {
                "matched" => "matched",
                "failed" => "failed",
                _ => "unknown",
            }
            .into(),
            behavior_status: "unknown".into(),
        });
    }
    Ok(result)
}

pub fn validate_report(root: &Path, report: &CodeScopeWitnessReport2) -> Result<()> {
    report
        .validate()
        .map_err(|errors| anyhow::anyhow!("q40_report_contract:{errors:?}"))?;
    ensure!(
        report.schema == "oev2.code-scope-witness-report/2"
            && report.catalog_sha256 == CATALOG_SHA
            && report.observation.witness.oracle_sha256 == ORACLE_SHA,
        "q40_report_identity"
    );
    super::validate_report_profile(root, &report.observation, Profile::Q40)?;
    ensure!(
        report.relations == relations(&report.observation)?,
        "q40_report_relations_changed"
    );
    let expected: std::collections::BTreeMap<_, _> = bindings(root)?
        .into_iter()
        .map(|row| (row.scope_id, row.case_ids))
        .collect();
    for item in &report.observation.items {
        let permitted = expected.get(&item.scope_id).context("q40_item_id")?;
        let observed: Vec<String> = permitted
            .iter()
            .filter(|case| {
                report.relations.iter().any(|relation| {
                    relation.scope_id == item.scope_id
                        && relation.case_id == **case
                        && relation.observation == "matched"
                })
            })
            .cloned()
            .collect();
        ensure!(
            item.observed_case_ids == observed,
            "q40_observed_cases_changed"
        );
    }
    Ok(())
}
