use base64::{Engine, engine::general_purpose::STANDARD};
use oe_core::{
    json::{canonical_json, strict_json},
    sha256,
};
use oe_semantic::{contracts::*, delta::*};
use serde_json::{Value, json};
use std::{
    io::{Cursor, Read},
    path::Path,
};

fn binding() -> ProjectBinding {
    parse_project_binding(&canonical_json(&json!({"$schema":BINDING_SCHEMA,"binding_id":"binding:test","project":{"project_id":"project:test","domain":"manufacturing"},
        "semantic_target":{"kind":"workspace","package_id":"method-test","workspace_id":"workspace:test"},
        "baseline":{"version":"1.0.0","digest":"a".repeat(64)},"evidence":{"logical_root":"evidence:project-test"},
        "authority":{"fact":{"authority_id":"authority:engineer","scope":["observations","measurements"]},"decision":{"authority_id":"authority:board","scope":["promotion","candidate-verdict"]}},
        "allowed_actions":KNOWN_ACTIONS,"lifecycle_actions":STATE_MODEL,"promotion":{"target":PROMOTION_TARGET,"requires_decision_authority":true},
        "created_at":"2026-09-27T10:00:00Z","semantic_api":NATIVE_REFINERY_CONTRACT})).unwrap()).unwrap()
}
fn source() -> Value {
    json!({"source_id":"source:test","uri":"evidence:project-test/observation","sha256":"b".repeat(64),"media_type":"application/json","captured_at":"2026-09-27T10:00:00Z"})
}
fn asset(category: &str, id: &str, operation: &str) -> Value {
    let payload = b"source-bound content";
    let (content, digest, old) = match operation {
        "add" => (
            json!(STANDARD.encode(payload)),
            sha256(payload),
            Value::Null,
        ),
        "replace" => (
            json!(STANDARD.encode(payload)),
            sha256(payload),
            json!("c".repeat(64)),
        ),
        "remove" => (Value::Null, "c".repeat(64), json!("c".repeat(64))),
        _ => unreachable!(),
    };
    json!({"category":category,"asset_id":id,"operation":operation,"media_type":"text/plain","sha256":digest,"content_base64":content,"replaces_sha256":old,"role":null,"case_kind":if category=="cases" {json!("positive")}else{Value::Null}})
}
fn delta() -> Value {
    let mut value = json!({"schema_version":"1.0","package_id":"method-test","base_version":"1.0.0","base_package_sha256":"a".repeat(64),"target_version":"2.0.0","rationale":"Source-bound method revision",
        "created_by":"authority:engineer","created_at":"2026-09-27T11:00:00Z","required_capabilities":["semantica.ontology.refinery/v1"],"source_evidence":[source()],"book_impact":"vol1-method"});
    for category in ASSET_CATEGORIES {
        value[category] = json!([]);
    }
    value["ontology"] = json!([asset("ontology", "ontology:main", "add")]);
    value
}
fn native_delta() -> Value {
    let mut value = delta();
    value["created_by"] = json!(authority_token(&binding().fact_authority).unwrap());
    value
}
fn parse(value: &Value) -> anyhow::Result<Value> {
    parse_delta(&canonical_json(value).unwrap(), &binding())
}
fn authorization(candidate: &str, action: &str) -> Value {
    json!({"$schema":AUTHORIZATION_SCHEMA,"authorization_id":"authorization:test","action":action,"actor_id":"authority:board","authority_id":"authority:board","authority_scope":["promotion","candidate-verdict"],"package_id":"method-test","delta_sha256":candidate,"promotion_target":PROMOTION_TARGET,"reason":"Explicit fixture approval","source":source(),"issued_at":"2026-09-27T12:00:00Z","decisions":[]})
}
fn parse_auth(value: &Value, candidate: &str, action: &str) -> anyhow::Result<Value> {
    parse_authorization(
        &canonical_json(value).unwrap(),
        &binding(),
        action,
        candidate,
    )
}
fn decision(asset: &Value) -> Value {
    json!({"category":asset["category"],"asset_id":asset["asset_id"],"operation":asset["operation"],"replaces_sha256":asset["replaces_sha256"],"verdict":"approve","reason":"Reviewed exact old bytes"})
}

