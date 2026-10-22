//! Remaining SHACL Core constraints evaluated over the exact distinct value set.
use super::{G, SH, lexical};
use anyhow::Result;
use oxigraph::model::Term;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Finding {
    pub component: &'static str,
    pub value: Option<Term>,
    pub path: Option<Term>,
    pub message: &'static str,
}
fn boolean(shapes: &G, shape: &Term, name: &str) -> Result<bool> {
    Ok(shapes
        .one(shape, name)?
        .map(|v| lexical(&v).map(|s| matches!(s, "true" | "1")))
        .transpose()?
        .unwrap_or(false))
}
pub(super) fn evaluate(
    shape: &Term,
    focus: &Term,
    values: &[Term],
    shapes: &G,
    data: &G,
    conforms: &mut dyn FnMut(&Term, &Term) -> Result<bool>,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let mut emit = |component, value: Option<Term>, path, message| {
        findings.push(Finding {
            component,
            value,
            path,
            message,
        })
    };
    for name in ["equals", "disjoint"] {
        for predicate in shapes.sh(shape, name) {
            let Term::NamedNode(predicate) = predicate else {
                anyhow::bail!("shacl_pair_parameter_must_be_iri");
            };
            let others = data.objects(focus, predicate.as_str());
            for value in values {
                if (name == "equals" && !others.contains(value))
                    || (name == "disjoint" && others.contains(value))
                {
                    emit(
                        name,
                        Some(value.clone()),
                        None,
                        "property pair value sets differ",
                    );
                }
            }
            if name == "equals" {
                for other in others {
                    if !values.contains(&other) {
                        emit(
                            name,
                            Some(other),
                            None,
                            "comparison property has an unmatched value",
                        );
                    }
                }
            }
        }
    }
    if let Some(head) = shapes.one(shape, "languageIn")? {
        let ranges = shapes
            .list(&head)?
            .iter()
            .map(|range| lexical(range).map(str::to_ascii_lowercase))
            .collect::<Result<Vec<_>>>()?;
        for value in values {
            let tag = if let Term::Literal(literal) = value {
                literal.language().unwrap_or("")
            } else {
                ""
            }
            .to_ascii_lowercase();
            let matched = !tag.is_empty()
                && ranges.iter().any(|range| {
                    range == "*"
                        || range == &tag
                        || tag
                            .strip_prefix(range)
                            .is_some_and(|suffix| suffix.starts_with('-'))
                });
            if !matched {
                emit(
                    "languageIn",
                    Some(value.clone()),
                    None,
                    "language tag is outside the declared basic ranges",
                );
            }
        }
    }
    if boolean(shapes, shape, "uniqueLang")? {
        let mut counts = BTreeMap::<String, usize>::new();
        for value in values {
            if let Term::Literal(literal) = value
                && let Some(tag) = literal.language()
            {
                *counts.entry(tag.to_ascii_lowercase()).or_default() += 1;
            }
        }
        for count in counts.values() {
            if *count > 1 {
                emit(
                    "uniqueLang",
                    None,
                    None,
                    "multiple distinct values use the same language tag",
                );
            }
        }
    }
    if boolean(shapes, shape, "closed")? {
        let mut allowed = BTreeSet::new();
        for property in shapes.sh(shape, "property") {
            if let Some(Term::NamedNode(predicate)) = shapes.one(&property, "path")? {
                allowed.insert(predicate.as_str().to_owned());
            }
        }
        if let Some(head) = shapes.one(shape, "ignoredProperties")? {
            for predicate in shapes.list(&head)? {
                let Term::NamedNode(predicate) = predicate else {
                    anyhow::bail!("shacl_ignored_property_invalid");
                };
                allowed.insert(predicate.as_str().into());
            }
        }
        for value in values {
            for (predicate, object) in data
                .by_subject
                .get(&value.to_string())
                .into_iter()
                .flatten()
            {
                if !allowed.contains(predicate) {
                    emit(
                        "closed",
                        Some(object.clone()),
                        Some(Term::NamedNode(oxigraph::model::NamedNode::new(predicate)?)),
                        "triple uses a predicate outside this closed shape",
                    );
                }
            }
        }
    }
    if let Some(qualified) = shapes.one(shape, "qualifiedValueShape")? {
        let mut siblings = BTreeMap::new();
        if boolean(shapes, shape, "qualifiedValueShapesDisjoint")? {
            for parent in shapes.by_subject.values() {
                if parent
                    .iter()
                    .any(|(p, object)| p == &format!("{SH}property") && object == shape)
                {
                    for (p, property) in parent {
                        if p == &format!("{SH}property") {
                            for sibling in shapes.sh(property, "qualifiedValueShape") {
                                if sibling != qualified {
                                    siblings.insert(sibling.to_string(), sibling);
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut count = 0;
        for value in values {
            let matched = conforms(&qualified, value)?;
            let mut sibling_match = false;
            for sibling in siblings.values() {
                // Keep every sibling's failures, even when another shape matched.
                sibling_match |= conforms(sibling, value)?;
            }
            if matched && !sibling_match {
                count += 1;
            }
        }
        for name in ["qualifiedMinCount", "qualifiedMaxCount"] {
            if let Some(bound) = shapes.one(shape, name)? {
                let bound = super::count(&bound)?;
                if (name == "qualifiedMinCount" && count < bound)
                    || (name == "qualifiedMaxCount" && count > bound)
                {
                    emit(
                        name,
                        None,
                        None,
                        "qualified distinct value count is outside the declared bound",
                    );
                }
            }
        }
    }
    Ok(findings)
}
