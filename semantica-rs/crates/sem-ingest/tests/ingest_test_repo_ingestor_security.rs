//! Port of `tests/ingest/test_repo_ingestor_security.py` (issue #868). Every test installs
//! the directory's `conftest.mock_dns` and the file's `_clear_repo_host_resolve_cache`
//! fixture; because the host-resolution cache (and its TTL / size settings) is process-wide
//! state, the tests of this file run one at a time, as under pytest.
#![allow(non_snake_case)]

mod common;

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use common::*;
use sem_ingest::repo_ingestor::{
    ALLOWED_CLONE_OPTIONS, ANALYZE_COMMITS, ANALYZE_STRUCTURE, CALCULATE_METRICS, EXTRACT_CODE_FILES,
    GET_PROGRESS_TRACKER, GET_REPOSITORY_INFO, GIT_REPO, GitApi, GitCommit, GitRepo, MKDTEMP, RMTREE,
    RepoIngestor, clear_repo_host_resolve_cache, repo_host_resolve_cache_max_entries,
    repo_host_resolve_cache_ttl_seconds, set_repo_host_resolve_cache_max_entries,
    set_repo_host_resolve_cache_ttl_seconds,
};
use sem_ingest::ssrf::socket::{self, ResolverGuard};
use sem_ingest::support::ipaddr::ip_address;
use sem_ingest::support::kw::Kwargs;
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

static SERIAL: Mutex<()> = Mutex::new(());

/// `conftest.mock_dns` + `_clear_repo_host_resolve_cache` + one-test-at-a-time.
struct Fixture {
    _dns: ResolverGuard,
    _serial: MutexGuard<'static, ()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        clear_repo_host_resolve_cache();
    }
}

fn fixture() -> Fixture {
    let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    clear_repo_host_resolve_cache();
    Fixture {
        _dns: mock_dns(),
        _serial: serial,
    }
}

/// `patch("semantica.ingest.repo_ingestor.socket.getaddrinfo", return_value=_fake_addrinfo(...))`.
fn fake_addrinfo(addrs: &[&str]) -> ResolverGuard {
    dns_returning(addrs)
}

/// The same patch with a call counter (`mock_gai.call_count`).
fn counting_addrinfo(addrs: &[&str]) -> (ResolverGuard, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let addrs: Vec<String> = addrs.iter().map(|s| s.to_string()).collect();
    let guard = socket::set_thread_getaddrinfo(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(addrs.clone())
    });
    (guard, calls)
}

fn assert_validation(err: SemanticaError, pattern: &str) {
    assert!(err.is_validation_error(), "expected ValidationError, got {err:?}");
    let re = regex::Regex::new(pattern).unwrap();
    assert!(re.is_match(&err.to_string()), "{err} does not match {pattern}");
}

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

// --- fakes for `patch("semantica.ingest.repo_ingestor.git.Repo")` ---

/// `MockRepo.return_value` / `MockRepo.clone_from.return_value` (an unconfigured MagicMock:
/// iterating its collections yields nothing, attribute chains are truthy).
struct MockRepo;

impl GitRepo for MockRepo {
    fn checkout(&mut self, _branch: &str) -> Result<(), SemanticaError> {
        Ok(())
    }

    fn branches(&self) -> Result<Vec<String>, SemanticaError> {
        Ok(Vec::new())
    }

    fn tags(&self) -> Result<Vec<String>, SemanticaError> {
        Ok(Vec::new())
    }

    fn remotes(&self) -> Result<Vec<(String, String)>, SemanticaError> {
        Ok(vec![("origin".to_string(), "<MagicMock>".to_string())])
    }

    fn has_heads(&self) -> Result<bool, SemanticaError> {
        Ok(true)
    }

    fn head_commit(&self) -> Result<GitCommit, SemanticaError> {
        Ok(GitCommit {
            hexsha: "<MagicMock>".to_string(),
            message: "<MagicMock>".to_string(),
            author: "<MagicMock>".to_string(),
            committed_date: 1.0,
            files: Vec::new(),
            insertions: 0,
            deletions: 0,
        })
    }

