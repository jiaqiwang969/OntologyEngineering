//! Explicit graph identity mapping; no inferred prefixes or object renaming.
use anyhow::{Result, ensure};
use oe_contracts::RdfObjectFocus;
use oxigraph::model::{Dataset, Literal, NamedNode, NamedOrBlankNodeRef, TermRef};
pub(crate) fn resolve(
    object_id: &str,
    mapping: Option<&RdfObjectFocus>,
    data: &Dataset,
) -> Result<NamedNode> {
    let Some(mapping) = mapping else {
        return Ok(NamedNode::new(object_id)?);
    };
    mapping
        .validate()
        .map_err(|_| anyhow::anyhow!("rdf_object_focus_contract_invalid"))?;
    ensure!(
        mapping.schema == "oev2.rdf-object-focus/1" && mapping.object_id == object_id,
        "rdf_object_focus_identity_mismatch"
    );
    let focus = NamedNode::new(&mapping.focus_iri)?;
    let literal = Literal::new_simple_literal(object_id);
    let predicates: Vec<_> = data
        .iter()
        .filter(|q| q.predicate.as_str() == "urn:oev2:project-context:objectId")
        .collect();
    let matches: Vec<_> = predicates
        .iter()
        .filter(|q| q.object == TermRef::Literal(literal.as_ref()))
        .collect();
    ensure!(
        matches.len() == 1 && matches[0].subject == NamedOrBlankNodeRef::NamedNode(focus.as_ref()),
        "rdf_object_focus_unique_declaration_unproven"
    );
    ensure!(
        predicates
            .iter()
            .filter(|q| q.subject == NamedOrBlankNodeRef::NamedNode(focus.as_ref()))
            .count()
            == 1,
        "rdf_object_focus_multiple_object_ids"
    );
    Ok(focus)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mapping() -> RdfObjectFocus {
        RdfObjectFocus {
            schema: "oev2.rdf-object-focus/1".into(),
            object_id: "shaft-A".into(),
            focus_iri: "urn:test:shaft".into(),
        }
    }
    #[test]
    fn explicit_mapping_requires_exact_unique_declaration() {
        let m = mapping();
        let good = b"<urn:test:shaft> <urn:oev2:project-context:objectId> \"shaft-A\" .";
        assert_eq!(
            resolve("shaft-A", Some(&m), &crate::graph::load(good).unwrap())
                .unwrap()
                .as_str(),
            "urn:test:shaft"
        );
        assert!(resolve("shaft-A", None, &crate::graph::load(good).unwrap()).is_err());
        assert_eq!(
            resolve("urn:legacy:shaft", None, &Dataset::new())
                .unwrap()
                .as_str(),
            "urn:legacy:shaft"
        );
        for bad in [
            "",
            "<urn:other> <urn:oev2:project-context:objectId> \"shaft-A\" .",
            "<urn:test:shaft> <urn:oev2:project-context:objectId> \"other\" .",
            "<urn:test:shaft> <urn:oev2:project-context:objectId> \"shaft-A\", \"other\" .",
            "<urn:test:shaft> <urn:oev2:project-context:objectId> \"shaft-A\" . <urn:other> <urn:oev2:project-context:objectId> \"shaft-A\" .",
        ] {
            assert!(
                resolve(
                    "shaft-A",
                    Some(&m),
                    &crate::graph::load(bad.as_bytes()).unwrap()
                )
                .is_err(),
                "{bad}"
            );
        }
        assert!(resolve("different", Some(&m), &crate::graph::load(good).unwrap()).is_err());
    }
}
