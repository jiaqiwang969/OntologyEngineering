//! Finite path GET transport over the existing session, budget and host supervisor.
use crate::{
    McpService,
    explorer_http::{
        HostAuthorization, MAX_HEADER_BYTES, finish_connection_with_limit, parse_header_with_body,
        read_header, remaining, write_json_reply,
    },
    explorer_path_route::{self, Parsed},
};
use semantica_contracts::{
    SemanticExplorerPathDetails, SemanticExplorerPathDetailsExecution,
    SemanticExplorerPathDetailsJob, SemanticExplorerPathHttpMetrics,
    SemanticExplorerPathHttpResponse, SemanticRuntimeFailure,
};
use serde_json::{Value, json};
use std::{
    io,
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};

pub const PROFILE: &str = "semantica.explorer-path-http/1";

fn project(details: SemanticExplorerPathDetails) -> io::Result<Value> {
    // The service already validates source, query, budget, result and all metric
    // cross-fields. This projection does not calculate a second interpretation.
    let result = details.result;
    let required = || io::Error::other("path_http_reachable_result_required");
    let body = SemanticExplorerPathHttpResponse {
        schema: "semantica.explorer-path-http-response/1".into(),
        source: details.query.source,
        target: details.query.target,
        algorithm: details.query.algorithm,
        directed: details.query.directed,
        path: result.path,
        edge_ids: result.edge_ids,
        total_weight: result.total_weight.ok_or_else(required)?,
        hop_count: result.hop_count.ok_or_else(required)?,
        distance_band: details.distance_band.ok_or_else(required)?,
        semantic_similarity: details.semantic_similarity.value,
        path_coherence_score: details.path_coherence_score.value,
        confidence_decay: details.confidence_decay.value,
        bottleneck_node: details.bottleneck_node.value.clone(),
        alternative_path_count: details.alternative_path_count.value,
        metrics: SemanticExplorerPathHttpMetrics {
            semantic_similarity: details.semantic_similarity,
            path_coherence_score: details.path_coherence_score,
            confidence_decay: details.confidence_decay,
            bottleneck_node: details.bottleneck_node,
            alternative_path_count: details.alternative_path_count,
            work_limit: details.work_limit,
            work_used: details.work_used,
            bottleneck_work_used: details.bottleneck_work_used,
            alternative_work_used: details.alternative_work_used,
        },
        interpretation: details.interpretation,
    };
    serde_json::to_value(body).map_err(io::Error::other)
}

/// The host owns resource selection, work allowance, credentials, current source
/// checks and supervised process termination. This transport never renews time.
pub fn serve_connection<A, F>(
    service: &McpService<'_>,
    mut stream: TcpStream,
    authorize: &A,
    deadline: Instant,
    work_limit: u64,
    execute: F,
) -> io::Result<()>
where
    A: Fn(Option<&str>) -> HostAuthorization,
    F: FnOnce(
        &SemanticExplorerPathDetailsJob,
        Instant,
    ) -> Result<SemanticExplorerPathDetailsExecution, SemanticRuntimeFailure>,
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
            let parsed = explorer_path_route::parse_target(
                &request.method,
                &request.target,
                &request.headers["host"],
            );
            // Route selection and redirects precede route dependencies in the
            // original router; query-validation errors do not precede auth.
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
                        Some(json!({"detail":"Host authentication is not configured."})),
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
            if query.validate().is_err() {
                return reply(
                    &mut stream,
                    422,
                    Some(
                        json!({"detail":"Node IDs must contain non-whitespace characters in this native profile."}),
                    ),
                    None,
                );
            }
            let bytes = serde_json::to_vec(&query).map_err(io::Error::other)?;
            // Raw target bytes have already been charged by read_header; this
            // derived JSON must not be charged as a second network request.
            let result = service.execute_explorer_path_details_accounted(
                &bytes, work_limit, deadline, execute, true,
            );
            match result {
                Ok(result) => {
                    let details = result
                        .result
                        .ok_or_else(|| io::Error::other("path_http_result_missing"))?;
                    if !details.result.reachable {
                        return reply(
                            &mut stream,
                            404,
                            Some(json!({"detail":"No path found between source and target."})),
                            None,
                        );
                    }
                    reply(&mut stream, 200, Some(project(details)?), None)
                }
                Err(error) => {
                    let status = match error.code.as_str() {
                        "explorer_path_source_missing" | "explorer_path_target_missing" => 404,
                        _ => 503,
                    };
                    reply(
                        &mut stream,
                        status,
                        Some(json!({"detail":error.code})),
                        None,
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
            MAX_HEADER_BYTES,
        )
    })();
    let _ = stream.shutdown(Shutdown::Both);
    result
}
