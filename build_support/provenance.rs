// Shared build/runtime inventory rules; no Cargo dependency on the crate being built.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub fn inputs(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    fn visit(root: &Path, directory: &Path, out: &mut BTreeSet<PathBuf>) -> std::io::Result<()> {
        for item in std::fs::read_dir(directory)? {
            let p = item?.path();
            let m = std::fs::symlink_metadata(&p)?;
            if m.file_type().is_symlink() {
                return Err(std::io::Error::other("compiled_input_symlink"));
            }
            if m.is_dir() {
                visit(root, &p, out)?;
            } else if matches!(p.extension().and_then(|s| s.to_str()), Some("rs" | "toml")) {
                out.insert(
                    p.strip_prefix(root)
                        .map_err(std::io::Error::other)?
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    for d in [
        "crates",
        "vendor/spargebra",
        "vendor/spareval",
        "vendor/sparopt",
        "vendor/oxttl",
        "vendor/oxrdfxml",
        "vendor/oxjsonld",
        "build_support",
        "modules/engineering-memory/runtime",
        "modules/engineering-memory/contracts",
    ] {
        visit(root, &root.join(d), &mut out)?;
    }
    for p in [
        "Cargo.toml",
        "Cargo.lock",
        "modules/engineering-memory/contracts/templates/workflow.rs.tmpl",
        "modules/engineering-memory/contracts/templates/types.rs.tmpl",
        "modules/engineering-memory/contracts/generate.py",
        "specs/owner-schemas/engineering-exploration.json",
        "crates/semantica-json/src/data/python_xml_name_unicode151.json",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        // Embedded executable data and frozen test oracles are build inputs,
        // just as the Rust code that consumes them is.
        "crates/oe-cad/tests/fixtures/correspondence/plate-v1.synthetic-observation.json",
        "crates/oe-cad/tests/fixtures/correspondence/plate-diameter-v2.synthetic-observation.json",
        "crates/oe-cad/tests/fixtures/correspondence/plate-deleted-v2.synthetic-observation.json",
        "crates/oe-cad/tests/fixtures/correspondence/plate-ambiguous-v2.synthetic-observation.json",
        "crates/oe-cad/tests/fixtures/correspondence/plate-reopened-v1.synthetic-observation.json",
        "crates/oe-cad/tests/fixtures/correspondence/plate-partial-v1.synthetic-observation.json",
        "crates/oe-cad/tests/fixtures/correspondence/selectors.json",
    ] {
        if root.join(p).is_file() {
            out.insert(PathBuf::from(p));
        } else if p != "rust-toolchain.toml" {
            return Err(std::io::Error::other("compiled_input_missing"));
        }
    }
    // The migration baseline is compile-time input to the source-bound checker.
    // It stays part of fixture/receiver inventories once the explicit scope exists.
    for p in [
        "vendor/oxrdfxml/UPSTREAM-PROVENANCE.json",
        "vendor/oxrdfxml/LICENSE-MIT",
        "vendor/oxrdfxml/LICENSE-APACHE",
        "vendor/oxrdfxml/LICENSE-RDFLIB",
        "vendor/oxjsonld/UPSTREAM-PROVENANCE.json",
        "vendor/oxjsonld/LICENSE-MIT",
        "vendor/oxjsonld/LICENSE-APACHE",
        "vendor/oxjsonld/LICENSE-RDFLIB",
        "vendor/oxttl/UPSTREAM-PROVENANCE.json",
        "vendor/oxttl/LICENSE-MIT",
        "vendor/oxttl/LICENSE-APACHE",
        "crates/semantica-numeric/PROVENANCE.md",
        "crates/semantica-numeric/LICENSE-PYTHON.txt",
        "crates/semantica-numeric/tests/fixtures/source-derived-arithmetic.json",
        "crates/semantica-numeric/tests/fixtures/source-derived-binary64.json",
        "crates/semantica-numeric/tests/fixtures/binary64_mixed_cases.json",
        "crates/semantica-numeric/tests/fixtures/sum_avg_cases.json",
        "crates/semantica-numeric/tests/fixtures/source_comparison_cases.json",
        "vendor/spareval/tests/fixtures/query_numeric_cases.json",
        "vendor/spareval/tests/fixtures/query_binary64_cases.json",
        "vendor/spareval/tests/fixtures/query_binary64_signal_resource.json",
        "vendor/spareval/src/eval/fixtures/query_sum_avg_cases.json",
        "vendor/spareval/src/eval/fixtures/query_sum_avg_demand_cases.json",
        "vendor/spareval/src/eval/fixtures/literal_identity_cases.json",
        "vendor/spareval/src/eval/fixtures/literal_language_cases.json",
        "vendor/spareval/src/eval/fixtures/literal_nonliteral_cases.json",
        "vendor/spareval/src/eval/fixtures/source_group_concat_cases.json",
        "vendor/spareval/src/eval/fixtures/source_group_concat_limits_supplement.json",
        "vendor/spareval/src/eval/fixtures/source_extrema_cases.json",
        "vendor/spareval/src/eval/fixtures/source_sort_schedule_cases.json",
        "vendor/spareval/src/eval/fixtures/source_order_cases.json",
        "vendor/spareval/LICENSE-CPYTHON",
        "vendor/spareval/SOURCE-SORT-PROVENANCE.md",
        "vendor/spareval/src/eval/fixtures/source_context_set_cases.json",
        "vendor/spareval/src/eval/fixtures/source_context_query_cases.json",
        "vendor/spareval/src/tuple/fixtures/source_context_primitives_cases.json",
        "vendor/spareval/src/tuple/fixtures/source_context_owner_cases.json",
        "vendor/spareval/src/eval/fixtures/source_context_owner_query_cases.json",
        "vendor/spareval/src/eval/fixtures/source_exists_frontend_gate_controls.json",
        "vendor/spareval/src/eval/fixtures/source_exists_runtime_cases.json",
        "vendor/spareval/src/eval/fixtures/source_relational_unary_protocols.json",
        "vendor/spareval/src/eval/fixtures/source_initial_cells_protocols.json",
        "vendor/spareval/src/eval/fixtures/source_compvalue_protocols.json",
        "vendor/spareval/src/eval/fixtures/source_graph_terms_cases.json",
        "vendor/spareval/src/dataset/fixtures/source_memory_graph_cases.json",
        "vendor/spareval/src/dataset/fixtures/source_mutable_memory_cases.json",
        "vendor/spareval/src/eval/fixtures/source_mutable_set_cases.json",
        "vendor/spareval/LICENSE-RDFLIB",
        "vendor/spareval/src/eval/fixtures/source_bgp_query_cases.json",
        "vendor/spareval/src/eval/fixtures/source_triple_rows_cases.json",
        "vendor/spareval/src/eval/fixtures/source_triples_query_cases.json",
        "vendor/spareval/src/eval/fixtures/source_path_native_cases.json",
        "vendor/spareval/src/eval/fixtures/source_path_native_scan_cases.json",
        "vendor/spareval/src/eval/fixtures/source_path_query_cases.json",
        "vendor/spareval/src/eval/fixtures/source_path_query_scan_cases.json",
        "vendor/spareval/src/eval/fixtures/source_boolean_string_cases.json",
        "vendor/spareval/src/eval/fixtures/source_exists_ast_cases.json",
        "vendor/spareval/src/eval/fixtures/source_exists_separator_cases.json",
        "vendor/spareval/src/eval/fixtures/source_exists_mapping_cases.json",
        "vendor/spareval/src/eval/fixtures/source_exists_projection_instance_cases.json",
        "vendor/spareval/src/eval/fixtures/source_literal_parser_bridge_cases.json",
        "vendor/spareval/src/numeric/fixtures/source_literal_truth_cases.json",
        "vendor/spareval/src/numeric/fixtures/source_literal_numeric_token_cases.json",
        "vendor/spareval/src/numeric/fixtures/source_literal_error_truth_cases.json",
        "vendor/spareval/LICENSE-SIPHASH",
        "vendor/spareval/SOURCE-HASH-SET-PROVENANCE.md",
        "vendor/spareval/QUERY-NUMERIC-PROVENANCE.json",
        "vendor/spareval/UPSTREAM-PROVENANCE.json",
        "vendor/sparopt/UPSTREAM-PROVENANCE.json",
        "vendor/spareval/NULLABLE-CHAIN-PROVENANCE.json",
        "vendor/spareval/DESCRIBE-PATCH-PROVENANCE.json",
        "vendor/spargebra/DESCRIBE-PATCH-PROVENANCE.json",
        "crates/semantica-runtime/tests/fixtures/query-minus-rdflib-oracle.json",
        "crates/semantica-runtime/tests/fixtures/query-minus-rdflib-regression.json",
        "crates/semantica-runtime/tests/fixtures/query-bindings-original15.json",
        "crates/semantica-runtime/tests/fixtures/describe-context-original17.json",
        "crates/semantica-runtime/tests/fixtures/describe-modifier-original9.json",
        "crates/semantica-runtime/tests/fixtures/describe-sorted-target-original2.json",
        "crates/semantica-service/tests/fixtures/original-mcp-catalog.json",
        "crates/semantica-service/tests/fixtures/original-mcp-isolated-oracle.json",
        "crates/semantica-service/tests/fixtures/original-literal-object-oracle.json",
        "runtime/semantica-scope.json",
        "runtime/semantica-scope-lock.json",
        "runtime/semantica-migration-mappings.json",
    ] {
        if root.join(p).is_file() {
            out.insert(PathBuf::from(p));
        }
    }
    Ok(out.into_iter().collect())
}
