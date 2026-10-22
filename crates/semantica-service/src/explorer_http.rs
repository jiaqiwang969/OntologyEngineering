//! One real Explorer route, borrowing the existing session, budget, and host authorization.
//! This finite loopback HTTP profile neither starts a global service nor owns credentials.
use crate::{McpService, failure, require_contract};
use semantica_contracts::{
    SemanticExplorerGraphStats, SemanticExplorerGraphStatsPort, SemanticExplorerGraphStatsRequest,
    SemanticRuntimeFailure, SemanticServiceBudgetCharge,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};

pub const PROFILE: &str = "semantica.explorer-graph-stats-http/1";
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub(super) const UNCONFIGURED: &str = "Server is not configured for authentication. Set the SEMANTICA_API_KEY environment variable, or explicitly opt into unauthenticated access (development only) with SEMANTICA_ALLOW_ANONYMOUS=true.";
const UNAUTHORIZED: &str = "Invalid or missing API key. Send it as the X-API-Key header.";

/// Fresh application-host decision. Not serializable and never parsed from the HTTP body.
/// The host retains its existing secret/configuration/rotation authority; this adapter reads no env.
#[derive(Clone, Copy)]
pub enum HostAuthorization {
    Granted,
    NotConfigured,
    Unauthorized,
}

impl McpService<'_> {
    pub fn execute_graph_stats(
        &self,
        input: &SemanticExplorerGraphStatsRequest,
    ) -> Result<SemanticExplorerGraphStats, SemanticRuntimeFailure> {
        require_contract(input.validate())?;
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "request".into(),
            bytes: 0,
        })?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| failure("service_session_lock_failed", "session"))?;
        if session.revision() != input.expected_revision {
            return Err(failure("service_revision_mismatch", "session"));
        }
        let sha = session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?
            .dataset_sha256;
        if sha != input.expected_dataset_sha256 {
            return Err(failure("service_dataset_identity_mismatch", "source"));
        }
        let raw = semantica_runtime::context::explorer_graph_stats(&session)
            .map_err(|_| failure("service_graph_session_unavailable", "session"))?;
        let result: SemanticExplorerGraphStats =
            serde_json::from_value(raw).map_err(|_| failure("service_result_invalid", "query"))?;
        require_contract(result.validate())?;
        let bytes = serde_json::to_vec(&result)
            .map_err(|_| failure("service_result_invalid", "query"))?
            .len();
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "response".into(),
            bytes: bytes as u64,
        })?;
        Ok(result)
    }
    fn stats_request(&self) -> Result<SemanticExplorerGraphStatsRequest, SemanticRuntimeFailure> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| failure("service_session_lock_failed", "session"))?;
        Ok(SemanticExplorerGraphStatsRequest {
            schema: "semantica.explorer-graph-stats-request/1".into(),
            expected_revision: session.revision(),
            expected_dataset_sha256: session
                .snapshot()
                .map_err(|_| failure("service_snapshot_failed", "session"))?
                .dataset_sha256,
        })
    }
}
impl SemanticExplorerGraphStatsPort for McpService<'_> {
    fn graph_stats(
        &self,
        input: &SemanticExplorerGraphStatsRequest,
    ) -> Result<SemanticExplorerGraphStats, SemanticRuntimeFailure> {
        self.execute_graph_stats(input)
    }
}

