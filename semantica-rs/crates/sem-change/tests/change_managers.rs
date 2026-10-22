//! Characterisation tests for `managers.rs` (`semantica/change_management/managers.py`):
//! `TemporalVersionManager` (diffs, tags, mutation audit trail, attach/restore with a graph,
//! pruning with a triplet store), `OntologyVersionManager` and the `BaseVersionManager`
//! contract. Expected values come from the oracle (`tests/fixtures/oracle.json`), which drove
//! the original with the same fake graph and fake store.

mod common;

use std::cell::RefCell;

use common::{assert_map_with_sets, mask, obj, oracle, outcome};
use sem_change::managers::{
    MutationCallback, MutationTrackedGraph, SnapshotOptions, TripletStoreQuery, init_storage,
};
use sem_change::{
    BaseVersionManager, OntologyVersionManager, TemporalVersionManager, verify_checksum,
};
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

const DIFF_LISTS: [&str; 12] = [
    "entities_added",
    "entities_removed",
    "entities_modified",
    "relationships_added",
    "relationships_removed",
    "relationships_modified",
    "nodes_added",
    "nodes_removed",
    "nodes_modified",
    "edges_added",
    "edges_removed",
    "edges_modified",
];

const STRUCTURE_LISTS: [&str; 8] = [
    "classes_added",
    "classes_removed",
    "properties_added",
    "properties_removed",
    "individuals_added",
    "individuals_removed",
    "axioms_added",
    "axioms_removed",
];

const TIME_KEYS: [&str; 2] = ["timestamp", "created_at"];

fn opts() -> SnapshotOptions {
    SnapshotOptions::default()
}

/// The oracle's `FakeStore` (a triplet store whose `execute_query` records the query).
struct FakeStore {
    queries: RefCell<Vec<String>>,
    fail: bool,
}

impl TripletStoreQuery for FakeStore {
    fn execute_query(&self, query: &str) -> Result<Value, SemanticaError> {
        self.queries.borrow_mut().push(query.to_string());
        if self.fail {
            return Err(SemanticaError::Runtime("store down".into()));
        }
        Ok(json!({"ok": true}))
    }
}

/// The oracle's `FakeGraph` (a `ContextGraph` stand-in).
#[derive(Default)]
struct FakeGraph {
    graph_id: Option<String>,
    callback: Option<MutationCallback>,
    suspend: bool,
    loaded: Vec<Map<String, Value>>,
    suspend_seen: Vec<bool>,
}

impl MutationTrackedGraph for FakeGraph {
    fn set_mutation_callback(&mut self, callback: MutationCallback) {
        self.callback = Some(callback);
    }

    fn graph_id(&self) -> Option<String> {
        self.graph_id.clone()
    }

    fn suspend_mutation_callback(&self) -> bool {
        self.suspend
    }

    fn set_suspend_mutation_callback(&mut self, value: bool) {
        self.suspend = value;
    }

    fn from_dict(&mut self, payload: &Map<String, Value>) -> Result<(), SemanticaError> {
        self.suspend_seen.push(self.suspend);
        self.loaded.push(payload.clone());
        Ok(())
    }
}

fn assert_compare(actual: &Value, expected: &Value, context: &str) {
    match expected.get("ok") {
        Some(ok) => assert_map_with_sets(&actual["ok"], ok, &DIFF_LISTS, context),
        None => assert_eq!(actual, expected, "{context}"),
    }
}