    fn iter_commits(&self, _args: &[String]) -> Result<Vec<GitCommit>, SemanticaError> {
        Ok(Vec::new())
    }
}

/// The patched `git.Repo` class recording `clone_from` calls.
#[derive(Default)]
struct MockGit {
    clone_calls: Mutex<Vec<(String, String, Kwargs)>>,
}

impl GitApi for MockGit {
    fn clone_from(&self, url: &str, to_path: &str, options: &Kwargs) -> Result<Box<dyn GitRepo>, SemanticaError> {
        self.clone_calls
            .lock()
            .unwrap()
            .push((url.to_string(), to_path.to_string(), options.clone()));
        Ok(Box::new(MockRepo))
    }

    fn open(&self, _path: &str) -> Result<Box<dyn GitRepo>, SemanticaError> {
        Ok(Box::new(MockRepo))
    }
}

fn mock_git() -> (sem_ingest::support::hooks::HookGuard, Arc<MockGit>) {
    let git = Arc::new(MockGit::default());
    let guard = GIT_REPO.set(git.clone() as Arc<dyn GitApi>);
    (guard, git)
}

fn mock_tracker() -> sem_ingest::support::hooks::HookGuard {
    let (tracker, _display) = recording_tracker();
    GET_PROGRESS_TRACKER.set(Box::new(move || tracker.clone()))
}

// --- TestRepoUrlValidation ---

#[test]
fn test_repo_url_validation__test_accepts_https_github_url() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["140.82.112.3"]);
    RepoIngestor::_validate_repo_url("https://github.com/user/repo.git").unwrap();
}

#[test]
fn test_repo_url_validation__test_accepts_ssh_scheme() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["140.82.112.3"]);
    RepoIngestor::_validate_repo_url("ssh://git@github.com/user/repo.git").unwrap();
}

#[test]
fn test_repo_url_validation__test_accepts_scp_like_ssh_remote() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["140.82.112.3"]);
    RepoIngestor::_validate_repo_url("git@github.com:user/repo.git").unwrap();
    RepoIngestor::_validate_repo_url("deploy@gitlab.example.com:team/app.git").unwrap();
}

#[test]
fn test_repo_url_validation__test_normalizes_scp_like_to_ssh_url() {
    let _f = fixture();
    assert_eq!(
        RepoIngestor::_normalize_repo_url("git@github.com:user/repo.git"),
        "ssh://git@github.com/user/repo.git"
    );
    assert_eq!(
        RepoIngestor::_normalize_repo_url("https://github.com/user/repo.git"),
        "https://github.com/user/repo.git"
    );
}

#[test]
fn test_repo_url_validation__test_rejects_empty() {
    let _f = fixture();
    assert_validation(RepoIngestor::_validate_repo_url("").unwrap_err(), "non-empty");
}

#[test]
fn test_repo_url_validation__test_rejects_file_scheme() {
    let _f = fixture();
    assert_validation(
        RepoIngestor::_validate_repo_url("file:///tmp/repo.git").unwrap_err(),
        "Unsupported repository URL scheme",
    );
}

#[test]
fn test_repo_url_validation__test_rejects_env_var_tokens() {
    let _f = fixture();
    for url in [
        "https://attacker.example/${AWS_SECRET_ACCESS_KEY}/repo.git",
        "https://$GITHUB_TOKEN@attacker.example/repo.git",
        "git@github.com:org/${AWS_SECRET_ACCESS_KEY}.git",
    ] {
        assert_validation(RepoIngestor::_validate_repo_url(url).unwrap_err(), "environment variable");
    }
}

#[test]
fn test_repo_url_validation__test_accepts_literal_dollar_without_env_var_token() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["140.82.112.3"]);
    RepoIngestor::_validate_repo_url("https://example.com/repo$1.git").unwrap();
    RepoIngestor::_validate_repo_url("git@example.com:team/repo$1.git").unwrap();
}

