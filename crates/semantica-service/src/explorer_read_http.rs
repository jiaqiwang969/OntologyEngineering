//! Graph reads share the existing HTTP framing, source, budget and deadline owners.
use crate::{
    McpService,
    explorer_http::{
        HostAuthorization, MAX_HEADER_BYTES, finish_connection_with_limit, parse_header_with_body,
        read_header, remaining, write_json_reply,
    },
    explorer_read_route::{self, Parsed},
};
use serde_json::json;
use std::{
    io,
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};
pub const PROFILE: &str = "semantica.explorer-graph-read-http/1";

/// The host guard rechecks its original source before and after computation,
/// before any successful body is published. It grants no credentials or budget.
pub fn serve_connection<A, G>(
    service: &McpService<'_>,
    mut stream: TcpStream,
    authorize: &A,
    deadline: Instant,
    check_source: &G,
) -> io::Result<()>
where
    A: Fn(Option<&str>) -> HostAuthorization,
    G: Fn() -> io::Result<()>,
{
    let result = (|| {
        stream.set_nonblocking(false)?;
        if !stream.peer_addr()?.ip().is_loopback() || remaining(deadline)? > Duration::from_secs(30)
        {
            return Err(io::Error::other("http_finite_profile_binding_invalid"));
        }
        let mut consumed = 0;
        let raw = read_header(&mut stream, deadline, Some(service), &mut consumed)?;
        let mut run = || -> io::Result<()> {
            let parsed_request = parse_header_with_body(&raw, 0);
            let head_only = parsed_request.as_ref().is_ok_and(|r| r.method == "HEAD");
            let reply = |stream: &mut TcpStream, status, body, location: Option<&str>| {
                write_json_reply(
                    service, stream, deadline, PROFILE, status, body, location, "GET", head_only,
                )
            };
            if raw.len() == MAX_HEADER_BYTES && !raw.ends_with(b"\r\n\r\n") {
                return reply(
                    &mut stream,
                    431,
                    Some(json!({"detail":"HTTP headers exceed finite profile limit"})),
                    None,
                );
            }
            let request = match parsed_request {
                Ok(v) => v,
                Err(message) => {
                    return reply(&mut stream, 400, Some(json!({"detail":message})), None);
                }
            };
            let parsed = explorer_read_route::parse_target(
                &request.method,
                &request.target,
                &request.headers["host"],
            );
            if let Parsed::Reply {
                status,
                body,
                location,
            } = &parsed
            {
                if *status != 422 {
                    return reply(&mut stream, *status, body.clone(), location.as_deref());
                }
            }
            match authorize(request.headers.get("x-api-key").map(String::as_str)) {
                HostAuthorization::Granted => {}
                HostAuthorization::Unauthorized => {
                    return reply(
                        &mut stream,
                        401,
                        Some(
                            json!({"detail":"Invalid or missing API key. Send it as the X-API-Key header."}),
                        ),
                        None,
                    );
                }
                HostAuthorization::NotConfigured => {
                    return reply(
                        &mut stream,
                        503,
                        Some(json!({"detail":crate::explorer_http::UNCONFIGURED})),
                        None,
                    );
                }
            }
            let query = match parsed {
                Parsed::Query(q) => q,
                Parsed::Reply {
                    status,
                    body,
                    location,
                } => return reply(&mut stream, status, body, location.as_deref()),
            };
            check_source()?;
            remaining(deadline)?;
            let result = service
                .graph_read_request(query, deadline)
                .and_then(|input| service.execute_graph_read_accounted(&input, deadline, true));
            // Check even when execution failed; never send a success on changed source.
            check_source()?;
            remaining(deadline)?;
            match result {
                Ok(result) => reply(&mut stream, result.status as u16, Some(result.body), None),
                Err(error) => reply(&mut stream, 503, Some(json!({"detail":error.code})), None),
            }
        };
        run()?;
        finish_connection_with_limit(
            &mut stream,
            Some(service),
            deadline,
            consumed,
            MAX_HEADER_BYTES,
        )
    })();
    let _ = stream.shutdown(Shutdown::Both);
    result
}
