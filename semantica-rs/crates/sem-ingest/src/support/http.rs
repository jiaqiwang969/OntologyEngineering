//! A port of the parts of `requests` the ingest module relies on: `Session` (headers, auth,
//! `trust_env`, proxies, mounted adapters with urllib3 `Retry` policies), `Response`
//! (`status_code`, case-insensitive `headers`, `content`, `text`, `json()`,
//! `raise_for_status()`) and the `requests.exceptions` hierarchy.
//!
//! The wire is behind the [`Transport`] trait: [`ReqwestTransport`] speaks HTTP through
//! `reqwest` (with the SSRF guard's IP pinning, see [`PinnedTarget`]); tests install fakes, either
//! on one session ([`Session::set_transport`], like `patch.object(session, "request")`) or for
//! every session created on the current thread ([`set_thread_session_factory`], like
//! `patch("requests.Session")` / `patch("requests.Session.request")`).

use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use indexmap::IndexMap;
use sem_utils::SemanticaError;
use serde_json::{Map, Value};

use super::pyurl;

/// `requests.__version__` of the reference environment (used in the default `User-Agent`).
pub const REQUESTS_VERSION: &str = "2.34.2";

/// `requests.structures.CaseInsensitiveDict`: insertion-ordered, case-insensitive keys that
/// remember the casing of the last assignment.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CaseInsensitiveDict {
    items: Vec<(String, String)>,
}

impl fmt::Debug for CaseInsensitiveDict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.items.iter().map(|(k, v)| (k, v)))
            .finish()
    }
}

impl CaseInsensitiveDict {
    /// An empty dictionary.
    pub fn new() -> Self {
        CaseInsensitiveDict::default()
    }

    fn position(&self, key: &str) -> Option<usize> {
        let lower = key.to_lowercase();
        self.items.iter().position(|(k, _)| k.to_lowercase() == lower)
    }

    /// `d[key]` lookup.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.position(key).map(|i| self.items[i].1.as_str())
    }

    /// `key in d`.
    pub fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    /// `d[key] = value` (keeps the original position, takes the new casing).
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        match self.position(&key) {
            Some(i) => self.items[i] = (key, value),
            None => self.items.push((key, value)),
        }
    }

    /// `d.pop(key, None)`.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.position(key).map(|i| self.items.remove(i).1)
    }

    /// `d.update(other)`.
    pub fn update<K: AsRef<str>, V: AsRef<str>>(&mut self, other: impl IntoIterator<Item = (K, V)>) {
        for (k, v) in other {
            self.insert(k.as_ref(), v.as_ref());
        }
    }

    /// `d.clear()`.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Items in insertion order (with the stored key casing).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// `len(d)`.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `not d`.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// `dict(d)` as an ordered map with the stored key casing.
    pub fn to_map(&self) -> IndexMap<String, String> {
        self.items.iter().cloned().collect()
    }

    /// `dict(d)` as a JSON object.
    pub fn to_json(&self) -> Map<String, Value> {
        self.items
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect()
    }
}

impl<K: Into<String>, V: Into<String>> FromIterator<(K, V)> for CaseInsensitiveDict {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut d = CaseInsensitiveDict::new();
        for (k, v) in iter {
            d.insert(k, v);
        }
        d
    }
}

/// A `requests` auth handler (`auth=("user", "pass")` is HTTP Basic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Auth {
    /// HTTP Basic credentials.
    Basic(String, String),
}

/// A request body given through `data=`.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// Form fields (`data={...}`), sent `application/x-www-form-urlencoded`.
    Form(Vec<(String, String)>),
    /// A raw body (`data="..."` / `data=b"..."`).
    Raw(Vec<u8>),
}

