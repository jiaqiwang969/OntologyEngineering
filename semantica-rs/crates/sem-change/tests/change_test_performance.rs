//! Port of `tests/change_management/test_performance.py` (`TestStoragePerformance`,
//! `TestVersionManagerPerformance`, `TestChecksumPerformance`, `TestConcurrencyPerformance`,
//! `TestMemoryUsage`).
//!
//! The timing thresholds are the original's. Tests of this file run one at a time (pytest runs
//! them serially), so their timings and memory readings are not disturbed by each other.
#![allow(non_snake_case)]

mod common;

use std::time::Instant;

use common::{obj, serial, temp_db};
use indexmap::IndexMap;
use sem_change::managers::SnapshotOptions;
use sem_change::{
    BaseVersionManager, InMemoryVersionStorage, OntologyVersionManager, SQLiteVersionStorage,
    TemporalVersionManager, VersionStorage, compute_checksum, verify_checksum,
};
use serde_json::{Map, Value, json};

/// `PerformanceTestSuite`.
struct PerformanceTestSuite;

impl PerformanceTestSuite {
    /// `measure_time(func)`: the result and the elapsed wall time in seconds.
    fn measure_time<T>(&self, func: impl FnOnce() -> T) -> (T, f64) {
        let start_time = Instant::now();
        let result = func();
        (result, start_time.elapsed().as_secs_f64())
    }

    /// `generate_test_graph(num_entities, num_relationships)`.
    fn generate_test_graph(
        &self,
        num_entities: usize,
        num_relationships: usize,
    ) -> Map<String, Value> {
        let mut entities = Vec::with_capacity(num_entities);
        for i in 0..num_entities {
            entities.push(json!({
                "id": format!("entity_{i}"),
                "name": format!("Entity {i}"),
                "type": format!("Type_{}", i % 10),
                "description": format!("Description for entity {i}").repeat(5),
                "properties": {
                    "category": format!("Category_{}", i % 5),
                    "score": i as f64 * 0.1,
                    "active": i % 2 == 0,
                },
            }));
        }
        let mut relationships = Vec::with_capacity(num_relationships);
        for i in 0..num_relationships {
            let source_idx = i % num_entities;
            let target_idx = (i + 1) % num_entities;
            relationships.push(json!({
                "source": format!("entity_{source_idx}"),
                "target": format!("entity_{target_idx}"),
                "type": format!("relation_type_{}", i % 5),
                "weight": i as f64 * 0.01,
                "properties": {
                    "strength": i % 10,
                    "confidence": 0.8 + (i % 20) as f64 * 0.01,
                },
            }));
        }
        obj(json!({"entities": entities, "relationships": relationships}))
    }

    /// `generate_test_ontology(num_classes, num_properties)`.
    fn generate_test_ontology(
        &self,
        num_classes: usize,
        num_properties: usize,
    ) -> Map<String, Value> {
        let classes: Vec<String> = (0..num_classes).map(|i| format!("Class_{i}")).collect();
        let properties: Vec<String> = (0..num_properties)
            .map(|i| format!("property_{i}"))
            .collect();
        let individuals: Vec<String> = (0..num_classes / 2)
            .map(|i| format!("individual_{i}"))
            .collect();
        let axioms: Vec<String> = (0..num_classes)
            .map(|i| format!("Class_{i} hasProperty property_{}", i % num_properties))
            .collect();
        obj(json!({
            "uri": "https://test.com/ontology",
            "version_info": {"version": "1.0", "date": "2024-01-30"},
            "structure": {
                "classes": classes,
                "properties": properties,
                "individuals": individuals,
                "axioms": axioms,
            },
        }))
    }
}

/// The snapshot dict the storage tests save.
fn storage_snapshot(
    label: &str,
    description: &str,
    graph: &Map<String, Value>,
    author: &str,
) -> Map<String, Value> {
    obj(json!({
        "label": label,
        "timestamp": "2024-01-30T12:00:00Z",
        "author": author,
        "description": description,
        "entities": graph["entities"],
        "relationships": graph["relationships"],
        "checksum": compute_checksum(graph),
    }))
}

