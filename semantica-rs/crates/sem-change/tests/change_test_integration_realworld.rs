//! Port of `tests/change_management/test_integration_realworld.py`: healthcare (HIPAA),
//! financial (SOX) and pharmaceutical (FDA 21 CFR Part 11) audit scenarios, large-scale
//! production, corruption/recovery, migration, edge cases and long-running workflows.
//!
//! Tests with wall-clock assertions or many threads run one at a time (as under pytest).
#![allow(non_snake_case)]

mod common;

use std::time::Instant;

use chrono::{Duration, NaiveDate};
use common::{assert_raises, is_processing, is_validation, obj, path_str, serial, temp_db};
use sem_change::managers::SnapshotOptions;
use sem_change::{BaseVersionManager, OntologyVersionManager, TemporalVersionManager};
use serde_json::{Map, Value, json};

fn opts() -> SnapshotOptions {
    SnapshotOptions::default()
}

fn label(version: &Map<String, Value>) -> &str {
    version["label"].as_str().expect("string label")
}

fn len(value: &Value) -> usize {
    value.as_array().expect("list").len()
}

fn push(value: &mut Value, item: Value) {
    value.as_array_mut().expect("list").push(item);
}

// ---------------------------------------------------------------------------------------------
// TestHealthcareCompliance (HIPAA § 164.312(b))
// ---------------------------------------------------------------------------------------------

#[test]
fn test_healthcare_compliance__test_patient_record_audit_trail() {
    let db_path = temp_db();
    let manager = TemporalVersionManager::new(Some(path_str(&db_path)), None).unwrap();

    // Initial patient record
    let mut patient_record = json!({
        "entities": [
            {
                "id": "patient_001",
                "type": "Patient",
                "name": "John Doe",
                "dob": "1980-05-15",
                "mrn": "MR-2024-001",
                "ssn_last4": "1234"
            },
            {
                "id": "diagnosis_001",
                "type": "Diagnosis",
                "code": "I10",
                "description": "Essential hypertension",
                "date": "2024-01-15"
            }
        ],
        "relationships": [
            {
                "source": "patient_001",
                "target": "diagnosis_001",
                "type": "has_diagnosis",
                "date": "2024-01-15"
            }
        ]
    });

    // Create initial version (v1 is not None)
    let v1 = manager
        .create_snapshot(
            &patient_record,
            "patient_001_v1.0",
            "dr.smith@hospital.com",
            "Initial patient record with hypertension diagnosis",
            &opts(),
        )
        .unwrap();
    assert!(manager.verify_checksum(&v1));

    // Add medication
    push(
        &mut patient_record["entities"],
        json!({
            "id": "medication_001",
            "type": "Medication",
            "name": "Lisinopril",
            "dosage": "10mg",
            "frequency": "once daily",
            "prescribed_date": "2024-01-15"
        }),
    );
    push(
        &mut patient_record["relationships"],
        json!({
            "source": "patient_001",
            "target": "medication_001",
            "type": "prescribed",
            "date": "2024-01-15"
        }),
    );

    let _v2 = manager
        .create_snapshot(
            &patient_record,
            "patient_001_v1.1",
            "dr.smith@hospital.com",
            "Added Lisinopril 10mg prescription",
            &opts(),
        )
        .unwrap();

    // Verify audit trail
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 2);

    // Verify all changes are tracked
    let diff = manager
        .compare_versions(&json!("patient_001_v1.0"), &json!("patient_001_v1.1"), None)
        .unwrap();
    assert_eq!(diff["summary"]["entities_added"], 1);
    assert_eq!(diff["summary"]["relationships_added"], 1);

    // Verify data integrity for compliance
    for version in &versions {
        let retrieved = manager.get_version(label(version)).unwrap().unwrap();
        assert!(
            manager.verify_checksum(&retrieved),
            "Integrity check failed for {}",
            label(version)
        );
    }

    // Verify author attribution
    assert!(
        versions
            .iter()
            .all(|v| v["author"] == "dr.smith@hospital.com")
    );
}

