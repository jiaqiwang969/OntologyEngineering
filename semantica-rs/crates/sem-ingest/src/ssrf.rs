//! SSRF safeguards for ingest HTTP clients (`semantica/ingest/ssrf.py`).
//!
//! Validates outbound request URLs before they reach the HTTP transport so user-supplied
//! targets cannot reach private, loopback, or link-local addresses (including cloud metadata
//! endpoints). [`request_with_ssrf_guard`] re-validates every redirect hop, pins each
//! connection to the addresses that were validated (closing the DNS-rebinding window) and
//! strips credentials on cross-origin redirects.

use std::net::IpAddr;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use once_cell::sync::Lazy;
use sem_utils::{SemanticaError, ValidationError};
use serde_json::Value;

use crate::support::http::{HttpAdapter, PinnedTarget, RequestOptions, Response, Session};
use crate::support::{ipaddr, pyurl};

/// `ALLOWED_URL_SCHEMES`.
pub const ALLOWED_URL_SCHEMES: [&str; 2] = ["http", "https"];

const TRUE_STRINGS: [&str; 4] = ["1", "true", "yes", "on"];
const FALSE_STRINGS: [&str; 4] = ["0", "false", "no", "off"];

/// `_DNS_RESOLVE_TIMEOUT_SECONDS`: DNS lookups are bounded so fake/unreachable hosts cannot
/// hang request validation.
pub const DNS_RESOLVE_TIMEOUT_SECONDS: f64 = 2.0;
const DNS_EXECUTOR_WORKERS: usize = 4;

/// `_DEFAULT_MAX_REDIRECTS`.
pub const DEFAULT_MAX_REDIRECTS: u32 = 10;
const REDIRECT_STATUS_CODES: [u16; 5] = [301, 302, 303, 307, 308];
const STRIP_BODY_ON_REDIRECT: [u16; 3] = [301, 302, 303];

/// `BLOCKED_NETWORKS`: explicit blocked networks from issue #867 plus common non-routable
/// ranges.
pub const BLOCKED_NETWORKS: [&str; 10] = [
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

fn validation(message: impl Into<String>) -> SemanticaError {
    ValidationError::new(message).into()
}

/// `socket.getaddrinfo` as seen by the SSRF guard (and the repository ingestor, which shares
/// the module in Python): the system resolver, or a per-thread replacement installed with
/// [`socket::set_thread_getaddrinfo`] (the Rust form of `patch("...ssrf.socket.getaddrinfo")`).
pub mod socket {
    use std::cell::RefCell;
    use std::net::ToSocketAddrs;
    use std::sync::Arc;

    /// Failures `getaddrinfo` reports.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum GaiError {
        /// `socket.gaierror` (its `str()`).
        Gaierror(String),
        /// `TimeoutError`.
        Timeout,
        /// Any other `OSError` (its `str()`).
        Os(String),
    }

    impl std::fmt::Display for GaiError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                GaiError::Gaierror(m) | GaiError::Os(m) => f.write_str(m),
                GaiError::Timeout => f.write_str("timed out"),
            }
        }
    }

    /// A resolver: host name -> the `sockaddr[0]` of every `getaddrinfo` entry.
    pub type Resolver = Arc<dyn Fn(&str) -> Result<Vec<String>, GaiError> + Send + Sync>;

    thread_local! {
        static RESOLVER: RefCell<Option<Resolver>> = const { RefCell::new(None) };
    }

    /// Restores the previous resolver on drop.
    pub struct ResolverGuard {
        previous: Option<Resolver>,
    }

    impl Drop for ResolverGuard {
        fn drop(&mut self) {
            let previous = self.previous.take();
            RESOLVER.with(|r| *r.borrow_mut() = previous);
        }
    }

    /// Replaces `getaddrinfo` on the current thread until the guard is dropped.
    pub fn set_thread_getaddrinfo(
        resolver: impl Fn(&str) -> Result<Vec<String>, GaiError> + Send + Sync + 'static,
    ) -> ResolverGuard {
        let previous = RESOLVER.with(|r| r.borrow_mut().replace(Arc::new(resolver)));
        ResolverGuard { previous }
    }

    /// The resolver in effect on this thread.
    pub fn current_getaddrinfo() -> Resolver {
        RESOLVER
            .with(|r| r.borrow().clone())
            .unwrap_or_else(|| Arc::new(system_getaddrinfo))
    }

    /// `socket.getaddrinfo(host, None)`: the addresses `host` resolves to.
    pub fn getaddrinfo(host: &str) -> Result<Vec<String>, GaiError> {
        current_getaddrinfo()(host)
    }

    /// The operating system resolver.
    pub fn system_getaddrinfo(host: &str) -> Result<Vec<String>, GaiError> {
        match (host, 0u16).to_socket_addrs() {
            Ok(addrs) => Ok(addrs.map(|a| a.ip().to_string()).collect()),
            Err(e) => Err(GaiError::Gaierror(e.to_string())),
        }
    }
}

