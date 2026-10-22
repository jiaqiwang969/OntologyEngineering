//! Finite POST transport over the existing service/session and host supervisor.
use crate::{
    McpService,
    explorer_http::{
        HostAuthorization, MAX_HEADER_BYTES, finish_connection_with_limit, parse_header_with_body,
        read_header, remaining,
    },
};
use semantica_contracts::{
    SemanticExplorerSparqlExecution, SemanticExplorerSparqlHttpRequest, SemanticExplorerSparqlJob,
    SemanticExplorerSparqlJobV2, SemanticRuntimeFailure, SemanticServiceBudgetCharge,
};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};

pub const PROFILE: &str = "semantica.explorer-sparql-http/1";
pub const PROFILE_V2: &str = "semantica.explorer-sparql-http/2";
pub const MAX_BODY: usize = 256 * 1024;

fn reply(
    service: &McpService<'_>,
    stream: &mut TcpStream,
    deadline: Instant,
    profile: &str,
    status: u16,
    body: Value,
    head_only: bool,
) -> io::Result<()> {
    crate::explorer_http::write_json_reply(
        service,
        stream,
        deadline,
        profile,
        status,
        Some(body),
        None,
        "POST",
        head_only,
    )
}

/// The trusted host callback must enforce process termination and source checks
/// before returning. This transport does not create credentials, budgets or a graph.
/// Deadline expiry closes the connection; it never extends time to send an error.
pub fn serve_connection<A, F>(
    service: &McpService<'_>,
    stream: TcpStream,
    authorize: &A,
    deadline: Instant,
    execute: F,
) -> io::Result<()>
where
    A: Fn(Option<&str>) -> HostAuthorization,
    F: FnOnce(
        &SemanticExplorerSparqlJob,
        Instant,
    ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
{
    serve_connection_with(
        service,
        stream,
        authorize,
        deadline,
        PROFILE,
        |query, deadline| service.execute_explorer_sparql_accounted(query, deadline, execute, true),
    )
}

/// Same transport ledger and deadline, with resources selected by the host session.
pub fn serve_connection_v2<A, F>(
    service: &McpService<'_>,
    stream: TcpStream,
    authorize: &A,
    deadline: Instant,
    execute: F,
) -> io::Result<()>
where
    A: Fn(Option<&str>) -> HostAuthorization,
    F: FnOnce(
        &SemanticExplorerSparqlJobV2,
        Instant,
    ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
{
    serve_connection_with(
        service,
        stream,
        authorize,
        deadline,
        PROFILE_V2,
        |query, deadline| {
            service.execute_explorer_sparql_v2_accounted(query, deadline, execute, true)
        },
    )
}

fn serve_connection_with<A, F>(
    service: &McpService<'_>,
    mut stream: TcpStream,
    authorize: &A,
    deadline: Instant,
    profile: &str,
    execute: F,
) -> io::Result<()>
where
    A: Fn(Option<&str>) -> HostAuthorization,
    F: FnOnce(&str, Instant) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
{
    let result = (|| {
        // Accepted descriptors may inherit O_NONBLOCK on Darwin. Socket timeouts
        // only govern blocking reads/writes; configure the owned stream explicitly.
        stream.set_nonblocking(false)?;
        let left = remaining(deadline)?;
        if !stream.peer_addr()?.ip().is_loopback() || left > Duration::from_secs(30) {
            return Err(io::Error::other("http_finite_profile_binding_invalid"));
        }
        let mut consumed = 0;
        let raw = read_header(&mut stream, deadline, Some(service), &mut consumed)?;
        let parsed_request = parse_header_with_body(&raw, MAX_BODY as u64);
        let head_only = parsed_request.as_ref().is_ok_and(|r| r.method == "HEAD");
        let reply =
            |service: &McpService<'_>,
             stream: &mut TcpStream,
             deadline,
             profile: &str,
             status,
             body| reply(service, stream, deadline, profile, status, body, head_only);
        let mut run = || -> io::Result<()> {
            if raw.len() == MAX_HEADER_BYTES && !raw.ends_with(b"\r\n\r\n") {
                return reply(
                    service,
                    &mut stream,
                    deadline,
                    profile,
                    431,
                    json!({"detail":"HTTP headers exceed finite profile limit"}),
                );
            }
            let request = match parsed_request {
                Ok(request) => request,
                Err(message) => {
                    return reply(
                        service,
                        &mut stream,
                        deadline,
                        profile,
                        400,
                        json!({"detail":message}),
                    );
                }
            };
            if request.path != "/api/sparql" {
                return reply(
                    service,
                    &mut stream,
                    deadline,
                    profile,
                    404,
                    json!({"detail":"API route not found"}),
                );
            }
            if request.method != "POST" {
                return reply(
                    service,
                    &mut stream,
                    deadline,
                    profile,
                    405,
                    json!({"detail":"Method Not Allowed"}),
                );
            }
            match authorize(request.headers.get("x-api-key").map(String::as_str)) {
                HostAuthorization::Granted => {}
                HostAuthorization::Unauthorized => {
                    return reply(
                        service,
                        &mut stream,
                        deadline,
                        profile,
                        401,
                        json!({"detail":"Invalid or missing API key. Send it as the X-API-Key header."}),
                    );
                }
                HostAuthorization::NotConfigured => {
                    return reply(
                        service,
                        &mut stream,
                        deadline,
                        profile,
                        503,
                        json!({"detail":"Host authentication is not configured."}),
                    );
                }
            }
            let Some(length) = request
                .headers
                .get("content-length")
                .and_then(|s| s.parse::<usize>().ok())
            else {
                return reply(
                    service,
                    &mut stream,
                    deadline,
                    profile,
                    411,
                    json!({"detail":"Content-Length required"}),
                );
            };
            let mut body = vec![0u8; length];
            let mut offset = 0;
            while offset < length {
                stream.set_read_timeout(Some(remaining(deadline)?))?;
                let n = stream.read(&mut body[offset..])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "http_request_body_incomplete",
                    ));
                }
                offset += n;
                consumed += n;
                service
                    .budget
                    .charge(&SemanticServiceBudgetCharge {
                        kind: "framing".into(),
                        bytes: n as u64,
                    })
                    .map_err(|_| io::Error::other("http_service_input_budget"))?;
            }
            let request = semantica_contracts::json::strict_json(&body)
                .ok()
                .and_then(|v| serde_json::from_value::<SemanticExplorerSparqlHttpRequest>(v).ok());
            let Some(request) = request else {
                return reply(
                    service,
                    &mut stream,
                    deadline,
                    profile,
                    400,
                    json!({"detail":"Invalid query request"}),
                );
            };
            let result = execute(&request.query, deadline);
            match result {
                Ok(result) => reply(service, &mut stream, deadline, profile, 200, result.result),
                Err(error) => {
                    // Original query admission errors are completed HTTP responses,
                    // not host unavailability. Do not disguise source/process failures.
                    let (status, message) = match error.code.as_str() {
                        "explorer_worker_read_only_required" => (
                            200,
                            "Only SELECT, ASK, CONSTRUCT, and DESCRIBE queries are permitted."
                                .to_owned(),
                        ),
                        "explorer_worker_query_budget" => (
                            200,
                            format!(
                                "Query exceeds the maximum allowed length of 10,000 characters ({} received). Please shorten your query.",
                                grouped_count(request.query.chars().count())
                            ),
                        ),
                        _ => (503, error.code),
                    };
                    reply(
                        service,
                        &mut stream,
                        deadline,
                        profile,
                        status,
                        json!({"columns":[],"rows":[],"total":0,"truncated":false,"error":message,"error_line":null,"error_column":null}),
                    )
                }
            }
        };
        run()?;
        finish_connection_with_limit(
            &mut stream,
            Some(service),
            deadline,
            consumed,
            MAX_HEADER_BYTES + MAX_BODY,
        )
    })();
    let _ = stream.shutdown(Shutdown::Both);
    result
}

// Python's decimal grouping for the original human-readable character count.
fn grouped_count(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
