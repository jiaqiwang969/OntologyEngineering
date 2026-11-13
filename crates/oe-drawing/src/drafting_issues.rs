//! Deterministic review items from ontology findings; interpretation-only deviations cannot edit a source.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::sha256;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const CUTTING_RULE: &str = "G2-043-cutting-letter-required";

#[derive(Clone, Debug)]
struct Span {
    path: String,
    start: f64,
    end: f64,
}

/// Rule facts use a derived visible-outline object for an interval-scoped coincidence or hatch
/// overrun.  Its ID carries the focus object's normalized interval so the review item can reuse
/// the same range without widening back to the member's whole circle/line.
fn derived_source_intervals(value: Option<&str>) -> Vec<[f64; 2]> {
    let Some(value) = value else {
        return Vec::new();
    };
    let mut out: Vec<[f64; 2]> = Vec::new();
    for part in value.split('|') {
        let Some((_, marker)) = part
            .split_once("coincidence_span:")
            .or_else(|| part.split_once("hatch_overrun_span:"))
        else {
            continue;
        };
        let mut fields = marker.split(':');
        let (Some(start), Some(end)) = (
            fields.next().and_then(|s| s.trim().parse::<f64>().ok()),
            fields.next().and_then(|s| s.trim().parse::<f64>().ok()),
        ) else {
            continue;
        };
        if end <= start {
            continue;
        }
        let interval = [start.clamp(0., 1.), end.clamp(0., 1.)];
        if !out
            .iter()
            .any(|old| (old[0] - interval[0]).abs() < 1e-10 && (old[1] - interval[1]).abs() < 1e-10)
        {
            out.push(interval);
        }
    }
    out
}

fn curve_boundary_intervals(member: &Value) -> Vec<(f64, f64)> {
    member["evidence_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|e| e.strip_prefix("section_boundary:"))
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .filter_map(|e| {
            let a = e["interval"][0].as_f64()?;
            let b = e["interval"][1].as_f64()?;
            Some((a.clamp(0., 1.), b.clamp(0., 1.)))
        })
        .filter(|(a, b)| b > a)
        .collect()
}

fn geometry_kind<'a>(graph: &'a Value, handle: &str) -> Option<&'a str> {
    graph["source"]["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["id"] == handle || e["handle"] == handle)
        .and_then(|e| e["geometry_kind"].as_str())
}

fn focused_members<'a>(
    focus: &str,
    members: &[&'a Value],
    source_handles: &[String],
) -> Vec<&'a Value> {
    members
        .iter()
        .copied()
        .filter(|member| {
            let Some(id) = member["id"].as_str() else {
                return false;
            };
            if id.is_empty() || !focus.contains(id) {
                return false;
            }
            let Some(handle) = member["source_handle"].as_str() else {
                return false;
            };
            source_handles.iter().any(|source| source == handle)
        })
        .collect()
}

fn focused_line_index(focus: &str, lines: &Value) -> Option<usize> {
    lines
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .find(|(_, line)| {
            line["id"].as_str().is_some_and(|id| {
                let marker = format!("{id}:");
                focus.contains(&marker) || focus.ends_with(id)
            })
        })
        .map(|(index, _)| index)
        .or_else(|| {
            focus
                .rsplit_once("line:")
                .and_then(|(_, rest)| rest.split(':').next())
                .and_then(|n| n.parse::<usize>().ok())
        })
}