/// `self.process.memory_info().rss / 1024 / 1024` (MB).
fn rss_mb() -> f64 {
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm")
        && let Some(pages) = statm
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.parse::<f64>().ok())
    {
        return pages * 4096.0 / 1024.0 / 1024.0;
    }
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("run ps to read the resident set size");
    let kb: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("ps prints the RSS in KiB");
    kb / 1024.0
}

#[test]
fn test_storage_performance__test_inmemory_storage_performance() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    let test_sizes = [10, 50, 100, 500, 1000];
    println!("\n=== InMemoryVersionStorage Performance ===");

    let storage = InMemoryVersionStorage::new();
    let mut results: IndexMap<usize, [f64; 3]> = IndexMap::new();

    for size in test_sizes {
        let graph = perf_suite.generate_test_graph(size, size * 2);
        let snapshot = storage_snapshot(
            &format!("test_v{size}"),
            &format!("Test snapshot with {size} entities"),
            &graph,
            "test@example.com",
        );

        let (saved, save_time) = perf_suite.measure_time(|| storage.save(&snapshot));
        saved.unwrap();
        let (got, get_time) = perf_suite.measure_time(|| storage.get(&format!("test_v{size}")));
        got.unwrap();
        let (listed, list_time) = perf_suite.measure_time(|| storage.list_all());
        listed.unwrap();

        results.insert(size, [save_time, get_time, list_time]);
        println!(
            "Size {size:4}: Save={:6.2}ms, Get={:6.2}ms, List={:6.2}ms",
            save_time * 1000.0,
            get_time * 1000.0,
            list_time * 1000.0
        );
    }

    assert!(
        results[&1000][0] < 0.1,
        "Large snapshot save should be under 100ms"
    );
    assert!(
        results[&1000][1] < 0.05,
        "Large snapshot retrieval should be under 50ms"
    );
}

#[test]
fn test_storage_performance__test_sqlite_storage_performance() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    let test_sizes = [10, 50, 100, 500, 1000];
    println!("\n=== SQLiteVersionStorage Performance ===");

    let db_path = temp_db();
    let storage = SQLiteVersionStorage::new(&db_path).unwrap();
    let mut results: IndexMap<usize, [f64; 3]> = IndexMap::new();

    for size in test_sizes {
        let graph = perf_suite.generate_test_graph(size, size * 2);
        let snapshot = storage_snapshot(
            &format!("test_v{size}"),
            &format!("Test snapshot with {size} entities"),
            &graph,
            "test@example.com",
        );

        let (saved, save_time) = perf_suite.measure_time(|| storage.save(&snapshot));
        saved.unwrap();
        let (got, get_time) = perf_suite.measure_time(|| storage.get(&format!("test_v{size}")));
        got.unwrap();
        let (listed, list_time) = perf_suite.measure_time(|| storage.list_all());
        listed.unwrap();

        results.insert(size, [save_time, get_time, list_time]);
        println!(
            "Size {size:4}: Save={:6.2}ms, Get={:6.2}ms, List={:6.2}ms",
            save_time * 1000.0,
            get_time * 1000.0,
            list_time * 1000.0
        );
    }

    assert!(
        results[&1000][0] < 0.5,
        "Large snapshot save should be under 500ms"
    );
    assert!(
        results[&1000][1] < 0.1,
        "Large snapshot retrieval should be under 100ms"
    );
}

