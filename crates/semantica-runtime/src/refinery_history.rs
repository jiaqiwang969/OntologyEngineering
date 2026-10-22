//! Pinned refinery history data rules. A well-formed event is not evidence of
//! authorization, actual semantic execution, adoption, or a verified chain.
//!
//! Workspace and pointer parsing deliberately retain the original weak data
//! domains. The store owner separately binds workspace identity, exact bytes,
//! event order, current tip, retained objects and the complete chain.
use crate::refinery_contracts as wire;
use anyhow::{Context, Result, ensure};
use serde_json::Value;

fn fields(value: &Value, allowed: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .context("refinery_history_object_required")?;
    ensure!(
        object.keys().all(|key| allowed.contains(&key.as_str())),
        "refinery_history_unknown_field"
    );
    Ok(())
}
fn text(value: &Value) -> Result<&str> {
    let text = value.as_str().context("refinery_history_text_required")?;
    ensure!(
        !text.is_empty()
            && text == text
                .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)),
        "refinery_history_nonempty_exact_text_required"
    );
    Ok(text)
}
fn opaque(value: &Value) -> Result<()> {
    wire::opaque_identifier(value).map(|_| ())
}
fn sha(value: &Value) -> Result<()> {
    ensure!(
        value.as_str().is_some_and(wire::valid_sha),
        "refinery_history_sha256_invalid"
    );
    Ok(())
}
fn nullable_sha(value: &Value) -> Result<()> {
    if value.is_null() { Ok(()) } else { sha(value) }
}

/// Exact original workspace data checks plus the caller's existing registry
/// binding. Original extra fields and an unchecked created_at remain data.
pub fn workspace(value: &Value, registry_id: &str) -> Result<()> {
    ensure!(value.is_object(), "refinery_workspace_object_required");
    ensure!(
        value["schema_version"] == wire::SCHEMA,
        "refinery_workspace_schema_invalid"
    );
    ensure!(
        value["contract"] == wire::CONTRACT,
        "refinery_workspace_contract_invalid"
    );
    ensure!(
        value["empty_package_sha256"] == wire::EMPTY_PACKAGE_SHA256,
        "refinery_workspace_empty_identity_invalid"
    );
    opaque(&value["registry_id"])?;
    // No new registry or permission is inferred from the workspace document.
    ensure!(
        value["registry_id"] == registry_id,
        "refinery_workspace_owner_identity_mismatch"
    );
    Ok(())
}

fn original_pointer_sequence(value: &Value) -> bool {
    // The original dataclass tests sequence<1, unlike immutable event validation
    // it has no integer/non-bool check or state-index equality. Keep that data
    // distinction; production chain verification must use strict event order.
    match value {
        Value::Bool(value) => *value,
        Value::Number(number) => {
            let text = number.to_string();
            if text.contains(['.', 'e', 'E']) {
                number
                    .as_f64()
                    .is_some_and(|value| value.is_finite() && value >= 1.0)
            } else {
                !text.starts_with('-')
                    && text.bytes().all(|byte| byte.is_ascii_digit())
                    && text.bytes().any(|byte| byte != b'0')
            }
        }
        _ => false,
    }
}
/// Original _state_from_pointer parsing only. The two nullable fields may be
/// absent. Its weak sequence domain does not attest a valid event chain.
pub fn pointer(value: &Value) -> Result<()> {
    fields(
        value,
        &[
            "schema_version",
            "delta_sha256",
            "package_id",
            "state",
            "sequence",
            "event_sha256",
            "previous_event_sha256",
            "package_sha256",
            "recorded_at",
        ],
    )?;
    ensure!(
        value["schema_version"] == wire::SCHEMA,
        "refinery_pointer_schema_invalid"
    );
    sha(&value["delta_sha256"])?;
    opaque(&value["package_id"])?;
    let state = text(&value["state"])?;
    ensure!(
        wire::STATES.contains(&state),
        "refinery_pointer_state_invalid"
    );
    ensure!(
        original_pointer_sequence(&value["sequence"]),
        "refinery_pointer_sequence_invalid"
    );
    sha(&value["event_sha256"])?;
    nullable_sha(&value["previous_event_sha256"])?;
    nullable_sha(&value["package_sha256"])?;
    ensure!(
        ![
            "committed",
            "regression_passed",
            "release_complete",
            "promoted"
        ]
        .contains(&state)
            || !value["package_sha256"].is_null(),
        "refinery_pointer_postcommit_package_missing"
    );
    wire::timestamp(&value["recorded_at"])?;
    Ok(())
}