#[test]
fn test_healthcare_compliance__test_hipaa_access_logging() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    // Create patient record
    let patient_data = json!({
        "entities": [{"id": "patient_123", "name": "Jane Smith", "ssn": "***-**-5678"}],
        "relationships": []
    });

    // Multiple healthcare providers accessing and modifying
    let providers = [
        ("dr.jones@hospital.com", "Initial examination"),
        ("nurse.williams@hospital.com", "Vital signs recorded"),
        ("dr.chen@hospital.com", "Lab results added"),
        ("pharmacist@hospital.com", "Medication dispensed"),
    ];

    for (i, (provider, description)) in providers.iter().enumerate() {
        let snapshot = manager
            .create_snapshot(
                &patient_data,
                &format!("patient_123_v{}", i + 1),
                provider,
                description,
                &opts(),
            )
            .unwrap();
        assert_eq!(snapshot["author"], *provider);
        assert_eq!(snapshot["description"], *description);
    }

    // Verify complete access log
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 4);

    // Verify each access is properly attributed
    for (i, version) in versions.iter().enumerate() {
        assert_eq!(version["author"], providers[i].0);
        assert_eq!(version["description"], providers[i].1);
    }
}

#[test]
fn test_healthcare_compliance__test_phi_data_integrity() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    // PHI data
    let phi_data = json!({
        "entities": [
            {
                "id": "patient_456",
                "name": "Robert Johnson",
                "dob": "1975-03-20",
                "ssn": "***-**-9012",
                "address": "123 Main St, City, State",
                "phone": "555-0123",
                "email": "robert.j@email.com"
            }
        ],
        "relationships": []
    });

    let mut snapshot = manager
        .create_snapshot(
            &phi_data,
            "phi_v1",
            "admin@hospital.com",
            "PHI data snapshot",
            &opts(),
        )
        .unwrap();

    // Verify integrity
    assert!(manager.verify_checksum(&snapshot));

    // Simulate tampering
    snapshot.get_mut("entities").unwrap()[0]["ssn"] = json!("123-45-6789"); // Unauthorized modification

    // Should detect tampering
    assert!(!manager.verify_checksum(&snapshot));
}

// ---------------------------------------------------------------------------------------------
// TestFinancialCompliance (SOX § 404)
// ---------------------------------------------------------------------------------------------

#[test]
fn test_financial_compliance__test_financial_transaction_audit() {
    let db_path = temp_db();
    let manager = TemporalVersionManager::new(Some(path_str(&db_path)), None).unwrap();

    // Financial transaction graph
    let mut transaction_graph = json!({
        "entities": [
            {
                "id": "txn_001",
                "type": "Transaction",
                "amount": 10000.00,
                "currency": "USD",
                "date": "2024-01-15",
                "status": "pending"
            },
            {
                "id": "account_001",
                "type": "Account",
                "number": "****1234",
                "balance": 50000.00
            }
        ],
        "relationships": [
            {
                "source": "txn_001",
                "target": "account_001",
                "type": "debits"
            }
        ]
    });

    // Create transaction record
    let _v1 = manager
        .create_snapshot(
            &transaction_graph,
            "txn_001_initial",
            "system@bank.com",
            "Transaction initiated",
            &opts(),
        )
        .unwrap();

    // Approval workflow
    transaction_graph["entities"][0]["status"] = json!("approved");
    transaction_graph["entities"][0]["approved_by"] = json!("manager@bank.com");
    transaction_graph["entities"][0]["approved_date"] = json!("2024-01-15T10:30:00Z");

    let _v2 = manager
        .create_snapshot(
            &transaction_graph,
            "txn_001_approved",
            "manager@bank.com",
            "Transaction approved by manager",
            &opts(),
        )
        .unwrap();

    // Completion
    transaction_graph["entities"][0]["status"] = json!("completed");
    transaction_graph["entities"][1]["balance"] = json!(40000.00);

    let _v3 = manager
        .create_snapshot(
            &transaction_graph,
            "txn_001_completed",
            "system@bank.com",
            "Transaction completed and balance updated",
            &opts(),
        )
        .unwrap();

    // Verify complete audit trail
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 3);

    // Verify immutability - cannot overwrite
    assert_raises(
        manager.create_snapshot(
            &transaction_graph,
            "txn_001_initial", // Duplicate label
            "hacker@evil.com",
            "Attempting to modify history",
            &opts(),
        ),
        |e| is_validation(e) || is_processing(e),
        "",
    );

    // Verify all versions have integrity
    for version in &versions {
        let retrieved = manager.get_version(label(version)).unwrap().unwrap();
        assert!(manager.verify_checksum(&retrieved));
    }
}

