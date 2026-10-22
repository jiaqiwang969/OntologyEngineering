use oe_core::SkillRoot;
use std::path::{Path, PathBuf};
#[path = "../../../build_support/provenance.rs"]
mod input_inventory;

// Public provenance binds the fixed private-input catalog and its consumers.
// Original receiving payloads have moved without changing their identities.
use oe_core::sha256 as private_fixture_sha256;
#[allow(dead_code)]
mod private_input_catalog {
    include!("../../../build_support/private_test_fixtures.rs");
    pub fn identity(id: &str) -> Option<(u64, &'static str)> {
        CATALOG
            .iter()
            .find(|item| item.path == id)
            .map(|item| (item.bytes, item.sha256))
    }
}
const PRIVATE_BINDINGS: &[(&str, u64, &str)] = &[
    (
        "crates/semantica-runtime/tests/fixtures/source-jsonld-original.json",
        233965,
        "46cc834b81b46f8a978eef100d5e537b3f84ccceb1577383233f4ac47728b61f",
    ),
    (
        "crates/semantica-runtime/tests/fixtures/source-persistent-session-original.json",
        505886,
        "31f784753857d28cfefe211ed159c1d1c807132833b3103961af8078de080274",
    ),
    (
        "crates/semantica-runtime/tests/fixtures/source-rdf-duplicate-original.json",
        3817,
        "68ef73b5f80c1ee478e5032463d636cee3592801e57bc321c935c9da36075853",
    ),
    (
        "crates/semantica-runtime/tests/fixtures/source-rdf-formats-original.json",
        178007,
        "d21101fd3000371ed93e08c44caf03f969d15bef088b84db1a72dcfbdd780daf",
    ),
    (
        "crates/semantica-runtime/tests/fixtures/source-rdf-loading-original.json",
        874158,
        "7e1fc6dd5ae195c038a3329cd44093ab8bd2080a0bd40ad9f09cc69decde0baf",
    ),
    (
        "crates/semantica-runtime/tests/fixtures/source-trix-construct-original.json",
        362041,
        "e6cf066345c4820aefbc58a15e7e677ca55a62d7f507c72c38c3ea10bcb30690",
    ),
    (
        "crates/semantica-runtime/tests/fixtures/source-update-original.json",
        1964360,
        "7bb3c73aebd8725aaaafa1bebfcf54fb1d1c0f1ed3ed27d0bcf0f9673d258895",
    ),
    (
        "vendor/spareval/src/numeric/fixtures/source_variable_original.json",
        532348,
        "04897e1b5d024b7ef47b07caebde42a9445bbca3b99120678ff2f8474cd324c1",
    ),
    (
        "vendor/spareval/src/numeric/fixtures/source_variable_plan.json",
        12954,
        "001e10de65e41964d10f9b88731a26dfe111bd83a66543f5c911a1fd8b4fb211",
    ),
];
fn compiled_input_binding<'a>(root: &SkillRoot, original: &'a str) -> &'a str {
    if let Some((_, size, digest)) = PRIVATE_BINDINGS.iter().find(|item| item.0 == original) {
        assert!(
            !root.path().join(original).exists(),
            "private payload in public source: {original}"
        );
        assert!(
            oe_design::provenance::compiled().unwrap()["files"]
                .get(original)
                .is_none()
        );
        assert_eq!(
            private_input_catalog::identity(&format!("release-inputs-03/{original}")),
            Some((*size, *digest)),
            "independently frozen original identity: {original}"
        );
        "build_support/private_test_fixtures.rs"
    } else {
        original
    }
}

