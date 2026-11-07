//! Every drafting rule of the ontology is traceable: concepts of the catalogue, a standard clause, a label and a
//! message; the knowledge file carries no identity of a real drawing.
use oe_drawing::engineering_expression::drawing_concepts;
use oe_drawing_check::drafting::{self, KNOWLEDGE};
use std::collections::BTreeSet;

#[test]
fn every_rule_names_concepts_of_the_catalogue_and_a_clause() {
    let catalogue: BTreeSet<&str> = drawing_concepts()
        .unwrap()
        .iter()
        .flat_map(|g| g.concepts.iter().map(|c| c.id.as_str()))
        .collect();
    let k = drafting::knowledge().unwrap();
    let rules = drafting::rules(&k.shapes).unwrap();
    let ids: BTreeSet<&str> = rules.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.len() >= 12, "{ids:?}");
    let mut groups: BTreeSet<&str> = BTreeSet::new();
    for r in &rules {
        assert!(!r.id.is_empty() && r.shape.contains(&r.id), "shape {} without its rule id", r.shape);
        assert!(!r.clause.is_empty(), "{} has no clause", r.id);
        assert!(!r.label.is_empty() && !r.messages.is_empty(), "{} has no label or message", r.id);
        assert!(!r.concept_ids.is_empty(), "{} names no concept", r.id);
        for c in &r.concept_ids {
            assert!(catalogue.contains(c.as_str()), "{} names {c}, not in the catalogue", r.id);
        }
        groups.insert(r.group.as_str());
    }
    assert!(groups.is_superset(&BTreeSet::from(["G1", "G2", "G4"])), "{groups:?}");
    // The classes of the ontology that name a concept name one of the catalogue.
    for line in k.ontology.lines() {
        let Some(rest) = line.split_once("md:concept ").map(|x| x.1) else {
            continue;
        };
        for token in rest.split([' ', ',', ';', '.']) {
            if let Some(c) = token.strip_prefix("dc:") {
                assert!(catalogue.contains(c), "ontology names {c}, not in the catalogue");
            }
        }
    }
    // No source handle of a drawing: no run of twelve or more hexadecimal capitals.
    let mut run = 0;
    for ch in KNOWLEDGE.chars() {
        run = if ch.is_ascii_hexdigit() && (ch.is_ascii_digit() || ch.is_ascii_uppercase()) { run + 1 } else { 0 };
        assert!(run < 12, "the knowledge file contains a handle-like token");
    }
}