#[test]
fn test_financial_compliance__test_sox_change_control() {
    let manager = OntologyVersionManager::new(None, None).unwrap();

    // Financial ontology
    let mut financial_ontology = obj(json!({
        "uri": "https://bank.com/ontology/financial",
        "version_info": {"version": "1.0", "date": "2024-01-30"},
        "structure": {
            "classes": ["Account", "Transaction", "Customer"],
            "properties": ["accountNumber", "balance", "transactionAmount"],
            "individuals": ["CheckingAccount", "SavingsAccount"],
            "axioms": [
                "Account belongsTo exactly 1 Customer",
                "Transaction involves exactly 1 Account"
            ]
        }
    }));

    // Initial version
    let _v1 = manager
        .create_snapshot(
            &financial_ontology,
            "financial_ont_v1.0",
            "architect@bank.com",
            "Initial financial ontology",
            &opts(),
        )
        .unwrap();

    // Add compliance requirements
    let structure = &mut financial_ontology["structure"];
    for class in ["ComplianceCheck", "AuditLog"] {
        push(&mut structure["classes"], json!(class));
    }
    for property in ["complianceStatus", "auditTimestamp"] {
        push(&mut structure["properties"], json!(property));
    }
    push(
        &mut structure["axioms"],
        json!("Transaction requiresCompliance exactly 1 ComplianceCheck"),
    );

    let _v2 = manager
        .create_snapshot(
            &financial_ontology,
            "financial_ont_v2.0",
            "compliance@bank.com",
            "Added SOX compliance requirements",
            &opts(),
        )
        .unwrap();

    // Verify structural changes are tracked
    let diff = manager
        .compare_versions("financial_ont_v1.0", "financial_ont_v2.0")
        .unwrap();
    let classes_added = diff["classes_added"].as_array().unwrap();
    assert!(classes_added.contains(&json!("ComplianceCheck")));
    assert!(classes_added.contains(&json!("AuditLog")));
    assert_eq!(len(&diff["axioms_added"]), 1);
}

// ---------------------------------------------------------------------------------------------
// TestPharmaceuticalCompliance (FDA 21 CFR Part 11)
// ---------------------------------------------------------------------------------------------

#[test]
fn test_pharmaceutical_compliance__test_clinical_trial_data_integrity() {
    let db_path = temp_db();
    let manager = TemporalVersionManager::new(Some(path_str(&db_path)), None).unwrap();

    // Clinical trial data
    let mut trial_data = json!({
        "entities": [
            {
                "id": "trial_001",
                "type": "ClinicalTrial",
                "name": "Phase III Efficacy Study",
                "drug": "Compound-X",
                "protocol": "PROTO-2024-001",
                "status": "active"
            },
            {
                "id": "cohort_001",
                "type": "PatientCohort",
                "size": 500,
                "demographics": "Adults 18-65",
                "enrollment_date": "2024-01-01"
            }
        ],
        "relationships": [
            {
                "source": "trial_001",
                "target": "cohort_001",
                "type": "includes_cohort"
            }
        ]
    });

    // Baseline data with electronic signature
    let v1 = manager
        .create_snapshot(
            &trial_data,
            "trial_001_baseline",
            "principal.investigator@pharma.com",
            "Baseline clinical trial data - FDA 21 CFR Part 11 compliant",
            &opts(),
        )
        .unwrap();

    // Verify electronic signature (author email)
    assert_eq!(v1["author"], "principal.investigator@pharma.com");

    // Verify data integrity
    assert!(manager.verify_checksum(&v1));

    // Add interim results
    push(
        &mut trial_data["entities"],
        json!({
            "id": "results_001",
            "type": "InterimResults",
            "date": "2024-02-15",
            "efficacy_rate": 0.75,
            "adverse_events": 12
        }),
    );

    let _v2 = manager
        .create_snapshot(
            &trial_data,
            "trial_001_interim",
            "data.manager@pharma.com",
            "Interim results - 6 week analysis",
            &opts(),
        )
        .unwrap();

    // Verify audit trail
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 2);

    // Verify data integrity for all versions (FDA requirement)
    for version in &versions {
        let retrieved = manager.get_version(label(version)).unwrap().unwrap();
        assert!(
            manager.verify_checksum(&retrieved),
            "FDA 21 CFR Part 11 integrity check failed for {}",
            label(version)
        );
    }

    // Generate audit report
    let mut audit_report = Vec::new();
    for version in &versions {
        let retrieved = manager.get_version(label(version)).unwrap().unwrap();
        audit_report.push(json!({
            "version": version["label"],
            "timestamp": version["timestamp"],
            "author": version["author"],
            "description": version["description"],
            "checksum": version["checksum"],
            "integrity_verified": manager.verify_checksum(&retrieved),
        }));
    }

    // All entries should have verified integrity
    assert!(
        audit_report
            .iter()
            .all(|entry| entry["integrity_verified"] == true)
    );
}

