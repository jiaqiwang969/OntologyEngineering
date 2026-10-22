//! Operational evidence handoff. Integrity never establishes applicability.
use crate::context::{self, digest, object};
use crate::poppler::PopplerTools;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

pub const SCHEMA: &str = "ontology-engineering.supplier-evidence/v1";
use crate::citations::MAX_SOURCE_BYTES as SOURCE_LIMIT;
fn array<'a>(v: &'a Value, code: &str) -> Result<&'a Vec<Value>> {
    v.as_array().with_context(|| code.to_owned())
}
fn string<'a>(v: &'a Value, code: &str) -> Result<&'a str> {
    v.as_str().with_context(|| code.to_owned())
}
pub(crate) fn source_path(root: &SkillRoot, value: &Value) -> Result<PathBuf> {
    let s = string(value, "supplier_handoff_source_path_invalid")?;
    let p = Path::new(s);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.path().join(p)
    };
    let relative = full
        .strip_prefix(root.path())
        .context("supplier_handoff_source_outside_root")?;
    ensure!(
        relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
            && (relative.starts_with("sources") || relative.starts_with("var/cache/knowledge")),
        "supplier_handoff_source_outside_root"
    );
    no_symlinks(&full)?;
    Ok(full)
}
pub(crate) fn span_text(body: &str, start: &Value, end: &Value) -> Result<String> {
    let s = start
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .context("supplier_handoff_span_mismatch")?;
    let e = end
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .context("supplier_handoff_span_mismatch")?;
    ensure!(
        s < e && e <= body.chars().count(),
        "supplier_handoff_span_mismatch"
    );
    Ok(body.chars().skip(s).take(e - s).collect())
}
fn relative_parts(value: &Value) -> Result<Vec<&str>> {
    let s = string(value, "supplier_handoff_relative_source_mismatch")?;
    let parts = s.split('/').collect::<Vec<_>>();
    ensure!(
        !s.is_empty()
            && !s.contains(['\\', '\0'])
            && parts
                .iter()
                .all(|p| !p.is_empty() && *p != "." && *p != ".."),
        "supplier_handoff_relative_source_mismatch"
    );
    Ok(parts)
}
fn file_uri(path: &Path) -> Result<String> {
    let mut uri = String::from("file://");
    for b in path
        .to_str()
        .context("supplier_handoff_source_path_invalid")?
        .bytes()
    {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(char::from(b));
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(uri)
}
fn check_source_pack(root: &SkillRoot, result: &Value) -> Result<()> {
    let manifest_name = string(
        &result["source_pack_manifest"],
        "supplier_handoff_source_pack_identity_mismatch",
    )?;
    relative_parts(&result["source_pack_manifest"])?;
    let path = source_path(root, &result["source_pack_manifest"])?;
    ensure!(
        !Path::new(manifest_name).is_absolute(),
        "supplier_handoff_source_pack_identity_mismatch"
    );
    let raw = read_regular_bytes(&path, crate::source_pack::MAX_MANIFEST_BYTES)?;
    ensure!(
        sha256(&raw) == result["source_pack_manifest_sha256"],
        "supplier_handoff_source_pack_identity_mismatch"
    );
    let document = crate::source_pack::load_manifest(root.path(), &path)?;
    ensure!(
        document == oe_core::json::strict_json(&raw)?,
        "supplier_handoff_source_pack_identity_mismatch"
    );
    let member = string(
        &result["source_relative_path"],
        "supplier_handoff_source_pack_member_mismatch",
    )?;
    let entries = document["segments"]
        .as_array()
        .context("supplier_handoff_source_pack_member_mismatch")?
        .iter()
        .flat_map(|s| s["files"].as_array().into_iter().flatten())
        .filter(|f| f["path"] == member)
        .collect::<Vec<_>>();
    ensure!(
        entries.len() == 1 && entries[0]["sha256"] == result["source_sha256"],
        "supplier_handoff_source_pack_member_mismatch"
    );
    let source = source_path(root, &result["source_pdf"])?;
    let original = fingerprint_file(&source, crate::source_pack::MAX_SOURCE_BYTES)?;
    ensure!(
        entries[0]["bytes"].as_u64() == Some(original.bytes)
            && entries[0]["sha256"] == original.sha256,
        "supplier_handoff_source_pack_member_mismatch"
    );
    let pack_root = path
        .parent()
        .context("supplier_handoff_source_pack_identity_mismatch")?;
    let cache = root
        .path()
        .join("var/cache/knowledge/source-pages")
        .join(sha256(&raw))
        .join(member);
    let legacy = pack_root.join("cache/archive").join(member);
    ensure!(
        source == cache || source == legacy,
        "supplier_handoff_source_pack_member_mismatch"
    );
    if let Some(relative) = result.get("assembled_relative_path") {
        let matches = document["assembled_pdfs"]
            .as_array()
            .context("supplier_handoff_source_pack_assembly_mismatch")?
            .iter()
            .filter(|f| f["catalog"] == result["catalog"] && f["path"] == *relative)
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1
                && matches[0]["sha256"] == result["assembled_pdf_sha256_at_index_build"],
            "supplier_handoff_source_pack_assembly_mismatch"
        );
        let assembled = source_path(root, &result["assembled_pdf"])?;
        ensure!(
            assembled
                == pack_root.join("cache/archive").join(string(
                    relative,
                    "supplier_handoff_source_pack_assembly_mismatch"
                )?),
            "supplier_handoff_source_pack_assembly_mismatch"
        );
    }
    ensure!(
        read_regular_bytes(&path, crate::source_pack::MAX_MANIFEST_BYTES)? == raw,
        "supplier_handoff_source_pack_identity_mismatch"
    );
    Ok(())
}
fn relocation(root: &SkillRoot, result: &Value) -> Result<Map<String, Value>> {
    let mut extra = Map::new();
    let mut roots = BTreeSet::new();
    for (rel_key, abs_key) in [
        ("source_relative_path", "source_pdf"),
        ("assembled_relative_path", "assembled_pdf"),
    ] {
        if let Some(relative) = result.get(rel_key) {
            let parts = relative_parts(relative)?;
            let full = source_path(root, &result[abs_key])?;
            let mut base = full.clone();
            for part in parts.iter().rev() {
                ensure!(
                    base.file_name().and_then(|s| s.to_str()) == Some(*part),
                    "supplier_handoff_relative_source_mismatch"
                );
                base.pop();
            }
            roots.insert(base);
            extra.insert(rel_key.to_owned(), relative.clone());
            if abs_key == "assembled_pdf" {
                extra.insert(abs_key.to_owned(), result[abs_key].clone());
            }
        }
    }
    if result.get("source_pack_manifest").is_some()
        || result.get("source_pack_manifest_sha256").is_some()
    {
        check_source_pack(root, result)?;
        for key in ["source_pack_manifest", "source_pack_manifest_sha256"] {
            extra.insert(key.to_owned(), result[key].clone());
        }
    } else {
        ensure!(
            roots.len() <= 1,
            "supplier_handoff_relative_source_mismatch"
        );
    }
    if let Some(status) = result.get("assembled_pdf_status") {
        let s = string(status, "supplier_handoff_assembled_status_mismatch")?;
        let assembled = source_path(root, &result["assembled_pdf"])?;
        let exists = assembled.is_file();
        ensure!(
            [
                "verified_present",
                "present_not_reverified",
                "not_downloaded",
                "not_restored",
                "hash_mismatch"
            ]
            .contains(&s)
                && result["assembled_pdf_exists"] == exists
                && (["not_downloaded", "not_restored"].contains(&s)) != exists,
            "supplier_handoff_assembled_status_mismatch"
        );
        if s == "verified_present" {
            ensure!(
                fingerprint_file(&assembled, SOURCE_LIMIT)?.sha256
                    == result["assembled_pdf_sha256_at_index_build"],
                "supplier_handoff_source_changed"
            );
        }
        let uri = if ["verified_present", "present_not_reverified"].contains(&s) {
            json!(file_uri(&assembled)?)
        } else {
            Value::Null
        };
        let page_uri = if let Some(u) = uri.as_str() {
            format!("{u}#page={}", result["pdf_page"])
        } else {
            format!(
                "{}#page=1",
                file_uri(&source_path(root, &result["source_pdf"])?)?
            )
        };
        let portable = Path::new(string(
            &result["source_pdf"],
            "supplier_handoff_source_path_invalid",
        )?)
        .is_relative();
        if !portable || result.get("assembled_pdf_uri").is_some() || result.get("pdf_uri").is_some()
        {
            ensure!(
                result["assembled_pdf_uri"] == uri && result["pdf_uri"] == page_uri,
                "supplier_handoff_assembled_uri_mismatch"
            );
        }
        for key in [
            "assembled_pdf_exists",
            "assembled_pdf_status",
            "assembled_pdf_uri",
            "assembled_pdf_download_url",
            "assembled_pdf_sha256_at_index_build",
            "pdf_uri",
        ] {
            if !portable || !["assembled_pdf_uri", "pdf_uri"].contains(&key) {
                extra.insert(key.to_owned(), result[key].clone());
            }
        }
    }
    Ok(extra)
}

