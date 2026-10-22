//! CQ-driven premise selection advisor.
//!
//! Reads a completed routing result and recommends a `selections` array
//! suitable for passing directly to `oe_semantic::build_premises()`.
//!
//! Design principle: the two books (Vol.1 / Vol.2) plus the domain semantic
//! packages define *which questions are worth asking* for a given capability.
//! The advisor maps route capability_ids → relevant CQ asset_ids, so the
//! engineer does not have to manually cross-reference package manifests.

use anyhow::{Context, Result, ensure};
use oe_core::{AdvisoryFileLock, SkillRoot, read_regular_bytes};
use oe_semantic::LoadedBundle;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Bundle → CQ mapping (source-of-truth: semantic-bundles.json lock)
// ---------------------------------------------------------------------------

/// A single CQ entry resolved from a semantic package.
#[derive(Debug, Clone)]
pub struct ResolvedCq {
    pub bundle: String,
    pub cq_id: String,
    pub question: String,
    /// SPARQL asset_id (e.g. "q01"), if present in the manifest.
    pub sparql_asset_id: Option<String>,
}

/// A recommended selection for one capability route.
#[derive(Debug, Clone)]
pub struct SelectionRecommendation {
    pub capability_id: String,
    pub bundle: String,
    pub asset_ids: Vec<String>,
    pub purpose: String,
    /// "caller_assertion_not_established" per protocol.
    pub applicability: Value,
    pub cqs: Vec<ResolvedCq>,
}

impl SelectionRecommendation {
    /// Serialise into the shape `build_premises()` / `oe semantics premises` expects.
    pub fn to_selection(&self) -> Value {
        json!({
            "bundle": self.bundle,
            "asset_ids": self.asset_ids,
            "purpose": self.purpose,
            "applicability": self.applicability,
        })
    }
}

// ---------------------------------------------------------------------------
// Route result ingestion
// ---------------------------------------------------------------------------

/// Minimal view of a single capability verdict from `routing.json`.
#[derive(Debug, Clone)]
pub struct CapabilityVerdict {
    pub capability_id: String,
    pub need: String, // "needed" | "not_needed" | "not_established"
    /// Missing or null model metadata remains unknown, never synthetic zero.
    pub confidence: Option<f64>,
}

pub fn read_routing_result(routing_path: &Path) -> Result<Vec<CapabilityVerdict>> {
    let bytes = read_regular_bytes(routing_path, 4 * 1024 * 1024)
        .with_context(|| format!("cannot read routing result: {}", routing_path.display()))?;
    let doc = oe_core::json::strict_json(&bytes)?;

    let routes = doc
        .get("routes")
        .and_then(Value::as_array)
        .context("routing result missing routes array")?;

    ensure!(routes.len() <= 256, "premise_routing_count_limit");
    let mut verdicts = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for route in routes {
        ensure!(route.is_object(), "premise_route_must_be_object");
        let cap_id = route
            .get("capability_id")
            .and_then(Value::as_str)
            .filter(|value| {
                !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
            })
            .context("premise_route_capability_id_required")?
            .to_owned();
        ensure!(
            seen.insert(cap_id.clone()),
            "premise_route_duplicate_capability"
        );
        let need = match route.get("need") {
            None | Some(Value::Null) => "not_established",
            Some(value) => value
                .as_str()
                .filter(|value| matches!(*value, "needed" | "not_needed" | "not_established"))
                .context("premise_route_need_invalid")?,
        }
        .to_owned();
        let confidence = match route.get("answer") {
            None | Some(Value::Null) => None,
            Some(answer) => {
                ensure!(answer.is_object(), "premise_route_answer_must_be_object");
                match answer.get("confidence") {
                    None | Some(Value::Null) => None,
                    Some(value) => Some(
                        value
                            .as_f64()
                            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                            .context("premise_route_confidence_invalid")?,
                    ),
                }
            }
        };
        verdicts.push(CapabilityVerdict {
            capability_id: cap_id,
            need,
            confidence,
        });
    }
    Ok(verdicts)
}

// ---------------------------------------------------------------------------
// Package CQ index (read from installed bundle zip)
// ---------------------------------------------------------------------------

/// Read CQs only from a transport-validated, runtime-bound bundle snapshot.
fn load_cq_registry(bundle: &LoadedBundle) -> Result<Vec<Value>> {
    let Some(bytes) = bundle.payload.get("assets/cq-registry.json") else {
        return Ok(vec![]);
    };
    let doc = oe_core::json::strict_json(bytes)?;
    doc.get("competency_questions")
        .and_then(Value::as_array)
        .cloned()
        .context("CQ registry missing competency_questions array")
}

/// Build a map: cq_id → sparql asset_id from package manifest assets.
/// Handles both numbered format (q01..q20) and named format (query-pattern-*).
fn sparql_asset_map(bundle: &LoadedBundle) -> Result<BTreeMap<usize, String>> {
    let assets = bundle
        .manifest
        .get("assets")
        .and_then(Value::as_array)
        .context("verified manifest missing assets array")?;

    // Assets named "q01".."q20" with role "sparql" correspond to CQ index 1..20
    let mut result = BTreeMap::new();
    for asset in assets {
        let id = asset.get("asset_id").and_then(Value::as_str).unwrap_or("");
        let role = asset.get("role").and_then(Value::as_str).unwrap_or("");
        let format = asset.get("format").and_then(Value::as_str).unwrap_or("");
        if (role == "sparql" || format == "sparql")
            && let Ok(n) = id.trim_start_matches('q').parse::<usize>()
        {
            result.insert(n, id.to_owned());
        }
    }
    Ok(result)
}

