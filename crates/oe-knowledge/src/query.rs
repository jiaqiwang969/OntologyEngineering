//! Problem-led bounded retrieval and source evidence under the existing project contract.
mod navigation;
mod reading;
use crate::{
    context, feedback,
    policy::{self, MethodPolicy},
    poppler::PopplerTools,
    sources::{Provider, RetrievalRequest},
};
use anyhow::{Context, Result, ensure};
use oe_core::json::canonical_digest;
use oe_core::{SkillRoot, write_new};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Instant, SystemTime},
};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    pub kind: String,
    pub index: Option<PathBuf>,
}

pub struct QueryOptions {
    pub query: String,
    pub context: Option<Value>,
    pub history: Vec<Value>,
    pub providers: Vec<ProviderSpec>,
    pub candidates: usize,
    pub top: usize,
    pub output: PathBuf,
    pub full_text: bool,
    pub tools: PopplerTools,
    pub retrieval_order: String,
}

pub(crate) struct Engine<'a> {
    pub query: &'a str,
    pub context: &'a Value,
    pub context_sha: String,
    pub policy: &'a MethodPolicy,
    pub retrieval_order: &'a str,
}

pub(crate) fn merge(target: &mut Value, addition: Value) -> Result<()> {
    target
        .as_object_mut()
        .context("knowledge_report_not_object")?
        .extend(
            addition
                .as_object()
                .context("knowledge_projection_not_object")?
                .clone(),
        );
    Ok(())
}

pub(crate) fn error(code: &str, stage: &str) -> Value {
    json!({"code":code,"stage":stage})
}