thread_local! {
    static DNS_TIMEOUT_OVERRIDE: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
}

/// Restores the previous DNS timeout on drop.
pub struct DnsTimeoutGuard {
    previous: Option<f64>,
}

impl Drop for DnsTimeoutGuard {
    fn drop(&mut self) {
        DNS_TIMEOUT_OVERRIDE.with(|c| c.set(self.previous));
    }
}

/// Overrides `_DNS_RESOLVE_TIMEOUT_SECONDS` on the current thread until the guard is dropped.
pub fn set_thread_dns_resolve_timeout(seconds: f64) -> DnsTimeoutGuard {
    let previous = DNS_TIMEOUT_OVERRIDE.with(|c| c.replace(Some(seconds)));
    DnsTimeoutGuard { previous }
}

fn dns_resolve_timeout() -> f64 {
    DNS_TIMEOUT_OVERRIDE
        .with(|c| c.get())
        .unwrap_or(DNS_RESOLVE_TIMEOUT_SECONDS)
}

type DnsJob = Box<dyn FnOnce() + Send>;

/// `_get_dns_executor()`: a process-wide pool of DNS worker threads. A lookup that times out
/// keeps its worker busy, but the caller never waits for it.
struct DnsExecutor {
    sender: Mutex<mpsc::Sender<DnsJob>>,
}

static DNS_EXECUTOR: Lazy<DnsExecutor> = Lazy::new(|| {
    let (sender, receiver) = mpsc::channel::<DnsJob>();
    let receiver = Arc::new(Mutex::new(receiver));
    for index in 0..DNS_EXECUTOR_WORKERS {
        let receiver = receiver.clone();
        let _ = std::thread::Builder::new()
            .name(format!("semantica-ssrf-dns_{index}"))
            .spawn(move || {
                loop {
                    let job = {
                        let guard = match receiver.lock() {
                            Ok(g) => g,
                            Err(e) => e.into_inner(),
                        };
                        guard.recv()
                    };
                    match job {
                        Ok(job) => job(),
                        Err(_) => break,
                    }
                }
            });
    }
    DnsExecutor {
        sender: Mutex::new(sender),
    }
});

fn resolve_bounded(host: &str) -> Result<Vec<String>, SemanticaError> {
    let resolver = socket::current_getaddrinfo();
    let (tx, rx) = mpsc::channel();
    let host_owned = host.to_string();
    let job: DnsJob = Box::new(move || {
        let _ = tx.send(resolver(&host_owned));
    });
    let submitted = DNS_EXECUTOR
        .sender
        .lock()
        .map(|s| s.send(job).is_ok())
        .unwrap_or(false);
    let blocked = || {
        validation(format!(
            "URL host '{host}' could not be resolved safely (DNS error or timeout); request blocked"
        ))
    };
    if !submitted {
        return Err(blocked());
    }
    match rx.recv_timeout(Duration::from_secs_f64(dns_resolve_timeout().max(0.0))) {
        Ok(Ok(addrs)) => Ok(addrs),
        Ok(Err(_)) | Err(_) => Err(blocked()),
    }
}

/// `parse_bool(value, default=False)`: parses a config value into a bool without
/// truthy-string pitfalls (`bool('false')` is `True` in Python). Accepts only explicit bools,
/// `0`/`1` and a small allowlist of strings.
pub fn parse_bool(value: &Value, default: bool) -> Result<bool, SemanticaError> {
    use sem_utils::pycompat::fmt::{py_repr, type_name};
    match value {
        Value::Null => Ok(default),
        Value::Bool(b) => Ok(*b),
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(f64::NAN);
            if f == 1.0 {
                Ok(true)
            } else if f == 0.0 {
                Ok(false)
            } else {
                Err(validation(format!("Invalid boolean value: {}", py_repr(value))))
            }
        }
        Value::String(s) => {
            let normalized = sem_utils::pycompat::fmt::strip(s).to_lowercase();
            if TRUE_STRINGS.contains(&normalized.as_str()) {
                Ok(true)
            } else if FALSE_STRINGS.contains(&normalized.as_str()) {
                Ok(false)
            } else {
                Err(validation(format!("Invalid boolean value: {}", py_repr(value))))
            }
        }
        other => Err(validation(format!(
            "Invalid boolean type: {} ({})",
            type_name(other),
            py_repr(other)
        ))),
    }
}