/// The keyword arguments of `Session.request` (`**kwargs`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestOptions {
    /// `headers=` (a plain dict: keys keep their casing).
    pub headers: Option<IndexMap<String, String>>,
    /// `params=` (values already converted with `str()`).
    pub params: Option<Vec<(String, String)>>,
    /// `data=`.
    pub data: Option<Body>,
    /// `json=`.
    pub json: Option<Value>,
    /// `auth=`.
    pub auth: Option<Auth>,
    /// `timeout=` in seconds.
    pub timeout: Option<f64>,
    /// `allow_redirects=`.
    pub allow_redirects: Option<bool>,
    /// `verify=`.
    pub verify: Option<bool>,
    /// Any other keyword (`stream`, `cookies`, ...), passed through untouched.
    pub extra: Map<String, Value>,
}

impl RequestOptions {
    /// Converts a `**kwargs` mapping (`headers`, `params`, `data`, `json`, `auth`, `timeout`,
    /// `allow_redirects`, `verify`; everything else lands in `extra`).
    pub fn from_kwargs(kwargs: &Map<String, Value>) -> Result<RequestOptions, SemanticaError> {
        let mut options = RequestOptions::default();
        for (key, value) in kwargs {
            match key.as_str() {
                "headers" => options.headers = json_to_str_map(value)?,
                "params" => options.params = json_to_params(value)?,
                "data" => {
                    options.data = match value {
                        Value::Null => None,
                        Value::String(s) => Some(Body::Raw(s.as_bytes().to_vec())),
                        Value::Object(map) => Some(Body::Form(
                            map.iter()
                                .map(|(k, v)| (k.clone(), sem_utils::pycompat::fmt::py_str(v)))
                                .collect(),
                        )),
                        other => Some(Body::Raw(other.to_string().into_bytes())),
                    }
                }
                "json" => {
                    options.json = if value.is_null() {
                        None
                    } else {
                        Some(value.clone())
                    }
                }
                "auth" => {
                    options.auth = match value {
                        Value::Null => None,
                        Value::Array(items) if items.len() == 2 => Some(Auth::Basic(
                            sem_utils::pycompat::fmt::py_str(&items[0]),
                            sem_utils::pycompat::fmt::py_str(&items[1]),
                        )),
                        other => {
                            return Err(SemanticaError::Type(format!(
                                "Unsupported auth value: {other}"
                            )));
                        }
                    }
                }
                "timeout" => options.timeout = value.as_f64(),
                "allow_redirects" => options.allow_redirects = value.as_bool(),
                "verify" => options.verify = value.as_bool(),
                _ => {
                    options.extra.insert(key.clone(), value.clone());
                }
            }
        }
        Ok(options)
    }
}

fn json_to_str_map(value: &Value) -> Result<Option<IndexMap<String, String>>, SemanticaError> {
    match value {
        Value::Null => Ok(None),
        Value::Object(map) => Ok(Some(
            map.iter()
                .map(|(k, v)| (k.clone(), sem_utils::pycompat::fmt::py_str(v)))
                .collect(),
        )),
        other => Err(SemanticaError::Type(format!(
            "headers must be a mapping, got {}",
            sem_utils::pycompat::fmt::type_name(other)
        ))),
    }
}

/// Converts a `params=` mapping to `(key, str(value))` pairs (lists expand like `doseq`).
pub fn json_to_params(value: &Value) -> Result<Option<Vec<(String, String)>>, SemanticaError> {
    match value {
        Value::Null => Ok(None),
        Value::Object(map) => {
            let mut out = Vec::new();
            for (k, v) in map {
                match v {
                    Value::Null => {}
                    Value::Array(items) => {
                        for item in items {
                            out.push((k.clone(), sem_utils::pycompat::fmt::py_str(item)));
                        }
                    }
                    other => out.push((k.clone(), sem_utils::pycompat::fmt::py_str(other))),
                }
            }
            Ok(Some(out))
        }
        other => Err(SemanticaError::Type(format!(
            "params must be a mapping, got {}",
            sem_utils::pycompat::fmt::type_name(other)
        ))),
    }
}

/// `requests.exceptions.<kind>(message)`.
pub fn requests_error(kind: &str, message: impl Into<String>) -> SemanticaError {
    SemanticaError::Other {
        module: "requests.exceptions".to_string(),
        type_name: kind.to_string(),
        message: message.into(),
    }
}