pub fn run(root: &SkillRoot, options: &QueryOptions) -> Result<Value> {
    let started = Instant::now();
    ensure!(
        !options.query.trim().is_empty() && options.query.chars().count() <= 4000,
        "knowledge_query_invalid"
    );
    context::validate(
        &json!({"schema":context::SCHEMA,"task_id":"query-validation","goal":options.query}),
    )?;
    ensure!(
        (1..=96).contains(&options.candidates)
            && options.top > 0
            && options.top <= options.candidates,
        "knowledge_candidate_limits_invalid"
    );
    ensure!(
        options.history.len() <= 16,
        "knowledge_query_limits_invalid"
    );
    ensure!(
        !options.providers.is_empty() && options.providers.len() <= 2,
        "knowledge_provider_selection_invalid"
    );
    let mut kinds = BTreeSet::new();
    for spec in &options.providers {
        ensure!(
            kinds.insert(match spec.kind.as_str() {
                "local-pdf" | "local_pdf" => "local_pdf",
                "misumi" => "misumi",
                _ => anyhow::bail!("unknown_provider"),
            }),
            "duplicate_provider_selection"
        );
    }
    let input=options.context.clone().unwrap_or_else(||json!({"schema":context::SCHEMA,"task_id":format!("ad-hoc-{}",&oe_core::sha256(options.query.as_bytes())[..16]),"goal":options.query}));
    let context = if input["schema"] == context::PROJECTION_SCHEMA {
        context::project(root, &input)?
    } else {
        context::validate(&input)?
    };
    let context_sha = canonical_digest(&context)?;
    let initial_freshness = context::verify_sources(root, &context)?;
    let policy = policy::load(root)?;
    let mut history = Vec::new();
    for reference in &options.history {
        history.push(feedback::load_reference_with_tools(
            root,
            reference,
            &context,
            &options.tools,
        )?);
    }
    let mut providers = Vec::new();
    let mut readiness = Vec::new();
    let mut errors = Vec::new();
    if initial_freshness["status"] != "stale" {
        for spec in &options.providers {
            match Provider::open(root, &spec.kind, spec.index.as_deref()) {
                Ok(p) => {
                    readiness.push(p.status());
                    providers.push(p);
                }
                Err(_) => {
                    readiness.push(json!({"provider":spec.kind,"status":"not_ready","errors":[{"code":"source_index_missing_invalid_or_changed"}]}));
                    errors.push(error("source_index_missing_invalid_or_changed", &spec.kind));
                }
            }
        }
    }
    let statuses = readiness.clone();
    let output = root.private_path(&options.output)?;
    root.new_private_dir(&output)?;
    let engine = Engine {
        query: &options.query,
        context: &context,
        context_sha: context_sha.clone(),
        policy: &policy,
        retrieval_order: &options.retrieval_order,
    };
    let mut report = json!({"schema":"ontology-engineering.knowledge-search-result/v1","version":env!("CARGO_PKG_VERSION"),"query":options.query,
        "created_at":chrono::DateTime::<chrono::Utc>::from(SystemTime::now()).to_rfc3339(),"status":"running","mode":"local",
        "model":null,"context_sha256":context_sha,"context_snapshot":context,
        "knowledge_policy_sha256":policy.sha256,"method_bundle":policy.value["method_bundle"],"method_bundle_sha256":policy.sha256,
        "project_source_freshness":initial_freshness,"explicit_history":history,"source_readiness":readiness,"source_indexes":statuses,
        "index":{"fingerprint":canonical_digest(&json!(statuses))?},"results":[],"evaluations":[],"errors":errors,
        "retrieval":{"method":"context_body_fts_round_robin","candidate_limit":options.candidates,"full_corpus_semantic_scan":false,
            "cross_source_order":"round_robin_not_comparable_bm25"},
        "parameter_verification":"not_run","live_information":"not_available_from_archive",
        "semantic_review":"not_run","ontology_promotion":"not_run"});
    let mut stages = json!({"context_and_source_discovery":started.elapsed().as_secs_f64()});
    let mut route = None;
    if initial_freshness["status"] == "stale" {
        report["status"] = json!("stale_context");
        push_error(
            &mut report,
            error("knowledge_context_sources_stale", "context"),
        );
    } else if providers.is_empty() {
        report["status"] = json!("not_ready");
    } else {
        let work = (|| -> Result<()> {
            let mut chapters = Vec::new();
            for provider in &providers {
                chapters.extend(provider.chapters()?);
            }
            let tick = Instant::now();
            let routing = navigation::route(&engine, &chapters)?;
            stages["routing_including_chapters"] = json!(tick.elapsed().as_secs_f64());
            report["routing"] = routing.clone();
            for item in routing["errors"].as_array().into_iter().flatten() {
                push_error(&mut report, item.clone());
            }
            let blocked = match routing["scope"].as_str() {
                Some("live_only") => Some("unsupported_live_query"),
                Some("outside") => Some("no_match_in_candidates"),
                Some("unclear")
                    if routing["needed_topics"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
                        && routing["chapters"].as_array().is_some_and(Vec::is_empty) =>
                {
                    Some("insufficient_context")
                }
                _ => None,
            };
            route = Some(routing.clone());
            if let Some(status) = blocked {
                report["status"] = json!(status);
                report["retrieval"]["candidates_retrieved"] = json!(0);
                return Ok(());
            }
            let tick = Instant::now();
            let (pages, quotas, replays) =
                retrieve(&engine, &providers, &routing, options.candidates)?;
            report["retrieval"]["candidate_replays"] = json!(replays);
            report["retrieval"]["pool_order"] = json!(options.retrieval_order);
            stages["local_retrieval"] = json!(tick.elapsed().as_secs_f64());
            report["retrieval"]["candidates_retrieved"] = json!(pages.len());
            report["retrieval"]["source_quotas"] = quotas;
            let tick = Instant::now();
            let evaluations = reading::screen(&engine, &pages, &routing)?;
            stages["screening"] = json!(tick.elapsed().as_secs_f64());
            for evaluation in &evaluations {
                for code in evaluation["errors"].as_array().into_iter().flatten() {
                    push_error(
                        &mut report,
                        json!({"stage":"screening","page_id":evaluation["id"],"code":code}),
                    );
                }
            }
            report["evaluations"] = json!(evaluations);
            let mut pools: Vec<Vec<(&Value, &Value)>> = Vec::new();
            for (index, _) in providers.iter().enumerate() {
                let mut pool = Vec::new();
                for e in &evaluations {
                    if let Some(page) = pages
                        .iter()
                        .find(|p| p["id"] == e["id"] && p["query_provider_index"] == index)
                        && (matches!(
                            e["relation"].as_str(),
                            Some("direct" | "related" | "lexical_only" | "navigation_only")
                        ) || page["navigation_only"] == true)
                    {
                        pool.push((page, e));
                    }
                }
                pool.sort_by(|(a, ae), (b, be)| {
                    rank(ae)
                        .cmp(&rank(be))
                        .then_with(|| coverage(be, &routing).cmp(&coverage(ae, &routing)))
                        .then_with(|| {
                            a["lexical_score"]
                                .as_f64()
                                .unwrap_or(0.0)
                                .total_cmp(&b["lexical_score"].as_f64().unwrap_or(0.0))
                        })
                        .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
                });
                pools.push(pool);
            }
            let mut results = Vec::new();
            for slot in 0..options.candidates {
                for (i, pool) in pools.iter().enumerate() {
                    let Some((page, evaluation)) = pool.get(slot) else {
                        continue;
                    };
                    match providers[i].source_result(page, &options.query, &options.tools) {
                        Ok(mut candidate) => {
                            candidate["judgment"] = (*evaluation).clone();
                            candidate["judgment"]
                                .as_object_mut()
                                .unwrap()
                                .remove("chunks");
                            candidate["judgment"].as_object_mut().unwrap().remove("id");
                            candidate["knowledge_units"] =
                                json!(reading::units(&engine, page, evaluation, &routing)?);
                            candidate["retrieval_pool"] = page["retrieval_pool"].clone();
                            if options.full_text {
                                candidate["native_text"] = page["body"].clone();
                            }
                            if let Some(o) = candidate.as_object_mut() {
                                for key in [
                                    "body",
                                    "source_json",
                                    "source_registration",
                                    "catalog_registration",
                                    "query_provider_index",
                                ] {
                                    o.remove(key);
                                }
                            }
                            results.push(candidate);
                        }
                        Err(_) => push_error(
                            &mut report,
                            json!({"stage":"source_return","page_id":page["id"],"code":"source_identity_or_locator_unavailable"}),
                        ),
                    }
                    if results.len() >= options.top {
                        break;
                    }
                }
                if results.len() >= options.top {
                    break;
                }
            }
            report["results"] = json!(results);
            report["status"] = json!(if report["errors"].as_array().unwrap().is_empty() {
                if results.is_empty() {
                    "no_match_in_candidates"
                } else {
                    "completed"
                }
            } else {
                "partial"
            });
            Ok(())
        })();
        if let Err(error) = work {
            report["status"] = json!("failed");
            push_error(&mut report, error_value(&error, "query"));
        }
    }
    let before_review = context::verify_sources(root, &context)?;
    if before_review["status"] != "stale"
        && let Some(route) = &route
    {
        let tick = Instant::now();
        match reading::review(&engine, report["results"].as_array().unwrap(), route) {
            Ok(plan) => {
                if matches!(plan["status"].as_str(), Some("failed" | "partial")) {
                    push_error(
                        &mut report,
                        error("incomplete_next_inquiry_review", "review"),
                    );
                    if report["status"] != "failed" {
                        report["status"] = json!("partial");
                    }
                }
                report["review_plan"] = plan;
            }
            Err(e) => {
                push_error(&mut report, error_value(&e, "review"));
                if report["status"] != "failed" {
                    report["status"] = json!("partial");
                }
            }
        }
        stages["next_inquiry_review"] = json!(tick.elapsed().as_secs_f64());
    }
    let freshness = context::verify_sources(root, &context)?;
    report["project_source_freshness"] = freshness.clone();
    if freshness["status"] == "stale" {
        report["status"] = json!("stale_context");
        push_error(
            &mut report,
            error("knowledge_context_sources_stale", "final_check"),
        );
    }
    if !policy::load(root).is_ok_and(|p| p.sha256 == policy.sha256) {
        report["status"] = json!("stale_method");
        push_error(
            &mut report,
            error("knowledge_method_changed", "final_check"),
        );
    }
    for provider in &providers {
        if provider.status()["status"] != "ready" {
            report["status"] = json!("stale_sources");
            push_error(&mut report, error("knowledge_index_changed", "final_check"));
        }
    }
    let mut changed_pages = Vec::new();
    for candidate in report["results"].as_array().unwrap() {
        if crate::packet::verify_returned_source(root, candidate, &options.tools).is_err() {
            changed_pages.push(candidate["id"].clone());
        }
    }
    for page_id in changed_pages {
        report["status"] = json!("stale_sources");
        push_error(
            &mut report,
            json!({"code":"knowledge_original_changed_or_unavailable","stage":"final_check","page_id":page_id}),
        );
    }
    for reference in &options.history {
        if feedback::load_reference_with_tools(root, reference, &context, &options.tools).is_err() {
            report["status"] = json!("stale_history");
            push_error(
                &mut report,
                error("knowledge_history_changed", "final_check"),
            );
        }
    }
    let mut gaps: Vec<_> = context["unknowns"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| json!({"kind":"context_unknown","question":u,"status":"unresolved"}))
        .collect();
    if context["object"]["name"] == "" {
        gaps.push(json!({"kind":"identity","question":"Object identity, revision and applicability need explicit project context.","status":"not_established"}));
    }
    report["knowledge_gaps"] = json!(gaps);
    report["calls"] = json!([]);
    report["usage"] = json!({"network_requests":0,"fixture_requests":0,"model_calls":0});
    report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    report["timing"] = json!({"query_elapsed_seconds":report["elapsed_seconds"],"stage_seconds":stages,
        "meaning":"Observed wall time; parallel stages and attempt times are not additive. Rendering and later engineering adoption are separate."});
    let file = output.join("report.json");
    report["record_file"] = json!(file.strip_prefix(root.path())?);
    write_new(&file, &report)?;
    Ok(report)
}

fn push_error(report: &mut Value, value: Value) {
    report["errors"].as_array_mut().unwrap().push(value);
}
fn error_value(failure: &anyhow::Error, stage: &str) -> Value {
    // Keep stable diagnostic codes without persisting private paths or arbitrary tool output.
    let codes: Vec<_> = failure
        .chain()
        .map(ToString::to_string)
        .filter(|message| {
            message.len() <= 96
                && message.contains('_')
                && message
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
        .collect();
    let mut value = error(
        codes
            .first()
            .map(String::as_str)
            .unwrap_or("knowledge_stage_failed"),
        stage,
    );
    value["causes"] = json!(codes);
    value
}
fn rank(e: &Value) -> usize {
    match e["relation"].as_str() {
        Some("direct") => 0,
        Some("related") => 1,
        Some("lexical_only") => 2,
        _ => 3,
    }
}
fn coverage(e: &Value, route: &Value) -> usize {
    e["knowledge_roles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| {
            route["needed_topics"]
                .as_array()
                .is_some_and(|a| a.contains(t))
        })
        .count()
}

fn retrieve(
    engine: &Engine,
    providers: &[Provider],
    route: &Value,
    limit: usize,
) -> Result<(Vec<Value>, Value, Vec<crate::retrieval::Replay>)> {
    let anchor = engine.context["object"]["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let f = engine.context["functions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            if f.is_empty() { engine.query.into() } else { f }
        });
    let active = route["needed_topics"]
        .as_array()
        .filter(|v| !v.is_empty())
        .or_else(|| route["uncertain_topics"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut active = active;
    active.extend(
        route["unanswered_topics"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );
    let mut topic_ids = BTreeSet::new();
    let mut pools = Vec::new();
    let mut quotas = Map::new();
    let mut replays = Vec::new();
    for (i, provider) in providers.iter().enumerate() {
        let quota = limit / providers.len() + usize::from(i < limit % providers.len());
        let status = provider.status();
        quotas.insert(
            status["provider"].as_str().unwrap_or("unknown").into(),
            json!(quota),
        );
        if quota == 0 {
            pools.push(Vec::new());
            continue;
        }
        let available = provider.chapters()?;
        let chapters: Vec<_> = route["chapters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["id"].as_str())
            .filter(|id| available.iter().any(|a| a["id"] == *id))
            .map(str::to_owned)
            .collect();
        let mut requests = Vec::new();
        let mut labels = Vec::new();
        let named = engine.context["object"]["name"].as_str().unwrap_or("");
        let mut seen = BTreeSet::new();
        for label in named
            .split(|c: char| c.is_whitespace() || "、,，;；".contains(c))
            .filter(|s| !s.is_empty())
            // Valid context names may contain numeric IDs or punctuation. They
            // stay in the source snapshot but cannot form an empty body query.
            // Filter before the six-query limit so later useful names survive.
            .filter(|s| !crate::text::match_expression(s).is_empty())
            .filter(|s| seen.insert((*s).to_string()))
            .take(6)
        {
            requests.push(RetrievalRequest::Body {
                anchor: label.into(),
                terms: String::new(),
                limit: quota,
                chapters: vec![],
            });
            labels.push(format!("explicit_object:{label}"));
        }
        topic_ids.clear();
        for tid in &active {
            let Some(id) = tid.as_str() else {
                continue;
            };
            if !topic_ids.insert(id) {
                continue;
            }
            let topic = engine.policy.value["topics"]
                .as_array()
                .context("knowledge_topics_missing")?
                .iter()
                .find(|t| t["id"] == id)
                .context("knowledge_topic_missing")?;
            let terms = topic["terms"]
                .as_str()
                .context("knowledge_topic_terms_invalid")?;
            requests.push(RetrievalRequest::Body {
                anchor: anchor.clone(),
                terms: terms.into(),
                limit: quota,
                chapters: vec![],
            });
            labels.push(format!("{id}:object_and_body"));
            if !chapters.is_empty() {
                requests.push(RetrievalRequest::Body {
                    anchor: String::new(),
                    terms: terms.into(),
                    limit: quota,
                    chapters: chapters.clone(),
                });
                labels.push(format!("{id}:routed_chapter"));
            }
        }
        requests.push(RetrievalRequest::Lexical {
            query: format!("{} {anchor}", engine.query),
            limit: quota,
            chapters,
        });
        labels.push("query_and_object".into());
        let mut retrieved = provider.retrieve_plan(&requests)?;
        for (pool, label) in retrieved.iter_mut().zip(&labels) {
            for p in pool.iter_mut() {
                p["retrieval_pool"] = json!(label);
                p["query_provider_index"] = json!(i);
            }
            if label.starts_with("explicit_object:") {
                let needle = crate::text::normalize(label.trim_start_matches("explicit_object:"));
                pool.sort_by_key(|p| {
                    let opening = crate::text::normalize(
                        &p["body"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(500)
                            .collect::<String>(),
                    )
                    .split_whitespace()
                    .collect::<String>();
                    let cues = [
                        "基础",
                        "技术资料",
                        "概要",
                        "原理",
                        "选型",
                        "安装",
                        "注意事项",
                        "使用方法",
                        "结构",
                    ]
                    .iter()
                    .filter(|w| opening.contains(**w))
                    .count()
                    .min(2);
                    (
                        std::cmp::Reverse(opening.contains(&needle)),
                        std::cmp::Reverse(cues),
                    )
                });
            }
        }
        let replay = crate::retrieval::Replay {
            schema: "oev2.retrieval-pools/1".into(),
            query: engine.query.into(),
            context_sha256: engine.context_sha.clone(),
            provider: status["provider"].as_str().unwrap().into(),
            index_identity: status["index_identity"].clone(),
            limit: quota,
            pools: labels
                .into_iter()
                .zip(retrieved)
                .map(|(label, pages)| crate::retrieval::Pool { label, pages })
                .collect(),
        };
        let selected = crate::retrieval::select_retrieval_pools(&replay, engine.retrieval_order)?;
        replays.push(replay);
        pools.push(selected);
    }
    let mut pages = Vec::new();
    for slot in 0..limit {
        for pool in &pools {
            if let Some(page) = pool.get(slot) {
                pages.push(page.clone());
            }
        }
    }
    Ok((pages, Value::Object(quotas), replays))
}
