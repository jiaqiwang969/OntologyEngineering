//! Locked oe.7 test-by-test fixtures. Original source and assertions were frozen
//! before implementation; these are regression cases, not independent transfer.
use semantica_runtime::datalog::{
    Bindings, ContextGraphData, DatalogFact, DatalogLimits, DatalogReasoner,
};
use serde_json::json;
use std::collections::BTreeSet;

fn seeded(facts: &[&str], rules: &[&str]) -> DatalogReasoner {
    let mut engine = DatalogReasoner::new();
    for fact in facts {
        engine.add_fact(*fact).unwrap();
    }
    for rule in rules {
        engine.add_rule(rule).unwrap();
    }
    engine
}
fn row(entries: &[(&str, &str)]) -> Bindings {
    entries
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}
fn fact(predicate: &str, args: &[&str]) -> DatalogFact {
    DatalogFact {
        predicate: predicate.into(),
        args: args.iter().map(|v| (*v).into()).collect(),
    }
}
fn manages() -> ContextGraphData {
    ContextGraphData {
        nodes: vec![],
        edges: vec![
            json!({"source":"alice","target":"bob","type":"manages"}),
            json!({"source":"bob","target":"carol","type":"manages"}),
        ],
    }
}

#[test]
fn test_add_string_fact() {
    let engine = seeded(&["parent(tom, bob)"], &[]);
    assert_eq!(engine.all_facts().len(), 1);
    assert_eq!(
        engine.all_facts().first().unwrap(),
        &fact("parent", &["tom", "bob"])
    );
}
#[test]
fn test_add_dict_fact() {
    let mut engine = DatalogReasoner::new();
    engine
        .add_fact(&json!({"subject":"bob","predicate":"parent","object":"ann"}))
        .unwrap();
    assert_eq!(engine.all_facts().len(), 1);
    assert_eq!(
        engine.all_facts().first().unwrap(),
        &fact("parent", &["bob", "ann"])
    );
}
#[test]
fn test_duplicate_fact_ignored() {
    let mut engine = seeded(&["parent(tom, bob)"], &[]);
    assert!(!engine.add_fact("parent(tom, bob)").unwrap());
    assert_eq!(engine.all_facts().len(), 1);
}
#[test]
fn test_empty_arguments_raise_error() {
    let mut engine = DatalogReasoner::new();
    for input in ["parent( )", "parent(tom, )"] {
        let error = engine.add_fact(input).unwrap_err().to_string();
        assert!(error.contains("Empty argument"), "{error}");
    }
    assert!(engine.all_facts().is_empty());
}
#[test]
fn test_single_rule() {
    let mut engine = seeded(&["parent(tom, bob)"], &["ancestor(X, Y) :- parent(X, Y)."]);
    assert!(
        engine
            .derive_all()
            .unwrap()
            .contains(&"ancestor(tom, bob)".into())
    );
}
#[test]
fn test_recursive_ancestor() {
    let mut engine = seeded(
        &["parent(tom, bob)", "parent(bob, ann)"],
        &[
            "ancestor(X, Y) :- parent(X, Y).",
            "ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).",
        ],
    );
    let derived = engine.derive_all().unwrap();
    for expected in [
        "ancestor(tom, bob)",
        "ancestor(bob, ann)",
        "ancestor(tom, ann)",
    ] {
        assert!(derived.contains(&expected.into()));
    }
}
#[test]
fn test_multi_hop_three_levels() {
    let mut engine = seeded(
        &["edge(1, 2)", "edge(2, 3)", "edge(3, 4)"],
        &[
            "reachable(X, Y) :- edge(X, Y).",
            "reachable(X, Y) :- edge(X, Z), reachable(Z, Y).",
        ],
    );
    assert!(
        engine
            .derive_all()
            .unwrap()
            .contains(&"reachable(1, 4)".into())
    );
}
#[test]
fn test_two_body_atoms() {
    let mut engine = seeded(
        &["parent(tom, bob)", "parent(bob, ann)"],
        &["grandparent(X, Y) :- parent(X, Z), parent(Z, Y)."],
    );
    let derived = engine.derive_all().unwrap();
    assert!(derived.contains(&"grandparent(tom, ann)".into()));
    assert!(!derived.contains(&"grandparent(tom, bob)".into()));
}
#[test]
fn test_variable_binding() {
    let mut engine = seeded(
        &["parent(tom, bob)", "parent(tom, alex)"],
        &["ancestor(X, Y) :- parent(X, Y)."],
    );
    let results = engine.query("ancestor(tom, ?Y)", None).unwrap();
    let mut ys: Vec<_> = results.iter().map(|r| r["Y"].as_str()).collect();
    ys.sort();
    assert_eq!(ys, vec!["alex", "bob"]);
}
#[test]
fn test_lowercase_variable_query() {
    let mut engine = seeded(&["parent(tom, bob)"], &[]);
    let results = engine.query("parent(tom, ?y)", None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["y"], "bob");
}
#[test]
fn test_pre_bound_variable() {
    let mut engine = seeded(&["parent(tom, bob)"], &["ancestor(X, Y) :- parent(X, Y)."]);
    let bob = engine
        .query("ancestor(tom, ?Y)", Some(&row(&[("Y", "bob")])))
        .unwrap();
    assert_eq!(bob.len(), 1);
    assert_eq!(bob[0]["Y"], "bob");
    let ann = engine
        .query("ancestor(tom, ?Y)", Some(&row(&[("Y", "ann")])))
        .unwrap();
    assert!(ann.is_empty());
}
#[test]
fn test_no_match_returns_empty() {
    let mut engine = seeded(&["parent(tom, bob)"], &[]);
    assert!(engine.query("parent(sarah, ?Y)", None).unwrap().is_empty());
}
#[test]
fn test_load_from_graph() {
    let graph = ContextGraphData {
        nodes: vec![json!({"id":"microsoft","type":"company"})],
        edges: vec![json!({"source":"microsoft","target":"openai","type":"invested_in"})],
    };
    let mut engine = DatalogReasoner::new();
    assert_eq!(engine.load_from_graph(&graph).unwrap(), 2);
    assert!(
        engine
            .all_facts()
            .contains(&fact("company", &["microsoft"]))
    );
    assert!(
        engine
            .all_facts()
            .contains(&fact("invested_in", &["microsoft", "openai"]))
    );
}
#[test]
fn test_edge_becomes_fact() {
    let mut engine = DatalogReasoner::new();
    engine.load_from_graph(&manages()).unwrap();
    assert!(
        engine
            .all_facts()
            .contains(&fact("manages", &["alice", "bob"]))
    );
    assert!(
        engine
            .all_facts()
            .contains(&fact("manages", &["bob", "carol"]))
    );
}
#[test]
fn test_derive_after_load() {
    let mut engine = DatalogReasoner::new();
    engine.load_from_graph(&manages()).unwrap();
    engine
        .add_rule("transitive_manages(X, Y) :- manages(X, Y).")
        .unwrap();
    engine
        .add_rule("transitive_manages(X, Y) :- manages(X, Z), transitive_manages(Z, Y).")
        .unwrap();
    assert!(
        engine
            .derive_all()
            .unwrap()
            .contains(&"transitive_manages(alice, carol)".into())
    );
}
#[test]
fn test_empty_program() {
    assert!(DatalogReasoner::new().derive_all().unwrap().is_empty());
}
#[test]
fn test_derive_all_idempotent() {
    let mut engine = seeded(&["parent(tom, bob)"], &["ancestor(X, Y) :- parent(X, Y)."]);
    let first = engine.derive_all().unwrap();
    let second = engine.derive_all().unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(first, second);
    assert!(engine.is_derived());
}
#[test]
fn test_clear_resets_state() {
    let mut engine = seeded(&["parent(tom, bob)"], &["ancestor(X, Y) :- parent(X, Y)."]);
    engine.derive_all().unwrap();
    engine.clear();
    assert!(engine.all_facts().is_empty());
    assert!(engine.rules().is_empty());
    assert_eq!(engine.indexed_predicates(), 0);
    assert!(!engine.is_derived());
    assert!(engine.last_stats().is_none());
    // Original delta sets are local work state in Rust and cannot survive clear.
    assert!(engine.derive_all().unwrap().is_empty());
}

