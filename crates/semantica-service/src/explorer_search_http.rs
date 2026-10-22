//! Finite search POST on the existing session, authorization and service budget.
use crate::{
    McpService,
    explorer_http::{
        HostAuthorization, MAX_HEADER_BYTES, finish_connection_with_limit, parse_header_with_body,
        read_header, remaining, write_json_reply,
    },
    explorer_search_route::{self, Target},
};
use semantica_contracts::{SemanticExplorerSearchQuery, SemanticServiceBudgetCharge};
use serde_json::json;
use std::{
    io::{self, Read},
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};
pub const PROFILE: &str = "semantica.explorer-search-http/1";
pub const MAX_BODY: usize = 256 * 1024;
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
            let parsed_request = parse_header_with_body(&raw, MAX_BODY as u64);
            let head_only = parsed_request.as_ref().is_ok_and(|r| r.method == "HEAD");
            let reply = |stream: &mut TcpStream, status, body, location: Option<&str>| {
                write_json_reply(
                    service, stream, deadline, PROFILE, status, body, location, "POST", head_only,
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
            if let Target::Reply {
                status,
                body,
                location,
            } = explorer_search_route::parse_target(
                &request.method,
                &request.target,
                &request.headers["host"],
            ) {
                return reply(&mut stream, status, body, location.as_deref());
            }
            let Some(length) = request
                .headers
                .get("content-length")
                .and_then(|s| s.parse::<usize>().ok())
            else {
                return reply(
                    &mut stream,
                    411,
                    Some(json!({"detail":"Content-Length required"})),
                    None,
                );
            };
            let mut body = vec![0; length];
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
            let media = request.headers.get("content-type").map(|s| {
                s.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            });
            let is_json = media.as_deref().is_none_or(|m| {
                m == "application/json" || (m.starts_with("application/") && m.ends_with("+json"))
            });
            let body = if body.is_empty() {
                serde_json::Value::Null
            } else {
                let text = match std::str::from_utf8(&body) {
                    Ok(v) => v,
                    Err(_) => {
                        return reply(
                            &mut stream,
                            400,
                            Some(json!({"detail":"There was an error parsing the body"})),
                            None,
                        );
                    }
                };
                if is_json {
                    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
                    if let Some(error) = crate::explorer_json::syntax_error(text, deadline)? {
                        return reply(&mut stream, 422, Some(error), None);
                    }
                    match semantica_contracts::json::python_finite_json(text, MAX_BODY) {
                        Ok(v) => v,
                        Err(_) => {
                            return reply(
                                &mut stream,
                                400,
                                Some(json!({"detail":"JSON value outside finite profile"})),
                                None,
                            );
                        }
                    }
                } else {
                    json!(text)
                }
            };
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
            if let Some(error) =
                explorer_search_route::validation_error(&body).map_err(io::Error::other)?
            {
                return reply(&mut stream, 422, Some(error), None);
            }
            check_source()?;
            remaining(deadline)?;
            let query = SemanticExplorerSearchQuery {
                schema: "semantica.explorer-search-query/1".into(),
                body,
            };
            let result = service
                .search_request(query, deadline)
                .and_then(|input| service.execute_search_accounted(&input, deadline, true));
            check_source()?;
            remaining(deadline)?;
            match result {
                Ok(result) => reply(&mut stream, result.status as u16, Some(result.body), None),
                Err(error)
                    if [
                        "explorer_search_tags_not_iterable",
                        "explorer_search_integer_float_overflow",
                    ]
                    .contains(&error.code.as_str()) =>
                {
                    reply(
                        &mut stream,
                        500,
                        Some(json!({"detail":"Internal server error"})),
                        None,
                    )
                }
                Err(error) => reply(&mut stream, 503, Some(json!({"detail":error.code})), None),
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
