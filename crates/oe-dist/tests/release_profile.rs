use oe_dist::release_profile::{RC4, RC5, check};

fn source_features() -> Vec<String> {
    vec![
        "CARGO_FEATURE_RDFLIB760_NULLABLE_INTERNAL".into(),
        "CARGO_FEATURE_SOURCE_SESSION_LOCAL_V1".into(),
    ]
}

#[test]
fn old_rc4_feature_gate_is_not_weakened() {
    let old = check(RC4, "2.0.0-rc.4", &[]).unwrap();
    assert!(old.local_source_session_versions.is_empty());
    for observed in [
        source_features(),
        vec![source_features()[0].clone()],
        vec![source_features()[1].clone()],
    ] {
        assert_eq!(
            check(RC4, "2.0.0-rc.4", &observed).unwrap_err().to_string(),
            "release_semantic_profile_features_mismatch"
        );
    }
}

#[test]
fn rc5_requires_complete_exact_dependency_features() {
    let features = source_features();
    let received = check(RC5, "2.0.0-rc.5", &features).unwrap();
    assert_eq!(received.compiled_semantic_features, features);
    assert_eq!(received.public_query_versions, vec![1, 2, 3, 4]);
    assert_eq!(received.local_source_session_versions, vec![1, 2, 3]);
    let mut unknown = features.clone();
    unknown.push("CARGO_FEATURE_UNREVIEWED".into());
    let mut duplicate = features.clone();
    duplicate.push(features[1].clone());
    let mut reversed = features.clone();
    reversed.reverse();
    for changed in [
        vec![],
        vec![features[0].clone()],
        vec![features[1].clone()],
        unknown,
        duplicate,
        reversed,
    ] {
        assert_eq!(
            check(RC5, "2.0.0-rc.5", &changed).unwrap_err().to_string(),
            "release_semantic_profile_features_mismatch"
        );
    }
}

#[test]
fn release_profile_versions_cannot_inherit_qualification() {
    for (profile, version, features) in [
        (RC4, "2.0.0-rc.5", vec![]),
        (RC5, "2.0.0-rc.4", source_features()),
        (RC5, "2.0.0-rc.5-private", source_features()),
    ] {
        assert_eq!(
            check(profile, version, &features).unwrap_err().to_string(),
            "release_semantic_profile_version_mismatch"
        );
    }
    assert_eq!(
        check("oev2.rc5-local-source/2", "2.0.0-rc.5", &source_features())
            .unwrap_err()
            .to_string(),
        "release_semantic_profile_unsupported"
    );
}
