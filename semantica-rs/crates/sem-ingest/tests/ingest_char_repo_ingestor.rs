//! Characterisation tests for `semantica/ingest/repo_ingestor.py` (expected values from the
//! original run in the oracle environment; `git.Repo` patched with fakes).

use std::path::Path;
use std::sync::{Arc, Mutex};

use sem_ingest::repo_ingestor::{
    CodeExtractor, CodeFile, CommitInfo, GIT_REPO, GitAnalyzer, GitApi, GitCliRepo, GitCommit, GitRepo,
    RepoIngestor, transform_kwargs,
};
use sem_ingest::support::kw::Kwargs;
use sem_utils::SemanticaError;
use sem_utils::pycompat::time::PyDateTime;
use serde_json::{Map, Value, json};

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

fn sample_repo() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let p = root.path();
    std::fs::write(
        p.join("main.py"),
        b"\"\"\"Module doc.\"\"\"\r\nimport os\r\nfrom pkg.sub import x\r\n\r\nclass Foo:\r\n    \"\"\"Foo doc.\"\"\"\r\n    def bar(self):  # inline note\r\n        pass\r\n# top comment\r\ndef baz():\r\n    return \"caf\xff\"\r\n",
    )
    .unwrap();
    std::fs::write(
        p.join("app.js"),
        "import x from 'lib-a';\nconst y = require(\"lib-b\");\nclass Widget {}\nfunction render() {}\nconst load = async () => {};\nconst obj = { handler: (e) => e };\n// js comment\n",
    )
    .unwrap();
    std::fs::write(p.join("README.md"), "# Title\nSome text\n").unwrap();
    std::fs::write(p.join("notes.txt"), "").unwrap();
    std::fs::create_dir(p.join("sub")).unwrap();
    std::fs::write(
        p.join("sub").join("Util.java"),
        "import java.util.List;\nimport com.acme.Thing;\npublic class Util {}\n",
    )
    .unwrap();
    std::fs::write(p.join(".gitignore"), "*.so\n").unwrap();
    std::fs::write(p.join("lib.so"), b"\x00\x01").unwrap();
    root
}

fn summary(root: &Path, c: &CodeFile) -> Value {
    let mut md = c.metadata.clone();
    md.remove("modified");
    let rel = Path::new(&c.path).strip_prefix(root).unwrap().to_string_lossy().into_owned();
    json!({"path_rel": rel, "name": c.name, "language": c.language, "content": c.content, "size": c.size, "lines": c.lines, "metadata": md})
}

fn p(root: &Path, rel: &str) -> String {
    root.join(rel).to_string_lossy().into_owned()
}

