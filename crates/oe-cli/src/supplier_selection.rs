//! Structural review of a MISUMI selection record.
//!
//! This is deliberately a receipt validator, not a product selector.  The
//! browser, catalogue reader and native CAD owner remain responsible for
//! obtaining their own observations.  This boundary makes the three source
//! roles and the reason for a choice durable without turning an inCAD BOM or
//! a historical CAD file into a current orderable part.

use anyhow::{ensure, Context, Result};
use oe_core::{read_regular_bytes, write_new, SkillRoot};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const SCHEMA: &str = "oev2.misumi-selection-review/1";
const RESULT_SCHEMA: &str = "oev2.misumi-selection-review-result/1";
const MAX_INPUT: usize = 4 * 1024 * 1024;
const REQUIRED_SELECTED_CHECKS: [&str; 4] = [
    "official_configuration",
    "catalog_basis",
    "condition_match",
    "identity",
];
const LAYERS: [&str; 3] = ["incad", "catalog", "official_web"];

pub struct SelectionReviewArgs {
    pub input: PathBuf,
    pub output: PathBuf,
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .context(format!("{label}_object_required"))
}

fn exact_keys(value: &Value, label: &str, allowed: &[&str]) -> Result<()> {
    let actual = object(value, label)?;
    let allowed: BTreeSet<&str> = allowed.iter().copied().collect();
    let unknown: Vec<_> = actual
        .keys()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect();
    ensure!(unknown.is_empty(), "{label}_unknown_fields:{unknown:?}");
    for key in allowed {
        ensure!(actual.contains_key(key), "{label}_field_missing:{key}");
    }
    Ok(())
}

fn string<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    value
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .context(format!("{label}_nonempty_string_required"))
}

fn strings(value: &Value, label: &str) -> Result<Vec<String>> {
    value
        .as_array()
        .context(format!("{label}_array_required"))?
        .iter()
        .map(|v| Ok(string(v, label)?.to_owned()))
        .collect()
}

fn sha256(value: &Value, label: &str) -> Result<()> {
    let value = string(value, label)?;
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "{label}_sha256_required"
    );
    Ok(())
}

fn source_reference(value: &Value, label: &str) -> Result<()> {
    exact_keys(value, label, &["path", "sha256"])?;
    let path = string(&value["path"], &format!("{label}_path"))?;
    ensure!(
        !Path::new(path).is_absolute(),
        "{label}_path_must_be_portable"
    );
    ensure!(
        !Path::new(path)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)),
        "{label}_path_traversal"
    );
    sha256(&value["sha256"], &format!("{label}_sha256"))
}

fn layer(value: &Value, name: &str) -> Result<Value> {
    exact_keys(value, &format!("{name}_layer"), &["status", "references"])?;
    let status = string(&value["status"], &format!("{name}_status"))?;
    ensure!(
        matches!(status, "used" | "not_used" | "unavailable" | "stale"),
        "{name}_status_invalid"
    );
    let refs = value["references"]
        .as_array()
        .context(format!("{name}_references_array_required"))?;
    for (index, reference) in refs.iter().enumerate() {
        let label = format!("{name}_reference_{index}");
        let keys = if name == "official_web" {
            &[
                "id",
                "role",
                "source",
                "locator",
                "observed_at",
                "part_number",
                "configuration",
            ][..]
        } else {
            &["id", "role", "source", "locator"][..]
        };
        exact_keys(reference, &label, keys)?;
        string(&reference["id"], &format!("{label}_id"))?;
        string(&reference["role"], &format!("{label}_role"))?;
        source_reference(&reference["source"], &format!("{label}_source"))?;
        string(&reference["locator"], &format!("{label}_locator"))?;
        if name == "official_web" {
            string(&reference["observed_at"], &format!("{label}_observed_at"))?;
            string(&reference["part_number"], &format!("{label}_part_number"))?;
            string(
                &reference["configuration"],
                &format!("{label}_configuration"),
            )?;
        }
    }
    Ok(value.clone())
}