#[test]
fn normalized_delta_binds_content_and_scoped_fact_authority() {
    let mut input = delta();
    input["ontology"][0]
        .as_object_mut()
        .unwrap()
        .shift_remove("role");
    let actual = parse(&input).unwrap();
    assert_eq!(
        actual["created_by"],
        authority_token(&binding().fact_authority).unwrap()
    );
    assert_eq!(actual["ontology"][0]["role"], Value::Null);
    let mut content = actual.clone();
    let digest = content
        .as_object_mut()
        .unwrap()
        .shift_remove("delta_sha256")
        .unwrap();
    assert_eq!(digest, sha256(canonical_json(&content).unwrap()));
    input["delta_sha256"] = digest;
    assert_eq!(parse(&input).unwrap(), actual);
    input["rationale"] = json!("Changed evaluation rationale");
    assert!(parse(&input).is_err());
}
#[test]
fn every_category_and_operation_are_validated_as_one_complete_delta() {
    let mut value = delta();
    for (index, category) in ASSET_CATEGORIES.iter().enumerate() {
        value[category] = json!([asset(
            category,
            &format!("asset:{index}"),
            ["add", "replace", "remove"][index % 3]
        )]);
    }
    let normalized = parse(&value).unwrap();
    for category in ASSET_CATEGORIES {
        assert_eq!(normalized[category], value[category]);
    }
    value["ontology"][0]["category"] = json!("rules");
    assert!(parse(&value).is_err());
}
#[test]
fn delta_requires_exact_creator_and_current_baseline() {
    for (field, value) in [
        (
            "created_by",
            json!(authority_token(&binding().fact_authority).unwrap()),
        ),
        ("created_by", json!("authority:other")),
        ("package_id", json!("other")),
        ("base_version", json!("0.9.0")),
        ("base_package_sha256", json!("d".repeat(64))),
    ] {
        let mut input = delta();
        input[field] = value;
        assert!(parse(&input).is_err(), "accepted {field}");
    }
    let mut input = native_delta();
    input["base_version"] = json!("0");
    input["base_package_sha256"] = json!(empty_package_sha256().unwrap());
    assert!(validate_native_delta(&input).is_ok());
    input["base_package_sha256"] = json!("0".repeat(64));
    assert!(validate_native_delta(&input).is_err());
}
#[test]
fn delta_rejects_unknown_missing_shadow_and_nonfinite_fields() {
    for field in ["backend", "fallback", "unknown"] {
        let mut input = delta();
        input[field] = json!(true);
        assert!(parse(&input).is_err());
    }
    let mut input = delta();
    input["ontology"][0]["fallback"] = json!("copy");
    assert!(parse(&input).is_err());
    for field in [
        "schema_version",
        "cases",
        "source_evidence",
        "required_capabilities",
    ] {
        let mut input = delta();
        input.as_object_mut().unwrap().shift_remove(field);
        assert!(parse(&input).is_err(), "accepted missing {field}");
    }
    let mut input = delta();
    input["ontology"][0]
        .as_object_mut()
        .unwrap()
        .shift_remove("content_base64");
    assert!(parse(&input).is_err());
    let raw = String::from_utf8(canonical_json(&delta()).unwrap()).unwrap();
    let duplicate = format!("{{\"target_version\":\"3.0.0\",{}", &raw[1..]);
    assert!(parse_delta(duplicate.as_bytes(), &binding()).is_err());
    assert!(parse_delta(b"{\"number\":NaN}", &binding()).is_err());
}
#[test]
fn assets_reject_bad_bytes_hashes_ids_and_conflicting_case_kind() {
    for (field, value) in [
        ("content_base64", json!("not base64!")),
        ("sha256", json!("0".repeat(64))),
        ("asset_id", json!("../../asset")),
        ("asset_id", json!("a".repeat(201))),
        ("operation", json!("merge")),
        ("media_type", json!(" text/plain ")),
        ("case_kind", json!("positive")),
        ("replaces_sha256", json!("c".repeat(64))),
    ] {
        let mut input = delta();
        input["ontology"][0][field] = value;
        assert!(parse(&input).is_err(), "accepted {field}");
    }
    let mut input = delta();
    input["cases"] = json!([asset("cases", "case:one", "add")]);
    input["cases"][0]["case_kind"] = Value::Null;
    assert!(parse(&input).is_err());
    input["cases"][0]["case_kind"] = json!("negative");
    assert!(parse(&input).is_ok());
}
#[test]
fn destructive_assets_require_exact_old_hash_and_no_ambiguous_payload() {
    let mut input = delta();
    input["ontology"] = json!([asset("ontology", "ontology:main", "replace")]);
    input["ontology"][0]["replaces_sha256"] = Value::Null;
    assert!(parse(&input).is_err());
    input["ontology"] = json!([asset("ontology", "ontology:main", "remove")]);
    assert!(parse(&input).is_ok());
    input["ontology"][0]["sha256"] = json!("d".repeat(64));
    assert!(parse(&input).is_err());
    input["ontology"][0]["sha256"] = json!("c".repeat(64));
    input["ontology"][0]["content_base64"] = json!("");
    assert!(parse(&input).is_err());
}
#[test]
fn global_asset_and_source_ids_are_unique_and_empty_delta_fails() {
    let mut input = delta();
    input["rules"] = json!([asset("rules", "ontology:main", "add")]);
    assert!(parse(&input).is_err());
    input = delta();
    input["source_evidence"]
        .as_array_mut()
        .unwrap()
        .push(source());
    assert!(parse(&input).is_err());
    input["source_evidence"][1]["source_id"] = json!("source:another");
    assert!(parse(&input).is_ok()); // native permits two identities at one logical URI
    input = delta();
    input["ontology"] = json!([]);
    assert!(parse(&input).is_err());
    input = delta();
    input["required_capabilities"] = json!(["same", "same"]);
    assert!(parse(&input).is_err());
    input["required_capabilities"] = json!([" padded "]);
    assert!(parse(&input).is_err());
}
#[test]
fn native_sources_are_logical_identifiers_never_resolved_paths() {
    for uri in [
        "file:///tmp/fact",
        "https://example.test/fact",
        "evidence:x/../secret",
        "evidence:x/%2e%2e/secret",
        "evidence:x%2f..%3fsecret",
        r"evidence:x\secret",
    ] {
        let mut input = delta();
        input["source_evidence"][0]["uri"] = json!(uri);
        assert!(parse(&input).is_err(), "accepted {uri}");
    }
    // Delta sources may include shared book material outside a project's root;
    // only task and action authorization evidence are restricted to that root.
    let mut input = delta();
    input["source_evidence"][0]["uri"] = json!("urn:shared-book:chapter-2");
    assert!(parse(&input).is_ok());
    input["source_evidence"][0]["captured_at"] = json!("2026-09-27T12:00:00");
    assert!(parse(&input).is_err());
}
#[test]
fn base64_preserves_native_padding_contract_and_empty_bytes() {
    for (encoded, payload) in [
        ("", b"".as_slice()),
        ("Zg==", b"f"),
        ("Zh==", b"f"),
        ("YWJj===", b"abc"),
    ] {
        let mut input = delta();
        input["ontology"][0]["content_base64"] = json!(encoded);
        input["ontology"][0]["sha256"] = json!(sha256(payload));
        assert!(parse(&input).is_ok(), "rejected {encoded}");
    }
    for encoded in ["=", "Zg", "Zg=", "Zg===", "Z g==", "Zg==a", "Z==="] {
        let mut input = delta();
        input["ontology"][0]["content_base64"] = json!(encoded);
        input["ontology"][0]["sha256"] = json!(sha256(b"f"));
        assert!(parse(&input).is_err(), "accepted {encoded}");
    }
}
#[test]
fn all_frozen_bootstrap_deltas_have_identical_native_normalization_and_digest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut count = 0;
    for archive in [
        "engineering-judgment-intake-0.2.2.bootstrap.zip",
        "engineering-evidence-methods-0.1.0.bootstrap.zip",
    ] {
        let bytes = oe_core::read_regular_bytes(
            &root.join("runtime/vendor").join(archive),
            64 * 1024 * 1024,
        )
        .unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).unwrap();
            if !entry.name().ends_with("/delta.json") {
                continue;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            let original = strict_json(&data).unwrap();
            assert_eq!(
                validate_native_delta(&original).unwrap(),
                original,
                "native capsule DTO drift: {} {}",
                archive,
                entry.name()
            );
            count += 1;
        }
    }
    assert_eq!(count, 6);
}
#[test]
fn authorization_projects_exact_scoped_actor_and_keeps_candidate_identity() {
    let candidate = parse(&delta()).unwrap()["delta_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let input = authorization(&candidate, "commit");
    let output = parse_auth(&input, &candidate, "commit").unwrap();
    let token = authority_token(&binding().decision_authority).unwrap();
    assert_eq!(output["actor_id"], token);
    assert_eq!(output["authority"], token);
    assert_eq!(output["source"], source());
    let mut content = output.clone();
    let digest = content
        .as_object_mut()
        .unwrap()
        .shift_remove("authorization_sha256")
        .unwrap();
    assert_eq!(digest, sha256(canonical_json(&content).unwrap()));
    assert_eq!(validate_native_authorization(&output).unwrap(), output);
}
#[test]
fn authorization_cannot_replay_other_action_candidate_actor_scope_or_target() {
    let candidate = "d".repeat(64);
    for (field, value) in [
        ("action", json!("promote")),
        ("actor_id", json!("actor:other")),
        ("authority_id", json!("authority:other")),
        ("authority_scope", json!(["promotion"])),
        ("package_id", json!("other")),
        ("delta_sha256", json!("e".repeat(64))),
        ("promotion_target", json!("project-registry")),
        (
            "authority_scope",
            json!(["promotion", "candidate-verdict", " promotion "]),
        ),
    ] {
        let mut input = authorization(&candidate, "commit");
        input[field] = value;
        assert!(
            parse_auth(&input, &candidate, "commit").is_err(),
            "accepted {field}"
        );
    }
    let input = authorization(&candidate, "commit");
    assert!(parse_auth(&input, &candidate, "unknown").is_err());
    assert!(parse_auth(&input, "not-a-sha", "commit").is_err());
}
#[test]
fn authorization_source_and_unknown_fields_fail_closed() {
    let candidate = "d".repeat(64);
    for (pointer, value) in [
        ("/source/uri", json!("evidence:project-test-other/x")),
        ("/source/sha256", json!("A".repeat(64))),
        ("/source/captured_at", json!("2026-09-27")),
        ("/issued_at", json!("2026-09-27T10:00:00")),
    ] {
        let mut input = authorization(&candidate, "commit");
        *input.pointer_mut(pointer).unwrap() = value;
        assert!(parse_auth(&input, &candidate, "commit").is_err());
    }
    let mut input = authorization(&candidate, "commit");
    input["authorization_sha256"] = json!("a".repeat(64));
    assert!(parse_auth(&input, &candidate, "commit").is_err());
    input = authorization(&candidate, "commit");
    input["source"]["fallback"] = json!("none");
    assert!(parse_auth(&input, &candidate, "commit").is_err());
}
#[test]
fn exact_destructive_decisions_reject_missing_extra_wrong_hash_and_promotion_replay() {
    let mut input = delta();
    let replacement = asset("ontology", "ontology:main", "replace");
    let removal = asset("rules", "rule:old", "remove");
    input["ontology"] = json!([replacement]);
    input["rules"] = json!([removal]);
    let delta = parse(&input).unwrap();
    let candidate = delta["delta_sha256"].as_str().unwrap();
    let mut auth = authorization(candidate, "commit");
    assert!(
        validate_asset_decisions(&delta, &parse_auth(&auth, candidate, "commit").unwrap()).is_err()
    );
    auth["decisions"] = json!([decision(&removal), decision(&replacement)]);
    let normalized = parse_auth(&auth, candidate, "commit").unwrap();
    assert_eq!(normalized["decisions"][0]["category"], "ontology");
    validate_asset_decisions(&delta, &normalized).unwrap();
    auth["decisions"][0]["replaces_sha256"] = json!("d".repeat(64));
    assert!(
        validate_asset_decisions(&delta, &parse_auth(&auth, candidate, "commit").unwrap()).is_err()
    );
    auth["decisions"] = json!([
        decision(&removal),
        decision(&replacement),
        decision(&asset("rules", "rule:unrelated", "remove"))
    ]);
    assert!(
        validate_asset_decisions(&delta, &parse_auth(&auth, candidate, "commit").unwrap()).is_err()
    );
    auth["decisions"] = json!([decision(&replacement), decision(&replacement)]);
    assert!(parse_auth(&auth, candidate, "commit").is_err());
    auth["decisions"] = json!([decision(&replacement)]);
    auth["action"] = json!("promote");
    assert!(parse_auth(&auth, candidate, "promote").is_err());
    auth["decisions"] = json!([]);
    validate_asset_decisions(&delta, &parse_auth(&auth, candidate, "promote").unwrap()).unwrap();
}
#[test]
fn decisions_cannot_hide_unknown_fields_or_unapproved_old_mutations() {
    let candidate = "d".repeat(64);
    let old = asset("ontology", "ontology:main", "replace");
    for (field, value) in [
        ("verdict", json!("unknown")),
        ("operation", json!("add")),
        ("replaces_sha256", json!("0")),
        ("category", json!("physical-pass")),
        ("asset_id", json!("../path")),
        ("auto_approve", json!(true)),
    ] {
        let mut input = authorization(&candidate, "commit");
        input["decisions"] = json!([decision(&old)]);
        input["decisions"][0][field] = value;
        assert!(
            parse_auth(&input, &candidate, "commit").is_err(),
            "accepted {field}"
        );
    }
}