/// Recheck only the originals and manifest represented by an already returned
/// result. This is the same relocation contract used by packet construction.
pub(crate) fn verify_returned_source(
    root: &SkillRoot,
    result: &Value,
    tools: &PopplerTools,
) -> Result<()> {
    crate::citations::verify_locator_with_tools(root.path(), result, tools)?;
    relocation(root, result)?;
    Ok(())
}

fn checked_evidence(
    root: &SkillRoot,
    results: &Value,
    context: &Value,
    tools: &PopplerTools,
) -> Result<Value> {
    let mut evidence = Vec::new();
    let mut hashes = HashMap::new();
    let mut ids = BTreeSet::new();
    let context_sha = digest(context)?;
    for result in array(results, "supplier_handoff_packet_evidence_mismatch")? {
        object(result, "supplier_handoff_packet_evidence_mismatch")?;
        let id = string(&result["id"], "supplier_handoff_packet_evidence_mismatch")?;
        ensure!(
            !id.is_empty() && ids.insert(id),
            "supplier_handoff_duplicate_candidate"
        );
        let source = source_path(root, &result["source_pdf"])?;
        if !hashes.contains_key(&source) {
            hashes.insert(
                source.clone(),
                fingerprint_file(&source, SOURCE_LIMIT)
                    .context("supplier_handoff_source_changed")?
                    .sha256,
            );
        }
        ensure!(
            hashes[&source] == result["source_sha256"],
            "supplier_handoff_source_changed"
        );
        let body = string(&result["native_text"], "supplier_handoff_text_mismatch")?;
        ensure!(
            sha256(body) == result["text_sha256"],
            "supplier_handoff_text_mismatch"
        );
        let empty = json!([]);
        let units = result.get("knowledge_units").unwrap_or(&empty);
        let mut unit_ids = BTreeSet::new();
        for unit in array(units, "supplier_handoff_span_mismatch")? {
            let uid = string(&unit["id"], "supplier_handoff_span_mismatch")?;
            ensure!(
                !uid.is_empty() && unit_ids.insert(uid),
                "supplier_handoff_span_mismatch"
            );
            let span = &unit["source_span"];
            let selected = span_text(body, &span["start"], &span["end"])?;
            ensure!(
                selected == span["text"]
                    && sha256(&selected) == span["sha256"]
                    && unit["page_id"] == result["id"]
                    && unit["context_sha256"] == context_sha
                    && unit["source_text_sha256"] == result["text_sha256"],
                "supplier_handoff_span_mismatch"
            );
            ensure!(
                unit["status"] == "source_passage_candidate"
                    && unit["applicability"] == "not_assessed",
                "supplier_handoff_unreviewed_assertion"
            );
        }
        let mut fields = Map::new();
        for key in [
            "catalog",
            "catalog_title",
            "printed_page",
            "pdf_page",
            "source_pdf",
            "source_url",
            "source_sha256",
            "text_sha256",
        ] {
            fields.insert(
                key.to_owned(),
                result
                    .get(key)
                    .context("supplier_handoff_packet_evidence_mismatch")?
                    .clone(),
            );
        }
        fields.extend(relocation(root, result)?);
        let catalog = result["catalog"].as_str().unwrap_or("");
        // Known native providers require physical-page checks even if a caller
        // strips the optional field. Older generic byte evidence stays byte-only.
        if result.get("source_locator").is_some()
            || catalog.starts_with("local_pdf:")
            || crate::supplier_index::CATALOG_CODES.contains(&catalog)
        {
            fields.insert(
                "source_locator".into(),
                crate::citations::verify_locator_with_tools(root.path(), result, tools)?,
            );
        }
        if let Some(origin) = result.get("text_origin") {
            fields.insert("text_origin".into(), origin.clone());
        }
        evidence.push(json!({"id":id,"source":fields,"knowledge_units":units,"page_native_text":body,"candidate_judgment":result.get("judgment").context("supplier_handoff_packet_evidence_mismatch")?,"applicability":"not_assessed","adoption":"pending_agent_review","engineering_verdict":"not_run"}));
    }
    Ok(json!(evidence))
}