/// For packages using named SPARQL assets (not q01..q20), collect all sparql asset_ids.
fn named_sparql_assets(bundle: &LoadedBundle) -> Result<Vec<String>> {
    let assets = bundle
        .manifest
        .get("assets")
        .and_then(Value::as_array)
        .context("verified manifest missing assets array")?;

    let mut result = Vec::new();
    for asset in assets {
        let id = asset.get("asset_id").and_then(Value::as_str).unwrap_or("");
        let role = asset.get("role").and_then(Value::as_str).unwrap_or("");
        let format = asset.get("format").and_then(Value::as_str).unwrap_or("");
        if (role == "sparql" || format == "sparql") && !id.is_empty() {
            result.push(id.to_owned());
        }
    }
    Ok(result)
}

/// Which named SPARQL asset_ids are most relevant for each capability
/// in the engineering-judgment-intake package.
fn named_assets_for_capability(capability_id: &str) -> &'static [&'static str] {
    match capability_id {
        // evidence: identity (同一对象核验), claim (断言与来源), scope (范围保留)
        "evidence" => &[
            "query-pattern-identity",
            "query-pattern-claim",
            "query-pattern-scope",
        ],
        // project_modeling: identity, realization (功能→实现链路), change (变更依赖)
        "project_modeling" => &[
            "query-pattern-identity",
            "query-pattern-realization",
            "query-pattern-change",
        ],
        // semantic_review: claim, realization, scope
        "semantic_review" | "semantic_integration" => &[
            "query-pattern-claim",
            "query-pattern-realization",
            "query-pattern-scope",
        ],
        // ontology_evolution: change, claim
        "ontology_evolution" => &["query-pattern-change", "query-pattern-claim"],
        _ => &[],
    }
}

/// Canonical map from route capability_id to the bundle name in semantic-bundles.json.
/// Extend this as new domain packages are added.
fn capability_to_bundle(capability_id: &str) -> Option<&'static str> {
    match capability_id {
        "manufacturing" => Some("manufacturing-process-cost"),
        "evidence" => Some("engineering-judgment-intake"),
        "semantic_review" | "project_modeling" | "semantic_integration" | "ontology_evolution" => {
            Some("engineering-judgment-intake")
        }
        // cad, external_specialist: no dedicated semantic package yet
        _ => None,
    }
}

/// Which CQ indices (1-based) are most relevant for each capability?
/// Keeps recommendations focused rather than dumping all 23 CQs.
fn priority_cqs_for_capability(capability_id: &str) -> &'static [usize] {
    match capability_id {
        // manufacturing: version scope, resource vs capability, cost completeness,
        //                quantity basis, change propagation
        "manufacturing" => &[3, 5, 6, 7, 10],
        // evidence: source role/conflict, partial answer closure, cross-call state
        "evidence" => &[2, 9, 13],
        // semantic review / project modeling: feedback boundary, reusable method
        "semantic_review" | "project_modeling" => &[15, 16, 18],
        // semantic_integration: change applicability, version scope
        "semantic_integration" => &[1, 3],
        // ontology evolution: feedback to reusable method, closure
        "ontology_evolution" => &[16, 20],
        _ => &[],
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct AdviceResult {
    pub recommendations: Vec<SelectionRecommendation>,
    pub unmatched_capabilities: Vec<String>,
    pub selections_json: Value,
}

/// A caller report has no attached native receipt, project revision or baseline.
/// This type constrains its spelling, not its truth or engineering authority.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ReportedVerdict {
    Passed,
    Blocked,
    NotEstablished,
}
impl ReportedVerdict {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "passed" => Ok(Self::Passed),
            "blocked" => Ok(Self::Blocked),
            "not_established" => Ok(Self::NotEstablished),
            _ => anyhow::bail!("unsupported_reported_verdict"),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Blocked => "blocked",
            Self::NotEstablished => "not_established",
        }
    }
}

