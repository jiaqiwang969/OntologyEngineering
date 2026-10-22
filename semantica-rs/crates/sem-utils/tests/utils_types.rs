//! Characterisation of `semantica/utils/types.py` (expected values from the oracle).

mod common;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use common::{err_json, obj, oracle};
use sem_utils::types::{
    BatchResult, DataType, Entity, EntityType, Exporter, Importer, ProcessingResult,
    ProcessingStatus, Processor, QualityLevel, QualityMetrics, Relationship, RelationshipType,
    Validator, dict_to_entity, dict_to_relationship, entity_to_dict, is_entity_dict,
    is_processing_result, is_relationship_dict, relationship_to_dict,
};
use serde_json::{Map, Value, json};

fn enum_rows<E: Copy + std::fmt::Display>(
    all: &[E],
    name: fn(E) -> &'static str,
    value: fn(E) -> &'static str,
) -> Value {
    Value::Array(
        all.iter()
            .map(|m| {
                json!({"name": name(*m), "value": value(*m), "str": m.to_string(), "format": format!("{m}")})
            })
            .collect(),
    )
}

#[test]
fn test_enums_match_oracle() {
    let enums = oracle("enums");
    assert_eq!(
        enum_rows(EntityType::ALL, EntityType::name, EntityType::value),
        enums["EntityType"]
    );
    assert_eq!(
        enum_rows(DataType::ALL, DataType::name, DataType::value),
        enums["DataType"]
    );
    assert_eq!(
        enum_rows(
            ProcessingStatus::ALL,
            ProcessingStatus::name,
            ProcessingStatus::value
        ),
        enums["ProcessingStatus"]
    );
    assert_eq!(
        enum_rows(QualityLevel::ALL, QualityLevel::name, QualityLevel::value),
        enums["QualityLevel"]
    );
    assert_eq!(
        enum_rows(
            RelationshipType::ALL,
            RelationshipType::name,
            RelationshipType::value
        ),
        enums["RelationshipType"]
    );
    // str-enum behaviour: members compare equal to their values and round-trip.
    assert!(EntityType::Person == "PERSON");
    assert_eq!(
        ProcessingStatus::from_value("paused"),
        Some(ProcessingStatus::Paused)
    );
    assert_eq!(QualityLevel::from_value("HIGH"), None);
    assert_eq!(serde_json::to_value(DataType::Text).unwrap(), json!("text"));
    assert_eq!(
        serde_json::from_value::<RelationshipType>(json!("WORKS_FOR")).unwrap(),
        RelationshipType::WorksFor
    );
}

fn example_entity() -> Entity {
    Entity {
        id: "e1".into(),
        text: "John Doe".into(),
        r#type: EntityType::Person.into(),
        confidence: 0.95,
        start: Some(0),
        end: Some(8),
        metadata: obj(&json!({"src": "doc"})),
        relations: vec![obj(&json!({"id": "r1"}))],
    }
}

#[test]
fn test_entity_and_relationship_conversions_match_oracle() {
    assert_eq!(
        Value::Object(entity_to_dict(&example_entity())),
        *oracle("entity_to_dict")
    );
    assert_eq!(
        Value::Object(entity_to_dict(&Entity::new("e2", "Acme", "ORG"))),
        *oracle("entity_to_dict_str_type")
    );
    let rel = Relationship {
        id: "r1".into(),
        source_id: "e1".into(),
        target_id: "e2".into(),
        r#type: RelationshipType::WorksFor.into(),
        confidence: 0.8,
        metadata: obj(&json!({"m": 1})),
        properties: obj(&json!({"since": 2020})),
    };
    assert_eq!(
        Value::Object(relationship_to_dict(&rel)),
        *oracle("relationship_to_dict")
    );
}

#[test]
fn test_dict_conversions_match_oracle() {
    let inputs = [
        json!({"id": "e1", "text": "t", "type": "PERSON"}),
        json!({"id": "e1", "text": "t", "type": "PERSON", "confidence": 0.5, "start": 1, "end": 2,
               "metadata": {"a": 1}, "relations": [{"id": "r"}]}),
        json!({"text": "t", "type": "X"}),
        json!({"id": "e1", "type": "X"}),
    ];
    for (input, expected) in inputs
        .iter()
        .zip(oracle("dict_to_entity").as_array().unwrap())
    {
        let got = match dict_to_entity(&obj(input)) {
            Ok(entity) => json!({"ok": entity_to_dict(&entity)}),
            Err(e) => err_json(&e),
        };
        assert_eq!(got, *expected, "dict_to_entity({input})");
    }
    let inputs = [
        json!({"id": "r1", "source_id": "a", "target_id": "b", "type": "REL"}),
        json!({"id": "r1", "source_id": "a", "target_id": "b", "type": "REL", "confidence": 0.3,
               "metadata": {"x": 1}, "properties": {"p": true}}),
        json!({"id": "r1", "source_id": "a", "type": "REL"}),
    ];
    for (input, expected) in inputs
        .iter()
        .zip(oracle("dict_to_relationship").as_array().unwrap())
    {
        let got = match dict_to_relationship(&obj(input)) {
            Ok(rel) => json!({"ok": relationship_to_dict(&rel)}),
            Err(e) => err_json(&e),
        };
        assert_eq!(got, *expected, "dict_to_relationship({input})");
    }
    // Values a typed record cannot hold are a TypeError (Python stores them unchecked).
    let err = dict_to_entity(&obj(&json!({"id": 5, "text": "t", "type": "X"}))).unwrap_err();
    assert_eq!(err.type_name(), "TypeError");
}