pub fn validate_pattern_handoff(value: &Value, bundle: &Value) -> Result<()> {
    let Some(contract) = bundle.get("pattern_contract") else {
        return Ok(());
    };
    object(contract, "supplier_handoff_pattern_identity_mismatch")?;
    ensure!(
        value["pattern_identity"] == contract["identity"]
            && value["semantic_review"] == "not_run"
            && value["ontology_promotion"] == "not_run",
        "supplier_handoff_pattern_identity_mismatch"
    );
    let empty_answers = json!({});
    let empty_refs = json!([]);
    let answers = object(
        value.get("pattern_answers").unwrap_or(&empty_answers),
        "supplier_handoff_pattern_choices_mismatch",
    )?;
    let refs = value.get("pattern_refs").unwrap_or(&empty_refs);
    array(refs, "supplier_handoff_pattern_choices_mismatch")?;
    if value["pattern_relevance"] == "not_run" {
        ensure!(
            answers.is_empty() && refs.as_array().is_some_and(Vec::is_empty),
            "supplier_handoff_pattern_choices_mismatch"
        );
        return Ok(());
    }
    let qids = array(
        &contract["question_ids"],
        "supplier_handoff_pattern_choices_mismatch",
    )?;
    let names = qids
        .iter()
        .map(|q| string(q, "supplier_handoff_pattern_choices_mismatch"))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        answers.keys().all(|k| names.contains(k.as_str())),
        "supplier_handoff_pattern_choices_mismatch"
    );
    let missing = qids
        .iter()
        .filter(|q| !answers.contains_key(q.as_str().unwrap_or_default()))
        .cloned()
        .collect::<Vec<_>>();
    ensure!(
        value["pattern_relevance"]
            == if missing.is_empty() {
                "candidate"
            } else {
                "partial"
            }
            && value
                .get("missing_pattern_question_ids")
                .unwrap_or(&empty_refs)
                == &json!(missing),
        "supplier_handoff_pattern_choices_mismatch"
    );
    let mut expected = Vec::new();
    for p in array(
        &contract["patterns"],
        "supplier_handoff_pattern_choices_mismatch",
    )? {
        let q = format!(
            "pattern_{}",
            string(&p["id"], "supplier_handoff_pattern_choices_mismatch")?
        );
        if let Some(a) = answers.get(&q) {
            let choice = string(&a["choice"], "supplier_handoff_pattern_choices_mismatch")?;
            ensure!(
                ["related", "unrelated", "not_established"].contains(&choice),
                "supplier_handoff_pattern_choices_mismatch"
            );
            let mut r = p.clone();
            object(&r, "supplier_handoff_pattern_choices_mismatch")?;
            r["choice"] = json!(choice);
            r["status"] = json!("candidate");
            r["adoption"] = json!("pending_agent_review");
            r["execution"] = json!("not_run");
            expected.push(r);
        }
    }
    ensure!(
        *refs == json!(expected),
        "supplier_handoff_pattern_choices_mismatch"
    );
    Ok(())
}