#[test]
fn test_storage_performance__test_storage_comparison() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    println!("\n=== Storage Backend Comparison ===");

    let test_size = 500;
    let graph = perf_suite.generate_test_graph(test_size, test_size * 2);
    let snapshot = storage_snapshot(
        "comparison_test",
        &format!("Comparison test with {test_size} entities"),
        &graph,
        "test@example.com",
    );

    let inmemory_storage = InMemoryVersionStorage::new();
    let (r, inmemory_save) = perf_suite.measure_time(|| inmemory_storage.save(&snapshot));
    r.unwrap();
    let (r, inmemory_get) = perf_suite.measure_time(|| inmemory_storage.get("comparison_test"));
    r.unwrap();

    let db_path = temp_db();
    let sqlite_storage = SQLiteVersionStorage::new(&db_path).unwrap();
    let (r, sqlite_save) = perf_suite.measure_time(|| sqlite_storage.save(&snapshot));
    r.unwrap();
    let (r, sqlite_get) = perf_suite.measure_time(|| sqlite_storage.get("comparison_test"));
    r.unwrap();

    println!(
        "InMemory: Save={:6.2}ms, Get={:6.2}ms",
        inmemory_save * 1000.0,
        inmemory_get * 1000.0
    );
    println!(
        "SQLite:   Save={:6.2}ms, Get={:6.2}ms",
        sqlite_save * 1000.0,
        sqlite_get * 1000.0
    );
    println!(
        "SQLite overhead: Save={:.1}x, Get={:.1}x",
        sqlite_save / inmemory_save,
        sqlite_get / inmemory_get
    );

    assert!(
        sqlite_save < inmemory_save * 10.0,
        "SQLite save shouldn't be more than 10x slower"
    );
    assert!(
        sqlite_get < inmemory_get * 5.0,
        "SQLite get shouldn't be more than 5x slower"
    );
}

#[test]
fn test_version_manager_performance__test_temporal_version_manager_performance() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    let test_sizes = [50, 100, 500, 1000, 2000];
    println!("\n=== TemporalVersionManager Performance ===");

    let manager = TemporalVersionManager::new(None, None).unwrap();
    let mut results: IndexMap<usize, [f64; 3]> = IndexMap::new();

    for size in test_sizes {
        let graph = Value::Object(perf_suite.generate_test_graph(size, size * 2));

        let (r, create_time) = perf_suite.measure_time(|| {
            manager.create_snapshot(
                &graph,
                &format!("perf_test_v{size}"),
                "test@example.com",
                &format!("Performance test with {size} entities"),
                &SnapshotOptions::default(),
            )
        });
        r.unwrap();
        let (r, list_time) = perf_suite.measure_time(|| manager.list_versions());
        r.unwrap();
        let (r, get_time) =
            perf_suite.measure_time(|| manager.get_version(&format!("perf_test_v{size}")));
        r.unwrap();

        results.insert(size, [create_time, list_time, get_time]);
        println!(
            "Size {size:4}: Create={:6.2}ms, List={:6.2}ms, Get={:6.2}ms",
            create_time * 1000.0,
            list_time * 1000.0,
            get_time * 1000.0
        );
    }

    assert!(
        results[&2000][0] < 0.5,
        "Large snapshot creation should be under 500ms"
    );
}

#[test]
fn test_version_manager_performance__test_version_comparison_performance() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    println!("\n=== Version Comparison Performance ===");

    let manager = TemporalVersionManager::new(None, None).unwrap();
    let mut results: IndexMap<usize, f64> = IndexMap::new();

    for size in [100, 500, 1000] {
        let graph1 = Value::Object(perf_suite.generate_test_graph(size, size * 2));
        let graph2 = Value::Object(perf_suite.generate_test_graph(size + 10, size * 2 + 20));

        let options = SnapshotOptions::default();
        manager
            .create_snapshot(
                &graph1,
                &format!("v1_{size}"),
                "test@example.com",
                "Version 1",
                &options,
            )
            .unwrap();
        manager
            .create_snapshot(
                &graph2,
                &format!("v2_{size}"),
                "test@example.com",
                "Version 2",
                &options,
            )
            .unwrap();

        let (r, compare_time) = perf_suite.measure_time(|| {
            manager.compare_versions(
                &json!(format!("v1_{size}")),
                &json!(format!("v2_{size}")),
                None,
            )
        });
        r.unwrap();

        results.insert(size, compare_time);
        println!("Size {size:4}: Compare={:6.2}ms", compare_time * 1000.0);
    }

    assert!(
        results[&1000] < 1.0,
        "Large graph comparison should be under 1 second"
    );
}