#[test]
fn test_repo_url_validation__test_rejects_localhost_and_loopback() {
    let _f = fixture();
    assert_validation(
        RepoIngestor::_validate_repo_url("https://localhost/repo.git").unwrap_err(),
        "not allowed|blocked",
    );
    assert_validation(RepoIngestor::_validate_repo_url("https://127.0.0.1/repo.git").unwrap_err(), "blocked");
    assert_validation(
        RepoIngestor::_validate_repo_url("git@localhost:repo.git").unwrap_err(),
        "not allowed|blocked",
    );
    assert_validation(RepoIngestor::_validate_repo_url("git@127.0.0.1:repo.git").unwrap_err(), "blocked");
}

#[test]
fn test_repo_url_validation__test_rejects_private_and_metadata_ips() {
    let _f = fixture();
    for url in [
        "https://10.0.0.1/repo.git",
        "https://192.168.1.1/repo.git",
        "https://172.16.5.5/repo.git",
        "http://169.254.169.254/latest/meta-data/",
        "git@10.0.0.1:repo.git",
        "git@169.254.169.254:repo.git",
    ] {
        assert_validation(RepoIngestor::_validate_repo_url(url).unwrap_err(), "blocked");
    }
}

#[test]
fn test_repo_url_validation__test_rejects_hostname_resolving_to_private_ip() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["10.0.0.5"]);
    assert_validation(
        RepoIngestor::_validate_repo_url("https://internal.example/repo.git").unwrap_err(),
        "blocked",
    );
}

#[test]
fn test_repo_url_validation__test_rejects_hostname_if_any_resolved_ip_is_blocked() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["8.8.8.8", "127.0.0.1"]);
    assert_validation(
        RepoIngestor::_validate_repo_url("https://mixed.example/repo.git").unwrap_err(),
        "blocked",
    );
}

#[test]
fn test_repo_url_validation__test_rejects_unresolvable_hostname() {
    let _f = fixture();
    let _gai = dns_failing("[Errno 8] Name or service not known");
    assert_validation(
        RepoIngestor::_validate_repo_url("https://does-not-resolve.invalid/repo.git").unwrap_err(),
        "Cannot resolve",
    );
}