/// `_ip_is_blocked(addr)`.
pub fn _ip_is_blocked(addr: IpAddr) -> bool {
    if ipaddr::is_private(addr)
        || ipaddr::is_loopback(addr)
        || ipaddr::is_link_local(addr)
        || ipaddr::is_reserved(addr)
        || ipaddr::is_multicast(addr)
        || ipaddr::is_unspecified(addr)
    {
        return true;
    }
    BLOCKED_NETWORKS
        .iter()
        .any(|net| ipaddr::in_network(addr, net))
}

/// `_hostname_resolves_to_blocked(hostname)`: true when any resolved address is blocked.
/// DNS failures and timeouts fail closed with a `ValidationError`.
pub fn _hostname_resolves_to_blocked(hostname: &str) -> Result<bool, SemanticaError> {
    let resolved = resolve_bounded(hostname)?;
    for addr in resolved {
        let ip = ipaddr::ip_address(&addr)?;
        if _ip_is_blocked(ip) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `validate_url_for_request(url, *, allow_private_ips=False)`: validates that `url` is safe
/// to fetch over HTTP(S).
pub fn validate_url_for_request(url: &str, allow_private_ips: bool) -> Result<(), SemanticaError> {
    if sem_utils::pycompat::fmt::strip(url).is_empty() {
        return Err(validation("URL must be a non-empty string"));
    }
    let parsed = pyurl::urlparse(sem_utils::pycompat::fmt::strip(url))?;
    let scheme = parsed.scheme.to_lowercase();
    if !ALLOWED_URL_SCHEMES.contains(&scheme.as_str()) {
        return Err(validation(format!(
            "URL scheme '{}' is not permitted. Only http and https are allowed.",
            parsed.scheme
        )));
    }
    if parsed.netloc.is_empty() {
        return Err(validation(format!(
            "Invalid URL format: {url}. URL must include scheme (http/https) and netloc (domain)."
        )));
    }
    let Some(host) = parsed.hostname() else {
        return Err(validation(format!(
            "Invalid URL format: {url}. URL must include a hostname."
        )));
    };
    if allow_private_ips {
        return Ok(());
    }
    let lowered = host.to_lowercase();
    let lowered = lowered.trim_end_matches('.');
    if lowered == "localhost" || lowered.ends_with(".localhost") {
        return Err(validation(format!("URL host is not allowed: {host}")));
    }
    if let Ok(ip) = ipaddr::ip_address(&host) {
        if _ip_is_blocked(ip) {
            return Err(validation(format!("URL points to a blocked address: {host}")));
        }
        return Ok(());
    }
    if _hostname_resolves_to_blocked(&host)? {
        return Err(validation(format!(
            "URL host '{host}' resolves to a blocked (private/loopback/link-local) address"
        )));
    }
    Ok(())
}

/// `_resolve_pinned_ips(url, *, allow_private_ips)`: validates `url` and returns every resolved
/// IP for connection pinning (`None` when private IPs are trusted or there is no host).
pub fn _resolve_pinned_ips(
    url: &str,
    allow_private_ips: bool,
) -> Result<Option<Vec<String>>, SemanticaError> {
    validate_url_for_request(url, allow_private_ips)?;
    if allow_private_ips {
        return Ok(None);
    }
    let Some(host) = pyurl::urlparse(url)?.hostname() else {
        return Ok(None);
    };
    if let Ok(ip) = ipaddr::ip_address(&host) {
        return Ok(Some(vec![ipaddr::ip_to_string(ip)]));
    }
    let resolved = resolve_bounded(&host)?;
    let mut pinned: Vec<String> = Vec::new();
    for addr in resolved {
        let ip = ipaddr::ip_address(&addr)?;
        if _ip_is_blocked(ip) {
            return Err(validation(format!(
                "URL host '{host}' resolves to a blocked (private/loopback/link-local) address"
            )));
        }
        let text = ipaddr::ip_to_string(ip);
        if !pinned.contains(&text) {
            pinned.push(text);
        }
    }
    if pinned.is_empty() {
        return Err(validation(format!(
            "URL host '{host}' could not be resolved to a usable address"
        )));
    }
    Ok(Some(pinned))
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }
}

/// `_should_strip_auth(old_url, new_url)`: mirrors `requests.utils.should_strip_auth`.
pub fn _should_strip_auth(old_url: &str, new_url: &str) -> bool {
    let (Ok(old), Ok(new)) = (pyurl::urlparse(old_url), pyurl::urlparse(new_url)) else {
        return true;
    };
    if old.hostname() != new.hostname() {
        return true;
    }
    let old_port = old.port().ok().flatten();
    let new_port = new.port().ok().flatten();
    if old.scheme == "http"
        && matches!(old_port, None | Some(80))
        && new.scheme == "https"
        && matches!(new_port, None | Some(443))
    {
        return false;
    }
    let changed_port = old_port != new_port;
    let changed_scheme = old.scheme != new.scheme;
    let default = default_port(&old.scheme);
    let is_default = |p: Option<u16>| p.is_none() || p == default;
    if !changed_scheme && is_default(old_port) && is_default(new_port) {
        return false;
    }
    changed_port || changed_scheme
}

/// Mounts (or removes) IP pinning on `session` for the next hop (`_apply_connection_pin`).
fn apply_connection_pin(
    session: &mut Session,
    url: &str,
    pinned_ips: Option<&Vec<String>>,
    original: &SessionSnapshot,
) -> Result<(), SemanticaError> {
    let parsed = pyurl::urlparse(url)?;
    match pinned_ips {
        Some(ips) if !ips.is_empty() => {
            let port = parsed.port().ok().flatten();
            let default = default_port(&parsed.scheme).unwrap_or(80);
            let hostname = parsed.hostname().unwrap_or_default();
            let host_header = if port.is_none() || port == Some(default) {
                hostname.clone()
            } else {
                format!("{hostname}:{}", port.unwrap_or(default))
            };
            // `_PinnedIPHTTPAdapter()` is a fresh HTTPAdapter: default retry policy.
            let adapter = HttpAdapter {
                max_retries: crate::support::http::Retry::default(),
                pinned: Some(PinnedTarget {
                    ips: ips.clone(),
                    hostname,
                }),
            };
            session.mount("http://", adapter.clone());
            session.mount("https://", adapter);
            session.headers.insert("Host", host_header);
        }
        _ => {
            session.mount("http://", original.http_adapter.clone());
            session.mount("https://", original.https_adapter.clone());
            match &original.host_header {
                Some(h) => session.headers.insert("Host", h.clone()),
                None => {
                    session.headers.remove("Host");
                }
            }
        }
    }
    Ok(())
}

struct SessionSnapshot {
    http_adapter: HttpAdapter,
    https_adapter: HttpAdapter,
    host_header: Option<String>,
    auth_headers: Vec<(String, String)>,
    auth: Option<crate::support::http::Auth>,
    trust_env: bool,
}

const SENSITIVE: [&str; 2] = ["Authorization", "Proxy-Authorization"];

/// Options of [`request_with_ssrf_guard`] other than the pass-through request keywords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardOptions {
    /// `allow_private_ips`: trust the caller's own URL.
    pub allow_private_ips: bool,
    /// `allow_private_ips_on_redirect`: trust for redirect targets on a different host
    /// (`None` = same as `allow_private_ips`).
    pub allow_private_ips_on_redirect: Option<bool>,
    /// `max_redirects`.
    pub max_redirects: u32,
}

