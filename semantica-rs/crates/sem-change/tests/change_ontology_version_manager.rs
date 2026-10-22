//! Characterisation tests for `ontology_version_manager.rs`
//! (`semantica/change_management/ontology_version_manager.py`): version records, versioned
//! IRIs, comparison, diffs, migration and import resolution. Expected values come from the
//! oracle (`tests/fixtures/oracle.json`).

mod common;

use common::{assert_map_with_sets, mask, obj, oracle, outcome, quiet_progress};
use sem_change::namespace_manager::NamespaceManager;
use sem_change::ontology_version_manager::CreateVersionOptions;
use sem_change::{OntologyVersion, VersionManager};
use sem_utils::pycompat::fmt::py_str;
use serde_json::{Map, Value, json};

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

fn record(version: &OntologyVersion) -> Value {
    mask(
        &serde_json::to_value(version).unwrap(),
        &["timestamp", "created_at"],
    )
}

fn sorted_strings(values: &[Value]) -> Value {
    let mut strings: Vec<String> = values.iter().map(py_str).collect();
    strings.sort();
    json!(strings)
}

#[test]
fn test_version_manager_matches_oracle() {
    let expected = &oracle()["version_manager"];
    let mut vm = VersionManager::new(
        Some(obj(json!({"base_uri": "https://example.org/ontology/"}))),
        None,
    );
    quiet_progress();

    let mut onto = obj(json!({"name": "Test"}));
    let v1 = vm
        .create_version(
            "1.0",
            &mut onto,
            &CreateVersionOptions {
                changes: Some(vec!["Added Person class".into()]),
                metadata: Some(obj(json!({
                    "structure": {"classes": ["Person", "Org"], "properties": ["name"], "individuals": [], "axioms": []},
                    "imports": ["http://ex.org/dep/v1.0", "http://ex.org/other"]}))),
            },
        )
        .unwrap();
    assert_eq!(record(&v1), expected["create_v1"]["record"]);
    assert_eq!(Value::Object(onto), expected["create_v1"]["ontology"]);

    let mut onto2 = obj(json!({"uri": "https://example.org/custom/", "x": 1}));
    let v2 = vm
        .create_version(
            "2.0",
            &mut onto2,
            &CreateVersionOptions {
                changes: None,
                metadata: Some(obj(json!({"structure": {"classes": ["Person", "Project"],
                    "properties": ["name", "email"], "individuals": ["i1"], "axioms": ["ax"]}}))),
            },
        )
        .unwrap();
    assert_eq!(record(&v2), expected["create_v2"]["record"]);
    assert_eq!(Value::Object(onto2), expected["create_v2"]["ontology"]);

    let mut onto3 = obj(json!({"uri": "https://example.org/custom/v3.0"}));
    let v3 = vm
        .create_version("3.0", &mut onto3, &CreateVersionOptions::default())
        .unwrap();
    assert_eq!(record(&v3), expected["create_v3"]["record"]);
    assert_eq!(Value::Object(onto3), expected["create_v3"]["ontology"]);

    let errors = &expected["create_errors"];
    let mut bad = obj(json!({"uri": 5}));
    assert_eq!(
        outcome(
            vm.create_version("9.0", &mut bad, &CreateVersionOptions::default())
                .map(|v| record(&v))
        ),
        errors["uri_number"]
    );
    let mut bad = obj(json!({"uri": ["x"]}));
    assert_eq!(
        outcome(
            vm.create_version("9.1", &mut bad, &CreateVersionOptions::default())
                .map(|v| record(&v))
        ),
        errors["uri_list"]
    );
    assert_eq!(
        json!(vm.list_versions()),
        expected["after_errors"]["versions"]
    );
    assert_eq!(json!(vm.latest_version), expected["after_errors"]["latest"]);

    assert_eq!(json!(vm.list_versions()), expected["list_versions"]);
    assert_eq!(
        json!([
            record(vm.get_version("1.0").unwrap()),
            vm.get_version("nope")
        ]),
        expected["get_version"]
    );
    assert_eq!(record(vm.get_latest_version().unwrap()), expected["latest"]);

    assert_map_with_sets(
        &Value::Object(vm.compare_versions("1.0", "2.0").unwrap()),
        &expected["compare"],
        &STRUCTURE_LISTS,
        "compare",
    );
    assert_map_with_sets(
        &Value::Object(vm.compare_versions("2.0", "3.0").unwrap()),
        &expected["compare_3"],
        &STRUCTURE_LISTS,
        "compare_3",
    );
    assert_eq!(
        json!([
            outcome(vm.compare_versions("x", "1.0")),
            outcome(vm.compare_versions("1.0", "y"))
        ]),
        expected["compare_errors"]
    );

    let mut iris = Vec::new();
    for name in ["Person", "has name", "John Doe!"] {
        for kind in ["class", "property", "individual", "other"] {
            iris.push(json!([
                name,
                kind,
                outcome(vm.generate_element_iri(name, Some(kind)))
            ]));
        }
    }
    assert_eq!(Value::Array(iris), expected["element_iris"]);
    assert_eq!(
        json!(vm.generate_element_iri("Person", None).unwrap()),
        expected["element_iri_default"]
    );

    let mut mig = obj(json!({"version": "1.0", "uri": "x"}));
    let migrated = vm.migrate_ontology("1.0", "2.0", &mut mig).unwrap();
    assert_eq!(Value::Object(migrated), expected["migrate"]["result"]);
    assert_eq!(Value::Object(mig), expected["migrate"]["mutated"]);
    assert_eq!(
        json!([
            outcome(vm.migrate_ontology("x", "1.0", &mut Map::new())),
            outcome(vm.migrate_ontology("1.0", "y", &mut Map::new()))
        ]),
        expected["migrate_errors"]
    );

    let imports = &expected["imports"];
    let resolve = |o: Value| vm.resolve_versioned_imports(o.as_object().unwrap());
    assert_eq!(
        sorted_strings(
            &resolve(json!({"imports": ["http://ex.org/a/v1.0", "http://ex.org/b"]})).unwrap()
        ),
        imports["basic"]
    );
    assert_eq!(
        sorted_strings(
            &resolve(json!({"imports": ["http://ex.org/x/v1.0/y", "http://ex.org/x/v1.0/y"]}))
                .unwrap()
        ),
        imports["transitive"]
    );
    assert_eq!(
        json!(resolve(json!({"imports": ["http://ex.org/v9.9"]})).unwrap()),
        imports["unknown_version"]
    );
    assert_eq!(
        json!(resolve(json!({"imports": ["http://ex.org/v1"]})).unwrap()),
        imports["no_minor"]
    );
    assert_eq!(json!(resolve(json!({})).unwrap()), imports["none"]);
    assert_eq!(outcome(resolve(json!({"imports": [5]}))), imports["number"]);
    assert_eq!(
        outcome(resolve(json!({"imports": [["x"]]}))),
        imports["unhashable"]
    );
    assert_eq!(
        outcome(resolve(json!({"imports": null}))),
        imports["not_iterable"]
    );

    for case in expected["diff"].as_array().unwrap() {
        let result = outcome(vm.diff_ontologies(
            case["base"].as_object().unwrap(),
            case["target"].as_object().unwrap(),
        ));
        assert_eq!(
            result, case["result"],
            "diff {} -> {}",
            case["base"], case["target"]
        );
    }
}

