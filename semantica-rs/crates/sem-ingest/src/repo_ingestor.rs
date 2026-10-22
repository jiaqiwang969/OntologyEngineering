//! Git repository ingestion (`semantica/ingest/repo_ingestor.py`): repository URL validation
//! (scheme allowlist, `$VAR` rejection, SSRF-safe host screening with a TTL-bounded DNS
//! cache), allowlisted clone options, cloning through the [`GitApi`] trait (the real backend
//! drives the `git` command line as GitPython does), code file extraction with language
//! detection, commit history and repository structure / metrics.

use std::collections::HashSet;
use std::net::IpAddr;
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::{py_str, str_repr, strip};
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Value, json};

use crate::config::{processing_error, validation_error};
use crate::ssrf::socket::{self, GaiError};
use crate::support::hooks::Hook;
use crate::support::ipaddr;
use crate::support::kw::{self, Kwargs};
use crate::support::{pypath, pyurl};

/// Safe subset of clone options (`ALLOWED_CLONE_OPTIONS`). Broader GitPython kwargs
/// (`multi_options`, `upload_pack`, `template`, `config`, `env`, ...) stay out of the call
/// surface.
pub const ALLOWED_CLONE_OPTIONS: [&str; 4] = ["depth", "branch", "single_branch", "no_tags"];

/// `ALLOWED_REPO_URL_SCHEMES`.
pub const ALLOWED_REPO_URL_SCHEMES: [&str; 4] = ["https", "http", "git", "ssh"];

static SCP_LIKE_REPO_URL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[^@\s]+@[^:\s]+:.+$").expect("valid regex"));
static ENV_VAR_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\$(\{[A-Za-z_][A-Za-z0-9_]*\}|[A-Za-z_][A-Za-z0-9_]*)").expect("valid regex")
});

struct CacheEntry {
    expires_at: f64,
    ips: Vec<String>,
}

/// `_REPO_HOST_RESOLVE_CACHE` (+ its lock): host -> (expiry, addresses), oldest first.
static REPO_HOST_RESOLVE_CACHE: Lazy<Mutex<IndexMap<String, CacheEntry>>> =
    Lazy::new(|| Mutex::new(IndexMap::new()));
static REPO_HOST_RESOLVE_CACHE_TTL_BITS: AtomicU64 = AtomicU64::new(60.0f64.to_bits());
static REPO_HOST_RESOLVE_CACHE_MAX: AtomicUsize = AtomicUsize::new(1024);
static MONOTONIC_START: Lazy<Instant> = Lazy::new(Instant::now);

fn monotonic() -> f64 {
    // An arbitrary positive origin, as `time.monotonic()` has.
    1.0 + MONOTONIC_START.elapsed().as_secs_f64()
}

fn cache_lock() -> std::sync::MutexGuard<'static, IndexMap<String, CacheEntry>> {
    match REPO_HOST_RESOLVE_CACHE.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

/// `_REPO_HOST_RESOLVE_CACHE_TTL_SECONDS`.
pub fn repo_host_resolve_cache_ttl_seconds() -> f64 {
    f64::from_bits(REPO_HOST_RESOLVE_CACHE_TTL_BITS.load(Ordering::SeqCst))
}

/// Sets `_REPO_HOST_RESOLVE_CACHE_TTL_SECONDS`.
pub fn set_repo_host_resolve_cache_ttl_seconds(seconds: f64) {
    REPO_HOST_RESOLVE_CACHE_TTL_BITS.store(seconds.to_bits(), Ordering::SeqCst);
}

/// `_REPO_HOST_RESOLVE_CACHE_MAX_ENTRIES`.
pub fn repo_host_resolve_cache_max_entries() -> usize {
    REPO_HOST_RESOLVE_CACHE_MAX.load(Ordering::SeqCst)
}

/// Sets `_REPO_HOST_RESOLVE_CACHE_MAX_ENTRIES`.
pub fn set_repo_host_resolve_cache_max_entries(max: usize) {
    REPO_HOST_RESOLVE_CACHE_MAX.store(max, Ordering::SeqCst);
}

/// `_REPO_HOST_RESOLVE_CACHE.clear()`.
pub fn clear_repo_host_resolve_cache() {
    cache_lock().clear();
}

/// `len(_REPO_HOST_RESOLVE_CACHE)`.
pub fn repo_host_resolve_cache_len() -> usize {
    cache_lock().len()
}

fn prune_locked(cache: &mut IndexMap<String, CacheEntry>, now: f64) {
    cache.retain(|_, entry| entry.expires_at > now);
    let max = repo_host_resolve_cache_max_entries();
    while cache.len() > max {
        cache.shift_remove_index(0);
    }
}

/// `CodeFile`.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeFile {
    /// `path`.
    pub path: String,
    /// `name`.
    pub name: String,
    /// `language`.
    pub language: String,
    /// `content`.
    pub content: String,
    /// `size`.
    pub size: u64,
    /// `lines`.
    pub lines: usize,
    /// `metadata` (`extension`, `size_bytes`, `modified`, `structure`).
    pub metadata: Map<String, Value>,
}

impl CodeFile {
    /// `code_file.__dict__`.
    pub fn to_dict(&self) -> Map<String, Value> {
        kw::obj([
            ("path", json!(self.path)),
            ("name", json!(self.name)),
            ("language", json!(self.language)),
            ("content", json!(self.content)),
            ("size", json!(self.size)),
            ("lines", json!(self.lines)),
            ("metadata", Value::Object(self.metadata.clone())),
        ])
    }
}

/// `CommitInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct CommitInfo {
    /// `hash` (short form).
    pub hash: String,
    /// `message`.
    pub message: String,
    /// `author`.
    pub author: String,
    /// `date`.
    pub date: PyDateTime,
    /// `files_changed`.
    pub files_changed: Vec<String>,
    /// `additions`.
    pub additions: i64,
    /// `deletions`.
    pub deletions: i64,
}

impl CommitInfo {
    /// `commit_info.__dict__` (the date as ISO text).
    pub fn to_dict(&self) -> Map<String, Value> {
        kw::obj([
            ("hash", json!(self.hash)),
            ("message", json!(self.message)),
            ("author", json!(self.author)),
            ("date", json!(self.date.isoformat('T'))),
            ("files_changed", json!(self.files_changed)),
            ("additions", json!(self.additions)),
            ("deletions", json!(self.deletions)),
        ])
    }
}

