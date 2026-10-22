//! Root-goal route anchoring: every stage object must trace back to the root
//! object through declared relations, and streaks of off-root or no-progress
//! stages are reported as drift. Only declared relations and counts are
//! checked; whether a declaration is true still needs sources and validation.
use super::{Project, validate};
use anyhow::Result;
use oe_contracts::{RouteAnchor, RouteAuditReport, RouteFinding, RouteStage};
use std::collections::{BTreeMap, BTreeSet};

pub const ANCHOR_SCHEMA: &str = "oev2.route-anchor/1";
pub const REPORT_SCHEMA: &str = "oev2.route-audit/1";
const SCOPE: &str = "Declared stage relations, acceptance progress, streak and review counts only; \
does not prove that a declaration is true, that the deliverable is acceptable, or engineering adoption.";

struct Findings(Vec<RouteFinding>);
impl Findings {
    fn add(&mut self, severity: &str, code: &str, stage: Option<&str>, detail: String) {
        self.0.push(RouteFinding {
            code: code.into(),
            severity: severity.into(),
            stage: stage.map(str::to_owned),
            detail,
        });
    }
    fn invalid(&mut self, code: &str, stage: Option<&str>, detail: String) {
        self.add("invalid", code, stage, detail)
    }
    fn drift(&mut self, code: &str, stage: Option<&str>, detail: String) {
        self.add("drift", code, stage, detail)
    }
}

fn off_root(stage: &RouteStage) -> bool {
    matches!(stage.relation.as_str(), "reference_case" | "external_case")
}

