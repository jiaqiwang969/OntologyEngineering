//! Shared fakes for the ported ingest tests (the `MagicMock` / `patch` stand-ins).
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use sem_ingest::ssrf::socket::{self, GaiError, ResolverGuard};
use sem_ingest::support::http::{
    Auth, CaseInsensitiveDict, EnvironmentProxiesGuard, RequestOptions, Response, Session,
    SessionFactoryGuard, Transport, requests_error, set_thread_environment_proxies,
    set_thread_session_factory,
};
use sem_utils::SemanticaError;
use sem_utils::progress_tracker::{ProgressDisplay, ProgressItem, ProgressTracker};

/// The public IP every host resolves to in the original conftest (`mock_dns`).
pub const PUBLIC_IP: &str = "93.184.216.34";

/// `conftest.mock_dns`: every hostname resolves to a safe public IP.
pub fn mock_dns() -> ResolverGuard {
    socket::set_thread_getaddrinfo(|_| Ok(vec![PUBLIC_IP.to_string()]))
}

/// `patch("semantica.ingest.ssrf.socket.getaddrinfo", return_value=[... ips ...])`.
pub fn dns_returning(ips: &[&str]) -> ResolverGuard {
    let ips: Vec<String> = ips.iter().map(|s| s.to_string()).collect();
    socket::set_thread_getaddrinfo(move |_| Ok(ips.clone()))
}

/// `patch(..., side_effect=socket.gaierror(...))`.
pub fn dns_failing(message: &str) -> ResolverGuard {
    let message = message.to_string();
    socket::set_thread_getaddrinfo(move |_| Err(GaiError::Gaierror(message.clone())))
}

/// The reference machine's system proxy during the baseline run
/// (`{'http': 'http://127.0.0.1:7890', 'https': 'http://127.0.0.1:7890'}`): requests that reach
/// the real transport through the SSRF guard fail closed with the proxy `ValidationError`,
/// which is how the originals that did not mock the transport behaved (and why they failed).
pub fn baseline_proxies() -> EnvironmentProxiesGuard {
    let mut proxies = IndexMap::new();
    proxies.insert("http".to_string(), "http://127.0.0.1:7890".to_string());
    proxies.insert("https".to_string(), "http://127.0.0.1:7890".to_string());
    set_thread_environment_proxies(proxies)
}

/// The proxy `ValidationError` message of the SSRF-guarded request path.
pub const PROXY_REFUSED: &str = "Proxied requests are not supported through the SSRF-guarded request path (a proxy would resolve the host itself and bypass IP pinning).";

/// Asserts `err` is the fail-closed proxy `ValidationError`.
pub fn assert_proxy_refused(err: &SemanticaError) {
    assert!(err.is_validation_error(), "expected ValidationError, got {err:?}");
    assert_eq!(err.to_string(), PROXY_REFUSED);
}

/// What a session looked like when a request was issued.
#[derive(Debug, Clone)]
pub struct Call {
    pub method: String,
    pub url: String,
    pub options: RequestOptions,
    pub session_headers: IndexMap<String, String>,
    pub session_auth: Option<Auth>,
    pub session_trust_env: bool,
    pub pinned_ips: Option<Vec<String>>,
}

impl Call {
    /// `call_args.kwargs.get("headers", {})`.
    pub fn headers(&self) -> IndexMap<String, String> {
        self.options.headers.clone().unwrap_or_default()
    }
}

type Handler = dyn Fn(&Call) -> Result<Response, SemanticaError> + Send + Sync;

/// A `MagicMock` standing in for `Session.request`: records every call and answers from a
/// queue (`side_effect=[...]`), a handler, or a fixed response (`return_value`).
#[derive(Clone, Default)]
pub struct FakeTransport {
    pub calls: Arc<Mutex<Vec<Call>>>,
    queue: Arc<Mutex<VecDeque<Result<Response, SemanticaError>>>>,
    fixed: Arc<Mutex<Option<Response>>>,
    handler: Arc<Mutex<Option<Arc<Handler>>>>,
}

impl FakeTransport {
    pub fn new() -> FakeTransport {
        FakeTransport::default()
    }

    /// `return_value=response`.
    pub fn returning(response: Response) -> FakeTransport {
        let f = FakeTransport::new();
        *f.fixed.lock().unwrap() = Some(response);
        f
    }

    /// `side_effect=[...]`.
    pub fn sequence(responses: Vec<Response>) -> FakeTransport {
        let f = FakeTransport::new();
        f.queue.lock().unwrap().extend(responses.into_iter().map(Ok));
        f
    }