fn review_handoff(report: &Value, context: &Value) -> Result<Value> {
    let Some(plan) = report.get("review_plan").filter(|v| !v.is_null()) else {
        return Ok(Value::Null);
    };
    let bundle = &report["method_bundle"];
    let mut identity = object(bundle, "supplier_handoff_review_identity_mismatch")?.clone();
    for k in ["sha256", "source_availability", "meaning"] {
        identity.remove(k);
    }
    let sha = digest(&Value::Object(identity))?;
    ensure!(
        bundle["schema"] == "ontology-engineering.supplier-method-bundle/v1"
            && bundle["sha256"] == sha
            && report["method_bundle_sha256"] == sha
            && report["knowledge_policy_sha256"] == sha
            && plan["method_bundle_sha256"] == sha
            && plan["policy_sha256"] == sha
            && plan["context_sha256"] == digest(context)?,
        "supplier_handoff_review_identity_mismatch"
    );
    ensure!(
        plan["schema"] == "ontology-engineering.supplier-review-plan/v1"
            && ["completed", "partial", "failed", "not_run"]
                .contains(&plan["status"].as_str().unwrap_or_default())
            && plan["adoption"] == "pending_agent_review"
            && plan["execution"] == "not_run",
        "supplier_handoff_review_unreviewed_assertion"
    );
    validate_pattern_handoff(plan, bundle)?;
    if let Some(routing) = report
        .get("routing")
        .filter(|r| !r.is_null() && r.as_object().is_none_or(|o| !o.is_empty()))
    {
        validate_pattern_handoff(routing, bundle)?;
    }
    let empty = json!([]);
    let results = array(
        report.get("results").unwrap_or(&empty),
        "supplier_handoff_review_source_mismatch",
    )?;
    for page in array(
        &plan["evidence_input"]["pages"],
        "supplier_handoff_review_source_mismatch",
    )? {
        let result = results
            .iter()
            .find(|r| r["id"] == page["page_id"])
            .context("supplier_handoff_review_source_mismatch")?;
        ensure!(
            result["source_sha256"] == page["source_sha256"]
                && result["text_sha256"] == page["text_sha256"],
            "supplier_handoff_review_source_mismatch"
        );
        let body = string(
            &result["native_text"],
            "supplier_handoff_review_span_mismatch",
        )?;
        for span in array(&page["spans"], "supplier_handoff_review_span_mismatch")? {
            let unit = array(
                &result["knowledge_units"],
                "supplier_handoff_review_span_mismatch",
            )?
            .iter()
            .find(|u| u["id"] == span["unit_id"])
            .context("supplier_handoff_review_span_mismatch")?;
            let parent = &unit["source_span"];
            let start = span["start"]
                .as_u64()
                .context("supplier_handoff_review_span_mismatch")?;
            let end = span["end"]
                .as_u64()
                .context("supplier_handoff_review_span_mismatch")?;
            let selected = span_text(body, &span["start"], &span["end"])?;
            ensure!(
                parent["start"].as_u64().is_some_and(|s| s <= start)
                    && parent["end"].as_u64().is_some_and(|e| end <= e)
                    && span["parent_span_sha256"] == parent["sha256"]
                    && span["text"] == selected
                    && span["sha256"] == sha256(&selected),
                "supplier_handoff_review_span_mismatch"
            );
        }
    }
    let actions = array(&plan["actions"], "supplier_handoff_review_choices_mismatch")?;
    let answers = object(&plan["answers"], "supplier_handoff_review_choices_mismatch")?;
    let mut ids = BTreeSet::new();
    let mut selected = Vec::new();
    let mut uncertain = Vec::new();
    for action in actions {
        let id = string(&action["id"], "supplier_handoff_review_choices_mismatch")?;
        ensure!(ids.insert(id), "supplier_handoff_review_choices_mismatch");
        ensure!(
            action["status"] == "candidate"
                && action["execution"] == "not_run"
                && action["adoption"] == "pending_agent_review",
            "supplier_handoff_review_unreviewed_assertion"
        );
        let choice = string(
            &action["choice"],
            "supplier_handoff_review_choices_mismatch",
        )?;
        ensure!(
            ["needed", "not_needed", "not_established"].contains(&choice)
                && answers.get(id).is_some_and(|a| a["choice"] == choice),
            "supplier_handoff_review_choices_mismatch"
        );
        if choice == "needed" {
            selected.push(id);
        }
        if choice == "not_established" {
            uncertain.push(id);
        }
    }
    ensure!(
        ids == answers.keys().map(String::as_str).collect::<BTreeSet<_>>()
            && plan["selected_actions"] == json!(selected)
            && plan["uncertain_actions"] == json!(uncertain),
        "supplier_handoff_review_choices_mismatch"
    );
    let missing = array(
        plan.get("missing_action_ids").unwrap_or(&empty),
        "supplier_handoff_review_choices_mismatch",
    )?;
    let mut missing_ids = BTreeSet::new();
    for id in missing {
        let id = string(id, "supplier_handoff_review_choices_mismatch")?;
        ensure!(
            !id.is_empty() && !ids.contains(id) && missing_ids.insert(id),
            "supplier_handoff_review_choices_mismatch"
        );
    }
    ensure!(
        missing.is_empty() || plan["status"] != "completed",
        "supplier_handoff_review_choices_mismatch"
    );
    Ok(plan.clone())
}

