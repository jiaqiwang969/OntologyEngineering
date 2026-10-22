//! Pure source-bound manufacturing report views and decimal arithmetic.
//! Supplied excerpts are not inspected attachments; rendered claims are not
//! Semantica execution, engineering approval or accepted prices.
mod decimal;
pub mod pdf;
pub mod publication;
mod render;
use anyhow::{Context, Result, ensure};
use decimal::{Decimal, integer};
pub use render::{RenderedReport, render, source_tex, tex};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const FORMAT: &str = "ontology-engineering.manufacturing-report/v1";
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
pub(super) fn array<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .with_context(|| format!("{key}_must_be_list"))
}
pub(super) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .with_context(|| format!("{key}_must_be_nonempty_string"))
}
pub(super) fn strings<'a>(v: &'a Value, key: &str) -> Result<Vec<&'a str>> {
    array(v, key)?
        .iter()
        .map(|s| s.as_str().context("reference_must_be_string"))
        .collect()
}
pub(super) type Index<'a> = BTreeMap<&'a str, &'a Value>;
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
pub(super) fn index<'a>(rows: &'a [Value], collection: &str) -> Result<Index<'a>> {
    let mut ids = BTreeMap::new();
    for row in rows {
        let id = text(row, "id")?;
        ensure!(valid_id(id), "{collection}_invalid_ID");
        ensure!(ids.insert(id, row).is_none(), "{collection}_duplicate_ID");
    }
    Ok(ids)
}
fn refs(row: &Value, key: &str, index: &Index<'_>, allow_empty: bool) -> Result<()> {
    let values = strings(row, key)?;
    ensure!(
        allow_empty || !values.is_empty(),
        "explicit_references_required"
    );
    let mut seen = BTreeSet::new();
    for value in values {
        ensure!(seen.insert(value), "duplicate_references");
        ensure!(index.contains_key(value), "dangling_references");
    }
    Ok(())
}
fn optional_changes(s: &Value) -> Result<&[Value]> {
    match s.get("changes") {
        None => Ok(&[]),
        Some(v) => v
            .as_array()
            .map(Vec::as_slice)
            .context("changes_must_be_list"),
    }
}
fn cost_contract(cost: &Value) -> Result<()> {
    for field in [
        "currency",
        "tax_basis",
        "quantity_basis",
        "scope",
        "assumptions",
    ] {
        text(cost, field)?;
    }
    let quantity = cost.get("quantity").context("quantity_required")?;
    if !quantity.is_null() {
        integer(quantity)?;
    }
    index(array(cost, "lines")?, "cost_lines")?;
    for line in array(cost, "lines")? {
        let basis = text(line, "basis")?;
        ensure!(
            ["per_unit", "per_batch", "one_time"].contains(&basis),
            "unknown_fee_basis"
        );
        for field in ["name", "nature"] {
            text(line, field)?;
        }
        let count = integer(line.get("count").context("fee_count_required")?)?;
        ensure!(
            basis != "per_unit" || count == num_bigint::BigInt::from(1u8),
            "per_unit_count_must_be_one"
        );
        Decimal::amount(line.get("value").context("fee_value_required")?)?;
    }
    Ok(())
}
/// Validate the existing report transport contract and explicit references only.
pub fn validate(s: &Value) -> Result<()> {
    ensure!(
        oe_core::json::ordered_json(s)?.len() <= MAX_INPUT_BYTES,
        "report_input_exceeds_4_MiB"
    );
    ensure!(
        s.get("format").and_then(Value::as_str) == Some(FORMAT),
        "unsupported_report_input_format"
    );
    let d = s.get("document").context("document_required")?;
    for field in [
        "id",
        "revision",
        "title",
        "date",
        "audience",
        "project_state",
        "evidence_scope",
        "decision",
        "footer",
    ] {
        text(d, field)?;
    }
    ensure!(valid_id(text(d, "id")?), "invalid_document_ID");
    ensure!(
        s.get("semantic_status").and_then(Value::as_str) == Some("not_executed_for_this_snapshot"),
        "renderer_cannot_attest_semantic_execution"
    );
    let sources = index(array(s, "sources")?, "sources")?;
    let records = index(array(s, "records")?, "records")?;
    let objects = index(array(s, "objects")?, "objects")?;
    let relations = index(array(s, "relations")?, "relations")?;
    let views = index(array(s, "views")?, "views")?;
    ensure!(
        !views.is_empty(),
        "at_least_one_explicit_decision_view_required"
    );
    for source in sources.values() {
        for field in ["title", "locator", "excerpt", "basis"] {
            text(source, field)?;
        }
    }
    for obj in objects.values() {
        for field in ["name", "revision", "state"] {
            text(obj, field)?;
        }
        refs(obj, "source_ids", &sources, false)?;
    }
    for row in records.values() {
        for field in [
            "title",
            "kind",
            "statement",
            "nature",
            "status",
            "conditions",
        ] {
            text(row, field)?;
        }
        refs(row, "source_ids", &sources, false)?;
        refs(row, "object_ids", &objects, false)?;
    }
    let mut nodes = objects.clone();
    for (id, row) in &records {
        ensure!(
            nodes.insert(id, row).is_none(),
            "object_and_record_ID_overlap"
        );
    }
    for row in relations.values() {
        ensure!(
            nodes.contains_key(text(row, "from")?) && nodes.contains_key(text(row, "to")?),
            "relation_endpoint_missing"
        );
        text(row, "relation")?;
        text(row, "reason")?;
        refs(row, "source_ids", &sources, false)?;
    }
    let changes = index(optional_changes(s)?, "changes")?;
    for row in changes.values() {
        for field in ["before", "after", "reason"] {
            text(row, field)?;
        }
        refs(row, "source_ids", &sources, false)?;
        refs(row, "affected_ids", &nodes, false)?;
    }
    for row in views.values() {
        for field in ["title", "question", "update_triggers"] {
            text(row, field)?;
        }
        match text(row, "kind")? {
            "records" => refs(row, "items", &records, true)?,
            "relations" => refs(row, "items", &relations, true)?,
            "cost" | "changes" => (),
            _ => anyhow::bail!("unknown_view_kind"),
        }
    }
    let mut all = BTreeSet::new();
    for ids in [&objects, &records, &relations, &changes] {
        for id in ids.keys() {
            ensure!(all.insert(*id), "cross_collection_ID_overlap");
        }
    }
    if let Some(cost) = s.get("cost").filter(|v| !v.is_null()) {
        cost_contract(cost)?;
        refs(cost, "source_ids", &sources, false)?;
        for line in array(cost, "lines")? {
            refs(line, "source_ids", &sources, false)?;
            refs(line, "object_ids", &objects, false)?;
            ensure!(all.insert(text(line, "id")?), "cross_collection_ID_overlap");
        }
    }
    Ok(())
}
/// Arithmetic over the supplied fee bases. Null value/quantity is omitted, never
/// changed to zero; completeness, quote scope and acceptance are not inferred.
pub fn cost_result(cost: Option<&Value>) -> Result<Option<Value>> {
    let Some(cost) = cost.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    cost_contract(cost)?;
    let quantity = if cost["quantity"].is_null() {
        None
    } else {
        Some(integer(&cost["quantity"])?)
    };
    let mut total = Decimal::zero();
    let mut omitted = Vec::new();
    let mut rows = Vec::new();
    for line in array(cost, "lines")? {
        let value = Decimal::amount(&line["value"])?;
        let count = integer(&line["count"])?;
        let factor = if text(line, "basis")? == "per_unit" {
            quantity.as_ref()
        } else {
            Some(&count)
        };
        let subtotal = if let (Some(value), Some(factor)) = (value, factor) {
            Some(value.multiply(factor))
        } else {
            None
        };
        if let Some(subtotal) = &subtotal {
            total = total.add(subtotal);
        } else {
            omitted.push(text(line, "id")?);
        }
        rows.push(json!({"id":line["id"],"subtotal":subtotal.map(|d|d.money())}));
    }
    Ok(Some(
        json!({"known_subtotal":total.money(),"omitted_ids":omitted,"rows":rows,"scope":"Only the explicitly supplied fee lines; completeness and price acceptance are not inferred."}),
    ))
}
