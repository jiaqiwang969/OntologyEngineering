//! Port of `tests/provenance/test_bridge_axiom.py`: bridge axiom translation chains across
//! high-stakes domains (`TestBridgeAxiomBasics`, `TestDomainSpecificAxioms`,
//! `TestTranslationChains`, `TestProvenanceIntegration`, `TestEdgeCases`).
#![allow(non_snake_case)]

mod common;

use common::{approx, obj};
use sem_provenance::ProvenanceManager;
use sem_provenance::bridge_axiom::{BridgeAxiom, TranslationChain, create_translation_chain};
use serde_json::{Map, Value, json};

fn no_kwargs() -> Map<String, Value> {
    Map::new()
}

fn axiom(
    axiom_id: &str,
    name: &str,
    rule: &str,
    coefficient: f64,
    source_doi: &str,
    source_page: &str,
) -> BridgeAxiom {
    BridgeAxiom::new(axiom_id, name, rule, coefficient, source_doi, source_page)
}

fn f(value: &Value) -> f64 {
    value.as_f64().expect("numeric value")
}

// --- TestBridgeAxiomBasics ---

#[test]
fn test_bridge_axiom_basics__test_create_bridge_axiom() {
    let ba = axiom(
        "BA-TEST-001",
        "test_axiom",
        "Test rule",
        0.5,
        "10.1234/test",
        "Page 1",
    );
    assert_eq!(ba.axiom_id, "BA-TEST-001");
    assert_eq!(ba.name, "test_axiom");
    assert_eq!(ba.coefficient, 0.5);
    assert_eq!(ba.confidence, 1.0);
}

#[test]
fn test_bridge_axiom_basics__test_bridge_axiom_with_domains() {
    let ba = BridgeAxiom {
        input_domain: "domain_a".into(),
        output_domain: "domain_b".into(),
        ..axiom(
            "BA-TEST-002",
            "domain_test",
            "Test rule",
            0.75,
            "10.1234/test",
            "Page 1",
        )
    };
    assert_eq!(ba.input_domain, "domain_a");
    assert_eq!(ba.output_domain, "domain_b");
}

#[test]
fn test_bridge_axiom_basics__test_bridge_axiom_apply_without_provenance() {
    let ba = axiom(
        "BA-TEST-003",
        "apply_test",
        "Test rule",
        2.0,
        "10.1234/test",
        "Page 1",
    );
    let result = ba.apply("test_entity", 10.0, None, &no_kwargs()).unwrap();
    assert_eq!(f(&result["output_value"]), 20.0);
    assert_eq!(f(&result["input_value"]), 10.0);
    assert_eq!(f(&result["coefficient"]), 2.0);
}

#[test]
fn test_bridge_axiom_basics__test_bridge_axiom_apply_with_provenance() {
    let prov_mgr = ProvenanceManager::new().unwrap();
    let ba = axiom(
        "BA-TEST-004",
        "prov_test",
        "Test rule",
        1.5,
        "10.1234/test",
        "Page 1",
    );
    let result = ba
        .apply("test_entity", 100.0, Some(&prov_mgr), &no_kwargs())
        .unwrap();
    assert_eq!(f(&result["output_value"]), 150.0);
    assert!(result.contains_key("output_entity"));

    // Verify provenance was tracked
    let lineage = prov_mgr.get_lineage(result["output_entity"].as_str().unwrap());
    assert!(lineage.is_ok());
}

// --- TestDomainSpecificAxioms ---

#[test]
fn test_domain_specific_axioms__test_blue_finance_axiom() {
    let ba = BridgeAxiom {
        input_domain: "ecological".into(),
        output_domain: "financial".into(),
        confidence: 0.92,
        ..axiom(
            "BA-FINANCE-001",
            "biomass_tourism_elasticity",
            "1% biomass increase → 0.346% tourism revenue increase",
            0.346,
            "10.1038/s41586-021-03371-z",
            "Table S4",
        )
    };
    let result = ba
        .apply("cabo_pulmo_biomass", 463, None, &no_kwargs())
        .unwrap();
    assert!(approx(f(&result["output_value"]), 160.098, 0.01));
    assert_eq!(result["input_domain"], "ecological");
    assert_eq!(result["output_domain"], "financial");
}

#[test]
fn test_domain_specific_axioms__test_healthcare_axiom() {
    let ba = BridgeAxiom {
        input_domain: "clinical_observation".into(),
        output_domain: "diagnostic_probability".into(),
        confidence: 0.85,
        ..axiom(
            "BA-HEALTH-001",
            "fever_influenza_correlation",
            "Fever >38°C increases influenza probability by 0.65",
            0.65,
            "10.1001/jama.2020.12345",
            "Table 2",
        )
    };
    let result = ba
        .apply("patient_123_fever", 38.5, None, &no_kwargs())
        .unwrap();
    assert!(approx(f(&result["output_value"]), 25.025, 0.01));
    assert_eq!(f(&result["confidence"]), 0.85);
}

