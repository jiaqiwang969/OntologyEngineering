//! Independent expectations: W3C SHACL 4.7.2 and XSD 1.1 doubleRep.
use oe_kernel::shacl;
use serde_json::Value;

const PREFIXES: &str = "@prefix ex: <urn:audit:> . @prefix sh: <http://www.w3.org/ns/shacl#> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . ";
const NESTED: &str = "ex:Shape sh:targetNode ex:parent; sh:property [sh:path ex:child; sh:property [sh:path ex:name; sh:minCount 1]].";

fn validate(data: &str, shape: &str) -> Value {
    shacl::validate(
        format!("{PREFIXES}{data}").as_bytes(),
        format!("{PREFIXES}{shape}").as_bytes(),
    )
    .unwrap()
}

#[test]
fn nested_property_requires_each_child_name() {
    let missing = validate("ex:parent ex:child ex:child1; ex:name \"parent\".", NESTED);
    assert_eq!(
        missing["conforms"], false,
        "a parent's name cannot satisfy its child's constraint"
    );
    assert_eq!(missing["violation_count"], 1);
    assert_eq!(missing["findings"][0]["focus"], "<urn:audit:child1>");
    let mixed = validate(
        "ex:parent ex:child ex:child1, ex:child2; ex:name \"parent\". ex:child1 ex:name \"child\".",
        NESTED,
    );
    assert_eq!(mixed["violation_count"], 1);
    assert_eq!(mixed["findings"][0]["focus"], "<urn:audit:child2>");
}

#[test]
fn nested_property_does_not_require_parent_name() {
    assert_eq!(
        validate(
            "ex:parent ex:child ex:child1. ex:child1 ex:name \"child\".",
            NESTED
        )["conforms"],
        true
    );
    assert_eq!(
        validate(
            "ex:parent ex:child ex:child1; ex:name \"parent\". ex:child1 ex:name \"child\".",
            NESTED
        )["conforms"],
        true
    );
    // No child means no child-specific obligation; an outer minCount is separate.
    assert_eq!(validate("ex:parent ex:other 1.", NESTED)["conforms"], true);
}

#[test]
fn nested_property_follows_every_level_and_keeps_node_shape_behavior() {
    let shape = "ex:Shape sh:targetNode ex:parent; sh:property [sh:path ex:child; sh:property [sh:path ex:part; sh:property [sh:path ex:name; sh:minCount 1]]].";
    let data = "ex:parent ex:child ex:child1. ex:child1 ex:part ex:part1.";
    assert_eq!(validate(data, shape)["conforms"], false);
    assert_eq!(
        validate(&format!("{data} ex:part1 ex:name \"part\"."), shape)["conforms"],
        true
    );
    let direct = "ex:Shape sh:targetNode ex:parent; sh:property [sh:path ex:name; sh:minCount 1].";
    assert_eq!(
        validate("ex:parent ex:name \"parent\".", direct)["conforms"],
        true
    );
    assert_eq!(validate("ex:parent ex:other 1.", direct)["conforms"], false);
}

fn lexical(word: &str, datatype: &str) -> Value {
    validate(
        &format!("ex:subject ex:value \"{word}\"^^xsd:{datatype}."),
        &format!(
            "ex:Shape sh:targetNode ex:subject; sh:property [sh:path ex:value; sh:datatype xsd:{datatype}]."
        ),
    )
}

#[test]
fn floating_datatypes_reject_non_xsd_spellings() {
    for datatype in ["double", "float"] {
        for word in [
            "inf", "infinity", "nan", "Infinity", "NAN", "+NaN", "-NaN", "1e", "1_000", "0x1", ".",
            "nonsense",
        ] {
            assert_eq!(
                lexical(word, datatype)["conforms"],
                false,
                "{datatype}: {word}"
            );
        }
    }
}

#[test]
fn floating_datatypes_accept_xsd_numeric_and_special_forms() {
    for datatype in ["double", "float"] {
        for word in [
            "INF", "+INF", "-INF", "NaN", "0", "-0", "+1", "1.", ".5", "-.5", "1.25e2", "-2E-3",
            "1e9999",
        ] {
            assert_eq!(
                lexical(word, datatype)["conforms"],
                true,
                "{datatype}: {word}"
            );
        }
    }
}