fn duration(v: &Value) -> bool {
    v.as_number()
        .and_then(|n| n.as_f64())
        .is_some_and(|n| n.is_finite() && n >= 0.0)
}
pub fn query_observations(report: &Value) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for k in [
        "elapsed_seconds",
        "query_budget",
        "timing",
        "calls",
        "run_id",
        "execution_summary",
    ] {
        if let Some(v) = report.get(k) {
            out.insert(k.to_owned(), v.clone());
        }
    }
    digest(&Value::Object(out.clone())).context("supplier_handoff_invalid_query_observations")?;
    if let Some(v) = out.get("elapsed_seconds") {
        ensure!(duration(v), "supplier_handoff_invalid_query_observations");
    }
    if let Some(v) = out.get("run_id") {
        ensure!(
            v.as_str().is_some_and(|s| !s.trim().is_empty()),
            "supplier_handoff_invalid_query_observations"
        );
    }
    if let Some(v) = out.get("calls") {
        ensure!(v.is_array(), "supplier_handoff_invalid_query_observations");
    }
    if let Some(v) = out.get("execution_summary") {
        ensure!(v.is_object(), "supplier_handoff_invalid_query_observations");
    }
    if let Some(v) = out.get("timing") {
        for (k, v) in object(v, "supplier_handoff_invalid_query_observations")? {
            if k.ends_with("_seconds") || k.ends_with("_seconds_sum") {
                if k == "stage_seconds" {
                    ensure!(
                        v.as_object().is_some_and(|o| o.values().all(duration)),
                        "supplier_handoff_invalid_query_observations"
                    );
                } else {
                    ensure!(duration(v), "supplier_handoff_invalid_query_observations");
                }
            }
        }
    }
    if let Some(v) = out.get("query_budget") {
        let max = v["max_requests"]
            .as_u64()
            .context("supplier_handoff_invalid_query_observations")?;
        let used = v["network_requests"]
            .as_u64()
            .context("supplier_handoff_invalid_query_observations")?;
        let left = v["remaining_requests"]
            .as_u64()
            .context("supplier_handoff_invalid_query_observations")?;
        let fixture = v
            .get("fixture_requests")
            .map(|x| {
                x.as_u64()
                    .context("supplier_handoff_invalid_query_observations")
            })
            .transpose()?
            .unwrap_or(0);
        let charged = used
            .checked_add(fixture)
            .context("supplier_handoff_invalid_query_observations")?;
        let reserved = v
            .get("reserved_attempts")
            .map(|x| {
                x.as_u64()
                    .context("supplier_handoff_invalid_query_observations")
            })
            .transpose()?
            .unwrap_or(charged);
        ensure!(
            v["denied_requests"].as_u64().is_some()
                && max >= 1
                && reserved == charged
                && charged <= max
                && left == max - charged
                && ["available", "exhausted"].contains(&v["status"].as_str().unwrap_or_default()),
            "supplier_handoff_invalid_query_observations"
        );
    }
    Ok(out)
}

