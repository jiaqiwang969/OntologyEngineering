mod common;
use common::Fixture;
use oe_contracts::{EngineeringAdoptionRequest, ObjectIdentity};
use oe_engineering::{adoption, artifacts};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

fn put(f: &Fixture, path: &str, v: &Value) -> oe_contracts::ArtifactReference {
    fs::write(f.temp.path().join(path), serde_json::to_vec(v).unwrap()).unwrap();
    artifacts::reference(&f.root, &f.temp.path().join(path)).unwrap()
}
fn fixture() -> (Fixture, EngineeringAdoptionRequest) {
    let f = Fixture::new("adoption-synthetic", "required_semantic_review");
    let mut p: oe_kernel::project::Project =
        serde_json::from_value(f.read("var/inputs/project.json")).unwrap();
    let mut b: oe_kernel::Binding =
        serde_json::from_value(f.read("var/inputs/binding.json")).unwrap();
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":b.object_id,"object_revision":"r1",
        "purpose":p.purpose,"acceptance":["source applicability within explicitly declared fluid scope"],
        "facts":[{"property":"fluid","value":"glycol","status":"declared","source_refs":["fixture"]}],
        "assertions":[{"assertion_id":"fluid-scope","kind":"source_applicability","subject":"source",
            "requirements":{"limits":{"fluid":["glycol"]}},"source_refs":["fixture"],"affected_decisions":["select"]}]});
    p.conditions.insert(
        b.condition_scope.clone(),
        serde_json::to_string(&basis).unwrap(),
    );
    let old = fs::read_to_string(f.temp.path().join("var/inputs/evidence.ttl")).unwrap();
    let raw = old.replace(
        "ee:established_conditions \"flow\" ;",
        "ee:established_conditions \"flow\", \"fluid\" ;",
    ) + &format!(
        "\n<{}> <urn:oev2:project-context:conditionSha256> \"{}\" .",
        b.object_id,
        oe_core::sha256(p.conditions[&b.condition_scope].as_bytes())
    );
    fs::write(f.temp.path().join("var/inputs/current.ttl"), raw.as_bytes()).unwrap();
    b.source_sha256 = oe_core::sha256(raw.as_bytes());
    p.evidence.insert("fixture".into(), b.source_sha256.clone());
    let object = ObjectIdentity {
        object_id: b.object_id.clone(),
        revision: b.object_revision.clone(),
        condition_scope: b.condition_scope.clone(),
        source_sha256: b.source_sha256.clone(),
    };
    p.decisions.push(oe_kernel::project::Decision {
        id: "select".into(),
        object_id: b.object_id.clone(),
        object_revision: "r1".into(),
        condition_scope: b.condition_scope.clone(),
        source_sha256: b.source_sha256.clone(),
        conclusion: "unknown".into(),
        adopted: false,
        basis: vec![],
    });
    let engineering = oe_kernel::engineering::checks(&p, &object);
    assert!(
        engineering.iter().all(|c| c.conclusion == "pass"),
        "{engineering:?}"
    );
    b.required_checks
        .extend(engineering.into_iter().map(|c| c.id));
    b.project_context = Some(oe_kernel::project::context(&p, &object).unwrap());
    let project = put(
        &f,
        "var/inputs/current-project.json",
        &serde_json::to_value(&p).unwrap(),
    );
    let binding = put(
        &f,
        "var/inputs/current-binding.json",
        &serde_json::to_value(&b).unwrap(),
    );
    let evidence =
        artifacts::reference(&f.root, &f.temp.path().join("var/inputs/current.ttl")).unwrap();
    let approval = put(
        &f,
        "var/inputs/approval.json",
        &json!({"schema":"oev2.engineering-adoption-approval/1",
        "approver":"reviewer","decision_id":"select","claim_scope":"declared_fixture","project_context":b.project_context,
        "sources":{"fixture":evidence.sha256}}),
    );
    let policy = f.temp.path().join(adoption::POLICY);
    fs::create_dir_all(policy.parent().unwrap()).unwrap();
    fs::write(&policy,serde_json::to_vec(&json!({"schema":"oev2.engineering-adoption-authority/1","project_id":p.project_id,
        "decision_id":"select","claim_scope":"declared_fixture","actor_scopes":{"designer":["declared_fixture"]},
        "approvers":["reviewer"],"approved_approvals":[approval.sha256],"bundle":"engineering-judgment-intake","approved_package_sha256":b.package_sha256,"required_checks":b.required_checks,
        "required_sources":{"fixture":evidence}})).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&policy, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(windows)]
    common::windows_authority::set_policy_acl(&policy, false);
    let request = EngineeringAdoptionRequest {
        schema: "oev2.engineering-adoption-request/1".into(),
        actor: "designer".into(),
        decision_id: "select".into(),
        claim_scope: "declared_fixture".into(),
        project,
        binding,
        evidence,
        bundle: "engineering-judgment-intake".into(),
        query_ids: vec!["findings-pattern-scope".into()],
        shape_ids: vec!["shape-pattern-scope".into()],
        approval,
    };
    (f, request)
}
#[test]
fn adoption_rechecks_scope_sources_and_independent_authority_without_rewriting_input() {
    let (f, r) = fixture();
    let before = artifacts::read(&f.root, &r.project).unwrap();
    let result = adoption::adopt(&f.root, &r, Path::new("var/adoption-a")).unwrap();
    assert_eq!(before, artifacts::read(&f.root, &r.project).unwrap());
    let record = artifacts::json(&f.root, &result.record).unwrap();
    assert_eq!(
        adoption::verify(&f.root, &record).unwrap()["status"],
        "current_scope_bound"
    );
    let mut unauthorized = r.clone();
    unauthorized.actor = "unknown".into();
    assert!(adoption::adopt(&f.root, &unauthorized, Path::new("var/unauthorized")).is_err());
    assert!(!f.temp.path().join("var/unauthorized").exists());
    let mut false_scope = record.clone();
    false_scope["claim_scope"] = json!("manufacturing acceptance");
    assert!(adoption::verify(&f.root, &false_scope).is_err());
    let mut wrong_executor = record.clone();
    wrong_executor["executable_sha256"] = json!("0".repeat(64));
    assert!(adoption::verify(&f.root, &wrong_executor).is_err());
    fs::write(f.temp.path().join(&r.evidence.path), b"changed source").unwrap();
    assert!(adoption::verify(&f.root, &record).is_err());
    assert!(adoption::adopt(&f.root, &r, Path::new("var/stale")).is_err());
}
#[test]
fn declarations_self_approval_unknown_and_new_conditions_do_not_authorize_adoption() {
    let (f, mut r) = fixture();
    let mut approval = artifacts::json(&f.root, &r.approval).unwrap();
    approval["approver"] = json!("designer");
    r.approval = put(&f, "var/inputs/self-approval.json", &approval);
    let mut p: Value =
        serde_json::from_slice(&fs::read(f.temp.path().join(adoption::POLICY)).unwrap()).unwrap();
    p["approvers"] = json!(["reviewer", "designer"]);
    p["approved_approvals"] = json!([r.approval.sha256]);
    fs::write(
        f.temp.path().join(adoption::POLICY),
        serde_json::to_vec(&p).unwrap(),
    )
    .unwrap();
    assert!(adoption::adopt(&f.root, &r, Path::new("var/self")).is_err());
    let (f, mut r) = fixture();
    let approval = artifacts::json(&f.root, &r.approval).unwrap();
    fs::write(
        f.temp.path().join("var/inputs/renamed-approval.json"),
        serde_json::to_vec_pretty(&approval).unwrap(),
    )
    .unwrap();
    r.approval = artifacts::reference(
        &f.root,
        &f.temp.path().join("var/inputs/renamed-approval.json"),
    )
    .unwrap();
    assert!(adoption::adopt(&f.root, &r, Path::new("var/unbound-approval")).is_err());
    let (f, mut r) = fixture();
    let mut p = artifacts::json(&f.root, &r.project).unwrap();
    p["decisions"][0]["adopted"] = json!(true);
    p["decisions"][0]["conclusion"] = json!("supported");
    p["decisions"][0]["basis"] = json!(["manual declaration"]);
    p["conditions"]["declared_fixture"] = json!("new fluid; new boundary");
    r.project = put(&f, "var/inputs/declared.json", &p);
    assert!(adoption::adopt(&f.root, &r, Path::new("var/declaration")).is_err());
    let (f, mut r) = fixture();
    let raw = fs::read_to_string(f.temp.path().join(&r.evidence.path))
        .unwrap()
        .replace(
            "ee:established_conditions \"flow\", \"fluid\" ;",
            "ee:established_conditions \"flow\" ;",
        );
    fs::write(f.temp.path().join("var/inputs/unknown.ttl"), raw).unwrap();
    r.evidence =
        artifacts::reference(&f.root, &f.temp.path().join("var/inputs/unknown.ttl")).unwrap();
    assert!(adoption::adopt(&f.root, &r, Path::new("var/unknown")).is_err());
}