/// `open(path, "r", encoding="utf-8", errors="ignore").read()`: invalid UTF-8 sequences are
/// dropped and newlines translated (universal newlines).
pub fn read_text_ignore(path: &str) -> Result<String, SemanticaError> {
    let bytes = std::fs::read(path).map_err(|e| SemanticaError::from_io(&e, Some(std::path::Path::new(path))))?;
    let mut out = String::with_capacity(bytes.len());
    let mut rest: &[u8] = &bytes;
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                break;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                out.push_str(std::str::from_utf8(&rest[..valid]).unwrap_or(""));
                match e.error_len() {
                    Some(n) => rest = &rest[valid + n..],
                    None => break,
                }
            }
        }
    }
    Ok(out.replace("\r\n", "\n").replace('\r', "\n"))
}

static PY_CLASS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^class\s+(\w+)").expect("valid"));
static PY_FUNC_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*def\s+(\w+)").expect("valid"));
static JS_CLASS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"class\s+(\w+)").expect("valid"));
static JS_FUNC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"function\s+(\w+)|const\s+(\w+)\s*=\s*(?:async\s+)?\(|(\w+)\s*:\s*(?:async\s+)?\(.*?\)\s*=>")
        .expect("valid")
});
static DOCSTRING_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?s)"""(.*?)""""#).expect("valid"));
static PY_COMMENT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)#\s*(.*)").expect("valid"));
static SLASH_COMMENT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)//\s*(.*)").expect("valid"));
static PY_IMPORT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^(?:from|import)\s+([\w.]+)").expect("valid"));
static JS_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?:import|require)\(?['"]([^'"]+)['"]"#).expect("valid"));
static JAVA_IMPORT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^import\s+([\w.]+)").expect("valid"));

fn findall(re: &Regex, text: &str) -> Vec<Value> {
    re.captures_iter(text)
        .map(|c| json!(c.get(1).map(|m| m.as_str()).unwrap_or("")))
        .collect()
}

/// `CodeExtractor`: code content extraction, language detection, structure, documentation
/// and dependency analysis.
#[derive(Debug, Clone)]
pub struct CodeExtractor {
    /// Configuration.
    pub config: Kwargs,
    logger: Logger,
}

impl Default for CodeExtractor {
    fn default() -> Self {
        CodeExtractor::new(Kwargs::new())
    }
}

impl CodeExtractor {
    /// `LANGUAGE_MAP`: extension -> language.
    pub const LANGUAGE_MAP: [(&'static str, &'static str); 27] = [
        (".py", "python"),
        (".js", "javascript"),
        (".ts", "typescript"),
        (".java", "java"),
        (".cpp", "cpp"),
        (".c", "c"),
        (".cs", "csharp"),
        (".go", "go"),
        (".rs", "rust"),
        (".rb", "ruby"),
        (".php", "php"),
        (".swift", "swift"),
        (".kt", "kotlin"),
        (".scala", "scala"),
        (".r", "r"),
        (".m", "matlab"),
        (".sh", "shell"),
        (".sql", "sql"),
        (".html", "html"),
        (".css", "css"),
        (".scss", "scss"),
        (".json", "json"),
        (".xml", "xml"),
        (".yaml", "yaml"),
        (".yml", "yaml"),
        (".md", "markdown"),
        (".txt", "text"),
    ];

    /// `CodeExtractor(**config)`.
    pub fn new(config: Kwargs) -> CodeExtractor {
        CodeExtractor {
            config,
            logger: get_logger("code_extractor"),
        }
    }

    /// `extract_file_content(file_path, language=None)`.
    pub fn extract_file_content(&self, file_path: &str, language: Option<&str>) -> Result<CodeFile, SemanticaError> {
        let path = pypath::normalize(file_path);
        if !pypath::exists(&path) {
            return Err(validation_error(format!("File not found: {path}")));
        }
        let language = match language {
            Some(l) => l.to_string(),
            None => self._detect_language(&path),
        };
        let content = match read_text_ignore(&path) {
            Ok(c) => c,
            Err(e) => {
                self.logger.warning(format!("Failed to read file {path}: {e}"));
                String::new()
            }
        };
        let meta = std::fs::metadata(&path).map_err(|e| SemanticaError::from_io(&e, Some(std::path::Path::new(&path))))?;
        let lines = content.matches('\n').count() + usize::from(!content.is_empty());
        let mut metadata = Map::new();
        metadata.insert("extension".to_string(), json!(pypath::suffix(&path)));
        metadata.insert("size_bytes".to_string(), json!(meta.len()));
        let modified = pypath::mtime(&path)
            .and_then(|t| PyDateTime::from_timestamp(t, false).ok())
            .map(|d| Value::String(d.isoformat('T')))
            .unwrap_or(Value::Null);
        metadata.insert("modified".to_string(), modified);
        if ["python", "javascript", "typescript", "java"].contains(&language.as_str()) {
            metadata.insert(
                "structure".to_string(),
                Value::Object(self._extract_structure(&content, &language)),
            );
        }
        Ok(CodeFile {
            name: pypath::name(&path),
            path,
            language,
            content,
            size: meta.len(),
            lines,
            metadata,
        })
    }

    /// `extract_documentation(file_path)`: docstrings, comments, or the README text.
    pub fn extract_documentation(&self, file_path: &str) -> Result<Map<String, Value>, SemanticaError> {
        let path = pypath::normalize(file_path);
        let language = self._detect_language(&path);
        let mut docs = kw::obj([
            ("docstrings", json!([])),
            ("comments", json!([])),
            ("readme", Value::Null),
        ]);
        if ["readme.md", "readme.txt", "readme"].contains(&pypath::name(&path).to_lowercase().as_str()) {
            docs.insert("readme".to_string(), json!(read_text_ignore(&path)?));
            return Ok(docs);
        }
        let Ok(content) = read_text_ignore(&path) else {
            return Ok(docs);
        };
        if language == "python" {
            let docstrings: Vec<Value> = DOCSTRING_RE
                .captures_iter(&content)
                .map(|c| json!(strip(c.get(1).map(|m| m.as_str()).unwrap_or(""))))
                .collect();
            docs.insert("docstrings".to_string(), Value::Array(docstrings));
        }
        let pattern: Option<&Regex> = match language.as_str() {
            "python" => Some(&PY_COMMENT_RE),
            "javascript" | "java" | "c" | "cpp" => Some(&SLASH_COMMENT_RE),
            _ => None,
        };
        if let Some(re) = pattern {
            let comments: Vec<Value> = re
                .captures_iter(&content)
                .filter_map(|c| {
                    let s = strip(c.get(1).map(|m| m.as_str()).unwrap_or(""));
                    (!s.is_empty()).then(|| json!(s))
                })
                .collect();
            docs.insert("comments".to_string(), Value::Array(comments));
        }
        Ok(docs)
    }

    /// `analyze_dependencies(file_path)`: imports by language.
    pub fn analyze_dependencies(&self, file_path: &str) -> Map<String, Value> {
        let path = pypath::normalize(file_path);
        let language = self._detect_language(&path);
        let mut deps = kw::obj([
            ("external", json!([])),
            ("internal", json!([])),
            ("language", json!(language)),
        ]);
        let Ok(content) = read_text_ignore(&path) else {
            return deps;
        };
        let found = match language.as_str() {
            "python" => Some(findall(&PY_IMPORT_RE, &content)),
            "javascript" | "typescript" => Some(findall(&JS_IMPORT_RE, &content)),
            "java" => Some(findall(&JAVA_IMPORT_RE, &content)),
            _ => None,
        };
        if let Some(f) = found {
            deps.insert("external".to_string(), Value::Array(f));
        }
        deps
    }

    /// `_detect_language(file_path)`.
    pub fn _detect_language(&self, file_path: &str) -> String {
        let ext = pypath::suffix(file_path).to_lowercase();
        Self::LANGUAGE_MAP
            .iter()
            .find(|(e, _)| *e == ext)
            .map(|(_, l)| l.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// `_extract_structure(content, language)`: classes and functions.
    pub fn _extract_structure(&self, content: &str, language: &str) -> Map<String, Value> {
        let mut structure = kw::obj([
            ("classes", json!([])),
            ("functions", json!([])),
            ("imports", json!([])),
        ]);
        match language {
            "python" => {
                structure.insert("classes".to_string(), Value::Array(findall(&PY_CLASS_RE, content)));
                structure.insert("functions".to_string(), Value::Array(findall(&PY_FUNC_RE, content)));
            }
            "javascript" | "typescript" => {
                structure.insert("classes".to_string(), Value::Array(findall(&JS_CLASS_RE, content)));
                let functions: Vec<Value> = JS_FUNC_RE
                    .captures_iter(content)
                    .filter_map(|c| {
                        let groups: Vec<&str> = (1..=3)
                            .map(|i| c.get(i).map(|m| m.as_str()).unwrap_or(""))
                            .collect();
                        groups
                            .iter()
                            .find(|g| !g.is_empty())
                            .map(|g| json!(g))
                    })
                    .collect();
                structure.insert("functions".to_string(), Value::Array(functions));
            }
            _ => {}
        }
        structure
    }
}

/// `GitAnalyzer`: repository structure, metrics and pattern analysis.
#[derive(Debug, Clone)]
pub struct GitAnalyzer {
    /// Configuration (unused, as in the original).
    pub config: Kwargs,
}

/// Replacement for `GitAnalyzer.analyze_structure` (`patch.object(analyzer, ...)`).
pub static ANALYZE_STRUCTURE: Hook<Box<dyn Fn(&str) -> Map<String, Value>>> =
    Hook::new("semantica.ingest.repo_ingestor.GitAnalyzer.analyze_structure");
/// Replacement for `GitAnalyzer.calculate_metrics`.
pub static CALCULATE_METRICS: Hook<Box<dyn Fn(&str) -> Map<String, Value>>> =
    Hook::new("semantica.ingest.repo_ingestor.GitAnalyzer.calculate_metrics");

fn bump(map: &mut Map<String, Value>, key: &str, by: u64) {
    let current = map.get(key).and_then(Value::as_u64).unwrap_or(0);
    map.insert(key.to_string(), json!(current + by));
}

impl GitAnalyzer {
    /// `GitAnalyzer(**config)`.
    pub fn new(config: Kwargs) -> GitAnalyzer {
        GitAnalyzer { config }
    }

    /// `analyze_structure(repo_path)`: file / directory counts, extensions, depth (`.git`,
    /// `__pycache__` and `node_modules` are not descended into).
    pub fn analyze_structure(&self, repo_path: &str) -> Map<String, Value> {
        if let Some(f) = ANALYZE_STRUCTURE.get() {
            return f(repo_path);
        }
        let mut total_files = 0u64;
        let mut total_dirs = 0u64;
        let mut file_types = Map::new();
        let mut max_depth = 0u64;
        fn walk(
            path: &str,
            depth: u64,
            total_files: &mut u64,
            total_dirs: &mut u64,
            file_types: &mut Map<String, Value>,
            max_depth: &mut u64,
        ) {
            *max_depth = (*max_depth).max(depth);
            for item in pypath::iterdir(path) {
                if pypath::is_file(&item) {
                    *total_files += 1;
                    let ext = pypath::suffix(&item).to_lowercase();
                    let ext = if ext.is_empty() { "no_extension".to_string() } else { ext };
                    bump(file_types, &ext, 1);
                } else if pypath::is_dir(&item)
                    && ![".git", "__pycache__", "node_modules"].contains(&pypath::name(&item).as_str())
                {
                    *total_dirs += 1;
                    walk(&item, depth + 1, total_files, total_dirs, file_types, max_depth);
                }
            }
        }
        walk(
            &pypath::normalize(repo_path),
            0,
            &mut total_files,
            &mut total_dirs,
            &mut file_types,
            &mut max_depth,
        );
        kw::obj([
            ("total_files", json!(total_files)),
            ("total_directories", json!(total_dirs)),
            ("file_types", Value::Object(file_types)),
            ("directory_structure", json!({})),
            ("max_depth", json!(max_depth)),
        ])
    }

    /// `calculate_metrics(repo_path)`: lines and files per language.
    pub fn calculate_metrics(&self, repo_path: &str) -> Map<String, Value> {
        if let Some(f) = CALCULATE_METRICS.get() {
            return f(repo_path);
        }
        let mut total_lines = 0u64;
        let mut total_files = 0u64;
        let mut lines_by_language = Map::new();
        let mut files_by_language = Map::new();
        let extractor = CodeExtractor::default();
        for file_path in pypath::rglob_all(repo_path) {
            let name = pypath::name(&file_path);
            if !pypath::is_file(&file_path) || name == ".gitignore" || name == ".gitkeep" {
                continue;
            }
            if let Ok(code_file) = extractor.extract_file_content(&file_path, None) {
                total_files += 1;
                total_lines += code_file.lines as u64;
                bump(&mut lines_by_language, &code_file.language, code_file.lines as u64);
                bump(&mut files_by_language, &code_file.language, 1);
            }
        }
        kw::obj([
            ("total_lines", json!(total_lines)),
            ("total_files", json!(total_files)),
            ("lines_by_language", Value::Object(lines_by_language)),
            ("files_by_language", Value::Object(files_by_language)),
        ])
    }

    /// `detect_patterns(repo_path)`.
    pub fn detect_patterns(&self, repo_path: &str) -> Map<String, Value> {
        let mut naming = Map::new();
        if pypath::rglob_all(repo_path)
            .iter()
            .any(|p| pypath::fnmatch(&pypath::name(p), "*.py"))
        {
            naming.insert("python".to_string(), json!("detected"));
        }
        kw::obj([
            ("naming_conventions", Value::Object(naming)),
            ("file_organization", json!({})),
            ("common_structures", json!([])),
        ])
    }
}

/// One commit as `git.Commit` exposes it.
#[derive(Debug, Clone, PartialEq)]
pub struct GitCommit {
    /// `hexsha`.
    pub hexsha: String,
    /// `message`.
    pub message: String,
    /// `str(commit.author)` (the author name).
    pub author: String,
    /// `committed_date` (seconds since the epoch).
    pub committed_date: f64,
    /// `list(commit.stats.files.keys())`.
    pub files: Vec<String>,
    /// `commit.stats.total["insertions"]`.
    pub insertions: i64,
    /// `commit.stats.total["deletions"]`.
    pub deletions: i64,
}

/// An opened repository (`git.Repo`).
pub trait GitRepo: Send {
    /// `repo.git.checkout(branch)`.
    fn checkout(&mut self, branch: &str) -> Result<(), SemanticaError>;
    /// `[branch.name for branch in repo.branches]`.
    fn branches(&self) -> Result<Vec<String>, SemanticaError>;
    /// `[tag.name for tag in repo.tags]`.
    fn tags(&self) -> Result<Vec<String>, SemanticaError>;
    /// `repo.remotes` as `(name, url)` pairs.
    fn remotes(&self) -> Result<Vec<(String, String)>, SemanticaError>;
    /// `bool(repo.heads)`.
    fn has_heads(&self) -> Result<bool, SemanticaError>;
    /// `repo.head.commit`.
    fn head_commit(&self) -> Result<GitCommit, SemanticaError>;
    /// `repo.iter_commits(*args)` (GitPython binds the first two positional arguments to
    /// `rev` and `paths`).
    fn iter_commits(&self, args: &[String]) -> Result<Vec<GitCommit>, SemanticaError>;
}

/// The `git.Repo` class: cloning and opening repositories.
pub trait GitApi: Send + Sync {
    /// `git.Repo.clone_from(url, to_path, **options)`.
    fn clone_from(&self, url: &str, to_path: &str, options: &Kwargs) -> Result<Box<dyn GitRepo>, SemanticaError>;
    /// `git.Repo(path)`.
    fn open(&self, path: &str) -> Result<Box<dyn GitRepo>, SemanticaError>;
}

fn git_error(cmdline: &[String], status: Option<i32>, stderr: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "git.exc".to_string(),
        type_name: "GitCommandError".to_string(),
        message: format!(
            "Cmd('git') failed due to: exit code({})\n  cmdline: git {}\n  stderr: '{}'",
            status.unwrap_or(-1),
            cmdline.join(" "),
            stderr.trim()
        ),
    }
}

fn run_git(executable: &str, dir: Option<&str>, args: &[String]) -> Result<String, SemanticaError> {
    let mut cmd = Command::new(executable);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(args).env("GIT_TERMINAL_PROMPT", "0");
    let output = cmd.output().map_err(|e| SemanticaError::Other {
        module: "git.exc".to_string(),
        type_name: "GitCommandNotFound".to_string(),
        message: format!("Cmd('{executable}') not found due to: {e}"),
    })?;
    if !output.status.success() {
        return Err(git_error(args, output.status.code(), &String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn push_value(out: &mut Vec<String>, dashes: &str, flag: &str, value: &str) {
    if dashes == "-" {
        // Short options take their value as a separate argument (`-n 5`).
        out.push(format!("-{flag}"));
        out.push(value.to_string());
    } else {
        out.push(format!("--{flag}={value}"));
    }
}

/// GitPython's `transform_kwargs`: `key=True` -> `--key`, `key=value` -> `--key=value`
/// (underscores become dashes; `False` / `None` are dropped).
pub fn transform_kwargs(options: &Kwargs) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in options {
        let flag = k.replace('_', "-");
        let dashes = if flag.chars().count() == 1 { "-" } else { "--" };
        match v {
            Value::Bool(true) => out.push(format!("{dashes}{flag}")),
            Value::Bool(false) | Value::Null => {}
            Value::Array(items) => {
                for item in items {
                    push_value(&mut out, dashes, &flag, &py_str(item));
                }
            }
            other => push_value(&mut out, dashes, &flag, &py_str(other)),
        }
    }
    out
}

/// The real backend: the `git` command line (as GitPython drives it).
#[derive(Debug, Clone)]
pub struct GitCli {
    /// The executable (`git`).
    pub executable: String,
}

impl Default for GitCli {
    fn default() -> Self {
        GitCli {
            executable: "git".to_string(),
        }
    }
}

/// A repository opened through [`GitCli`].
#[derive(Debug, Clone)]
pub struct GitCliRepo {
    /// The git executable.
    pub executable: String,
    /// The working tree.
    pub path: String,
}

impl GitApi for GitCli {
    fn clone_from(&self, url: &str, to_path: &str, options: &Kwargs) -> Result<Box<dyn GitRepo>, SemanticaError> {
        let mut args = vec!["clone".to_string(), "-v".to_string()];
        args.extend(transform_kwargs(options));
        args.extend(["--".to_string(), url.to_string(), to_path.to_string()]);
        run_git(&self.executable, None, &args)?;
        self.open(to_path)
    }

    fn open(&self, path: &str) -> Result<Box<dyn GitRepo>, SemanticaError> {
        if !pypath::exists(path) {
            return Err(SemanticaError::Other {
                module: "git.exc".to_string(),
                type_name: "NoSuchPathError".to_string(),
                message: path.to_string(),
            });
        }
        run_git(&self.executable, Some(path), &["rev-parse".to_string(), "--git-dir".to_string()]).map_err(|_| {
            SemanticaError::Other {
                module: "git.exc".to_string(),
                type_name: "InvalidGitRepositoryError".to_string(),
                message: path.to_string(),
            }
        })?;
        Ok(Box::new(GitCliRepo {
            executable: self.executable.clone(),
            path: path.to_string(),
        }))
    }
}

const COMMIT_FORMAT: &str = "--format=%x1e%H%x1f%an%x1f%ct%x1f%B%x1f";

impl GitCliRepo {
    fn git(&self, args: &[&str]) -> Result<String, SemanticaError> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        run_git(&self.executable, Some(&self.path), &args)
    }

    fn refs(&self, prefix: &str) -> Result<Vec<String>, SemanticaError> {
        Ok(self
            .git(&["for-each-ref", "--format=%(refname:short)", prefix])?
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn parse_log(text: &str) -> Vec<GitCommit> {
        let mut commits = Vec::new();
        for record in text.split('\u{1e}').filter(|r| !r.trim().is_empty()) {
            let fields: Vec<&str> = record.splitn(5, '\u{1f}').collect();
            if fields.len() < 5 {
                continue;
            }
            let mut files = Vec::new();
            let (mut insertions, mut deletions) = (0i64, 0i64);
            for line in fields[4].lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 3 {
                    insertions += parts[0].parse::<i64>().unwrap_or(0);
                    deletions += parts[1].parse::<i64>().unwrap_or(0);
                    files.push(parts[2].to_string());
                }
            }
            commits.push(GitCommit {
                hexsha: fields[0].trim().to_string(),
                author: fields[1].to_string(),
                committed_date: fields[2].trim().parse::<f64>().unwrap_or(0.0),
                message: fields[3].to_string(),
                files,
                insertions,
                deletions,
            });
        }
        commits
    }
}

impl GitRepo for GitCliRepo {
    fn checkout(&mut self, branch: &str) -> Result<(), SemanticaError> {
        self.git(&["checkout", branch]).map(|_| ())
    }

    fn branches(&self) -> Result<Vec<String>, SemanticaError> {
        self.refs("refs/heads/")
    }

    fn tags(&self) -> Result<Vec<String>, SemanticaError> {
        self.refs("refs/tags/")
    }

    fn remotes(&self) -> Result<Vec<(String, String)>, SemanticaError> {
        let names: Vec<String> = self
            .git(&["remote"])?
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        names
            .into_iter()
            .map(|n| {
                let url = self.git(&["remote", "get-url", &n])?.trim().to_string();
                Ok((n, url))
            })
            .collect()
    }

    fn has_heads(&self) -> Result<bool, SemanticaError> {
        Ok(!self.branches()?.is_empty())
    }

    fn head_commit(&self) -> Result<GitCommit, SemanticaError> {
        let text = self.git(&["log", "-1", "--numstat", "--no-renames", COMMIT_FORMAT, "HEAD"])?;
        Self::parse_log(&text)
            .into_iter()
            .next()
            .ok_or_else(|| SemanticaError::Value("Reference at 'HEAD' does not exist".to_string()))
    }

    fn iter_commits(&self, args: &[String]) -> Result<Vec<GitCommit>, SemanticaError> {
        if args.len() > 2 {
            return Err(SemanticaError::Type(format!(
                "Repo.iter_commits() takes from 1 to 3 positional arguments but {} were given",
                args.len() + 1
            )));
        }
        let mut cmd: Vec<&str> = vec!["log", "--numstat", "--no-renames", "--diff-merges=first-parent", COMMIT_FORMAT];
        match args.first() {
            Some(rev) => cmd.push(rev),
            None => cmd.push("HEAD"),
        }
        cmd.push("--");
        if let Some(paths) = args.get(1).filter(|p| !p.is_empty()) {
            cmd.push(paths);
        }
        Ok(Self::parse_log(&self.git(&cmd)?))
    }
}

/// Replacement for the `git.Repo` class (`patch("semantica.ingest.repo_ingestor.git.Repo")`).
pub static GIT_REPO: Hook<Arc<dyn GitApi>> = Hook::new("semantica.ingest.repo_ingestor.git.Repo");
/// Replacement for `get_progress_tracker` as seen by this module.
pub static GET_PROGRESS_TRACKER: Hook<Box<dyn Fn() -> Arc<ProgressTracker>>> =
    Hook::new("semantica.ingest.repo_ingestor.get_progress_tracker");
/// Replacement for `tempfile.mkdtemp(prefix=...)` as seen by this module.
pub static MKDTEMP: Hook<Box<dyn Fn(&str) -> String>> = Hook::new("semantica.ingest.repo_ingestor.tempfile.mkdtemp");
/// Replacement for `shutil.rmtree` as seen by this module.
pub static RMTREE: Hook<Box<dyn Fn(&str)>> = Hook::new("semantica.ingest.repo_ingestor.shutil.rmtree");
/// Replacement for `RepoIngestor.extract_code_files`.
pub static EXTRACT_CODE_FILES: Hook<Box<dyn Fn(&str, &Kwargs) -> Vec<CodeFile>>> =
    Hook::new("semantica.ingest.repo_ingestor.RepoIngestor.extract_code_files");
/// Replacement for `RepoIngestor.get_repository_info`.
pub static GET_REPOSITORY_INFO: Hook<Box<dyn Fn(&str) -> Map<String, Value>>> =
    Hook::new("semantica.ingest.repo_ingestor.RepoIngestor.get_repository_info");
/// Replacement for `RepoIngestor.analyze_commits`.
pub static ANALYZE_COMMITS: Hook<Box<dyn Fn(&str, &Kwargs) -> Result<Vec<CommitInfo>, SemanticaError>>> =
    Hook::new("semantica.ingest.repo_ingestor.RepoIngestor.analyze_commits");

fn git_api() -> Arc<dyn GitApi> {
    match GIT_REPO.get() {
        Some(api) => (*api).clone(),
        None => Arc::new(GitCli::default()),
    }
}

/// `tempfile.mkdtemp(prefix=prefix)`: a new private directory under the temporary directory
/// (`TMPDIR`), created exclusively with mode `0o700`.
pub fn mkdtemp(prefix: &str) -> Result<String, SemanticaError> {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_";
    let base = std::env::temp_dir();
    for _ in 0..100 {
        let random = uuid::Uuid::new_v4();
        let name: String = random
            .as_bytes()
            .iter()
            .take(8)
            .map(|b| CHARS[*b as usize % CHARS.len()] as char)
            .collect();
        let path = base.join(format!("{prefix}{name}"));
        match std::fs::create_dir(&path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
                }
                return Ok(path.to_string_lossy().into_owned());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(SemanticaError::from_io(&e, Some(&path))),
        }
    }
    Err(SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "FileExistsError".to_string(),
        message: "No usable temporary directory name found".to_string(),
    })
}

/// `RepoIngestor`: Git repository ingestion.
pub struct RepoIngestor {
    /// Merged configuration.
    pub config: Kwargs,
    /// Code extractor.
    pub code_extractor: CodeExtractor,
    /// Repository analyzer.
    pub analyzer: GitAnalyzer,
    /// Progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    /// Temporary clone directory.
    pub temp_dir: Option<String>,
    logger: Logger,
}

impl std::fmt::Debug for RepoIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepoIngestor")
            .field("config", &self.config)
            .field("temp_dir", &self.temp_dir)
            .finish()
    }
}

impl Default for RepoIngestor {
    fn default() -> Self {
        RepoIngestor::new(Kwargs::new())
    }
}

impl RepoIngestor {
    /// `SUPPORTED_PLATFORMS`.
    pub const SUPPORTED_PLATFORMS: [&'static str; 4] = ["github", "gitlab", "bitbucket", "generic"];

    /// `RepoIngestor(config=None, **kwargs)`.
    pub fn new(config: Kwargs) -> RepoIngestor {
        let ingestor = RepoIngestor {
            code_extractor: CodeExtractor::new(config.clone()),
            analyzer: GitAnalyzer::new(config.clone()),
            config,
            progress_tracker: match GET_PROGRESS_TRACKER.get() {
                Some(f) => f(),
                None => sem_utils::get_progress_tracker(),
            },
            temp_dir: None,
            logger: get_logger("repo_ingestor"),
        };
        ingestor.logger.debug("Repo ingestor initialized");
        ingestor
    }

    /// `_is_scp_like_repo_url(repo_url)`: `user@host:path` without a scheme.
    pub fn _is_scp_like_repo_url(repo_url: &str) -> bool {
        let url = strip(repo_url);
        !url.contains("://") && SCP_LIKE_REPO_URL_RE.is_match(url)
    }

    /// `_scp_like_host(repo_url)`.
    pub fn _scp_like_host(repo_url: &str) -> String {
        let url = strip(repo_url);
        let rest = url.split_once('@').map(|(_, r)| r).unwrap_or("");
        rest.split_once(':').map(|(h, _)| h).unwrap_or(rest).to_string()
    }

    /// `_normalize_repo_url(repo_url)`: `git@host:org/repo.git` -> `ssh://git@host/org/repo.git`.
    pub fn _normalize_repo_url(repo_url: &str) -> String {
        let url = strip(repo_url);
        if !Self::_is_scp_like_repo_url(url) {
            return url.to_string();
        }
        let (user_host, path) = url.split_once(':').unwrap_or((url, ""));
        let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
        format!("ssh://{user_host}{path}")
    }

    /// `_is_blocked_ip(ip)`: private, loopback, link-local or unspecified (deliberately not
    /// `is_reserved`, which would block the NAT64 well-known prefix).
    pub fn _is_blocked_ip(ip: IpAddr) -> bool {
        ipaddr::is_private(ip) || ipaddr::is_loopback(ip) || ipaddr::is_link_local(ip) || ipaddr::is_unspecified(ip)
    }

    /// `_resolve_repo_host_ips(host)`: `getaddrinfo` with the shared TTL cache (the lookup
    /// itself runs outside the cache lock).
    pub fn _resolve_repo_host_ips(host: &str) -> Result<Vec<String>, SemanticaError> {
        let cache_key = host.to_lowercase().trim_end_matches('.').to_string();
        {
            let now = monotonic();
            let mut cache = cache_lock();
            prune_locked(&mut cache, now);
            if let Some(entry) = cache.shift_remove(&cache_key) {
                if now < entry.expires_at {
                    let ips = entry.ips.clone();
                    cache.insert(cache_key.clone(), entry);
                    return Ok(ips);
                }
            }
        }
        let addrs = match socket::getaddrinfo(host) {
            Ok(a) => a,
            Err(GaiError::Gaierror(msg)) => {
                return Err(validation_error(format!(
                    "Cannot resolve repository host {}: {msg}",
                    str_repr(host)
                )));
            }
            Err(GaiError::Timeout) => {
                return Err(SemanticaError::Other {
                    module: "builtins".to_string(),
                    type_name: "TimeoutError".to_string(),
                    message: "timed out".to_string(),
                });
            }
            Err(GaiError::Os(msg)) => {
                return Err(SemanticaError::Other {
                    module: "builtins".to_string(),
                    type_name: "OSError".to_string(),
                    message: msg,
                });
            }
        };
        let mut ips: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for addr in addrs {
            if seen.insert(addr.clone()) {
                ips.push(addr);
            }
        }
        if ips.is_empty() {
            return Err(validation_error(format!(
                "Cannot resolve repository host {}: no addresses",
                str_repr(host)
            )));
        }
        let now = monotonic();
        let mut cache = cache_lock();
        cache.shift_remove(&cache_key);
        cache.insert(
            cache_key,
            CacheEntry {
                expires_at: now + repo_host_resolve_cache_ttl_seconds(),
                ips: ips.clone(),
            },
        );
        prune_locked(&mut cache, now);
        Ok(ips)
    }

    /// `_prune_repo_host_resolve_cache(now=None)`: drops expired entries and enforces the
    /// size cap.
    pub fn _prune_repo_host_resolve_cache(now: Option<f64>) {
        let now = now.unwrap_or_else(monotonic);
        prune_locked(&mut cache_lock(), now);
    }

    /// `_validate_repo_host(host)`: rejects localhost names and hosts that are, or resolve
    /// to, blocked addresses (every resolved address is screened).
    pub fn _validate_repo_host(host: &str) -> Result<(), SemanticaError> {
        if host.is_empty() {
            return Err(validation_error("Repository URL must include a host"));
        }
        let lowered = host.to_lowercase();
        let lowered = lowered.trim_end_matches('.');
        if lowered == "localhost" || lowered.ends_with(".localhost") {
            return Err(validation_error(format!("Repository host is not allowed: {host}")));
        }
        match ipaddr::ip_address(host) {
            Ok(ip) => {
                if Self::_is_blocked_ip(ip) {
                    return Err(validation_error(format!(
                        "Repository host resolves to a blocked address: {host}"
                    )));
                }
                Ok(())
            }
            Err(_) => {
                for addr in Self::_resolve_repo_host_ips(host)? {
                    let Ok(resolved) = ipaddr::ip_address(&addr) else {
                        continue;
                    };
                    if Self::_is_blocked_ip(resolved) {
                        return Err(validation_error(format!(
                            "Repository host resolves to a blocked address: {host} -> {addr}"
                        )));
                    }
                }
                Ok(())
            }
        }
    }

    /// `_is_local_repo_path(repo_url)`: absolute or `./` / `../` paths and existing local
    /// paths (no scheme, not scp-like).
    pub fn _is_local_repo_path(repo_url: &str) -> bool {
        let url = strip(repo_url);
        if url.contains("://") || Self::_is_scp_like_repo_url(url) {
            return false;
        }
        if url.starts_with('/') {
            return true;
        }
        if url.starts_with("./") || url.starts_with("../") || url.starts_with(".\\") || url.starts_with("..\\") {
            return true;
        }
        pypath::exists(&pypath::normalize(url))
    }

    /// `_validate_repo_url(repo_url)`.
    pub fn _validate_repo_url(repo_url: &str) -> Result<(), SemanticaError> {
        if strip(repo_url).is_empty() {
            return Err(validation_error("Repository URL must be a non-empty string"));
        }
        if ENV_VAR_TOKEN_RE.is_match(repo_url) {
            return Err(validation_error(
                "Repository URL must not contain environment variable references ($VAR / ${VAR})",
            ));
        }
        let url = strip(repo_url);
        if Self::_is_local_repo_path(url) {
            return Ok(());
        }
        if Self::_is_scp_like_repo_url(url) {
            return Self::_validate_repo_host(&Self::_scp_like_host(url));
        }
        let parsed = pyurl::urlparse(url)
            .map_err(|e| validation_error(format!("Invalid repository URL: {e}")))?;
        let host = parsed.hostname();
        let scheme = parsed.scheme.to_lowercase();
        if !ALLOWED_REPO_URL_SCHEMES.contains(&scheme.as_str()) {
            let mut allowed: Vec<&str> = ALLOWED_REPO_URL_SCHEMES.to_vec();
            allowed.sort();
            let allowed: Vec<String> = allowed.iter().map(|s| str_repr(s)).collect();
            return Err(validation_error(format!(
                "Unsupported repository URL scheme {}. Allowed schemes: [{}]",
                str_repr(&scheme),
                allowed.join(", ")
            )));
        }
        let host = match host {
            Some(h) if !parsed.netloc.is_empty() && !h.is_empty() => h,
            _ => {
                return Err(validation_error(format!(
                    "Repository URL must include a host: {repo_url}"
                )));
            }
        };
        Self::_validate_repo_host(&host)
    }

    /// `_filter_clone_options(options)`: drops processing options and rejects anything
    /// outside [`ALLOWED_CLONE_OPTIONS`].
    pub fn _filter_clone_options(options: &Kwargs) -> Result<Kwargs, SemanticaError> {
        const NON_GIT: [&str; 5] = ["include_history", "file_filters", "commit_filters", "include_extensions", "max_depth"];
        let candidate: Kwargs = options
            .iter()
            .filter(|(k, _)| !NON_GIT.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut unsafe_keys: Vec<&String> = candidate
            .keys()
            .filter(|k| !ALLOWED_CLONE_OPTIONS.contains(&k.as_str()))
            .collect();
        if !unsafe_keys.is_empty() {
            unsafe_keys.sort();
            let mut allowed: Vec<&str> = ALLOWED_CLONE_OPTIONS.to_vec();
            allowed.sort();
            return Err(validation_error(format!(
                "Clone option(s) not permitted: [{}]. Allowed options: [{}]",
                unsafe_keys.iter().map(|k| str_repr(k)).collect::<Vec<_>>().join(", "),
                allowed.iter().map(|k| str_repr(k)).collect::<Vec<_>>().join(", ")
            )));
        }
        Ok(candidate)
    }

    /// `ingest_repository(repo_url, **options)`: validates, clones (allowlisted options
    /// only) and returns `repository_info`, `code_files`, `commits`, `structure`, `metrics`
    /// and `temp_path`.
    pub fn ingest_repository(&mut self, repo_url: &str, options: &Kwargs) -> Result<Map<String, Value>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            Some(repo_url),
            Some("ingest"),
            Some("RepoIngestor"),
            &format!("Repository: {repo_url}"),
            None,
        );
        let result = self.ingest_repository_inner(repo_url, options, &tracking_id);
        if let Err(e) = &result {
            self.progress_tracker
                .update_tracking(&tracking_id, Some("failed"), &e.to_string());
        }
        result
    }

    fn ingest_repository_inner(
        &mut self,
        repo_url: &str,
        options: &Kwargs,
        tracking_id: &str,
    ) -> Result<Map<String, Value>, SemanticaError> {
        Self::_validate_repo_url(repo_url)?;
        let clone_url = Self::_normalize_repo_url(repo_url);
        let mut options = options.clone();
        if options.contains_key("max_depth") && !options.contains_key("depth") {
            let v = options["max_depth"].clone();
            options.insert("depth".to_string(), v);
        }
        let clone_options = Self::_filter_clone_options(&options)?;
        let git = git_api();
        let temp_dir = self._get_temp_dir();
        if let Err(e) = GitApi::clone_from(&*git, &clone_url, &temp_dir, &clone_options) {
            self.progress_tracker
                .update_tracking(tracking_id, Some("failed"), &e.to_string());
            return Err(processing_error(format!("Failed to clone repository: {e}")));
        }
        let repo_path = self.temp_dir.clone().unwrap_or_default();
        let mut repo = git.open(&repo_path)?;
        self.progress_tracker
            .update_tracking(tracking_id, None, "Cloning repository...");
        if let Some(branch) = options.get("branch").filter(|b| kw::truthy(b)) {
            let branch = py_str(branch);
            if let Err(e) = repo.checkout(&branch) {
                self.logger
                    .warning(format!("Failed to checkout branch {branch}: {e}"));
            }
        }
        let repo_info = self.get_repository_info(repo_url, Some(repo.as_ref()));
        let mut file_filters = options
            .get("file_filters")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(exts) = options.get("include_extensions") {
            let normalized: Vec<Value> = exts
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|e| {
                    let e = py_str(e);
                    json!(if e.starts_with('.') { e } else { format!(".{e}") })
                })
                .collect();
            file_filters.insert("extensions".to_string(), Value::Array(normalized));
        }
        self.progress_tracker
            .update_tracking(tracking_id, None, "Extracting code files...");
        let code_files = self.extract_code_files(&repo_path, &file_filters);
        let mut commits = Vec::new();
        if options.get("include_history").map(kw::truthy).unwrap_or(true) {
            self.progress_tracker
                .update_tracking(tracking_id, None, "Analyzing commits...");
            let commit_filters = options
                .get("commit_filters")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            commits = self.analyze_commits(&repo_path, &commit_filters)?;
        }
        self.progress_tracker
            .update_tracking(tracking_id, None, "Analyzing structure...");
        let structure = self.analyzer.analyze_structure(&repo_path);
        let metrics = self.analyzer.calculate_metrics(&repo_path);
        self.progress_tracker.update_tracking(
            tracking_id,
            Some("completed"),
            &format!("Processed {} files, {} commits", code_files.len(), commits.len()),
        );
        Ok(kw::obj([
            ("repository_info", Value::Object(repo_info)),
            (
                "code_files",
                Value::Array(code_files.iter().map(|f| Value::Object(f.to_dict())).collect()),
            ),
            (
                "commits",
                Value::Array(commits.iter().map(|c| Value::Object(c.to_dict())).collect()),
            ),
            ("structure", Value::Object(structure)),
            ("metrics", Value::Object(metrics)),
            ("temp_path", json!(pypath::normalize(&repo_path))),
        ]))
    }

    /// `analyze_commits(repo_path, **filters)` (`since`, `until`, `author`, `max_commits`);
    /// git failures while iterating are logged and end the listing.
    pub fn analyze_commits(&self, repo_path: &str, filters: &Kwargs) -> Result<Vec<CommitInfo>, SemanticaError> {
        if let Some(f) = ANALYZE_COMMITS.get() {
            return f(repo_path, filters);
        }
        let repo = git_api()
            .open(repo_path)
            .map_err(|e| processing_error(format!("Failed to open repository: {e}")))?;
        let mut log_args = Vec::new();
        let truthy = |k: &str| filters.get(k).filter(|v| kw::truthy(v)).map(py_str);
        if let Some(v) = truthy("since") {
            log_args.push(format!("--since={v}"));
        }
        if let Some(v) = truthy("until") {
            log_args.push(format!("--until={v}"));
        }
        if let Some(v) = truthy("author") {
            log_args.push(format!("--author={v}"));
        }
        if let Some(v) = truthy("max_commits") {
            log_args.push(format!("-{v}"));
        }
        let mut commits = Vec::new();
        match repo.iter_commits(&log_args) {
            Ok(list) => {
                for commit in list {
                    commits.push(CommitInfo {
                        hash: commit.hexsha.chars().take(7).collect(),
                        message: strip(&commit.message).to_string(),
                        author: commit.author.clone(),
                        date: PyDateTime::from_timestamp(commit.committed_date, false)?,
                        files_changed: commit.files.clone(),
                        additions: commit.insertions,
                        deletions: commit.deletions,
                    });
                }
            }
            Err(e) => self.logger.error(format!("Error analyzing commits: {e}")),
        }
        Ok(commits)
    }

    /// `extract_code_files(repo_path, **filters)` (`extensions`, `languages`,
    /// `exclude_patterns`, `max_size`).
    pub fn extract_code_files(&self, repo_path: &str, filters: &Kwargs) -> Vec<CodeFile> {
        if let Some(f) = EXTRACT_CODE_FILES.get() {
            return f(repo_path, filters);
        }
        let extensions = kw::get_str_list(filters, "extensions").filter(|e| !e.is_empty());
        let exclude: Vec<fancy_regex::Regex> = kw::get_str_list(filters, "exclude_patterns")
            .unwrap_or_default()
            .iter()
            .filter_map(|p| fancy_regex::Regex::new(&format!(r"\A(?:{p})")).ok())
            .collect();
        let max_size = filters.get("max_size").filter(|v| kw::truthy(v)).and_then(Value::as_f64);
        let languages = kw::get_str_list(filters, "languages").filter(|l| !l.is_empty());
        let mut code_files = Vec::new();
        for file_path in pypath::rglob_all(repo_path) {
            if !pypath::is_file(&file_path) {
                continue;
            }
            let suffix = pypath::suffix(&file_path);
            if let Some(exts) = &extensions
                && !exts.contains(&suffix)
            {
                continue;
            }
            if exclude.iter().any(|re| re.is_match(&file_path).unwrap_or(false)) {
                continue;
            }
            if let Some(max) = max_size
                && std::fs::metadata(&file_path).map(|m| m.len() as f64).unwrap_or(0.0) > max
            {
                continue;
            }
            let name = pypath::name(&file_path);
            if [".gitignore", ".gitkeep", ".gitattributes"].contains(&name.as_str()) {
                continue;
            }
            if [".pyc", ".pyo", ".pyd", ".so", ".dll"].contains(&suffix.as_str()) {
                continue;
            }
            match self.code_extractor.extract_file_content(&file_path, None) {
                Ok(code_file) => {
                    if let Some(langs) = &languages
                        && !langs.contains(&code_file.language)
                    {
                        continue;
                    }
                    code_files.push(code_file);
                }
                Err(e) => self.logger.debug(format!("Failed to extract {file_path}: {e}")),
            }
        }
        code_files
    }

    /// `get_repository_info(repo_url, repo=None)`: branches, tags, origin remote and latest
    /// commit (a failure stops collecting and is logged).
    pub fn get_repository_info(&self, repo_url: &str, repo: Option<&dyn GitRepo>) -> Map<String, Value> {
        if let Some(f) = GET_REPOSITORY_INFO.get() {
            return f(repo_url);
        }
        let mut info = kw::obj([
            ("url", json!(repo_url)),
            ("branches", json!([])),
            ("tags", json!([])),
            ("remote_info", json!({})),
        ]);
        if let Some(repo) = repo {
            let result = (|| -> Result<(), SemanticaError> {
                info.insert("branches".to_string(), json!(repo.branches()?));
                info.insert("tags".to_string(), json!(repo.tags()?));
                let remotes = repo.remotes()?;
                if !remotes.is_empty() {
                    let (name, url) = remotes
                        .iter()
                        .find(|(n, _)| n == "origin")
                        .cloned()
                        .ok_or_else(|| SemanticaError::Attribute("origin".to_string()))?;
                    info.insert("remote_info".to_string(), json!({"url": url, "name": name}));
                }
                if repo.has_heads()? {
                    let latest = repo.head_commit()?;
                    info.insert(
                        "latest_commit".to_string(),
                        json!({
                            "hash": latest.hexsha.chars().take(7).collect::<String>(),
                            "message": strip(&latest.message),
                            "author": latest.author,
                            "date": PyDateTime::from_timestamp(latest.committed_date, false)?.isoformat('T'),
                        }),
                    );
                }
                Ok(())
            })();
            if let Err(e) = result {
                self.logger
                    .warning(format!("Error getting repository info: {e}"));
            }
        }
        info
    }

    /// `_get_temp_dir()`: creates `semantica_repo_*` under the system temp directory once.
    pub fn _get_temp_dir(&mut self) -> String {
        if let Some(d) = &self.temp_dir {
            return d.clone();
        }
        let dir = match MKDTEMP.get() {
            Some(f) => f("semantica_repo_"),
            None => mkdtemp("semantica_repo_").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        };
        self.temp_dir = Some(dir.clone());
        dir
    }

    /// `cleanup()`: removes the clone (read-only entries are made writable first).
    pub fn cleanup(&mut self) {
        let Some(dir) = self.temp_dir.clone() else { return };
        if !pypath::exists(&dir) {
            return;
        }
        match RMTREE.get() {
            Some(f) => f(&dir),
            None => {
                let result = std::fs::remove_dir_all(&dir).or_else(|_| {
                    for entry in pypath::rglob_all(&dir) {
                        if let Ok(meta) = std::fs::metadata(&entry) {
                            let mut perms = meta.permissions();
                            #[allow(clippy::permissions_set_readonly_false)]
                            perms.set_readonly(false);
                            let _ = std::fs::set_permissions(&entry, perms);
                        }
                    }
                    std::fs::remove_dir_all(&dir)
                });
                if let Err(e) = result {
                    self.logger
                        .warning(format!("Failed to clean up temp dir {dir}: {e}"));
                }
            }
        }
        self.temp_dir = None;
    }
}