#[test]
fn code_extractor_contents_docs_and_dependencies() {
    let tmp = sample_repo();
    let root = tmp.path();
    let ex = CodeExtractor::default();
    let cf = |rel: &str| summary(root, &ex.extract_file_content(&p(root, rel), None).unwrap());
    assert_eq!(
        cf("main.py"),
        json!({"path_rel": "main.py", "name": "main.py", "language": "python", "content": "\"\"\"Module doc.\"\"\"\nimport os\nfrom pkg.sub import x\n\nclass Foo:\n    \"\"\"Foo doc.\"\"\"\n    def bar(self):  # inline note\n        pass\n# top comment\ndef baz():\n    return \"caf\"\n", "size": 182, "lines": 12, "metadata": {"extension": ".py", "size_bytes": 182, "structure": {"classes": ["Foo"], "functions": ["bar", "baz"], "imports": []}}})
    );
    assert_eq!(
        cf("app.js"),
        json!({"path_rel": "app.js", "name": "app.js", "language": "javascript", "content": "import x from 'lib-a';\nconst y = require(\"lib-b\");\nclass Widget {}\nfunction render() {}\nconst load = async () => {};\nconst obj = { handler: (e) => e };\n// js comment\n", "size": 166, "lines": 8, "metadata": {"extension": ".js", "size_bytes": 166, "structure": {"classes": ["Widget"], "functions": ["render", "load", "handler"], "imports": []}}})
    );
    assert_eq!(
        cf("README.md"),
        json!({"path_rel": "README.md", "name": "README.md", "language": "markdown", "content": "# Title\nSome text\n", "size": 18, "lines": 3, "metadata": {"extension": ".md", "size_bytes": 18}})
    );
    assert_eq!(
        cf("notes.txt"),
        json!({"path_rel": "notes.txt", "name": "notes.txt", "language": "text", "content": "", "size": 0, "lines": 0, "metadata": {"extension": ".txt", "size_bytes": 0}})
    );
    assert_eq!(
        cf("sub/Util.java"),
        json!({"path_rel": "sub/Util.java", "name": "Util.java", "language": "java", "content": "import java.util.List;\nimport com.acme.Thing;\npublic class Util {}\n", "size": 67, "lines": 4, "metadata": {"extension": ".java", "size_bytes": 67, "structure": {"classes": [], "functions": [], "imports": []}}})
    );
    assert_eq!(
        cf("lib.so"),
        json!({"path_rel": "lib.so", "name": "lib.so", "language": "unknown", "content": "\u{0}\u{1}", "size": 2, "lines": 1, "metadata": {"extension": ".so", "size_bytes": 2}})
    );
    assert_eq!(
        summary(root, &ex.extract_file_content(&p(root, "notes.txt"), Some("python")).unwrap()),
        json!({"path_rel": "notes.txt", "name": "notes.txt", "language": "python", "content": "", "size": 0, "lines": 0, "metadata": {"extension": ".txt", "size_bytes": 0, "structure": {"classes": [], "functions": [], "imports": []}}})
    );
    let e = ex.extract_file_content(&p(root, "missing.py"), None).unwrap_err();
    assert!(e.is_validation_error());
    assert_eq!(e.to_string(), format!("File not found: {}", p(root, "missing.py")));

    let docs = |rel: &str| Value::Object(ex.extract_documentation(&p(root, rel)).unwrap());
    assert_eq!(
        docs("main.py"),
        json!({"docstrings": ["Module doc.", "Foo doc."], "comments": ["inline note", "top comment"], "readme": null})
    );
    assert_eq!(docs("app.js"), json!({"docstrings": [], "comments": ["js comment"], "readme": null}));
    assert_eq!(docs("README.md"), json!({"docstrings": [], "comments": [], "readme": "# Title\nSome text\n"}));
    assert_eq!(docs("sub/Util.java"), json!({"docstrings": [], "comments": [], "readme": null}));

    let deps = |rel: &str| Value::Object(ex.analyze_dependencies(&p(root, rel)));
    assert_eq!(deps("main.py"), json!({"external": ["os", "pkg.sub"], "internal": [], "language": "python"}));
    assert_eq!(deps("app.js"), json!({"external": ["lib-b"], "internal": [], "language": "javascript"}));
    assert_eq!(
        deps("sub/Util.java"),
        json!({"external": ["java.util.List", "com.acme.Thing"], "internal": [], "language": "java"})
    );
    assert_eq!(deps("README.md"), json!({"external": [], "internal": [], "language": "markdown"}));
}

#[test]
fn git_analyzer_structure_metrics_patterns() {
    let tmp = sample_repo();
    let root = tmp.path().to_string_lossy().into_owned();
    let an = GitAnalyzer::new(Kwargs::new());
    assert_eq!(
        Value::Object(an.analyze_structure(&root)),
        json!({"total_files": 7, "total_directories": 1, "file_types": {".txt": 1, ".md": 1, ".java": 1, "no_extension": 1, ".so": 1, ".py": 1, ".js": 1}, "directory_structure": {}, "max_depth": 1})
    );
    assert_eq!(
        Value::Object(an.calculate_metrics(&root)),
        json!({"total_lines": 28, "total_files": 6, "lines_by_language": {"text": 0, "markdown": 3, "unknown": 1, "python": 12, "javascript": 8, "java": 4}, "files_by_language": {"text": 1, "markdown": 1, "unknown": 1, "python": 1, "javascript": 1, "java": 1}})
    );
    assert_eq!(
        Value::Object(an.detect_patterns(&root)),
        json!({"naming_conventions": {"python": "detected"}, "file_organization": {}, "common_structures": []})
    );
}