#[test]
fn test_temporal_compare_versions_matches_oracle() {
    let expected = &oracle()["temporal"];
    let m = TemporalVersionManager::new(None, None).unwrap();
    for (i, case) in expected["compare"].as_array().unwrap().iter().enumerate() {
        m.create_snapshot(
            &case["g1"],
            &format!("a{i}"),
            "test@example.com",
            &format!("A {i}"),
            &opts(),
        )
        .unwrap();
        m.create_snapshot(
            &case["g2"],
            &format!("b{i}"),
            "test@example.com",
            &format!("B {i}"),
            &opts(),
        )
        .unwrap();
        let by_label =
            outcome(m.compare_versions(&json!(format!("a{i}")), &json!(format!("b{i}")), None));
        assert_compare(&by_label, &case["labels"], &format!("labels {i}"));
        let by_dict = outcome(m.compare_versions(&case["g1"], &case["g2"], None));
        assert_compare(&by_dict, &case["dicts"], &format!("dicts {i}"));
    }

    let errors = &expected["compare_errors"];
    let cases: [(&str, Value, Value); 9] = [
        ("missing_first", json!("nope"), json!("a0")),
        ("missing_second", json!("a0"), json!("nope")),
        ("non_dict", json!(["x"]), json!("a0")),
        ("number", json!(5), json!("a0")),
        ("no_schema", json!({"foo": []}), json!("a0")),
        ("non_dict_entity", json!({"entities": ["s"]}), json!("a0")),
        (
            "unhashable_id",
            json!({"entities": [{"id": [1]}]}),
            json!("a0"),
        ),
        (
            "non_dict_relationship",
            json!({"entities": [], "relationships": [5]}),
            json!("a0"),
        ),
        ("entities_number", json!({"entities": 5}), json!("a0")),
    ];
    for (key, v1, v2) in cases {
        assert_eq!(
            outcome(m.compare_versions(&v1, &v2, None)),
            errors[key],
            "{key}"
        );
    }
    let unknown = outcome(m.compare_versions(
        &json!({"entities": []}),
        &json!({"nodes": [], "label": null}),
        None,
    ));
    assert_compare(
        &unknown,
        &expected["compare_unknown_label"],
        "unknown label",
    );
    assert_compare(&outcome(m.diff("a0", "b0")), &expected["diff"], "diff");
    // The trait-level compare is the same comparison.
    let via_trait = outcome(BaseVersionManager::compare_versions(
        &m,
        &json!("a0"),
        &json!("b0"),
    ));
    assert_compare(&via_trait, &expected["diff"], "trait compare");
}

#[test]
fn test_temporal_create_snapshot_matches_oracle() {
    let expected = &oracle()["temporal"];
    let m = TemporalVersionManager::new(None, None).unwrap();
    m.create_snapshot(&json!({"entities": []}), "a0", "a@b.co", "d", &opts())
        .unwrap();
    let create = |graph: Value, label: &str, author: &str, description: &str| {
        outcome(m.create_snapshot(&graph, label, author, description, &opts()))
    };
    let errors = &expected["create_errors"];
    assert_eq!(
        create(json!({"entities": []}), "e1", "invalid-email", "d"),
        errors["author"]
    );
    assert_eq!(
        create(json!({"entities": []}), "e2", "a@b.co", &"x".repeat(501)),
        errors["long"]
    );
    assert_eq!(
        create(json!({"entities": []}), "e3", "a@b.co", "  "),
        errors["blank"]
    );
    assert_eq!(create(json!([1]), "e4", "a@b.co", "d"), errors["non_dict"]);
    assert_eq!(
        create(json!({"foo": []}), "e5", "a@b.co", "d"),
        errors["no_schema"]
    );
    assert_eq!(
        create(json!([1]), "e6", "bad", "d"),
        errors["author_before_graph"]
    );
    assert_eq!(
        create(json!({"entities": []}), "", "a@b.co", "d"),
        errors["empty_label"]
    );
    assert_eq!(
        create(json!({"entities": []}), "a0", "a@b.co", "d"),
        errors["duplicate"]
    );
    let strings = m
        .create_snapshot(&json!({"entities": "ab"}), "e7", "a@b.co", "d", &opts())
        .unwrap();
    assert_eq!(
        json!({"ok": mask(&Value::Object(strings), &["timestamp", "checksum"])}),
        errors["entities_string"]
    );

    let snap = m
        .create_snapshot(
            &json!({"edges": [{"source": "x"}]}),
            "meta",
            "a@b.co",
            "with metadata",
            &SnapshotOptions::with_metadata(json!({"k": [1]})),
        )
        .unwrap();
    let shape = &expected["create_shape"];
    let keys: Vec<&String> = snap.keys().collect();
    assert_eq!(json!(keys), shape["keys"]);
    assert_eq!(
        mask(&Value::Object(snap.clone()), &["timestamp", "checksum"]),
        shape["masked"]
    );
    assert_eq!(json!(verify_checksum(&snap)), shape["valid"]);
    let none = m
        .create_snapshot(
            &json!({"nodes": []}),
            "meta-none",
            "a@b.co",
            "metadata None",
            &SnapshotOptions {
                metadata: Some(Value::Null),
            },
        )
        .unwrap();
    assert_eq!(none["metadata"], expected["create_metadata_none"]);
}

