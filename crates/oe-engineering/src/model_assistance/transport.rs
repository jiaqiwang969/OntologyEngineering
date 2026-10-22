//! One-dispatch Responses transport. Engineering authority remains with the
//! project workflow, kernel and professional tools, not an HTTP 2xx status.
use std::{
    fmt,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use oe_contracts::{ModelServiceProfile, ModelTransportWaitAttempt, ModelTransportWaitObservation};
use oe_core::SkillRoot;
use oe_model_credentials::CredentialPool;
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue},
    redirect::Policy,
};
use serde::Serialize;
use serde_json::Value;
use url::{Host, Url};

/// Safe diagnostics deliberately exclude endpoint, input, provider prose and
/// credentials. A caller must preserve failures rather than retrying silently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelTransportError {
    pub code: String,
    pub http_status: Option<u16>,
    /// Client dispatch was attempted. DNS/connect failure does not prove that
    /// an HTTP request arrived at either the gateway or the upstream provider.
    pub dispatch_attempted: bool,
}

impl ModelTransportError {
    fn new(code: &str) -> Self {
        Self {
            code: code.into(),
            http_status: None,
            dispatch_attempted: false,
        }
    }
    fn http(status: u16) -> Self {
        Self::observed("http_error", status)
    }
    fn observed(code: &str, status: u16) -> Self {
        Self {
            code: code.into(),
            http_status: Some(status),
            dispatch_attempted: true,
        }
    }
    fn dispatched(mut self) -> Self {
        self.dispatch_attempted = true;
        self
    }
}
impl fmt::Display for ModelTransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code)?;
        if let Some(status) = self.http_status {
            write!(f, ":{status}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ModelTransportError {}

pub trait ModelTransport {
    fn identity(&self) -> String;
    fn send(&self, request: &Value) -> Result<Value, ModelTransportError>;
    /// None is an unverified injected transport, never a successful check.
    /// Builtin HTTP checks its actual factory settings before any dispatch.
    fn check_service_configuration(
        &self,
        _root: &SkillRoot,
        _profile: &ModelServiceProfile,
    ) -> Option<Result<(), ModelTransportError>> {
        None
    }
}

fn absolute_credential_path(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        std::env::current_dir().ok().map(|cwd| cwd.join(path))
    }
}
/// Nonsecret native pathname identity, with the same lexical normalization on
/// both sides. Reader validation still receives the original components; this
/// normalization never removes traversal/symlink refusal from credential I/O.
fn credential_path_identity(path: &Path) -> Option<String> {
    let absolute = absolute_credential_path(path)?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    Some(oe_core::sha256(normalized.as_os_str().as_encoded_bytes()))
}

fn parse_endpoint(endpoint: &str) -> Result<Url, ModelTransportError> {
    let endpoint =
        Url::parse(endpoint).map_err(|_| ModelTransportError::new("invalid_endpoint"))?;
    let loopback = match endpoint.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !matches!(endpoint.scheme(), "http" | "https")
        || (endpoint.scheme() == "http" && !loopback)
        || endpoint.host().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(ModelTransportError::new("invalid_endpoint"));
    }
    Ok(endpoint)
}

/// Bind a profile and its transport without retaining an endpoint in receipts.
pub fn endpoint_identity(endpoint: &str) -> Result<String, ModelTransportError> {
    let endpoint = parse_endpoint(endpoint)?;
    Ok(format!(
        "responses-http/v1:{}",
        oe_core::sha256(endpoint.as_str().as_bytes())
    ))
}

struct Credential(CredentialPool);
impl Credential {
    fn read(path: &Path) -> Result<Self, ModelTransportError> {
        let (_, generation) = read_bearer(path)?;
        let pool = CredentialPool::read_single_http_bearer(path)
            .map_err(|error| ModelTransportError::new(&error.to_string()))?;
        if pool.generation() != generation {
            return Err(ModelTransportError::new("credential_file_changed"));
        }
        Ok(Self(pool))
    }
    fn bearer(&self) -> Result<String, ModelTransportError> {
        self.0
            .lease()
            .map(|lease| lease.bearer())
            .map_err(|_| ModelTransportError::new("credential_pool_unavailable"))
    }
    fn contains_in(&self, bytes: &[u8]) -> bool {
        self.0.contains_in(bytes)
    }
}