#[test]
fn constants_multiple_variables_and_repeated_variables_are_real_joins() {
    let mut engine = seeded(
        &[
            "triple(alice, bob, red)",
            "triple(alice, carol, blue)",
            "triple(bob, bob, red)",
        ],
        &[
            "same_pair(X, X) :- triple(X, X, red).",
            "red_child(X, Y) :- triple(X, Y, red).",
        ],
    );
    let derived: BTreeSet<_> = engine.derive_all().unwrap().into_iter().collect();
    for expected in [
        "same_pair(bob, bob)",
        "red_child(alice, bob)",
        "red_child(bob, bob)",
    ] {
        assert!(derived.contains(expected));
    }
    for forbidden in ["same_pair(alice, bob)", "red_child(alice, carol)"] {
        assert!(!derived.contains(forbidden));
    }
    assert_eq!(
        engine.query("triple(?x, ?y, red)", None).unwrap(),
        vec![
            row(&[("x", "alice"), ("y", "bob")]),
            row(&[("x", "bob"), ("y", "bob")]),
        ]
    );
}
#[test]
fn cyclic_finite_graph_reaches_complete_fixed_point() {
    let mut engine = seeded(
        &["edge(a, b)", "edge(b, c)", "edge(c, a)"],
        &[
            "reach(X, Y) :- edge(X, Y).",
            "reach(X, Y) :- reach(X, Z), reach(Z, Y).",
        ],
    );
    let derived = engine.derive_all().unwrap();
    assert_eq!(derived.len(), 12);
    assert_eq!(engine.query("reach(?x, ?y)", None).unwrap().len(), 9);
    assert!(engine.is_derived());
    assert_eq!(engine.last_stats().unwrap().derived_facts, 9);
    assert_eq!(engine.derive_all().unwrap(), derived);
}
#[test]
fn adding_facts_and_rules_invalidates_then_recomputes_complete_closure() {
    let mut engine = seeded(&["edge(a, b)"], &["reach(X, Y) :- edge(X, Y)."]);
    engine.derive_all().unwrap();
    assert!(engine.is_derived());
    engine.add_fact("edge(b, c)").unwrap();
    assert!(!engine.is_derived());
    engine
        .add_rule("reach(X, Y) :- reach(X, Z), reach(Z, Y).")
        .unwrap();
    assert!(engine.derive_all().unwrap().contains(&"reach(a, c)".into()));
    let derived = engine.derive_all().unwrap();
    assert_eq!(engine.derive_all().unwrap(), derived);
}
#[test]
fn all_dictionary_formats_alias_precedence_and_normalization() {
    let mut engine = DatalogReasoner::new();
    for value in [
        json!({"subject":"Tom Smith","predicate":"PARENT Of","object":"ANN"}),
        json!({"source_name":"ALICE","target_id":"Bob","relation":"WORKS For"}),
        json!({"source_id":"A","target_name":"B"}),
        json!({"name":"ACME","type":"COMPANY"}),
        json!({"id":42,"type":"entity"}),
    ] {
        assert!(engine.add_fact(&value).unwrap());
    }
    assert!(!engine.add_fact(&json!({"unknown":"skip"})).unwrap());
    assert!(
        !engine
            .add_fact(&json!({"source":null,"source_id":"a","target":"b"}))
            .unwrap()
    );
    assert!(
        !engine
            .add_fact(&json!({"id":"","name":"fallback","type":"company"}))
            .unwrap()
    );
    let all = engine.derive_all().unwrap();
    assert_eq!(all.len(), 5);
    for expected in [
        "parent_of(tom_smith, ann)",
        "works_for(alice, bob)",
        "connected_to(a, b)",
        "company(acme)",
        "entity(42)",
    ] {
        assert!(all.contains(&expected.into()));
    }
}
#[test]
fn query_repeated_variables_lowercase_bindings_and_ground_projection() {
    let mut engine = seeded(&["pair(a, a)", "pair(a, b)", "pair(b, b)"], &[]);
    assert_eq!(
        engine.query("pair(?x, ?x)", None).unwrap(),
        vec![row(&[("x", "a")]), row(&[("x", "b")])]
    );
    assert_eq!(
        engine
            .query("pair(?x, ?x)", Some(&row(&[("x", "b")])))
            .unwrap(),
        vec![row(&[("x", "b")])]
    );
    assert_eq!(
        engine.query("pair(X, X)", None).unwrap(),
        vec![row(&[("X", "a")]), row(&[("X", "b")])]
    );
    assert!(engine.query("pair(a, a)", None).unwrap().is_empty());
}
#[test]
fn malformed_and_unsafe_programs_fail_without_mutating_existing_state() {
    let mut engine = seeded(&["parent(tom, bob)"], &[]);
    for rule in [
        "p(X) :- q(X), garbage",
        "p(X) :- q(Y).",
        "p(X) :- not q(X).",
        "p(X) :- q(f(X)).",
        "p(X) :- q(X),",
        "p(X) :- ",
        "p(X) q(X)",
    ] {
        assert!(engine.add_rule(rule).is_err(), "{rule}");
        assert!(engine.rules().is_empty());
    }
    for input in [
        "parent(Tom, bob)",
        "parent(tom, )",
        "bad",
        "p(a)\njunk",
        "p(a\0)",
    ] {
        assert!(engine.add_fact(input).is_err(), "{input}");
    }
    assert!(engine.query("parent(tom, ?)", None).is_err());
    assert!(engine.query("parent(tom, ?x), garbage", None).is_err());
    assert_eq!(engine.all_facts().len(), 1);
}
fn bounded_ancestor(limits: DatalogLimits) -> DatalogReasoner {
    let mut engine = DatalogReasoner::with_limits(limits).unwrap();
    for fact in ["edge(a, b)", "edge(b, c)"] {
        engine.add_fact(fact).unwrap();
    }
    engine.add_rule("reach(X, Y) :- edge(X, Y).").unwrap();
    engine
        .add_rule("reach(X, Y) :- edge(X, Z), reach(Z, Y).")
        .unwrap();
    engine
}
#[test]
fn inference_budgets_fail_atomically_and_never_report_a_fixed_point() {
    let defaults = DatalogLimits::default();
    let limits = [
        (
            DatalogLimits {
                max_facts: 2,
                ..defaults.clone()
            },
            "datalog_fact_budget",
        ),
        (
            DatalogLimits {
                max_iterations: 1,
                ..defaults.clone()
            },
            "datalog_iteration_budget",
        ),
        (
            DatalogLimits {
                max_match_attempts: 1,
                ..defaults.clone()
            },
            "datalog_match_budget",
        ),
        (
            DatalogLimits {
                max_bindings: 1,
                ..defaults.clone()
            },
            "datalog_join_binding_budget",
        ),
        (
            DatalogLimits {
                max_fact_bytes: 15,
                ..defaults
            },
            "datalog_fact_byte_budget",
        ),
    ];
    for (limit, expected_error) in limits {
        let mut engine = bounded_ancestor(limit);
        let before = engine.all_facts().clone();
        let error = engine.derive_all().unwrap_err().to_string();
        assert_eq!(error, expected_error);
        assert_eq!(engine.all_facts(), &before);
        assert!(!engine.is_derived());
        assert!(engine.last_stats().is_none());
        assert!(engine.query("reach(?x, ?y)", None).is_err());
        assert_eq!(engine.all_facts(), &before);
        engine.clear();
        assert!(engine.all_facts().is_empty());
        assert!(engine.rules().is_empty());
        assert_eq!(engine.indexed_predicates(), 0);
        assert!(!engine.is_derived());
    }
}
#[test]
fn query_row_budget_errors_instead_of_returning_truncated_success() {
    let mut engine = DatalogReasoner::with_limits(DatalogLimits {
        max_query_rows: 1,
        ..DatalogLimits::default()
    })
    .unwrap();
    engine.add_fact("parent(tom, bob)").unwrap();
    engine.add_fact("parent(tom, ann)").unwrap();
    assert_eq!(
        engine
            .query("parent(tom, ?y)", None)
            .unwrap_err()
            .to_string(),
        "datalog_query_row_budget"
    );
    assert_eq!(engine.all_facts().len(), 2);
}
#[test]
fn graph_import_is_atomic_on_invalid_record_and_entry_budget() {
    let mut engine = seeded(&["seed(a)"], &[]);
    let before = engine.all_facts().clone();
    let graph = json!({
        "edges":[{"source":"a","target":"b","type":"edge"}],
        "nodes":[{"id":"","type":"company"},{"subject":"a","predicate":"edge","object":""}]
    });
    assert!(engine.load_from_graph_value(&graph).is_err());
    assert_eq!(engine.all_facts(), &before);
    let mut limited = DatalogReasoner::with_limits(DatalogLimits {
        max_context_entries: 1,
        ..DatalogLimits::default()
    })
    .unwrap();
    assert_eq!(
        limited.load_from_graph(&manages()).unwrap_err().to_string(),
        "datalog_context_entry_budget"
    );
    assert!(limited.all_facts().is_empty());
}
#[test]
fn node_id_maps_and_graph_reloads_keep_fact_identity() {
    let mut engine = DatalogReasoner::new();
    let graph = json!({
        "nodes":{"n1":{"id":"company","type":"entity"}},
        "edges":[{"source_id":"company","target_id":"supplier","relation":"purchases_from"}]
    });
    assert_eq!(engine.load_from_graph_value(&graph).unwrap(), 2);
    assert_eq!(engine.load_from_graph_value(&graph).unwrap(), 0);
    assert_eq!(
        engine.derive_all().unwrap(),
        vec!["entity(company)", "purchases_from(company, supplier)"]
    );
}
#[test]
fn a_three_atom_join_respects_arity_and_constant_head_terms() {
    let mut engine = seeded(
        &[
            "edge(a, b)",
            "edge(b, c)",
            "tag(c, red)",
            "tag(b)",
            "edge(b, d)",
            "tag(d, blue)",
        ],
        &["path_tag(X, Z, red) :- edge(X, Y), edge(Y, Z), tag(Z, red)."],
    );
    let derived = engine.derive_all().unwrap();
    assert!(derived.contains(&"path_tag(a, c, red)".into()));
    assert!(!derived.contains(&"path_tag(a, d, red)".into()));
}
