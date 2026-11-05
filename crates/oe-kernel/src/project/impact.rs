//! Typed change propagation. Legacy names are adapted once, never reclassified
//! while walking the graph. Ambiguous legacy edges retain a visible diagnosis.
use super::{Project, validate};
use anyhow::{Result, ensure};
use oe_contracts::ProjectEntityRef;
use serde_json::{Value, json};
use std::collections::BTreeSet;

type Key = (String, String);

pub fn entity(kind: &str, id: &str) -> ProjectEntityRef {
    ProjectEntityRef {
        kind: kind.into(),
        id: id.into(),
    }
}
fn key(kind: &str, id: &str) -> Key {
    (kind.into(), id.into())
}

/// Historical inputs have no kind. Keep all possible readings, explicitly
/// reported as ambiguous; exact callers should use impact_typed instead.
fn resolve(projects: &[&Project], id: &str, ambiguities: &mut BTreeSet<String>) -> BTreeSet<Key> {
    let mut out = BTreeSet::new();
    for p in projects {
        for (kind, exists) in [
            ("object", p.objects.contains_key(id)),
            ("condition", p.conditions.contains_key(id)),
            ("evidence", p.evidence.contains_key(id)),
            ("decision", p.decisions.iter().any(|d| d.id == id)),
            ("source_digest", p.evidence.values().any(|s| s == id)),
            ("purpose", matches!(id, "purpose" | "project:purpose")),
        ] {
            if exists {
                out.insert(key(
                    kind,
                    if kind == "purpose" {
                        "project:purpose"
                    } else {
                        id
                    },
                ));
            }
        }
    }
    if out.is_empty() {
        out.insert(key("dependency", id));
    }
    if out.len() > 1 {
        ambiguities.insert(id.into());
    }
    out
}

