//! Recognition coverage of the drafting concept catalogue.
//!
//! `knowledge/recognizer-coverage.toml` is the audited table of every rule the recognizer emits:
//! the expression kinds it produces and the catalogue concepts its expressions are instances of,
//! with a coverage verdict per concept (full, or partial with what is missing). At the end of
//! recognition every expression receives its concept ids from three places: the table entry of
//! its rule (and of the line-piece rules grouped into it), the recognition cards named in its
//! evidence (a card naming one concept names it; cards naming several name each concept they
//! found as `concept:<id>` evidence), and nothing else. Drawing-wide conventions applied to
//! members and line styles (coincident-line precedence, the layer table) are listed apart.
//! A concept's status in the catalogue
//! (`card`, `rule`, `partial`, `none`) follows from this table and the recognition cards.
use super::*;
use oe_contracts::MdRecognizerCoverage;
use std::sync::OnceLock;

const SOURCE: &str = include_str!("../knowledge/recognizer-coverage.toml");
const SCHEMA: &str = "oev2.drawing-recognizer-coverage/1";

/// The parsed and checked coverage table (parsed once per process).
pub fn recognizer_coverage() -> Result<&'static MdRecognizerCoverage> {
    static K: OnceLock<std::result::Result<MdRecognizerCoverage, String>> = OnceLock::new();
    K.get_or_init(|| {
        let k: MdRecognizerCoverage =
            toml::from_str(SOURCE).map_err(|e| format!("recognizer_coverage_parse: {e}"))?;
        let mut bad: Vec<String> = k.validate().err().unwrap_or_default();
        if k.schema != SCHEMA || k.profile != PROFILE {
            bad.push("schema_or_profile".into());
        }
        let cards = representation::representation_knowledge().map_err(|e| e.to_string())?;
        let mut rules = BTreeSet::new();
        let mut named: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for r in &k.rules {
            bad.extend(r.validate().err().unwrap_or_default());
            if !rules.insert(r.rule.clone()) {
                bad.push(format!("duplicate_rule:{}", r.rule));
            }
            for kc in &r.concepts {
                if !kc.kind.is_empty() && !r.kinds.contains(&kc.kind) {
                    bad.push(format!("concept_kind_not_emitted:{}:{}", r.rule, kc.kind));
                }
                for id in &kc.ids {
                    named.entry(id.clone()).or_default().insert(r.rule.clone());
                }
            }
        }
        for c in &cards.cards {
            for id in &c.concepts {
                named
                    .entry(id.clone())
                    .or_default()
                    .insert(format!("card:{}", c.id));
            }
        }
        for c in &k.conventions {
            bad.extend(c.validate().err().unwrap_or_default());
            for id in &c.ids {
                named
                    .entry(id.clone())
                    .or_default()
                    .insert(format!("convention:{}", c.name));
            }
        }
        let mut seen = BTreeSet::new();
        for c in &k.concepts {
            bad.extend(c.validate().err().unwrap_or_default());
            if !seen.insert(c.id.clone()) {
                bad.push(format!("duplicate_concept:{}", c.id));
            }
            let by: BTreeSet<String> = c.by.iter().cloned().collect();
            if named.get(&c.id) != Some(&by) {
                bad.push(format!("coverage_by_mismatch:{}", c.id));
            }
            if c.level == "partial" && c.missing.as_deref().is_none_or(|m| m.trim().is_empty()) {
                bad.push(format!("partial_without_missing:{}", c.id));
            }
        }
        for id in named.keys() {
            if !seen.contains(id) {
                bad.push(format!("named_concept_without_verdict:{id}"));
            }
        }
        if bad.is_empty() {
            Ok(k)
        } else {
            Err(format!("recognizer_coverage_contract: {}", bad.join(",")))
        }
    })
    .as_ref()
    .map_err(|e| anyhow::anyhow!(e.clone()))
}

/// Status of a concept: `card` (named by a recognition card, coverage full), `rule` (named only
/// by recognizer rules, coverage full), `partial`, or `none`.
pub fn concept_status(id: &str, named_by_card: bool) -> Result<&'static str> {
    let k = recognizer_coverage()?;
    Ok(match k.concepts.iter().find(|c| c.id == id) {
        None if named_by_card => anyhow::bail!("card_concept_without_verdict:{id}"),
        None => "none",
        Some(c) if c.level == "partial" => "partial",
        Some(_) if named_by_card => "card",
        Some(_) => "rule",
    })
}

/// Give every expression its concept ids (see the module comment) and remove the
/// `concept:` evidence entries that carried them.
pub(super) fn assign(b: &mut Builder) -> Result<()> {
    let k = recognizer_coverage()?;
    let cards = representation::representation_knowledge()?;
    let catalogue: BTreeSet<&str> = representation::drawing_concepts()?
        .iter()
        .flat_map(|g| g.concepts.iter().map(|c| c.id.as_str()))
        .collect();
    let rule = |name: &str| k.rules.iter().find(|r| r.rule == name);
    for e in b.exprs.iter_mut() {
        let mut ids: BTreeSet<String> = BTreeSet::new();
        let mut keys: Vec<String> = vec![e.rule.clone()];
        // line pieces grouped by view keep their piece rule as `rule:<name>` evidence
        keys.extend(
            e.evidence_ids
                .iter()
                .filter_map(|x| x.strip_prefix("rule:"))
                .map(|x| format!("piece:{x}")),
        );
        for key in &keys {
            let Some(r) = rule(key) else {
                if key == &e.rule {
                    anyhow::bail!("rule_without_coverage:{key}");
                }
                continue;
            };
            for kc in &r.concepts {
                if kc.kind.is_empty() || kc.kind == e.kind {
                    ids.extend(kc.ids.iter().cloned());
                }
            }
        }
        for x in &e.evidence_ids {
            // a qualifier card that names one concept names it on every expression it qualifies;
            // cards that name several push the concept of the variant they found
            if let Some(cid) = x.strip_prefix("card:")
                && let Some(c) = cards.cards.iter().find(|c| c.id == cid)
                && c.concepts.len() == 1
                && !e.rule.starts_with(&format!("card:{cid}"))
            {
                ids.extend(c.concepts.iter().cloned());
            }
            if let Some(id) = x.strip_prefix("concept:") {
                ids.insert(id.to_string());
            }
        }
        // concepts carried as evidence must be listed by a card the expression names
        let named_cards: Vec<&str> = e
            .evidence_ids
            .iter()
            .filter_map(|x| x.strip_prefix("card:"))
            .chain(
                e.rule
                    .strip_prefix("card:")
                    .map(|r| r.split('/').next().unwrap_or(r)),
            )
            .collect();
        for x in e
            .evidence_ids
            .iter()
            .filter_map(|x| x.strip_prefix("concept:"))
        {
            ensure!(
                named_cards.iter().any(|c| cards
                    .cards
                    .iter()
                    .any(|k| k.id == *c && k.concepts.iter().any(|y| y == x))),
                "concept_not_named_by_card:{x}:{}",
                e.id
            );
        }
        // card knowledge of concepts that exclude each other on one expression
        for l in cards
            .cards
            .iter()
            .flat_map(|c| c.lexicon.iter().flatten())
            .filter(|l| l.entry == "excludes")
        {
            if ids.contains(&l.concept) {
                for x in &l.values {
                    ids.remove(x);
                }
            }
        }
        for id in &ids {
            ensure!(
                catalogue.contains(id.as_str()),
                "concept_not_in_catalogue:{id}"
            );
        }
        e.evidence_ids.retain(|x| !x.starts_with("concept:"));
        e.concept_ids = ids.into_iter().collect();
    }
    Ok(())
}