#[derive(Clone, Serialize)]
struct AttemptObservation {
    request_status: String,
    request_sha256: Option<String>,
    request_bytes: Option<u64>,
    client_dispatch_attempted: bool,
    http_status: Option<u16>,
    response_read_status: String,
    response_bytes_read: Option<u64>,
    response_json_status: String,
    safe_error: Option<String>,
    // Kept outside the historical /1 serialization and in the same attempt owner.
    #[serde(skip)]
    wait: WaitRecord,
}
#[derive(Clone)]
struct WaitRecord {
    started: Instant,
    data: ModelTransportWaitAttempt,
}
impl WaitRecord {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            data: ModelTransportWaitAttempt {
                attempt_index: 0,
                serialization_finished_us: None,
                dispatch_api_started_us: None,
                dispatch_api_finished_us: None,
                headers_observed_us: None,
                body_read_started_us: None,
                body_read_finished_us: None,
                terminal_us: None,
                http_status: None,
                terminal_stage: "pre_dispatch".into(),
                safe_error: None,
            },
        }
    }
    fn elapsed_us(&self) -> Option<u64> {
        u64::try_from(self.started.elapsed().as_micros()).ok()
    }
}
impl Default for AttemptObservation {
    fn default() -> Self {
        Self {
            request_status: "received".into(),
            request_sha256: None,
            request_bytes: None,
            client_dispatch_attempted: false,
            http_status: None,
            response_read_status: "not_started".into(),
            response_bytes_read: None,
            response_json_status: "not_parsed".into(),
            safe_error: None,
            wait: WaitRecord::new(),
        }
    }
}
pub struct HttpModelTransport {
    credential: Credential,
    client: Client,
    endpoint: Url,
    timeout: Duration,
    max_response_bytes: u64,
    credential_path_identity: Option<String>,
    identity: String,
    observations: Mutex<Vec<AttemptObservation>>,
}
impl fmt::Debug for HttpModelTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpModelTransport")
            .field("endpoint", &"[REDACTED]")
            .field("credential", &"[REDACTED]")
            .field("timeout", &self.timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}