fn closure(
    projects: &[&Project],
    changes: BTreeSet<Key>,
    mut ambiguities: BTreeSet<String>,
) -> Result<Value> {
    for p in projects {
        validate(p)?;
    }
    let mut edges = Vec::new();
    for p in projects {
        for (target, deps) in &p.dependencies {
            let targets = resolve(projects, target, &mut ambiguities);
            for dep in deps {
                let inputs = resolve(projects, dep, &mut ambiguities);
                for target in &targets {
                    edges.push((target.clone(), inputs.clone()));
                }
            }
        }
        for d in &p.decisions {
            let mut inputs = BTreeSet::from([
                key("object", &d.object_id),
                key("condition", &d.condition_scope),
                key("source_digest", &d.source_sha256),
            ]);
            for id in &d.basis {
                inputs.extend(resolve(projects, id, &mut ambiguities));
            }
            edges.push((key("decision", &d.id), inputs));
        }
        // The new bound relations are also dependencies in this same Project;
        // changing a child structure or its conditions must reopen its consumer.
        for raw in p.conditions.values() {
            let Ok(value) = oe_core::json::strict_json(raw.as_bytes()) else {
                continue;
            };
            if !value["schema"]
                .as_str()
                .is_some_and(|s| s.starts_with("oev2.engineering-basis/"))
            {
                continue;
            }
            for a in value["assertions"].as_array().into_iter().flatten() {
                let mut inputs = BTreeSet::new();
                let mut targets: BTreeSet<_> = a["affected_decisions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|s| key("decision", s))
                    .collect();
                let mut bind = |o: &oe_contracts::ObjectIdentity| {
                    inputs.extend([
                        key("object", &o.object_id),
                        key("condition", &o.condition_scope),
                        key("source_digest", &o.source_sha256),
                    ]);
                    if let Some(raw) = p.conditions.get(&o.condition_scope) {
                        if let Ok(basis) = oe_core::json::strict_json(raw.as_bytes()) {
                            for item in basis["facts"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .chain(basis["assertions"].as_array().into_iter().flatten())
                            {
                                for id in item["source_refs"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                {
                                    inputs.insert(key("evidence", id));
                                }
                            }
                        }
                    }
                };
                if a["kind"] == "function_continuity" {
                    if let Ok(r) = serde_json::from_value::<
                        oe_contracts::EngineeringFunctionRealization,
                    >(a["requirements"].clone())
                    {
                        if let Some(o) = r.realization {
                            bind(&o);
                        }
                    }
                } else if a["kind"] == "claim_composition" {
                    if let Ok(r) = serde_json::from_value::<oe_contracts::EngineeringClaimComposition>(
                        a["requirements"].clone(),
                    ) {
                        targets.insert(key("decision", &r.decision_id));
                        for reference in r
                            .all_of
                            .iter()
                            .chain(&r.interfaces)
                            .chain(r.any_of.iter().flat_map(|g| &g.alternatives))
                        {
                            bind(&reference.object);
                        }
                    }
                }
                for target in targets {
                    if !inputs.is_empty() {
                        edges.push((target, inputs.clone()));
                    }
                }
            }
        }
    }
    let mut affected = changes;
    loop {
        let before = affected.len();
        for (target, inputs) in &edges {
            if !inputs.is_disjoint(&affected) {
                affected.insert(target.clone());
            }
        }
        if affected.len() == before {
            break;
        }
    }
    let p = projects.last().expect("current project");
    let entities: Vec<_> = affected.iter().map(|(kind, id)| entity(kind, id)).collect();
    let names: BTreeSet<_> = affected.iter().map(|(_, id)| id).collect();
    let decisions: Vec<_> = p.decisions.iter().filter(|d| affected.contains(&key("decision", &d.id)))
        .map(|d| json!({"decision_id":d.id,"prior_conclusion":d.conclusion,
            "current_status":"requires_review","reason":"linked object/source/condition changed; previous history retained"})).collect();
    Ok(
        json!({"schema":"oev2.project-impact/2","project_id":p.project_id,
        "affected":names,"affected_entities":entities,"decisions":decisions,
        "legacy_ambiguities":ambiguities,"legacy_adapter":"ambiguous untyped edges require explicit review; affected is display-only",
        "unknown_dependencies":"unlisted links remain unverified"}),
    )
}

pub fn impact_typed(p: &Project, changes: &[ProjectEntityRef]) -> Result<Value> {
    let mut keys = BTreeSet::new();
    for c in changes {
        c.validate()
            .map_err(|e| anyhow::anyhow!("project_change_invalid:{e:?}"))?;
        ensure!(!c.id.trim().is_empty(), "project_change_identity_missing");
        ensure!(
            c.kind != "purpose" || c.id == "project:purpose",
            "project_purpose_identity_invalid"
        );
        keys.insert((c.kind.clone(), c.id.clone()));
    }
    closure(&[p], keys, BTreeSet::new())
}

pub fn impact(p: &Project, changes: &BTreeSet<String>) -> Result<Value> {
    let mut ambiguities = BTreeSet::new();
    let changes = changes
        .iter()
        .flat_map(|id| resolve(&[p], id, &mut ambiguities))
        .collect();
    closure(&[p], changes, ambiguities)
}

pub fn decision_impact(p: &Project, ids: &BTreeSet<String>) -> Result<Value> {
    impact_typed(
        p,
        &ids.iter()
            .map(|id| entity("decision", id))
            .collect::<Vec<_>>(),
    )
}

pub(super) fn reconcile_impact(previous: &Project, current: &Project) -> Result<Value> {
    let projects = [previous, current];
    let mut changes = BTreeSet::new();
    let mut ambiguities = BTreeSet::new();
    if previous.purpose != current.purpose {
        for id in super::purpose_changes(&projects) {
            if matches!(id.as_str(), "purpose" | "project:purpose") {
                changes.insert(key("purpose", "project:purpose"));
            } else {
                changes.insert(key("decision", &id));
            }
        }
    }
    for (kind, before, after) in [
        ("object", &previous.objects, &current.objects),
        ("condition", &previous.conditions, &current.conditions),
        ("evidence", &previous.evidence, &current.evidence),
    ] {
        for id in before.keys().chain(after.keys()) {
            if before.get(id) != after.get(id) {
                changes.insert(key(kind, id));
                if kind == "evidence" {
                    for digest in before.get(id).into_iter().chain(after.get(id)) {
                        changes.insert(key("source_digest", digest));
                    }
                }
            }
        }
    }
    for old in &previous.decisions {
        if current
            .decisions
            .iter()
            .find(|d| d.id == old.id)
            .is_none_or(|d| serde_json::to_value(d).ok() != serde_json::to_value(old).ok())
        {
            changes.insert(key("decision", &old.id));
        }
    }
    for target in previous
        .dependencies
        .keys()
        .chain(current.dependencies.keys())
    {
        if previous.dependencies.get(target) != current.dependencies.get(target) {
            changes.extend(resolve(&projects, target, &mut ambiguities));
        }
    }
    closure(&projects, changes, ambiguities)
}