fn payload_fields(state: &str) -> Option<&'static [&'static str]> {
    Some(match state {
        "candidate" => &[
            "envelope_sha256",
            "binding_sha256",
            "engagement_receipt_sha256",
            "engagement_status",
            "transition_context_sha256",
            "transition_context_object_sha256",
        ],
        "proposed" => &[
            "proposal",
            "transition_context_sha256",
            "transition_context_object_sha256",
        ],
        "committed" => &[
            "manifest_sha256",
            "authorization_sha256",
            "authorization_object_sha256",
            "engagement_receipt_sha256",
            "transition_context_sha256",
            "transition_context_object_sha256",
        ],
        "regression_passed" => &[
            "regression_evidence_sha256",
            "regression_evidence_object_sha256",
            "execution_suite_sha256",
            "execution_suite_object_sha256",
            "transition_context_sha256",
            "transition_context_object_sha256",
            "provenance_closure_sha256",
            "provenance_closure_object_sha256",
        ],
        "release_complete" => &[
            "release_evidence_sha256",
            "release_evidence_object_sha256",
            "execution_suite_sha256",
            "execution_suite_object_sha256",
            "transition_context_sha256",
            "transition_context_object_sha256",
            "provenance_closure_sha256",
            "provenance_closure_object_sha256",
        ],
        "promoted" => &[
            "promotion_record_sha256",
            "authorization_sha256",
            "authorization_object_sha256",
            "promotion_target",
            "transition_context_sha256",
            "transition_context_object_sha256",
            "provenance_closure_sha256",
            "provenance_closure_object_sha256",
        ],
        _ => return None,
    })
}
fn payload(state: &str, value: &Value) -> Result<()> {
    let expected = payload_fields(state).context("refinery_event_payload_state_invalid")?;
    fields(value, expected)?;
    let object = value.as_object().unwrap();
    ensure!(
        object.len() == expected.len() && expected.iter().all(|field| object.contains_key(*field)),
        "refinery_event_payload_missing_field"
    );
    for field in expected {
        match *field {
            "proposal" | "promotion_target" => {
                text(&value[*field])?;
            }
            "engagement_status" => ensure!(
                value[*field] == "complete" || value[*field] == "blocked",
                "refinery_event_engagement_status_invalid"
            ),
            _ => sha(&value[*field])?,
        }
    }
    Ok(())
}
/// Complete strict original immutable event structure, all six payloads and
/// exact canonical hash. Actual prior/object/authority checks remain separate.
pub fn event(value: &Value) -> Result<()> {
    fields(
        value,
        &[
            "schema_version",
            "sequence",
            "state",
            "delta_sha256",
            "package_id",
            "previous_event_sha256",
            "package_sha256",
            "recorded_at",
            "payload",
            "event_sha256",
        ],
    )?;
    ensure!(
        value["schema_version"] == wire::SCHEMA,
        "refinery_event_schema_invalid"
    );
    let state = text(&value["state"])?;
    let index = wire::STATES
        .iter()
        .position(|candidate| *candidate == state)
        .context("refinery_event_state_invalid")?;
    // as_u64 accepts integer tokens only: booleans, floats and negative tokens
    // cannot silently acquire the original strictly integer event sequence.
    let sequence = value["sequence"]
        .as_u64()
        .context("refinery_event_integer_sequence_required")?;
    ensure!(
        sequence == index as u64 + 1,
        "refinery_event_state_sequence_mismatch"
    );
    sha(&value["delta_sha256"])?;
    opaque(&value["package_id"])?;
    if sequence == 1 {
        ensure!(
            value["previous_event_sha256"].is_null(),
            "refinery_candidate_predecessor_forbidden"
        );
    } else {
        sha(&value["previous_event_sha256"])?;
    }
    if sequence <= 2 {
        ensure!(
            value["package_sha256"].is_null(),
            "refinery_precommit_package_forbidden"
        );
    } else {
        sha(&value["package_sha256"])?;
    }
    wire::timestamp(&value["recorded_at"])?;
    payload(state, &value["payload"])?;
    sha(&value["event_sha256"])?;
    let mut content = value.clone();
    content.as_object_mut().unwrap().remove("event_sha256");
    ensure!(
        value["event_sha256"] == wire::digest(&content)?,
        "refinery_event_content_hash_mismatch"
    );
    Ok(())
}
