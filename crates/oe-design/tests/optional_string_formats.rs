use oe_design::Design;
use serde_json::json;

// A minimal design independent of the evolving engineering specification.
// Expected mappings are the public Rust data shape and optional-field contract.
fn design(rust_type: &str, required: bool, format: &str, values: &[&str]) -> Design {
    serde_json::from_value(json!({
        "schema": "oev2.design/1",
        "version": "optional-string-formats-test/1",
        "problem": "Preserve absent diagnostics and reject invalid present values",
        "purpose": "Normalize optional SHA and status contracts without weakening required fields",
        "requirements": [{
            "id": "req_optional",
            "function": "Represent optional structured diagnostic values",
            "condition": "Present values obey their declared format",
            "acceptance": "Valid mappings survive normalization and incompatible mappings fail"
        }],
        "modules": [{
            "id": "contracts",
            "kind": "contract",
            "rust_path": "crates/oe-contracts",
            "purpose": "Own typed shared diagnostic contracts",
            "realizes": ["req_optional"],
            "platform": "Rust supported platforms",
            "effects": "Validate values without external effects",
            "failures": "Reject invalid format mappings",
            "status": "source_only"
        }],
        "types": [{
            "name": "DiagnosticExample",
            "meaning": "Optional diagnostic format example",
            "identity": "Independent specification normalization fixture",
            "fields": [{
                "name": "value",
                "meaning": "Absent value remains absent; a present value obeys the format",
                "rust_type": rust_type,
                "required": required,
                "format": format,
                "unit": "UTF-8 bytes",
                "precision": "Exact SHA or allowed status membership",
                "write": "Bounded diagnostic adapter",
                "applicability": "Optional or required string format",
                "acceptance": "None permits absence; Some invalid strings are rejected by generated validation",
                "allowed_values": values
            }]
        }]
    }))
    .unwrap()
}

#[test]
fn optional_and_required_string_formats_preserve_normalized_mappings() {
    for (rust_type, required) in [("String", true), ("Option<String>", false)] {
        for (format, values) in [
            ("sha256", Vec::new()),
            ("status", vec!["unknown", "context_length_exceeded"]),
        ] {
            let input = design(rust_type, required, format, &values);
            let normalized = input.clone().normalize().unwrap();
            let field = &normalized.types[0].fields[0];
            assert_eq!(field.rust_type, rust_type);
            assert_eq!(field.required, required);
            assert_eq!(field.format, format);
            let mut expected: Vec<_> = values.iter().map(|value| value.to_string()).collect();
            expected.sort();
            assert_eq!(field.allowed_values, expected);
            assert_eq!(normalized.canonical().unwrap(), input.canonical().unwrap());
            assert!(normalized.rdf().is_ok());
        }
    }
}

#[test]
fn formats_reject_non_string_mappings_and_incompatible_required_flags() {
    for (rust_type, required, format, expected_error) in [
        ("bool", true, "status", "status_values_missing"),
        ("Option<u64>", false, "status", "status_values_missing"),
        ("Vec<String>", true, "status", "status_values_missing"),
        ("bool", true, "sha256", "sha256_mapping_invalid"),
        ("Option<u64>", false, "sha256", "sha256_mapping_invalid"),
        ("Vec<String>", true, "sha256", "sha256_mapping_invalid"),
        (
            "Option<String>",
            true,
            "status",
            "field_required_mapping_conflict",
        ),
        ("String", false, "status", "field_required_mapping_conflict"),
        (
            "Option<String>",
            true,
            "sha256",
            "field_required_mapping_conflict",
        ),
        ("String", false, "sha256", "field_required_mapping_conflict"),
    ] {
        let input = design(rust_type, required, format, &["unknown"]);
        let error = input.normalize().unwrap_err().to_string();
        assert!(
            error.starts_with(expected_error),
            "{rust_type}/{required}/{format}: {error}"
        );
    }
}

#[test]
fn optional_status_requires_the_same_strict_whitelist_as_required_status() {
    for (rust_type, required) in [("String", true), ("Option<String>", false)] {
        for (values, expected_error) in [
            (vec![], "status_values_missing"),
            (vec!["unknown", "unknown"], "field_allowed_values_invalid"),
            (vec![""], "field_allowed_values_invalid"),
            (vec![" \n\t"], "field_allowed_values_invalid"),
            (vec!["unknown\0"], "field_allowed_values_invalid"),
        ] {
            let error = design(rust_type, required, "status", &values)
                .normalize()
                .unwrap_err()
                .to_string();
            assert_eq!(error, expected_error, "{rust_type}: {values:?}");
        }
    }
}