#[test]
fn extract_code_files_filters() {
    let tmp = sample_repo();
    let root = tmp.path();
    let ing = RepoIngestor::new(Kwargs::new());
    let names = |filters: Value| {
        let mut out: Vec<String> = ing
            .extract_code_files(&root.to_string_lossy(), &kwargs(filters))
            .iter()
            .map(|f| Path::new(&f.path).strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    };
    assert_eq!(names(json!({})), vec!["README.md", "app.js", "main.py", "notes.txt", "sub/Util.java"]);
    assert_eq!(names(json!({"extensions": [".py", ".java"]})), vec!["main.py", "sub/Util.java"]);
    assert_eq!(names(json!({"languages": ["javascript", "markdown"]})), vec!["README.md", "app.js"]);
    assert_eq!(
        names(json!({"exclude_patterns": [".*\\.txt$", ".*sub"]})),
        vec!["README.md", "app.js", "main.py"]
    );
    assert_eq!(names(json!({"max_size": 20})), vec!["README.md", "notes.txt"]);
}

/// A repository object with fixed history (the oracle run's `MagicMock` repo).
struct FakeRepo {
    commits: Vec<GitCommit>,
    remotes: Vec<(String, String)>,
    log_args: Arc<Mutex<Vec<Vec<String>>>>,
}

impl GitRepo for FakeRepo {
    fn checkout(&mut self, _branch: &str) -> Result<(), SemanticaError> {
        Ok(())
    }
    fn branches(&self) -> Result<Vec<String>, SemanticaError> {
        Ok(vec!["main".to_string(), "dev".to_string()])
    }
    fn tags(&self) -> Result<Vec<String>, SemanticaError> {
        Ok(vec!["v1.0".to_string()])
    }
    fn remotes(&self) -> Result<Vec<(String, String)>, SemanticaError> {
        Ok(self.remotes.clone())
    }
    fn has_heads(&self) -> Result<bool, SemanticaError> {
        Ok(true)
    }
    fn head_commit(&self) -> Result<GitCommit, SemanticaError> {
        Ok(self.commits[0].clone())
    }
    fn iter_commits(&self, args: &[String]) -> Result<Vec<GitCommit>, SemanticaError> {
        self.log_args.lock().unwrap().push(args.to_vec());
        Ok(self.commits.clone())
    }
}

fn commits() -> Vec<GitCommit> {
    vec![
        GitCommit {
            hexsha: "0123456789abcdef".to_string(),
            message: "  Fix bug\n\nDetails\n".to_string(),
            author: "Ann".to_string(),
            committed_date: 1_700_000_000.0,
            files: vec!["a.py".to_string(), "b.md".to_string()],
            insertions: 5,
            deletions: 2,
        },
        GitCommit {
            hexsha: "fedcba9876543210".to_string(),
            message: "Init".to_string(),
            author: "Bob".to_string(),
            committed_date: 1_600_000_000.0,
            files: vec![],
            insertions: 0,
            deletions: 0,
        },
    ]
}

struct FakeGit {
    log_args: Arc<Mutex<Vec<Vec<String>>>>,
    fail_open: bool,
}

impl GitApi for FakeGit {
    fn clone_from(&self, _url: &str, _to: &str, _options: &Kwargs) -> Result<Box<dyn GitRepo>, SemanticaError> {
        self.open("")
    }
    fn open(&self, _path: &str) -> Result<Box<dyn GitRepo>, SemanticaError> {
        if self.fail_open {
            return Err(SemanticaError::Other {
                module: "builtins".to_string(),
                type_name: "Exception".to_string(),
                message: "bad repo".to_string(),
            });
        }
        Ok(Box::new(FakeRepo {
            commits: commits(),
            remotes: vec![("origin".to_string(), "https://example.com/r.git".to_string())],
            log_args: self.log_args.clone(),
        }))
    }
}

#[test]
fn analyze_commits_and_repository_info() {
    let log_args = Arc::new(Mutex::new(Vec::new()));
    let _git = GIT_REPO.set(Arc::new(FakeGit {
        log_args: log_args.clone(),
        fail_open: false,
    }) as Arc<dyn GitApi>);
    let ing = RepoIngestor::new(Kwargs::new());
    let out: Vec<CommitInfo> = ing
        .analyze_commits("/repo", &kwargs(json!({"since": "2020-01-01", "max_commits": 5})))
        .unwrap();
    let summary: Vec<Value> = out
        .iter()
        .map(|c| json!([c.hash, c.message, c.author, c.files_changed, c.additions, c.deletions]))
        .collect();
    assert_eq!(
        summary,
        vec![
            json!(["0123456", "Fix bug\n\nDetails", "Ann", ["a.py", "b.md"], 5, 2]),
            json!(["fedcba9", "Init", "Bob", [], 0, 0]),
        ]
    );
    // `datetime.fromtimestamp(committed_date)`: local wall time.
    assert_eq!(out[0].date, PyDateTime::from_timestamp(1_700_000_000.0, false).unwrap());
    assert_eq!(*log_args.lock().unwrap(), vec![vec!["--since=2020-01-01".to_string(), "-5".to_string()]]);
    assert_eq!(
        out[0].to_dict().keys().collect::<Vec<_>>(),
        vec!["hash", "message", "author", "date", "files_changed", "additions", "deletions"]
    );

    let info = ing.get_repository_info("https://x/y.git", None);
    assert_eq!(
        Value::Object(info),
        json!({"url": "https://x/y.git", "branches": [], "tags": [], "remote_info": {}})
    );
    let repo = FakeRepo {
        commits: commits(),
        remotes: vec![("origin".to_string(), "https://example.com/r.git".to_string())],
        log_args: log_args.clone(),
    };
    let info = ing.get_repository_info("https://example.com/r.git", Some(&repo));
    let date = PyDateTime::from_timestamp(1_700_000_000.0, false).unwrap().isoformat('T');
    assert_eq!(
        Value::Object(info),
        json!({"url": "https://example.com/r.git", "branches": ["main", "dev"], "tags": ["v1.0"],
               "remote_info": {"url": "https://example.com/r.git", "name": "origin"},
               "latest_commit": {"hash": "0123456", "message": "Fix bug\n\nDetails", "author": "Ann", "date": date}})
    );
    let no_origin = FakeRepo {
        commits: commits(),
        remotes: vec![("upstream".to_string(), "u".to_string())],
        log_args,
    };
    assert_eq!(
        Value::Object(ing.get_repository_info("u", Some(&no_origin))),
        json!({"url": "u", "branches": ["main", "dev"], "tags": ["v1.0"], "remote_info": {}})
    );

    let _failing = GIT_REPO.set(Arc::new(FakeGit {
        log_args: Arc::new(Mutex::new(Vec::new())),
        fail_open: true,
    }) as Arc<dyn GitApi>);
    let e = ing.analyze_commits("/repo", &Kwargs::new()).unwrap_err();
    assert_eq!(e.type_name(), "ProcessingError");
    assert_eq!(e.to_string(), "Failed to open repository: bad repo");
}

#[test]
fn temp_dir_and_cleanup() {
    let mut ing = RepoIngestor::new(Kwargs::new());
    let d = ing._get_temp_dir();
    assert!(Path::new(&d).file_name().unwrap().to_string_lossy().starts_with("semantica_repo_"));
    assert!(Path::new(&d).is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
    }
    ing.cleanup();
    assert!(!Path::new(&d).exists());
    assert_eq!(ing.temp_dir, None);
    let f = CodeFile {
        path: "p".to_string(),
        name: "n".to_string(),
        language: "l".to_string(),
        content: "c".to_string(),
        size: 1,
        lines: 1,
        metadata: Map::new(),
    };
    let mut keys: Vec<String> = f.to_dict().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, vec!["content", "language", "lines", "metadata", "name", "path", "size"]);
}

#[test]
fn git_cli_argument_mapping() {
    // GitPython's `transform_kwargs` for the allowlisted clone options.
    assert_eq!(
        transform_kwargs(&kwargs(json!({"depth": 1, "branch": "main", "single_branch": true, "no_tags": false}))),
        vec!["--depth=1", "--branch=main", "--single-branch"]
    );
    assert_eq!(transform_kwargs(&kwargs(json!({"v": true, "n": 5, "x": null}))), vec!["-v", "-n", "5"]);
    let _ = GitCliRepo {
        executable: "git".to_string(),
        path: ".".to_string(),
    };
}