fn error(detail: &str) -> Value {
    json!({"detail": detail})
}
pub(super) fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "http_connection_deadline"))
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}
pub(super) struct Request {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) target: String,
    pub(super) headers: BTreeMap<String, String>,
}
fn parse_header(raw: &[u8]) -> Result<Request, &'static str> {
    parse_header_with_body(raw, 0)
}
pub(super) fn parse_header_with_body(raw: &[u8], max_body: u64) -> Result<Request, &'static str> {
    let s = std::str::from_utf8(raw).map_err(|_| "Invalid HTTP framing")?;
    if !s.ends_with("\r\n\r\n")
        || s.bytes()
            .any(|b| b == 0 || (b < 32 && ![b'\r', b'\n'].contains(&b)))
    {
        return Err("Invalid HTTP framing");
    }
    let mut lines = s[..s.len() - 4].split("\r\n");
    let parts = lines
        .next()
        .ok_or("Invalid HTTP framing")?
        .split(' ')
        .collect::<Vec<_>>();
    if parts.len() != 3
        || !token(parts[0])
        || parts[2] != "HTTP/1.1"
        || !parts[1].starts_with('/')
        || !parts[1].is_ascii()
        || parts[1].bytes().any(|b| b <= 32 || b == 127 || b == b'#')
    {
        return Err("Invalid HTTP framing");
    }
    let mut headers = BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("Invalid HTTP framing")?;
        if !token(name) || value.bytes().any(|b| !(32..127).contains(&b)) {
            return Err("Invalid HTTP framing");
        }
        // A deliberately strict finite profile: duplicates are rejected, not ambiguously combined.
        if headers
            .insert(
                name.to_ascii_lowercase(),
                value.trim_matches(' ').to_owned(),
            )
            .is_some()
        {
            return Err("Ambiguous HTTP framing");
        }
    }
    if headers.get("host").is_none_or(|v| v.is_empty()) {
        return Err("Host header required");
    }
    if headers.contains_key("transfer-encoding") {
        return Err("Transfer-Encoding is unsupported in this finite profile");
    }
    if let Some(v) = headers.get("content-length") {
        if v.is_empty()
            || !v.bytes().all(|b| b.is_ascii_digit())
            || v.parse::<u64>().ok().is_none_or(|n| n > max_body)
        {
            return Err("Request bodies are unsupported in this finite profile");
        }
    }
    Ok(Request {
        method: parts[0].into(),
        path: parts[1].split('?').next().unwrap_or(parts[1]).into(),
        target: parts[1].into(),
        headers,
    })
}
pub(super) fn read_header(
    stream: &mut TcpStream,
    deadline: Instant,
    service: Option<&McpService<'_>>,
    raw_consumed: &mut usize,
) -> io::Result<Vec<u8>> {
    let mut header = Vec::new();
    loop {
        if header.len() == MAX_HEADER_BYTES {
            return Ok(header);
        }
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let mut b = [0u8; 1];
        let n = stream.read(&mut b)?;
        if n == 0 {
            return Ok(header);
        }
        // Retain and charge the actual consumed byte before any later socket
        // timeout/error. The existing budget port remains the sole ledger.
        *raw_consumed += n;
        header.push(b[0]);
        if let Some(service) = service {
            service
                .budget
                .charge(&SemanticServiceBudgetCharge {
                    kind: "framing".into(),
                    bytes: n as u64,
                })
                .map_err(|_| io::Error::other("http_service_input_budget"))?;
        }
        if header.ends_with(b"\r\n\r\n") {
            return Ok(header);
        }
    }
}
fn respond(
    stream: &mut TcpStream,
    deadline: Instant,
    service: Option<&McpService<'_>>,
    status: u16,
    body: &Value,
    body_already_charged: bool,
) -> io::Result<()> {
    respond_for_method(
        stream,
        deadline,
        service,
        status,
        body,
        body_already_charged,
        false,
    )
}
fn respond_for_method(
    stream: &mut TcpStream,
    deadline: Instant,
    service: Option<&McpService<'_>>,
    status: u16,
    body: &Value,
    body_already_charged: bool,
    head_only: bool,
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let body = serde_json::to_vec(body).map_err(io::Error::other)?;
    let allow = if status == 405 { "Allow: GET\r\n" } else { "" };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nX-Semantica-Profile: {PROFILE}\r\nX-Semantica-Stats-Scope: canonical-context-records\r\nX-Semantica-Historical-Index: not-reconstructed\r\n{allow}\r\n",
        body.len()
    );
    if let Some(service) = service {
        let bytes = header.len()
            + if head_only || body_already_charged {
                0
            } else {
                body.len()
            };
        service
            .budget
            .charge(&SemanticServiceBudgetCharge {
                kind: "response".into(),
                bytes: bytes as u64,
            })
            .map_err(|_| io::Error::other("http_service_output_budget"))?;
    }
    // Check the same absolute deadline before EVERY write, not a renewed per-write timeout.
    for data in [
        header.as_bytes(),
        if head_only { &[] } else { body.as_slice() },
    ] {
        let mut rest = data;
        while !rest.is_empty() {
            stream.set_write_timeout(Some(remaining(deadline)?))?;
            let n = stream.write(rest)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "http_response_write_zero",
                ));
            }
            rest = &rest[n..];
        }
    }
    Ok(())
}
fn serve_inner<F: Fn(Option<&str>) -> HostAuthorization>(
    service: Option<&McpService<'_>>,
    stream: &mut TcpStream,
    authorize: &F,
    deadline: Instant,
    raw_consumed: &mut usize,
) -> io::Result<()> {
    let raw = read_header(stream, deadline, service, raw_consumed)?;
    if raw.len() == MAX_HEADER_BYTES && !raw.ends_with(b"\r\n\r\n") {
        return respond(
            stream,
            deadline,
            service,
            431,
            &error("HTTP headers exceed finite profile limit"),
            false,
        );
    }
    let request = match parse_header(&raw) {
        Ok(v) => v,
        Err(message) => return respond(stream, deadline, service, 400, &error(message), false),
    };
    let head_only = request.method == "HEAD";
    let respond = |stream: &mut TcpStream,
                   deadline,
                   service: Option<&McpService<'_>>,
                   status,
                   body: &Value,
                   charged| {
        respond_for_method(stream, deadline, service, status, body, charged, head_only)
    };
    if request.path != "/api/graph/stats" {
        return respond(
            stream,
            deadline,
            service,
            404,
            &error("API route not found"),
            false,
        );
    }
    if request.method != "GET" {
        return respond(
            stream,
            deadline,
            service,
            405,
            &error("Method Not Allowed"),
            false,
        );
    }
    match authorize(request.headers.get("x-api-key").map(String::as_str)) {
        HostAuthorization::NotConfigured => {
            return respond(stream, deadline, service, 503, &error(UNCONFIGURED), false);
        }
        HostAuthorization::Unauthorized => {
            return respond(stream, deadline, service, 401, &error(UNAUTHORIZED), false);
        }
        HostAuthorization::Granted => {}
    }
    let Some(service) = service else {
        return respond(
            stream,
            deadline,
            None,
            503,
            &error("GraphSession not initialized."),
            false,
        );
    };
    let result = service
        .stats_request()
        .and_then(|request| SemanticExplorerGraphStatsPort::graph_stats(service, &request));
    match result {
        Ok(stats) => respond(
            stream,
            deadline,
            Some(service),
            200,
            &serde_json::to_value(stats).map_err(io::Error::other)?,
            true,
        ),
        Err(e) => {
            let message = if e.code == "service_graph_session_unavailable" {
                "GraphSession not initialized."
            } else {
                "Graph statistics unavailable in the current service state."
            };
            respond(stream, deadline, Some(service), 503, &error(message), false)
        }
    }
}