/// Audit `anchor` against `project`. Byte digests are supplied by the caller,
/// which read both inputs; the report binds them so a stale audit is visible.
pub fn audit(
    project: &Project,
    anchor: &RouteAnchor,
    project_sha256: &str,
    anchor_sha256: &str,
) -> Result<RouteAuditReport> {
    validate(project)?;
    let mut out = Findings(Vec::new());
    if let Err(failures) = anchor.validate() {
        for failure in failures {
            out.invalid("anchor_contract_invalid", None, failure);
        }
    }
    // Generated validation is per type; check every nested item explicitly.
    let mut nested: Vec<String> = Vec::new();
    let mut take = |r: Result<(), Vec<String>>| {
        if let Err(f) = r {
            nested.extend(f)
        }
    };
    take(anchor.policy.validate());
    anchor.requests.iter().for_each(|x| take(x.validate()));
    anchor.functions.iter().for_each(|x| take(x.validate()));
    anchor.acceptance.iter().for_each(|x| take(x.validate()));
    anchor.reviews.iter().for_each(|x| take(x.validate()));
    for s in &anchor.stages {
        take(s.validate());
        if let Some(r) = &s.return_to {
            take(r.validate());
        }
    }
    for failure in nested {
        out.invalid("anchor_contract_invalid", None, failure);
    }
    if anchor.schema != ANCHOR_SCHEMA {
        out.invalid("anchor_schema", None, anchor.schema.clone());
    }
    if anchor.project_id != project.project_id {
        out.invalid(
            "project_mismatch",
            None,
            format!("{} != {}", anchor.project_id, project.project_id),
        );
    }
    if !project.objects.contains_key(&anchor.root_object) {
        out.invalid("root_object_unknown", None, anchor.root_object.clone());
    }

    // Policy may only be stricter than the method defaults.
    let p = &anchor.policy;
    for (name, value, max) in [
        ("off_root_stage_limit", p.off_root_stage_limit, 2),
        ("no_progress_stage_limit", p.no_progress_stage_limit, 2),
        ("review_interval", p.review_interval, 5),
    ] {
        if value == 0 || value > max {
            out.invalid(
                "policy_looser_than_method",
                None,
                format!("{name}={value}, allowed 1..={max}"),
            );
        }
    }

    // Unique identities across each list.
    let unique = |out: &mut Findings, kind: &str, ids: Vec<&String>| {
        let mut seen = BTreeSet::new();
        for id in ids {
            if !seen.insert(id) {
                out.invalid("duplicate_id", None, format!("{kind}:{id}"));
            }
        }
    };
    unique(
        &mut out,
        "request",
        anchor.requests.iter().map(|r| &r.id).collect(),
    );
    unique(
        &mut out,
        "function",
        anchor.functions.iter().map(|r| &r.id).collect(),
    );
    unique(
        &mut out,
        "acceptance",
        anchor.acceptance.iter().map(|r| &r.id).collect(),
    );
    unique(
        &mut out,
        "stage",
        anchor.stages.iter().map(|r| &r.id).collect(),
    );
    unique(
        &mut out,
        "review",
        anchor.reviews.iter().map(|r| &r.id).collect(),
    );

    let functions: BTreeSet<_> = anchor.functions.iter().map(|f| f.id.as_str()).collect();
    let acceptance: BTreeSet<_> = anchor.acceptance.iter().map(|a| a.id.as_str()).collect();
    let stage_index: BTreeMap<_, _> = anchor
        .stages
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();
    let approvals: BTreeSet<_> = anchor
        .requests
        .iter()
        .filter(|r| r.kind == "approval")
        .map(|r| r.id.as_str())
        .collect();

    // Root request, deliverable, function decomposition and acceptance.
    if anchor.requests.first().map(|r| r.kind.as_str()) != Some("root") {
        out.invalid(
            "root_request_missing",
            None,
            "first request must be the verbatim root request".into(),
        );
    }
    if anchor.functions.is_empty() {
        out.invalid(
            "function_list_missing",
            None,
            "list the functions of the root object before structure or mechanism".into(),
        );
    }
    if anchor.acceptance.is_empty() {
        out.invalid(
            "acceptance_missing",
            None,
            "the deliverable needs checkable acceptance items".into(),
        );
    }
    let mut covered = BTreeSet::new();
    for a in &anchor.acceptance {
        for f in &a.functions {
            if functions.contains(f.as_str()) {
                covered.insert(f.as_str());
            } else {
                out.invalid("acceptance_function_unknown", None, format!("{}:{f}", a.id));
            }
        }
    }
    for f in &functions {
        if !covered.contains(f) {
            out.drift("function_without_acceptance", None, (*f).into());
        }
    }

    // Relations: each object keeps one relation; component/model chains reach the root.
    let mut relation: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for s in &anchor.stages {
        let sid = Some(s.id.as_str());
        if !project.objects.contains_key(&s.object_id) {
            out.invalid("stage_object_unknown", sid, s.object_id.clone());
        }
        if !project.objects.contains_key(&s.relation_target) {
            out.invalid("relation_target_unknown", sid, s.relation_target.clone());
        }
        if s.relation == "root"
            && (s.object_id != anchor.root_object || s.relation_target != anchor.root_object)
        {
            out.invalid("root_relation_on_other_object", sid, s.object_id.clone());
        }
        if s.relation != "root" && s.object_id == anchor.root_object {
            out.invalid("root_object_relation", sid, s.relation.clone());
        }
        let declared = (s.relation.as_str(), s.relation_target.as_str());
        match relation.get(s.object_id.as_str()) {
            Some(prior) if *prior != declared => out.invalid(
                "inconsistent_object_relation",
                sid,
                format!("{} declared {:?} then {:?}", s.object_id, prior, declared),
            ),
            _ => {
                relation.insert(s.object_id.as_str(), declared);
            }
        }
        for a in &s.advances {
            if !acceptance.contains(a.as_str()) {
                out.invalid("advance_acceptance_unknown", sid, a.clone());
            }
        }
        if s.outcome == "advanced" && s.advances.is_empty() {
            out.invalid(
                "advance_without_acceptance",
                sid,
                "outcome advanced but no acceptance item".into(),
            );
        }
        if off_root(s) {
            match &s.return_to {
                None => out.invalid("missing_return_condition", sid, s.object_id.clone()),
                Some(r) if !acceptance.contains(r.acceptance.as_str()) => {
                    out.invalid("return_acceptance_unknown", sid, r.acceptance.clone())
                }
                Some(r) if r.condition.trim().is_empty() => {
                    out.invalid("missing_return_condition", sid, s.object_id.clone())
                }
                _ => {}
            }
        }
        if let Some(a) = &s.approval
            && !approvals.contains(a.as_str())
        {
            out.invalid("approval_unknown", sid, a.clone());
        }
    }
    let rooted = |start: &str| -> bool {
        let mut seen = BTreeSet::new();
        let mut current = start;
        loop {
            if current == anchor.root_object {
                return true;
            }
            if !seen.insert(current) {
                return false;
            }
            match relation.get(current) {
                Some((kind, target)) if matches!(*kind, "component" | "model") => current = target,
                _ => return false,
            }
        }
    };
    for s in &anchor.stages {
        let sid = Some(s.id.as_str());
        match s.relation.as_str() {
            "component" | "model" if !rooted(&s.object_id) => out.invalid(
                "relation_chain_not_rooted",
                sid,
                format!(
                    "{} does not reach {} through component/model relations",
                    s.object_id, anchor.root_object
                ),
            ),
            "reference_case" | "external_case" if !rooted(&s.relation_target) => out.invalid(
                "case_target_not_rooted",
                sid,
                format!(
                    "{} must point at the root object, a component or a model of it",
                    s.relation_target
                ),
            ),
            _ => {}
        }
    }

    // Route reviews.
    let mut last_review: Option<usize> = None;
    for r in &anchor.reviews {
        match stage_index.get(r.after_stage.as_str()) {
            Some(i) => last_review = Some(last_review.map_or(*i, |l| l.max(*i))),
            None => out.invalid("review_stage_unknown", None, r.after_stage.clone()),
        }
    }

    // Streaks: off-root without approval, and stages without progress. A streak
    // stays drift until a route review is recorded after its last stage; a
    // permanent block would only reward rewriting the history.
    let off_flags: Vec<Option<bool>> = anchor
        .stages
        .iter()
        .map(|s| Some(off_root(s) && s.approval.is_none()))
        .collect();
    let idle_flags: Vec<Option<bool>> = anchor
        .stages
        .iter()
        .map(|s| match s.outcome.as_str() {
            // A pending stage has not finished; it neither extends nor ends a run.
            "pending" => None,
            "no_progress" => Some(true),
            _ => Some(s.advances.is_empty()),
        })
        .collect();
    for (code, flags, limit, what) in [
        (
            "off_root_streak",
            &off_flags,
            p.off_root_stage_limit,
            "consecutive reference/external stages without user approval",
        ),
        (
            "no_progress_streak",
            &idle_flags,
            p.no_progress_stage_limit,
            "consecutive stages advanced no acceptance item",
        ),
    ] {
        if limit == 0 {
            continue;
        }
        let mut run: Vec<usize> = Vec::new();
        let close = |run: &mut Vec<usize>, out: &mut Findings| {
            if run.len() as u64 >= limit {
                let last = *run.last().unwrap();
                if last_review.is_none_or(|r| r < last) {
                    let hit = run[limit as usize - 1];
                    out.drift(
                        code,
                        Some(anchor.stages[hit].id.as_str()),
                        format!(
                            "{} {what}; no route review after {}",
                            run.len(),
                            anchor.stages[last].id
                        ),
                    );
                }
            }
            run.clear();
        };
        for (i, flag) in flags.iter().enumerate() {
            match flag {
                Some(true) => run.push(i),
                Some(false) => close(&mut run, &mut out),
                None => {}
            }
        }
        close(&mut run, &mut out);
    }
    let since = anchor.stages.len() - last_review.map_or(0, |i| i + 1);
    if p.review_interval > 0 && since as u64 > p.review_interval {
        out.drift(
            "route_review_overdue",
            anchor.stages.last().map(|s| s.id.as_str()),
            format!("{since} stages since the last route review"),
        );
    }

    // Corrections must be taken up by a later stage.
    for u in &anchor.requests {
        let after = match &u.after_stage {
            None => None,
            Some(id) => match stage_index.get(id.as_str()) {
                Some(i) => Some(*i),
                None => {
                    out.invalid("request_stage_unknown", None, format!("{}:{id}", u.id));
                    continue;
                }
            },
        };
        if u.kind != "correction" {
            continue;
        }
        if u.affects.is_empty() {
            out.invalid("correction_without_acceptance", None, u.id.clone());
        }
        let first_later = after.map_or(0, |i| i + 1);
        for a in &u.affects {
            if !acceptance.contains(a.as_str()) {
                out.invalid(
                    "correction_acceptance_unknown",
                    None,
                    format!("{}:{a}", u.id),
                );
                continue;
            }
            let addressed = anchor.stages[first_later.min(anchor.stages.len())..]
                .iter()
                .any(|s| s.advances.iter().any(|x| x == a));
            if !addressed {
                out.drift(
                    "correction_not_addressed",
                    None,
                    format!("{} affects {a}; no later stage advances it", u.id),
                );
            }
        }
    }

    // Project objects that the route never anchors.
    let mentioned: BTreeSet<&str> = anchor
        .stages
        .iter()
        .flat_map(|s| [s.object_id.as_str(), s.relation_target.as_str()])
        .chain([anchor.root_object.as_str()])
        .collect();
    let unanchored: Vec<String> = project
        .objects
        .keys()
        .filter(|id| !mentioned.contains(id.as_str()))
        .cloned()
        .collect();
    for id in &unanchored {
        out.drift("unanchored_project_object", None, id.clone());
    }

    let status = if out.0.iter().any(|f| f.severity == "invalid") {
        "invalid"
    } else if out.0.is_empty() {
        "anchored"
    } else {
        "drift"
    };
    let report = RouteAuditReport {
        schema: REPORT_SCHEMA.into(),
        status: status.into(),
        project_id: project.project_id.clone(),
        project_sha256: project_sha256.into(),
        anchor_sha256: anchor_sha256.into(),
        stage_count: anchor.stages.len() as u64,
        open_acceptance: anchor
            .acceptance
            .iter()
            .filter(|a| a.status != "closed")
            .map(|a| a.id.clone())
            .collect(),
        unanchored_objects: unanchored,
        findings: out.0,
        scope: SCOPE.into(),
    };
    report
        .validate()
        .map_err(|f| anyhow::anyhow!("route_audit_report_invalid:{}", f.join(",")))?;
    Ok(report)
}