#[test]
fn test_pharmaceutical_compliance__test_electronic_signature_validation() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    // Valid electronic signature (email)
    let data = json!({"entities": [], "relationships": []});
    let snapshot = manager
        .create_snapshot(
            &data,
            "v1",
            "qualified.person@pharma.com",
            "Signed by qualified person",
            &opts(),
        )
        .unwrap();

    assert_eq!(snapshot["author"], "qualified.person@pharma.com");

    // Invalid signature should be rejected
    assert_raises(
        manager.create_snapshot(
            &data,
            "v2",
            "not-a-valid-signature",
            "Invalid signature",
            &opts(),
        ),
        is_validation,
        "email",
    );
}

// ---------------------------------------------------------------------------------------------
// TestLargeScaleProduction
// ---------------------------------------------------------------------------------------------

#[test]
fn test_large_scale_production__test_high_volume_snapshots() {
    let _serial = serial();
    let manager = TemporalVersionManager::new(None, None).unwrap();

    // Create 100 versions rapidly
    let mut base_graph = json!({"entities": [], "relationships": []});

    let start_time = Instant::now();
    for i in 0..100 {
        push(
            &mut base_graph["entities"],
            json!({"id": format!("entity_{i}"), "value": i}),
        );
        manager
            .create_snapshot(
                &base_graph,
                &format!("v{i}"),
                "system@company.com",
                &format!("Version {i}"),
                &opts(),
            )
            .unwrap();
    }
    let duration = start_time.elapsed().as_secs_f64();

    // Should handle 100 versions efficiently
    assert!(
        duration < 5.0,
        "High volume test took {duration}s, should be <5s"
    );

    // Verify all versions are retrievable
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 100);

    // Spot check some versions
    for i in [0, 25, 50, 75, 99] {
        let version = manager.get_version(&format!("v{i}")).unwrap();
        let version = version.expect("version is not None");
        assert_eq!(len(&version["entities"]), i + 1);
    }
}

#[test]
fn test_large_scale_production__test_large_graph_performance() {
    let _serial = serial();
    let manager = TemporalVersionManager::new(None, None).unwrap();

    // Create graph with 5000 entities and 10000 relationships
    let entities: Vec<Value> = (0..5000)
        .map(|i| json!({"id": format!("entity_{i}"), "type": "Node", "value": i}))
        .collect();
    let relationships: Vec<Value> = (0..4999)
        .map(|i| {
            json!({
                "source": format!("entity_{i}"),
                "target": format!("entity_{}", i + 1),
                "type": "connects"
            })
        })
        .chain((0..4998).map(|i| {
            json!({
                "source": format!("entity_{i}"),
                "target": format!("entity_{}", i + 2),
                "type": "skips"
            })
        }))
        .collect();
    let large_graph = json!({"entities": entities, "relationships": relationships});

    // Should handle large graph efficiently
    let start = Instant::now();
    let snapshot = manager
        .create_snapshot(
            &large_graph,
            "large_v1",
            "system@company.com",
            "Large graph snapshot",
            &opts(),
        )
        .unwrap();
    let duration = start.elapsed().as_secs_f64();

    assert!(duration < 1.0, "Large graph snapshot took {duration}s");
    assert_eq!(len(&snapshot["entities"]), 5000);
    assert_eq!(len(&snapshot["relationships"]), 9997);
}