#[test]
fn test_domain_specific_axioms__test_legal_axiom() {
    let ba = BridgeAxiom {
        input_domain: "forensic_evidence".into(),
        output_domain: "legal_conclusion".into(),
        confidence: 0.98,
        ..axiom(
            "BA-LEGAL-001",
            "dna_match_conviction",
            "DNA match increases conviction probability by 0.95",
            0.95,
            "10.1016/j.forsciint.2019.12345",
            "Section 4.2",
        )
    };
    let result = ba
        .apply("case_2026_001_dna", 1.0, None, &no_kwargs())
        .unwrap();
    assert_eq!(f(&result["output_value"]), 0.95);
    assert_eq!(f(&result["confidence"]), 0.98);
}

#[test]
fn test_domain_specific_axioms__test_pharmaceutical_axiom() {
    let ba = BridgeAxiom {
        input_domain: "drug_dosage".into(),
        output_domain: "clinical_efficacy".into(),
        confidence: 0.88,
        ..axiom(
            "BA-PHARMA-001",
            "dosage_efficacy_relationship",
            "10mg increase → 0.15 efficacy improvement",
            0.15,
            "10.1056/NEJMoa2020123",
            "Figure 3",
        )
    };
    let result = ba
        .apply("trial_phase3_dosage", 50, None, &no_kwargs())
        .unwrap();
    assert_eq!(f(&result["output_value"]), 7.5);
    assert_eq!(result["input_domain"], "drug_dosage");
    assert_eq!(result["output_domain"], "clinical_efficacy");
}

#[test]
fn test_domain_specific_axioms__test_finance_risk_axiom() {
    let ba = BridgeAxiom {
        input_domain: "market_volatility".into(),
        output_domain: "portfolio_risk".into(),
        confidence: 0.91,
        ..axiom(
            "BA-RISK-001",
            "volatility_risk_correlation",
            "1% volatility increase → 0.8 risk score increase",
            0.8,
            "10.1111/jofi.2020.12345",
            "Table 5",
        )
    };
    let result = ba
        .apply("portfolio_A_volatility", 15.3, None, &no_kwargs())
        .unwrap();
    assert!(approx(f(&result["output_value"]), 12.24, 0.01));
}

#[test]
fn test_domain_specific_axioms__test_cybersecurity_axiom() {
    let ba = BridgeAxiom {
        input_domain: "anomaly_detection".into(),
        output_domain: "threat_assessment".into(),
        confidence: 0.89,
        ..axiom(
            "BA-SECURITY-001",
            "anomaly_threat_correlation",
            "Anomaly score >0.7 increases threat level by 0.85",
            0.85,
            "10.1109/TDSC.2020.12345",
            "Algorithm 2",
        )
    };
    let result = ba
        .apply("network_anomaly", 0.82, None, &no_kwargs())
        .unwrap();
    assert!(approx(f(&result["output_value"]), 0.697, 0.01));
}

// --- TestTranslationChains ---

#[test]
fn test_translation_chains__test_create_translation_chain() {
    let chain = TranslationChain::new("chain_001");
    assert_eq!(chain.chain_id, "chain_001");
    assert_eq!(chain.layers.len(), 0);
    assert_eq!(chain.confidence, 1.0);
}

#[test]
fn test_translation_chains__test_add_layers_to_chain() {
    let mut chain = TranslationChain::new("chain_002");
    chain.add_layer("L1", "input", 100, Some("doc_1"), Map::new());
    chain.add_layer("L2", "bridge_axiom", 0.5, Some("axiom_1"), Map::new());
    chain.add_layer("L3", "output", 50, Value::Null, Map::new());

    assert_eq!(chain.layers.len(), 3);
    assert_eq!(chain.get_layer("L1").unwrap()["value"], json!(100));
    assert_eq!(chain.get_layer("L2").unwrap()["value"], json!(0.5));
    assert_eq!(chain.get_layer("L3").unwrap()["value"], json!(50));
}

#[test]
fn test_translation_chains__test_multi_axiom_chain() {
    let prov_mgr = ProvenanceManager::new().unwrap();
    let input_data = obj(json!({"entity_id": "test_input", "value": 100, "source": "test_source"}));
    let axioms = vec![
        axiom(
            "BA-001",
            "axiom_1",
            "Test rule 1",
            2.0,
            "10.1234/test1",
            "Page 1",
        ),
        axiom(
            "BA-002",
            "axiom_2",
            "Test rule 2",
            0.5,
            "10.1234/test2",
            "Page 2",
        ),
    ];
    let chain = create_translation_chain(&input_data, &axioms, Some(&prov_mgr)).unwrap();

    assert!(chain.layers.len() >= 3); // L1 + 2 axioms + final output

    // Verify final value: 100 * 2.0 * 0.5 = 100
    let final_layer = chain.layers.last().unwrap();
    assert_eq!(f(&final_layer["value"]), 100.0);
}