#[test]
fn test_temporal_tags_mutations_attach_and_restore_match_oracle() {
    let expected = &oracle()["temporal"];
    let t = TemporalVersionManager::new(None, None).unwrap();
    t.create_snapshot(&json!({"entities": []}), "v1", "a@b.co", "d", &opts())
        .unwrap();
    assert_eq!(outcome(t.tag_version("v99", "x")), expected["tag_errors"]);
    t.tag_version("v1", "production-ready").unwrap();
    t.tag_version("v1", "beta").unwrap();
    assert_eq!(json!(t.list_tags().unwrap()), expected["tags"]);

    t.record_mutation(
        "ADD_NODE",
        "n1",
        &obj(json!({"properties": {"age": 30}})),
        None,
    )
    .unwrap();
    t.record_mutation(
        "UPDATE_NODE",
        "n1",
        &obj(json!({"properties": {"age": 31}})),
        Some("pinned"),
    )
    .unwrap();
    t.create_snapshot(&json!({"entities": []}), "v2", "a@b.co", "d", &opts())
        .unwrap();
    t.record_mutation("REMOVE_NODE", "n1", &Map::new(), None)
        .unwrap();
    assert_eq!(
        mask(&json!(t.get_node_history("n1").unwrap()), &TIME_KEYS),
        expected["node_history"]
    );
    assert_eq!(
        json!(t.get_node_history("other").unwrap()),
        expected["node_history_other"]
    );

    // attach_to_graph installs record_mutation as the graph's callback.
    let mut g = FakeGraph {
        graph_id: Some("graph-1".into()),
        ..FakeGraph::default()
    };
    t.attach_to_graph(&mut g);
    assert_eq!(
        expected["attached_callback_is_record_mutation"],
        json!(true)
    );
    let callback = g.callback.clone().expect("callback installed");
    callback("ADD_EDGE", "e1", &obj(json!({"w": 1})), None).unwrap();
    assert_eq!(
        mask(&json!(t.get_node_history("e1").unwrap()), &TIME_KEYS),
        expected["callback_history"]
    );

    let restore_errors = &expected["restore_errors"];
    assert_eq!(
        outcome(t.restore_snapshot(&mut g, "v1", None)),
        restore_errors["protected"]
    );
    assert_eq!(
        outcome(t.restore_snapshot(&mut g, "v1", Some(true))),
        restore_errors["protected_explicit"]
    );
    assert_eq!(
        outcome(t.restore_snapshot(&mut g, "nope", Some(false))),
        restore_errors["missing"]
    );
    t.create_snapshot(
        &json!({"nodes": [{"id": "r1"}], "edges": [{"source": "r1", "target": "r1"}]}),
        "restore-me",
        "a@b.co",
        "restore point",
        &opts(),
    )
    .unwrap();
    g.suspend = false;
    let restored = t
        .restore_snapshot(&mut g, "restore-me", Some(false))
        .unwrap();
    let restore = &expected["restore"];
    assert_eq!(json!(restored), restore["result"]);
    assert_eq!(json!(g.loaded), restore["loaded"]);
    assert_eq!(json!(g.suspend_seen), restore["suspend_seen"]);
    assert_eq!(json!(g.suspend), restore["suspend_after"]);

    let mut g2 = FakeGraph {
        suspend: true,
        ..FakeGraph::default()
    };
    t.restore_snapshot(&mut g2, "restore-me", Some(false))
        .unwrap();
    assert_eq!(json!(g2.suspend), expected["restore_keeps_previous_flag"]);
}

