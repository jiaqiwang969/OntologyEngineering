//! Source identity controls only; no native or physical-success fixture.
use oe_contracts::ObjectIdentity;
use oe_kernel::project::{
    Project,
    cad_focus::{bind_cad_focus, check_declaration, decode_focus},
    context,
};
use serde_json::json;

fn input(id: &str) -> (Vec<u8>, ObjectIdentity, oe_contracts::ProjectReviewContext) {
    let object = ObjectIdentity {
        object_id: id.into(),
        revision: "r2".into(),
        condition_scope: "study".into(),
        source_sha256: "a".repeat(64),
    };
    let project: Project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"project","revision":"r1","purpose":"fixture",
        "objects":{id:"r2"},"conditions":{"study":"original condition text"},"evidence":{"original":"a".repeat(64)},"decisions":[],"dependencies":{}})).unwrap();
    let current = context(&project, &object).unwrap();
    (serde_json::to_vec(&project).unwrap(), object, current)
}

#[test]
fn raw_ids_and_every_field_roundtrip_without_identity_rewrite() {
    for id in ["case-000210", "urn:part:original", "对象 é e\u{301} /\\\n"] {
        let (bytes, object, current) = input(id);
        let focus = bind_cad_focus(&bytes, &object, &current).unwrap();
        let expected = json!({"schema":"oev2.cad-engineering-focus-context/2","project_bytes_sha256":oe_core::sha256(&bytes),"object":object,"project_context":current});
        let encoded = focus
            .iri()
            .strip_prefix("urn:oev2:cad-engineering-focus:2:")
            .unwrap();
        let decoded: Vec<_> = encoded
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(oe_core::json::strict_json(&decoded).unwrap(), expected);
        assert_eq!(
            serde_json::to_value(decode_focus(focus.iri()).unwrap()).unwrap(),
            expected
        );
        assert_eq!(focus.context().object.object_id, id);
        assert_eq!(
            check_declaration(&focus, &focus.declaration_graph().unwrap()).conclusion,
            "pass"
        );
        let wrong = focus.declaration_graph().unwrap();
        let wrong = String::from_utf8(wrong)
            .unwrap()
            .replace("original-never-present", "ignored")
            .replace(&current.condition_sha256, "0");
        assert_eq!(
            check_declaration(&focus, wrong.as_bytes()).conclusion,
            "violated"
        );
    }
}

#[test]
fn current_context_and_complete_project_bytes_prevent_same_revision_reuse() {
    let (bytes, object, current) = input("case-000210");
    let focus = bind_cad_focus(&bytes, &object, &current).unwrap();
    let mut project: Project = serde_json::from_slice(&bytes).unwrap();
    project.decisions.push(oe_kernel::project::Decision {
        id: "d".into(),
        object_id: object.object_id.clone(),
        object_revision: "r2".into(),
        condition_scope: "study".into(),
        source_sha256: "a".repeat(64),
        conclusion: "unknown".into(),
        adopted: false,
        basis: vec![],
    });
    let changed = serde_json::to_vec(&project).unwrap();
    assert_eq!(context(&project, &object).unwrap(), current); // context intentionally excludes decisions
    assert!(
        focus
            .ensure_current(&changed)
            .unwrap_err()
            .to_string()
            .contains("cad_focus_project_bytes_changed")
    );
    assert_ne!(
        bind_cad_focus(&changed, &object, &current).unwrap().iri(),
        focus.iri()
    );
    project
        .conditions
        .insert("study".into(), "changed same named condition".into());
    assert!(bind_cad_focus(&serde_json::to_vec(&project).unwrap(), &object, &current).is_err());
    for field in [
        "object_id",
        "object_revision",
        "condition_scope",
        "condition_sha256",
        "project_revision",
        "project_sha256",
        "source_sha256",
    ] {
        let mut wrong = serde_json::to_value(&current).unwrap();
        wrong[field] = json!("wrong");
        let wrong = serde_json::from_value(wrong).unwrap();
        assert!(bind_cad_focus(&bytes, &object, &wrong).is_err(), "{field}");
    }
}

#[test]
fn caller_declarations_do_not_restore_verified_state_and_mapping_is_bounded() {
    let (bytes, object, current) = input("plain");
    let focus = bind_cad_focus(&bytes, &object, &current).unwrap();
    assert!(decode_focus(&focus.iri().to_uppercase()).is_err());
    assert!(decode_focus("urn:other:22").is_err());
    assert!(
        decode_focus(&(String::from("urn:oev2:cad-engineering-focus:2:") + &"00".repeat(8193)))
            .is_err()
    );
    let mut original = serde_json::to_value(focus.context()).unwrap();
    original["verified"] = json!(true);
    let encoded = oe_wire::json::canonical_json(&original)
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert!(decode_focus(&(String::from("urn:oev2:cad-engineering-focus:2:") + &encoded)).is_err());
    let (bytes, object, current) = input(&"x".repeat(8192));
    assert!(
        bind_cad_focus(&bytes, &object, &current)
            .unwrap_err()
            .to_string()
            .contains("payload_budget")
    );
}
