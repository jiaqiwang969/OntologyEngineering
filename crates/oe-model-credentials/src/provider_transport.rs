//! Owner-injected chat/embedding HTTP, using the same credential pool and no retries.
use crate::{CredentialPool, retry_after};
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER},
    redirect::Policy,
};
use semantica_contracts::*;
use sha2::{Digest, Sha256};
use std::{fmt, io::Read, time::Duration};

const MAX_BYTES: usize = 2 * 1024 * 1024;
fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn fail(code: &str, phase: &str) -> SemanticProviderFailure {
    SemanticProviderFailure {
        code: code.into(),
        phase: phase.into(),
        retryable: false,
        retry_after: None,
        http_status: None,
    }
}

pub struct ProviderHttpTransport<'a> {
    pool: &'a CredentialPool,
    client: Client,
    endpoint: String,
    provider: String,
    custom_endpoint: bool,
    embedding: bool,
    kind: &'static str,
    identity: &'static str,
}
impl fmt::Debug for ProviderHttpTransport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderHttpTransport")
            .field("provider", &self.provider)
            .field("kind", &self.kind)
            .field("credential", &"[REDACTED]")
            .finish()
    }
}
impl<'a> ProviderHttpTransport<'a> {
    fn build(
        pool: &'a CredentialPool,
        provider: &str,
        endpoint: &str,
        custom: bool,
        synthetic: bool,
        embedding: bool,
        timeout: Duration,
    ) -> Result<Self, SemanticProviderFailure> {
        if !matches!(provider, "openai" | "groq")
            || (embedding && provider != "openai")
            || (provider == "groq" && custom)
            || timeout.is_zero()
            || timeout > Duration::from_secs(120)
            || pool.contains_in(endpoint.as_bytes())
        {
            return Err(fail("provider_transport_scope_invalid", "configure"));
        }
        let url = reqwest::Url::parse(endpoint)
            .map_err(|_| fail("provider_endpoint_invalid", "configure"))?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.query().is_some()
        {
            return Err(fail("provider_endpoint_invalid", "configure"));
        }
        // The fixture entry never accepts a DNS hostname, so no rebinding.
        if synthetic
            && (url.scheme() != "http"
                || !url
                    .host_str()
                    .is_some_and(|s| s.parse::<std::net::IpAddr>().is_ok_and(|a| a.is_loopback())))
        {
            return Err(fail(
                "synthetic_endpoint_requires_literal_loopback",
                "configure",
            ));
        }
        let client = Client::builder()
            .timeout(timeout)
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .build()
            .map_err(|_| fail("provider_transport_unavailable", "configure"))?;
        Ok(Self {
            pool,
            client,
            endpoint: url.to_string(),
            provider: provider.into(),
            custom_endpoint: custom,
            embedding,
            kind: if synthetic {
                "synthetic_loopback"
            } else {
                "live_http"
            },
            identity: if synthetic {
                "semantica.synthetic-loopback-http/1"
            } else {
                "semantica.provider-http/1"
            },
        })
    }
    pub fn standard(
        pool: &'a CredentialPool,
        provider: &str,
        timeout: Duration,
    ) -> Result<Self, SemanticProviderFailure> {
        let endpoint = match provider {
            "openai" => "https://api.openai.com/v1/chat/completions",
            "groq" => "https://api.groq.com/openai/v1/chat/completions",
            _ => return Err(fail("provider_uncovered", "configure")),
        };
        Self::build(pool, provider, endpoint, false, false, false, timeout)
    }
    pub fn custom_openai(
        pool: &'a CredentialPool,
        endpoint: &str,
        timeout: Duration,
    ) -> Result<Self, SemanticProviderFailure> {
        Self::build(pool, "openai", endpoint, true, false, false, timeout)
    }
    pub fn local_synthetic(
        pool: &'a CredentialPool,
        provider: &str,
        endpoint: &str,
        custom_endpoint: bool,
        timeout: Duration,
    ) -> Result<Self, SemanticProviderFailure> {
        Self::build(
            pool,
            provider,
            endpoint,
            custom_endpoint,
            true,
            false,
            timeout,
        )
    }
    pub fn standard_embedding(
        pool: &'a CredentialPool,
        timeout: Duration,
    ) -> Result<Self, SemanticProviderFailure> {
        Self::build(
            pool,
            "openai",
            "https://api.openai.com/v1/embeddings",
            false,
            false,
            true,
            timeout,
        )
    }
    pub fn custom_openai_embedding(
        pool: &'a CredentialPool,
        endpoint: &str,
        timeout: Duration,
    ) -> Result<Self, SemanticProviderFailure> {
        Self::build(pool, "openai", endpoint, true, false, true, timeout)
    }
    pub fn local_synthetic_embedding(
        pool: &'a CredentialPool,
        endpoint: &str,
        custom_endpoint: bool,
        timeout: Duration,
    ) -> Result<Self, SemanticProviderFailure> {
        Self::build(
            pool,
            "openai",
            endpoint,
            custom_endpoint,
            true,
            true,
            timeout,
        )
    }
}
impl SemanticProviderTransportPort for ProviderHttpTransport<'_> {
    fn post(
        &self,
        input: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        let raw = serde_json::to_vec(&input.body)
            .map_err(|_| fail("provider_request_invalid", "guard"))?;
        if input.validate().is_err()
            || raw.len() > MAX_BYTES
            || hash(&raw) != input.body_sha256
            || input.provider != self.provider
            || input.custom_endpoint != self.custom_endpoint
            || (input.operation == "embed") != self.embedding
        {
            return Err(fail("provider_transport_scope_invalid", "guard"));
        }
        if self.pool.contains_in(&raw) {
            return Err(fail("credential_material_in_request", "guard"));
        }
        let lease = self
            .pool
            .lease()
            .map_err(|_| fail("credential_pool_unavailable", "guard"))?;
        let mut header = HeaderValue::from_str(&lease.bearer())
            .map_err(|_| fail("credential_header_invalid", "guard"))?;
        header.set_sensitive(true);
        let mut response = self
            .client
            .post(&self.endpoint)
            .header(AUTHORIZATION, header)
            .header(CONTENT_TYPE, "application/json")
            .body(raw)
            .send()
            .map_err(|_| {
                let mut e = fail("connection_or_timeout", "transport");
                e.retryable = true;
                e
            })?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let mut e = fail("provider_http_failure", "transport");
            e.http_status = Some(u64::from(status));
            e.retryable = status == 408 || status == 429 || status >= 500;
            e.retry_after = retry_after(
                response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
            );
            if matches!(status, 401 | 402) {
                let _ = lease.retire();
            }
            // Never return or persist arbitrary error-body text.
            return Err(e);
        }
        let mut raw = Vec::new();
        response
            .by_ref()
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut raw)
            .map_err(|_| fail("provider_response_unavailable", "transport"))?;
        if raw.len() > MAX_BYTES {
            return Err(fail("response_too_large", "guard"));
        }
        if self.pool.contains_in(&raw) {
            return Err(fail("credential_material_in_response", "guard"));
        }
        let data: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|_| fail("provider_response_invalid", "parse"))?;
        let decoded =
            serde_json::to_vec(&data).map_err(|_| fail("provider_response_invalid", "parse"))?;
        if self.pool.contains_in(&decoded) {
            return Err(fail("credential_material_in_response", "guard"));
        }
        let body = String::from_utf8(raw.clone())
            .map_err(|_| fail("provider_response_invalid_utf8", "parse"))?;
        Ok(SemanticProviderWireResponse {
            http_status: u64::from(status),
            body,
            body_sha256: hash(&raw),
            transport_kind: self.kind.into(),
            transport_identity: self.identity.into(),
        })
    }
}
impl SemanticProviderResultGuardPort for ProviderHttpTransport<'_> {
    fn check_result(
        &self,
        result: &SemanticProviderResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        let raw =
            serde_json::to_vec(result).map_err(|_| fail("provider_result_invalid", "guard"))?;
        if self.pool.contains_in(&raw) {
            return Err(fail("credential_material_in_response", "guard"));
        }
        Ok(SemanticProviderResultGuard {
            schema: "semantica.provider-result-guard/1".into(),
            credential_material_absent: true,
        })
    }
}

/// Native execution borrows the same owner pool, without an HTTP client or lease.
pub struct NativeCredentialGuard<'a> {
    pool: &'a CredentialPool,
}
impl<'a> NativeCredentialGuard<'a> {
    pub fn new(pool: &'a CredentialPool) -> Self {
        Self { pool }
    }
}
impl semantica_contracts::SemanticNativeResultGuardPort for NativeCredentialGuard<'_> {
    fn check_native_result(
        &self,
        result: &semantica_contracts::SemanticNativeResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        let raw = serde_json::to_vec(result).map_err(|_| fail("native_result_invalid", "guard"))?;
        if self.pool.contains_in(&raw) {
            return Err(fail("credential_material_in_response", "guard"));
        }
        Ok(SemanticProviderResultGuard {
            schema: "semantica.provider-result-guard/1".into(),
            credential_material_absent: true,
        })
    }
}