fn prune_case(snapshots: &[Value], keep: Option<i64>, store_fail: Option<bool>) -> Value {
    let p = TemporalVersionManager::new(None, None).unwrap();
    for s in snapshots {
        p.storage.save(s.as_object().unwrap()).unwrap();
    }
    let store = store_fail.map(|fail| FakeStore {
        queries: RefCell::new(Vec::new()),
        fail,
    });
    let result =
        outcome(p.prune_versions(keep, store.as_ref().map(|s| s as &dyn TripletStoreQuery)));
    let remaining: Vec<Value> = p
        .list_versions()
        .unwrap()
        .iter()
        .map(|v| v["label"].clone())
        .collect();
    json!({
        "result": result,
        "queries": store.map(|s| s.queries.into_inner()),
        "remaining": remaining,
    })
}

#[test]
fn test_temporal_prune_versions_matches_oracle() {
    let expected = &oracle()["temporal"]["prune"];
    let mut snaps: Vec<Value> = (1..8)
        .map(|i| {
            json!({"label": format!("s{i}"), "timestamp": format!("2024-01-0{i}T00:00:00"),
                   "graph_uri": format!("http://ex.org/g/{i}")})
        })
        .collect();
    snaps[2]["graph_uri"] = json!("http://example.org/graph> } ; DROP ALL ; #");
    snaps[3]["graph_uri"] = json!("  <urn:x:y z>  ");
    snaps[4]["graph_uri"] = json!("http://ex.org/\u{e9} g?q=1&r=[2]#f%20");
    snaps[5]["graph_uri"] = json!("");
    snaps[6].as_object_mut().unwrap().remove("graph_uri");

    assert_eq!(prune_case(&snaps, None, Some(false)), expected["default"]);
    assert_eq!(prune_case(&snaps, Some(1), Some(false)), expected["keep_1"]);
    assert_eq!(prune_case(&snaps, Some(0), Some(false)), expected["keep_0"]);
    assert_eq!(
        prune_case(&snaps, Some(-2), Some(false)),
        expected["keep_negative"]
    );
    assert_eq!(
        prune_case(&snaps, Some(10), Some(false)),
        expected["keep_more"]
    );
    assert_eq!(prune_case(&snaps, Some(2), None), expected["no_store"]);
    assert_eq!(
        prune_case(&snaps, Some(5), Some(true)),
        expected["failing_store"]
    );
    let ties = [
        json!({"label": "t1", "timestamp": "x"}),
        json!({"label": "t2", "timestamp": "x"}),
        json!({"label": "t3", "timestamp": "y"}),
    ];
    assert_eq!(prune_case(&ties, Some(1), Some(false)), expected["ties"]);
    let missing = [
        json!({"label": "m1"}),
        json!({"label": "m2", "timestamp": "2024"}),
    ];
    assert_eq!(
        prune_case(&missing, Some(1), Some(false)),
        expected["missing_timestamp"]
    );
    assert_eq!(
        prune_case(&[json!({"label": "m1"})], Some(0), Some(false)),
        expected["single_missing_timestamp"]
    );
    let numbers = [
        json!({"label": "n1", "timestamp": "1", "graph_uri": 5}),
        json!({"label": "n2", "timestamp": "2", "graph_uri": 0}),
    ];
    assert_eq!(
        prune_case(&numbers, Some(0), Some(false)),
        expected["graph_uri_number"]
    );
}

#[test]
fn test_temporal_manager_with_sqlite_storage_matches_oracle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("temporal.db");
    let path = path.to_str().unwrap();
    let m = TemporalVersionManager::new(Some(path), None).unwrap();
    m.create_snapshot(
        &json!({"entities": [{"id": "x"}]}),
        "s1",
        "a@b.co",
        "one",
        &opts(),
    )
    .unwrap();
    m.record_mutation("ADD_NODE", "x", &obj(json!({"p": 1})), None)
        .unwrap();
    m.tag_version("s1", "t").unwrap();
    let m2 = TemporalVersionManager::new(Some(path), None).unwrap();
    let labels: Vec<Value> = m2
        .list_versions()
        .unwrap()
        .iter()
        .map(|v| v["label"].clone())
        .collect();
    let expected = &oracle()["temporal_sqlite"];
    assert_eq!(Value::Array(labels), expected["versions"]);
    assert_eq!(json!(m2.list_tags().unwrap()), expected["tags"]);
    assert_eq!(
        mask(&json!(m2.get_node_history("x").unwrap()), &TIME_KEYS),
        expected["history"]
    );
    assert_eq!(
        json!(m2.verify_checksum(&m2.get_version("s1").unwrap().unwrap())),
        expected["valid"]
    );
}