#[test]
fn test_translation_chains__test_confidence_propagation() {
    let prov_mgr = ProvenanceManager::new().unwrap();
    let input_data = obj(json!({"entity_id": "test_input", "value": 50, "source": "test_source"}));
    let axioms = vec![
        BridgeAxiom {
            confidence: 0.9,
            ..axiom("BA-001", "axiom_1", "Test", 1.0, "10.1234/test", "Page 1")
        },
        BridgeAxiom {
            confidence: 0.8,
            ..axiom("BA-002", "axiom_2", "Test", 1.0, "10.1234/test", "Page 2")
        },
    ];
    let chain = create_translation_chain(&input_data, &axioms, Some(&prov_mgr)).unwrap();

    // Chain confidence should be minimum of all axiom confidences
    assert_eq!(chain.confidence, 0.8);
}

// --- TestProvenanceIntegration ---

#[test]
fn test_provenance_integration__test_axiom_tracks_provenance() {
    let prov_mgr = ProvenanceManager::new().unwrap();
    let ba = BridgeAxiom {
        source_quote: Some("Test quote".into()),
        ..axiom(
            "BA-TEST-005",
            "prov_integration_test",
            "Test rule",
            1.5,
            "10.1234/test",
            "Page 1",
        )
    };
    let result = ba
        .apply("input_entity", 100, Some(&prov_mgr), &no_kwargs())
        .unwrap();

    // Check provenance was tracked
    let prov = prov_mgr
        .get_provenance(result["output_entity"].as_str().unwrap())
        .unwrap();
    assert!(prov.is_some());
    let prov = prov.unwrap();
    assert_eq!(prov["source_document"], "10.1234/test");
    assert_eq!(prov["metadata"]["axiom_id"], "BA-TEST-005");
}

#[test]
fn test_provenance_integration__test_chain_tracks_complete_lineage() {
    let prov_mgr = ProvenanceManager::new().unwrap();
    let input_data = obj(json!({"entity_id": "lineage_test", "value": 75, "source": "test_doc"}));
    let axioms = vec![
        axiom("BA-L1", "layer_1", "Test", 2.0, "10.1234/l1", "P1"),
        axiom("BA-L2", "layer_2", "Test", 0.5, "10.1234/l2", "P2"),
    ];
    let chain = create_translation_chain(&input_data, &axioms, Some(&prov_mgr)).unwrap();

    // Trace lineage
    let final_layer = chain.layers.last().unwrap();
    if let Some(final_entity) = final_layer.get("entity_id").and_then(Value::as_str) {
        let lineage = prov_mgr.get_lineage(final_entity).unwrap();
        let chain_len = lineage
            .get("lineage_chain")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        assert!(chain_len > 0);
    }
}

// --- TestEdgeCases ---

#[test]
fn test_edge_cases__test_zero_coefficient() {
    let ba = axiom(
        "BA-ZERO",
        "zero_test",
        "Test",
        0.0,
        "10.1234/test",
        "Page 1",
    );
    let result = ba.apply("test", 100, None, &no_kwargs()).unwrap();
    assert_eq!(f(&result["output_value"]), 0.0);
}

#[test]
fn test_edge_cases__test_negative_coefficient() {
    let ba = axiom(
        "BA-NEG",
        "negative_test",
        "Test",
        -0.5,
        "10.1234/test",
        "Page 1",
    );
    let result = ba.apply("test", 100, None, &no_kwargs()).unwrap();
    assert_eq!(f(&result["output_value"]), -50.0);
}

#[test]
fn test_edge_cases__test_large_coefficient() {
    let ba = axiom(
        "BA-LARGE",
        "large_test",
        "Test",
        1000.0,
        "10.1234/test",
        "Page 1",
    );
    let result = ba.apply("test", 5, None, &no_kwargs()).unwrap();
    assert_eq!(f(&result["output_value"]), 5000.0);
}

#[test]
fn test_edge_cases__test_axiom_to_dict() {
    let ba = BridgeAxiom {
        input_domain: "domain_a".into(),
        output_domain: "domain_b".into(),
        ..axiom(
            "BA-DICT",
            "dict_test",
            "Test rule",
            1.5,
            "10.1234/test",
            "Page 1",
        )
    };
    let data = ba.to_dict();
    assert_eq!(data["axiom_id"], "BA-DICT");
    assert_eq!(data["name"], "dict_test");
    assert_eq!(f(&data["coefficient"]), 1.5);
    assert_eq!(data["input_domain"], "domain_a");
    assert_eq!(data["output_domain"], "domain_b");
}

#[test]
fn test_edge_cases__test_chain_to_dict() {
    let mut chain = TranslationChain::new("chain_dict");
    chain.add_layer("L1", "input", 100, Value::Null, Map::new());
    let data = chain.to_dict();
    assert_eq!(data["chain_id"], "chain_dict");
    assert_eq!(data["layers"].as_array().unwrap().len(), 1);
    assert_eq!(f(&data["confidence"]), 1.0);
}