/// `isinstance(e, requests.exceptions.RequestException)`.
pub fn is_request_exception(error: &SemanticaError) -> bool {
    matches!(error, SemanticaError::Other { module, .. } if module == "requests.exceptions")
}

/// A `requests.Response`.
#[derive(Clone, Debug, Default)]
pub struct Response {
    /// `status_code`.
    pub status_code: u16,
    /// `headers`.
    pub headers: CaseInsensitiveDict,
    /// `content` (raw body bytes, already decompressed).
    pub content: Vec<u8>,
    /// `url`.
    pub url: String,
    /// `reason`.
    pub reason: String,
    /// `encoding` (from the `Content-Type` charset, `ISO-8859-1` for `text/*`).
    pub encoding: Option<String>,
    text_override: Option<String>,
    json_override: Option<Result<Value, String>>,
    raise_override: Option<String>,
}

impl Response {
    /// A response with the given status and no body.
    pub fn new(status_code: u16) -> Response {
        Response {
            status_code,
            reason: reason_phrase(status_code).to_string(),
            ..Response::default()
        }
    }

    /// Sets `headers[name] = value`.
    pub fn with_header(mut self, name: &str, value: &str) -> Response {
        self.headers.insert(name, value);
        self.encoding = get_encoding_from_headers(&self.headers);
        self
    }

    /// Sets the body bytes.
    pub fn with_content(mut self, content: impl Into<Vec<u8>>) -> Response {
        self.content = content.into();
        self
    }

    /// Sets the decoded `text` (and the body bytes, UTF-8 encoded).
    pub fn with_text(mut self, text: impl Into<String>) -> Response {
        let text = text.into();
        self.content = text.as_bytes().to_vec();
        self.text_override = Some(text);
        self
    }

    /// Makes `json()` return `value` (without changing `text`).
    pub fn with_json(mut self, value: Value) -> Response {
        self.json_override = Some(Ok(value));
        self
    }

    /// Makes `json()` raise `ValueError(message)`.
    pub fn with_json_error(mut self, message: impl Into<String>) -> Response {
        self.json_override = Some(Err(message.into()));
        self
    }

    /// Makes `raise_for_status()` raise `HTTPError(message)` (whatever the status).
    pub fn with_raise_for_status(mut self, message: impl Into<String>) -> Response {
        self.raise_override = Some(message.into());
        self
    }

    /// Sets `url`.
    pub fn with_url(mut self, url: impl Into<String>) -> Response {
        self.url = url.into();
        self
    }

    /// `response.text`: the body decoded with `encoding` (or the apparent encoding).
    pub fn text(&self) -> String {
        if let Some(text) = &self.text_override {
            return text.clone();
        }
        let encoding = self
            .encoding
            .clone()
            .unwrap_or_else(|| apparent_encoding(&self.content));
        decode_with(&self.content, &encoding)
    }

    /// `response.json()`.
    pub fn json(&self) -> Result<Value, SemanticaError> {
        match &self.json_override {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err(message)) => Err(SemanticaError::Value(message.clone())),
            None => {
                let text = self.text();
                super::pyjson::loads(&text).map_err(|e| SemanticaError::JsonDecode(e.to_string()))
            }
        }
    }

    /// `response.raise_for_status()`.
    pub fn raise_for_status(&self) -> Result<(), SemanticaError> {
        if let Some(message) = &self.raise_override {
            return Err(requests_error("HTTPError", message.clone()));
        }
        let kind = if (400..500).contains(&self.status_code) {
            "Client"
        } else if (500..600).contains(&self.status_code) {
            "Server"
        } else {
            return Ok(());
        };
        Err(requests_error(
            "HTTPError",
            format!(
                "{} {kind} Error: {} for url: {}",
                self.status_code, self.reason, self.url
            ),
        ))
    }

    /// `response.ok`.
    pub fn ok(&self) -> bool {
        self.raise_for_status().is_ok()
    }

    /// `response.close()` (bodies are fully read, so nothing to release).
    pub fn close(&self) {}
}

