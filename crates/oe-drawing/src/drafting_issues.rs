//! Deterministic review items from ontology findings; interpretation-only deviations cannot edit a source.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::sha256;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const CUTTING_RULE: &str = "G2-043-cutting-letter-required";

fn location(f: &MdDraftingFinding, graph: &Value) -> Vec<String> {
    let focus = f.focus.split_once(':').map_or(f.focus.as_str(), |(_, v)| v);
    let all: Vec<_> = graph["members"].as_array().into_iter().flatten().collect();
    let mut members: Vec<_> = all.iter().copied().filter(|m| m["id"] == focus).collect();
    if let Some(index) = focus
        .strip_prefix("line:")
        .and_then(|n| n.parse::<usize>().ok())
    {
        for part in graph["structural_lines"][index]["pieces"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|p| p.strip_prefix("member:"))
        {
            members.extend(all.iter().copied().filter(|m| m["id"] == part));
        }
    }
    let mut spans: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for m in members {
        let h = m["source_handle"].as_str().unwrap_or_default();
        let path = match m["native_member_id"].as_str() {
            Some(n) if n.starts_with(&format!("{h}/")) => n.to_owned(),
            Some(n) => format!("{h}/{n}"),
            None => h.to_owned(),
        };
        let interval = (
            m["interval"][0].as_f64().unwrap_or(0.),
            m["interval"][1].as_f64().unwrap_or(1.),
        );
        spans.entry(path).or_default().push(interval);
    }
    if spans.is_empty() {
        for h in &f.source_handles {
            spans.entry(h.clone()).or_default().push((0., 1.));
        }
    }
    let mut positions = Vec::new();
    for (path, mut intervals) in spans {
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (start, end) in intervals {
            if let Some(last) = merged.last_mut()
                && start <= last.1 + 1e-9
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        for (start, end) in merged {
            positions.push(if start.abs() < 1e-9 && (end - 1.).abs() < 1e-9 {
                path.clone()
            } else {
                format!("{path}#[{start},{end}]")
            });
        }
    }
    if positions.is_empty() {
        positions.push(format!("derived:{focus}"));
    }
    positions
}

pub fn collect(
    report: &MdDraftingRuleReport,
    graph: &Value,
    source: &[u8],
) -> Result<Vec<MdDraftingIssue>> {
    ensure!(
        report.source_sha256 == sha256(source),
        "issues_source_binding"
    );
    let rules: BTreeMap<_, _> = report.rules.iter().map(|r| (r.id.as_str(), r)).collect();
    let sites = oe_drawing_check::drafting::cutting_sites(source, graph)?;
    let mut groups: BTreeMap<(String, Vec<String>), Vec<&MdDraftingFinding>> = BTreeMap::new();
    for finding in &report.findings {
        groups
            .entry((finding.rule_id.clone(), location(finding, graph)))
            .or_default()
            .push(finding);
    }
    let mut issues = Vec::new();
    for ((rule, positions), mut found) in groups {
        found.sort_by_cached_key(|f| serde_json::to_string(f).unwrap_or_default());
        let meta = rules
            .get(rule.as_str())
            .context("issue_rule_metadata_missing")?;
        let layers: BTreeSet<_> = found.iter().map(|f| f.layer.clone()).collect();
        let classification = if layers.contains("source") {
            "source_deviation_candidate"
        } else {
            "recognition_deviation"
        };
        let mut observed: BTreeSet<_> = found.iter().map(|f| f.message.clone()).collect();
        let id = format!(
            "issue-{}",
            &sha256(serde_json::to_vec(&(&rule, &positions))?)[..20]
        );
        let site = sites.iter().find(|site| {
            found.iter().any(|f| {
                f.focus
                    .split_once(':')
                    .is_some_and(|(_, id)| site["id"] == id)
            })
        });
        if let Some(site) = site {
            observed.insert(format!(
                "{}处缺少字母 {}；同路径参考字母源 {}，端点/转折位置 {}",
                if site["kind"] == "turn" {
                    "转折"
                } else {
                    "端点"
                },
                site["letter"].as_str().unwrap_or_default(),
                site["template_handle"].as_str().unwrap_or_default(),
                site["position"]
            ));
        }
        let views: BTreeSet<_> = found
            .iter()
            .flat_map(|f| f.expression_ids.iter())
            .flat_map(|id| {
                graph["expressions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(move |e| e["id"] == id.as_str())
            })
            .filter_map(|e| e["view_id"].as_str())
            .collect();
        let location = if views.is_empty() {
            positions.join("、")
        } else {
            format!(
                "{} · {}",
                views.into_iter().collect::<Vec<_>>().join("、"),
                positions.join("、")
            )
        };
        let details = json!({"source_positions":positions,"findings":found,"cutting_site":site});
        ensure!(
            ["certain", "likely", "needs_expert"].contains(&meta.confidence.as_str()),
            "issue_confidence_unknown"
        );
        issues.push(MdDraftingIssue {
            id,
            location,
            source_occurrences: positions,
            observed: observed.into_iter().collect(),
            rule,
            clause: meta.clause.clone(),
            concept_ids: meta.concept_ids.clone(),
            confidence: meta.confidence.clone(),
            alternatives: vec![],
            proposed_correction: meta.proposed_correction.clone(),
            classification: classification.into(),
            layers: layers.into_iter().collect(),
            details,
        });
    }
    // A revision difference is evidence to compare versions, not a violation of a drafting rule.
    if let Some(revisions) = graph["revisions"].as_object() {
        for change in revisions
            .get("comparisons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|c| c["differences"].as_array().into_iter().flatten())
        {
            if change["category"] == "same" {
                continue;
            }
            let key = serde_json::to_vec(change)?;
            issues.push(MdDraftingIssue {
                id: format!("revision-{}", &sha256(&key)[..20]),
                location: format!(
                    "版本比较 · 旧 {} → 新 {}",
                    change["older_handles"], change["newer_handles"]
                ),
                source_occurrences: ["older_handles", "newer_handles"]
                    .into_iter()
                    .flat_map(|k| {
                        change[k]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                    })
                    .collect(),
                observed: vec![serde_json::to_string(change)?],
                rule: "drawing_revision_comparison".into(),
                clause: "版本比较不构成制图偏离判定".into(),
                concept_ids: vec!["DC-G9-023".into()],
                confidence: "needs_expert".into(),
                alternatives: vec![],
                proposed_correction: "请专家确认版本差别及现行候选".into(),
                classification: "revision_difference".into(),
                layers: vec![],
                details: change.clone(),
            });
        }
    }
    let priority = |i: &MdDraftingIssue| match i.confidence.as_str() {
        "certain" => 0,
        "likely" => 1,
        _ => 2,
    };
    issues.sort_by(|a, b| {
        (&a.classification, priority(a), &a.rule, &a.location, &a.id).cmp(&(
            &b.classification,
            priority(b),
            &b.rule,
            &b.location,
            &b.id,
        ))
    });
    ensure!(
        issues.iter().all(|i| i.validate().is_ok()),
        "issues_contract"
    );
    Ok(issues)
}

pub fn plan(source: &[u8], issues: Vec<MdDraftingIssue>) -> Result<MdDrawingCorrectionPlan> {
    let mut operations = Vec::new();
    for issue in &issues {
        if issue.classification != "source_deviation_candidate"
            || issue.confidence != "certain"
            || issue.rule != CUTTING_RULE
        {
            continue;
        }
        let text = &issue.details["cutting_site"]["proposed_text"];
        if text.is_null() {
            continue;
        }
        operations.push(MdDrawingCorrectionOperation {
            id: format!("add-{}", issue.id),
            issue_id: issue.id.clone(),
            kind: "add_text".into(),
            occurrence: vec![],
            expected_record_sha256: None,
            text: Some(serde_json::from_value(text.clone())?),
            style: vec![],
            translation: vec![],
        });
    }
    let result = MdDrawingCorrectionPlan {
        source_sha256: sha256(source),
        issues,
        operations,
    };
    ensure!(
        result.validate().is_ok(),
        "automatic_correction_plan_contract"
    );
    Ok(result)
}

pub fn text(issues: &[MdDraftingIssue]) -> String {
    let mut out = String::from("制图不一致与版本比较清单（候选，供复核）\n");
    for item in issues {
        let class = match item.classification.as_str() {
            "source_deviation_candidate" => "原图偏离候选",
            "recognition_deviation" => "识别偏离",
            _ => "版本差别",
        };
        let confidence = match item.confidence.as_str() {
            "certain" => "确定",
            "likely" => "很可能",
            _ => "要问专家",
        };
        out.push_str(&format!(
            "\n{} · {} · {}\n位置：{}\n命中层：{}\n事实：{}\n规则：{}\n条款：{}\n建议：{}\n",
            item.id,
            class,
            confidence,
            item.location,
            item.layers.join("、"),
            item.observed.join("；"),
            item.rule,
            item.clause,
            item.proposed_correction
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_positions_preserve_disjoint_spans_and_nested_occurrences() {
        let graph = json!({"members":[
            {"id":"m1","source_handle":"B2","interval":[0.,0.4]},
            {"id":"m2","source_handle":"B2","interval":[0.6,1.]},
            {"id":"m3","source_handle":"B2","interval":[0.,1.]},
            {"id":"m4","source_handle":"B2","native_member_id":"B2/A1","interval":[0.,1.]}
        ],"structural_lines":[{"pieces":["member:m2","member:m1"]},{"pieces":["member:m1","member:m3"]}]});
        let at = |focus: &str| {
            location(
                &MdDraftingFinding {
                    layer: "source".into(),
                    rule_id: "fixture".into(),
                    focus: focus.into(),
                    focus_classes: vec![],
                    source_handles: vec!["B2".into()],
                    expression_ids: vec![],
                    value: None,
                    message: String::new(),
                    severity: "violation".into(),
                },
                &graph,
            )
        };
        assert_eq!(at("source:m1"), at("recognition:m1"));
        assert_ne!(at("source:m1"), at("source:m2"));
        assert_eq!(at("presentation:line:0"), vec!["B2#[0,0.4]", "B2#[0.6,1]"]);
        assert_eq!(at("presentation:line:1"), at("source:m3"));
        assert_eq!(at("source:m4"), vec!["B2/A1"]);
    }
}
