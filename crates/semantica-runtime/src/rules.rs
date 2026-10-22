//! Fixed-point unary Horn subset actually present in the v1 package corpus.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn atom(raw: &str) -> Result<(String, String)> {
    let raw = raw.trim();
    let (p, a) = raw
        .split_once('(')
        .ok_or_else(|| anyhow::anyhow!("rule_atom_invalid"))?;
    let a = a
        .strip_suffix(')')
        .ok_or_else(|| anyhow::anyhow!("rule_atom_invalid"))?;
    ensure!(
        !p.is_empty()
            && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !a.is_empty()
            && !a.contains(['(', ')', ',', ' ']),
        "rule_atom_unsupported"
    );
    Ok((p.into(), a.into()))
}
pub fn forward_chain(facts: &str, rules: &str) -> Result<Value> {
    ensure!(
        facts.len() + rules.len() <= crate::MAX_BYTES,
        "rules_byte_budget"
    );
    let mut known = BTreeSet::new();
    let mut origins: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in facts
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let a = atom(f)?;
        ensure!(!a.1.starts_with('?'), "fact_variable_unsupported");
        known.insert(a);
        origins.insert(f.into(), vec!["input_fact".into()]);
    }
    let initial = known.clone();
    let mut parsed = Vec::new();
    for r in rules
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let raw = r
            .strip_prefix("IF ")
            .ok_or_else(|| anyhow::anyhow!("rule_syntax_unsupported"))?;
        let (before, after) = raw
            .split_once(" THEN ")
            .ok_or_else(|| anyhow::anyhow!("rule_syntax_unsupported"))?;
        let premises = before
            .split(" AND ")
            .map(atom)
            .collect::<Result<Vec<_>>>()?;
        let conclusion = atom(after)?;
        ensure!(
            !premises.is_empty()
                && premises.len() <= 8
                && premises.iter().all(|a| a.1 == conclusion.1)
                && conclusion.1.starts_with('?'),
            "rule_variable_scope_unsupported"
        );
        parsed.push((premises, conclusion, r.to_owned()));
    }
    ensure!(
        parsed.len() <= 1000 && known.len() <= 10_000,
        "rule_count_budget"
    );
    for iteration in 0..1000 {
        let mut produced = Vec::new();
        for (premises, conclusion, source) in &parsed {
            for (p, a) in &known {
                if p != &premises[0].0 {
                    continue;
                }
                if premises
                    .iter()
                    .all(|(p, _)| known.contains(&(p.clone(), a.clone())))
                {
                    let f = (conclusion.0.clone(), a.clone());
                    if !known.contains(&f) {
                        produced.push((
                            f,
                            source.clone(),
                            premises
                                .iter()
                                .map(|(p, _)| format!("{p}({a})"))
                                .collect::<Vec<_>>(),
                        ));
                    }
                }
            }
        }
        if produced.is_empty() {
            let conclusions: Vec<_> = known
                .difference(&initial)
                .map(|(p, a)| format!("{p}({a})"))
                .collect();
            return Ok(
                json!({"conclusions":conclusions,"provenance":origins,"iterations":iteration,"scope":"unary positive Horn subset"}),
            );
        }
        for (f, rule, from) in produced {
            origins
                .entry(format!("{}({})", f.0, f.1))
                .or_insert_with(|| {
                    let mut out = vec![rule];
                    out.extend(from);
                    out
                });
            known.insert(f);
        }
        ensure!(known.len() <= 10_000, "rule_fact_budget");
    }
    anyhow::bail!("rule_fixed_point_budget_exceeded")
}