/// `requests.utils.get_encoding_from_headers`.
pub fn get_encoding_from_headers(headers: &CaseInsensitiveDict) -> Option<String> {
    let content_type = headers.get("content-type")?;
    let mut parts = content_type.split(';');
    let mime = parts.next().unwrap_or("").trim().to_lowercase();
    for param in parts {
        let param = param.trim();
        if let Some((k, v)) = param.split_once('=')
            && k.trim().eq_ignore_ascii_case("charset")
        {
            return Some(v.trim().trim_matches(|c| c == '\'' || c == '"').to_string());
        }
    }
    if mime.contains("text") {
        return Some("ISO-8859-1".to_string());
    }
    if mime.contains("application/json") {
        return Some("utf-8".to_string());
    }
    None
}

fn apparent_encoding(content: &[u8]) -> String {
    if std::str::from_utf8(content).is_ok() {
        return "utf-8".to_string();
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(content, true);
    detector.guess(None, true).name().to_string()
}

/// Decodes bytes with a Python codec name (`errors="replace"`).
pub fn decode_with(content: &[u8], encoding: &str) -> String {
    let label = encoding.trim().to_lowercase();
    let label = match label.as_str() {
        "latin-1" | "latin1" | "iso-8859-1" | "iso8859-1" | "l1" => {
            // Python's latin-1 maps every byte to the code point of the same value.
            return content.iter().map(|&b| b as char).collect();
        }
        "utf8" | "utf_8" | "u8" => "utf-8".to_string(),
        "ascii" | "us-ascii" => "utf-8".to_string(),
        other => other.to_string(),
    };
    match encoding_rs::Encoding::for_label(label.as_bytes()) {
        Some(enc) => enc.decode_without_bom_handling(content).0.into_owned(),
        None => String::from_utf8_lossy(content).into_owned(),
    }
}

/// The standard reason phrase for a status code (`http.HTTPStatus(code).phrase`).
pub fn reason_phrase(code: u16) -> &'static str {
    match code {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "Non-Authoritative Information",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Request Entity Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// urllib3's `Retry` policy as mounted through `HTTPAdapter(max_retries=...)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Retry {
    /// `total` (`None` = unlimited; requests' default adapter uses `0` with no read retries).
    pub total: Option<i64>,
    /// `backoff_factor`.
    pub backoff_factor: f64,
    /// `status_forcelist`.
    pub status_forcelist: Vec<u16>,
}

impl Default for Retry {
    /// `Retry(0, read=False)`, what `HTTPAdapter()` uses.
    fn default() -> Self {
        Retry {
            total: Some(0),
            backoff_factor: 0.0,
            status_forcelist: Vec::new(),
        }
    }
}

/// Connection pinning installed by the SSRF guard (`_PinnedIPHTTPAdapter`): connect only to
/// `ips`, presenting `hostname` for TLS SNI and the `Host` header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinnedTarget {
    /// Validated IP addresses, in resolution order.
    pub ips: Vec<String>,
    /// The host name the URL names.
    pub hostname: String,
}

/// `requests.adapters.HTTPAdapter`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HttpAdapter {
    /// `max_retries`.
    pub max_retries: Retry,
    /// Set on the SSRF guard's pinned adapters (`_semantica_pinned`).
    pub pinned: Option<PinnedTarget>,
}

impl HttpAdapter {
    /// `HTTPAdapter(max_retries=retry)`.
    pub fn with_retries(retry: Retry) -> HttpAdapter {
        HttpAdapter {
            max_retries: retry,
            pinned: None,
        }
    }

    /// `adapter.close()`.
    pub fn close(&self) {}
}

/// Sends one prepared request (`Session.request` without redirect handling of its own when
/// `allow_redirects=False`).
pub trait Transport: Send + Sync {
    /// Performs `method url` for `session` with `options`.
    fn request(
        &self,
        session: &Session,
        method: &str,
        url: &str,
        options: &RequestOptions,
    ) -> Result<Response, SemanticaError>;
}

type SessionFactory = Arc<dyn Fn() -> Session + Send + Sync>;