#[test]
fn test_type_guards_match_oracle() {
    for case in oracle("type_guards").as_array().unwrap() {
        let value = &case[0];
        assert_eq!(is_entity_dict(value), case[1].as_bool().unwrap(), "{value}");
        assert_eq!(
            is_relationship_dict(value),
            case[2].as_bool().unwrap(),
            "{value}"
        );
        assert_eq!(
            is_processing_result(value),
            case[3].as_bool().unwrap(),
            "{value}"
        );
    }
}

fn hash_of<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn test_entity_equality_matches_oracle() {
    let expected = oracle("entity_eq").as_array().unwrap();
    let ent = example_entity();
    let got = [
        ent == Entity::new("e1", "other", "X"),
        ent == Entity::new("e2", "John Doe", "PERSON"),
        ent == obj(&json!({"id": "e1"})),
        ent == obj(&json!({"entity_id": "e1"})),
        ent == obj(&json!({"id": "", "entity_id": "e1"})),
        ent == obj(&json!({"id": "e2"})),
    ];
    for (i, g) in got.iter().enumerate() {
        assert_eq!(*g, expected[i].as_bool().unwrap(), "case {i}");
    }
    // ent == "e1" is False in Python; Rust does not compare an Entity with a string.
    assert!(!expected[6].as_bool().unwrap());
    assert_eq!(
        hash_of(&ent) == hash_of("e1"),
        expected[7].as_bool().unwrap()
    );
}

#[test]
fn test_result_and_batch_result_match_oracle() {
    let fixture = oracle("result");
    let ok = sem_utils::Result::ok(5);
    let err: sem_utils::Result<i32> = sem_utils::Result::err("failed");
    let none: sem_utils::Result<i32> = sem_utils::Result::new(None, None);
    assert_eq!(
        json!([
            format!("{ok:?}"),
            ok.as_bool(),
            ok.success,
            ok.value,
            ok.error
        ]),
        fixture["ok"]
    );
    assert_eq!(
        json!([
            format!("{err:?}"),
            err.as_bool(),
            err.success,
            err.value,
            err.error
        ]),
        fixture["err"]
    );
    assert_eq!(
        json!([format!("{none:?}"), none.as_bool()]),
        fixture["none"]
    );
    let batch = BatchResult::new(vec![
        ok.clone(),
        err.clone(),
        sem_utils::Result::ok(7),
        none,
    ]);
    assert_eq!(
        json!([
            batch.success_count(),
            batch.failure_count(),
            batch.total_count(),
            batch.success_rate()
        ]),
        fixture["batch"]
    );
    assert_eq!(batch.successful().len(), 3);
    assert_eq!(batch.failed(), vec![&err]);
    assert_eq!(
        json!(BatchResult::<i32>::new(vec![]).success_rate()),
        fixture["empty_batch_rate"]
    );
}

#[test]
fn test_dataclass_defaults_match_oracle() {
    let fixture = oracle("dataclass_defaults");
    assert_eq!(
        Value::Object(entity_to_dict(&Entity::new("i", "t", "x"))),
        fixture["entity"]
    );
    assert_eq!(
        Value::Object(relationship_to_dict(&Relationship::new("r", "a", "b", "t"))),
        fixture["relationship"]
    );
    let pr = ProcessingResult::new(ProcessingStatus::Completed);
    assert_eq!(
        json!([
            pr.status.value(),
            pr.data,
            pr.entities,
            pr.relationships,
            pr.metadata,
            pr.error,
            pr.execution_time
        ]),
        fixture["processing_result"]
    );
    let qm = QualityMetrics::new(0.9);
    assert_eq!(
        json!([
            qm.score,
            qm.completeness,
            qm.accuracy,
            qm.consistency,
            qm.validity,
            qm.timeliness,
            qm.metadata
        ]),
        fixture["quality_metrics"]
    );
}

/// The protocols are interfaces with no behaviour of their own (`...` bodies); an
/// implementation is called through the trait.
struct Echo;

impl Processor for Echo {
    fn process(&self, data: &Value, _options: &Map<String, Value>) -> ProcessingResult {
        let mut result = ProcessingResult::new(ProcessingStatus::Completed);
        result.data = Some(data.clone());
        result
    }
}

impl Validator for Echo {
    fn validate(&self, data: &Value, _options: &Map<String, Value>) -> bool {
        !data.is_null()
    }
}

impl Exporter for Echo {
    fn export(&self, _data: &Value, destination: &str, _options: &Map<String, Value>) -> bool {
        destination.ends_with(".json")
    }
}

impl Importer for Echo {
    fn import_data(&self, source: &str, _options: &Map<String, Value>) -> Value {
        json!({"source": source})
    }
}

#[test]
fn test_protocols_dispatch_to_implementations() {
    let processor: &dyn Processor = &Echo;
    let validator: &dyn Validator = &Echo;
    let exporter: &dyn Exporter = &Echo;
    let importer: &dyn Importer = &Echo;
    let options = Map::new();
    let result = processor.process(&json!({"x": 1}), &options);
    assert_eq!(result.status, ProcessingStatus::Completed);
    assert_eq!(result.data, Some(json!({"x": 1})));
    assert!(validator.validate(&json!(1), &options));
    assert!(!validator.validate(&Value::Null, &options));
    assert!(exporter.export(&json!([]), "out.json", &options));
    assert_eq!(
        importer.import_data("in.csv", &options),
        json!({"source": "in.csv"})
    );
}