#[test]
fn test_ontology_version_manager_matches_oracle() {
    let expected = &oracle()["ontology_manager"];
    let m = OntologyVersionManager::new(None, None).unwrap();
    assert_eq!(json!(m.versions()), expected["initial_versions"]);
    let o1 = obj(
        json!({"uri": "https://example.com/ontology", "version_info": {"version": "1.0"},
        "structure": {"classes": ["Person", "Organization", "Person"], "properties": ["name", "email"],
                      "individuals": ["john_doe"], "axioms": ["Person hasName exactly 1 string"]}}),
    );
    let o2 = obj(
        json!({"uri": "https://example.com/ontology/v2", "version_info": {"version": "2.0"},
        "structure": {"classes": ["Person", "Project"], "properties": ["name", "email", "description"],
                      "individuals": ["john_doe", "acme_corp"], "axioms": ["Project hasDescription some string"]}}),
    );
    let s1 = m
        .create_snapshot(&o1, "v1.0", "test@example.com", "Version 1", &opts())
        .unwrap();
    m.create_snapshot(
        &o2,
        "v2.0",
        "test@example.com",
        "Version 2",
        &SnapshotOptions::with_metadata(json!({"reviewed": true})),
    )
    .unwrap();
    let keys: Vec<&String> = s1.keys().collect();
    assert_eq!(json!(keys), expected["snapshot"]["keys"]);
    assert_eq!(
        mask(&Value::Object(s1.clone()), &["timestamp", "checksum"]),
        expected["snapshot"]["masked"]
    );
    assert_eq!(json!(verify_checksum(&s1)), expected["snapshot"]["valid"]);
    let version_keys: Vec<String> = m.versions().keys().cloned().collect();
    assert_eq!(json!(version_keys), expected["versions_keys"]);

    let compare = |a: &str, b: &str| outcome(m.compare_versions(a, b));
    let check = |actual: Value, expected: &Value, context: &str| match expected.get("ok") {
        Some(ok) => assert_map_with_sets(&actual["ok"], ok, &STRUCTURE_LISTS, context),
        None => assert_map_with_sets(&actual, expected, &STRUCTURE_LISTS, context),
    };
    check(
        compare("v1.0", "v2.0")["ok"].clone(),
        &expected["compare"],
        "compare",
    );
    check(
        compare("v1.0", "v1.0")["ok"].clone(),
        &expected["compare_same"],
        "same",
    );

    m.create_snapshot(
        &Map::new(),
        "empty",
        "test@example.com",
        "Empty ontology",
        &opts(),
    )
    .unwrap();
    assert_eq!(
        mask(
            &json!(m.get_version("empty").unwrap()),
            &["timestamp", "checksum"]
        ),
        expected["empty_snapshot"]
    );
    check(
        compare("empty", "v1.0")["ok"].clone(),
        &expected["compare_empty"],
        "empty",
    );

    for (data, label) in [
        (
            json!({"structure": {"classes": "AB", "properties": null}}),
            "odd",
        ),
        (json!({"structure": {"classes": [["x"]]}}), "unhashable"),
        (json!({"structure": null}), "nostructure"),
        (
            json!({"structure": {"classes": [1, 1.0, true, "1"]}}),
            "numbers",
        ),
    ] {
        m.create_snapshot(
            data.as_object().unwrap(),
            label,
            "test@example.com",
            "Odd",
            &opts(),
        )
        .unwrap();
    }
    let errors = &expected["errors"];
    assert_eq!(compare("nonexistent1", "nonexistent2"), errors["missing"]);
    assert_eq!(compare("v1.0", "nonexistent2"), errors["missing_second"]);
    assert_eq!(compare("odd", "v1.0"), errors["none_section"]);
    assert_eq!(compare("v1.0", "odd"), errors["string_section"]);
    assert_eq!(compare("unhashable", "v1.0"), errors["unhashable"]);
    assert_eq!(compare("nostructure", "v1.0"), errors["structure_none"]);
    assert_eq!(
        outcome(m.create_snapshot(&o1, "x", "bad", "d", &opts())),
        errors["author"]
    );
    assert_eq!(
        outcome(m.create_snapshot(&o1, "v1.0", "a@b.co", "d", &opts())),
        errors["duplicate"]
    );
    check(
        compare("numbers", "v1.0")["ok"].clone(),
        &expected["numbers_vs_v1"],
        "numbers",
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ontology.db");
    let p1 = OntologyVersionManager::new(Some(path.to_str().unwrap()), None).unwrap();
    p1.create_snapshot(
        &o1,
        "persistent_v1.0",
        "test@example.com",
        "Persistent test",
        &opts(),
    )
    .unwrap();
    let p2 = OntologyVersionManager::new(Some(path.to_str().unwrap()), None).unwrap();
    let got = p2.get_version("persistent_v1.0").unwrap().unwrap();
    let persistence = &expected["persistence"];
    assert_eq!(got["label"], persistence["label"]);
    assert_eq!(got["ontology_iri"], persistence["ontology_iri"]);
    assert_eq!(json!(p2.versions()), persistence["versions"]);
}

/// The abstract `BaseVersionManager` contract, through trait objects for both managers.
#[test]
fn test_base_version_manager_trait_objects() {
    let data = json!({"entities": [{"id": "a"}], "uri": "urn:o", "structure": {"classes": ["A"]}});
    let managers: Vec<Box<dyn BaseVersionManager>> = vec![
        Box::new(TemporalVersionManager::new(None, None).unwrap()),
        Box::new(OntologyVersionManager::new(None, None).unwrap()),
    ];
    for manager in &managers {
        let snapshot = manager
            .create_snapshot(&data, "v1", "a@b.co", "first", &opts())
            .unwrap();
        assert!(manager.verify_checksum(&snapshot));
        assert_eq!(manager.list_versions().unwrap().len(), 1);
        assert_eq!(manager.get_version("v1").unwrap(), Some(snapshot));
        assert!(manager.storage().exists("v1").unwrap());
        let diff = manager
            .compare_versions(&json!("v1"), &json!("v1"))
            .unwrap();
        assert_eq!(diff["version1"], "v1");
        let err = manager
            .create_snapshot(&json!([1]), "v2", "bad-author", "d", &opts())
            .unwrap_err();
        assert!(err.to_string().starts_with("Invalid email format"));
    }
    // Ontology data must be a dict: AttributeError once the metadata is valid.
    let err = managers[1]
        .create_snapshot(&json!([1]), "v2", "a@b.co", "d", &opts())
        .unwrap_err();
    assert_eq!(err.to_string(), "'list' object has no attribute 'get'");
    let err = managers[1]
        .compare_versions(&json!({}), &json!("v1"))
        .unwrap_err();
    assert_eq!(err.to_string(), "unhashable type: 'dict'");
}

#[test]
fn test_init_storage_picks_backend() {
    let logger = sem_utils::get_logger("init_storage_test");
    let memory = init_storage(None, &logger).unwrap();
    let empty_path = init_storage(Some(""), &logger).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("v.db");
    let sqlite = init_storage(Some(path.to_str().unwrap()), &logger).unwrap();
    assert!(path.exists());
    for storage in [memory, empty_path, sqlite] {
        storage.save(&obj(json!({"label": "x"}))).unwrap();
        assert!(storage.exists("x").unwrap());
    }
    // TemporalVersionManager config keywords are kept.
    let m = TemporalVersionManager::new(None, Some(obj(json!({"retention": 3})))).unwrap();
    assert_eq!(m.config["retention"], 3);
    assert!(format!("{m:?}").contains("retention"));
}