pub fn make(root: &SkillRoot, report: &Value, context: &Value) -> Result<Value> {
    make_with_tools(root, report, context, &PopplerTools::default())
}

pub fn make_with_tools(
    root: &SkillRoot,
    report: &Value,
    context: &Value,
    tools: &PopplerTools,
) -> Result<Value> {
    let context = context::validate(context)?;
    ensure!(
        [
            "misumi.search-result/v1",
            "ontology-engineering.knowledge-search-result/v1"
        ]
        .contains(&report["schema"].as_str().unwrap_or_default())
            && report["context_sha256"] == digest(&context)?,
        "supplier_handoff_context_mismatch"
    );
    ensure!(
        report["parameter_verification"] == "not_run",
        "supplier_handoff_unexpected_parameter_verdict"
    );
    let candidates = checked_evidence(
        root,
        report.get("results").unwrap_or(&json!([])),
        &context,
        tools,
    )?;
    let review = review_handoff(report, &context)?;
    let freshness = context::verify_sources(root, &context)?;
    let mut errors = array(
        report.get("errors").unwrap_or(&json!([])),
        "supplier_handoff_invalid_errors",
    )?
    .clone();
    if freshness["status"] == "stale"
        && !errors
            .iter()
            .any(|e| e["code"] == "knowledge_context_sources_stale")
    {
        errors.push(json!({"code":"knowledge_context_sources_stale"}));
    }
    let mut packet = json!({"schema":SCHEMA,"status":if freshness["status"]=="stale"{json!("stale_context")}else{report.get("status").context("supplier_handoff_missing_status")?.clone()},"query":report.get("query").context("supplier_handoff_missing_query")?,"task_id":context["task_id"],"project":context["project"],"object":context["object"],"context_sha256":digest(&context)?,"context_snapshot":context,"search_record":report["record_file"],"index":report["index"],"routing":report["routing"],"knowledge_gaps":report.get("knowledge_gaps").unwrap_or(&json!([])),"method_bundle":report["method_bundle"],"review_plan":review,"evidence_candidates":candidates,"errors":errors,"project_source_freshness":freshness,"limitations":report.get("limitations").unwrap_or(&json!([])),"usage":report["usage"],"integrity":"source_hash_and_exact_text_spans_checked","adoption":"pending_agent_review","semantic_review":"not_run","ontology_promotion":"not_run"});
    if let Some(id) = context.get("decision_id") {
        packet["decision_id"] = id.clone();
    }
    packet
        .as_object_mut()
        .context("supplier_handoff_internal_packet")?
        .extend(query_observations(report)?);
    Ok(packet)
}