thread_local! {
    static SESSION_FACTORY: RefCell<Option<SessionFactory>> = const { RefCell::new(None) };
    static ENVIRONMENT_PROXIES: RefCell<Option<IndexMap<String, String>>> = const { RefCell::new(None) };
}

/// Restores the previous thread session factory on drop.
pub struct SessionFactoryGuard {
    previous: Option<SessionFactory>,
}

impl Drop for SessionFactoryGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        SESSION_FACTORY.with(|f| *f.borrow_mut() = previous);
    }
}

/// Makes every `Session::new()` on this thread return `factory()` until the guard is dropped
/// (the Rust form of `patch("requests.Session")` / `patch("requests.Session.request")`).
pub fn set_thread_session_factory(
    factory: impl Fn() -> Session + Send + Sync + 'static,
) -> SessionFactoryGuard {
    let previous = SESSION_FACTORY.with(|f| f.borrow_mut().replace(Arc::new(factory)));
    SessionFactoryGuard { previous }
}

/// Restores the previous environment proxy override on drop.
pub struct EnvironmentProxiesGuard {
    previous: Option<IndexMap<String, String>>,
}

impl Drop for EnvironmentProxiesGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        ENVIRONMENT_PROXIES.with(|p| *p.borrow_mut() = previous);
    }
}

/// Overrides the proxies `urllib.request.getproxies()` reports on this thread (the
/// reference machine had a system proxy at `127.0.0.1:7890`).
pub fn set_thread_environment_proxies(proxies: IndexMap<String, String>) -> EnvironmentProxiesGuard {
    let previous = ENVIRONMENT_PROXIES.with(|p| p.borrow_mut().replace(proxies));
    EnvironmentProxiesGuard { previous }
}

/// `urllib.request.getproxies()` from the environment (`<scheme>_proxy` variables, lower
/// case first) or the thread override.
pub fn environment_proxies() -> IndexMap<String, String> {
    if let Some(p) = ENVIRONMENT_PROXIES.with(|p| p.borrow().clone()) {
        return p;
    }
    let mut proxies = IndexMap::new();
    let mut vars: Vec<(String, String)> = std::env::vars().collect();
    // Upper-case names first so lower-case ones win, as getproxies_environment does.
    vars.sort_by_key(|(k, _)| k.chars().any(|c| c.is_ascii_lowercase()));
    for (name, value) in vars {
        let lower = name.to_lowercase();
        if let Some(scheme) = lower.strip_suffix("_proxy")
            && !value.is_empty()
        {
            proxies.insert(scheme.to_string(), value);
        }
    }
    proxies
}

/// `requests.utils.select_proxy(url, proxies)`.
pub fn select_proxy(url: &str, proxies: &IndexMap<String, String>) -> Option<String> {
    let parts = pyurl::urlparse(url).ok()?;
    match parts.hostname() {
        None => proxies
            .get(&parts.scheme)
            .or_else(|| proxies.get("all"))
            .cloned(),
        Some(host) => {
            let keys = [
                format!("{}://{}", parts.scheme, host),
                parts.scheme.clone(),
                format!("all://{host}"),
                "all".to_string(),
            ];
            keys.iter().find_map(|k| proxies.get(k).cloned())
        }
    }
}

fn should_bypass_proxy(url: &str, proxies: &IndexMap<String, String>) -> bool {
    let Some(no_proxy) = proxies.get("no") else {
        return false;
    };
    let Ok(parts) = pyurl::urlparse(url) else {
        return false;
    };
    let Some(host) = parts.hostname() else {
        return false;
    };
    no_proxy
        .split(',')
        .map(|h| h.trim().trim_start_matches('.'))
        .filter(|h| !h.is_empty())
        .any(|h| h == "*" || host == h || host.ends_with(&format!(".{h}")))
}