// A completed response must not be discarded by a reset from already queued,
// deliberately unprocessed excess input. Send FIN and discard only a bounded
// amount immediately available now. No renewed wait, request parsing or dispatch.
fn finish_connection(
    stream: &mut TcpStream,
    service: Option<&McpService<'_>>,
    deadline: Instant,
    raw_consumed: usize,
) -> io::Result<()> {
    finish_connection_with_limit(stream, service, deadline, raw_consumed, MAX_HEADER_BYTES)
}
pub(super) fn finish_connection_with_limit(
    stream: &mut TcpStream,
    service: Option<&McpService<'_>>,
    deadline: Instant,
    raw_consumed: usize,
    max_consumed: usize,
) -> io::Result<()> {
    stream.shutdown(Shutdown::Write)?;
    stream.set_nonblocking(true)?;
    let mut drained = 0;
    let cap = max_consumed.saturating_sub(raw_consumed);
    let mut bytes = [0u8; 4096];
    while drained < cap {
        remaining(deadline)?;
        let limit = bytes.len().min(cap - drained);
        match stream.read(&mut bytes[..limit]) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                drained += n;
                if let Some(service) = service {
                    service
                        .budget
                        .charge(&SemanticServiceBudgetCharge {
                            kind: "framing".into(),
                            bytes: n as u64,
                        })
                        .map_err(|_| io::Error::other("http_service_cleanup_input_budget"))?;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    // No consumption remains available. A nonblocking peek is an observation,
    // not an extra consumed byte or framing allowance. Only actual peer EOF
    // proves there is no queued excess; WouldBlock is still unknown/refused.
    loop {
        remaining(deadline)?;
        let mut observed = [0u8; 1];
        match stream.peek(&mut observed) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                // The response completed and Write FIN already succeeded in
                // this same private call chain. Stop receiving queued excess;
                // do not consume another byte, renew the deadline or dispatch.
                remaining(deadline)?;
                return receive_shutdown_result(stream.shutdown(Shutdown::Read));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                return Err(io::Error::other("http_finite_cleanup_eof_unknown"));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Handle and RAII-close one actual loopback connection transferred by the application.
/// The caller retains listener, session, host-auth policy, service budget and accept lifecycle.
/// A missing service is explicitly not ready; a present arbitrary RDF dataset is also refused.
/// The remaining absolute deadline bounds socket I/O and rejects late output; it cannot
/// preempt a synchronous host callback or finite Dataset CPU work. No total CPU claim is made.
/// Response headers distinguish current canonical record statistics from unknown legacy
/// history-dependent type-index semantics; no caller-provided "history free" proof is accepted.
pub fn serve_connection<F: Fn(Option<&str>) -> HostAuthorization>(
    service: Option<&McpService<'_>>,
    mut stream: TcpStream,
    authorize: &F,
    timeout: Duration,
) -> io::Result<()> {
    let outcome = (|| {
        if !stream.peer_addr()?.ip().is_loopback()
            || timeout.is_zero()
            || timeout > Duration::from_secs(30)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "http_finite_profile_binding_invalid",
            ));
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| io::Error::other("http_deadline_overflow"))?;
        let mut raw_consumed = 0;
        serve_inner(service, &mut stream, authorize, deadline, &mut raw_consumed)?;
        finish_connection(&mut stream, service, deadline, raw_consumed)
    })();
    let _ = stream.shutdown(Shutdown::Both);
    outcome
}

// Called only for the actual shutdown(Read) operation in finish_connection,
// after serve_inner completed its response and shutdown(Write) succeeded.
// macOS reports ENOTCONN when the local direction is already disconnected.
// This is local closure, not peer EOF or validation/consumption of queued input.
fn receive_shutdown_result(result: io::Result<()>) -> io::Result<()> {
    match result {
        #[cfg(target_os = "macos")]
        Err(error)
            if error.kind() == io::ErrorKind::NotConnected && error.raw_os_error() == Some(57) =>
        {
            Ok(())
        }
        other => other,
    }
}

#[cfg(test)]
mod receive_shutdown_classification_tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn nonclosure_errors_are_preserved_by_the_private_classifier() {
        for kind in [
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::TimedOut,
            io::ErrorKind::InvalidInput,
            io::ErrorKind::Interrupted,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::Other,
        ] {
            let error = receive_shutdown_result(Err(io::Error::new(
                kind,
                "actual classified error control",
            )))
            .unwrap_err();
            assert_eq!(error.kind(), kind);
        }
    }
    #[test]
    fn synthetic_notconnected_without_raw_errno_is_not_a_darwin_closure_proof() {
        let error = receive_shutdown_result(Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "synthetic kind is not a syscall observation",
        )))
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotConnected);
        assert_eq!(error.raw_os_error(), None);
        assert_eq!(
            error.to_string(),
            "synthetic kind is not a syscall observation"
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn actual_darwin_receive_shutdown_after_complete_response_and_fin_is_local_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        peer.shutdown(Shutdown::Write).unwrap();
        let mut observed = [0u8; 1];
        assert_eq!(server.read(&mut observed).unwrap(), 0);
        respond(
            &mut server,
            Instant::now() + Duration::from_secs(2),
            None,
            431,
            &error("local receive-close syscall control"),
            false,
        )
        .unwrap();
        server.shutdown(Shutdown::Write).unwrap();
        let raw = server.shutdown(Shutdown::Read).unwrap_err();
        eprintln!(
            "actual Darwin ReceiveShutdown {:?} raw {:?}: local closure only",
            raw.kind(),
            raw.raw_os_error()
        );
        assert_eq!(raw.kind(), io::ErrorKind::NotConnected);
        assert_eq!(raw.raw_os_error(), Some(57));
        receive_shutdown_result(Err(raw)).unwrap();
        drop(peer);
    }
}

