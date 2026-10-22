//! Core parameter grammar is checked before target selection, including empty targets.
use super::{G, RDF, SH, XSD, count, datatype_valid};
use anyhow::{Result, ensure};
use oxigraph::model::Term;

pub(super) fn parameters(shapes: &G) -> Result<()> {
    for statements in shapes.by_subject.values() {
        let values = |name: &str| -> Vec<&Term> {
            statements
                .iter()
                .filter(|(p, _)| p == &format!("{SH}{name}"))
                .map(|(_, value)| value)
                .collect()
        };
        let property = !values("path").is_empty();
        for name in [
            "path",
            "datatype",
            "nodeKind",
            "minCount",
            "maxCount",
            "minExclusive",
            "minInclusive",
            "maxExclusive",
            "maxInclusive",
            "minLength",
            "maxLength",
            "pattern",
            "flags",
            "languageIn",
            "uniqueLang",
            "closed",
            "ignoredProperties",
            "qualifiedValueShape",
            "qualifiedMinCount",
            "qualifiedMaxCount",
            "qualifiedValueShapesDisjoint",
            "deactivated",
            "severity",
        ] {
            ensure!(
                values(name).len() <= 1,
                "shacl_parameter_cardinality:{name}"
            );
        }
        let types: Vec<_> = statements
            .iter()
            .filter(|(p, _)| p == &format!("{RDF}type"))
            .map(|(_, value)| value.to_string())
            .collect();
        ensure!(
            !types.contains(&format!("<{SH}NodeShape>")) || !property,
            "shacl_node_shape_has_path"
        );
        ensure!(
            !types.contains(&format!("<{SH}PropertyShape>")) || property,
            "shacl_property_shape_path_missing"
        );
        for name in [
            "minCount",
            "maxCount",
            "uniqueLang",
            "equals",
            "disjoint",
            "lessThan",
            "lessThanOrEquals",
            "qualifiedValueShape",
            "qualifiedMinCount",
            "qualifiedMaxCount",
            "qualifiedValueShapesDisjoint",
        ] {
            ensure!(
                property || values(name).is_empty(),
                "shacl_parameter_requires_property_shape:{name}"
            );
        }
        for name in [
            "closed",
            "uniqueLang",
            "deactivated",
            "qualifiedValueShapesDisjoint",
        ] {
            for value in values(name) {
                ensure!(
                    datatype_valid(value, &format!("{XSD}boolean")),
                    "shacl_boolean_parameter_invalid:{name}"
                );
            }
        }
        for name in [
            "minCount",
            "maxCount",
            "minLength",
            "maxLength",
            "qualifiedMinCount",
            "qualifiedMaxCount",
        ] {
            for value in values(name) {
                ensure!(
                    datatype_valid(value, &format!("{XSD}integer")),
                    "shacl_integer_parameter_invalid:{name}"
                );
                count(value)?;
            }
        }
        for name in [
            "class",
            "datatype",
            "equals",
            "disjoint",
            "lessThan",
            "lessThanOrEquals",
            "targetClass",
            "targetSubjectsOf",
            "targetObjectsOf",
            "severity",
        ] {
            for value in values(name) {
                ensure!(
                    matches!(value, Term::NamedNode(_)),
                    "shacl_iri_parameter_invalid:{name}"
                );
            }
        }
        for name in ["pattern", "flags"] {
            for value in values(name) {
                ensure!(
                    datatype_valid(value, &format!("{XSD}string")),
                    "shacl_string_parameter_invalid:{name}"
                );
            }
        }
        for value in values("message") {
            ensure!(
                matches!(value, Term::Literal(l) if l.datatype().as_str() == format!("{XSD}string") || l.datatype().as_str() == format!("{RDF}langString")),
                "shacl_message_parameter_invalid"
            );
        }
        for value in values("nodeKind") {
            ensure!(
                [
                    "BlankNode",
                    "IRI",
                    "Literal",
                    "BlankNodeOrIRI",
                    "BlankNodeOrLiteral",
                    "IRIOrLiteral"
                ]
                .iter()
                .any(|kind| value.to_string() == format!("<{SH}{kind}>")),
                "shacl_nodekind_invalid"
            );
        }
        for name in ["not", "node", "property", "qualifiedValueShape"] {
            for value in values(name) {
                ensure!(
                    matches!(value, Term::NamedNode(_) | Term::BlankNode(_)),
                    "shacl_shape_parameter_invalid:{name}"
                );
                if name == "property" {
                    ensure!(
                        shapes.one(value, "path")?.is_some(),
                        "shacl_referenced_property_shape_path_missing"
                    );
                }
                if name == "node" {
                    ensure!(
                        shapes.one(value, "path")?.is_none(),
                        "shacl_node_parameter_is_property_shape"
                    );
                }
            }
        }
        for name in ["in", "and", "or", "xone", "languageIn", "ignoredProperties"] {
            for value in values(name) {
                for member in shapes.list(value)? {
                    if matches!(name, "and" | "or" | "xone") {
                        ensure!(
                            matches!(member, Term::NamedNode(_) | Term::BlankNode(_)),
                            "shacl_logical_shape_invalid:{name}"
                        );
                    }
                    if name == "ignoredProperties" {
                        ensure!(
                            matches!(member, Term::NamedNode(_)),
                            "shacl_ignored_property_invalid"
                        );
                    }
                    if name == "languageIn" {
                        ensure!(
                            datatype_valid(&member, &format!("{XSD}string")),
                            "shacl_language_range_invalid"
                        );
                    }
                }
            }
        }
        ensure!(
            !values("qualifiedValueShape").is_empty()
                || (values("qualifiedMinCount").is_empty()
                    && values("qualifiedMaxCount").is_empty()
                    && values("qualifiedValueShapesDisjoint").is_empty()),
            "shacl_qualified_shape_missing"
        );
    }
    Ok(())
}