/// A `requests.Session`.
pub struct Session {
    /// `session.headers`.
    pub headers: CaseInsensitiveDict,
    /// `session.auth`.
    pub auth: Option<Auth>,
    /// `session.trust_env` (environment proxies; `.netrc` is not consulted).
    pub trust_env: bool,
    /// `session.proxies`.
    pub proxies: IndexMap<String, String>,
    /// `session.adapters` (prefix -> adapter; the longest matching prefix wins).
    pub adapters: IndexMap<String, HttpAdapter>,
    /// `session.max_redirects`.
    pub max_redirects: u32,
    transport: Arc<dyn Transport>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("headers", &self.headers)
            .field("auth", &self.auth)
            .field("trust_env", &self.trust_env)
            .field("proxies", &self.proxies)
            .field("adapters", &self.adapters)
            .finish()
    }
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

/// `requests.utils.default_headers()`.
pub fn default_headers() -> CaseInsensitiveDict {
    [
        (
            "User-Agent".to_string(),
            format!("python-requests/{REQUESTS_VERSION}"),
        ),
        ("Accept-Encoding".to_string(), "gzip, deflate".to_string()),
        ("Accept".to_string(), "*/*".to_string()),
        ("Connection".to_string(), "keep-alive".to_string()),
    ]
    .into_iter()
    .collect()
}

impl Session {
    /// `requests.Session()`: honours the thread session factory, else a real session.
    pub fn new() -> Session {
        if let Some(factory) = SESSION_FACTORY.with(|f| f.borrow().clone()) {
            return factory();
        }
        Session::with_transport(Arc::new(ReqwestTransport))
    }

    /// A session that sends through `transport`.
    pub fn with_transport(transport: Arc<dyn Transport>) -> Session {
        let mut adapters = IndexMap::new();
        adapters.insert("https://".to_string(), HttpAdapter::default());
        adapters.insert("http://".to_string(), HttpAdapter::default());
        Session {
            headers: default_headers(),
            auth: None,
            trust_env: true,
            proxies: IndexMap::new(),
            adapters,
            max_redirects: 30,
            transport,
        }
    }

    /// Replaces the transport (`patch.object(session, "request", ...)`).
    pub fn set_transport(&mut self, transport: Arc<dyn Transport>) {
        self.transport = transport;
    }

    /// The transport this session sends through.
    pub fn transport(&self) -> Arc<dyn Transport> {
        self.transport.clone()
    }

    /// `session.mount(prefix, adapter)`.
    pub fn mount(&mut self, prefix: &str, adapter: HttpAdapter) {
        self.adapters.insert(prefix.to_string(), adapter);
        // requests keeps adapters sorted by descending prefix length.
        let mut entries: Vec<(String, HttpAdapter)> = self.adapters.drain(..).collect();
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        self.adapters = entries.into_iter().collect();
    }

    /// `session.get_adapter(url)`.
    pub fn get_adapter(&self, url: &str) -> Option<&HttpAdapter> {
        let lower = url.to_lowercase();
        self.adapters
            .iter()
            .find(|(prefix, _)| lower.starts_with(&prefix.to_lowercase()))
            .map(|(_, a)| a)
    }

    /// `session.request(method, url, **kwargs)`.
    pub fn request(
        &self,
        method: &str,
        url: &str,
        options: &RequestOptions,
    ) -> Result<Response, SemanticaError> {
        self.transport.request(self, method, url, options)
    }

    /// `session.close()`.
    pub fn close(&self) {}
}

/// The real transport: HTTP(S) through `reqwest`, honouring the session's headers, auth,
/// mounted retry policies, proxies (`trust_env`) and the SSRF guard's IP pinning.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestTransport;

const REDIRECT_CODES: [u16; 5] = [301, 302, 303, 307, 308];

impl ReqwestTransport {
    fn merged_proxies(session: &Session, url: &str) -> IndexMap<String, String> {
        let mut proxies = session.proxies.clone();
        if session.trust_env {
            let env = environment_proxies();
            if !should_bypass_proxy(url, &env) {
                for (k, v) in env {
                    proxies.entry(k).or_insert(v);
                }
            }
        }
        proxies
    }

