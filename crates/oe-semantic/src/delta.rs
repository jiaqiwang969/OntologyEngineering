//! Content and authority validation for native refinery DTOs. These checks do
//! not evaluate semantics, grant approval, or mutate the native registry.
use crate::contracts::{self as c, ContractErrorKind as Kind, ProjectBinding};
use anyhow::Result;
use base64::{
    Engine, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use oe_core::{json::canonical_json, sha256};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub const ASSET_CATEGORIES: &[&str] = &[
    "ontology",
    "competency_questions",
    "shapes",
    "queries",
    "rules",
    "cases",
    "contract",
    "provenance",
];
const DELTA_REQUIRED: &[&str] = &[
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

fn exact_hash(value: &Value, name: &str) -> Result<String> {
    c::exact_text(value, name)?;
    c::hash(value, name)
}
fn exact_timestamp(value: &Value, name: &str) -> Result<String> {
    c::exact_text(value, name)?;
    c::timestamp(value, name)
}
fn opaque(value: &Value, name: &str) -> Result<String> {
    let text = c::exact_text(value, name)?;
    c::valid(
        text.len() <= 200
            && text.as_bytes()[0].is_ascii_alphanumeric()
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:@+-".contains(&b)),
        format!("{name} must be an opaque identifier, never a path"),
    )?;
    Ok(text)
}
fn percent_decode(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
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
            continue;
        }
        result.push(bytes[index]);
        index += 1;
    }
    result
}
fn native_source(value: &Value) -> Result<Value> {
    c::keys(
        value,
        &["source_id", "uri", "sha256", "media_type", "captured_at"],
        &[],
        "source evidence",
    )?;
    let uri = c::exact_text(&value["uri"], "source.uri")?;
    let scheme = uri
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    c::valid(
        scheme
            .as_deref()
            .is_some_and(|s| ["urn", "semantica", "evidence", "ni"].contains(&s))
            && !uri.contains('\\'),
        "source requires a non-resolving logical URI",
    )?;
    c::valid(
        !percent_decode(&uri)
            .split(|b| b"/:?#".contains(b))
            .any(|part| part == b"." || part == b".."),
        "source URI must not contain encoded or literal dot segments",
    )?;
    Ok(
        json!({"source_id":c::exact_text(&value["source_id"],"source_id")?,"uri":uri,
        "sha256":exact_hash(&value["sha256"],"source.sha256")?,"media_type":c::exact_text(&value["media_type"],"source.media_type")?,
        "captured_at":exact_timestamp(&value["captured_at"],"source.captured_at")?}),
    )
}
// Python's validated base64 accepts nonzero trailing bits and surplus padding
// after complete groups. Preserve that byte contract without accepting missing
// required padding, whitespace, internal padding, or arbitrary nonalphabet data.
fn content_bytes(value: &Value) -> Result<Vec<u8>> {
    let encoded = value
        .as_str()
        .ok_or_else(|| c::invalid("asset content_base64 must be text"))?;
    let data = encoded.trim_end_matches('=');
    let padding = encoded.len() - data.len();
    c::valid(
        data.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/".contains(&b))
            && match data.len() % 4 {
                0 => !data.is_empty() || padding == 0,
                2 => padding == 2,
                3 => padding == 1,
                _ => false,
            },
        "invalid asset content_base64",
    )?;
    GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    )
    .decode(data)
    .map_err(|_| c::invalid("invalid asset content_base64"))
}
fn asset(value: &Value, category: &str) -> Result<Value> {
    c::keys(
        value,
        &["category", "asset_id", "operation", "media_type", "sha256"],
        &["content_base64", "replaces_sha256", "role", "case_kind"],
        "asset delta",
    )?;
    c::valid(
        value["category"] == category,
        "asset category differs from containing array",
    )?;
    let operation = c::exact_text(&value["operation"], "asset.operation")?;
    c::valid(
        ["add", "replace", "remove"].contains(&operation.as_str()),
        "unsupported asset operation",
    )?;
    let digest = exact_hash(&value["sha256"], "asset.sha256")?;
    let replaces = if value["replaces_sha256"].is_null() {
        Value::Null
    } else {
        json!(exact_hash(
            &value["replaces_sha256"],
            "asset.replaces_sha256"
        )?)
    };
    let role = if value["role"].is_null() {
        Value::Null
    } else {
        json!(c::exact_text(&value["role"], "asset.role")?)
    };
    let case_kind = value["case_kind"].clone();
    if category == "cases" {
        c::valid(
            case_kind.as_str().is_some_and(|kind| {
                ["positive", "negative", "ambiguity", "prior_release"].contains(&kind)
            }),
            "case asset requires an exact supported case_kind",
        )?;
    } else {
        c::valid(case_kind.is_null(), "only case assets may have case_kind")?;
    }
    if operation == "remove" {
        c::valid(
            value["content_base64"].is_null() && replaces == digest,
            "remove must identify exact old content and contain no new content",
        )?;
    } else {
        c::valid(
            sha256(content_bytes(&value["content_base64"])?) == digest,
            "asset content hash mismatch",
        )?;
        c::valid(
            if operation == "replace" {
                !replaces.is_null()
            } else {
                replaces.is_null()
            },
            "asset operation has invalid old-content hash",
        )?;
    }
    Ok(
        json!({"category":category,"asset_id":opaque(&value["asset_id"],"asset_id")?,"operation":operation,
        "media_type":c::exact_text(&value["media_type"],"asset.media_type")?,"sha256":digest,"content_base64":value["content_base64"],
        "replaces_sha256":replaces,"role":role,"case_kind":case_kind}),
    )
}
pub fn empty_package_sha256() -> Result<String> {
    Ok(sha256(canonical_json(
        &json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]}),
    )?))
}
/// Validate an already-native DTO (including its scoped creator token) and
/// return exactly its canonical native representation and verified digest.
pub fn validate_native_delta(value: &Value) -> Result<Value> {
    c::forbid_selection(value)?;
    c::keys(
        value,
        DELTA_REQUIRED,
        &["schema_version", "delta_sha256"],
        "native delta",
    )?;
    c::valid(
        value.get("schema_version").is_none_or(|v| v == "1.0"),
        "unsupported delta schema_version",
    )?;
    let package_id = opaque(&value["package_id"], "package_id")?;
    let base = c::exact_text(&value["base_version"], "base_version")?;
    let baseline = exact_hash(&value["base_package_sha256"], "base_package_sha256")?;
    let target = c::exact_text(&value["target_version"], "target_version")?;
    c::valid(
        target != base,
        "target version must differ from base version",
    )?;
    c::valid(
        base != "0" || baseline == empty_package_sha256()?,
        "base version 0 requires empty package identity",
    )?;
    let capabilities = c::unique_texts(
        &value["required_capabilities"],
        true,
        false,
        "required_capabilities",
    )?;
    for item in &value["required_capabilities"].as_array().unwrap()[..] {
        c::exact_text(item, "required_capabilities")?;
    }
    let sources = value["source_evidence"]
        .as_array()
        .ok_or_else(|| c::invalid("source_evidence must be a list"))?;
    c::valid(!sources.is_empty(), "delta requires source evidence")?;
    let mut ids = BTreeSet::new();
    let sources = sources
        .iter()
        .map(|item| {
            let source = native_source(item)?;
            c::valid(
                ids.insert(source["source_id"].as_str().unwrap().to_owned()),
                "duplicate source_id",
            )?;
            Ok(source)
        })
        .collect::<Result<Vec<_>>>()?;
    c::valid(
        value["book_impact"]
            .as_str()
            .is_some_and(|v| ["none", "vol1-method", "vol2-iso-exemplar", "both"].contains(&v)),
        "unsupported book_impact",
    )?;
    let mut result = json!({"schema_version":"1.0","package_id":package_id,"base_version":base,"base_package_sha256":baseline,"target_version":target,
        "rationale":c::exact_text(&value["rationale"],"rationale")?,"created_by":c::exact_text(&value["created_by"],"created_by")?,"created_at":exact_timestamp(&value["created_at"],"created_at")?,
        "required_capabilities":capabilities,"source_evidence":sources,"book_impact":value["book_impact"]});
    let mut asset_ids = BTreeSet::new();
    for category in ASSET_CATEGORIES {
        let raw = value[category]
            .as_array()
            .ok_or_else(|| c::invalid("every asset category must be an explicit list"))?;
        let normalized = raw
            .iter()
            .map(|item| {
                let item = asset(item, category)?;
                c::valid(
                    asset_ids.insert(item["asset_id"].as_str().unwrap().to_owned()),
                    "asset_id must be globally unique across all categories",
                )?;
                Ok(item)
            })
            .collect::<Result<Vec<_>>>()?;
        result[category] = json!(normalized);
    }
    c::valid(
        !asset_ids.is_empty(),
        "delta requires at least one asset mutation",
    )?;
    let digest = sha256(canonical_json(&result)?);
    c::valid(
        value.get("delta_sha256").is_none_or(|v| v == &digest),
        "declared delta_sha256 differs from content",
    )?;
    result["delta_sha256"] = json!(digest);
    Ok(result)
}
pub fn read_delta(path: &Path, binding: &ProjectBinding) -> Result<Value> {
    parse_delta(&c::read(path)?, binding)
}
pub fn parse_delta(raw: &[u8], binding: &ProjectBinding) -> Result<Value> {
    let mut value = c::decode(raw)?;
    c::valid(
        value.get("schema_version").is_some(),
        "owned delta requires schema_version",
    )?;
    c::check(
        value["package_id"] == binding.package_id
            && value["base_version"] == binding.baseline_version
            && value["base_package_sha256"] == binding.baseline_digest,
        Kind::BaselineMismatchError,
        "delta package/baseline differs from binding",
    )?;
    c::check(
        value["created_by"] == binding.fact_authority["authority_id"],
        Kind::ActionNotAllowedError,
        "delta creator must exactly match declared fact authority_id",
    )?;
    for category in ASSET_CATEGORIES {
        if let Some(assets) = value[category].as_array() {
            for item in assets {
                c::valid(
                    item.get("content_base64").is_some(),
                    "owned asset delta requires content_base64, including null for removal",
                )?;
            }
        }
    }
    value["created_by"] = json!(c::authority_token(&binding.fact_authority)?);
    validate_native_delta(&value)
}
fn decisions(value: &Value, normalize: bool) -> Result<Vec<Value>> {
    let array = value
        .as_array()
        .ok_or_else(|| c::invalid("decisions must be a list"))?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for item in array {
        c::keys(
            item,
            &[
                "category",
                "asset_id",
                "operation",
                "replaces_sha256",
                "verdict",
                "reason",
            ],
            &[],
            "asset decision",
        )?;
        let mut decision = json!({});
        for name in [
            "category",
            "asset_id",
            "operation",
            "replaces_sha256",
            "verdict",
            "reason",
        ] {
            decision[name] = json!(if normalize {
                c::text(&item[name], name)?
            } else {
                c::exact_text(&item[name], name)?
            });
        }
        c::valid(
            ASSET_CATEGORIES.contains(&decision["category"].as_str().unwrap()),
            "unsupported decision category",
        )?;
        opaque(&decision["asset_id"], "decision.asset_id")?;
        c::valid(
            ["replace", "remove"].contains(&decision["operation"].as_str().unwrap()),
            "decision operation must be replace or remove",
        )?;
        exact_hash(&decision["replaces_sha256"], "decision.replaces_sha256")?;
        c::valid(
            decision["verdict"] == "approve",
            "asset decision must explicitly approve",
        )?;
        c::valid(
            seen.insert(decision_key(&decision)),
            "duplicate asset decision",
        )?;
        result.push(decision);
    }
    if normalize {
        result.sort_by_key(decision_key);
    }
    Ok(result)
}
fn decision_key(value: &Value) -> Vec<String> {
    ["category", "asset_id", "operation", "replaces_sha256"]
        .iter()
        .map(|key| value[key].as_str().unwrap_or("").to_owned())
        .collect()
}
/// Validate and hash the native authorization DTO without inferring permission.
/// Binding/action permission still requires parse_authorization or the native
/// registry's authority checks, not this structural function alone.
pub fn validate_native_authorization(value: &Value) -> Result<Value> {
    c::forbid_selection(value)?;
    c::keys(
        value,
        &[
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
        ],
        &["authorization_sha256"],
        "native authorization",
    )?;
    c::valid(
        value["action"] == "commit" || value["action"] == "promote",
        "authorization action must be commit or promote",
    )?;
    c::valid(
        value["promotion_target"] == c::PROMOTION_TARGET,
        "authorization target must be industry-registry",
    )?;
    let decisions = decisions(&value["decisions"], false)?;
    let mut result = json!({"authorization_id":c::exact_text(&value["authorization_id"],"authorization_id")?,"action":value["action"],
        "actor_id":c::exact_text(&value["actor_id"],"actor_id")?,"authority":c::exact_text(&value["authority"],"authority")?,
        "package_id":opaque(&value["package_id"],"package_id")?,"delta_sha256":exact_hash(&value["delta_sha256"],"delta_sha256")?,
        "promotion_target":c::PROMOTION_TARGET,"reason":c::exact_text(&value["reason"],"reason")?,"source":native_source(&value["source"])?,
        "issued_at":exact_timestamp(&value["issued_at"],"issued_at")?,"decisions":decisions});
    let digest = sha256(canonical_json(&result)?);
    c::valid(
        value
            .get("authorization_sha256")
            .is_none_or(|v| v == &digest),
        "authorization_sha256 differs from content",
    )?;
    result["authorization_sha256"] = json!(digest);
    Ok(result)
}
pub fn read_authorization(
    path: &Path,
    binding: &ProjectBinding,
    expected_action: &str,
    candidate_sha256: &str,
) -> Result<Value> {
    parse_authorization(&c::read(path)?, binding, expected_action, candidate_sha256)
}
pub fn parse_authorization(
    raw: &[u8],
    binding: &ProjectBinding,
    expected_action: &str,
    candidate_sha256: &str,
) -> Result<Value> {
    c::valid(
        ["commit", "promote"].contains(&expected_action),
        "authorization expected action must be commit or promote",
    )?;
    exact_hash(&json!(candidate_sha256), "expected candidate_sha256")?;
    let value = c::decode(raw)?;
    c::forbid_selection(&value)?;
    c::keys(
        &value,
        &[
            "$schema",
            "authorization_id",
            "action",
            "actor_id",
            "authority_id",
            "authority_scope",
            "package_id",
            "delta_sha256",
            "promotion_target",
            "reason",
            "source",
            "issued_at",
            "decisions",
        ],
        &[],
        "authorization",
    )?;
    c::valid(
        value["$schema"] == c::AUTHORIZATION_SCHEMA,
        "unsupported authorization schema",
    )?;
    c::check(
        value["action"] == expected_action
            && value["actor_id"] == binding.decision_authority["authority_id"]
            && value["authority_id"] == binding.decision_authority["authority_id"],
        Kind::ActionNotAllowedError,
        "authorization action/actor/authority differs from current decision authority",
    )?;
    let mut scope = c::unique_texts(&value["authority_scope"], true, true, "authority_scope")?;
    scope.sort();
    c::check(
        json!(scope) == binding.decision_authority["scope"],
        Kind::ActionNotAllowedError,
        "authorization scope differs from binding",
    )?;
    c::check(
        value["package_id"] == binding.package_id
            && value["delta_sha256"] == candidate_sha256
            && value["promotion_target"] == binding.promotion_target,
        Kind::BaselineMismatchError,
        "authorization package/candidate/target differs from binding",
    )?;
    // Check the owned root relation, then retain and validate every native source
    // byte-valued field. Whitespace is not silently discarded from evidence.
    c::evidence_ref(&value["source"], Some(&binding.evidence_logical_root))?;
    let token = c::authority_token(&binding.decision_authority)?;
    let decisions = decisions(&value["decisions"], true)?;
    c::check(
        expected_action != "promote" || decisions.is_empty(),
        Kind::ActionNotAllowedError,
        "promotion authorization must not contain asset mutation decisions",
    )?;
    validate_native_authorization(
        &json!({"authorization_id":c::text(&value["authorization_id"],"authorization_id")?,"action":expected_action,
        "actor_id":token,"authority":token,"package_id":binding.package_id,"delta_sha256":candidate_sha256,"promotion_target":binding.promotion_target,
        "reason":c::text(&value["reason"],"reason")?,"source":value["source"],"issued_at":value["issued_at"],"decisions":decisions}),
    )
}
/// Exact destructive decision coverage; extra approvals are not reusable grants.
/// Native Semantica still validates the baseline's actual asset hashes at commit.
pub fn validate_asset_decisions(delta: &Value, authorization: &Value) -> Result<()> {
    let delta = validate_native_delta(delta)?;
    let authorization = validate_native_authorization(authorization)?;
    c::check(
        delta["package_id"] == authorization["package_id"]
            && delta["delta_sha256"] == authorization["delta_sha256"],
        Kind::BaselineMismatchError,
        "authorization does not bind exact delta",
    )?;
    let actual: BTreeSet<_> = authorization["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(decision_key)
        .collect();
    let expected: BTreeSet<_> = if authorization["action"] == "commit" {
        ASSET_CATEGORIES
            .iter()
            .flat_map(|category| delta[category].as_array().unwrap())
            .filter(|item| item["operation"] != "add")
            .map(decision_key)
            .collect()
    } else {
        BTreeSet::new()
    };
    c::check(
        actual == expected,
        Kind::ActionNotAllowedError,
        "authorization decisions must exactly match the current action's destructive mutations",
    )
}
