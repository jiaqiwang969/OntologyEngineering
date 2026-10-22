//! Frozen before implementation: independent W3C SHACL Core expectations.
//! https://www.w3.org/TR/shacl/#property-paths
//! https://www.w3.org/TR/shacl/#core-components
//! These are semantic fixtures, not a claim of all W3C-suite conformance.
use semantica_runtime::shacl;
use serde_json::Value;

const PREFIXES: &str = "@prefix ex: <urn:core:> . @prefix sh: <http://www.w3.org/ns/shacl#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . ";
fn result(data: &str, shapes: &str) -> anyhow::Result<Value> {
    shacl::validate(
        format!("{PREFIXES}{data}").as_bytes(),
        format!("{PREFIXES}{shapes}").as_bytes(),
    )
}
fn validate(data: &str, shapes: &str) -> Value {
    result(data, shapes).unwrap()
}
fn property(path: &str, constraints: &str) -> String {
    format!("ex:S sh:targetNode ex:a; sh:property [ sh:path {path}; {constraints} ].")
}

#[test]
fn inverse_path_targets_actual_predecessors() {
    let s = property(
        "[sh:inversePath ex:parent]",
        "sh:hasValue ex:b; sh:minCount 1; sh:maxCount 1",
    );
    assert_eq!(validate("ex:b ex:parent ex:a.", &s)["conforms"], true);
    assert_eq!(validate("ex:a ex:parent ex:b.", &s)["conforms"], false);
}
#[test]
fn sequence_path_cardinality_is_distinct_endpoints() {
    let s = property(
        "(ex:p ex:q)",
        "sh:hasValue ex:d; sh:minCount 1; sh:maxCount 1",
    );
    assert_eq!(
        validate("ex:a ex:p ex:b, ex:c. ex:b ex:q ex:d. ex:c ex:q ex:d.", &s)["conforms"],
        true
    );
    assert_eq!(
        validate("ex:a ex:p ex:b, ex:c. ex:b ex:q ex:d. ex:c ex:q ex:e.", &s)["violation_count"],
        1
    );
}
#[test]
fn inverse_of_sequence_reverses_each_step_and_step_order() {
    let s = property(
        "[sh:inversePath (ex:p ex:q)]",
        "sh:hasValue ex:c; sh:minCount 1; sh:maxCount 1",
    );
    assert_eq!(
        validate("ex:c ex:p ex:b. ex:b ex:q ex:a.", &s)["conforms"],
        true
    );
    assert_eq!(
        validate("ex:c ex:q ex:b. ex:b ex:p ex:a.", &s)["conforms"],
        false
    );
}
#[test]
fn alternative_path_unions_and_deduplicates_values() {
    let s = property(
        "[sh:alternativePath (ex:p ex:q)]",
        "sh:minCount 2; sh:maxCount 2; sh:hasValue ex:c",
    );
    assert_eq!(
        validate("ex:a ex:p ex:b; ex:q ex:b, ex:c.", &s)["conforms"],
        true
    );
    assert_eq!(
        validate("ex:a ex:p ex:b; ex:q ex:b.", &s)["conforms"],
        false
    );
}
#[test]
fn repetitions_terminate_on_data_cycles_with_exact_zero_semantics() {
    let d = "ex:a ex:p ex:b. ex:b ex:p ex:c. ex:c ex:p ex:b.";
    let zero = property(
        "[sh:zeroOrMorePath ex:p]",
        "sh:minCount 3; sh:maxCount 3; sh:hasValue ex:a",
    );
    let one = property(
        "[sh:oneOrMorePath ex:p]",
        "sh:minCount 2; sh:maxCount 2; sh:hasValue ex:c",
    );
    let optional = property(
        "[sh:zeroOrOnePath ex:p]",
        "sh:minCount 2; sh:maxCount 2; sh:hasValue ex:a",
    );
    for s in [zero, one, optional] {
        assert_eq!(validate(d, &s)["conforms"], true);
    }
    let no_edges = property(
        "[sh:zeroOrMorePath ex:p]",
        "sh:hasValue ex:a; sh:minCount 1; sh:maxCount 1",
    );
    assert_eq!(validate("", &no_edges)["conforms"], true);
    assert_eq!(
        validate("", &property("[sh:oneOrMorePath ex:p]", "sh:minCount 1"))["conforms"],
        false
    );
}
#[test]
fn repeated_complex_path_uses_composed_relation() {
    let s = property(
        "[sh:oneOrMorePath (ex:p [sh:inversePath ex:q])]",
        "sh:hasValue ex:c; sh:minCount 2; sh:maxCount 2",
    );
    assert_eq!(
        validate(
            "ex:a ex:p ex:x. ex:b ex:q ex:x; ex:p ex:y. ex:c ex:q ex:y.",
            &s
        )["conforms"],
        true
    );
}
#[test]
fn malformed_and_recursive_path_grammars_fail_without_targets() {
    for path in [
        "\"literal\"",
        "(ex:p)",
        "[sh:alternativePath (ex:p)]",
        "[sh:inversePath ex:p; sh:zeroOrMorePath ex:q]",
        "_:loop",
    ] {
        let s = format!("ex:S sh:path {path}; sh:minCount 1. _:loop sh:inversePath _:loop.");
        assert!(result("", &s).is_err(), "{path}");
    }
}
#[test]
fn equals_reports_symmetric_difference_not_just_missing_left() {
    let s = property("ex:p", "sh:equals ex:q");
    assert_eq!(validate("ex:a ex:p ex:b; ex:q ex:b.", &s)["conforms"], true);
    assert_eq!(
        validate("ex:a ex:p ex:b; ex:q ex:c.", &s)["violation_count"],
        2
    );
    assert_eq!(validate("ex:a ex:q ex:c.", &s)["violation_count"], 1);
}
#[test]
fn disjoint_reports_only_intersection_for_each_comparison() {
    let s = property("ex:p", "sh:disjoint ex:q, ex:r");
    assert_eq!(
        validate("ex:a ex:p ex:b, ex:c; ex:q ex:c; ex:r ex:d.", &s)["violation_count"],
        1
    );
    assert_eq!(
        validate("ex:a ex:p ex:b; ex:q ex:c; ex:r ex:d.", &s)["conforms"],
        true
    );
}
#[test]
fn closed_shape_checks_each_triple_and_ignored_predicates() {
    let s = "ex:S sh:targetNode ex:a; sh:closed true; sh:ignoredProperties (rdf:type); sh:property [sh:path ex:p].";
    assert_eq!(validate("ex:a a ex:T; ex:p ex:b.", s)["conforms"], true);
    let r = validate("ex:a a ex:T; ex:p ex:b; ex:q ex:c, ex:d.", s);
    assert_eq!(r["violation_count"], 2);
    assert_eq!(r["findings"][0]["path"], "<urn:core:q>");
    assert_eq!(
        validate(
            "ex:a ex:q ex:c.",
            "ex:S sh:targetNode ex:a; sh:closed false."
        )["conforms"],
        true
    );
}
#[test]
fn closed_nested_property_shape_applies_to_value_node() {
    let s = property("ex:p", "sh:closed true; sh:property [sh:path ex:q]");
    let r = validate(
        "ex:a ex:p ex:b; ex:allowedAtParent ex:c. ex:b ex:q ex:d; ex:bad ex:e.",
        &s,
    );
    assert_eq!(r["violation_count"], 1);
    assert_eq!(r["findings"][0]["value"], "<urn:core:e>");
}
#[test]
fn language_in_uses_case_insensitive_basic_language_ranges() {
    let s = property("ex:p", "sh:languageIn (\"en\" \"mi\")");
    assert_eq!(
        validate("ex:a ex:p \"a\"@en, \"b\"@en-NZ, \"c\"@mi.", &s)["conforms"],
        true
    );
    assert_eq!(
        validate("ex:a ex:p \"a\", \"b\"@de, ex:c.", &s)["violation_count"],
        3
    );
    assert_eq!(
        validate(
            "ex:a ex:p \"a\"@fr.",
            &property("ex:p", "sh:languageIn (\"*\")")
        )["conforms"],
        true
    );
    assert_eq!(
        validate(
            "ex:a ex:p \"a\".",
            &property("ex:p", "sh:languageIn (\"*\")")
        )["conforms"],
        false
    );
}
#[test]
fn unique_language_has_one_finding_per_duplicate_nonempty_tag() {
    let s = property("ex:p", "sh:uniqueLang true");
    assert_eq!(
        validate("ex:a ex:p \"a\", \"b\", \"c\"@en, \"d\"@fr.", &s)["conforms"],
        true
    );
    assert_eq!(
        validate(
            "ex:a ex:p \"a\"@en, \"b\"@EN, \"c\"@en, \"d\"@fr, \"e\"@fr.",
            &s
        )["violation_count"],
        2
    );
    assert_eq!(
        validate(
            "ex:a ex:p \"a\"@en, \"b\"@en.",
            &property("ex:p", "sh:uniqueLang false")
        )["conforms"],
        true
    );
}
#[test]
fn xone_requires_exactly_one_matching_shape() {
    let s = property(
        "ex:p",
        "sh:xone ([sh:datatype xsd:string] [sh:minLength 2])",
    );
    assert_eq!(validate("ex:a ex:p \"a\".", &s)["conforms"], true);
    assert_eq!(validate("ex:a ex:p \"abc\".", &s)["conforms"], false);
    assert_eq!(validate("ex:a ex:p 1.", &s)["conforms"], false);
    let failing_branch = property("ex:p", "sh:or ([sh:datatype xsd:string] [sh:node ex:Loop])");
    assert!(
        result(
            "ex:a ex:p \"a\".",
            &format!("{failing_branch} ex:Loop sh:node ex:Loop.")
        )
        .is_err(),
        "a matching branch must not swallow another branch's failure"
    );
}
#[test]
fn qualified_cardinality_counts_conforming_distinct_values() {
    let s = property(
        "ex:p",
        "sh:qualifiedValueShape [sh:class ex:Special]; sh:qualifiedMinCount 1; sh:qualifiedMaxCount 1",
    );
    assert_eq!(
        validate("ex:a ex:p ex:b, ex:c. ex:b a ex:Special.", &s)["conforms"],
        true
    );
    assert_eq!(
        validate(
            "ex:a ex:p ex:b, ex:c. ex:b a ex:Special. ex:c a ex:Special.",
            &s
        )["violation_count"],
        1
    );
    assert_eq!(validate("ex:a ex:p ex:b.", &s)["violation_count"], 1);
}
#[test]
fn qualified_disjoint_excludes_sibling_matches_without_path_filter() {
    let s = "ex:S sh:targetNode ex:a; sh:property ex:Q, ex:R. ex:Q sh:path ex:p; sh:qualifiedValueShape ex:X; sh:qualifiedValueShapesDisjoint true; sh:qualifiedMinCount 1. ex:R sh:path ex:different; sh:qualifiedValueShape ex:Y; sh:qualifiedMinCount 0. ex:X sh:class ex:A. ex:Y sh:class ex:B.";
    assert_eq!(
        validate("ex:a ex:p ex:b. ex:b a ex:A.", s)["conforms"],
        true
    );
    assert_eq!(
        validate("ex:a ex:p ex:b. ex:b a ex:A, ex:B.", s)["violation_count"],
        1
    );
}
#[test]
fn multiple_required_values_and_classes_all_remain_required() {
    let s = property("ex:p", "sh:hasValue ex:b, ex:c; sh:class ex:A, ex:B");
    assert_eq!(
        validate(
            "ex:a ex:p ex:b, ex:c. ex:b a ex:A, ex:B. ex:c a ex:A, ex:B.",
            &s
        )["conforms"],
        true
    );
    assert_eq!(
        validate("ex:a ex:p ex:b. ex:b a ex:A, ex:B.", &s)["violation_count"],
        1
    );
}
#[test]
fn all_six_node_kinds_are_supported() {
    for (kind, data, expected) in [
        ("BlankNodeOrLiteral", "ex:a ex:p [] , \"a\".", true),
        ("BlankNodeOrLiteral", "ex:a ex:p ex:b.", false),
        ("IRIOrLiteral", "ex:a ex:p ex:b, \"a\".", true),
        ("IRIOrLiteral", "ex:a ex:p [].", false),
    ] {
        assert_eq!(
            validate(data, &property("ex:p", &format!("sh:nodeKind sh:{kind}")))["conforms"],
            expected
        );
    }
}
#[test]
fn class_shapes_have_implicit_class_targets_including_subclass_instances() {
    let s = "ex:A a rdfs:Class, sh:NodeShape; sh:property [sh:path ex:p; sh:minCount 1].";
    let r = validate("ex:b a ex:A. ex:c a ex:B. ex:B rdfs:subClassOf ex:A.", s);
    assert_eq!(r["focus_count"], 2);
    assert_eq!(r["violation_count"], 2);
}
#[test]
fn ill_formed_core_parameters_are_rejected_even_without_targets() {
    for s in [
        "ex:S sh:path ex:p; sh:uniqueLang \"true\".",
        "ex:S sh:closed \"true\".",
        "ex:S sh:closed true; sh:ignoredProperties (\"p\").",
        "ex:S sh:path ex:p; sh:languageIn (ex:en).",
        "ex:S sh:path ex:p; sh:qualifiedMinCount 1.",
        "ex:S sh:uniqueLang true.",
        "ex:S sh:minCount 1.",
        "ex:S sh:path ex:p; sh:equals \"p\".",
        "ex:S a sh:NodeShape; sh:path ex:p.",
        "ex:S a sh:PropertyShape.",
        "ex:S sh:property [sh:minCount 1].",
        "ex:S sh:path ex:p; sh:qualifiedValueShape [sh:class ex:T]; sh:qualifiedMinCount -1.",
    ] {
        assert!(result("", s).is_err(), "{s}");
    }
    assert!(result("", "ex:S sh:unsupportedProperty true.").is_err());
}