#[test]
fn test_large_scale_production__test_concurrent_write_load() {
    let _serial = serial();
    let db_path = temp_db();
    let manager = TemporalVersionManager::new(Some(path_str(&db_path)), None).unwrap();

    // Create versions from a thread
    let create_version = |thread_id: usize, count: usize| -> Vec<bool> {
        let mut results = Vec::new();
        for i in 0..count {
            let graph = json!({
                "entities": [{"id": format!("t{thread_id}_e{i}"), "value": i}],
                "relationships": []
            });
            let snapshot = manager.create_snapshot(
                &graph,
                &format!("thread_{thread_id}_v{i}"),
                &format!("user{thread_id}@company.com"),
                &format!("Thread {thread_id} version {i}"),
                &opts(),
            );
            results.push(snapshot.is_ok());
        }
        results
    };

    // Run 10 threads, each creating 20 versions
    let all_results: Vec<bool> = std::thread::scope(|scope| {
        let futures: Vec<_> = (0..10)
            .map(|thread_id| scope.spawn(move || create_version(thread_id, 20)))
            .collect();
        futures
            .into_iter()
            .flat_map(|future| future.join().expect("worker panicked"))
            .collect()
    });

    // All operations should succeed
    assert!(
        all_results.iter().all(|ok| *ok),
        "Some concurrent operations failed"
    );

    // Verify all 200 versions were created
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 200);
}

// ---------------------------------------------------------------------------------------------
// TestDataCorruptionRecovery
// ---------------------------------------------------------------------------------------------

#[test]
fn test_data_corruption_recovery__test_detect_corrupted_data() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let graph = json!({
        "entities": [{"id": "e1", "value": "original"}],
        "relationships": []
    });

    let mut snapshot = manager
        .create_snapshot(&graph, "v1", "user@company.com", "Original data", &opts())
        .unwrap();

    // Verify original is valid
    assert!(manager.verify_checksum(&snapshot));

    // Simulate corruption
    snapshot.get_mut("entities").unwrap()[0]["value"] = json!("corrupted");

    // Should detect corruption
    assert!(!manager.verify_checksum(&snapshot));
}

#[test]
fn test_data_corruption_recovery__test_checksum_mismatch_detection() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let graph = json!({"entities": [{"id": "e1"}], "relationships": []});
    let mut snapshot = manager
        .create_snapshot(&graph, "v1", "user@company.com", "Test", &opts())
        .unwrap();

    // Tamper with checksum
    let original_checksum = snapshot["checksum"].clone();
    snapshot.insert("checksum".into(), json!("0".repeat(64))); // Invalid checksum

    assert!(!manager.verify_checksum(&snapshot));

    // Restore correct checksum
    snapshot.insert("checksum".into(), original_checksum);
    assert!(manager.verify_checksum(&snapshot));
}

#[test]
fn test_data_corruption_recovery__test_recovery_from_valid_snapshot() {
    let db_path = temp_db();
    let manager = TemporalVersionManager::new(Some(path_str(&db_path)), None).unwrap();

    // Create valid snapshots
    let graph = json!({"entities": [{"id": "e1", "status": "good"}], "relationships": []});

    let _v1 = manager
        .create_snapshot(&graph, "v1", "user@company.com", "Good v1", &opts())
        .unwrap();
    let mut v2 = manager
        .create_snapshot(&graph, "v2", "user@company.com", "Good v2", &opts())
        .unwrap();

    // Simulate corruption in v2
    v2.get_mut("entities").unwrap()[0]["status"] = json!("corrupted");

    // Detect corruption
    assert!(!manager.verify_checksum(&v2));

    // Recover by retrieving v1
    let recovered = manager.get_version("v1").unwrap().unwrap();
    assert!(manager.verify_checksum(&recovered));
    assert_eq!(recovered["entities"][0]["status"], "good");
}