#[cfg(test)]
mod finite_cleanup_guard_tests {
    use super::*;
    use std::net::TcpListener;
    fn connected() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (server, peer)
    }
    #[test]
    fn exhausted_cap_with_unobserved_peer_eof_is_still_refused() {
        let (mut server, _peer) = connected();
        let deadline = Instant::now() + Duration::from_secs(2);
        respond(
            &mut server,
            deadline,
            None,
            431,
            &error("finite cleanup unit control"),
            false,
        )
        .unwrap();
        let error = finish_connection(&mut server, None, deadline, MAX_HEADER_BYTES).unwrap_err();
        assert_eq!(error.to_string(), "http_finite_cleanup_eof_unknown");
    }
    #[test]
    fn expired_cleanup_deadline_is_not_renewed_by_receive_shutdown() {
        let (mut server, _peer) = connected();
        respond(
            &mut server,
            Instant::now() + Duration::from_secs(2),
            None,
            431,
            &error("finite cleanup unit control"),
            false,
        )
        .unwrap();
        let error = finish_connection(
            &mut server,
            None,
            Instant::now() - Duration::from_millis(1),
            MAX_HEADER_BYTES,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn original_write_fin_notconnected_failure_is_not_normalized() {
        let (mut server, _peer) = connected();
        server.shutdown(Shutdown::Both).unwrap();
        let error = finish_connection(
            &mut server,
            None,
            Instant::now() + Duration::from_secs(2),
            MAX_HEADER_BYTES,
        )
        .unwrap_err();
        eprintln!(
            "actual first FIN failure {:?}, raw {:?}",
            error.kind(),
            error.raw_os_error()
        );
        assert_eq!(error.kind(), io::ErrorKind::NotConnected);
    }
}

/// Shared finite response writer; reserve exact wire bytes before sending.
pub(super) fn write_json_reply(
    service: &McpService<'_>,
    stream: &mut TcpStream,
    deadline: Instant,
    profile: &str,
    status: u16,
    body: Option<serde_json::Value>,
    location: Option<&str>,
    allow_method: &str,
    head_only: bool,
) -> io::Result<()> {
    if location.is_some_and(|s| !s.is_ascii() || s.bytes().any(|b| b < 32 || b == 127)) {
        return Err(io::Error::other("http_response_location_invalid"));
    }
    let content_type = if body.is_some() {
        "Content-Type: application/json\r\n"
    } else {
        ""
    };
    let body = body
        .map(|v| serde_json::to_vec(&v))
        .transpose()
        .map_err(io::Error::other)?
        .unwrap_or_default();
    let reason = match status {
        200 => "OK",
        307 => "Temporary Redirect",
        422 => "Unprocessable Entity",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let allow = if status == 405 {
        format!("Allow: {allow_method}\r\n")
    } else {
        String::new()
    };
    let location = location
        .map(|s| format!("Location: {s}\r\n"))
        .unwrap_or_default();
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n{content_type}Content-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nX-Semantica-Profile: {profile}\r\n{allow}{location}\r\n",
        body.len()
    );
    service
        .budget
        .charge(&SemanticServiceBudgetCharge {
            kind: "response".into(),
            bytes: (header.len() + if head_only { 0 } else { body.len() }) as u64,
        })
        .map_err(|_| io::Error::other("http_service_output_budget"))?;
    for bytes in [
        header.as_bytes(),
        if head_only { &[] } else { body.as_slice() },
    ] {
        let mut rest = bytes;
        while !rest.is_empty() {
            stream.set_write_timeout(Some(remaining(deadline)?))?;
            let n = stream.write(rest)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "http_response_write_zero",
                ));
            }
            rest = &rest[n..];
        }
    }
    Ok(())
}