#[test]
fn test_repo_url_validation__test_hostname_resolution_is_cached() {
    let _f = fixture();
    let (_gai, calls) = counting_addrinfo(&["1.2.3.4"]);
    RepoIngestor::_validate_repo_url("https://cached.example/repo.git").unwrap();
    RepoIngestor::_validate_repo_url("https://cached.example/other.git").unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_repo_url_validation__test_rejects_malformed_netloc_as_validation_error() {
    let _f = fixture();
    for url in ["http://[::1", "http://[", "https://user@[::1/repo.git"] {
        assert_validation(RepoIngestor::_validate_repo_url(url).unwrap_err(), "Invalid repository URL");
    }
}

#[test]
fn test_repo_url_validation__test_malformed_url_surfaces_as_validation_error_from_ingest() {
    let _f = fixture();
    let (_git, git) = mock_git();
    let _tracker = mock_tracker();
    let mut ingestor = RepoIngestor::new(Kwargs::new());
    assert_validation(
        ingestor.ingest_repository("http://[::1", &Kwargs::new()).unwrap_err(),
        "Invalid repository URL",
    );
    assert!(git.clone_calls.lock().unwrap().is_empty());
}

// --- TestCloneOptionAllowlist ---

#[test]
fn test_clone_option_allowlist__test_allows_safe_options() {
    let _f = fixture();
    let filtered = RepoIngestor::_filter_clone_options(&kwargs(
        json!({"depth": 1, "branch": "main", "single_branch": true, "no_tags": true}),
    ))
    .unwrap();
    assert_eq!(
        Value::Object(filtered),
        json!({"depth": 1, "branch": "main", "single_branch": true, "no_tags": true})
    );
}

#[test]
fn test_clone_option_allowlist__test_strips_processing_options_without_error() {
    let _f = fixture();
    let filtered = RepoIngestor::_filter_clone_options(&kwargs(json!({
        "depth": 1,
        "include_history": true,
        "include_extensions": ["py"],
        "file_filters": {},
        "commit_filters": {},
        "max_depth": 5,
    })))
    .unwrap();
    assert_eq!(Value::Object(filtered), json!({"depth": 1}));
}

#[test]
fn test_clone_option_allowlist__test_rejects_multi_options() {
    let _f = fixture();
    assert_validation(
        RepoIngestor::_filter_clone_options(&kwargs(json!({"multi_options": ["--template=/tmp/evil"]})))
            .unwrap_err(),
        "not permitted",
    );
}

#[test]
fn test_clone_option_allowlist__test_rejects_upload_pack_and_template() {
    let _f = fixture();
    for key in ["upload_pack", "template", "config", "env"] {
        let mut options = Map::new();
        options.insert(key.to_string(), json!("x"));
        assert_validation(RepoIngestor::_filter_clone_options(&options).unwrap_err(), "not permitted");
    }
}

#[test]
fn test_clone_option_allowlist__test_allowlist_matches_documented_safe_set() {
    let _f = fixture();
    let allowed: HashSet<&str> = ALLOWED_CLONE_OPTIONS.iter().copied().collect();
    assert_eq!(allowed, HashSet::from(["depth", "branch", "single_branch", "no_tags"]));
}

// --- TestIngestRepositoryGuards ---

#[test]
fn test_ingest_repository_guards__test_unsafe_url_never_reaches_clone_from() {
    let _f = fixture();
    let (_git, git) = mock_git();
    let _tracker = mock_tracker();
    let mut ingestor = RepoIngestor::new(Kwargs::new());
    assert_validation(
        ingestor
            .ingest_repository("https://evil.example/${AWS_SECRET_ACCESS_KEY}/r.git", &Kwargs::new())
            .unwrap_err(),
        "environment variable",
    );
    assert!(git.clone_calls.lock().unwrap().is_empty());
}

#[test]
fn test_ingest_repository_guards__test_unsafe_clone_option_never_reaches_clone_from() {
    let _f = fixture();
    let (_git, git) = mock_git();
    let _tracker = mock_tracker();
    let _gai = fake_addrinfo(&["140.82.112.3"]);
    let mut ingestor = RepoIngestor::new(Kwargs::new());
    assert_validation(
        ingestor
            .ingest_repository(
                "https://github.com/user/repo.git",
                &kwargs(json!({"multi_options": ["--template=/tmp/evil"]})),
            )
            .unwrap_err(),
        "not permitted",
    );
    assert!(git.clone_calls.lock().unwrap().is_empty());
}

#[test]
fn test_ingest_repository_guards__test_hostname_resolving_private_never_reaches_clone_from() {
    let _f = fixture();
    let (_git, git) = mock_git();
    let _tracker = mock_tracker();
    let _gai = fake_addrinfo(&["192.168.1.50"]);
    let mut ingestor = RepoIngestor::new(Kwargs::new());
    assert_validation(
        ingestor
            .ingest_repository("https://ssrf.example/internal/repo.git", &Kwargs::new())
            .unwrap_err(),
        "blocked",
    );
    assert!(git.clone_calls.lock().unwrap().is_empty());
}

/// The stack of patches shared by the two "forwarded" tests.
struct ClonePatches {
    _hooks: Vec<sem_ingest::support::hooks::HookGuard>,
    _gai: ResolverGuard,
    git: Arc<MockGit>,
}

fn clone_patches() -> ClonePatches {
    let (git_guard, git) = mock_git();
    let hooks = vec![
        git_guard,
        MKDTEMP.set(Box::new(|_prefix: &str| "/tmp/fake-repo".to_string())),
        RMTREE.set(Box::new(|_path: &str| {})),
        mock_tracker(),
        EXTRACT_CODE_FILES.set(Box::new(|_path: &str, _filters: &Kwargs| Vec::new())),
        GET_REPOSITORY_INFO.set(Box::new(|_url: &str| kwargs(json!({"url": "x"})))),
        ANALYZE_COMMITS.set(Box::new(|_path: &str, _filters: &Kwargs| Ok(Vec::new()))),
        ANALYZE_STRUCTURE.set(Box::new(|_path: &str| Map::new())),
        CALCULATE_METRICS.set(Box::new(|_path: &str| Map::new())),
    ];
    ClonePatches {
        _hooks: hooks,
        _gai: fake_addrinfo(&["140.82.112.3"]),
        git,
    }
}

#[test]
fn test_ingest_repository_guards__test_safe_options_forwarded_to_clone_from() {
    let _f = fixture();
    let patches = clone_patches();
    let mut ingestor = RepoIngestor::new(Kwargs::new());
    ingestor
        .ingest_repository(
            "https://github.com/user/repo.git",
            &kwargs(json!({"depth": 1, "branch": "main", "include_history": false})),
        )
        .unwrap();
    let calls = patches.git.clone_calls.lock().unwrap();
    let (_, _, options) = calls.last().unwrap();
    assert_eq!(options.get("depth"), Some(&json!(1)));
    assert_eq!(options.get("branch"), Some(&json!("main")));
    assert!(!options.contains_key("include_history"));
    assert!(!options.contains_key("multi_options"));
}

#[test]
fn test_ingest_repository_guards__test_scp_like_remote_normalized_before_clone() {
    let _f = fixture();
    let patches = clone_patches();
    let mut ingestor = RepoIngestor::new(Kwargs::new());
    ingestor
        .ingest_repository("git@github.com:user/repo.git", &Kwargs::new())
        .unwrap();
    let calls = patches.git.clone_calls.lock().unwrap();
    assert_eq!(calls.last().unwrap().0, "ssh://git@github.com/user/repo.git");
}

// --- TestIsReservedNAT64Regression ---

fn blocked(addr: &str) -> bool {
    RepoIngestor::_is_blocked_ip(ip_address(addr).unwrap())
}

#[test]
fn test_is_reserved_nat64_regression__test_nat64_prefix_not_blocked() {
    let _f = fixture();
    // Typical NAT64 translation of 140.82.112.3 (github.com)
    assert!(!blocked("64:ff9b::8c52:7003"));
}

#[test]
fn test_is_reserved_nat64_regression__test_nat64_local_prefix_not_blocked() {
    let _f = fixture();
    assert!(blocked("64:ff9b:1::1"));
}

#[test]
fn test_is_reserved_nat64_regression__test_private_ipv6_still_blocked() {
    let _f = fixture();
    assert!(blocked("fc00::1"));
    assert!(blocked("fd12:3456::1"));
}

#[test]
fn test_is_reserved_nat64_regression__test_ipv6_loopback_still_blocked() {
    let _f = fixture();
    assert!(blocked("::1"));
}

#[test]
fn test_is_reserved_nat64_regression__test_ipv6_link_local_still_blocked() {
    let _f = fixture();
    assert!(blocked("fe80::1"));
}

#[test]
fn test_is_reserved_nat64_regression__test_documentation_prefix_blocked() {
    let _f = fixture();
    assert!(blocked("2001:db8::1"));
}

#[test]
fn test_is_reserved_nat64_regression__test_public_ipv4_not_blocked() {
    let _f = fixture();
    assert!(!blocked("140.82.112.3"));
}

#[test]
fn test_is_reserved_nat64_regression__test_public_ipv6_not_blocked() {
    let _f = fixture();
    assert!(!blocked("2001:4860:4860::8888"));
}

#[test]
fn test_is_reserved_nat64_regression__test_host_resolving_to_nat64_address_is_allowed() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["64:ff9b::8c52:7003", "140.82.112.3"]);
    RepoIngestor::_validate_repo_url("https://github.com/user/repo.git").unwrap();
}

