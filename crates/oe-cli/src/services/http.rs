//! Application host for the existing finite HTTP profile. No alternate dataset or authority.
use anyhow::{Result, ensure};
use clap::Args;
use oe_core::{SkillRoot, read_regular_bytes};
use semantica_runtime::dataset::DatasetSession;
use semantica_service::explorer_http::{HostAuthorization, serve_connection};
use semantica_service::{McpService, NativeServiceBudget, load_context_graph};
use std::{
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Args)]
pub struct HttpGraphStatsArgs {
    #[arg(long)]
    context_graph: PathBuf,
    #[arg(long)]
    context_graph_sha256: String,
    #[arg(long, default_value = "127.0.0.1:0")]
    bind: SocketAddr,
    /// Read this private file afresh for every request; never print its contents.
    #[arg(long, conflicts_with = "allow_anonymous")]
    api_key_file: Option<PathBuf>,
    #[arg(long)]
    allow_anonymous: bool,
    #[arg(long, default_value_t = 32, value_parser = clap::value_parser!(u32).range(1..=128))]
    max_requests: u32,
    #[arg(long, default_value_t = 30000, value_parser = clap::value_parser!(u64).range(1..=60000))]
    duration_ms: u64,
}

#[derive(Args)]
pub struct HttpGraphReadArgs {
    #[arg(long, default_value="semantica.dataset-default/1", value_parser=["semantica.dataset-default/1", "semantica.context-50k/1"])]
    pub resource_profile: String,
    #[command(flatten)]
    host: HttpGraphStatsArgs,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum GraphRoute {
    Stats,
    Read,
    Search,
    Decisions,
}
pub(super) fn execute_read(
    root: &SkillRoot,
    args: HttpGraphReadArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
) -> Result<u8> {
    execute_profile(
        root,
        args.host,
        session,
        budget,
        None,
        false,
        None,
        GraphRoute::Read,
    )
}

pub(super) fn execute_decisions(
    root: &SkillRoot,
    args: HttpGraphReadArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
) -> Result<u8> {
    execute_profile(
        root,
        args.host,
        session,
        budget,
        None,
        false,
        None,
        GraphRoute::Decisions,
    )
}

pub(super) fn execute_search(
    root: &SkillRoot,
    args: HttpGraphReadArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
) -> Result<u8> {
    execute_profile(
        root,
        args.host,
        session,
        budget,
        None,
        false,
        None,
        GraphRoute::Search,
    )
}

#[derive(Args)]
pub struct HttpSparqlArgs {
    #[command(flatten)]
    host: HttpGraphStatsArgs,
    /// New private directory for per-request subprocess evidence.
    #[arg(long)]
    evidence: PathBuf,
}
#[derive(Args)]
pub struct HttpSparqlV2Args {
    #[arg(long, value_parser=["semantica.dataset-default/1", "semantica.context-50k/1"])]
    pub resource_profile: String,
    #[command(flatten)]
    pub query: HttpSparqlArgs,
}
#[derive(Args)]
pub struct HttpPathArgs {
    #[arg(long, value_parser=["semantica.dataset-default/1", "semantica.context-50k/1"])]
    pub resource_profile: String,
    #[command(flatten)]
    pub query: HttpSparqlArgs,
    #[arg(long,default_value_t=1_000_000,value_parser=clap::value_parser!(u64).range(1..=10_000_000))]
    pub work_limit: u64,
}
pub(super) fn execute_path(
    root: &SkillRoot,
    args: HttpSparqlArgs,
    work_limit: u64,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
) -> Result<u8> {
    let evidence = root.expand(&args.evidence)?;
    ensure!(
        evidence.starts_with(root.path().join("var")),
        "explorer_query_evidence_outside_private_root"
    );
    oe_core::create_new_private_dir(&evidence)?;
    execute_profile(
        root,
        args.host,
        session,
        budget,
        Some(&evidence),
        false,
        Some(work_limit),
        GraphRoute::Stats,
    )
}
pub(super) fn execute_sparql(
    root: &SkillRoot,
    args: HttpSparqlArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
) -> Result<u8> {
    execute_sparql_profile(root, args, session, budget, false)
}
pub(super) fn execute_sparql_v2(
    root: &SkillRoot,
    args: HttpSparqlArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
) -> Result<u8> {
    execute_sparql_profile(root, args, session, budget, true)
}
fn execute_sparql_profile(
    root: &SkillRoot,
    args: HttpSparqlArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
    version2: bool,
) -> Result<u8> {
    let evidence = root.expand(&args.evidence)?;
    ensure!(
        evidence.starts_with(root.path().join("var")),
        "explorer_query_evidence_outside_private_root"
    );
    oe_core::create_new_private_dir(&evidence)?;
    execute_profile(
        root,
        args.host,
        session,
        budget,
        Some(&evidence),
        version2,
        None,
        GraphRoute::Stats,
    )
}

fn authorize(path: Option<&Path>, anonymous: bool, candidate: Option<&str>) -> HostAuthorization {
    if anonymous {
        return HostAuthorization::Granted;
    }
    let Some(path) = path else {
        return HostAuthorization::NotConfigured;
    };
    let key = (|| -> Result<Vec<u8>> {
        let bytes = {
            #[cfg(windows)]
            {
                let snapshot = oe_core::private_file::read(path, 4096)?;
                snapshot.recheck(path, 4096)?;
                snapshot.bytes
            }
            #[cfg(unix)]
            {
                let file = oe_core::open_regular_file(path, 4096)?;
                #[cfg(unix)]
                let before = {
                    use std::os::unix::fs::MetadataExt;
                    let m = file.metadata()?;
                    ensure!(m.mode() & 0o777 == 0o600, "service_key_permissions");
                    (
                        m.dev(),
                        m.ino(),
                        m.mode(),
                        m.size(),
                        m.mtime(),
                        m.mtime_nsec(),
                        m.ctime(),
                        m.ctime_nsec(),
                    )
                };
                let mut bytes = Vec::new();
                (&file).take(4097).read_to_end(&mut bytes)?;
                ensure!(bytes.len() <= 4096, "service_key_size");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    for m in [file.metadata()?, path.symlink_metadata()?] {
                        ensure!(
                            m.is_file()
                                && (
                                    m.dev(),
                                    m.ino(),
                                    m.mode(),
                                    m.size(),
                                    m.mtime(),
                                    m.mtime_nsec(),
                                    m.ctime(),
                                    m.ctime_nsec()
                                ) == before,
                            "service_key_changed"
                        );
                    }
                }
                bytes
            }
            #[cfg(not(any(unix, windows)))]
            {
                anyhow::bail!("service_key_platform_unverified");
            }
        };
        ensure!(
            !bytes.is_empty() && bytes.iter().all(|b| (33..=126).contains(b)),
            "service_key_format"
        );
        Ok(bytes)
    })();
    let Ok(key) = key else {
        return HostAuthorization::NotConfigured;
    };
    let Some(candidate) = candidate else {
        return HostAuthorization::Unauthorized;
    };
    let supplied = candidate.as_bytes();
    let mut difference = key.len() ^ supplied.len();
    for (i, byte) in key.iter().enumerate() {
        difference |= usize::from(*byte ^ supplied.get(i).copied().unwrap_or(0));
    }
    if difference == 0 {
        HostAuthorization::Granted
    } else {
        HostAuthorization::Unauthorized
    }
}