#[test]
fn report_keeps_shape_component_severity_and_all_declared_messages() {
    let s = "ex:S sh:targetNode ex:a; sh:property ex:P. ex:P sh:path ex:p; sh:minCount 1; sh:severity sh:Warning; sh:message \"missing\"@en, \"缺失\"@zh.";
    let r = validate("", s);
    assert_eq!(
        r["conforms"], false,
        "severity does not erase a validation result"
    );
    let finding = &r["findings"][0];
    assert_eq!(finding["source_shape"], "<urn:core:P>");
    assert_eq!(
        finding["source_constraint_component"],
        "http://www.w3.org/ns/shacl#MinCountConstraintComponent"
    );
    assert_eq!(finding["severity"], "http://www.w3.org/ns/shacl#Warning");
    assert_eq!(finding["messages"].as_array().unwrap().len(), 2);
}

#[test]
fn logical_empty_lists_follow_their_quantifiers_and_multiple_constraints_accumulate() {
    assert_eq!(
        validate("", "ex:S sh:targetNode ex:a; sh:and ().")["conforms"],
        true
    );
    assert_eq!(
        validate("", "ex:S sh:targetNode ex:a; sh:or ().")["conforms"],
        false
    );
    assert_eq!(
        validate("", "ex:S sh:targetNode ex:a; sh:xone ().")["conforms"],
        false
    );
    let s = "ex:S sh:targetNode ex:a; sh:node [sh:class ex:T], [sh:class ex:U].";
    assert_eq!(validate("ex:a a ex:T.", s)["violation_count"], 1);
}