impl AdviceResult {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": "ontology-engineering.premise-advice/v1",
            "recommendation_count": self.recommendations.len(),
            "unmatched_capabilities": self.unmatched_capabilities,
            "selections": self.selections_json,
            "selection_status": "recommendations_not_execution",
            "engineering_acceptance": "not_determined",
            "recommendations": self.recommendations.iter().map(|r| json!({
                "capability_id": r.capability_id,
                "bundle": r.bundle,
                "asset_ids": r.asset_ids,
                "purpose": r.purpose,
                "cqs": r.cqs.iter().map(|c| json!({
                    "id": c.cq_id,
                    "question": c.question,
                    "sparql_asset_id": c.sparql_asset_id,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }

    /// Explicitly report selection usage. Frequency is not evidence of validity
    /// or usefulness; this method cannot attest a native execution or outcome.
    pub fn record_loaded(&self, root: &SkillRoot) -> Result<()> {
        write_experience(root, &self.recommendations, None)
    }

    /// Keep a caller-reported verdict separately from native verified outcomes.
    /// No result or receipt is supplied here, so even "passed" is unverified.
    pub fn record_with_verdict(&self, root: &SkillRoot, verdict: &str) -> Result<()> {
        write_experience(
            root,
            &self.recommendations,
            Some(ReportedVerdict::parse(verdict)?),
        )
    }

    /// Return review_request templates for the third pyramid layer.
    ///
    /// For capabilities whose packages use named SPARQL assets (evidence,
    /// project_modeling, semantic_review), generates a template object per
    /// asset that the caller can fill with `evidence_file` and `focus_iri`
    /// before passing to `oe semantics project-review`.  Manufacturing-style
    /// numbered assets are not surfaced here — those require source-aware review, not
    /// project-review.
    pub fn review_templates(&self) -> Vec<Value> {
        // Named assets that map directly to review pattern queries
        let review_patterns: &[&str] = &[
            "query-pattern-identity",
            "query-pattern-claim",
            "query-pattern-scope",
            "query-pattern-realization",
            "query-pattern-change",
        ];
        let shape_for = |query: &str| -> &'static str {
            match query {
                "query-pattern-identity" => "shape-pattern-identity",
                "query-pattern-claim" => "shape-pattern-claim",
                "query-pattern-scope" => "shape-pattern-scope",
                "query-pattern-realization" => "shape-pattern-realization",
                "query-pattern-change" => "shape-pattern-change",
                _ => "",
            }
        };

        let mut templates = Vec::new();
        for rec in &self.recommendations {
            for asset_id in &rec.asset_ids {
                if !review_patterns.contains(&asset_id.as_str()) {
                    continue;
                }
                let shape = shape_for(asset_id.as_str());
                templates.push(json!({
                    "schema": "ontology-engineering.review-request-template/v1",
                    "capability_id": rec.capability_id,
                    "bundle": rec.bundle,
                    "query_asset_id": asset_id,
                    "shape_asset_id": shape,
                    // Caller must supply these two fields:
                    "evidence_file": null,
                    "focus_iri": null,
                    "note": "fill evidence_file and focus_iri, then pass to oe semantics project-review"
                }));
            }
        }
        templates
    }
}

/// Given a completed routing result, recommend `selections` for `build_premises()`.
///
/// Only `needed` capabilities are matched. Returns the full advice including
/// a ready-to-use `selections` array.
pub fn advise(root: &SkillRoot, routing_path: &Path) -> Result<AdviceResult> {
    let verdicts = read_routing_result(routing_path)?;
    let needed: Vec<_> = verdicts.iter().filter(|v| v.need == "needed").collect();

    // Load bundle lock to discover available zips
    let bundles_lock_path = root.path().join("runtime").join("semantic-bundles.json");
    let lock_bytes = read_regular_bytes(&bundles_lock_path, 512 * 1024)
        .context("cannot read semantic-bundles.json")?;
    let lock: Value = oe_core::json::strict_json(&lock_bytes)?;
    let bundles_obj = lock
        .get("bundles")
        .and_then(Value::as_object)
        .context("semantic-bundles.json missing bundles object")?;

    let mut recommendations = Vec::new();
    let mut unmatched = Vec::new();
    let mut loaded_bundles = BTreeMap::<String, LoadedBundle>::new();

    for verdict in &needed {
        let Some(bundle_name) = capability_to_bundle(&verdict.capability_id) else {
            unmatched.push(verdict.capability_id.clone());
            continue;
        };
        if !bundles_obj.contains_key(bundle_name) {
            unmatched.push(format!(
                "{} (bundle {} not in lock)",
                verdict.capability_id, bundle_name
            ));
            continue;
        }
        if !loaded_bundles.contains_key(bundle_name) {
            let bundle = oe_semantic::load_bundle(root.path(), bundle_name)
                .with_context(|| format!("cannot verify premise bundle {bundle_name}"))?;
            loaded_bundles.insert(bundle_name.to_owned(), bundle);
        }
        let bundle = &loaded_bundles[bundle_name];
        let cq_list = load_cq_registry(bundle)?;

        let priority_indices = priority_cqs_for_capability(&verdict.capability_id);
        let named_priority_static = named_assets_for_capability(&verdict.capability_id);

        // Experience-reordered named assets: history-frequent first, static fallback at end.
        let experienced = experienced_priority(root, &verdict.capability_id)?;
        let named_priority: Vec<String> =
            if !experienced.is_empty() && !named_priority_static.is_empty() {
                let mut ordered: Vec<String> = experienced
                    .iter()
                    .filter_map(|id| {
                        named_priority_static
                            .iter()
                            .find(|&&n| n == id.as_str())
                            .map(|&n| n.to_owned())
                    })
                    .collect();
                for &n in named_priority_static {
                    if !ordered.iter().any(|o| o.as_str() == n) {
                        ordered.push(n.to_owned());
                    }
                }
                ordered
            } else {
                named_priority_static
                    .iter()
                    .map(|&s| s.to_owned())
                    .collect()
            };

        // Two paths: numbered SPARQL (manufacturing-process-cost) vs named SPARQL (judgment-intake)
        let (mut asset_ids, mut resolved_cqs) = (Vec::new(), Vec::new());

        if !named_priority.is_empty() {
            // Named asset path: directly use the asset_id strings
            let all_named = named_sparql_assets(bundle)?;
            for target in &named_priority {
                if all_named.iter().any(|a| a.as_str() == target.as_str()) {
                    asset_ids.push(target.clone());
                    let question = match target.as_str() {
                        "query-pattern-identity" => "同一对象与配置核验",
                        "query-pattern-claim" => "断言支持、证据角色与独立采用依据",
                        "query-pattern-scope" => "主张范围、来源范围与必要条件保留",
                        "query-pattern-realization" => "功能与实现链路、适用审查的连接",
                        "query-pattern-change" => "变更依赖的当前版本审查",
                        "findings-pattern-identity" => "身份发现模式",
                        "findings-pattern-claim" => "主张发现模式",
                        "findings-pattern-scope" => "范围发现模式",
                        "findings-pattern-realization" => "实现发现模式",
                        "findings-pattern-change" => "变更发现模式",
                        other => other,
                    };
                    resolved_cqs.push(ResolvedCq {
                        bundle: bundle_name.to_owned(),
                        cq_id: target.clone(),
                        question: question.to_owned(),
                        sparql_asset_id: Some(target.clone()),
                    });
                }
            }
        } else if !priority_indices.is_empty() {
            // Numbered SPARQL path: q01..q20
            let sparql_map = sparql_asset_map(bundle)?;
            for &idx in priority_indices {
                if let Some(cq) = cq_list.get(idx.saturating_sub(1)) {
                    let cq_id = cq
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let question = cq
                        .get("question")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let sparql_asset = sparql_map.get(&idx).cloned();
                    if let Some(ref aid) = sparql_asset {
                        asset_ids.push(aid.clone());
                    }
                    resolved_cqs.push(ResolvedCq {
                        bundle: bundle_name.to_owned(),
                        cq_id,
                        question,
                        sparql_asset_id: sparql_asset,
                    });
                }
            }
        } else {
            // Fallback: first 3 CQs from numbered map
            let sparql_map = sparql_asset_map(bundle)?;
            for idx in 1..=cq_list.len().min(3) {
                if let Some(cq) = cq_list.get(idx - 1) {
                    let cq_id = cq
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let question = cq
                        .get("question")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let sparql_asset = sparql_map.get(&idx).cloned();
                    if let Some(ref aid) = sparql_asset {
                        asset_ids.push(aid.clone());
                    }
                    resolved_cqs.push(ResolvedCq {
                        bundle: bundle_name.to_owned(),
                        cq_id,
                        question,
                        sparql_asset_id: sparql_asset,
                    });
                }
            }
        }

        // Build purpose string from CQ questions
        let purpose = if resolved_cqs.is_empty() {
            format!("{}能力相关语义前提", verdict.capability_id)
        } else {
            resolved_cqs
                .iter()
                .map(|c| c.question.as_str())
                .collect::<Vec<_>>()
                .join("；")
        };

        // Applicability is caller-asserted; marked as not_established per protocol
        let applicability = json!({
            "capability_id": verdict.capability_id,
            "route_need": verdict.need,
            "applicability_status": "caller_assertion_not_established",
            "note": "路由判断触发前提推荐；适用性由工程师核验，不由路由自动确立"
        });

        if !asset_ids.is_empty() {
            recommendations.push(SelectionRecommendation {
                capability_id: verdict.capability_id.clone(),
                bundle: bundle_name.to_owned(),
                asset_ids,
                purpose,
                applicability,
                cqs: resolved_cqs,
            });
        } else {
            // Bundle matched but no SPARQL assets selected; record as unmatched
            unmatched.push(format!(
                "{} (no SPARQL assets for priority CQs)",
                verdict.capability_id
            ));
        }
    }

    let selections_json = Value::Array(recommendations.iter().map(|r| r.to_selection()).collect());

    Ok(AdviceResult {
        recommendations,
        unmatched_capabilities: unmatched,
        selections_json,
    })
}

// ---------------------------------------------------------------------------
// Experience recording and recall
// ---------------------------------------------------------------------------

const EXPERIENCE_FILE: &str = "var/premise-experience.json";

/// Load the experience index from `var/premise-experience.json`.
fn load_experience(root: &SkillRoot) -> Result<Value> {
    let path = root.private_path(Path::new(EXPERIENCE_FILE))?;
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(e).context("premise_experience_metadata_failed"),
        Ok(_) => (),
    }
    let value = oe_core::json::strict_json(&read_regular_bytes(&path, 4 * 1024 * 1024)?)?;
    ensure!(value.is_object(), "premise_experience_must_be_object");
    Ok(value)
}

fn increment(map: &mut serde_json::Map<String, Value>, key: &str) -> Result<()> {
    let count = map
        .get(key)
        .map(|v| v.as_u64().context("experience_count_must_be_u64"))
        .transpose()?
        .unwrap_or(0);
    let next = count.checked_add(1).context("experience_count_overflow")?;
    map.insert(key.to_owned(), json!(next));
    Ok(())
}
/// Usage accounting only. A persistent lock serialises participating writers;
/// atomic replacement prevents a partially written index from replacing history.
fn write_experience(
    root: &SkillRoot,
    recommendations: &[SelectionRecommendation],
    verdict: Option<ReportedVerdict>,
) -> Result<()> {
    if recommendations.is_empty() {
        return Ok(());
    }
    let path = root.private_path(Path::new(EXPERIENCE_FILE))?;
    let lock_path = root.private_path(Path::new("var/premise-experience.lock"))?;
    let guard = AdvisoryFileLock::acquire(&lock_path, true)?;
    let mut experience = load_experience(root)?;
    let exp_obj = experience
        .as_object_mut()
        .context("premise-experience.json must be an object")?;

    for rec in recommendations {
        let cap_entry = exp_obj
            .entry(rec.capability_id.as_str())
            .or_insert_with(|| json!({"total_uses": 0, "verdicts": {}, "assets": {}}));
        let cap_obj = cap_entry
            .as_object_mut()
            .context("experience capability entry must be an object")?;

        increment(cap_obj, "total_uses")?;
        cap_obj.insert(
            "signal_kind".into(),
            json!("reported_selection_usage_not_validated_outcome"),
        );
        // Preserve historical verdict buckets, but never inherit them as proof.
        cap_obj.insert(
            "legacy_verdict_provenance".into(),
            json!("unverified_caller_reports"),
        );

        // Reports without exact native receipt bindings are not verified verdicts.
        if let Some(v) = verdict {
            let verdicts = cap_obj
                .entry("reported_verdicts")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .context("reported_verdicts must be an object")?;
            increment(verdicts, v.as_str())?;
        }

        // per-asset_id use counts
        let asset_counts = cap_obj
            .entry("assets")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .context("experience assets must be an object")?;
        for asset_id in &rec.asset_ids {
            increment(asset_counts, asset_id)?;
        }
    }

    let bytes = serde_json::to_vec_pretty(&experience)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "premise_experience_size_limit"
    );
    let staging = root.private_path(&std::path::PathBuf::from(format!(
        "var/.premise-experience-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    )))?;
    oe_core::write_new_bytes(&staging, &bytes)?;
    guard.ensure_current()?;
    oe_core::rename_regular_file(&staging, &path)?;
    Ok(())
}

/// Convenience wrapper for legacy callers that have a step output directory
/// but not an `AdviceResult` in scope.  Reads `premises.json` from the
/// directory and delegates to `write_experience`.
pub fn record_experience(root: &SkillRoot, step_output_dir: &Path) -> Result<()> {
    let premises_path = step_output_dir.join("premises.json");
    let bytes = read_regular_bytes(&premises_path, 512 * 1024)
        .context("cannot read premises.json for experience recording")?;
    let doc: Value = oe_core::json::strict_json(&bytes)?;

    // Reconstruct minimal SelectionRecommendation list from the serialised selections
    let selections = doc
        .get("selections")
        .and_then(Value::as_array)
        .context("premises_selections_must_be_array")?;

    let mut recs = Vec::new();
    for sel in selections {
        let cap_id = sel
            .get("applicability")
            .and_then(|a| a.get("capability_id"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .context("reported_usage_requires_capability_id")?
            .to_owned();
        let bundle = sel
            .get("bundle")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .context("reported_usage_requires_bundle")?
            .to_owned();
        let asset_ids: Vec<String> = sel
            .get("asset_ids")
            .and_then(Value::as_array)
            .context("reported_usage_asset_ids_must_be_array")?
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .context("reported_usage_asset_id_must_be_text")
            })
            .collect::<Result<_>>()?;
        recs.push(SelectionRecommendation {
            capability_id: cap_id,
            bundle,
            asset_ids,
            purpose: String::new(),
            applicability: Value::Null,
            cqs: vec![],
        });
    }
    write_experience(root, &recs, None)
}

/// Return asset_ids sorted by historical frequency for a given capability.
pub fn experienced_priority(root: &SkillRoot, capability_id: &str) -> Result<Vec<String>> {
    let experience = load_experience(root)?;
    let cap = match experience.get(capability_id) {
        Some(value) => value
            .as_object()
            .context("experience_capability_must_be_object")?,
        None => return Ok(vec![]),
    };
    let asset_counts = match cap.get("assets") {
        Some(value) => value
            .as_object()
            .context("experience_assets_must_be_object")?,
        None => return Ok(vec![]),
    };
    let mut pairs: Vec<(String, u64)> = asset_counts
        .iter()
        .map(|(k, v)| {
            Ok((
                k.clone(),
                v.as_u64().context("experience_count_must_be_u64")?,
            ))
        })
        .collect::<Result<_>>()?;
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(pairs.into_iter().map(|(k, _)| k).collect())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- pure mapping functions ---

    #[test]
    fn capability_to_bundle_known_caps() {
        assert_eq!(
            capability_to_bundle("manufacturing"),
            Some("manufacturing-process-cost")
        );
        assert_eq!(
            capability_to_bundle("evidence"),
            Some("engineering-judgment-intake")
        );
        assert_eq!(
            capability_to_bundle("project_modeling"),
            Some("engineering-judgment-intake")
        );
        assert_eq!(
            capability_to_bundle("semantic_review"),
            Some("engineering-judgment-intake")
        );
        assert_eq!(
            capability_to_bundle("semantic_integration"),
            Some("engineering-judgment-intake")
        );
        assert_eq!(
            capability_to_bundle("ontology_evolution"),
            Some("engineering-judgment-intake")
        );
    }

    #[test]
    fn capability_to_bundle_unknown_returns_none() {
        assert!(capability_to_bundle("cad").is_none());
        assert!(capability_to_bundle("external_specialist").is_none());
        assert!(capability_to_bundle("").is_none());
    }

    #[test]
    fn priority_cqs_for_manufacturing_has_five_entries() {
        let indices = priority_cqs_for_capability("manufacturing");
        assert_eq!(indices, &[3usize, 5, 6, 7, 10]);
    }

    #[test]
    fn priority_cqs_for_unknown_is_empty() {
        assert!(priority_cqs_for_capability("cad").is_empty());
        assert!(priority_cqs_for_capability("").is_empty());
    }

    #[test]
    fn named_assets_evidence_has_three_entries() {
        let assets = named_assets_for_capability("evidence");
        assert_eq!(assets.len(), 3);
        assert!(assets.contains(&"query-pattern-identity"));
        assert!(assets.contains(&"query-pattern-claim"));
        assert!(assets.contains(&"query-pattern-scope"));
    }

    #[test]
    fn named_assets_manufacturing_is_empty() {
        // manufacturing uses numbered CQs (q01..q20), not named assets
        assert!(named_assets_for_capability("manufacturing").is_empty());
    }

    // --- read_routing_result ---

    #[test]
    fn read_routing_result_parses_needed_routes() {
        let routing_json = json!({
            "routes": [
                {"capability_id": "manufacturing", "need": "needed", "answer": {"confidence": 0.95}},
                {"capability_id": "cad", "need": "not_needed", "answer": {"confidence": 0.1}},
                {"capability_id": "evidence", "need": "needed", "answer": {"confidence": 1.0}},
            ]
        });
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().canonicalize().unwrap();
        let routing_path = real_dir.join("routing.json");
        std::fs::write(&routing_path, serde_json::to_string(&routing_json).unwrap()).unwrap();
        let verdicts = read_routing_result(&routing_path).unwrap();
        assert_eq!(verdicts.len(), 3);
        let mfg = verdicts
            .iter()
            .find(|v| v.capability_id == "manufacturing")
            .unwrap();
        assert_eq!(mfg.need, "needed");
        assert_eq!(mfg.confidence, Some(0.95));
        let cad = verdicts.iter().find(|v| v.capability_id == "cad").unwrap();
        assert_eq!(cad.need, "not_needed");
    }

    #[test]
    fn read_routing_result_missing_routes_errors() {
        let json = json!({"status": "completed"});
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().canonicalize().unwrap();
        let path = real_dir.join("routing.json");
        std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();
        assert!(read_routing_result(&path).is_err());
    }

    #[test]
    fn malformed_route_fields_are_not_silently_discarded_or_defaulted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("routing.json");
        for route in [
            json!({"need":"needed"}),
            json!({"capability_id":"evidence","need":"invalid"}),
            json!({"capability_id":"evidence","need":"needed","answer":{"confidence":"invalid"}}),
            json!({"capability_id":"evidence","need":"needed","answer":{"confidence":1.1}}),
        ] {
            std::fs::write(
                &path,
                serde_json::to_vec(&json!({"routes":[route]})).unwrap(),
            )
            .unwrap();
            assert!(read_routing_result(&path).is_err());
        }
    }
    #[test]
    fn malformed_selected_history_is_not_a_successful_empty_history() {
        for capability in [
            json!(null),
            json!([]),
            json!({"assets":null}),
            json!({"assets":[]}),
        ] {
            let (_dir, root) = make_skill_root();
            let path = root.path().join(EXPERIENCE_FILE);
            let original = serde_json::to_vec(&json!({"evidence":capability})).unwrap();
            std::fs::write(&path, &original).unwrap();
            assert!(experienced_priority(&root, "evidence").is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }
    #[test]
    fn unavailable_model_metadata_keeps_each_route_unknown_or_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("routing.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({"routes":[
                {"capability_id":"evidence","need":"needed","answer":{"confidence":null}},
                {"capability_id":"semantic_review","answer":null}
            ]}))
            .unwrap(),
        )
        .unwrap();
        let routes = read_routing_result(&path).unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].confidence, None);
        assert_eq!(routes[0].need, "needed");
        assert_eq!(routes[1].confidence, None);
        assert_eq!(routes[1].need, "not_established");
    }
    #[test]
    fn duplicate_or_unbounded_route_lists_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("routing.json");
        let route = json!({"capability_id":"evidence","need":"needed"});
        for routes in [vec![route.clone(), route.clone()], vec![route; 257]] {
            std::fs::write(
                &path,
                serde_json::to_vec(&json!({"routes":routes})).unwrap(),
            )
            .unwrap();
            assert!(read_routing_result(&path).is_err());
        }
    }

    // --- review_templates ---

    fn make_advice_result(capability_id: &str, asset_ids: Vec<&str>) -> AdviceResult {
        let recs = vec![SelectionRecommendation {
            capability_id: capability_id.to_owned(),
            bundle: "engineering-judgment-intake".to_owned(),
            asset_ids: asset_ids.iter().map(|s| s.to_string()).collect(),
            purpose: "test".to_owned(),
            applicability: json!({}),
            cqs: vec![],
        }];
        let selections_json = Value::Array(recs.iter().map(|r| r.to_selection()).collect());
        AdviceResult {
            recommendations: recs,
            unmatched_capabilities: vec![],
            selections_json,
        }
    }

    #[test]
    fn review_templates_produces_template_per_named_pattern_asset() {
        let advice = make_advice_result(
            "evidence",
            vec![
                "query-pattern-identity",
                "query-pattern-claim",
                "query-pattern-scope",
            ],
        );
        let templates = advice.review_templates();
        assert_eq!(templates.len(), 3);
        let t = &templates[0];
        assert_eq!(t["capability_id"], "evidence");
        assert_eq!(t["query_asset_id"], "query-pattern-identity");
        assert_eq!(t["shape_asset_id"], "shape-pattern-identity");
        assert!(t["evidence_file"].is_null());
        assert!(t["focus_iri"].is_null());
    }

    #[test]
    fn review_templates_skips_numbered_assets() {
        // Manufacturing uses q03, q05 etc — should produce zero templates
        let advice = make_advice_result("manufacturing", vec!["q03", "q05", "q06"]);
        assert!(advice.review_templates().is_empty());
    }

    // --- experience recording and recall ---

    fn make_skill_root() -> (tempfile::TempDir, SkillRoot) {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        std::fs::write(real.join("SKILL.md"), b"").unwrap();
        std::fs::create_dir_all(real.join("var")).unwrap();
        let root = SkillRoot::new(&real).unwrap();
        (dir, root)
    }

    // Copy frozen transport bytes into an isolated fixture. These tests do not
    // call a model, execute formal semantics or modify the installed source bundles.
    fn make_advisor_root() -> (tempfile::TempDir, SkillRoot, std::path::PathBuf) {
        let (dir, root) = make_skill_root();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(root.path().join("runtime/vendor")).unwrap();
        for name in [
            "runtime/semantic-bundles.json",
            "runtime/semantica-source-lock.json",
        ] {
            std::fs::copy(source.join(name), root.path().join(name)).unwrap();
        }
        let lock: Value = serde_json::from_slice(
            &std::fs::read(source.join("runtime/semantic-bundles.json")).unwrap(),
        )
        .unwrap();
        for name in ["engineering-judgment-intake", "manufacturing-process-cost"] {
            let path = lock["bundles"][name]["path"].as_str().unwrap();
            std::fs::copy(source.join(path), root.path().join(path)).unwrap();
        }
        let routing = root.path().join("routing.json");
        std::fs::write(&routing, serde_json::to_vec(&json!({
            "schema": "ontology-engineering.context-routing/v1",
            "status": "completed",
            "routes": [
                {"capability_id": "evidence", "need": "needed", "answer": {"confidence": 0.95}},
                {"capability_id": "manufacturing", "need": "needed", "answer": {"confidence": 0.95}}
            ]
        })).unwrap()).unwrap();
        (dir, root, routing)
    }

    fn intake_archive(root: &SkillRoot) -> std::path::PathBuf {
        let lock: Value = serde_json::from_slice(
            &std::fs::read(root.path().join("runtime/semantic-bundles.json")).unwrap(),
        )
        .unwrap();
        root.path().join(
            lock["bundles"]["engineering-judgment-intake"]["path"]
                .as_str()
                .unwrap(),
        )
    }

    #[test]
    fn advice_from_valid_frozen_bundles_builds_the_shared_premise_snapshot() {
        let (_dir, root, routing) = make_advisor_root();
        let advice = advise(&root, &routing).unwrap();
        assert_eq!(advice.recommendations.len(), 2);
        assert_eq!(advice.recommendations[0].asset_ids.len(), 3);
        assert_eq!(advice.recommendations[1].asset_ids.len(), 5);
        let snapshot = oe_semantic::build_premises(root.path(), &advice.selections_json).unwrap();
        assert_eq!(snapshot["asset_count"], 8);
        assert_eq!(snapshot["semantic_execution"], "not_run");
        assert_eq!(snapshot["fact_admitted"], false);
    }

    #[test]
    fn advice_rejects_modified_archive_bytes_even_when_the_zip_is_readable() {
        let (_dir, root, routing) = make_advisor_root();
        let path = intake_archive(&root);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.extend_from_slice(b"UNDECLARED_TRAILING_BYTES");
        std::fs::write(path, bytes).unwrap();
        let error = advise(&root, &routing)
            .err()
            .expect("modified archive must be rejected");
        assert!(format!("{error:#}").contains("bundle hash mismatch"));
    }

    #[test]
    fn advice_rejects_a_bundle_bound_to_a_different_runtime() {
        let (_dir, root, routing) = make_advisor_root();
        let path = root.path().join("runtime/semantica-source-lock.json");
        let mut lock: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        lock["source"]["version"] = json!("0.0.0-fixture-mismatch");
        std::fs::write(path, serde_json::to_vec(&lock).unwrap()).unwrap();
        let error = advise(&root, &routing)
            .err()
            .expect("runtime mismatch must be rejected");
        assert!(format!("{error:#}").contains("different source-locked runtime"));
    }

    #[test]
    fn advice_rejects_a_missing_declared_archive_instead_of_returning_empty_advice() {
        let (_dir, root, routing) = make_advisor_root();
        std::fs::remove_file(intake_archive(&root)).unwrap();
        let error = advise(&root, &routing)
            .err()
            .expect("missing declared archive must be rejected");
        assert!(format!("{error:#}").contains("cannot verify premise bundle"));
    }

    #[cfg(unix)]
    #[test]
    fn advice_rejects_symlinked_archives_even_with_matching_bytes() {
        let (_dir, root, routing) = make_advisor_root();
        let path = intake_archive(&root);
        let alternate = root.path().join("runtime/vendor/alternate-fixture.zip");
        std::fs::rename(&path, &alternate).unwrap();
        std::os::unix::fs::symlink(alternate, path).unwrap();
        let error = advise(&root, &routing)
            .err()
            .expect("symlinked archive must be rejected");
        assert!(format!("{error:#}").contains("symbolic link"));
    }

    #[test]
    fn write_and_load_experience_increments_counts() {
        let (_dir, root) = make_skill_root();
        let advice = make_advice_result("evidence", vec!["query-pattern-claim"]);
        advice.record_loaded(&root).unwrap();
        let exp = load_experience(&root).unwrap();
        assert_eq!(exp["evidence"]["total_uses"], 1);
        assert_eq!(exp["evidence"]["assets"]["query-pattern-claim"], 1);
        assert!(exp["evidence"]["verdicts"].as_object().unwrap().is_empty());

        // Second load increments again
        advice.record_loaded(&root).unwrap();
        let exp2 = load_experience(&root).unwrap();
        assert_eq!(exp2["evidence"]["total_uses"], 2);
        assert_eq!(exp2["evidence"]["assets"]["query-pattern-claim"], 2);
    }

    #[test]
    fn caller_verdict_is_kept_as_an_unverified_report() {
        let (_dir, root) = make_skill_root();
        let advice = make_advice_result("evidence", vec!["query-pattern-claim"]);
        advice.record_with_verdict(&root, "passed").unwrap();
        let exp = load_experience(&root).unwrap();
        assert_eq!(exp["evidence"]["reported_verdicts"]["passed"], 1);
        assert_eq!(
            exp["evidence"]["signal_kind"],
            "reported_selection_usage_not_validated_outcome"
        );
        assert!(exp["evidence"]["verdicts"].as_object().unwrap().is_empty());
        assert_eq!(exp["evidence"]["total_uses"], 1);

        advice.record_with_verdict(&root, "blocked").unwrap();
        let exp2 = load_experience(&root).unwrap();
        assert_eq!(exp2["evidence"]["reported_verdicts"]["passed"], 1);
        assert_eq!(exp2["evidence"]["reported_verdicts"]["blocked"], 1);
        assert_eq!(exp2["evidence"]["total_uses"], 2);
    }

    #[test]
    fn experienced_priority_sorts_by_frequency_desc() {
        let (_dir, root) = make_skill_root();
        // Load claim 3 times, identity 1 time → claim should rank first
        let advice_claim = make_advice_result("evidence", vec!["query-pattern-claim"]);
        let advice_identity = make_advice_result("evidence", vec!["query-pattern-identity"]);
        advice_claim.record_loaded(&root).unwrap();
        advice_claim.record_loaded(&root).unwrap();
        advice_claim.record_loaded(&root).unwrap();
        advice_identity.record_loaded(&root).unwrap();
        let priority = experienced_priority(&root, "evidence").unwrap();
        assert_eq!(priority[0], "query-pattern-claim");
        assert_eq!(priority[1], "query-pattern-identity");
    }

    #[test]
    fn experienced_priority_returns_empty_for_unknown_capability() {
        let (_dir, root) = make_skill_root();
        assert!(
            experienced_priority(&root, "nonexistent")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn invalid_verdict_never_creates_experience() {
        let (_dir, root) = make_skill_root();
        let advice = make_advice_result("evidence", vec!["query-pattern-claim"]);
        assert!(advice.record_with_verdict(&root, "proven").is_err());
        assert!(!root.path().join(EXPERIENCE_FILE).exists());
    }
    #[test]
    fn damaged_history_is_not_silently_reset_and_overwritten() {
        let (_dir, root) = make_skill_root();
        let path = root.path().join(EXPERIENCE_FILE);
        std::fs::write(&path, b"{broken history").unwrap();
        let advice = make_advice_result("evidence", vec!["query-pattern-claim"]);
        assert!(advice.record_loaded(&root).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken history");
        assert!(experienced_priority(&root, "evidence").is_err());
    }
    #[test]
    fn overflowing_or_malformed_counts_preserve_the_original_index() {
        for count in [json!(u64::MAX), json!("invalid")] {
            let (_dir, root) = make_skill_root();
            let path = root.path().join(EXPERIENCE_FILE);
            let original =
                serde_json::to_vec(&json!({"evidence":{"total_uses":count,"assets":{}}})).unwrap();
            std::fs::write(&path, &original).unwrap();
            let advice = make_advice_result("evidence", vec!["query-pattern-claim"]);
            assert!(advice.record_loaded(&root).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }
    #[test]
    fn competing_writer_is_reported_without_losing_history() {
        let (_dir, root) = make_skill_root();
        let advice = make_advice_result("evidence", vec!["query-pattern-claim"]);
        advice.record_loaded(&root).unwrap();
        let original = std::fs::read(root.path().join(EXPERIENCE_FILE)).unwrap();
        let _guard =
            AdvisoryFileLock::acquire(&root.path().join("var/premise-experience.lock"), true)
                .unwrap();
        assert!(advice.record_loaded(&root).is_err());
        assert_eq!(
            std::fs::read(root.path().join(EXPERIENCE_FILE)).unwrap(),
            original
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_history_cannot_redirect_accounting_writes() {
        let (_dir, root) = make_skill_root();
        let elsewhere = root.path().join("var/other.json");
        std::fs::write(&elsewhere, b"{}").unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.path().join(EXPERIENCE_FILE)).unwrap();
        let advice = make_advice_result("evidence", vec!["query-pattern-claim"]);
        assert!(advice.record_loaded(&root).is_err());
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"{}");
    }

    #[test]
    fn review_templates_shape_mapping_is_consistent() {
        let patterns = [
            ("query-pattern-identity", "shape-pattern-identity"),
            ("query-pattern-claim", "shape-pattern-claim"),
            ("query-pattern-scope", "shape-pattern-scope"),
            ("query-pattern-realization", "shape-pattern-realization"),
            ("query-pattern-change", "shape-pattern-change"),
        ];
        let asset_ids: Vec<&str> = patterns.iter().map(|(q, _)| *q).collect();
        let advice = make_advice_result("evidence", asset_ids);
        let templates = advice.review_templates();
        assert_eq!(templates.len(), patterns.len());
        for (t, (_, expected_shape)) in templates.iter().zip(patterns.iter()) {
            assert_eq!(t["shape_asset_id"].as_str().unwrap(), *expected_shape);
        }
    }
}