pub(super) fn execute(
    root: &SkillRoot,
    args: HttpGraphStatsArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
) -> Result<u8> {
    execute_profile(
        root,
        args,
        session,
        budget,
        None,
        false,
        None,
        GraphRoute::Stats,
    )
}
fn execute_profile(
    root: &SkillRoot,
    args: HttpGraphStatsArgs,
    session: &mut DatasetSession,
    budget: &NativeServiceBudget,
    evidence: Option<&Path>,
    version2: bool,
    path_work_limit: Option<u64>,
    route: GraphRoute,
) -> Result<u8> {
    ensure!(args.bind.ip().is_loopback(), "service_loopback_required");
    let source = root.expand(&args.context_graph)?;
    let source_limit = session.dataset_byte_limit();
    let read_source = || -> Result<Vec<u8>> {
        let bytes = read_regular_bytes(&source, source_limit)?;
        ensure!(
            oe_core::sha256(&bytes) == args.context_graph_sha256,
            "service_source_identity_mismatch"
        );
        Ok(bytes)
    };
    let deadline = Instant::now() + Duration::from_millis(args.duration_ms);
    let bytes = read_source()?;
    load_context_graph(session, &oe_core::json::strict_json(&bytes)?)?;
    session.prepare_explorer_index(deadline)?;
    read_source()?;
    let key = args
        .api_key_file
        .as_ref()
        .map(|p| root.expand(p))
        .transpose()?;
    let service = McpService::new(session, budget);
    let listener = TcpListener::bind(args.bind)?;
    listener.set_nonblocking(true)?;
    println!(
        "{}",
        serde_json::json!({"schema":"semantica.http-host-ready/1","address":listener.local_addr()?.to_string(),"profile":if route==GraphRoute::Decisions{semantica_service::explorer_decisions_http::PROFILE}else if route==GraphRoute::Search{semantica_service::explorer_search_http::PROFILE}else if route==GraphRoute::Read{semantica_service::explorer_read_http::PROFILE}else if path_work_limit.is_some(){semantica_service::explorer_path_http::PROFILE}else if version2{semantica_service::explorer_sparql_http::PROFILE_V2}else if evidence.is_some(){semantica_service::explorer_sparql_http::PROFILE}else{semantica_service::explorer_http::PROFILE},"source_sha256":args.context_graph_sha256,"engineering_adoption":"not_established"})
    );
    std::io::stdout().flush()?;
    let mut requests = 0;
    while requests < args.max_requests && Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                read_source()?;
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    break;
                };
                let auth = |candidate: &str| {
                    authorize(key.as_deref(), args.allow_anonymous, Some(candidate))
                };
                if let Some(evidence) = evidence {
                    let query_deadline = deadline.min(Instant::now() + Duration::from_secs(30));
                    let authorization = |candidate: Option<&str>| match candidate {
                        Some(value) => auth(value),
                        None => authorize(key.as_deref(), args.allow_anonymous, None),
                    };
                    let request_evidence = evidence.join(format!("request-{requests:04}"));
                    if let Some(work_limit) = path_work_limit {
                        semantica_service::explorer_path_http::serve_connection(
                            &service,
                            stream,
                            &authorization,
                            query_deadline,
                            work_limit,
                            |job, original_deadline| {
                                checked_source_execution(&read_source, || {
                                    super::path_details::supervise_job(
                                        root,
                                        job,
                                        &request_evidence,
                                        original_deadline,
                                        Duration::from_secs(30),
                                    )
                                })
                            },
                        )?;
                    } else if version2 {
                        semantica_service::explorer_sparql_http::serve_connection_v2(
                            &service,
                            stream,
                            &authorization,
                            query_deadline,
                            |job, original_deadline| {
                                checked_source_execution(&read_source, || {
                                    super::sparql::supervise_job_v2(
                                        root,
                                        job,
                                        &request_evidence,
                                        original_deadline,
                                        Duration::from_secs(30),
                                    )
                                })
                            },
                        )?;
                    } else {
                        semantica_service::explorer_sparql_http::serve_connection(
                            &service,
                            stream,
                            &authorization,
                            query_deadline,
                            |job, original_deadline| {
                                checked_source_execution(&read_source, || {
                                    super::sparql::supervise_job(
                                        root,
                                        job,
                                        &request_evidence,
                                        original_deadline,
                                        Duration::from_secs(30),
                                    )
                                })
                            },
                        )?;
                    }
                } else if route == GraphRoute::Decisions {
                    semantica_service::explorer_decisions_http::serve_connection(
                        &service,
                        stream,
                        &|candidate| authorize(key.as_deref(), args.allow_anonymous, candidate),
                        deadline.min(Instant::now() + Duration::from_secs(30)),
                        &|| read_source().map(|_| ()).map_err(std::io::Error::other),
                    )?;
                } else if route == GraphRoute::Search {
                    semantica_service::explorer_search_http::serve_connection(
                        &service,
                        stream,
                        &|candidate| authorize(key.as_deref(), args.allow_anonymous, candidate),
                        deadline.min(Instant::now() + Duration::from_secs(30)),
                        &|| read_source().map(|_| ()).map_err(std::io::Error::other),
                    )?;
                } else if route == GraphRoute::Read {
                    semantica_service::explorer_read_http::serve_connection(
                        &service,
                        stream,
                        &|candidate| authorize(key.as_deref(), args.allow_anonymous, candidate),
                        deadline.min(Instant::now() + Duration::from_secs(30)),
                        &|| read_source().map(|_| ()).map_err(std::io::Error::other),
                    )?;
                } else {
                    serve_connection(
                        Some(&service),
                        stream,
                        &|candidate| authorize(key.as_deref(), args.allow_anonymous, candidate),
                        remaining.min(Duration::from_secs(2)),
                    )?;
                }
                requests += 1;
                read_source()?;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => return Err(error.into()),
        }
    }
    println!(
        "{}",
        serde_json::json!({"schema":"semantica.http-host-stop/1","requests":requests,"engineering_adoption":"not_established"})
    );
    Ok(0)
}