#[test]
fn test_version_manager_with_custom_namespace_manager_and_config() {
    quiet_progress();
    let nm = NamespaceManager::new(Some(obj(
        json!({"base_uri": "https://custom.org/", "version": "7"}),
    )));
    let mut vm = VersionManager::new(None, Some(nm));
    let expected = &oracle()["version_manager_custom_ns"];
    assert_eq!(
        json!(vm.generate_element_iri("Thing", None).unwrap()),
        expected["element"]
    );
    let created = vm
        .create_version("8", &mut Map::new(), &CreateVersionOptions::default())
        .unwrap();
    assert_eq!(record(&created), expected["create"]);

    let mut vm = VersionManager::new(
        Some(obj(
            json!({"base_uri": "https://cfg.org/o", "version": "2.0"}),
        )),
        None,
    );
    let expected = &oracle()["version_manager_config"];
    assert_eq!(Value::Object(vm.config.clone()), expected["config"]);
    assert_eq!(
        json!(vm.namespace_manager.get_base_uri().unwrap()),
        expected["ns_base"]
    );
    assert_eq!(
        json!(
            vm.generate_element_iri("has value", Some("property"))
                .unwrap()
        ),
        expected["element"]
    );
    let created = vm
        .create_version("2.0", &mut Map::new(), &CreateVersionOptions::default())
        .unwrap();
    assert_eq!(record(&created), expected["create"]);
    assert!(format!("{vm:?}").contains("VersionManager"));
    assert!(VersionManager::default().versions.is_empty());
}

#[test]
fn test_ontology_version_dataclass() {
    let version = OntologyVersion::new("1", "iri", "1", "now", None, None);
    assert_eq!(
        serde_json::to_value(&version).unwrap(),
        oracle()["ontology_version_dataclass"]
    );
    let back: OntologyVersion =
        serde_json::from_value(oracle()["ontology_version_dataclass"].clone()).unwrap();
    assert_eq!(back, version);
}