#[test]
fn actual_ggen_validation_plan_preserves_complete_ordered_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let design = oe_design::load(&root.join("specs/engineering.toml")).unwrap();
    let mut validations: Vec<_> = design.validations.iter().collect();
    validations.sort_by(|a, b| a.id.cmp(&b.id));
    // Independent source projection: no partition logic or query execution is
    // reused here. Comparing the full ordered rows catches omission, repetition,
    // field loss and order changes even when each ggen query is within its cap.
    let expected: Vec<_> = validations
        .into_iter()
        .map(|v| {
            serde_json::json!({
                "name": v.id,
                "behavior": v.behavior,
                "rustPath": v.rust_path,
                "oracle": v.oracle,
                "scope": v.scope,
            })
        })
        .collect();
    let actual: Vec<serde_json::Value> = serde_json::from_slice(
        &std::fs::read(root.join("generated/validation-plan.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

const PLANNER_INPUTS: [&str; 6] = [
    "vendor/sparopt/src/lib.rs",
    "vendor/sparopt/src/algebra.rs",
    "vendor/sparopt/src/optimizer.rs",
    "vendor/sparopt/src/type_inference.rs",
    "vendor/sparopt/Cargo.toml",
    "vendor/sparopt/UPSTREAM-PROVENANCE.json",
];

const PARSER_INPUTS: [&str; 9] = [
    "vendor/spargebra/src/term.rs",
    "vendor/spargebra/src/source_literal_occurrence.rs",
    "vendor/spargebra/src/source_exists.rs",
    "vendor/spargebra/src/source_path.rs",
    "vendor/spargebra/src/parser/source_frontend.rs",
    "vendor/spargebra/src/lib.rs",
    "vendor/spargebra/src/algebra.rs",
    "vendor/spargebra/src/parser.rs",
    "vendor/spargebra/Cargo.toml",
];

const RUNTIME_GUARD_INPUTS: [&str; 6] = [
    "crates/semantica-runtime/src/dataset/source_update.rs",
    "crates/semantica-runtime/src/dataset/source_failure.rs",
    "crates/semantica-runtime/src/dataset/source_variable_wrapper_receiving.rs",
    "crates/semantica-runtime/src/dataset/source_persistent_receiving.rs",
    "crates/semantica-runtime/src/graph.rs",
    "crates/semantica-runtime/src/graph/group_null.rs",
];

const NUMERIC_INPUTS: [&str; 18] = [
    "crates/semantica-numeric/src/lib.rs",
    "crates/semantica-numeric/src/decimal.rs",
    "crates/semantica-numeric/src/lexical.rs",
    "crates/semantica-numeric/Cargo.toml",
    "crates/semantica-numeric/PROVENANCE.md",
    "crates/semantica-numeric/LICENSE-PYTHON.txt",
    "crates/semantica-numeric/tests/frozen_cases.rs",
    "crates/semantica-numeric/tests/fixtures/source-derived-arithmetic.json",
    "crates/semantica-numeric/src/binary64.rs",
    "crates/semantica-numeric/tests/binary64_frozen.rs",
    "crates/semantica-numeric/tests/fixtures/source-derived-binary64.json",
    "crates/semantica-numeric/tests/binary64_mixed_frozen.rs",
    "crates/semantica-numeric/tests/fixtures/binary64_mixed_cases.json",
    "crates/semantica-numeric/tests/sum_avg_frozen.rs",
    "crates/semantica-numeric/tests/fixtures/sum_avg_cases.json",
    "crates/semantica-numeric/src/compare.rs",
    "crates/semantica-numeric/tests/source_comparison_cases.rs",
    "crates/semantica-numeric/tests/fixtures/source_comparison_cases.json",
];

const QUERY_NUMERIC_INPUTS: [&str; 123] = [
    "vendor/spareval/src/eval/source_persistent/lifecycle.rs",
    "vendor/spareval/src/numeric/source_variable.rs",
    "vendor/spareval/src/numeric/source_variable_receiving.rs",
    "vendor/spareval/src/numeric/fixtures/source_variable_original.json",
    "vendor/spareval/src/numeric/fixtures/source_variable_plan.json",
    "vendor/spareval/src/eval/source_variable_receiving.rs",
    "crates/semantica-runtime/tests/fixtures/source-persistent-session-original.json",
    "vendor/spareval/src/dataset/source_memory/persistent.rs",
    "vendor/spareval/src/eval/source_persistent.rs",
    "vendor/spareval/src/dataset/source_memory/contexts.rs",
    "vendor/spareval/src/dataset/source_mutable_memory_receiving.rs",
    "vendor/spareval/src/dataset/fixtures/source_mutable_memory_cases.json",
    "vendor/spareval/src/eval/source_mutable_set_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_mutable_set_cases.json",
    "vendor/spareval/LICENSE-RDFLIB",
    "vendor/spareval/src/dataset.rs",
    "vendor/spareval/src/dataset/source_memory.rs",
    "vendor/spareval/src/dataset/source_memory_receiving.rs",
    "vendor/spareval/src/dataset/fixtures/source_memory_graph_cases.json",
    "vendor/spareval/src/numeric/source_bgp_order.rs",
    "vendor/spareval/src/eval/source_bgp.rs",
    "vendor/spareval/src/eval/source_bgp_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_bgp_query_cases.json",
    "vendor/spareval/src/eval/source_triples_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_triples_query_cases.json",
    "vendor/spareval/src/eval/source_path.rs",
    "vendor/spareval/src/eval/source_path_receiving.rs",
    "vendor/spareval/src/eval/source_path_query_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_path_query_cases.json",
    "vendor/spareval/src/eval/fixtures/source_path_query_scan_cases.json",
    "vendor/spareval/src/eval/fixtures/source_path_native_cases.json",
    "vendor/spareval/src/eval/fixtures/source_path_native_scan_cases.json",
    "vendor/spareval/src/eval/fixtures/source_boolean_string_cases.json",
    "vendor/spareval/src/update.rs",
    "vendor/spareval/src/eval/source_graph_terms_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_graph_terms_cases.json",
    "vendor/spareval/src/eval/source_compvalue.rs",
    "vendor/spareval/src/eval/source_compvalue_receiving.rs",
    "vendor/spareval/src/eval/source_variable_set.rs",
    "vendor/spareval/src/eval/fixtures/source_compvalue_protocols.json",
    "vendor/spareval/src/eval/source_initial_cells_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_initial_cells_protocols.json",
    "vendor/spareval/src/numeric/source_ebv.rs",
    "vendor/spareval/src/numeric/source_relational.rs",
    "vendor/spareval/src/eval/source_relational_unary_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_relational_unary_protocols.json",
    "vendor/spareval/src/eval/source_exists.rs",
    "vendor/spareval/src/eval/source_exists_runtime_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_exists_runtime_cases.json",
    "vendor/spareval/src/eval/fixtures/source_exists_projection_instance_cases.json",
    "vendor/spareval/src/eval/fixtures/source_exists_separator_cases.json",
    "vendor/spareval/src/eval/fixtures/source_exists_mapping_cases.json",
    "vendor/spareval/src/eval/source_exists_ast_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_exists_ast_cases.json",
    "vendor/spareval/src/eval/source_exists_gate_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_exists_frontend_gate_controls.json",
    "vendor/spareval/src/eval/source_literal_bridge_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_literal_parser_bridge_cases.json",
    "vendor/spareval/src/numeric/prepared_literal.rs",
    "vendor/spareval/src/numeric/source_literal_truth_receiving.rs",
    "vendor/spareval/src/numeric/fixtures/source_literal_truth_cases.json",
    "vendor/spareval/src/numeric/fixtures/source_literal_numeric_token_cases.json",
    "vendor/spareval/src/numeric/fixtures/source_literal_error_truth_cases.json",
    "vendor/spareval/src/numeric.rs",
    "vendor/spareval/src/dataset/nullable_numeric.rs",
    "vendor/spareval/src/eval/numeric_query.rs",
    "vendor/spareval/src/eval/nullable_numeric_receiving.rs",
    "vendor/spareval/tests/fixtures/query_numeric_cases.json",
    "vendor/spareval/QUERY-NUMERIC-PROVENANCE.json",
    "vendor/spareval/src/eval/nullable_binary64_receiving.rs",
    "vendor/spareval/tests/fixtures/query_binary64_cases.json",
    "vendor/spareval/tests/fixtures/query_binary64_signal_resource.json",
    "vendor/spareval/src/eval/nullable_sum_avg_receiving.rs",
    "vendor/spareval/src/eval/fixtures/query_sum_avg_cases.json",
    "vendor/spareval/src/eval/fixtures/query_sum_avg_demand_cases.json",
    "vendor/spareval/src/numeric/source_literal.rs",
    "vendor/spareval/src/numeric/source_text.rs",
    "vendor/spareval/src/eval/nullable_group_concat_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_group_concat_cases.json",
    "vendor/spareval/src/eval/fixtures/source_group_concat_limits_supplement.json",
    "vendor/spareval/src/numeric/source_order.rs",
    "vendor/spareval/src/eval/nullable_extrema_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_extrema_cases.json",
    "vendor/spareval/src/eval/source_sort.rs",
    "vendor/spareval/src/eval/source_ordering.rs",
    "vendor/spareval/src/eval/source_start_receiving.rs",
    "vendor/spareval/src/eval/source_sort_receiving.rs",
    "vendor/spareval/src/eval/nullable_order_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_sort_schedule_cases.json",
    "vendor/spareval/src/eval/fixtures/source_order_cases.json",
    "vendor/spareval/LICENSE-CPYTHON",
    "vendor/spareval/SOURCE-SORT-PROVENANCE.md",
    "vendor/spareval/src/eval/nullable_literal_receiving.rs",
    "vendor/spareval/src/eval/fixtures/literal_identity_cases.json",
    "vendor/spareval/src/eval/fixtures/literal_language_cases.json",
    "vendor/spareval/src/eval/fixtures/literal_nonliteral_cases.json",
    "vendor/spareval/src/tuple.rs",
    "vendor/spareval/src/model.rs",
    "vendor/spareval/src/numeric/aggregate.rs",
    "vendor/spareval/src/eval/source_group.rs",
    "vendor/spareval/src/source_hash.rs",
    "vendor/spareval/src/source_hash_receiving.rs",
    "vendor/spareval/src/eval/source_set.rs",
    "vendor/spareval/src/eval/source_set_receiving.rs",
    "vendor/spareval/src/numeric/source_context.rs",
    "vendor/spareval/src/numeric/source_context_receiving.rs",
    "vendor/spareval/src/tuple/source_context.rs",
    "vendor/spareval/src/tuple/source_context_receiving.rs",
    "vendor/spareval/src/tuple/source_context_owner.rs",
    "vendor/spareval/src/tuple/source_context_owner_receiving.rs",
    "vendor/spareval/src/tuple/fixtures/source_context_owner_cases.json",
    "vendor/spareval/src/eval/source_context_owner_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_context_owner_query_cases.json",
    "vendor/spareval/src/eval/source_context.rs",
    "vendor/spareval/src/eval/source_context_join.rs",
    "vendor/spareval/src/eval/nullable_context_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_context_query_cases.json",
    "vendor/spareval/src/numeric/source_boolean.rs",
    "vendor/spareval/src/dataset/source_object_receiving.rs",
    "vendor/spareval/src/eval/fixtures/source_context_set_cases.json",
    "vendor/spareval/src/tuple/fixtures/source_context_primitives_cases.json",
    "vendor/spareval/LICENSE-SIPHASH",
    "vendor/spareval/SOURCE-HASH-SET-PROVENANCE.md",
];

const FLOAT_REPR_INPUTS: [&str; 4] = [
    "crates/semantica-json/src/lib.rs",
    "crates/semantica-json/src/native_operations.rs",
    "crates/semantica-json/tests/python_representation.rs",
    "crates/semantica-json/tests/native_operations.rs",
];

const PRIVATE_FIXTURE_SUPPORT_INPUTS: [&str; 1] = ["build_support/private_test_fixtures.rs"];

fn root() -> SkillRoot {
    SkillRoot::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn numeric_source_and_frozen_fixture_are_bound_to_the_same_design_owner() {
    let root = root();
    let captured = oe_design::provenance::compiled().unwrap();
    let inputs = input_inventory::inputs(root.path()).unwrap();
    for name in NUMERIC_INPUTS {
        assert!(inputs.contains(&PathBuf::from(name)), "{name}");
        let bytes = std::fs::read(root.path().join(name)).unwrap();
        assert_eq!(captured["files"][name]["sha256"], oe_core::sha256(&bytes));
        assert_eq!(captured["files"][name]["bytes"], bytes.len());
    }
    let design = oe_design::load(&root.path().join("specs/engineering.toml")).unwrap();
    let owner = design
        .modules
        .iter()
        .find(|m| m.id == "semantic_numeric")
        .unwrap();
    assert_eq!(owner.rust_path, "crates/semantica-numeric");
    assert_eq!(owner.depends_on, vec!["semantic_json"]);
    assert!(
        design
            .modules
            .iter()
            .find(|m| m.id == "semantic_runtime")
            .unwrap()
            .depends_on
            .contains(&owner.id)
    );
    let inventory = oe_design::inspection::inventory(root.path()).unwrap();
    let (rdf, sources) = oe_design::inspection::rdf(root.path(), &design).unwrap();
    for name in NUMERIC_INPUTS.iter().filter(|name| name.ends_with(".rs")) {
        let file = inventory.iter().find(|file| file.path == *name).unwrap();
        assert_eq!(sources[*name], file.sha256);
        let file_id = oe_core::sha256(name.as_bytes());
        assert!(rdf.contains(&format!(
            "code:{file_id} sp:module <urn:oev2:design:{}> .",
            owner.id
        )));
    }
    let constraints: Vec<_> = design
        .constraints
        .iter()
        .filter(|c| c.module == owner.id)
        .collect();
    assert!(!constraints.is_empty());
    for constraint in constraints {
        assert!(
            inventory
                .iter()
                .any(|f| f.path == constraint.rust_path && f.functions.contains(&constraint.check)),
            "{}",
            constraint.id
        );
        let validations: Vec<_> = design
            .validations
            .iter()
            .filter(|v| v.constraint == constraint.id)
            .collect();
        assert!(!validations.is_empty(), "{}", constraint.id);
        for validation in validations {
            assert!(
                inventory
                    .iter()
                    .any(|f| f.path == validation.rust_path
                        && f.tests.contains(&validation.behavior)),
                "{}",
                validation.id
            );
        }
        let function_id =
            oe_core::sha256(format!("{}::{}", constraint.rust_path, constraint.check).as_bytes());
        assert!(rdf.contains(&format!(
            "function:{function_id} sp:executes <urn:oev2:design:{}> .",
            constraint.id
        )));
    }
}
#[test]
fn query_numeric_sources_and_frozen_cases_share_the_evaluator_owner() {
    let root = root();
    let captured = oe_design::provenance::compiled().unwrap();
    let inputs = input_inventory::inputs(root.path()).unwrap();
    for name in QUERY_NUMERIC_INPUTS
        .into_iter()
        .chain(PARSER_INPUTS)
        .chain(RUNTIME_GUARD_INPUTS)
        .chain(FLOAT_REPR_INPUTS)
        .chain(PRIVATE_FIXTURE_SUPPORT_INPUTS)
    {
        let name = compiled_input_binding(&root, name);
        assert!(inputs.contains(&PathBuf::from(name)), "{name}");
        let bytes = std::fs::read(root.path().join(name)).unwrap();
        assert_eq!(captured["files"][name]["sha256"], oe_core::sha256(&bytes));
        assert_eq!(captured["files"][name]["bytes"], bytes.len());
    }
    let design = oe_design::load(&root.path().join("specs/engineering.toml")).unwrap();
    let evaluator = design
        .modules
        .iter()
        .find(|m| m.id == "semantic_evaluator")
        .unwrap();
    assert_eq!(evaluator.rust_path, "vendor/spareval");
    assert!(
        evaluator
            .depends_on
            .iter()
            .any(|id| id == "semantic_numeric")
    );
    let inventory = oe_design::inspection::inventory(root.path()).unwrap();
    let (rdf, sources) = oe_design::inspection::rdf(root.path(), &design).unwrap();
    let parser = design
        .modules
        .iter()
        .find(|module| module.id == "semantic_query_parser")
        .unwrap();
    assert_eq!(parser.rust_path, "vendor/spargebra");
    assert!(
        design
            .modules
            .iter()
            .find(|module| module.id == "semantic_query_planner")
            .unwrap()
            .depends_on
            .contains(&parser.id)
    );
    for name in PARSER_INPUTS.iter().filter(|name| name.ends_with(".rs")) {
        let file = inventory.iter().find(|file| file.path == *name).unwrap();
        assert_eq!(sources[*name], file.sha256);
        let file_id = oe_core::sha256(name.as_bytes());
        assert!(rdf.contains(&format!(
            "code:{file_id} sp:module <urn:oev2:design:{}> .",
            parser.id
        )));
    }
    for name in QUERY_NUMERIC_INPUTS
        .iter()
        .filter(|name| name.ends_with(".rs"))
    {
        let file = inventory.iter().find(|file| file.path == *name).unwrap();
        assert_eq!(sources[*name], file.sha256);
        let file_id = oe_core::sha256(name.as_bytes());
        assert!(rdf.contains(&format!(
            "code:{file_id} sp:module <urn:oev2:design:{}> .",
            evaluator.id
        )));
    }
    let generated_source_map: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("generated/source-map.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(generated_source_map["schema"], "oev2.source-map/1");
    let generated_files = generated_source_map["files"].as_array().unwrap();
    let mut expected_guard_rows = Vec::new();
    for name in RUNTIME_GUARD_INPUTS {
        let file = inventory.iter().find(|file| file.path == name).unwrap();
        assert_eq!(sources[name], file.sha256);
        let file_id = oe_core::sha256(name.as_bytes());
        assert!(rdf.contains(&format!(
            "code:{file_id} sp:module <urn:oev2:design:semantic_runtime> ."
        )));
        // Shared crate paths declare multiple static memberships, not one
        // exclusive functional owner selected by module declaration order.
        let expected: std::collections::BTreeSet<_> = design
            .modules
            .iter()
            .filter(|module| name.starts_with(&format!("{}/", module.rust_path)))
            .map(|module| module.id.as_str())
            .collect();
        let prefix = format!("code:{file_id} sp:module <urn:oev2:design:");
        let actual: std::collections::BTreeSet<_> = rdf
            .lines()
            .filter_map(|line| line.strip_prefix(&prefix)?.strip_suffix("> ."))
            .collect();
        assert_eq!(actual, expected, "declared module membership: {name}");
        assert!(!actual.contains("semantic_evaluator"));
        for module in &expected {
            expected_guard_rows.push((name, *module));
        }
        for row in generated_files.iter().filter(|row| row["path"] == name) {
            assert_eq!(row["sha256"], file.sha256, "generated source SHA: {name}");
            assert_eq!(row["production"], true);
        }
    }
    expected_guard_rows.sort_unstable();
    let actual_guard_rows: Vec<_> = generated_files
        .iter()
        .filter(|row| RUNTIME_GUARD_INPUTS.contains(&row["path"].as_str().unwrap()))
        .map(|row| {
            (
                row["path"].as_str().unwrap(),
                row["module"].as_str().unwrap(),
            )
        })
        .collect();
    // Compare the actual ggen artifact in order, retaining duplicate rows so
    // both a missing association and an extra/repeated association fail.
    assert_eq!(actual_guard_rows, expected_guard_rows);
    let mut reversed_design = design.clone();
    reversed_design.modules.reverse();
    let (reversed_rdf, reversed_sources) =
        oe_design::inspection::rdf(root.path(), &reversed_design).unwrap();
    assert_eq!(
        reversed_rdf, rdf,
        "module declaration order must not select an owner"
    );
    assert_eq!(reversed_sources, sources);
    let formatter = design
        .modules
        .iter()
        .find(|m| m.id == "semantic_json")
        .unwrap();
    assert_eq!(formatter.rust_path, "crates/semantica-json");
    assert!(
        design
            .modules
            .iter()
            .find(|m| m.id == "semantic_numeric")
            .unwrap()
            .depends_on
            .contains(&formatter.id)
    );
    for name in FLOAT_REPR_INPUTS {
        let file = inventory.iter().find(|file| file.path == name).unwrap();
        assert_eq!(sources[name], file.sha256);
        let file_id = oe_core::sha256(name.as_bytes());
        assert!(rdf.contains(&format!(
            "code:{file_id} sp:module <urn:oev2:design:{}> .",
            formatter.id
        )));
    }
    let constraints: Vec<_> = design
        .constraints
        .iter()
        .filter(|c| c.id.starts_with("semantic_query_numeric_"))
        .collect();
    assert!(!constraints.is_empty());
    for constraint in constraints {
        let owner = design
            .modules
            .iter()
            .find(|m| m.id == constraint.module)
            .unwrap();
        assert!(
            constraint
                .rust_path
                .starts_with(&format!("{}/", owner.rust_path))
        );
        assert!(
            inventory
                .iter()
                .any(|f| f.path == constraint.rust_path && f.functions.contains(&constraint.check))
        );
        let validations: Vec<_> = design
            .validations
            .iter()
            .filter(|v| v.constraint == constraint.id)
            .collect();
        assert!(!validations.is_empty(), "{}", constraint.id);
        for validation in validations {
            assert!(
                inventory
                    .iter()
                    .any(|f| f.path == validation.rust_path
                        && f.tests.contains(&validation.behavior)),
                "{}",
                validation.id
            );
        }
        let function_id =
            oe_core::sha256(format!("{}::{}", constraint.rust_path, constraint.check).as_bytes());
        assert!(rdf.contains(&format!(
            "function:{function_id} sp:executes <urn:oev2:design:{}> .",
            constraint.id
        )));
    }
}

fn fixture(root: &SkillRoot) -> SkillRoot {
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = root
        .new_private_dir(Path::new(&format!(
            "var/maintenance/evaluator-source-tests/{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )))
        .unwrap();
    std::fs::write(
        path.join("SKILL.md"),
        "isolated source-bound evaluator test\n",
    )
    .unwrap();
    for name in oe_design::provenance::compiled().unwrap()["files"]
        .as_object()
        .unwrap()
        .keys()
    {
        let out = path.join(name);
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::copy(root.path().join(name), out).unwrap();
    }
    SkillRoot::new(path).unwrap()
}
#[test]
fn local_evaluator_source_is_bound_and_missing_changed_or_added_code_is_rejected() {
    let root = root();
    let files = input_inventory::inputs(root.path()).unwrap();
    let target = "vendor/spareval/src/eval.rs";
    assert!(files.contains(&PathBuf::from(target)));
    assert!(files.contains(&PathBuf::from("vendor/spareval/UPSTREAM-PROVENANCE.json")));
    let oracle = "crates/semantica-runtime/tests/fixtures/query-minus-rdflib-oracle.json";
    assert!(files.contains(&PathBuf::from(oracle)));
    let captured = oe_design::provenance::compiled().unwrap();
    assert_eq!(
        captured["files"][target]["sha256"],
        oe_core::sha256(std::fs::read(root.path().join(target)).unwrap())
    );
    let f = fixture(&root);
    oe_design::provenance::require(&f).unwrap();
    let original = std::fs::read(f.path().join(target)).unwrap();
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n// independent source drift counterexample\n");
    std::fs::write(f.path().join(target), changed).unwrap();
    assert_eq!(
        oe_design::provenance::binding(&f).unwrap()["changed"],
        serde_json::json!([target])
    );
    assert!(oe_design::provenance::require(&f).is_err());
    std::fs::remove_file(f.path().join(target)).unwrap();
    assert_eq!(
        oe_design::provenance::binding(&f).unwrap()["missing"],
        serde_json::json!([target])
    );
    assert!(oe_design::provenance::require(&f).is_err());
    std::fs::write(f.path().join(target), original).unwrap();
    let original_oracle = std::fs::read(f.path().join(oracle)).unwrap();
    let changed_oracle = String::from_utf8(original_oracle.clone())
        .unwrap()
        .replace("shaft", "bearing");
    std::fs::write(f.path().join(oracle), changed_oracle).unwrap();
    assert_eq!(
        oe_design::provenance::binding(&f).unwrap()["changed"],
        serde_json::json!([oracle])
    );
    assert!(oe_design::provenance::require(&f).is_err());
    std::fs::write(f.path().join(oracle), original_oracle).unwrap();
    let added = "vendor/spareval/src/undeclared_new_code.rs";
    std::fs::write(f.path().join(added), "pub fn undeclared() {}\n").unwrap();
    assert_eq!(
        oe_design::provenance::binding(&f).unwrap()["added"],
        serde_json::json!([added])
    );
    assert!(oe_design::provenance::require(&f).is_err());
    std::fs::remove_file(f.path().join(added)).unwrap();

    // Reuse this existing provenance fixture for the planner counterexamples;
    // do not create another workspace or build output for each changed input.
    for name in PLANNER_INPUTS
        .into_iter()
        .chain(PARSER_INPUTS)
        .chain(RUNTIME_GUARD_INPUTS)
        .chain(NUMERIC_INPUTS)
        .chain(QUERY_NUMERIC_INPUTS)
        .chain(FLOAT_REPR_INPUTS)
        .chain(PRIVATE_FIXTURE_SUPPORT_INPUTS)
    {
        oe_design::provenance::require(&f).unwrap();
        let name = compiled_input_binding(&f, name);
        let path = f.path().join(name);
        let original = std::fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n");
        std::fs::write(&path, changed).unwrap();
        let binding = oe_design::provenance::binding(&f).unwrap();
        assert_eq!(binding["status"], "mismatch", "changed {name}");
        assert_eq!(binding["changed"], serde_json::json!([name]));
        assert_eq!(binding["missing"], serde_json::json!([]));
        assert_eq!(binding["added"], serde_json::json!([]));
        assert!(oe_design::provenance::require(&f).is_err());

        std::fs::remove_file(&path).unwrap();
        let binding = oe_design::provenance::binding(&f).unwrap();
        assert_eq!(binding["status"], "incomplete", "missing {name}");
        assert_eq!(binding["changed"], serde_json::json!([]));
        assert_eq!(binding["missing"], serde_json::json!([name]));
        assert_eq!(binding["added"], serde_json::json!([]));
        assert!(oe_design::provenance::require(&f).is_err());
        std::fs::write(&path, original).unwrap();
    }
    for name in [
        "vendor/sparopt/src/undeclared_new_code.rs",
        "vendor/sparopt/undeclared_new_manifest.toml",
    ] {
        oe_design::provenance::require(&f).unwrap();
        std::fs::write(f.path().join(name), "\n").unwrap();
        let binding = oe_design::provenance::binding(&f).unwrap();
        assert_eq!(binding["status"], "mismatch", "added {name}");
        assert_eq!(binding["changed"], serde_json::json!([]));
        assert_eq!(binding["missing"], serde_json::json!([]));
        assert_eq!(binding["added"], serde_json::json!([name]));
        assert!(oe_design::provenance::require(&f).is_err());
        std::fs::remove_file(f.path().join(name)).unwrap();
    }
    oe_design::provenance::require(&f).unwrap();
}

#[test]
fn planner_source_manifest_and_provenance_are_in_the_compiled_inventory() {
    let root = root();
    let inputs = input_inventory::inputs(root.path()).unwrap();
    let captured = oe_design::provenance::compiled().unwrap();
    for name in PLANNER_INPUTS {
        assert!(inputs.contains(&PathBuf::from(name)), "uncaptured {name}");
        let bytes = std::fs::read(root.path().join(name)).unwrap();
        assert_eq!(captured["files"][name]["sha256"], oe_core::sha256(&bytes));
        assert_eq!(captured["files"][name]["bytes"], bytes.len() as u64);
    }
    let upstream: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("vendor/sparopt/UPSTREAM-PROVENANCE.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(upstream["name"], "sparopt");
    assert_eq!(upstream["version"], "0.3.7");
    assert_eq!(
        upstream["crate_sha256"],
        "973ee503ab3d6c3de66b4b459233cd5b22b44dad0fb7b8d0931daf73d2ede935"
    );
    assert_eq!(
        upstream["upstream_commit"],
        "0e81a29d27575ab69e7f85ea9c7f1058b277fcd0"
    );
    // These are upstream identities. The compiled record above binds the
    // current patched bytes separately, without relabelling them as upstream.
    for name in &PLANNER_INPUTS[..5] {
        let upstream_name = name.strip_prefix("vendor/sparopt/").unwrap();
        assert!(
            upstream["source_files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file["path"] == upstream_name)
        );
    }
}

#[test]
fn locally_patched_planner_has_an_explicit_design_owner_and_real_check_mapping() {
    let root = root();
    let design = oe_design::load(&root.path().join("specs/engineering.toml")).unwrap();
    let owner = design
        .modules
        .iter()
        .find(|module| module.rust_path == "vendor/sparopt")
        .expect("the local planner needs its own explicit design owner");
    assert_eq!(owner.id, "semantic_query_planner");
    let evaluator = design
        .modules
        .iter()
        .find(|module| module.id == "semantic_evaluator")
        .unwrap();
    assert!(evaluator.depends_on.contains(&owner.id));
    let inventory = oe_design::inspection::inventory(root.path()).unwrap();
    let (rdf, sources) = oe_design::inspection::rdf(root.path(), &design).unwrap();
    for name in &PLANNER_INPUTS[..4] {
        let file = inventory.iter().find(|file| file.path == *name).unwrap();
        assert_eq!(sources[*name], file.sha256);
        let file_id = oe_core::sha256(name.as_bytes());
        assert!(rdf.contains(&format!(
            "code:{file_id} sp:module <urn:oev2:design:{}> .",
            owner.id
        )));
    }
    for (name, function) in [
        ("vendor/sparopt/src/algebra.rs", "from_sparql_algebra"),
        ("vendor/sparopt/src/optimizer.rs", "normalize_expression"),
        (
            "vendor/sparopt/src/type_inference.rs",
            "infer_expression_type",
        ),
    ] {
        let file = inventory.iter().find(|file| file.path == name).unwrap();
        assert!(file.functions.contains(function), "{name}::{function}");
    }
    let planner_constraints: Vec<_> = design
        .constraints
        .iter()
        .filter(|constraint| constraint.rust_path.starts_with("vendor/sparopt/"))
        .collect();
    assert!(
        !planner_constraints.is_empty(),
        "planner checks must be declared"
    );
    for constraint in &planner_constraints {
        assert_eq!(constraint.module, owner.id);
        assert!(inventory.iter().any(|file| {
            file.path == constraint.rust_path && file.functions.contains(&constraint.check)
        }));
        assert!(
            design
                .validations
                .iter()
                .any(|validation| validation.constraint == constraint.id)
        );
        for validation in design
            .validations
            .iter()
            .filter(|validation| validation.constraint == constraint.id)
        {
            assert!(inventory.iter().any(|file| {
                file.path == validation.rust_path && file.tests.contains(&validation.behavior)
            }));
        }
        let function_id =
            oe_core::sha256(format!("{}::{}", constraint.rust_path, constraint.check).as_bytes());
        assert!(rdf.contains(&format!(
            "function:{function_id} sp:executes <urn:oev2:design:{}> .",
            constraint.id
        )));
    }
    let findings = oe_design::inspection::inspect(root.path(), &design).unwrap();
    assert!(
        !findings["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| {
                (finding["code"] == "source_outside_design"
                    && finding["subject"]
                        .as_str()
                        .is_some_and(|subject| subject.starts_with("vendor/sparopt/")))
                    || (finding["code"] == "constraint_execution_missing"
                        && planner_constraints
                            .iter()
                            .any(|constraint| finding["subject"] == constraint.id))
            })
    );
}
#[test]
fn locally_patched_evaluator_has_an_explicit_design_owner_and_real_check_mapping() {
    let root = root();
    let design = oe_design::load(&root.path().join("specs/engineering.toml")).unwrap();
    let inventory = oe_design::inspection::inventory(root.path()).unwrap();
    let evaluator = inventory
        .iter()
        .find(|f| f.path == "vendor/spareval/src/eval.rs")
        .unwrap();
    assert!(evaluator.functions.contains("with_initial_binding_context"));
    let (_, sources) = oe_design::inspection::rdf(root.path(), &design).unwrap();
    assert_eq!(sources["vendor/spareval/src/eval.rs"], evaluator.sha256);
    let findings = oe_design::inspection::inspect(root.path(), &design).unwrap();
    assert!(!findings["findings"].as_array().unwrap().iter().any(|f| {
        f["subject"] == "semantic_initial_context_domain"
            && f["code"] == "constraint_execution_missing"
    }));
    assert!(!findings["findings"].as_array().unwrap().iter().any(|f| {
        f["subject"]
            .as_str()
            .is_some_and(|s| s.starts_with("vendor/spareval/"))
            && f["code"] == "source_outside_design"
    }));
}

#[test]
fn describe_parser_and_frozen_oracles_share_the_original_compiled_inventory() {
    let root = root();
    let design = oe_design::load(&root.path().join("specs/engineering.toml")).unwrap();
    let inventory = oe_design::inspection::inventory(root.path()).unwrap();
    let parser = inventory
        .iter()
        .find(|file| file.path == "vendor/spargebra/src/parser.rs")
        .unwrap();
    assert!(parser.functions.contains("parse_query_rdflib760"));
    assert!(parser.functions.contains("build_describe"));
    let (_, sources) = oe_design::inspection::rdf(root.path(), &design).unwrap();
    assert_eq!(sources["vendor/spargebra/src/parser.rs"], parser.sha256);
    let findings = oe_design::inspection::inspect(root.path(), &design).unwrap();
    assert!(
        !findings["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| {
                finding["subject"] == "semantic_describe_modifier_target_projection"
                    && finding["code"] == "constraint_execution_missing"
            })
    );
    let copy = fixture(&root);
    for name in [
        "vendor/spargebra/src/parser.rs",
        "vendor/spareval/src/lib.rs",
        "vendor/spareval/DESCRIBE-PATCH-PROVENANCE.json",
        "vendor/spargebra/DESCRIBE-PATCH-PROVENANCE.json",
        "crates/semantica-runtime/tests/fixtures/describe-context-original17.json",
        "crates/semantica-runtime/tests/fixtures/describe-modifier-original9.json",
        "crates/semantica-runtime/tests/fixtures/describe-sorted-target-original2.json",
        "vendor/oxttl/src/terse.rs",
        "vendor/oxttl/src/source.rs",
        "vendor/oxrdfxml/UPSTREAM-PROVENANCE.json",
        "vendor/oxrdfxml/src/parser.rs",
        "vendor/oxrdfxml/src/source.rs",
        "vendor/oxrdfxml/src/source_xml.rs",
        "vendor/oxrdfxml/LICENSE-RDFLIB",
        "crates/semantica-runtime/tests/fixtures/source-rdf-formats-original.json",
        "vendor/oxjsonld/UPSTREAM-PROVENANCE.json",
        "vendor/oxjsonld/LICENSE-MIT",
        "vendor/oxjsonld/LICENSE-APACHE",
        "vendor/oxjsonld/LICENSE-RDFLIB",
        "crates/semantica-runtime/tests/fixtures/source-jsonld-original.json",
        "crates/semantica-runtime/tests/fixtures/source-trix-construct-original.json",
        "crates/semantica-runtime/tests/fixtures/source-update-original.json",
        "vendor/spargebra/src/source_update.rs",
        "vendor/spargebra/src/parser/source_update.rs",
        "vendor/spareval/src/eval/source_update.rs",
        "vendor/spareval/src/eval/source_update_receiving.rs",
        "crates/semantica-runtime/src/dataset/source_ingest.rs",
        "crates/semantica-runtime/src/dataset/source_load_receiving_trix_construct.rs",
        "vendor/oxjsonld/src/to_rdf.rs",
        "vendor/oxjsonld/src/expansion.rs",
        "vendor/oxjsonld/src/source.rs",
        "crates/semantica-runtime/src/dataset/source_load_receiving_jsonld.rs",
        "vendor/oxttl/UPSTREAM-PROVENANCE.json",
        "crates/semantica-runtime/tests/fixtures/source-rdf-loading-original.json",
        "crates/semantica-runtime/tests/fixtures/source-rdf-duplicate-original.json",
    ] {
        oe_design::provenance::require(&copy).unwrap();
        let name = compiled_input_binding(&copy, name);
        let path = copy.path().join(name);
        let original = std::fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n");
        std::fs::write(&path, changed).unwrap();
        assert_eq!(
            oe_design::provenance::binding(&copy).unwrap()["changed"],
            serde_json::json!([name])
        );
        assert!(oe_design::provenance::require(&copy).is_err());
        std::fs::write(&path, original).unwrap();
    }
    oe_design::provenance::require(&copy).unwrap();
}

#[test]
fn source_rdf_parser_and_frozen_oracles_share_actual_inventory() {
    let root = root();
    let captured = oe_design::provenance::compiled().unwrap();
    let design = oe_design::load(&root.path().join("specs/engineering.toml")).unwrap();
    let (rdf, sources) = oe_design::inspection::rdf(root.path(), &design).unwrap();
    let inventory = oe_design::inspection::inventory(root.path()).unwrap();
    for (path, symbol) in [
        ("vendor/oxrdfxml/src/parser.rs", "emit_triple_source"),
        ("vendor/oxrdfxml/src/source.rs", "bound_to_actual"),
        ("vendor/oxjsonld/src/to_rdf.rs", "emit_value"),
        ("vendor/oxjsonld/src/source.rs", "bound_to_actual"),
        (
            "crates/semantica-runtime/src/dataset/source_ingest.rs",
            "load_trix_source_internal",
        ),
        (
            "crates/semantica-runtime/src/dataset/source_ingest.rs",
            "load_construct_source_internal",
        ),
        ("vendor/oxttl/src/terse.rs", "push_literal"),
        ("vendor/oxttl/src/source.rs", "receive"),
        (
            "crates/semantica-runtime/src/dataset.rs",
            "load_with_mapping",
        ),
    ] {
        let bytes = std::fs::read(root.path().join(path)).unwrap();
        assert_eq!(captured["files"][path]["sha256"], oe_core::sha256(&bytes));
        let file = inventory.iter().find(|f| f.path == path).unwrap();
        assert!(file.functions.contains(symbol));
        assert_eq!(sources[path], file.sha256);
    }
    for path in [
        "vendor/oxrdfxml/UPSTREAM-PROVENANCE.json",
        "vendor/oxrdfxml/src/parser.rs",
        "vendor/oxrdfxml/src/source.rs",
        "vendor/oxrdfxml/src/source_xml.rs",
        "vendor/oxrdfxml/LICENSE-RDFLIB",
        "crates/semantica-runtime/tests/fixtures/source-rdf-formats-original.json",
        "vendor/oxjsonld/UPSTREAM-PROVENANCE.json",
        "vendor/oxjsonld/LICENSE-MIT",
        "vendor/oxjsonld/LICENSE-APACHE",
        "vendor/oxjsonld/LICENSE-RDFLIB",
        "crates/semantica-runtime/tests/fixtures/source-jsonld-original.json",
        "crates/semantica-runtime/tests/fixtures/source-trix-construct-original.json",
        "crates/semantica-runtime/tests/fixtures/source-update-original.json",
        "vendor/spargebra/src/source_update.rs",
        "vendor/spargebra/src/parser/source_update.rs",
        "vendor/spareval/src/eval/source_update.rs",
        "vendor/spareval/src/eval/source_update_receiving.rs",
        "crates/semantica-runtime/src/dataset/source_ingest.rs",
        "crates/semantica-runtime/src/dataset/source_load_receiving_trix_construct.rs",
        "vendor/oxjsonld/src/to_rdf.rs",
        "vendor/oxjsonld/src/expansion.rs",
        "vendor/oxjsonld/src/source.rs",
        "crates/semantica-runtime/src/dataset/source_load_receiving_jsonld.rs",
        "vendor/oxttl/UPSTREAM-PROVENANCE.json",
        "crates/semantica-runtime/tests/fixtures/source-rdf-loading-original.json",
        "crates/semantica-runtime/tests/fixtures/source-rdf-duplicate-original.json",
    ] {
        let path = compiled_input_binding(&root, path);
        assert_eq!(
            captured["files"][path]["sha256"],
            oe_core::sha256(std::fs::read(root.path().join(path)).unwrap())
        );
    }
    assert!(rdf.contains(&format!(
        "code:{} sp:module <urn:oev2:design:semantic_rdf_parser> .",
        oe_core::sha256(b"vendor/oxttl/src/terse.rs")
    )));
    let map: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("generated/source-map.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(map["missing_checkers"], serde_json::json!([]));
}