// A data-only stored ZIP fixture avoids a second production transport or external tool.
fn stored_zip(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }
    fn word(out: &mut Vec<u8>, n: u16) {
        out.extend_from_slice(&n.to_le_bytes());
    }
    fn dword(out: &mut Vec<u8>, n: u32) {
        out.extend_from_slice(&n.to_le_bytes());
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in files {
        let offset = u32::try_from(out.len()).unwrap();
        let length = u32::try_from(bytes.len()).unwrap();
        let name_length = u16::try_from(name.len()).unwrap();
        let crc = crc32(bytes);
        dword(&mut out, 0x0403_4b50);
        for n in [20, 0, 0, 0, 0] {
            word(&mut out, n);
        }
        for n in [crc, length, length] {
            dword(&mut out, n);
        }
        word(&mut out, name_length);
        word(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(bytes);
        dword(&mut central, 0x0201_4b50);
        for n in [20, 20, 0, 0, 0, 0] {
            word(&mut central, n);
        }
        for n in [crc, length, length] {
            dword(&mut central, n);
        }
        for n in [name_length, 0, 0, 0, 0] {
            word(&mut central, n);
        }
        dword(&mut central, 0);
        dword(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let offset = u32::try_from(out.len()).unwrap();
    out.extend_from_slice(&central);
    dword(&mut out, 0x0605_4b50);
    for n in [
        0,
        0,
        u16::try_from(files.len()).unwrap(),
        u16::try_from(files.len()).unwrap(),
    ] {
        word(&mut out, n);
    }
    dword(&mut out, u32::try_from(central.len()).unwrap());
    dword(&mut out, offset);
    word(&mut out, 0);
    out
}

#[test]
fn unchanged_approval_cannot_authorize_replaced_rules_under_the_same_private_bundle_name() {
    let (f, mut r) = fixture();
    let original = oe_semantic::load_bundle(f.root.path(), &r.bundle).unwrap();
    let private = "private:var/inputs/adoption-rule-spec.json";
    put(&f, "var/inputs/adoption-rule-spec.json", &original.spec);
    r.bundle = private.into();
    let policy_path = f.temp.path().join(adoption::POLICY);
    let mut policy = f.read(adoption::POLICY);
    policy["bundle"] = json!(private);
    fs::write(&policy_path, serde_json::to_vec(&policy).unwrap()).unwrap();
    adoption::adopt(&f.root, &r, Path::new("var/original-private-rule")).unwrap();
    let policy_before = fs::read(&policy_path).unwrap();
    let approval_before = artifacts::read(&f.root, &r.approval).unwrap();
    let project_before = artifacts::read(&f.root, &r.project).unwrap();
    let source_before = artifacts::read(&f.root, &r.evidence).unwrap();

    // Replace a real selected rule with a query that suppresses all findings.
    let mut files = original.payload;
    let query = "assets/findings-pattern-scope.rq";
    let replacement = b"SELECT ?status WHERE { FILTER(false) }\n".to_vec();
    assert_ne!(files[query], replacement);
    files.insert(query.into(), replacement);
    let mut manifest: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
    for asset in manifest["assets"].as_array_mut().unwrap() {
        let path = asset["path"].as_str().unwrap();
        asset["sha256"] = json!(oe_core::sha256(&files[path]));
    }
    files.insert(
        "manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    let archive = stored_zip(&files);
    fs::write(
        f.temp.path().join("var/inputs/replaced-rules.zip"),
        &archive,
    )
    .unwrap();
    let mut spec = original.spec;
    spec["path"] = json!("var/inputs/replaced-rules.zip");
    spec["sha256"] = json!(oe_core::sha256(&archive));
    spec["manifest_sha256"] = json!(oe_core::sha256(&files["manifest.json"]));
    for file in spec["files"].as_array_mut().unwrap() {
        let path = file["path"].as_str().unwrap();
        file["sha256"] = json!(oe_core::sha256(&files[path]));
    }
    put(&f, "var/inputs/adoption-rule-spec.json", &spec);
    let loaded = oe_semantic::load_bundle(f.root.path(), private).unwrap();
    assert_eq!(loaded.spec["sha256"], spec["sha256"]);
    assert_eq!(loaded.payload[query], files[query]);
    let mut binding = artifacts::json(&f.root, &r.binding).unwrap();
    assert_ne!(binding["package_sha256"], spec["sha256"]);
    binding["package_sha256"] = spec["sha256"].clone();
    r.binding = put(&f, "var/inputs/replaced-rule-binding.json", &binding);

    // The authority, approval, engineering object/conditions and all source bytes stay exact.
    assert_eq!(fs::read(&policy_path).unwrap(), policy_before);
    assert_eq!(
        artifacts::read(&f.root, &r.approval).unwrap(),
        approval_before
    );
    assert_eq!(
        artifacts::read(&f.root, &r.project).unwrap(),
        project_before
    );
    assert_eq!(
        artifacts::read(&f.root, &r.evidence).unwrap(),
        source_before
    );
    let attempted = adoption::adopt(&f.root, &r, Path::new("var/replaced-rule-adoption"));
    assert!(
        attempted.is_err(),
        "a valid new rule archive reused the original approval: {attempted:?}"
    );
    assert!(
        attempted
            .unwrap_err()
            .to_string()
            .contains("adoption_package_not_authority_bound")
    );
    assert!(!f.temp.path().join("var/replaced-rule-adoption").exists());
}

#[test]
fn adoption_requires_an_explicit_exact_package_pin_without_inventing_authority() {
    let (f, r) = fixture();
    let policy_path = f.temp.path().join(adoption::POLICY);
    let mut policy = f.read(adoption::POLICY);
    policy
        .as_object_mut()
        .unwrap()
        .remove("approved_package_sha256");
    let bytes = serde_json::to_vec(&policy).unwrap();
    fs::write(&policy_path, &bytes).unwrap();
    let missing = adoption::adopt(&f.root, &r, Path::new("var/missing-package-pin"));
    assert!(
        missing
            .unwrap_err()
            .to_string()
            .contains("approved_package_sha256")
    );
    assert_eq!(fs::read(&policy_path).unwrap(), bytes);
    assert!(!f.temp.path().join("var/missing-package-pin").exists());

    policy["approved_package_sha256"] = json!("0".repeat(64));
    let bytes = serde_json::to_vec(&policy).unwrap();
    fs::write(&policy_path, &bytes).unwrap();
    let wrong = adoption::adopt(&f.root, &r, Path::new("var/wrong-package-pin"));
    assert!(
        wrong
            .unwrap_err()
            .to_string()
            .contains("adoption_package_not_authority_bound")
    );
    assert_eq!(fs::read(&policy_path).unwrap(), bytes);
    assert!(!f.temp.path().join("var/wrong-package-pin").exists());
}

#[cfg(windows)]
#[test]
fn adoption_rejects_relaxed_windows_policy_without_repairing_or_granting() {
    let (f, r) = fixture();
    let original_project = artifacts::read(&f.root, &r.project).unwrap();
    let policy_path = f.root.path().join(adoption::POLICY);
    let original_policy = fs::read(&policy_path).unwrap();
    let accepted = adoption::adopt(&f.root, &r, Path::new("var/windows-private-policy")).unwrap();
    let record = artifacts::json(&f.root, &accepted.record).unwrap();
    adoption::verify(&f.root, &record).unwrap();
    common::windows_authority::set_policy_acl(&policy_path, true);
    let failure = adoption::adopt(&f.root, &r, Path::new("var/windows-public-policy")).unwrap_err();
    assert_eq!(
        failure.to_string(),
        "private_identity_file_permissions_required"
    );
    assert!(!f.root.path().join("var/windows-public-policy").exists());
    assert!(adoption::verify(&f.root, &record).is_err());
    assert!(artifacts::private_policy(&f.root, adoption::POLICY).is_err());
    assert_eq!(
        artifacts::read(&f.root, &r.project).unwrap(),
        original_project
    );
    assert_eq!(fs::read(&policy_path).unwrap(), original_policy);
}
