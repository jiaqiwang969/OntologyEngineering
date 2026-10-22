use oe_design::Design;
use serde_json::json;

fn independent_design(
    rust_type: &str,
    required: bool,
    format: &str,
    group: &str,
    values: &[&str],
) -> Design {
    serde_json::from_value(json!({
        "schema":"oev2.design/1","version":"declared-domains-test/1",
        "problem":"Every declared closed domain must survive program generation",
        "purpose":"Preserve optional absence and enforce the exact permitted values",
        "requirements":[{"id":"req_domain","function":"Validate declared string domains",
            "condition":"Only explicitly permitted present values satisfy a closed domain",
            "acceptance":"Both groups reject invalid present values without silently dropping declarations"}],
        "modules":[{"id":"domain_contract","contract_group":group,"kind":"contract",
            "rust_path":if group=="semantic" {"crates/semantica-contracts"} else {"crates/oe-contracts"},
            "purpose":"Own a typed closed domain","realizes":["req_domain"],
            "platform":"Rust supported platforms","effects":"Pure value validation",
            "failures":"Unsupported representations are rejected","status":"source_only"}],
        "types":[{"name":"DeclaredDomain","contract_group":group,
            "meaning":"An independently declared optional or required closed string domain",
            "identity":"Input value and exact permitted set",
            "fields":[{"name":"value","meaning":"Membership applies to every present value",
                "rust_type":rust_type,"required":required,"format":format,"unit":"UTF-8 bytes",
                "precision":"Exact declared membership","write":"Independent fixture",
                "applicability":"Present values of declared string fields",
                "acceptance":"None preserves absence; a present value must belong to the declared set",
                "allowed_values":values}]}]
    })).unwrap()
}

#[test]
fn string_domains_preserve_values_in_both_contract_groups() {
    for group in ["engineering", "semantic"] {
        for (rust_type, required) in [("String", true), ("Option<String>", false)] {
            for format in ["id", "raw_utf8", "status"] {
                let d = independent_design(
                    rust_type,
                    required,
                    format,
                    group,
                    &["qualification", "analysis"],
                );
                if format == "raw_utf8" && rust_type == "Option<String>" {
                    assert_eq!(
                        d.normalize().unwrap_err().to_string(),
                        "raw_utf8_contract_requires_string"
                    );
                    continue;
                }
                let normalized = d.normalize().unwrap();
                assert_eq!(
                    normalized.types[0].fields[0].allowed_values,
                    ["analysis", "qualification"]
                );
                assert!(
                    normalized
                        .rdf()
                        .unwrap()
                        .contains("<urn:oev2:design:allowedValueRust>")
                );
            }
        }
    }
}

#[test]
fn unsupported_closed_domain_representations_are_refused() {
    for group in ["engineering", "semantic"] {
        for (rust_type, required) in [
            ("bool", true),
            ("Option<u64>", false),
            ("Vec<String>", true),
        ] {
            let d = independent_design(
                rust_type,
                required,
                "id",
                group,
                &["qualification", "analysis"],
            );
            let error = d
                .normalize()
                .expect_err("an unimplemented closed-domain mapping must not be silently accepted");
            assert_eq!(
                error.to_string(),
                "field_allowed_values_mapping_invalid",
                "{group}/{rust_type}"
            );
        }
    }
}

#[test]
fn existing_status_and_digest_mapping_errors_keep_their_identity() {
    assert_eq!(
        independent_design("bool", true, "status", "engineering", &["unknown"])
            .normalize()
            .unwrap_err()
            .to_string(),
        "status_values_missing"
    );
    assert_eq!(
        independent_design("bool", true, "sha256", "semantic", &["unknown"])
            .normalize()
            .unwrap_err()
            .to_string(),
        "sha256_mapping_invalid"
    );
    assert_eq!(
        independent_design(
            "String",
            true,
            "id",
            "semantic",
            &["duplicate", "duplicate"]
        )
        .normalize()
        .unwrap_err()
        .to_string(),
        "field_allowed_values_invalid"
    );
}