    fn send_once(
        &self,
        session: &Session,
        method: &str,
        url: &str,
        options: &RequestOptions,
    ) -> Result<Response, SemanticaError> {
        let adapter = session.get_adapter(url).cloned().unwrap_or_default();
        let proxies = Self::merged_proxies(session, url);
        let proxy = select_proxy(url, &proxies);
        if adapter.pinned.is_some() && proxy.is_some() {
            return Err(sem_utils::ValidationError::new(
                "Proxied requests are not supported through the SSRF-guarded request path (a \
                 proxy would resolve the host itself and bypass IP pinning).",
            )
            .into());
        }

        let mut full_url = url.to_string();
        if let Some(params) = &options.params
            && !params.is_empty()
        {
            let encoded = pyurl::urlencode(params);
            let sep = if full_url.contains('?') { "&" } else { "?" };
            full_url = format!("{full_url}{sep}{encoded}");
        }
        let parsed = url::Url::parse(&full_url)
            .map_err(|e| requests_error("InvalidURL", format!("Invalid URL {url:?}: {e}")))?;

        let mut builder = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none());
        if let Some(timeout) = options.timeout {
            builder = builder.timeout(Duration::from_secs_f64(timeout.max(0.0)));
        }
        if options.verify == Some(false) {
            builder = builder.danger_accept_invalid_certs(true);
        }
        match (&adapter.pinned, &proxy) {
            (Some(pin), _) => {
                let port = parsed.port_or_known_default().unwrap_or(80);
                let addrs: Vec<std::net::SocketAddr> = pin
                    .ips
                    .iter()
                    .filter_map(|ip| ip.parse::<std::net::IpAddr>().ok())
                    .map(|ip| std::net::SocketAddr::new(ip, port))
                    .collect();
                builder = builder.no_proxy().resolve_to_addrs(&pin.hostname, &addrs);
            }
            (None, Some(proxy_url)) => {
                let proxy = reqwest::Proxy::all(proxy_url.as_str())
                    .map_err(|e| requests_error("ProxyError", e.to_string()))?;
                builder = builder.proxy(proxy);
            }
            (None, None) => builder = builder.no_proxy(),
        }
        let client = builder
            .build()
            .map_err(|e| requests_error("RequestException", e.to_string()))?;

        let method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
            .map_err(|e| requests_error("RequestException", e.to_string()))?;
        let mut request = client.request(method, parsed);

        // Session headers merged with request headers (request wins; case-insensitive).
        let mut headers = session.headers.clone();
        if let Some(extra) = &options.headers {
            for (k, v) in extra {
                headers.insert(k.clone(), v.clone());
            }
        }
        let auth = options.auth.clone().or_else(|| session.auth.clone());
        if let Some(Auth::Basic(user, pass)) = auth {
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            headers.insert("Authorization", format!("Basic {token}"));
        }
        let mut body: Option<Vec<u8>> = None;
        if let Some(json) = &options.json {
            if !headers.contains_key("Content-Type") {
                headers.insert("Content-Type", "application/json");
            }
            body = Some(serde_json::to_vec(json).unwrap_or_default());
        } else if let Some(data) = &options.data {
            match data {
                Body::Form(fields) => {
                    if !headers.contains_key("Content-Type") {
                        headers.insert("Content-Type", "application/x-www-form-urlencoded");
                    }
                    body = Some(pyurl::urlencode(fields).into_bytes());
                }
                Body::Raw(bytes) => body = Some(bytes.clone()),
            }
        }
        for (k, v) in headers.iter() {
            request = request.header(k, v);
        }
        if let Some(body) = body {
            request = request.body(body);
        }

        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                requests_error("Timeout", e.to_string())
            } else if e.is_connect() {
                requests_error("ConnectionError", e.to_string())
            } else {
                requests_error("RequestException", e.to_string())
            }
        })?;
        let status = response.status().as_u16();
        let reason = response
            .status()
            .canonical_reason()
            .unwrap_or("")
            .to_string();
        let final_url = response.url().to_string();
        let mut response_headers = CaseInsensitiveDict::new();
        for (name, value) in response.headers() {
            let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
            match response_headers.get(name.as_str()) {
                Some(existing) => {
                    let joined = format!("{existing}, {value}");
                    response_headers.insert(name.as_str(), joined);
                }
                None => response_headers.insert(name.as_str(), value),
            }
        }
        let raw = response
            .bytes()
            .map_err(|e| requests_error("ChunkedEncodingError", e.to_string()))?
            .to_vec();
        let content = match response_headers
            .get("Content-Encoding")
            .map(|s| s.trim().to_lowercase())
            .as_deref()
        {
            Some("gzip") | Some("x-gzip") => super::compress::gunzip(&raw)
                .map_err(|e| requests_error("ContentDecodingError", e))?,
            Some("deflate") => super::compress::inflate_zlib_or_raw(&raw)
                .map_err(|e| requests_error("ContentDecodingError", e))?,
            _ => raw,
        };
        let encoding = get_encoding_from_headers(&response_headers);
        Ok(Response {
            status_code: status,
            headers: response_headers,
            content,
            url: final_url,
            reason,
            encoding,
            ..Response::default()
        })
    }
}