#[test]
fn test_version_manager_performance__test_ontology_version_manager_performance() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    println!("\n=== OntologyVersionManager Performance ===");

    let manager = OntologyVersionManager::new(None, None).unwrap();
    let options = SnapshotOptions::default();

    for size in [50, 100, 500, 1000] {
        let ontology = perf_suite.generate_test_ontology(size, size / 2);
        let (r, create_time) = perf_suite.measure_time(|| {
            manager.create_snapshot(
                &ontology,
                &format!("ont_v{size}"),
                "test@example.com",
                &format!("Ontology with {size} classes"),
                &options,
            )
        });
        r.unwrap();
        println!("Classes {size:4}: Create={:6.2}ms", create_time * 1000.0);
    }

    let ont1 = perf_suite.generate_test_ontology(500, 250);
    let ont2 = perf_suite.generate_test_ontology(520, 260);
    manager
        .create_snapshot(
            &ont1,
            "ont_comp_1",
            "test@example.com",
            "Ontology 1",
            &options,
        )
        .unwrap();
    manager
        .create_snapshot(
            &ont2,
            "ont_comp_2",
            "test@example.com",
            "Ontology 2",
            &options,
        )
        .unwrap();

    let (r, compare_time) =
        perf_suite.measure_time(|| manager.compare_versions("ont_comp_1", "ont_comp_2"));
    r.unwrap();
    println!("Ontology comparison: {:6.2}ms", compare_time * 1000.0);
}

#[test]
fn test_checksum_performance__test_checksum_performance() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    println!("\n=== Checksum Performance ===");

    let mut results: IndexMap<usize, [f64; 2]> = IndexMap::new();
    for size in [100, 500, 1000, 5000, 10000] {
        let graph = perf_suite.generate_test_graph(size, size * 2);

        let (_, compute_time) = perf_suite.measure_time(|| compute_checksum(&graph));

        let checksum = compute_checksum(&graph);
        let mut graph_with_checksum = graph.clone();
        graph_with_checksum.insert("checksum".into(), Value::String(checksum));

        let (_, verify_time) = perf_suite.measure_time(|| verify_checksum(&graph_with_checksum));

        results.insert(size, [compute_time, verify_time]);
        println!(
            "Size {size:5}: Compute={:6.2}ms, Verify={:6.2}ms",
            compute_time * 1000.0,
            verify_time * 1000.0
        );
    }

    assert!(
        results[&10000][0] < 0.5,
        "Large checksum computation should be under 500ms"
    );
    assert!(
        results[&10000][1] < 0.5,
        "Large checksum verification should be under 500ms"
    );
}