fn checked_source_execution<R, F, T>(
    read_source: &R,
    execute: F,
) -> std::result::Result<T, semantica_contracts::SemanticRuntimeFailure>
where
    R: Fn() -> Result<Vec<u8>>,
    F: FnOnce() -> Result<(T, Vec<u8>)>,
{
    let outcome = (|| -> Result<_> {
        read_source()?;
        let result = execute();
        read_source()?;
        Ok(result?.0)
    })();
    outcome.map_err(|error| semantica_contracts::SemanticRuntimeFailure {
        code: error.to_string(),
        phase: "host".into(),
        message: "Host query failed".into(),
        retryable: false,
    })
}

#[cfg(all(test, windows))]
#[path = "http/windows_auth_receiving.rs"]
mod windows_auth_receiving;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn authentication_is_fresh_and_private() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::tempdir_in(parent).unwrap();
        let path = dir.path().join("key");
        assert!(matches!(
            authorize(None, false, None),
            HostAuthorization::NotConfigured
        ));
        assert!(matches!(
            authorize(None, true, None),
            HostAuthorization::Granted
        ));
        std::fs::write(&path, "fixture-first-key").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            authorize(Some(&path), false, Some("fixture-first-key")),
            HostAuthorization::Granted
        ));
        assert!(matches!(
            authorize(Some(&path), false, Some("wrong")),
            HostAuthorization::Unauthorized
        ));
        std::fs::write(&path, "fixture-second-key").unwrap();
        assert!(matches!(
            authorize(Some(&path), false, Some("fixture-first-key")),
            HostAuthorization::Unauthorized
        ));
        assert!(matches!(
            authorize(Some(&path), false, Some("fixture-second-key")),
            HostAuthorization::Granted
        ));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            authorize(Some(&path), false, Some("fixture-second-key")),
            HostAuthorization::NotConfigured
        ));
        let link = dir.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(matches!(
            authorize(Some(&link), false, Some("fixture-second-key")),
            HostAuthorization::NotConfigured
        ));
    }
}
