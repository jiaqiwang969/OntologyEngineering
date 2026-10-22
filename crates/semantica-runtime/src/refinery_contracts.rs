//! Neutral, pinned refinery data rules. Parsing a receipt is not attestation,
//! authorization, semantic execution, adoption, or promotion.
//!
//! The editable spec generates the public wire types; these algorithms retain
//! the original `from_dict` defaults, computed fields and canonical identities.
//! The full 22 original dataclass behaviors remain required beyond this profile.
use anyhow::{Context, Result, ensure};
use semantica_contracts::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const SCHEMA: &str = "1.0";
pub const CONTRACT: &str = "semantica.ontology.refinery/v1";
pub const EMPTY_PACKAGE_SHA256: &str =
    "fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947";
pub const CATEGORIES: &[&str] = &[
    "ontology",
    "competency_questions",
    "shapes",
    "queries",
    "rules",
    "cases",
    "contract",
    "provenance",
];
pub const STATES: &[&str] = &[
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted",
];
pub const CONTEXT_ACTIONS: &[&str] = &[
    "candidate",
    "proposed",
    "committed",
    "execute_candidate",
    "derive_regression_gate",
    "regression_passed",
    "derive_release_gate",
    "release_complete",
    "promoted",
];
const PHASES: &[&str] = &["execution", "regression", "receipt", "release"];

macro_rules! typed {
    ($kind:ty, $value:expr) => {{
        let value = $value;
        // `from_value` would deserialize JSON Value leaves a second time and
        // interpret serde's private number marker; generated hooks use bytes.
        let dto: $kind = serde_json::from_slice(&semantica_json::canonical_json(&value)?)
            .context("refinery_generated_wire_shape")?;
        dto.validate()
            .map_err(|_| anyhow::anyhow!("refinery_generated_wire_constraint"))?;
        Ok::<Value, anyhow::Error>(value)
    }};
}

fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let fields = value.as_object().context("refinery_object_required")?;
    ensure!(
        required.iter().all(|key| fields.contains_key(*key)),
        "refinery_field_missing"
    );
    ensure!(
        fields
            .keys()
            .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str())),
        "refinery_unknown_field"
    );
    Ok(())
}
fn python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}
fn exact(value: &Value) -> Result<&str> {
    let text = value.as_str().context("refinery_text_required")?;
    ensure!(
        text == text.trim_matches(python_space),
        "refinery_surrounding_whitespace"
    );
    Ok(text)
}
fn text(value: &Value) -> Result<&str> {
    let text = exact(value)?;
    ensure!(!text.is_empty(), "refinery_nonempty_text_required");
    Ok(text)
}
pub fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn sha(value: &Value) -> Result<&str> {
    let digest = text(value)?;
    ensure!(valid_sha(digest), "refinery_sha256_invalid");
    Ok(digest)
}
fn opaque(value: &Value) -> Result<&str> {
    let value = text(value)?;
    ensure!(
        value.len() <= 200
            && value.as_bytes()[0].is_ascii_alphanumeric()
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:@+-".contains(&b)),
        "refinery_opaque_identifier_invalid"
    );
    Ok(value)
}
pub fn opaque_identifier(value: &Value) -> Result<&str> {
    opaque(value)
}
fn decimal(bytes: &[u8]) -> Option<u32> {
    (!bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)).then(|| {
        bytes
            .iter()
            .fold(0, |value, digit| value * 10 + (digit - b'0') as u32)
    })
}
fn iso_date_end(bytes: &[u8]) -> Option<usize> {
    let year = decimal(bytes.get(..4)?)?;
    if !(1..=9999).contains(&year) {
        return None;
    }
    let expanded = bytes.get(4) == Some(&b'-');
    let week = bytes.get(if expanded { 5 } else { 4 }) == Some(&b'W');
    let (date, end) = if week {
        let start = if expanded { 6 } else { 5 };
        let week = decimal(bytes.get(start..start + 2)?)?;
        let mut end = start + 2;
        let weekday = if expanded && bytes.get(end) == Some(&b'-') {
            end += 2;
            decimal(bytes.get(end - 1..end)?)?
        } else if !expanded && bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
            decimal(bytes.get(end - 1..end)?)?
        } else {
            1
        };
        let weekday = match weekday {
            1 => chrono::Weekday::Mon,
            2 => chrono::Weekday::Tue,
            3 => chrono::Weekday::Wed,
            4 => chrono::Weekday::Thu,
            5 => chrono::Weekday::Fri,
            6 => chrono::Weekday::Sat,
            7 => chrono::Weekday::Sun,
            _ => return None,
        };
        (
            chrono::NaiveDate::from_isoywd_opt(year as i32, week, weekday)?,
            end,
        )
    } else if expanded {
        if bytes.get(7) != Some(&b'-') {
            return None;
        }
        let month = decimal(bytes.get(5..7)?)?;
        let day = decimal(bytes.get(8..10)?)?;
        (
            chrono::NaiveDate::from_ymd_opt(year as i32, month, day)?,
            10,
        )
    } else {
        let month = decimal(bytes.get(4..6)?)?;
        let day = decimal(bytes.get(6..8)?)?;
        (chrono::NaiveDate::from_ymd_opt(year as i32, month, day)?, 8)
    };
    (1..=9999)
        .contains(&chrono::Datelike::year(&date))
        .then_some(end)
}
fn iso_time_parts(text: &str, offset: bool) -> Option<[u32; 3]> {
    let fractional = text.split_once(['.', ',']);
    let mut body = if let Some((body, fraction)) = fractional {
        if (fraction.is_empty() && offset) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        body
    } else {
        text
    };
    // Retain observed Python3.13 fromisoformat input behavior: for the
    // local time a single final ASCII character after a whole component is
    // consumed, even without a fraction delimiter; offsets are stricter.
    if fractional.is_none() && !offset && body.is_ascii() {
        if body.contains(':') {
            if [6, 9].contains(&body.len()) {
                body = &body[..body.len() - 1];
            }
        } else if [3, 5, 7].contains(&body.len()) {
            body = &body[..body.len() - 1];
        }
    }
    // A basic seconds time can retain fraction digits without '.' in this
    // original host profile. They do not alter hour/minute/second validity.
    if fractional.is_none()
        && !body.contains(':')
        && body.len() >= 8
        && body.bytes().all(|b| b.is_ascii_digit())
    {
        body = &body[..6];
    }
    let mut result = [0; 3];
    if body.contains(':') {
        let components = body.split(':').collect::<Vec<_>>();
        if !(2..=3).contains(&components.len()) {
            return None;
        }
        for (index, value) in components.into_iter().enumerate() {
            if value.len() != 2 {
                return None;
            }
            result[index] = decimal(value.as_bytes())?;
        }
    } else {
        if ![2, 4, 6].contains(&body.len()) {
            return None;
        }
        for (index, value) in body.as_bytes().chunks_exact(2).enumerate() {
            result[index] = decimal(value)?;
        }
    }
    Some(result)
}
fn iso_timestamp(text: &str) -> Option<()> {
    let end = iso_date_end(text.as_bytes())?;
    // Python permits any one Unicode separator, not only RFC3339's 'T'.
    let rest = text.get(end..)?;
    let separator = rest.chars().next()?;
    let time = rest.get(separator.len_utf8()..)?;
    let zone = time.bytes().position(|b| b"Z+-".contains(&b))?;
    let local = iso_time_parts(time.get(..zone)?, false)?;
    if local[0] > 23 || local[1] > 59 || local[2] > 59 {
        return None;
    }
    let offset = time.get(zone..)?;
    if offset != "Z" {
        if !offset.starts_with(['+', '-']) {
            return None;
        }
        let parts = iso_time_parts(offset.get(1..)?, true)?;
        // fromisoformat normalizes offset minute/second components; the final
        // absolute offset, rather than each component, must be under24hours.
        if parts[0] * 3600 + parts[1] * 60 + parts[2] >= 86400 {
            return None;
        }
    }
    Some(())
}
pub fn timestamp(value: &Value) -> Result<&str> {
    let value = text(value)?;
    ensure!(
        iso_timestamp(value).is_some(),
        "refinery_timezone_timestamp_invalid"
    );
    Ok(value)
}
fn texts(value: &Value, empty: bool) -> Result<Vec<String>> {
    let values = value.as_array().context("refinery_text_list_required")?;
    ensure!(
        empty || !values.is_empty(),
        "refinery_nonempty_text_list_required"
    );
    let values = values
        .iter()
        .map(|v| text(v).map(str::to_owned))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        values.iter().collect::<BTreeSet<_>>().len() == values.len(),
        "refinery_duplicate_text_list"
    );
    Ok(values)
}
fn with_schema(value: &Value) -> Result<Value> {
    let mut value = value.clone();
    let object = value.as_object_mut().context("refinery_object_required")?;
    object.entry("schema_version").or_insert(json!(SCHEMA));
    ensure!(
        value["schema_version"] == SCHEMA,
        "refinery_schema_version_unsupported"
    );
    Ok(value)
}
pub fn digest(value: &Value) -> Result<String> {
    Ok(crate::sha256(semantica_json::canonical_json(value)?))
}
fn hashed(mut value: Value, input: &Value, field: &str) -> Result<Value> {
    let hash = digest(&value)?;
    ensure!(
        input.get(field).is_none_or(|declared| declared == &hash),
        "refinery_declared_hash_mismatch"
    );
    value[field] = json!(hash);
    Ok(value)
}
fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(a), Some(b)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            )
        {
            result.push((a * 16 + b) as u8);
            index += 3;
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    result
}
pub fn logical_uri(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value == value.trim_matches(python_space) && !value.contains('\\'),
        "refinery_logical_uri_invalid"
    );
    ensure!(
        value
            .split_once(':')
            .is_some_and(|(s, _)| ["urn", "semantica", "evidence", "ni"]
                .contains(&s.to_ascii_lowercase().as_str())),
        "refinery_logical_uri_scheme_invalid"
    );
    ensure!(
        !percent_decode(value)
            .split(|b| b"/:?#".contains(b))
            .any(|part| part == b"." || part == b".."),
        "refinery_logical_uri_dot_segment"
    );
    Ok(())
}
pub fn uri_within(uri: &str, root: &str) -> bool {
    if logical_uri(uri).is_err() || logical_uri(root).is_err() {
        return false;
    }
    uri == root
        || ['/', '#', ':', '?'].iter().any(|delimiter| {
            uri.starts_with(&format!(
                "{}{delimiter}",
                root.trim_end_matches(['/', '#', ':', '?'])
            ))
        })
}
pub fn source(value: &Value) -> Result<Value> {
    keys(
        value,
        &["source_id", "uri", "sha256", "media_type", "captured_at"],
        &[],
    )?;
    text(&value["source_id"])?;
    logical_uri(text(&value["uri"])?)?;
    sha(&value["sha256"])?;
    text(&value["media_type"])?;
    timestamp(&value["captured_at"])?;
    typed!(SemanticRefinerySourceEvidence, value.clone())
}
fn sources(value: &Value) -> Result<Vec<Value>> {
    let values = value.as_array().context("refinery_source_list_required")?;
    ensure!(!values.is_empty(), "refinery_source_list_empty");
    let values = values.iter().map(source).collect::<Result<Vec<_>>>()?;
    ensure!(
        values
            .iter()
            .map(|v| v["source_id"].as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == values.len(),
        "refinery_duplicate_source_identity"
    );
    Ok(values)
}

/// Original logical binding only. It does not authorize a host action.
pub fn binding(value: &Value) -> Result<Value> {
    const REQUIRED: &[&str] = &[
        "binding_id",
        "project_id",
        "domain",
        "package_id",
        "workspace_id",
        "baseline_version",
        "baseline_package_sha256",
        "evidence_root",
        "fact_authorities",
        "decision_authorities",
        "allowed_actions",
        "semantic_api_contract",
        "promotion_target",
        "created_at",
    ];
    keys(value, REQUIRED, &["schema_version", "binding_sha256"])?;
    let mut result = with_schema(value)?;
    result.as_object_mut().unwrap().remove("binding_sha256");
    for field in [
        "binding_id",
        "project_id",
        "domain",
        "workspace_id",
        "baseline_version",
        "promotion_target",
    ] {
        text(&result[field])?;
    }
    opaque(&result["package_id"])?;
    sha(&result["baseline_package_sha256"])?;
    ensure!(
        (result["baseline_version"] == "0")
            == (result["baseline_package_sha256"] == EMPTY_PACKAGE_SHA256),
        "refinery_binding_empty_baseline_mismatch"
    );
    logical_uri(text(&result["evidence_root"])?)?;
    texts(&result["fact_authorities"], false)?;
    texts(&result["decision_authorities"], false)?;
    let actions = texts(&result["allowed_actions"], false)?;
    ensure!(
        actions.len() <= STATES.len()
            && actions
                .iter()
                .map(String::as_str)
                .eq(STATES[..actions.len()].iter().copied()),
        "refinery_binding_lifecycle_prefix_invalid"
    );
    ensure!(
        result["semantic_api_contract"] == CONTRACT
            && result["promotion_target"] == "industry-registry",
        "refinery_binding_contract_or_target_invalid"
    );
    timestamp(&result["created_at"])?;
    typed!(
        SemanticRefineryProjectOntologyBinding,
        hashed(result, value, "binding_sha256")?
    )
}
pub fn task(value: &Value) -> Result<Value> {
    const REQUIRED: &[&str] = &[
        "task_id",
        "task_kind",
        "project_id",
        "domain",
        "intent",
        "requested_decision",
        "actor_id",
        "requested_actions",
        "required_capabilities",
        "evidence",
        "created_at",
    ];
    keys(value, REQUIRED, &["schema_version", "envelope_sha256"])?;
    let mut result = with_schema(value)?;
    result.as_object_mut().unwrap().remove("envelope_sha256");
    for field in [
        "task_id",
        "task_kind",
        "project_id",
        "domain",
        "intent",
        "requested_decision",
        "actor_id",
    ] {
        text(&result[field])?;
    }
    let actions = texts(&result["requested_actions"], false)?;
    ensure!(
        actions
            .iter()
            .all(|action| action == "engagement" || CONTEXT_ACTIONS.contains(&action.as_str())),
        "refinery_task_action_unsupported"
    );
    texts(&result["required_capabilities"], false)?;
    result["evidence"] = json!(sources(&result["evidence"])?);
    timestamp(&result["created_at"])?;
    typed!(
        SemanticRefinerySemanticTaskEnvelope,
        hashed(result, value, "envelope_sha256")?
    )
}

// Python b64decode(validate=True) accepts nonzero trailing bits and surplus
// padding after full groups. The bytes, not a normalized base64 string, bind
// asset identity. Empty content is valid; missing required padding is invalid.
fn decode_base64(value: &str) -> Result<Vec<u8>> {
    let data = value.trim_end_matches('=');
    let padding = value.len() - data.len();
    ensure!(
        data.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/".contains(&b))
            && match data.len() % 4 {
                0 => !data.is_empty() || padding == 0,
                2 => padding == 2,
                3 => padding == 1,
                _ => false,
            },
        "refinery_content_base64_invalid"
    );
    let mut result = Vec::with_capacity(data.len() / 4 * 3 + 2);
    let mut bits = 0u32;
    let mut count = 0u32;
    for b in data.bytes() {
        let digit = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => unreachable!(),
        };
        bits = (bits << 6) | digit as u32;
        count += 6;
        if count >= 8 {
            count -= 8;
            result.push((bits >> count) as u8);
        }
        bits &= (1 << count) - 1;
    }
    Ok(result)
}
fn asset(value: &Value, category: &str) -> Result<Value> {
    keys(
        value,
        &["category", "asset_id", "operation", "media_type", "sha256"],
        &["content_base64", "replaces_sha256", "role", "case_kind"],
    )?;
    ensure!(
        CATEGORIES.contains(&category) && value["category"] == category,
        "refinery_asset_category_invalid"
    );
    opaque(&value["asset_id"])?;
    text(&value["media_type"])?;
    sha(&value["sha256"])?;
    let operation = text(&value["operation"])?;
    ensure!(
        ["add", "replace", "remove"].contains(&operation),
        "refinery_asset_operation_invalid"
    );
    let mut result = value.clone();
    for name in ["content_base64", "replaces_sha256", "role", "case_kind"] {
        result
            .as_object_mut()
            .unwrap()
            .entry(name)
            .or_insert(Value::Null);
    }
    if !result["role"].is_null() {
        text(&result["role"])?;
    }
    if category == "cases" {
        ensure!(
            result["case_kind"].as_str().is_some_and(|kind| [
                "positive",
                "negative",
                "ambiguity",
                "prior_release"
            ]
            .contains(&kind)),
            "refinery_case_kind_invalid"
        );
    } else {
        ensure!(
            result["case_kind"].is_null(),
            "refinery_noncase_kind_declared"
        );
    }
    if operation == "remove" {
        sha(&result["replaces_sha256"])?;
        ensure!(
            result["content_base64"].is_null() && result["sha256"] == result["replaces_sha256"],
            "refinery_remove_content_invalid"
        );
    } else {
        let content = exact(&result["content_base64"])?;
        ensure!(
            crate::sha256(decode_base64(content)?) == sha(&result["sha256"])?,
            "refinery_asset_content_hash_mismatch"
        );
        if operation == "replace" {
            sha(&result["replaces_sha256"])?;
        } else {
            ensure!(
                result["replaces_sha256"].is_null(),
                "refinery_add_replaces_declared"
            );
        }
    }
    typed!(SemanticRefineryPackageAssetDelta, result)
}
/// Single neutral decoder used by both DTO normalization and the CAS engine.
pub fn asset_content(value: &Value) -> Result<Option<Vec<u8>>> {
    let normalized = asset(value, text(&value["category"])?)?;
    if normalized["content_base64"].is_null() {
        Ok(None)
    } else {
        decode_base64(exact(&normalized["content_base64"])?).map(Some)
    }
}
pub fn delta(value: &Value) -> Result<Value> {
    const REQUIRED: &[&str] = &[
        "package_id",
        "base_version",
        "base_package_sha256",
        "target_version",
        "rationale",
        "created_by",
        "created_at",
        "required_capabilities",
        "source_evidence",
        "book_impact",
        "ontology",
        "competency_questions",
        "shapes",
        "queries",
        "rules",
        "cases",
        "contract",
        "provenance",
    ];
    keys(value, REQUIRED, &["schema_version", "delta_sha256"])?;
    let mut result = with_schema(value)?;
    result.as_object_mut().unwrap().remove("delta_sha256");
    opaque(&result["package_id"])?;
    sha(&result["base_package_sha256"])?;
    for field in ["base_version", "target_version", "rationale", "created_by"] {
        text(&result[field])?;
    }
    ensure!(
        result["base_version"] != result["target_version"],
        "refinery_same_target_version"
    );
    ensure!(
        result["base_version"] != "0" || result["base_package_sha256"] == EMPTY_PACKAGE_SHA256,
        "refinery_delta_empty_baseline_mismatch"
    );
    timestamp(&result["created_at"])?;
    texts(&result["required_capabilities"], false)?;
    result["source_evidence"] = json!(sources(&result["source_evidence"])?);
    ensure!(
        result["book_impact"].as_str().is_some_and(|impact| [
            "none",
            "vol1-method",
            "vol2-iso-exemplar",
            "both"
        ]
        .contains(&impact)),
        "refinery_book_impact_invalid"
    );
    let mut identities = BTreeSet::new();
    for category in CATEGORIES {
        let assets = result[*category]
            .as_array()
            .context("refinery_explicit_asset_array_required")?;
        let assets = assets
            .iter()
            .map(|value| {
                let value = asset(value, category)?;
                ensure!(
                    identities.insert(text(&value["asset_id"])?.to_owned()),
                    "refinery_duplicate_global_asset_identity"
                );
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?;
        result[*category] = json!(assets);
    }
    ensure!(!identities.is_empty(), "refinery_delta_empty");
    typed!(
        SemanticRefineryPackageDelta,
        hashed(result, value, "delta_sha256")?
    )
}
pub fn authorization(value: &Value) -> Result<Value> {
    const REQUIRED: &[&str] = &[
        "authorization_id",
        "action",
        "actor_id",
        "authority",
        "package_id",
        "delta_sha256",
        "promotion_target",
        "reason",
        "source",
        "issued_at",
        "decisions",
    ];
    keys(value, REQUIRED, &["authorization_sha256"])?;
    let mut result = value.clone();
    result
        .as_object_mut()
        .unwrap()
        .remove("authorization_sha256");
    for field in ["authorization_id", "actor_id", "authority", "reason"] {
        text(&result[field])?;
    }
    ensure!(
        result["action"] == "commit" || result["action"] == "promote",
        "refinery_authorization_action_invalid"
    );
    ensure!(
        result["promotion_target"] == "industry-registry",
        "refinery_authorization_target_invalid"
    );
    opaque(&result["package_id"])?;
    sha(&result["delta_sha256"])?;
    timestamp(&result["issued_at"])?;
    result["source"] = source(&result["source"])?;
    let decisions = result["decisions"]
        .as_array()
        .context("refinery_decision_list_required")?;
    let mut seen = BTreeSet::new();
    for value in decisions {
        keys(
            value,
            &[
                "category",
                "asset_id",
                "operation",
                "replaces_sha256",
                "verdict",
                "reason",
            ],
            &[],
        )?;
        ensure!(
            value["category"]
                .as_str()
                .is_some_and(|v| CATEGORIES.contains(&v)),
            "refinery_decision_category_invalid"
        );
        opaque(&value["asset_id"])?;
        sha(&value["replaces_sha256"])?;
        text(&value["reason"])?;
        ensure!(
            value["operation"] == "replace" || value["operation"] == "remove",
            "refinery_decision_operation_invalid"
        );
        ensure!(
            value["verdict"] == "approve",
            "refinery_decision_not_approved"
        );
        ensure!(
            seen.insert(
                ["category", "asset_id", "operation", "replaces_sha256"]
                    .iter()
                    .map(|key| value[*key].as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            ),
            "refinery_duplicate_asset_decision"
        );
        let _: Value = typed!(SemanticRefineryAssetDecision, value.clone())?;
    }
    typed!(
        SemanticRefineryRefineryAuthorization,
        hashed(result, value, "authorization_sha256")?
    )
}

pub fn transition_context(
    action: &str,
    delta_sha256: &str,
    task_value: &Value,
    binding_value: &Value,
) -> Result<Value> {
    ensure!(
        CONTEXT_ACTIONS.contains(&action) && valid_sha(delta_sha256),
        "refinery_transition_action_or_delta_invalid"
    );
    let task = task(task_value)?;
    let binding = binding(binding_value)?;
    ensure!(
        task["requested_actions"] == json!([action]),
        "refinery_transition_exact_action_required"
    );
    ensure!(
        task["project_id"] == binding["project_id"] && task["domain"] == binding["domain"],
        "refinery_transition_project_domain_mismatch"
    );
    for value in task["evidence"].as_array().unwrap() {
        ensure!(
            uri_within(text(&value["uri"])?, text(&binding["evidence_root"])?),
            "refinery_transition_evidence_outside_root"
        );
    }
    let state = match action {
        "execute_candidate" => "committed",
        "derive_regression_gate" => "regression_passed",
        "derive_release_gate" => "release_complete",
        other => other,
    };
    ensure!(
        binding["allowed_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == state),
        "refinery_transition_state_outside_prefix"
    );
    let content = json!({"schema_version":SCHEMA,"action":action,"delta_sha256":delta_sha256,"envelope":task,"binding":binding});
    typed!(
        SemanticRefineryTransitionContext,
        hashed(content, &json!({}), "context_sha256")?
    )
}

/// Original source identity can be incomplete. Empty strings are retained.
pub fn runtime_source(value: &Value) -> Result<Value> {
    keys(
        value,
        &[
            "runtime_commit",
            "runtime_artifact_sha256",
            "runtime_version",
        ],
        &[],
    )?;
    for field in [
        "runtime_commit",
        "runtime_artifact_sha256",
        "runtime_version",
    ] {
        exact(&value[field])?;
    }
    typed!(SemanticRefineryRuntimeSourceIdentity, value.clone())
}
fn legacy_source_complete(value: &Value) -> bool {
    value["runtime_commit"].as_str().is_some_and(|v| {
        v.len() == 40
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) && value["runtime_artifact_sha256"]
        .as_str()
        .is_some_and(valid_sha)
        && value["runtime_version"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
}
pub fn phase(value: &Value) -> Result<Value> {
    keys(
        value,
        &[
            "name",
            "status",
            "required_capabilities",
            "observed_capabilities",
        ],
        &["missing_capabilities", "evidence_sha256", "details"],
    )?;
    ensure!(
        value["name"].as_str().is_some_and(|v| PHASES.contains(&v)),
        "refinery_phase_name_invalid"
    );
    ensure!(
        value["status"]
            .as_str()
            .is_some_and(|v| ["passed", "failed", "blocked"].contains(&v)),
        "refinery_phase_status_invalid"
    );
    let required = texts(&value["required_capabilities"], false)?;
    let observed = texts(&value["observed_capabilities"], true)?;
    let missing = required
        .iter()
        .filter(|v| !observed.contains(v))
        .cloned()
        .collect::<BTreeSet<_>>();
    let evidence = value.get("evidence_sha256").cloned().unwrap_or(Value::Null);
    if !evidence.is_null() {
        sha(&evidence)?;
    }
    ensure!(
        value["status"] != "passed" || (missing.is_empty() && !evidence.is_null()),
        "refinery_passed_phase_missing_capability_or_evidence"
    );
    let result = json!({"name":value["name"],"status":value["status"],"required_capabilities":required,"observed_capabilities":observed,"missing_capabilities":missing,"evidence_sha256":evidence,"details":value.get("details").cloned().unwrap_or_else(|| json!({}))});
    // Validate the details JSON through the single canonical JSON identity.
    semantica_json::canonical_json(&result["details"])?;
    typed!(SemanticRefineryEngagementPhase, result)
}
fn learning(value: &Value) -> Result<Value> {
    keys(value, &["status", "rationale"], &["delta_sha256"])?;
    text(&value["rationale"])?;
    let delta = value.get("delta_sha256").cloned().unwrap_or(Value::Null);
    if value["status"] == "candidate" {
        sha(&delta)?;
    } else {
        ensure!(
            value["status"] == "no_delta" && delta.is_null(),
            "refinery_learning_status_or_delta_invalid"
        );
    }
    typed!(
        SemanticRefineryLearningResult,
        json!({"status":value["status"],"rationale":value["rationale"],"delta_sha256":delta})
    )
}
fn execution_reference(value: &Value) -> Result<Value> {
    keys(
        value,
        &[
            "receipt_sha256",
            "package_id",
            "package_version",
            "package_digest",
        ],
        &[],
    )?;
    sha(&value["receipt_sha256"])?;
    sha(&value["package_digest"])?;
    text(&value["package_id"])?;
    text(&value["package_version"])?;
    typed!(SemanticRefineryExecutionReceiptReference, value.clone())
}
fn engagement_content(value: &Value, rust: bool, derive: bool) -> Result<Value> {
    const REQUIRED: &[&str] = &[
        "contract",
        "engagement_id",
        "envelope_sha256",
        "binding_sha256",
        "required_capabilities",
        "runtime_source",
        "execution",
        "regression",
        "receipt",
        "release",
        "learning",
        "execution_receipts",
        "status",
        "blocked_reasons",
        "created_at",
        "receipt_sha256",
    ];
    let required = REQUIRED
        .iter()
        .copied()
        .filter(|key| !derive || !["status", "blocked_reasons", "receipt_sha256"].contains(key))
        .collect::<Vec<_>>();
    keys(
        value,
        &required,
        if rust {
            &["schema_version", "profile"]
        } else {
            &["schema_version"]
        },
    )?;
    let mut result = with_schema(value)?;
    result.as_object_mut().unwrap().remove("receipt_sha256");
    ensure!(
        result["contract"] == CONTRACT,
        "refinery_engagement_contract_invalid"
    );
    text(&result["engagement_id"])?;
    sha(&result["envelope_sha256"])?;
    sha(&result["binding_sha256"])?;
    let required = texts(&result["required_capabilities"], false)?;
    let source_complete;
    if rust {
        ensure!(
            result["profile"] == "semantica.refinery-rust-observation/1",
            "refinery_rust_engagement_profile_invalid"
        );
        keys(
            &result["runtime_source"],
            &[
                "schema",
                "source_sha256",
                "executable_sha256",
                "executable_size_bytes",
                "status",
                "compilation_origin",
                "engineering_adoption",
            ],
            &[],
        )?;
        let source = &result["runtime_source"];
        ensure!(
            source["schema"] == "semantica.refinery-rust-source/1",
            "refinery_rust_source_schema_invalid"
        );
        sha(&source["source_sha256"])?;
        sha(&source["executable_sha256"])?;
        ensure!(
            source["executable_size_bytes"]
                .as_u64()
                .is_some_and(|size| size > 0),
            "refinery_rust_source_size_invalid"
        );
        ensure!(
            source["compilation_origin"] == "unknown"
                && source["engineering_adoption"] == "unknown",
            "refinery_rust_source_cannot_self_authorize"
        );
        text(&source["status"])?;
        let _: Value = typed!(SemanticRefineryRustSource, source.clone())?;
        source_complete = source["status"] == "source_bound";
    } else {
        result["runtime_source"] = runtime_source(&result["runtime_source"])?;
        source_complete = legacy_source_complete(&result["runtime_source"]);
    }
    let mut reasons = BTreeSet::new();
    if !source_complete {
        reasons.insert("runtime_source_identity".to_owned());
    }
    for name in PHASES {
        let normalized = phase(&result[*name])?;
        ensure!(
            normalized["name"] == *name,
            "refinery_engagement_phase_order_invalid"
        );
        let status = normalized["status"].as_str().unwrap();
        if status != "passed" {
            reasons.insert(format!("phase.{name}.{status}"));
        }
        for capability in normalized["missing_capabilities"].as_array().unwrap() {
            reasons.insert(format!(
                "capability.{name}.{}",
                capability.as_str().unwrap()
            ));
        }
        if normalized["evidence_sha256"].is_null() {
            reasons.insert(format!("evidence.{name}"));
        }
        result[*name] = normalized;
    }
    result["learning"] = learning(&result["learning"])?;
    let references = result["execution_receipts"]
        .as_array()
        .context("refinery_execution_receipt_list_required")?
        .iter()
        .map(execution_reference)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        references
            .iter()
            .map(|v| v["receipt_sha256"].as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == references.len(),
        "refinery_duplicate_execution_receipt"
    );
    if references.is_empty() {
        reasons.insert("execution_receipt".into());
    }
    result["execution_receipts"] = json!(references);
    let observed = texts(&result["execution"]["observed_capabilities"], true)?;
    for capability in required {
        if !observed.contains(&capability) {
            reasons.insert(format!("capability.engagement.{capability}"));
        }
    }
    let reasons: Vec<_> = reasons.into_iter().collect();
    let status = if reasons.is_empty() {
        "complete"
    } else {
        "blocked"
    };
    if !derive {
        ensure!(
            result["blocked_reasons"] == json!(reasons),
            "refinery_engagement_blocked_reasons_mismatch"
        );
        ensure!(
            result["status"] == status,
            "refinery_engagement_status_mismatch"
        );
    }
    result["blocked_reasons"] = json!(reasons);
    result["status"] = json!(status);
    timestamp(&result["created_at"])?;
    hashed(result, value, "receipt_sha256")
}
/// Original receipt data compatibility only; even `complete` is not trusted.
pub fn engagement(value: &Value) -> Result<Value> {
    typed!(
        SemanticRefinerySemanticEngagementReceipt,
        engagement_content(value, false, false)?
    )
}
/// Explicit Rust observation, verified structurally. Actual backend/source
/// binding and authorization are host-owned checks outside serialized JSON.
pub fn rust_engagement(value: &Value) -> Result<Value> {
    typed!(
        SemanticRefineryRustEngagement,
        engagement_content(value, true, false)?
    )
}
/// Derive metadata from internally produced phase observations. This builds
/// receipt data; execution authority remains outside JSON in the host backend.
/// Public strict receipt decoding keeps all declared metadata requirements.
pub fn prepare_rust_engagement(value: &Value) -> Result<Value> {
    typed!(
        SemanticRefineryRustEngagement,
        engagement_content(value, true, true)?
    )
}