    /// `side_effect=[...]` with errors.
    pub fn sequence_results(results: Vec<Result<Response, SemanticaError>>) -> FakeTransport {
        let f = FakeTransport::new();
        f.queue.lock().unwrap().extend(results);
        f
    }

    /// `side_effect=function`.
    pub fn handler(handler: impl Fn(&Call) -> Result<Response, SemanticaError> + Send + Sync + 'static) -> FakeTransport {
        let f = FakeTransport::new();
        *f.handler.lock().unwrap() = Some(Arc::new(handler));
        f
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    pub fn last_call(&self) -> Call {
        self.calls().last().cloned().expect("no calls recorded")
    }

    pub fn arc(&self) -> Arc<dyn Transport> {
        Arc::new(self.clone())
    }
}

impl Transport for FakeTransport {
    fn request(
        &self,
        session: &Session,
        method: &str,
        url: &str,
        options: &RequestOptions,
    ) -> Result<Response, SemanticaError> {
        let pinned = session
            .get_adapter(url)
            .and_then(|a| a.pinned.clone())
            .map(|p| p.ips);
        let call = Call {
            method: method.to_string(),
            url: url.to_string(),
            options: options.clone(),
            session_headers: session.headers.to_map(),
            session_auth: session.auth.clone(),
            session_trust_env: session.trust_env,
            pinned_ips: pinned,
        };
        self.calls.lock().unwrap().push(call.clone());
        if let Some(h) = self.handler.lock().unwrap().clone() {
            return h(&call);
        }
        if let Some(next) = self.queue.lock().unwrap().pop_front() {
            return next;
        }
        if let Some(r) = self.fixed.lock().unwrap().clone() {
            return Ok(r);
        }
        // An unconfigured MagicMock: a 200 response with no body.
        Ok(Response::new(200))
    }
}

/// `patch("requests.Session")` with `mock_session.headers = {}`: every new session sends
/// through `fake` and starts with empty headers.
pub fn patch_session_class(fake: &FakeTransport) -> SessionFactoryGuard {
    let t = fake.arc();
    set_thread_session_factory(move || {
        let mut s = Session::with_transport(t.clone());
        s.headers = CaseInsensitiveDict::new();
        s
    })
}

/// `patch("requests.Session.request")`: every new session sends through `fake`.
pub fn patch_session_request(fake: &FakeTransport) -> SessionFactoryGuard {
    let t = fake.arc();
    set_thread_session_factory(move || Session::with_transport(t.clone()))
}

/// `_mock_redirect(location, status=302)`.
pub fn redirect(location: &str, status: u16) -> Response {
    Response::new(status).with_header("Location", location)
}

/// `_mock_final(status=200)`.
pub fn final_response(status: u16) -> Response {
    Response::new(status)
}

/// A JSON response (`response.json.return_value = payload`).
pub fn json_response(payload: serde_json::Value) -> Response {
    Response::new(200)
        .with_header("Content-Type", "application/json")
        .with_text(payload.to_string())
        .with_json(payload)
}

/// `requests.exceptions.RequestException(message)`.
pub fn request_exception(message: &str) -> SemanticaError {
    requests_error("RequestException", message)
}

/// A progress display recording every update (the `MagicMock` tracker's call log).
#[derive(Default)]
pub struct RecordingDisplay {
    pub items: Mutex<Vec<ProgressItem>>,
}

impl ProgressDisplay for RecordingDisplay {
    fn update(&self, item: &ProgressItem) {
        self.items.lock().unwrap().push(item.clone());
    }

    fn show_summary(&self, _items: &[ProgressItem]) {}

    fn clear(&self) {}
}

/// A tracker whose updates can be inspected (`mock_tracker.start_tracking.assert_called()`).
pub fn recording_tracker() -> (Arc<ProgressTracker>, Arc<RecordingDisplay>) {
    let tracker = ProgressTracker::new(true, false, 0.0);
    tracker.set_enabled(true);
    let display = Arc::new(RecordingDisplay::default());
    tracker.set_displays(vec![display.clone()]);
    (Arc::new(tracker), display)
}

/// The progress items a recording display saw.
pub fn recorded(display: &RecordingDisplay) -> Vec<ProgressItem> {
    display.items.lock().unwrap().clone()
}

/// `patch.dict(os.environ, ...)` is not available (mutating the environment is unsafe in
/// Rust 2024); a sorted key list keeps assertions independent of map ordering.
pub fn sorted_keys(m: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    let mut keys: Vec<String> = m.keys().cloned().collect();
    keys.sort();
    keys
}