pub fn verify(root: &SkillRoot, packet: &Value, current: &Value) -> Result<Value> {
    verify_with_tools(root, packet, current, &PopplerTools::default())
}

pub fn verify_with_tools(
    root: &SkillRoot,
    packet: &Value,
    current: &Value,
    tools: &PopplerTools,
) -> Result<Value> {
    let current = context::validate(current)?;
    ensure!(
        packet.is_object() && packet["schema"] == SCHEMA,
        "supplier_handoff_packet_schema_mismatch"
    );
    query_observations(packet)?;
    let snapshot = context::validate(&packet["context_snapshot"])?;
    let snapshot_sha = digest(&snapshot)?;
    ensure!(
        packet["context_sha256"] == snapshot_sha,
        "supplier_handoff_packet_context_mismatch"
    );
    for k in ["task_id", "project", "object", "decision_id"] {
        ensure!(
            packet.get(k) == snapshot.get(k),
            "supplier_handoff_packet_identity_mismatch"
        );
    }
    ensure!(
        packet["adoption"] == "pending_agent_review"
            && packet["semantic_review"] == "not_run"
            && packet["ontology_promotion"] == "not_run",
        "supplier_handoff_unreviewed_assertion"
    );
    let mut results = Vec::new();
    for candidate in array(
        &packet["evidence_candidates"],
        "supplier_handoff_packet_evidence_mismatch",
    )? {
        let mut result = object(
            &candidate["source"],
            "supplier_handoff_packet_evidence_mismatch",
        )?
        .clone();
        ensure!(
            candidate["adoption"] == "pending_agent_review"
                && candidate["applicability"] == "not_assessed"
                && candidate["engineering_verdict"] == "not_run",
            "supplier_handoff_unreviewed_assertion"
        );
        for (src, dst) in [
            ("id", "id"),
            ("page_native_text", "native_text"),
            ("knowledge_units", "knowledge_units"),
            ("candidate_judgment", "judgment"),
        ] {
            result.insert(
                dst.to_owned(),
                candidate
                    .get(src)
                    .context("supplier_handoff_packet_evidence_mismatch")?
                    .clone(),
            );
        }
        results.push(Value::Object(result));
    }
    let checked = checked_evidence(root, &json!(results), &snapshot, tools)?;
    let bundle = packet
        .get("method_bundle")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or(json!({}));
    review_handoff(
        &json!({"results":results,"review_plan":packet["review_plan"],"routing":packet["routing"],"method_bundle":bundle,"method_bundle_sha256":bundle["sha256"],"knowledge_policy_sha256":bundle["sha256"]}),
        &snapshot,
    )?;
    let current_sha = digest(&current)?;
    let freshness = context::verify_sources(root, &current)?;
    let packet_freshness = context::verify_sources(root, &snapshot)?;
    let mut missing = Vec::new();
    for name in ["project", "object"] {
        for field in ["id", "revision"] {
            if snapshot[name][field]
                .as_str()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                missing.push(format!("{name}.{field}"));
            }
        }
    }
    if snapshot["decision_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        missing.push("decision_id".to_owned());
    }
    let status = if current_sha != snapshot_sha {
        "changed"
    } else if freshness["status"] == "stale" {
        "stale_context"
    } else if !missing.is_empty() {
        "unbound"
    } else {
        "ready_for_agent_review"
    };
    let keys = snapshot
        .as_object()
        .unwrap()
        .keys()
        .chain(current.as_object().unwrap().keys())
        .collect::<BTreeSet<_>>();
    let changed = keys
        .into_iter()
        .filter(|k| snapshot.get(*k) != current.get(*k))
        .cloned()
        .collect::<Vec<_>>();
    Ok(
        json!({"schema":"ontology-engineering.supplier-packet-binding/v1","status":status,"packet_context_sha256":snapshot_sha,"current_context_sha256":current_sha,"changed_fields":changed,"missing_identity":missing,"evidence_candidate_count":checked.as_array().map_or(0,Vec::len),"project_source_freshness":freshness,"packet_source_freshness":packet_freshness,"source_query_status":packet.get("status").unwrap_or(&json!("not_reported")),"source_query_errors":packet.get("errors").unwrap_or(&json!([])),"evidence_availability":if results.is_empty(){"no_returned_candidates"}else{"returned_candidates"},"review_plan_status":packet["review_plan"].get("status").unwrap_or(&json!("not_run")),"integrity":"source_hash_and_exact_text_spans_checked","adoption":"pending_agent_review","semantic_review":"not_run","ontology_promotion":"not_run","meaning":"Same-context handoff integrity only; matching identity does not mean the source query completed. Freshness covers declared source records only; not_bound does not establish current project state. No returned candidates does not establish that source information is absent. No applicability, engineering adoption or execution authority."}),
    )
}
