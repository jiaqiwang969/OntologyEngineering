use oe_model_credentials::{CredentialPool, provider_transport::NativeCredentialGuard};
use semantica_contracts::*;
use serde_json::json;
#[test]
fn native_guard_borrows_same_pool_without_lease_and_redacts_result() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let path = root.join(format!("var/synthetic-native-guard-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    let key = path.join("key");
    std::fs::write(&key, &["sk-", "SYNTHETIC_NATIVE_GUARD_ONLY"].concat()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let pool = CredentialPool::read_single_http_bearer(&key).unwrap();
    let before = pool.status();
    let guard = NativeCredentialGuard::new(&pool);
    let mut result = SemanticNativeResult {
        schema: "semantica.native-result/1".into(),
        execution: "completed".into(),
        profile: "semantica.native-candle-f32-cpu/1".into(),
        request_sha256: "1".repeat(64),
        model_sha256: "2".repeat(64),
        parameters: json!({}),
        token_inputs: json!({}),
        output: json!("safe"),
        usage: SemanticProviderUsage {
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
        },
        usage_status: "unknown".into(),
        execution_identity: "synthetic-owner-guard".into(),
    };
    assert!(
        guard
            .check_native_result(&result)
            .unwrap()
            .credential_material_absent
    );
    result.output = json!({"nested":&["sk-", "SYNTHETIC_NATIVE_GUARD_ONLY"].concat()});
    let failure = guard.check_native_result(&result).unwrap_err();
    assert_eq!(failure.code, "credential_material_in_response");
    assert!(
        !serde_json::to_string(&failure)
            .unwrap()
            .contains("SYNTHETIC_NATIVE_GUARD_ONLY")
    );
    assert_eq!(pool.status(), before);
    std::fs::remove_file(key).unwrap();
    std::fs::remove_dir(path).unwrap();
}