pub const ANCHOR2_SCHEMA: &str = "oev2.route-anchor/2";
pub const REPORT2_SCHEMA: &str = "oev2.route-audit/2";
const SCOPE2: &str = "Declared stage relations, acceptance closure against Project decisions and registered evidence, streak and review counts; \
does not prove that a declaration is true, the deliverable is acceptable, or engineering adoption.";

/// Route anchor /2: every /1 check plus acceptance closure against Project
/// decisions and registered evidence, reported as delivery scope.
pub fn audit2(
    project: &Project,
    anchor: &oe_contracts::RouteAnchor2,
    project_sha256: &str,
    anchor_sha256: &str,
) -> Result<oe_contracts::RouteAuditReport2> {
    let v1 = RouteAnchor {
        schema: ANCHOR_SCHEMA.into(),
        project_id: anchor.project_id.clone(),
        root_object: anchor.root_object.clone(),
        deliverable: anchor.deliverable.clone(),
        requests: anchor.requests.clone(),
        functions: anchor.functions.clone(),
        acceptance: anchor
            .acceptance
            .iter()
            .map(|a| oe_contracts::RouteAcceptance {
                id: a.id.clone(),
                statement: a.statement.clone(),
                functions: a.functions.clone(),
                status: a.status.clone(),
            })
            .collect(),
        stages: anchor.stages.clone(),
        reviews: anchor.reviews.clone(),
        policy: anchor.policy.clone(),
    };
    let base = audit(project, &v1, project_sha256, anchor_sha256)?;
    let mut out = Findings(base.findings);
    if anchor.schema != ANCHOR2_SCHEMA {
        out.invalid("anchor_schema", None, anchor.schema.clone());
    }
    for acc in &anchor.acceptance {
        if let Err(f) = acc.validate() {
            for x in f {
                out.invalid("anchor_contract_invalid", None, x);
            }
        }
    }
    let decisions: BTreeMap<&str, &super::Decision> = project
        .decisions
        .iter()
        .map(|d| (d.id.as_str(), d))
        .collect();
    let (mut verified, mut open, mut inconsistent) = (Vec::new(), Vec::new(), Vec::new());
    for acc in &anchor.acceptance {
        let mut problems = Vec::new();
        for d in &acc.decisions {
            match decisions.get(d.as_str()) {
                None => problems.push(format!("decision_unknown:{d}")),
                Some(dec) if acc.status == "closed" => {
                    if dec.conclusion != "supported" {
                        problems.push(format!("decision_not_supported:{d}"));
                    }
                    if project.objects.get(&dec.object_id) != Some(&dec.object_revision) {
                        problems.push(format!("decision_object_stale:{d}"));
                    }
                    if !project.evidence.values().any(|s| s == &dec.source_sha256) {
                        problems.push(format!("decision_source_missing:{d}"));
                    }
                    if dec.basis.is_empty()
                        || dec.basis.iter().any(|b| !project.evidence.contains_key(b))
                    {
                        problems.push(format!("decision_basis_not_registered:{d}"));
                    }
                }
                Some(_) => {}
            }
        }
        for e in &acc.evidence {
            if !project.evidence.contains_key(e) {
                problems.push(format!("evidence_unregistered:{e}"));
            }
            if acc.status == "closed" && super::memory::is_memory_evidence(e) {
                problems.push(format!("memory_evidence_not_decisive:{e}"));
            }
        }
        if acc.status == "closed" {
            if acc.decisions.is_empty() {
                problems.push("closed_without_decisions".into());
            }
            if problems.is_empty() {
                verified.push(acc.id.clone());
            } else {
                inconsistent.push(acc.id.clone());
                for p in problems {
                    out.invalid("acceptance_closure", None, format!("{}:{p}", acc.id));
                }
            }
        } else {
            for p in problems {
                out.invalid("acceptance_reference", None, format!("{}:{p}", acc.id));
            }
            open.push(acc.id.clone());
        }
    }
    let status = if out.0.iter().any(|f| f.severity == "invalid") {
        "invalid"
    } else if out.0.is_empty() {
        "anchored"
    } else {
        "drift"
    };
    let report = oe_contracts::RouteAuditReport2 {
        schema: REPORT2_SCHEMA.into(),
        status: status.into(),
        project_id: base.project_id,
        project_sha256: base.project_sha256,
        anchor_sha256: base.anchor_sha256,
        stage_count: base.stage_count,
        open_acceptance: base.open_acceptance,
        unanchored_objects: base.unanchored_objects,
        findings: out.0,
        delivery_verified: verified,
        delivery_open: open,
        delivery_inconsistent: inconsistent,
        scope: SCOPE2.into(),
    };
    report
        .validate()
        .map_err(|f| anyhow::anyhow!("route_audit_report2_invalid:{}", f.join(",")))?;
    Ok(report)
}