impl HttpModelTransport {
    /// HTTPS, or an explicitly supplied literal loopback HTTP gateway. There
    /// are no default credentials, redirect following, protocol retries or
    /// request truncation. The workflow independently gates request semantics.
    pub fn new(
        endpoint: &str,
        credential_file: &Path,
        timeout: Duration,
        max_response_bytes: u64,
    ) -> Result<Self, ModelTransportError> {
        let endpoint = parse_endpoint(endpoint)?;
        let loopback = match endpoint.host() {
            Some(Host::Ipv4(ip)) => ip.is_loopback(),
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        if timeout.is_zero() || max_response_bytes == 0 || max_response_bytes == u64::MAX {
            return Err(ModelTransportError::new("invalid_transport_limits"));
        }
        // Capture the path actually passed to the existing reader. No new
        // constructor rejection or credential owner is introduced.
        let absolute_credential = absolute_credential_path(credential_file);
        let actual_credential = absolute_credential.as_deref().unwrap_or(credential_file);
        let credential = Credential::read(actual_credential)?;
        let credential_path_identity = credential_path_identity(actual_credential);
        let builder = Client::builder()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .timeout(timeout)
            .https_only(endpoint.scheme() == "https");
        let builder = if loopback {
            builder.no_proxy()
        } else {
            builder
        };
        let client = builder
            .build()
            .map_err(|_| ModelTransportError::new("client_unavailable"))?;
        let identity = endpoint_identity(endpoint.as_str())?;
        Ok(Self {
            credential,
            client,
            endpoint,
            timeout,
            max_response_bytes,
            credential_path_identity,
            identity,
            observations: Mutex::new(Vec::new()),
        })
    }

    /// Local adapter observations, never an upstream account/attempt claim.
    /// Check only actual factory endpoint/path/timeout/response-cap settings
    /// against the current generated profile. This is not full profile,
    /// credential-content, provider-capacity or engineering attestation.
    pub fn verify_current_profile(
        &self,
        root: &SkillRoot,
        profile: &ModelServiceProfile,
    ) -> Result<(), ModelTransportError> {
        let expected_identity = endpoint_identity(&profile.endpoint)?;
        let expected_credential = root
            .expand(Path::new(&profile.credential_file))
            .map_err(|_| ModelTransportError::new("service_configuration_unavailable"))?;
        let expected_path = credential_path_identity(&expected_credential)
            .ok_or_else(|| ModelTransportError::new("service_configuration_unavailable"))?;
        let actual_path = self
            .credential_path_identity
            .as_ref()
            .ok_or_else(|| ModelTransportError::new("service_configuration_unavailable"))?;
        if self.identity != expected_identity
            || actual_path != &expected_path
            || self.timeout != Duration::from_secs(profile.timeout_seconds)
            || self.max_response_bytes != profile.maximum_response_bytes
        {
            return Err(ModelTransportError::new("service_configuration_mismatch"));
        }
        Ok(())
    }

    /// An HTTP status proves response headers were observed. A client dispatch
    /// alone leaves arrival at the gateway/provider unknown.
    pub fn observation(&self) -> Value {
        let Ok(attempts) = self.observations.lock() else {
            return serde_json::json!({"schema":"oev2.responses-http-observation/1", "status":"unavailable",
                "send_call_attempts":null,"client_dispatch_attempts":null,"observed_http_attempts":null,
                "upstream_attempts":"unknown","attempts":null});
        };
        serde_json::json!({"schema":"oev2.responses-http-observation/1","status":"recorded",
            "send_call_attempts":attempts.len(),
            "client_dispatch_attempts":attempts.iter().filter(|a| a.client_dispatch_attempted).count(),
            "observed_http_attempts":attempts.iter().filter(|a| a.http_status.is_some()).count(),
            "upstream_attempts":"unknown","attempts":attempts.clone()})
    }
    /// A separate generated view of monotonic client API milestones. These
    /// observations include local scheduling and do not time DNS, TLS, server
    /// processing, request arrival or an upstream account. Legacy /1 remains
    /// byte-for-byte the same projection. Blocking operation waits retain their
    /// existing limit; the same configured Duration also bounds the request's
    /// cumulative header/body lifetime. These are observations, not a hard
    /// real-time cancellation or a remote processing-time attestation.
    pub fn wait_observation(&self) -> ModelTransportWaitObservation {
        let attempts = self
            .observations
            .lock()
            .ok()
            .map(|attempts| attempts.iter().map(|a| a.wait.data.clone()).collect());
        ModelTransportWaitObservation {
            schema: "oev2.responses-http-wait-observation/1".into(),
            status: if attempts.is_some() {
                "recorded"
            } else {
                "unavailable"
            }
            .into(),
            clock: "monotonic_client_elapsed_microseconds".into(),
            timing_scope: "client_api_boundaries".into(),
            dns_tls_timing: "unknown".into(),
            server_processing_timing: "unknown".into(),
            upstream_arrival: "unknown".into(),
            usage_cost: "unknown".into(),
            attempts,
        }
    }
    fn begin_observation(&self) -> Result<usize, ModelTransportError> {
        let mut observations = self
            .observations
            .lock()
            .map_err(|_| ModelTransportError::new("observation_unavailable"))?;
        let index = observations.len();
        let mut attempt = AttemptObservation::default();
        attempt.wait.data.attempt_index = index as u64;
        observations.push(attempt);
        Ok(index)
    }
    fn observe(&self, index: usize, update: impl FnOnce(&mut AttemptObservation)) {
        if let Ok(mut observations) = self.observations.lock() {
            if let Some(attempt) = observations.get_mut(index) {
                update(attempt);
            }
        }
    }
}
impl ModelTransport for HttpModelTransport {
    fn check_service_configuration(
        &self,
        root: &SkillRoot,
        profile: &ModelServiceProfile,
    ) -> Option<Result<(), ModelTransportError>> {
        Some(self.verify_current_profile(root, profile))
    }
    fn identity(&self) -> String {
        self.identity.clone()
    }

