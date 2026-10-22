//! Versioned compile-profile checks. Behavior, transport, platform qualification
//! and actual selection retain their existing owners and separate receipts.
use anyhow::{Result, ensure};
use oe_contracts::ReleaseSemanticProfile;

pub const RC4: &str = "oev2.rc4-feature-off/1";
pub const RC5: &str = "oev2.rc5-local-source/1";
const SOURCE_FEATURES: [&str; 2] = [
    "CARGO_FEATURE_RDFLIB760_NULLABLE_INTERNAL",
    "CARGO_FEATURE_SOURCE_SESSION_LOCAL_V1",
];

/// Caller must obtain version/features from the actual compiled runtime and bind
/// the current source. No user-supplied feature declaration is accepted by CLI.
pub fn check(
    profile: &str,
    version: &str,
    compiled_semantic_features: &[String],
) -> Result<ReleaseSemanticProfile> {
    let (expected_version, expected_features, session_versions): (&str, &[&str], Vec<u64>) =
        match profile {
            RC4 => ("2.0.0-rc.4", &[], vec![]),
            RC5 => ("2.0.0-rc.5", &SOURCE_FEATURES, vec![1, 2, 3]),
            _ => anyhow::bail!("release_semantic_profile_unsupported"),
        };
    ensure!(
        version == expected_version,
        "release_semantic_profile_version_mismatch"
    );
    // Exact ordered observation: duplicates, unexpected flags and absent flags
    // must not be normalized into an eligible build.
    ensure!(
        compiled_semantic_features
            .iter()
            .map(String::as_str)
            .eq(expected_features.iter().copied()),
        "release_semantic_profile_features_mismatch"
    );
    Ok(ReleaseSemanticProfile {
        schema: "oev2.release-semantic-profile/1".into(),
        profile: profile.into(),
        version: version.into(),
        compiled_semantic_features: compiled_semantic_features.to_vec(),
        public_query_versions: vec![1, 2, 3, 4],
        local_source_session_versions: session_versions,
        scope: "compiled_profile_matches; declared admission requires independent behavior receiving; no installation, platform, engineering or method qualification".into(),
    })
}