#[test]
fn test_is_reserved_nat64_regression__test_host_resolving_only_to_nat64_is_allowed() {
    let _f = fixture();
    let _gai = fake_addrinfo(&["64:ff9b::8c52:7003"]);
    RepoIngestor::_validate_repo_url("https://github.com/user/repo.git").unwrap();
}

// --- TestLocalPathSupport ---

#[test]
fn test_local_path_support__test_is_local_repo_path_absolute() {
    let _f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    assert!(RepoIngestor::_is_local_repo_path(&tmp.path().to_string_lossy()));
}

#[test]
fn test_local_path_support__test_is_local_repo_path_relative() {
    let _f = fixture();
    assert!(RepoIngestor::_is_local_repo_path("./repo"));
    assert!(RepoIngestor::_is_local_repo_path("../sibling-repo"));
}

#[test]
fn test_local_path_support__test_is_local_repo_path_not_remote() {
    let _f = fixture();
    assert!(!RepoIngestor::_is_local_repo_path("https://github.com/u/r.git"));
    assert!(!RepoIngestor::_is_local_repo_path("git@github.com:u/r.git"));
    assert!(!RepoIngestor::_is_local_repo_path("ssh://git@github.com/r.git"));
}

#[test]
fn test_local_path_support__test_validate_repo_url_accepts_absolute_local_path() {
    let _f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    RepoIngestor::_validate_repo_url(&tmp.path().to_string_lossy()).unwrap();
}