// ---------------------------------------------------------------------------------------------
// TestMigrationUpgrade
// ---------------------------------------------------------------------------------------------

#[test]
fn test_migration_upgrade__test_storage_backend_migration() {
    // Start with in-memory
    let memory_manager = TemporalVersionManager::new(None, None).unwrap();

    let graph = json!({
        "entities": [{"id": "e1", "name": "Entity 1"}],
        "relationships": []
    });

    // Create versions in memory
    for i in 0..5 {
        memory_manager
            .create_snapshot(
                &graph,
                &format!("v{i}"),
                "user@company.com",
                &format!("Version {i}"),
                &opts(),
            )
            .unwrap();
    }

    let memory_versions = memory_manager.list_versions().unwrap();
    assert_eq!(memory_versions.len(), 5);

    // Migrate to SQLite
    let db_path = temp_db();
    let sqlite_manager = TemporalVersionManager::new(Some(path_str(&db_path)), None).unwrap();

    // Manually migrate data
    for version_info in &memory_versions {
        let version_data = memory_manager
            .get_version(label(version_info))
            .unwrap()
            .unwrap();
        sqlite_manager.storage.save(&version_data).unwrap();
    }

    // Verify migration
    let sqlite_versions = sqlite_manager.list_versions().unwrap();
    assert_eq!(sqlite_versions.len(), 5);

    // Verify data integrity after migration
    for version_info in &sqlite_versions {
        let version = sqlite_manager
            .get_version(label(version_info))
            .unwrap()
            .unwrap();
        assert!(sqlite_manager.verify_checksum(&version));
    }
}

#[test]
fn test_migration_upgrade__test_backward_compatibility() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    // Old-style usage (should still work)
    let graph = json!({"entities": [], "relationships": []});

    // New API (snapshot is not None)
    let snapshot = manager
        .create_snapshot(&graph, "v1", "user@company.com", "New API", &opts())
        .unwrap();

    assert!(snapshot.contains_key("checksum"));
    assert!(snapshot.contains_key("author"));
}

// ---------------------------------------------------------------------------------------------
// TestEdgeCasesStress
// ---------------------------------------------------------------------------------------------

#[test]
fn test_edge_cases_stress__test_empty_graph_operations() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let empty_graph = json!({"entities": [], "relationships": []});

    // snapshot is not None
    let snapshot = manager
        .create_snapshot(
            &empty_graph,
            "empty_v1",
            "user@company.com",
            "Empty graph",
            &opts(),
        )
        .unwrap();

    assert_eq!(len(&snapshot["entities"]), 0);
    assert_eq!(len(&snapshot["relationships"]), 0);
    assert!(manager.verify_checksum(&snapshot));
}

#[test]
fn test_edge_cases_stress__test_unicode_and_special_characters() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let unicode_graph = json!({
        "entities": [
            {
                "id": "e1",
                "name": "Test with 中文字符",
                "emoji": "🎉🚀💻",
                "special": "Special chars: @#$%^&*()",
                "quotes": "Single \"double\" quotes"
            }
        ],
        "relationships": []
    });

    let _snapshot = manager
        .create_snapshot(
            &unicode_graph,
            "unicode_v1",
            "user@company.com",
            "Unicode test with émojis 🎉",
            &opts(),
        )
        .unwrap();

    // Verify data is preserved
    let retrieved = manager.get_version("unicode_v1").unwrap().unwrap();
    assert_eq!(retrieved["entities"][0]["name"], "Test with 中文字符");
    assert_eq!(retrieved["entities"][0]["emoji"], "🎉🚀💻");
    assert!(manager.verify_checksum(&retrieved));
}