impl Default for GuardOptions {
    fn default() -> Self {
        GuardOptions {
            allow_private_ips: false,
            allow_private_ips_on_redirect: None,
            max_redirects: DEFAULT_MAX_REDIRECTS,
        }
    }
}

impl GuardOptions {
    /// `allow_private_ips=value` with the other defaults.
    pub fn allow_private_ips(value: bool) -> GuardOptions {
        GuardOptions {
            allow_private_ips: value,
            ..GuardOptions::default()
        }
    }
}

/// `request_with_ssrf_guard(method, url, *, session=None, allow_private_ips=False,
/// allow_private_ips_on_redirect=None, max_redirects=10, **kwargs)`: performs an HTTP request
/// with SSRF checks on `url` and every redirect.
///
/// Redirects are followed manually (`allow_redirects=False` on every hop); each `Location`
/// is re-validated and the connection re-pinned. Credentials (request `headers`, session
/// headers, `auth=`, `session.auth`, `session.trust_env`) are stripped once a hop changes
/// origin and stay stripped for the rest of the chain. A caller-supplied session is restored
/// to its original state however the call exits (exclusive `&mut` access replaces the
/// original's per-session lock).
pub fn request_with_ssrf_guard(
    method: &str,
    url: &str,
    session: Option<&mut Session>,
    guard: &GuardOptions,
    options: RequestOptions,
) -> Result<Response, SemanticaError> {
    let mut options = options;
    options.allow_redirects = None;

    let redirect_allow_private_ips = guard
        .allow_private_ips_on_redirect
        .unwrap_or(guard.allow_private_ips);
    let original_host = pyurl::urlparse(url)?
        .hostname()
        .unwrap_or_default()
        .to_lowercase();

    let current_pinned = _resolve_pinned_ips(url, guard.allow_private_ips)?;

    let caller_supplied = session.is_some();
    let mut owned;
    let active: &mut Session = match session {
        Some(s) => s,
        None => {
            owned = Session::new();
            &mut owned
        }
    };

    let snapshot = SessionSnapshot {
        http_adapter: active
            .adapters
            .get("http://")
            .cloned()
            .unwrap_or_default(),
        https_adapter: active
            .adapters
            .get("https://")
            .cloned()
            .unwrap_or_default(),
        host_header: active.headers.get("Host").map(str::to_string),
        auth_headers: if caller_supplied {
            SENSITIVE
                .iter()
                .filter_map(|h| active.headers.get(h).map(|v| (h.to_string(), v.to_string())))
                .collect()
        } else {
            Vec::new()
        },
        auth: active.auth.clone(),
        trust_env: active.trust_env,
    };

    let result = guarded_loop(
        active,
        method,
        url,
        guard,
        options,
        current_pinned,
        &original_host,
        redirect_allow_private_ips,
        caller_supplied,
        &snapshot,
    );

    if caller_supplied {
        for (h, v) in &snapshot.auth_headers {
            active.headers.insert(h.clone(), v.clone());
        }
        active.auth = snapshot.auth.clone();
        active.trust_env = snapshot.trust_env;
        active.mount("http://", snapshot.http_adapter.clone());
        active.mount("https://", snapshot.https_adapter.clone());
        match &snapshot.host_header {
            Some(h) => active.headers.insert("Host", h.clone()),
            None => {
                active.headers.remove("Host");
            }
        }
    } else {
        active.close();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn guarded_loop(
    session: &mut Session,
    method: &str,
    url: &str,
    guard: &GuardOptions,
    mut options: RequestOptions,
    mut current_pinned: Option<Vec<String>>,
    original_host: &str,
    redirect_allow_private_ips: bool,
    caller_supplied: bool,
    snapshot: &SessionSnapshot,
) -> Result<Response, SemanticaError> {
    let mut current_url = url.to_string();
    let mut current_method = method.to_uppercase();
    let mut redirects_followed: u32 = 0;
    let mut auth_stripped = false;

    loop {
        apply_connection_pin(session, &current_url, current_pinned.as_ref(), snapshot)?;
        let mut hop_options = options.clone();
        hop_options.allow_redirects = Some(false);
        let response = session.request(&current_method, &current_url, &hop_options)?;

        if !REDIRECT_STATUS_CODES.contains(&response.status_code) {
            return Ok(response);
        }

        if redirects_followed >= guard.max_redirects {
            response.close();
            return Err(validation(format!(
                "Exceeded maximum redirects ({}) while fetching '{url}'",
                guard.max_redirects
            )));
        }

        let location = response.headers.get("Location").map(str::to_string);
        let location = match location {
            Some(l) if !sem_utils::pycompat::fmt::strip(&l).is_empty() => l,
            _ => {
                response.close();
                return Err(validation(format!(
                    "Redirect from '{current_url}' is missing a Location header"
                )));
            }
        };

        let next_url = pyurl::urljoin(&current_url, sem_utils::pycompat::fmt::strip(&location))?;
        let next_host = pyurl::urlparse(&next_url)?
            .hostname()
            .unwrap_or_default()
            .to_lowercase();
        let hop_allow_private_ips = if !next_host.is_empty() && next_host == original_host {
            guard.allow_private_ips
        } else {
            redirect_allow_private_ips
        };
        current_pinned = _resolve_pinned_ips(&next_url, hop_allow_private_ips)?;

        if auth_stripped || _should_strip_auth(&current_url, &next_url) {
            auth_stripped = true;
            let mut headers = options.headers.clone().unwrap_or_default();
            for sensitive in SENSITIVE {
                headers.retain(|k, _| !k.eq_ignore_ascii_case(sensitive));
            }
            options.headers = Some(headers);
            options.auth = None;
            if caller_supplied {
                for sensitive in SENSITIVE {
                    session.headers.remove(sensitive);
                }
                session.auth = None;
                session.trust_env = false;
            }
        }

        if STRIP_BODY_ON_REDIRECT.contains(&response.status_code)
            && current_method != "GET"
            && current_method != "HEAD"
        {
            current_method = "GET".to_string();
            options.data = None;
            options.json = None;
            options.extra.remove("files");
        }

        options.params = None;

        response.close();
        current_url = next_url;
        redirects_followed += 1;
    }
}