fn spans(f: &MdDraftingFinding, graph: &Value) -> Vec<Span> {
    let intervals = derived_source_intervals(f.value.as_deref());
    if !intervals.is_empty() && !f.source_handles.is_empty() {
        return f
            .source_handles
            .iter()
            .flat_map(|path| {
                intervals.iter().map(move |interval| Span {
                    path: path.clone(),
                    start: interval[0],
                    end: interval[1],
                })
            })
            .collect();
    }
    let all: Vec<_> = graph["members"].as_array().into_iter().flatten().collect();
    let line_index = focused_line_index(&f.focus, &graph["structural_lines"]);
    // A structural-line focus names the concrete carrier line after an enclosing
    // region/member context.  Including that context here would make every line
    // in one section share the region's source handle and would merge distinct
    // physical positions.  For non-line findings, select every focused member
    // whose source handle is named by this finding; member-id length is not a
    // physical identity and must not decide the location.
    let mut members: Vec<_> = if line_index.is_none() {
        focused_members(&f.focus, &all, &f.source_handles)
    } else {
        Vec::new()
    };
    if let Some(index) = line_index {
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
        let curve_intervals = if matches!(geometry_kind(graph, h), Some("circle" | "arc")) {
            curve_boundary_intervals(m)
        } else {
            vec![]
        };
        let intervals = if curve_intervals.is_empty() {
            vec![(
                m["interval"][0].as_f64().unwrap_or(0.).clamp(0., 1.),
                m["interval"][1].as_f64().unwrap_or(1.).clamp(0., 1.),
            )]
        } else {
            curve_intervals
        };
        // The source handle is the physical identity used for issue merging. Native member paths
        // remain available in the finding details, but must not split one source occurrence.
        spans.entry(h.to_owned()).or_default().extend(intervals);
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
            positions.push(Span {
                path: path.clone(),
                start,
                end,
            });
        }
    }
    if positions.is_empty() {
        positions.push(Span {
            path: format!("derived:{}", f.focus),
            start: 0.,
            end: 1.,
        });
    }
    positions
}

fn render_spans(spans: &[Span]) -> Vec<String> {
    let mut grouped: BTreeMap<&str, Vec<(f64, f64)>> = BTreeMap::new();
    for span in spans {
        grouped
            .entry(span.path.as_str())
            .or_default()
            .push((span.start, span.end));
    }
    let mut positions = Vec::new();
    for (path, mut intervals) in grouped {
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
                path.to_owned()
            } else {
                format!("{path}#[{start},{end}]")
            });
        }
    }
    positions
}

fn same_position(a: &[Span], b: &[Span]) -> bool {
    // A source handle is the physical identity. Interval overlap is therefore included whenever
    // two findings point to the same handle, including a whole-source span and a sub-interval.
    a.iter()
        .any(|left| b.iter().any(|right| left.path == right.path))
}

struct FindingGroup<'a> {
    rule: String,
    spans: Vec<Span>,
    findings: Vec<&'a MdDraftingFinding>,
}

fn confidence_rank(confidence: &str) -> u8 {
    match confidence {
        "certain" => 0,
        "likely" => 1,
        _ => 2,
    }
}

fn rule_number(rule: &str) -> (u32, u32) {
    let Some(rest) = rule.strip_prefix('G') else {
        return (u32::MAX, u32::MAX);
    };
    let Some((group, number)) = rest.split_once('-') else {
        return (u32::MAX, u32::MAX);
    };
    (
        group.parse().unwrap_or(u32::MAX),
        number
            .split('-')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(u32::MAX),
    )
}

/// Presentation groups are ordered by their strongest item.  Revision differences are evidence
/// for comparison rather than drafting deviations, so they stay after every ordinary rule group.
fn group_order_key<'a>(
    rule: &'a str,
    items: &[&'a MdDraftingIssue],
) -> (u8, u8, (u32, u32), &'a str) {
    let revision = items
        .iter()
        .any(|item| item.classification == "revision_difference");
    let confidence = items
        .iter()
        .map(|item| confidence_rank(&item.confidence))
        .min()
        .unwrap_or(2);
    (
        if revision { 1 } else { 0 },
        confidence,
        rule_number(rule),
        rule,
    )
}

fn ordered_rule_groups(issues: &[MdDraftingIssue]) -> Vec<(&str, Vec<&MdDraftingIssue>)> {
    let mut by_rule: BTreeMap<&str, Vec<&MdDraftingIssue>> = BTreeMap::new();
    for issue in issues {
        by_rule.entry(issue.rule.as_str()).or_default().push(issue);
    }
    let mut groups: Vec<_> = by_rule.into_iter().collect();
    groups.sort_by(|(left_rule, left_items), (right_rule, right_items)| {
        group_order_key(left_rule, left_items).cmp(&group_order_key(right_rule, right_items))
    });
    groups
}