    fn send(&self, request: &Value) -> Result<Value, ModelTransportError> {
        let index = self.begin_observation()?;
        let result: Result<Value, ModelTransportError> = (|| {
            // The workflow freezes this exact ordered wire representation. Its
            // numeric formatting can differ from serde's finite scalar formatting;
            // the archive SHA must identify the bytes actually dispatched.
            let raw = oe_core::json::ordered_json(request)
                .map_err(|_| ModelTransportError::new("request_serialization_failed"))?;
            if self.credential.contains_in(&raw) {
                self.observe(index, |a| a.request_status = "credential_rejected".into());
                return Err(ModelTransportError::new("credential_material_in_request"));
            }
            self.observe(index, |a| {
                a.request_status = "serialized".into();
                a.request_sha256 = Some(oe_core::sha256(&raw));
                a.request_bytes = Some(raw.len() as u64);
                a.wait.data.serialization_finished_us = a.wait.elapsed_us();
            });
            let bearer = self.credential.bearer()?;
            let mut authorization = HeaderValue::from_str(&bearer)
                .map_err(|_| ModelTransportError::new("invalid_credential"))?;
            authorization.set_sensitive(true);
            self.observe(index, |a| {
                a.client_dispatch_attempted = true;
                a.wait.data.dispatch_api_started_us = a.wait.elapsed_us();
                a.wait.data.terminal_stage = "dispatch".into();
            });
            let response = self
                .client
                .post(self.endpoint.clone())
                .header(AUTHORIZATION, authorization)
                .header(CONTENT_TYPE, "application/json")
                .body(raw)
                // Blocking builder timeout renews per Read. The per-request
                // timeout installs the locked async total timer through body
                // reads, using this same profile Duration without resetting it.
                .timeout(self.timeout)
                .send();
            self.observe(index, |a| {
                a.wait.data.dispatch_api_finished_us = a.wait.elapsed_us();
            });
            let response = response.map_err(|error| {
                ModelTransportError::new(if error.is_timeout() {
                    "timeout"
                } else {
                    "connection_failed"
                })
                .dispatched()
            })?;
            let status = response.status();
            self.observe(index, |a| {
                a.http_status = Some(status.as_u16());
                a.wait.data.http_status = Some(u64::from(status.as_u16()));
                a.wait.data.headers_observed_us = a.wait.elapsed_us();
                a.wait.data.terminal_stage = "response_headers".into();
            });
            if !status.is_success() {
                self.observe(index, |a| {
                    a.response_read_status = "not_read_error_status".into()
                });
                // Error bodies can echo complete private inputs or authorization.
                // Preserve only the actual HTTP status; never read or return prose.
                return Err(ModelTransportError::http(status.as_u16()));
            }
            let mut raw = Vec::new();
            self.observe(index, |a| {
                a.wait.data.body_read_started_us = a.wait.elapsed_us();
                a.wait.data.terminal_stage = "body_read".into();
            });
            let read = response
                .take(self.max_response_bytes + 1)
                .read_to_end(&mut raw);
            self.observe(index, |a| {
                a.wait.data.body_read_finished_us = a.wait.elapsed_us();
                a.response_bytes_read = Some(raw.len() as u64);
                a.response_read_status = if read.is_ok() { "read" } else { "failed" }.into();
            });
            read.map_err(|_| {
                ModelTransportError::observed("response_read_failed", status.as_u16())
            })?;
            self.observe(index, |a| {
                a.wait.data.terminal_stage = "response_processing".into()
            });
            if raw.len() as u64 > self.max_response_bytes {
                self.observe(index, |a| {
                    a.response_read_status = "bounded_limit_exceeded".into()
                });
                return Err(ModelTransportError::observed(
                    "response_too_large",
                    status.as_u16(),
                ));
            }
            if self.credential.contains_in(&raw) {
                self.observe(index, |a| {
                    a.response_json_status = "credential_rejected".into()
                });
                return Err(ModelTransportError::observed(
                    "credential_material_in_response",
                    status.as_u16(),
                ));
            }
            let result = oe_core::json::strict_json(&raw).map_err(|_| {
                self.observe(index, |a| a.response_json_status = "malformed".into());
                ModelTransportError::observed("malformed_response", status.as_u16())
            })?;
            // Inspect decoded strings too, including object keys and escaped keys.
            let decoded = serde_json::to_vec(&result).map_err(|_| {
                ModelTransportError::observed("malformed_response", status.as_u16())
            })?;
            if self.credential.contains_in(&decoded) {
                self.observe(index, |a| {
                    a.response_json_status = "credential_rejected".into()
                });
                return Err(ModelTransportError::observed(
                    "credential_material_in_response",
                    status.as_u16(),
                ));
            }
            if !result.is_object() {
                self.observe(index, |a| a.response_json_status = "malformed".into());
                return Err(ModelTransportError::observed(
                    "malformed_response",
                    status.as_u16(),
                ));
            }
            self.observe(index, |a| {
                a.response_json_status = "object".into();
                a.wait.data.terminal_stage = "completed".into();
            });
            // Incomplete/failed/unknown provider states remain exactly as received.
            // HTTP success does not manufacture a completed semantic answer.
            Ok(result)
        })();
        self.observe(index, |a| {
            a.wait.data.terminal_us = a.wait.elapsed_us();
            if let Err(error) = &result {
                a.safe_error = Some(error.code.clone());
                a.wait.data.safe_error = Some(error.code.clone());
            }
        });
        result
    }
}

#[cfg(unix)]
fn read_bearer(path: &Path) -> Result<(String, String), ModelTransportError> {
    use std::os::unix::fs::MetadataExt;
    // Reuse the workspace's anchored directory descriptors, including refusal
    // of symlinks in ancestor components, rather than reopening loose paths.
    let file = oe_core::open_regular_file(path, 65_536)
        .map_err(|_| ModelTransportError::new("credential_file_unavailable"))?;
    let info = file
        .metadata()
        .map_err(|_| ModelTransportError::new("credential_file_unavailable"))?;
    // SAFETY: getuid has no inputs or pointer arguments.
    let owner = unsafe { libc::getuid() };
    if !info.is_file() || info.uid() != owner || info.mode() & 0o7777 != 0o600 {
        return Err(ModelTransportError::new(
            "credential_file_requires_owner_and_mode_0600",
        ));
    }
    let mut raw = Vec::new();
    file.take(65_537)
        .read_to_end(&mut raw)
        .map_err(|_| ModelTransportError::new("credential_file_unavailable"))?;
    if raw.len() > 65_536 {
        return Err(ModelTransportError::new("credential_file_too_large"));
    }
    let text = std::str::from_utf8(&raw)
        .map_err(|_| ModelTransportError::new("credential_file_invalid_utf8"))?;
    let key = text.trim();
    let unpadded = key.trim_end_matches('=');
    if unpadded.is_empty()
        || !unpadded
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~+/".contains(&b))
    {
        return Err(ModelTransportError::new(
            "credential_file_requires_one_bearer",
        ));
    }
    Ok((key.into(), oe_core::sha256(&raw)))
}
#[cfg(not(unix))]
fn read_bearer(_path: &Path) -> Result<(String, String), ModelTransportError> {
    Err(ModelTransportError::new(
        "unix_credential_permissions_required",
    ))
}