#[test]
fn test_local_path_support__test_validate_repo_url_accepts_relative_local_path() {
    let _f = fixture();
    RepoIngestor::_validate_repo_url("./repo").unwrap();
}

#[test]
fn test_local_path_support__test_validate_repo_url_env_var_still_blocked_in_local_path() {
    let _f = fixture();
    assert_validation(
        RepoIngestor::_validate_repo_url("./$SECRET_KEY/repo").unwrap_err(),
        "environment variable",
    );
}

#[test]
fn test_local_path_support__test_local_path_never_reaches_dns_resolution() {
    let _f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let (_gai, calls) = counting_addrinfo(&[]);
    RepoIngestor::_validate_repo_url(&tmp.path().to_string_lossy()).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_local_path_support__test_ingest_repository_local_path_passes_validation() {
    let _f = fixture();
    let tmp = tempfile::tempdir().unwrap();
    let (_git, _mock) = mock_git();
    let _tracker = mock_tracker();
    let mut ingestor = RepoIngestor::new(Kwargs::new());
    // Expect clone to fail (temp_dir logic), but NOT a ValidationError
    if let Err(exc) = ingestor.ingest_repository(&tmp.path().to_string_lossy(), &Kwargs::new()) {
        assert!(!exc.is_validation_error(), "Local path must not raise ValidationError; got: {exc}");
    }
    ingestor.cleanup();
}

// --- TestRepoHostResolveCacheThreadSafety ---

/// Restores the module-level TTL / size cap (the original's `finally:` block).
struct CacheLimits {
    ttl: f64,
    max: usize,
}

impl Drop for CacheLimits {
    fn drop(&mut self) {
        set_repo_host_resolve_cache_ttl_seconds(self.ttl);
        set_repo_host_resolve_cache_max_entries(self.max);
    }
}

#[test]
fn test_repo_host_resolve_cache_thread_safety__test_concurrent_resolve_repo_host_ips_does_not_raise() {
    let _f = fixture();
    let _limits = CacheLimits {
        ttl: repo_host_resolve_cache_ttl_seconds(),
        max: repo_host_resolve_cache_max_entries(),
    };
    // Small TTL/cap so eviction and pruning happen on nearly every call.
    set_repo_host_resolve_cache_ttl_seconds(0.001);
    set_repo_host_resolve_cache_max_entries(8);
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..32)
        .map(|worker_id| {
            let errors = errors.clone();
            std::thread::spawn(move || {
                // The patched `socket.getaddrinfo` is process-wide in Python; the Rust
                // resolver override is per thread, so each worker installs it.
                let _gai = socket::set_thread_getaddrinfo(|_| Ok(vec!["93.184.216.34".to_string()]));
                for i in 0..500 {
                    let host = format!("race-host-{worker_id}-{i}.example.com");
                    if let Err(e) = RepoIngestor::_resolve_repo_host_ips(&host) {
                        errors.lock().unwrap().push(e.to_string());
                    }
                }
            })
        })
        .collect();
    for t in threads {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !t.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(t.is_finished(), "worker thread did not finish within the 30s join timeout");
        t.join().unwrap();
    }
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "Concurrent host resolution raised {} error(s); first: {:?}", errors.len(), errors.first());
}