fn sort_issues_for_presentation(issues: &mut [MdDraftingIssue]) {
    let mut ranks: BTreeMap<String, (u8, u8, (u32, u32), String)> = BTreeMap::new();
    for (rule, items) in ordered_rule_groups(issues) {
        let (revision, confidence, number, name) = group_order_key(rule, &items);
        ranks.insert(
            rule.to_owned(),
            (revision, confidence, number, name.to_owned()),
        );
    }
    issues.sort_by(|left, right| {
        let left_group = &ranks[&left.rule];
        let right_group = &ranks[&right.rule];
        left_group
            .cmp(right_group)
            .then_with(|| {
                confidence_rank(&left.confidence).cmp(&confidence_rank(&right.confidence))
            })
            .then_with(|| left.location.cmp(&right.location))
            .then_with(|| left.id.cmp(&right.id))
    });
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
    let mut ordered: Vec<&MdDraftingFinding> = report.findings.iter().collect();
    ordered.sort_by_cached_key(|f| serde_json::to_string(f).unwrap_or_default());
    let mut groups: Vec<FindingGroup<'_>> = Vec::new();
    for finding in ordered {
        let current_spans = spans(finding, graph);
        let mut combined = FindingGroup {
            rule: finding.rule_id.clone(),
            spans: current_spans.clone(),
            findings: vec![finding],
        };
        let mut retained = Vec::with_capacity(groups.len());
        for group in groups {
            if group.rule == combined.rule && same_position(&group.spans, &current_spans) {
                combined.spans.extend(group.spans);
                combined.findings.extend(group.findings);
            } else {
                retained.push(group);
            }
        }
        retained.push(combined);
        groups = retained;
    }
    let mut issues = Vec::new();
    for group in groups {
        let rule = group.rule;
        let positions = render_spans(&group.spans);
        let mut found = group.findings;
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
            rule_label: meta.label.clone(),
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
                rule_label: "版本比较".into(),
                classification: "revision_difference".into(),
                layers: vec![],
                details: change.clone(),
            });
        }
    }
    sort_issues_for_presentation(&mut issues);
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
    for (rule, items) in ordered_rule_groups(issues) {
        let label = items[0].rule_label.as_str();
        out.push_str(&format!("\n规则：{label}（{rule}） · {} 条\n", items.len()));
        for item in items {
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
                "编号：{} · {} · {}\n位置：{}\n命中层：{}\n事实：{}\n规则：{}（{}）\n条款：{}\n建议：{}\n",
                item.id,
                class,
                confidence,
                item.location,
                item.layers.join("、"),
                item.observed.join("；"),
                item.rule_label,
                item.rule,
                item.clause,
                item.proposed_correction
            ));
        }
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
            render_spans(&spans(
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
            ))
        };
        assert_eq!(at("source:m1"), at("recognition:m1"));
        assert_ne!(at("source:m1"), at("source:m2"));
        assert_eq!(at("presentation:line:0"), vec!["B2#[0,0.4]", "B2#[0.6,1]"]);
        assert_eq!(at("presentation:line:1"), at("source:m3"));
        assert_eq!(at("source:m4"), vec!["B2"]);
    }

    #[test]
    fn structural_line_focus_does_not_merge_distinct_lines_in_one_region() {
        let graph = json!({
            "members": [
                {"id":"expression:region:m0","source_handle":"REGION","interval":[0.,1.]},
                {"id":"expression:line:m1","source_handle":"LINE-A","interval":[0.,1.]},
                {"id":"expression:line:m2","source_handle":"LINE-B","interval":[0.,1.]}
            ],
            "structural_lines": [
                {"id":"line:16","pieces":["member:expression:line:m1"]},
                {"id":"line:162","pieces":["member:expression:line:m2"]}
            ]
        });
        let finding = |line: usize| MdDraftingFinding {
            layer: "presentation".into(),
            rule_id: "fixture".into(),
            focus: format!(
                "presentation:Section:presentation:expression:region:m0:presentation:line:{line}:3"
            ),
            focus_classes: vec![],
            source_handles: vec![],
            expression_ids: vec![],
            value: None,
            message: String::new(),
            severity: "violation".into(),
        };
        let first = spans(&finding(16), &graph);
        let second = spans(&finding(162), &graph);
        assert_eq!(render_spans(&first), vec!["LINE-A"]);
        assert_eq!(render_spans(&second), vec!["LINE-B"]);
        assert!(!same_position(&first, &second));
    }

    #[test]
    fn focused_position_uses_named_source_handle_not_member_id_length_or_order() {
        let finding = |focus: &str, source_handle: &str| MdDraftingFinding {
            layer: "source".into(),
            rule_id: "fixture".into(),
            focus: focus.into(),
            focus_classes: vec![],
            source_handles: vec![source_handle.into()],
            expression_ids: vec![],
            value: None,
            message: String::new(),
            severity: "violation".into(),
        };
        let graph = json!({
            "members": [
                {"id":"region_member_with_long_id","source_handle":"REGION-A","interval":[0.,1.]},
                {"id":"line_member","source_handle":"LINE-A","interval":[0.2,0.8]},
                {"id":"memberbbbb","source_handle":"LINE-B","interval":[0.3,0.7]},
                {"id":"memberaaaa","source_handle":"REGION-B","interval":[0.,1.]}
            ],
            "structural_lines": []
        });
        let longer_region = finding(
            "source:region_member_with_long_id:source:line_member",
            "LINE-A",
        );
        assert_eq!(
            render_spans(&spans(&longer_region, &graph)),
            vec!["LINE-A#[0.2,0.8]"]
        );

        // These IDs have equal length and the non-matching region member is later
        // in the graph, so neither length nor iteration order may affect location.
        let equal_length = finding("source:memberbbbb:source:memberaaaa", "LINE-B");
        assert_eq!(
            render_spans(&spans(&equal_length, &graph)),
            vec!["LINE-B#[0.3,0.7]"]
        );
    }

    #[test]
    fn whole_curve_uses_the_boundary_evidence_interval() {
        let boundary = serde_json::to_string(&json!({
            "interval": [0.2, 0.3],
            "parameter": "source_curve"
        }))
        .unwrap();
        let graph = json!({
            "source": {"entities": [{"id": "C1", "geometry_kind": "circle"}]},
            "members": [{
                "id": "member:C1",
                "source_handle": "C1",
                "interval": [0., 1.],
                "evidence_ids": [format!("section_boundary:{boundary}")]
            }],
            "structural_lines": []
        });
        let finding = MdDraftingFinding {
            layer: "source".into(),
            rule_id: "fixture".into(),
            focus: "source:member:C1".into(),
            focus_classes: vec![],
            source_handles: vec!["C1".into()],
            expression_ids: vec![],
            value: None,
            message: "whole circle boundary violation".into(),
            severity: "violation".into(),
        };
        assert_eq!(render_spans(&spans(&finding, &graph)), vec!["C1#[0.2,0.3]"]);
    }

    #[test]
    fn presentation_groups_put_strongest_confidence_first_and_versions_last() {
        let issue =
            |rule: &str, label: &str, confidence: &str, classification: &str| MdDraftingIssue {
                id: format!("issue-{rule}-{confidence}"),
                location: rule.into(),
                source_occurrences: vec![rule.into()],
                observed: vec![label.into()],
                rule: rule.into(),
                clause: format!("source:{rule}"),
                concept_ids: vec![],
                confidence: confidence.into(),
                alternatives: vec![],
                proposed_correction: "请复核".into(),
                rule_label: label.into(),
                classification: classification.into(),
                layers: vec!["source".into()],
                details: json!({}),
            };
        let mut issues = vec![
            issue(
                "G1-028-likely",
                "很可能组",
                "likely",
                "source_deviation_candidate",
            ),
            issue(
                "G4-043-certain",
                "确定组",
                "certain",
                "source_deviation_candidate",
            ),
            issue(
                "G2-018-needs-expert",
                "要问专家组",
                "needs_expert",
                "recognition_deviation",
            ),
            issue(
                "drawing_revision_comparison",
                "版本差别组",
                "needs_expert",
                "revision_difference",
            ),
        ];
        sort_issues_for_presentation(&mut issues);
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule.as_str())
                .collect::<Vec<_>>(),
            vec![
                "G4-043-certain",
                "G1-028-likely",
                "G2-018-needs-expert",
                "drawing_revision_comparison",
            ]
        );
        let listing = text(&issues);
        let headings = ["确定组", "很可能组", "要问专家组", "版本差别组"];
        let offsets: Vec<_> = headings
            .iter()
            .map(|label| listing.find(&format!("规则：{label}")))
            .collect();
        assert!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "{offsets:?}"
        );
    }
}