impl Transport for ReqwestTransport {
    fn request(
        &self,
        session: &Session,
        method: &str,
        url: &str,
        options: &RequestOptions,
    ) -> Result<Response, SemanticaError> {
        let retry = session
            .get_adapter(url)
            .map(|a| a.max_retries.clone())
            .unwrap_or_default();
        let idempotent = matches!(
            method.to_uppercase().as_str(),
            "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS" | "TRACE"
        );
        let mut attempt: i64 = 0;
        let mut current_url = url.to_string();
        let mut current_method = method.to_uppercase();
        let mut redirects = 0u32;
        loop {
            let result = self.send_once(session, &current_method, &current_url, options);
            let retryable = match &result {
                Err(e) => matches!(e, SemanticaError::Other { type_name, .. }
                    if type_name == "ConnectionError"),
                Ok(r) => idempotent && retry.status_forcelist.contains(&r.status_code),
            };
            if retryable && retry.total.is_none_or(|total| attempt < total) {
                attempt += 1;
                if attempt > 1 && retry.backoff_factor > 0.0 {
                    let delay = (retry.backoff_factor * 2f64.powi((attempt - 1) as i32)).min(120.0);
                    std::thread::sleep(Duration::from_secs_f64(delay));
                }
                continue;
            }
            let response = result?;
            if options.allow_redirects != Some(false)
                && REDIRECT_CODES.contains(&response.status_code)
                && let Some(location) = response.headers.get("Location")
            {
                redirects += 1;
                if redirects > session.max_redirects {
                    return Err(requests_error(
                        "TooManyRedirects",
                        format!("Exceeded {} redirects.", session.max_redirects),
                    ));
                }
                current_url = pyurl::urljoin(&current_url, location)?;
                if matches!(response.status_code, 301..=303) && current_method != "HEAD" {
                    current_method = "GET".to_string();
                }
                continue;
            }
            return Ok(response);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_dict_semantics() {
        let mut d = CaseInsensitiveDict::new();
        d.insert("Authorization", "a");
        d.insert("AUTHORIZATION", "b");
        assert_eq!(d.len(), 1);
        assert_eq!(d.get("authorization"), Some("b"));
        assert_eq!(d.iter().next().unwrap().0, "AUTHORIZATION");
        assert_eq!(d.remove("Authorization").as_deref(), Some("b"));
        assert!(d.is_empty());
    }

    #[test]
    fn proxy_selection() {
        let mut proxies = IndexMap::new();
        proxies.insert("http".to_string(), "http://127.0.0.1:7890".to_string());
        assert_eq!(
            select_proxy("http://example.com/x", &proxies).as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert!(select_proxy("https://example.com/x", &proxies).is_none());
    }

    #[test]
    fn encodings() {
        let r = Response::new(200)
            .with_header("Content-Type", "text/html")
            .with_content(b"caf\xe9".to_vec());
        assert_eq!(r.text(), "café");
        let r = Response::new(200)
            .with_header("Content-Type", "application/json")
            .with_content(br#"{"a": 1}"#.to_vec());
        assert_eq!(r.json().unwrap()["a"], 1);
        assert!(Response::new(404).raise_for_status().is_err());
    }
}