#[test]
fn test_concurrency_performance__test_concurrent_storage_operations() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    println!("\n=== Concurrent Storage Operations ===");

    let storage = InMemoryVersionStorage::new();
    let num_threads = 10;
    let operations_per_thread = 50;

    let worker_function = |thread_id: usize| -> Vec<f64> {
        let mut times = Vec::new();
        for i in 0..operations_per_thread {
            let graph = perf_suite.generate_test_graph(50, 100);
            let snapshot = storage_snapshot(
                &format!("thread_{thread_id}_snapshot_{i}"),
                &format!("Concurrent test snapshot {i}"),
                &graph,
                &format!("thread_{thread_id}@example.com"),
            );
            let start_time = Instant::now();
            storage.save(&snapshot).unwrap();
            times.push(start_time.elapsed().as_secs_f64());
        }
        times
    };

    let start_time = Instant::now();
    let all_times: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..num_threads)
            .map(|i| scope.spawn(move || worker_function(i)))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker panicked"))
            .collect()
    });
    let total_time = start_time.elapsed().as_secs_f64();

    let avg_operation_time = all_times.iter().sum::<f64>() / all_times.len() as f64;
    let max_operation_time = all_times.iter().cloned().fold(f64::MIN, f64::max);
    let total_operations = (num_threads * operations_per_thread) as f64;

    println!("Total operations: {total_operations}");
    println!("Total time: {total_time:.2}s");
    println!(
        "Operations per second: {:.1}",
        total_operations / total_time
    );
    println!(
        "Average operation time: {:.2}ms",
        avg_operation_time * 1000.0
    );
    println!("Max operation time: {:.2}ms", max_operation_time * 1000.0);

    assert!(
        avg_operation_time < 0.1,
        "Average concurrent operation should be under 100ms"
    );
    assert!(
        total_operations / total_time > 50.0,
        "Should handle at least 50 operations per second"
    );
}

#[test]
fn test_concurrency_performance__test_concurrent_version_manager_operations() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    println!("\n=== Concurrent Version Manager Operations ===");

    let manager = TemporalVersionManager::new(None, None).unwrap();
    let num_threads = 5;
    let snapshots_per_thread = 20;

    let create_snapshots = |thread_id: usize| -> Vec<f64> {
        let mut times = Vec::new();
        for i in 0..snapshots_per_thread {
            let graph = Value::Object(perf_suite.generate_test_graph(100, 200));
            let start_time = Instant::now();
            manager
                .create_snapshot(
                    &graph,
                    &format!("concurrent_t{thread_id}_s{i}"),
                    &format!("thread{thread_id}@example.com"),
                    &format!("Concurrent snapshot {i}"),
                    &SnapshotOptions::default(),
                )
                .unwrap();
            times.push(start_time.elapsed().as_secs_f64());
        }
        times
    };

    let start_time = Instant::now();
    let all_times: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..num_threads)
            .map(|i| scope.spawn(move || create_snapshots(i)))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker panicked"))
            .collect()
    });
    let total_time = start_time.elapsed().as_secs_f64();

    let versions = manager.list_versions().unwrap();
    let expected_count = num_threads * snapshots_per_thread;

    println!("Created {} snapshots in {total_time:.2}s", versions.len());
    println!(
        "Average creation time: {:.2}ms",
        all_times.iter().sum::<f64>() / all_times.len() as f64 * 1000.0
    );

    assert_eq!(
        versions.len(),
        expected_count,
        "Expected {expected_count} snapshots, got {}",
        versions.len()
    );
}

#[test]
fn test_memory_usage__test_memory_usage_scaling() {
    let _serial = serial();
    let perf_suite = PerformanceTestSuite;
    println!("\n=== Memory Usage Scaling ===");

    let manager = TemporalVersionManager::new(None, None).unwrap();
    let initial_memory = rss_mb();

    let mut memory_usage: IndexMap<usize, f64> = IndexMap::new();
    for size in [100, 500, 1000, 2000] {
        let graph = Value::Object(perf_suite.generate_test_graph(size, size * 2));
        manager
            .create_snapshot(
                &graph,
                &format!("memory_test_{size}"),
                "test@example.com",
                &format!("Memory test with {size} entities"),
                &SnapshotOptions::default(),
            )
            .unwrap();

        let current_memory = rss_mb();
        let memory_used = current_memory - initial_memory;
        memory_usage.insert(size, memory_used);
        println!("Size {size:4}: Memory used: {memory_used:.1}MB");
    }

    let memory_per_entity = memory_usage[&2000] / 2000.0;
    println!("Memory per entity: {:.2}KB", memory_per_entity * 1024.0);

    assert!(
        memory_usage[&1000] < 50.0,
        "Memory usage should be reasonable for large datasets"
    );
}