#[test]
fn test_edge_cases_stress__test_deeply_nested_structures() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let nested_graph = json!({
        "entities": [
            {
                "id": "e1",
                "level1": {
                    "level2": {
                        "level3": {
                            "level4": {
                                "level5": {
                                    "value": "deep nested value"
                                }
                            }
                        }
                    }
                }
            }
        ],
        "relationships": []
    });

    let _snapshot = manager
        .create_snapshot(
            &nested_graph,
            "nested_v1",
            "user@company.com",
            "Deeply nested structure",
            &opts(),
        )
        .unwrap();

    let retrieved = manager.get_version("nested_v1").unwrap().unwrap();
    assert_eq!(
        retrieved["entities"][0]["level1"]["level2"]["level3"]["level4"]["level5"]["value"],
        "deep nested value"
    );
    assert!(manager.verify_checksum(&retrieved));
}

#[test]
fn test_edge_cases_stress__test_very_long_descriptions() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let graph = json!({"entities": [], "relationships": []});

    // Maximum allowed length (500 chars)
    let max_description = "x".repeat(500);
    let snapshot = manager
        .create_snapshot(&graph, "v1", "user@company.com", &max_description, &opts())
        .unwrap();
    assert_eq!(snapshot["description"], json!(max_description));

    // Exceeding maximum should fail
    let too_long = "x".repeat(501);
    assert_raises(
        manager.create_snapshot(&graph, "v2", "user@company.com", &too_long, &opts()),
        is_validation,
        "too long",
    );
}

#[test]
fn test_edge_cases_stress__test_rapid_version_creation() {
    let _serial = serial();
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let graph = json!({"entities": [], "relationships": []});

    // Create 50 versions as fast as possible
    let start = Instant::now();
    for i in 0..50 {
        manager
            .create_snapshot(
                &graph,
                &format!("rapid_v{i}"),
                "user@company.com",
                &format!("Rapid version {i}"),
                &opts(),
            )
            .unwrap();
    }
    let duration = start.elapsed().as_secs_f64();

    // Should complete quickly
    assert!(duration < 2.0, "Rapid creation took {duration}s");

    // All versions should be present
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 50);
}

// ---------------------------------------------------------------------------------------------
// TestLongRunningWorkflows
// ---------------------------------------------------------------------------------------------

#[test]
fn test_long_running_workflows__test_daily_snapshot_workflow() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    // Simulate 30 days of daily snapshots
    let base_date = NaiveDate::from_ymd_opt(2024, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let mut graph = json!({"entities": [], "relationships": []});

    for day in 0..30 {
        let current_date = base_date + Duration::days(day);

        // Add daily data
        push(
            &mut graph["entities"],
            json!({
                "id": format!("daily_entity_{day}"),
                "date": current_date.format("%Y-%m-%dT%H:%M:%S").to_string(),
                "value": day
            }),
        );

        manager
            .create_snapshot(
                &graph,
                &format!("daily_{}", current_date.format("%Y%m%d")),
                "system@company.com",
                &format!("Daily snapshot for {}", current_date.format("%Y-%m-%d")),
                &opts(),
            )
            .unwrap();
    }

    // Verify all 30 days are recorded
    let versions = manager.list_versions().unwrap();
    assert_eq!(versions.len(), 30);

    // Verify data accumulation
    let final_version = manager.get_version("daily_20240130").unwrap().unwrap();
    assert_eq!(len(&final_version["entities"]), 30);
}

#[test]
fn test_long_running_workflows__test_version_retention_policy() {
    let manager = TemporalVersionManager::new(None, None).unwrap();

    let graph = json!({"entities": [], "relationships": []});

    // Create versions with different ages (the original's dates are unused)
    // Old versions
    for i in 0..5 {
        manager
            .create_snapshot(
                &graph,
                &format!("old_v{i}"),
                "user@company.com",
                &format!("Old version {i}"),
                &opts(),
            )
            .unwrap();
    }

    // Recent versions
    for i in 0..5 {
        manager
            .create_snapshot(
                &graph,
                &format!("recent_v{i}"),
                "user@company.com",
                &format!("Recent version {i}"),
                &opts(),
            )
            .unwrap();
    }

    // Simulate retention policy (keep only recent)
    let all_versions = manager.list_versions().unwrap();
    assert_eq!(all_versions.len(), 10);

    // In production, would implement cleanup based on timestamp
    // For now, verify all versions are accessible
    for version in &all_versions {
        let retrieved = manager.get_version(label(version)).unwrap();
        assert!(retrieved.is_some());
    }
}
