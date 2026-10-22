#![cfg(unix)]
use oe_model_credentials::{CredentialPool, provider_transport::ProviderHttpTransport};
use semantica_contracts::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, net::TcpListener, os::unix::fs::PermissionsExt, time::Duration};
#[test]
fn chat_and_embedding_scope_mismatch_refuses_before_lease_or_local_http() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../var");
    fs::create_dir_all(&base).unwrap();
    let case = base.join(format!(
        "synthetic-embedding-operation-scope-{}",
        std::process::id()
    ));
    fs::create_dir(&case).unwrap();
    let key = case.join("synthetic-key");
    fs::write(&key, &["sk-", "SYNTHETIC_OPERATION_SCOPE_ONLY"].concat()).unwrap();
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    let pool = CredentialPool::read_single_http_bearer(&key).unwrap();
    let before = pool.status().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/synthetic", listener.local_addr().unwrap());
    let chat = ProviderHttpTransport::local_synthetic(
        &pool,
        "openai",
        &url,
        false,
        Duration::from_millis(500),
    )
    .unwrap();
    let embed = ProviderHttpTransport::local_synthetic_embedding(
        &pool,
        &url,
        false,
        Duration::from_millis(500),
    )
    .unwrap();
    let body = json!({"model":"synthetic","input":"literal"});
    let mut wire = SemanticProviderWireRequest {
        provider: "openai".into(),
        operation: "embed".into(),
        custom_endpoint: false,
        body_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(&body).unwrap())),
        body,
    };
    assert_eq!(
        chat.post(&wire).unwrap_err().code,
        "provider_transport_scope_invalid"
    );
    wire.operation = "generate".into();
    assert_eq!(
        embed.post(&wire).unwrap_err().code,
        "provider_transport_scope_invalid"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(pool.status().unwrap(), before);
    assert!(
        !format!("{chat:?}{embed:?}").contains(&["sk-", "SYNTHETIC_OPERATION_SCOPE_ONLY"].concat())
    );
    fs::remove_dir_all(case).unwrap();
}