pub fn review(value: &Value) -> Result<(Value, Vec<String>)> {
    exact_keys(
        value,
        "selection_review",
        &[
            "schema",
            "project",
            "subject",
            "decision",
            "layers",
            "rationale",
            "checks",
        ],
    )?;
    ensure!(
        value["schema"] == SCHEMA,
        "selection_review_schema_unsupported"
    );

    exact_keys(
        &value["project"],
        "selection_project",
        &["project_id", "work_id", "context_sha256"],
    )?;
    string(&value["project"]["project_id"], "selection_project_id")?;
    string(&value["project"]["work_id"], "selection_work_id")?;
    sha256(
        &value["project"]["context_sha256"],
        "selection_project_context",
    )?;
    exact_keys(
        &value["subject"],
        "selection_subject",
        &["kind", "identity"],
    )?;
    string(&value["subject"]["kind"], "selection_subject_kind")?;
    string(&value["subject"]["identity"], "selection_subject_identity")?;
    exact_keys(
        &value["decision"],
        "selection_decision",
        &["status", "part_number"],
    )?;
    let decision_status = string(&value["decision"]["status"], "selection_decision_status")?;
    ensure!(
        matches!(decision_status, "candidate" | "selected" | "blocked"),
        "selection_decision_status_invalid"
    );
    ensure!(
        value["decision"]["part_number"].is_string(),
        "selection_part_number_string_required"
    );
    let part_number = value["decision"]["part_number"]
        .as_str()
        .unwrap_or("")
        .trim();
    if decision_status == "selected" {
        ensure!(!part_number.is_empty(), "selected_part_number_required");
    }

    let layers = object(&value["layers"], "selection_layers")?;
    for name in LAYERS {
        ensure!(layers.contains_key(name), "selection_layer_missing:{name}");
    }
    let mut parsed_layers = BTreeMap::new();
    for name in LAYERS {
        parsed_layers.insert(name, layer(&layers[name], name)?);
    }

    exact_keys(
        &value["rationale"],
        "selection_rationale",
        &[
            "function",
            "mechanism",
            "conditions",
            "interfaces",
            "why_selected",
            "alternatives",
            "unknowns",
        ],
    )?;
    for key in [
        "function",
        "mechanism",
        "conditions",
        "interfaces",
        "why_selected",
    ] {
        string(
            &value["rationale"][key],
            &format!("selection_rationale_{key}"),
        )?;
    }
    let alternatives = strings(
        &value["rationale"]["alternatives"],
        "selection_rationale_alternatives",
    )?;
    let unknowns = strings(
        &value["rationale"]["unknowns"],
        "selection_rationale_unknowns",
    )?;

    let checks = value["checks"]
        .as_array()
        .context("selection_checks_array_required")?;
    ensure!(!checks.is_empty(), "selection_checks_required");
    let mut statuses = BTreeMap::new();
    for (index, check) in checks.iter().enumerate() {
        let label = format!("selection_check_{index}");
        exact_keys(check, &label, &["id", "status", "basis"])?;
        let id = string(&check["id"], &format!("{label}_id"))?.to_owned();
        let status = string(&check["status"], &format!("{label}_status"))?;
        ensure!(
            matches!(status, "pass" | "fail" | "unknown" | "not_run"),
            "{label}_status_invalid"
        );
        let basis = strings(&check["basis"], &format!("{label}_basis"))?;
        ensure!(!basis.is_empty(), "{label}_basis_required");
        ensure!(
            statuses.insert(id, status.to_owned()).is_none(),
            "selection_check_duplicate"
        );
    }

    let official = &parsed_layers["official_web"];
    let official_refs = official["references"]
        .as_array()
        .expect("validated references");
    let matching_official = official_refs.iter().any(|reference| {
        reference["part_number"].as_str() == Some(part_number)
            && reference["configuration"]
                .as_str()
                .is_some_and(|v| !v.trim().is_empty())
    });
    let mut issues = Vec::new();
    if decision_status == "selected" {
        if official["status"] != "used" || !matching_official {
            issues.push("selected_part_lacks_current_official_configuration".to_owned());
        }
        let catalog = &parsed_layers["catalog"];
        if catalog["status"] != "used" || catalog["references"].as_array().is_none_or(Vec::is_empty)
        {
            issues.push("selected_part_lacks_catalog_or_manual_basis".to_owned());
        }
        for check in REQUIRED_SELECTED_CHECKS {
            if statuses.get(check).map(String::as_str) != Some("pass") {
                issues.push(format!("required_check_not_passed:{check}"));
            }
        }
        if value["rationale"]["why_selected"] == "" {
            issues.push("selection_reason_missing".to_owned());
        }
    }
    if official["status"] != "used" && !part_number.is_empty() {
        issues.push("current_official_web_observation_missing".to_owned());
    }
    if !unknowns.is_empty() && decision_status == "selected" {
        issues.push("selection_retains_unknowns_for_engineering_review".to_owned());
    }
    let status = if decision_status == "blocked"
        || issues.iter().any(|i| {
            i.starts_with("selected_part_lacks")
                || i.starts_with("required_check_not_passed")
                || i == "current_official_web_observation_missing"
        }) {
        "selection_blocked"
    } else if decision_status == "selected" {
        "selected_pending_engineering_verification"
    } else {
        "reviewable_candidate"
    };
    let _ = (alternatives, parsed_layers);
    Ok((
        json!({
            "schema": RESULT_SCHEMA,
            "status": status,
            "issues": issues,
            "project": value["project"],
            "subject": value["subject"],
            "decision": value["decision"],
            "layers": value["layers"],
            "rationale": value["rationale"],
            "checks": value["checks"],
            "source_bytes": "not_read",
            "browser_execution": "not_run",
            "native_cad": "not_run",
            "purchasing": "not_performed",
            "engineering_adoption": "not_established",
            "rrsi": "not_run"
        }),
        issues,
    ))
}

pub fn run(root: &SkillRoot, args: SelectionReviewArgs) -> Result<(Value, u8)> {
    let input = root.private_path(&args.input)?;
    let bytes = read_regular_bytes(&input, MAX_INPUT)?;
    let value = oe_core::json::strict_json(&bytes)?;
    let (result, issues) = review(&value)?;
    let output = root.new_private_dir(&args.output)?;
    write_new(&output.join("selection-review.json"), &result)?;
    let status = result["status"].as_str().unwrap_or("selection_blocked");
    let code = if status == "selection_blocked" { 1 } else { 0 };
    Ok((
        json!({"schema": "ontology-engineering.native-supplier-selection/v1", "status": status, "issues": issues, "result": output.strip_prefix(root.data_path())?, "engineering_adoption": "not_established", "purchasing": "not_performed"}),
        code,
    ))
}
