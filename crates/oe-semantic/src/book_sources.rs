//! Book text provenance and explicit authoring-byte maintenance.
//! No operation in this module establishes semantic or publication approval.
use anyhow::{Context, Result, bail, ensure};
use oe_core::json::{ordered_json, strict_json};
use oe_core::{FileIdentity, fingerprint_file, no_symlinks, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const VOL1: &str = "ontology-engineering-book";
pub const VOL2: &str = "product-trustworthiness-book";
pub const VOL1_CHAPTERS: [&str; 9] = [
    "ch01-introduction",
    "ch02-ontology-foundations",
    "ch03-ontology-methodology",
    "ch04-ontology-languages",
    "ch05-reasoning",
    "ch06-applications",
    "ch07-knowledge-graph",
    "ch08-ontology-llm",
    "ch09-capstone-manufacturing",
];
pub const VOL2_CHAPTERS: [&str; 20] = [
    "ch01-introduction",
    "ch02-concepts-terminology",
    "ch03-safety-management",
    "ch04-concept-hara",
    "ch05-system-development",
    "ch06-hardware-development",
    "ch07-software-development",
    "ch08-asil-decomposition-dfa",
    "ch09-production-operation",
    "ch10-supporting-processes",
    "ch11-claim-ontology",
    "ch12-identity-ontology",
    "ch13-governance-ontology",
    "ch14-context-hazard-ontology",
    "ch15-requirements-ontology",
    "ch16-measurement-ontology",
    "ch17-change-ontology",
    "ch18-dependency-ontology",
    "ch19-field-ontology",
    "ch20-assurance-ontology",
];
pub const ARCHIVE_WARNING: &str = "PROVENANCE WARNING: --scope archive searches historical or non-authoritative book-adjacent files. Do not cite these hits as current book sources or executable Semantica semantics.";
const TEXT_LIMIT: usize = 16 * 1024 * 1024;
const FILE_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const MAX_SEARCH_BYTES: usize = 256 * 1024 * 1024;
const HEADERS: [&str; 4] = [
    "# SHA-256 lock for the clone-local Vol.1 book and TeX authoring tree.\n# Paths are relative to references/ontology-engineering-book; PDF and this lock are excluded.\n",
    "# SHA-256 lock for the current rewritten book sources and text factory.\n# Paths are relative to references/product-trustworthiness-book.\n# Update deliberately whenever an authoritative Markdown or factory source changes.\n",
    "# SHA-256 lock for binary publication assets in this repository clone.\n# Paths are relative to references/product-trustworthiness-book.\n# Default isolated builds consume these local files; no mother repo is required.\n",
    "# SHA-256 lock for Vol.2 formal-search reader guides.\n# Paths are relative to references/product-trustworthiness-book.\n# Formal search fails closed if any guide drifts from this reviewed set.\n",
];
const BUILD_SUFFIXES: [&str; 22] = [
    ".aux",
    ".bbl",
    ".bcf",
    ".blg",
    ".dvi",
    ".fdb_latexmk",
    ".fls",
    ".idx",
    ".ilg",
    ".ind",
    ".lof",
    ".log",
    ".lot",
    ".nav",
    ".out",
    ".pdf",
    ".run.xml",
    ".snm",
    ".synctex.gz",
    ".toc",
    ".vrb",
    ".xdv",
];

fn relative(value: &str) -> Result<&str> {
    crate::safe_relative(value)?;
    ensure!(value.trim() == value, "book_noncanonical_path");
    Ok(value)
}
fn absolute(path: &Path) -> Result<PathBuf> {
    no_symlinks(path)?;
    let result = fs::canonicalize(path).context("book_root_unavailable")?;
    ensure!(result.is_dir(), "book_root_not_directory");
    Ok(result)
}
fn optional_directory(path: &Path) -> Result<Option<PathBuf>> {
    no_symlinks(path)?;
    if !path.is_dir() {
        return Ok(None);
    }
    Ok(Some(absolute(path)?))
}
fn child(root: &Path, name: &str) -> Result<PathBuf> {
    let path = root.join(relative(name)?);
    no_symlinks(&path)?;
    Ok(path)
}
fn posix(path: &Path) -> Result<String> {
    let raw = path.to_str().context("book_non_utf8_path")?;
    #[cfg(unix)]
    {
        ensure!(!raw.contains('\\'), "book_noncanonical_path");
        Ok(raw.to_owned())
    }
    #[cfg(not(unix))]
    {
        Ok(raw.replace('\\', "/"))
    }
}
fn rel(root: &Path, path: &Path) -> Result<String> {
    posix(path.strip_prefix(root).context("book_path_outside_root")?)
}
fn names<'a>(values: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}
fn safe_error(error: &anyhow::Error) -> String {
    let code = error.to_string();
    if !code.is_empty()
        && code.len() <= 96
        && code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        code
    } else {
        "book_input_or_io_failed".into()
    }
}
fn guides(volume: &str) -> BTreeSet<String> {
    let mut result = if volume == VOL1 {
        names(["README.md", "resources/README.md"])
    } else {
        names(["README.md", "propositions-index.md", "handbook/README.md"])
    };
    let chapters: &[&str] = if volume == VOL1 {
        &VOL1_CHAPTERS
    } else {
        &VOL2_CHAPTERS
    };
    result.extend(chapters.iter().map(|name| format!("{name}/README.md")));
    result
}
fn required(volume: &str) -> BTreeSet<String> {
    if volume == VOL1 {
        let mut result = guides(volume);
        result.extend(names([
            "handbook/README.md",
            "handbook/main.tex",
            "handbook/preamble.tex",
            "handbook/chapters/appB-glossary.tex",
        ]));
        result.extend((1..=9).map(|i| format!("handbook/chapters/ch{i:02}.tex")));
        result
    } else {
        let mut result = names([
            "front-matter/preface.md",
            "appendices/appendix-a-semiconductor.md",
            "appendices/appendix-b-motorcycle-truck.md",
            "appendices/appendix-c-glossary.md",
            "appendices/appendix-d-method-tables.md",
            "handbook/book-metadata.tex",
            "handbook/main.tex",
            "handbook/preamble.tex",
        ]);
        result.extend(
            VOL2_CHAPTERS
                .iter()
                .map(|name| format!("{name}/chapter.md")),
        );
        result
    }
}
fn walk(root: &Path, hidden: bool, skip_search: bool) -> Result<Vec<PathBuf>> {
    no_symlinks(root)?;
    ensure!(root.is_dir(), "book_directory_missing");
    let mut pending = vec![root.to_owned()];
    let mut files = Vec::new();
    let mut count = 0;
    while let Some(dir) = pending.pop() {
        no_symlinks(&dir)?;
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().context("book_non_utf8_path")?;
            if (hidden && name.starts_with('.')) || (skip_search && skip_part(name)) {
                continue;
            }
            count += 1;
            ensure!(count <= MAX_FILES, "book_tree_exceeds_limit");
            let ty = entry.file_type()?;
            ensure!(!ty.is_symlink(), "book_symlink_input");
            if ty.is_dir() {
                pending.push(entry.path());
            } else {
                ensure!(ty.is_file(), "book_special_input");
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

#[derive(Default)]
struct Snapshot(BTreeMap<PathBuf, FileIdentity>);
impl Snapshot {
    fn track(&mut self, path: &Path) -> Result<String> {
        let identity = fingerprint_file(path, FILE_LIMIT)?;
        if let Some(old) = self.0.insert(path.to_owned(), identity.clone()) {
            ensure!(old == identity, "book_source_changed");
        }
        Ok(identity.sha256)
    }
    fn bytes(&mut self, path: &Path) -> Result<Vec<u8>> {
        let raw = read_regular_bytes(path, TEXT_LIMIT)?;
        let identity = FileIdentity {
            bytes: raw.len() as u64,
            sha256: sha256(&raw),
        };
        if let Some(old) = self.0.insert(path.to_owned(), identity.clone()) {
            ensure!(old == identity, "book_source_changed");
        }
        Ok(raw)
    }
    fn verify(&self) -> Result<()> {
        for (path, identity) in &self.0 {
            ensure!(
                fingerprint_file(path, FILE_LIMIT)? == *identity,
                "book_source_changed"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub scope: String,
    pub limit: usize,
    pub context: usize,
    /// Explicit external PDE root (or its containing workspace), not a book root.
    pub external_root: Option<PathBuf>,
    /// Explicit legacy CAD case source; read-only text, never application authority.
    pub cad_root: Option<PathBuf>,
}
impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            scope: "book".into(),
            limit: 12,
            context: 2,
            external_root: None,
            cad_root: None,
        }
    }
}
fn skip_part(name: &str) -> bool {
    [
        ".git",
        ".venv",
        "__pycache__",
        "codex_outputs",
        "dist",
        "figures-deck",
        "node_modules",
        "runs",
    ]
    .contains(&name)
}
fn include(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    [
        "java", "md", "owl", "py", "rdf", "rq", "shacl", "sparql", "swrl", "tex", "ttl", "txt",
    ]
    .contains(&ext)
        && !path
            .components()
            .any(|c| c.as_os_str().to_str().is_some_and(skip_part))
        && !path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("/handbook/fragments/")
}
fn formal_content(volume: &str, guide: bool, value: &str) -> bool {
    if guide {
        return guides(volume).contains(value);
    }
    if volume == VOL1 {
        guides(volume).contains(value)
            || [
                "handbook/README.md",
                "handbook/main.tex",
                "handbook/preamble.tex",
            ]
            .contains(&value)
            || value
                .strip_prefix("handbook/chapters/")
                .is_some_and(|p| !p.contains('/') && p.ends_with(".tex"))
    } else {
        value == "front-matter/preface.md"
            || value.starts_with("appendices/")
            || [
                "handbook/book-metadata.tex",
                "handbook/main.tex",
                "handbook/preamble.tex",
            ]
            .contains(&value)
            || value.split_once('/').is_some_and(|(head, tail)| {
                tail == "chapter.md"
                    && head.len() > 5
                    && head.starts_with("ch")
                    && head.as_bytes()[2..4].iter().all(u8::is_ascii_digit)
                    && head.as_bytes()[4] == b'-'
            })
    }
}
fn formal_files(root: &Path, snapshot: &mut Snapshot) -> Result<BTreeMap<PathBuf, String>> {
    let mut selected = BTreeMap::new();
    for volume in [VOL1, VOL2] {
        let book = child(root, &format!("references/{volume}"))?;
        ensure!(book.is_dir(), "book_corpus_incomplete");
        if volume == VOL1 {
            ensure!(
                !book
                    .join("handbook/authoring-sources.sha256")
                    .try_exists()?,
                "book_retired_lock_present"
            );
        }
        let specs = if volume == VOL1 {
            vec![("authoring-sources.sha256", false)]
        } else {
            vec![
                ("handbook/current-source.sha256", false),
                ("handbook/formal-search-guides.sha256", true),
            ]
        };
        for (lock, guide) in specs {
            let raw = snapshot.bytes(&child(&book, lock)?)?;
            let text = std::str::from_utf8(&raw).context("book_lock_not_utf8")?;
            let mut entries = BTreeSet::new();
            for line in text.lines() {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (hash, path) = line.split_once("  ").context("book_lock_malformed")?;
                ensure!(
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "book_lock_malformed"
                );
                relative(path)?;
                ensure!(entries.insert(path.to_owned()), "book_lock_duplicate");
                let input = child(&book, path)?;
                // Even excluded authoring tools must exist as regular inputs.
                drop(oe_core::open_regular_file(&input, FILE_LIMIT)?);
                if include(&input) && formal_content(volume, guide, path) {
                    ensure!(
                        selected.insert(input, hash.to_owned()).is_none(),
                        "book_multiple_locks"
                    );
                }
            }
            ensure!(!entries.is_empty(), "book_lock_empty");
            let expected = if guide {
                guides(volume)
            } else {
                required(volume)
            };
            ensure!(
                expected.is_subset(&entries) && (!guide || entries == expected),
                "book_lock_coverage_mismatch"
            );
        }
    }
    Ok(selected)
}
fn tokenize(query: &str) -> Vec<String> {
    let q = query.trim().to_lowercase();
    let mut terms = BTreeSet::new();
    let mut token = String::new();
    for ch in q.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || "_+-.".contains(ch) {
            token.push(ch);
        } else if !token.is_empty() {
            terms.insert(std::mem::take(&mut token));
        }
    }
    let cjk: Vec<char> = q
        .chars()
        .filter(|c| ('\u{3400}'..='\u{9fff}').contains(c))
        .collect();
    if !cjk.is_empty() {
        terms.insert(cjk.iter().collect());
        if cjk.len() <= 4 {
            terms.extend(cjk.iter().map(char::to_string));
        }
        for n in 2..=4 {
            for chars in cjk.windows(n) {
                terms.insert(chars.iter().collect());
            }
        }
    }
    let mut terms: Vec<_> = terms.into_iter().collect();
    terms.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(a.cmp(b)));
    terms
}
fn score(text: &str, query: &str, terms: &[String]) -> usize {
    let text = text.to_lowercase();
    usize::from(!query.is_empty() && text.contains(query)) * (25 + query.chars().count())
        + terms
            .iter()
            .map(|t| text.matches(t.as_str()).count() * (4 + t.chars().count().min(12)))
            .sum::<usize>()
}
fn utf8_ignore(raw: &[u8]) -> String {
    let mut left = raw;
    let mut out = String::new();
    loop {
        match std::str::from_utf8(left) {
            Ok(s) => {
                out.push_str(s);
                break;
            }
            Err(error) => {
                out.push_str(
                    std::str::from_utf8(&left[..error.valid_up_to()])
                        .expect("validated UTF-8 prefix"),
                );
                match error.error_len() {
                    Some(n) => left = &left[error.valid_up_to() + n..],
                    None => break,
                }
            }
        }
    }
    out
}

/// The legacy JSON hit array. All formal selected files are checked before a hit
/// limit is applied; zero matches cannot hide a modified source or lock.
pub fn search(root: &Path, query: &str, options: &SearchOptions) -> Result<Value> {
    let root = absolute(root)?;
    ensure!(
        ["all", "book", "archive", "pde", "cad"].contains(&options.scope.as_str()),
        "book_unknown_scope"
    );
    ensure!(
        options.limit <= 10_000 && options.context <= 1_000 && query.len() <= 64 * 1024,
        "book_search_budget_exceeded"
    );
    let mut snapshot = Snapshot::default();
    let references = root.join("references");
    let mut files = BTreeMap::<PathBuf, Option<String>>::new();
    if ["all", "book", "archive"].contains(&options.scope.as_str()) {
        let formal = formal_files(&root, &mut snapshot)?;
        if options.scope == "archive" {
            for volume in [VOL1, VOL2] {
                for path in walk(&references.join(volume), false, true)? {
                    if include(&path) && !formal.contains_key(&path) {
                        files.insert(path, None);
                    }
                }
            }
        } else {
            files.extend(formal.into_iter().map(|(p, h)| (p, Some(h))));
        }
    }
    let mut workspace = references.clone();
    if ["all", "pde"].contains(&options.scope.as_str()) {
        let candidate = options
            .external_root
            .as_ref()
            .map(|p| optional_directory(p))
            .transpose()?
            .flatten();
        let pde = candidate.and_then(|p| {
            if p.file_name().is_some_and(|s| s == "cauchyx-ai") {
                Some(p)
            } else if p.join("cauchyx-ai").is_dir() {
                Some(p.join("cauchyx-ai"))
            } else {
                None
            }
        });
        if let Some(pde) = pde {
            workspace = pde.parent().context("book_pde_parent_missing")?.to_owned();
            for path in walk(&pde, false, true)? {
                if include(&path) {
                    ensure!(
                        !files.get(&path).is_some_and(Option::is_some),
                        "book_conflicting_provenance"
                    );
                    files.insert(path, None);
                }
            }
        } else {
            ensure!(options.scope != "pde", "book_pde_root_required");
        }
    }
    let cad = if ["all", "cad"].contains(&options.scope.as_str()) {
        options
            .cad_root
            .as_ref()
            .map(|p| optional_directory(p))
            .transpose()?
            .flatten()
            .filter(|p| p.join("pyproject.toml").is_file() && p.join("src/cad_agent").is_dir())
    } else {
        None
    };
    if ["all", "cad"].contains(&options.scope.as_str())
        && let Some(cad) = &cad
    {
        for path in walk(cad, false, true)? {
            if include(&path) {
                ensure!(
                    !files.get(&path).is_some_and(Option::is_some),
                    "book_conflicting_provenance"
                );
                files.insert(path, None);
            }
        }
    }
    ensure!(files.len() <= MAX_FILES, "book_search_budget_exceeded");
    let full = query.trim().to_lowercase();
    let terms = tokenize(query);
    let mut hits = Vec::<(usize, PathBuf, usize, Value)>::new();
    let mut total = 0usize;
    for (path, expected) in files {
        let raw = snapshot.bytes(&path)?;
        total = total
            .checked_add(raw.len())
            .context("book_search_budget_exceeded")?;
        ensure!(total <= MAX_SEARCH_BYTES, "book_search_budget_exceeded");
        if let Some(hash) = &expected {
            ensure!(sha256(&raw) == *hash, "book_formal_source_changed");
        }
        let text = utf8_ignore(&raw);
        let lines: Vec<_> = text.lines().collect();
        let display = if let Ok(p) = path.strip_prefix(&references) {
            posix(p)?
        } else if let Ok(p) = path.strip_prefix(&workspace) {
            posix(p)?
        } else if let Some(cad) = &cad
            && let Ok(p) = path.strip_prefix(cad)
        {
            format!("cad-agent/{}", posix(p)?)
        } else {
            posix(&path)?
        };
        let raw_path = posix(&path)?;
        let path_score = 2
            * (score(&raw_path, &full, &terms)
                + score(&raw_path.replace(['_', '-'], " "), &full, &terms));
        let best = lines
            .iter()
            .take(200)
            .enumerate()
            .max_by(|(i, a), (j, b)| {
                score(a, &full, &terms)
                    .cmp(&score(b, &full, &terms))
                    .then(j.cmp(i))
            })
            .map_or(0, |(i, _)| i);
        let mut selected = Vec::new();
        if path_score > 0 {
            selected.push((best, path_score, format!("[path] {display}")));
        }
        for (i, line) in lines.iter().enumerate() {
            let s = score(line, &full, &terms);
            if s > 0 {
                selected.push((i, s, (*line).to_owned()));
            }
        }
        for (i, s, line) in selected {
            let context: Vec<_> = lines
                .iter()
                .enumerate()
                .skip(i.saturating_sub(options.context))
                .take(
                    i.saturating_add(options.context)
                        .saturating_add(1)
                        .min(lines.len())
                        .saturating_sub(i.saturating_sub(options.context)),
                )
                .map(|(n, l)| format!("{}: {l}", n + 1))
                .collect();
            let mut hit =
                json!({"score":s,"path":display,"line":i+1,"text":line.trim(),"context":context});
            if let Some(hash) = &expected {
                hit["source_sha256"] = hash.clone().into();
            }
            if options.scope == "archive" {
                hit["source_status"] = "archive_non_authoritative".into();
                hit["provenance_warning"] = ARCHIVE_WARNING.into();
            }
            hits.push((s, path.clone(), i, hit));
        }
    }
    snapshot.verify()?;
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut locations = BTreeMap::<PathBuf, Vec<usize>>::new();
    let mut selected = Vec::new();
    for (_, path, line, hit) in hits {
        if selected.len() == options.limit {
            break;
        }
        let previous = locations.entry(path).or_default();
        if previous.len() == 4 || previous.iter().any(|n| n.abs_diff(line) <= 2) {
            continue;
        }
        previous.push(line);
        selected.push(hit);
    }
    Ok(Value::Array(selected))
}

struct Change {
    target: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}
impl Change {
    fn capture(target: PathBuf, after: Vec<u8>) -> Result<Self> {
        no_symlinks(&target)?;
        let before = match fs::symlink_metadata(&target) {
            Ok(_) => Some(read_regular_bytes(&target, TEXT_LIMIT)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e).context("book_target_unavailable"),
        };
        Ok(Self {
            target,
            before,
            after,
        })
    }
    fn changed(&self) -> bool {
        self.before.as_deref() != Some(&self.after)
    }
    fn verify(&self) -> Result<()> {
        no_symlinks(&self.target)?;
        match &self.before {
            Some(raw) => ensure!(
                read_regular_bytes(&self.target, TEXT_LIMIT)? == *raw,
                "book_target_changed"
            ),
            None => ensure!(!self.target.try_exists()?, "book_target_changed"),
        }
        Ok(())
    }
}

#[cfg(unix)]
fn directory(path: &Path) -> Result<fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    ensure!(fd >= 0, "book_directory_open_failed");
    let mut file = unsafe { fs::File::from_raw_fd(fd) };
    for part in path.components() {
        let name = match part {
            std::path::Component::RootDir => continue,
            std::path::Component::Normal(n) => std::ffi::CString::new(n.as_bytes())?,
            _ => bail!("book_noncanonical_directory"),
        };
        let fd = unsafe {
            libc::openat(
                file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        ensure!(fd >= 0, "book_directory_open_failed");
        file = unsafe { fs::File::from_raw_fd(fd) };
    }
    Ok(file)
}

/// Stages and before-images are retained under the book root's var directory.
/// Each file replacement is atomic; the batch is deliberately not called a
/// transaction. A crash can leave a prefix published. The append-only plan,
/// before/after bytes and per-file receipts make that prefix recoverable.
fn publish(root: &Path, changes: &[Change], snapshot: &Snapshot) -> Result<Value> {
    publish_with(root, changes, snapshot, |_| Ok(()))
}
fn publish_with(
    root: &Path,
    changes: &[Change],
    snapshot: &Snapshot,
    checkpoint: impl Fn(usize) -> Result<()>,
) -> Result<Value> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let stage = root
        .join("var/maintenance")
        .join(format!("book-sources-{stamp}"));
    oe_core::create_new_private_dir(&stage)?;
    let changed: Vec<_> = changes.iter().filter(|c| c.changed()).collect();
    let mut entries = Vec::new();
    for (i, change) in changed.iter().enumerate() {
        if let Some(before) = &change.before {
            oe_core::write_new_bytes(&stage.join(format!("{i:03}.before")), before)?;
        }
        oe_core::write_new_bytes(&stage.join(format!("{i:03}.after")), &change.after)?;
        // Keep a separate expendable publication copy; .after survives success.
        oe_core::write_new_bytes(&stage.join(format!("{i:03}.publish")), &change.after)?;
        entries.push(json!({"ordinal":i,"target":posix(&change.target)?,"before_sha256":change.before.as_ref().map(sha256),"after_sha256":sha256(&change.after),"before_present":change.before.is_some()}));
    }
    oe_core::write_new(
        &stage.join("plan.json"),
        &json!({"schema":"ontology-engineering.book-source-write/v1","operation":"explicit_authoring_byte_refresh","atomic_batch":false,"entries":entries}),
    )?;
    let attempt = (|| -> Result<usize> {
        snapshot.verify()?;
        for change in changes {
            change.verify()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let device = fs::metadata(&stage)?.dev();
            for change in &changed {
                ensure!(
                    directory(
                        change
                            .target
                            .parent()
                            .context("book_target_parent_missing")?
                    )?
                    .metadata()?
                    .dev()
                        == device,
                    "book_atomic_cross_device_unsupported"
                );
            }
        }
        for (i, change) in changed.iter().enumerate() {
            checkpoint(i)?;
            change.verify()?;
            let source = stage.join(format!("{i:03}.publish"));
            #[cfg(unix)]
            {
                use std::os::fd::AsRawFd;
                use std::os::unix::ffi::OsStrExt;
                let from = directory(&stage)?;
                let to = directory(
                    change
                        .target
                        .parent()
                        .context("book_target_parent_missing")?,
                )?;
                let old_name = std::ffi::CString::new(
                    source
                        .file_name()
                        .context("book_target_name_missing")?
                        .as_bytes(),
                )?;
                let new_name = std::ffi::CString::new(
                    change
                        .target
                        .file_name()
                        .context("book_target_name_missing")?
                        .as_bytes(),
                )?;
                let status = unsafe {
                    libc::renameat(
                        from.as_raw_fd(),
                        old_name.as_ptr(),
                        to.as_raw_fd(),
                        new_name.as_ptr(),
                    )
                };
                ensure!(status == 0, "book_atomic_publish_failed");
                to.sync_all().context("book_target_sync_failed")?;
            }
            #[cfg(not(unix))]
            {
                no_symlinks(&change.target)?;
                fs::rename(&source, &change.target).context("book_atomic_publish_failed")?;
            }
            ensure!(
                read_regular_bytes(&change.target, TEXT_LIMIT)? == change.after,
                "book_written_target_changed"
            );
            oe_core::write_new(&stage.join(format!("{i:03}.published.json")), &entries[i])?;
        }
        // Sources include manifests being intentionally replaced during rebind.
        // All other inputs remain byte-bound, and all new targets are rechecked.
        for (path, identity) in &snapshot.0 {
            if !changed.iter().any(|c| c.target == *path) {
                ensure!(
                    fingerprint_file(path, FILE_LIMIT)? == *identity,
                    "book_source_changed"
                );
            }
        }
        for change in &changed {
            ensure!(
                read_regular_bytes(&change.target, TEXT_LIMIT)? == change.after,
                "book_written_target_changed"
            );
        }
        Ok(changed.len())
    })();
    let report = match attempt {
        Ok(count) => {
            json!({"status":"completed","published_count":count,"atomic_batch":false,"recovery":rel(root,&stage)?})
        }
        Err(error) => {
            json!({"status":"failed","error_code":safe_error(&error),"atomic_batch":false,"recovery":rel(root,&stage)?,"obligation":"Inspect plan, before/after bytes, actual targets and published receipts; do not assume rollback or retry blindly."})
        }
    };
    oe_core::write_new(&stage.join("result.json"), &report)?;
    Ok(report)
}

fn authoring_paths(book: &Path, volume: &str, kind: &str) -> Result<BTreeSet<String>> {
    let mut files = BTreeSet::new();
    match (volume, kind) {
        (VOL1, "sources") => {
            ensure!(
                !book
                    .join("handbook/authoring-sources.sha256")
                    .try_exists()?,
                "book_retired_lock_present"
            );
            files.extend(guides(VOL1));
            for entry in fs::read_dir(child(book, "handbook")?)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_str().context("book_non_utf8_path")?;
                if name.starts_with('.') {
                    continue;
                }
                let ty = entry.file_type()?;
                ensure!(!ty.is_symlink(), "book_symlink_input");
                if ty.is_file() && !BUILD_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
                    files.insert(format!("handbook/{name}"));
                } else {
                    ensure!(ty.is_file() || ty.is_dir(), "book_special_input");
                }
            }
            for directory in ["chapters", "figures", "fragments"] {
                for path in walk(&child(book, &format!("handbook/{directory}"))?, true, false)? {
                    files.insert(rel(book, &path)?);
                }
            }
        }
        (VOL2, "sources") => {
            files.extend(names([
                "front-matter/preface.md",
                "handbook/book-metadata.tex",
                "handbook/build_handbook.py",
                "handbook/build_isolated.py",
                "handbook/main.tex",
                "handbook/preamble.tex",
                "handbook/test_build_handbook.py",
            ]));
            files.extend(VOL2_CHAPTERS.iter().map(|c| format!("{c}/chapter.md")));
            let mut appendices = 0;
            for entry in fs::read_dir(child(book, "appendices")?)? {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_str().context("book_non_utf8_path")?;
                if name.starts_with("appendix-") && name.ends_with(".md") {
                    ensure!(entry.file_type()?.is_file(), "book_special_input");
                    files.insert(format!("appendices/{name}"));
                    appendices += 1;
                }
            }
            ensure!(appendices == 4, "book_appendix_coverage_mismatch");
        }
        (VOL2, "guides") => files.extend(guides(VOL2)),
        (VOL2, "assets") => {
            for directory in ["handbook/figures-imagegen", "handbook/figures-rendered"] {
                for path in walk(&child(book, directory)?, false, false)? {
                    files.insert(rel(book, &path)?);
                }
            }
        }
        _ => bail!("book_unknown_lock_kind"),
    }
    for path in &files {
        relative(path)?;
    }
    Ok(files)
}

pub fn check_authoring_locks(root: &Path, write: bool) -> Result<Value> {
    let root = absolute(root)?;
    let mut snapshot = Snapshot::default();
    let mut changes = Vec::new();
    let mut locks = Vec::new();
    for (i, (label, volume, kind, target)) in [
        ("vol1_sources", VOL1, "sources", "authoring-sources.sha256"),
        (
            "vol2_sources",
            VOL2,
            "sources",
            "handbook/current-source.sha256",
        ),
        (
            "vol2_assets",
            VOL2,
            "assets",
            "handbook/authoring-assets.sha256",
        ),
        (
            "vol2_search_guides",
            VOL2,
            "guides",
            "handbook/formal-search-guides.sha256",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let book = child(&root, &format!("references/{volume}"))?;
        let mut expected = HEADERS[i].to_owned();
        for path in authoring_paths(&book, volume, kind)? {
            expected.push_str(&format!(
                "{}  {path}\n",
                snapshot.track(&child(&book, &path)?)?
            ));
        }
        let change = Change::capture(child(&book, target)?, expected.into_bytes())?;
        locks.push(json!({"lock":label,"path":rel(&root,&change.target)?,"matched":!change.changed(),"updated":false}));
        changes.push(change);
    }
    snapshot.verify()?;
    let recovery = if write && changes.iter().any(Change::changed) {
        Some(publish(&root, &changes, &snapshot)?)
    } else {
        None
    };
    for (item, change) in locks.iter_mut().zip(&changes) {
        if write && recovery.as_ref().is_none_or(|r| r["status"] == "completed") {
            item["matched"] = true.into();
            item["updated"] = change.changed().into();
        }
    }
    let mut report = json!({"schema_version":"1.0","mode":if write {"write"} else {"check"},"passed":locks.iter().all(|l| l["matched"] == true),"locks":locks});
    if let Some(recovery) = recovery {
        report["write_recovery"] = recovery;
    }
    Ok(report)
}

// Python's insertion-ordered indent=2 writer, using the common JSON number
// formatter rather than serde's differently spelled finite floating values.
fn encoded(value: &Value) -> Result<Vec<u8>> {
    let input = ordered_json(value)?;
    let mut result = Vec::new();
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for (i, b) in input.iter().copied().enumerate() {
        if quoted {
            result.push(b);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
            continue;
        }
        match b {
            b'"' => {
                quoted = true;
                result.push(b);
            }
            b'{' | b'[' => {
                result.push(b);
                depth += 1;
                if input.get(i + 1) != Some(&(if b == b'{' { b'}' } else { b']' })) {
                    result.push(b'\n');
                    result.extend(std::iter::repeat_n(b' ', depth * 2));
                }
            }
            b'}' | b']' => {
                depth -= 1;
                if i > 0 && input[i - 1] != (if b == b'}' { b'{' } else { b'[' }) {
                    result.push(b'\n');
                    result.extend(std::iter::repeat_n(b' ', depth * 2));
                }
                result.push(b);
            }
            b',' => {
                result.extend_from_slice(b",\n");
                result.extend(std::iter::repeat_n(b' ', depth * 2));
            }
            b':' => result.extend_from_slice(b": "),
            _ => result.push(b),
        }
    }
    result.push(b'\n');
    Ok(result)
}
fn object(value: &mut Value) -> Result<&mut serde_json::Map<String, Value>> {
    value.as_object_mut().context("book_expected_object")
}
fn subobject<'a>(value: &'a mut Value, key: &str) -> Result<&'a mut Value> {
    let map = object(value)?;
    let value = map.entry(key).or_insert_with(|| json!({}));
    ensure!(value.is_object(), "book_expected_object");
    Ok(value)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    oe_core::text(&value[key], "book_required_text")
}
fn read_json(snapshot: &mut Snapshot, path: &Path) -> Result<Value> {
    let value = strict_json(&snapshot.bytes(path)?)?;
    ensure!(value.is_object(), "book_expected_object");
    Ok(value)
}
fn anchor(book: &Path, anchor: &str) -> Result<PathBuf> {
    child(
        book,
        anchor
            .strip_prefix("ontology-engineering/")
            .unwrap_or(anchor),
    )
}
struct Binding {
    source: String,
    source_hash: String,
    guide: String,
    guide_hash: String,
    tex: String,
    tex_hash: String,
    old: BTreeSet<String>,
}
impl Binding {
    fn apply(&self, target: &mut Value, book_source: bool) -> Result<()> {
        let map = object(target)?;
        for (name, value) in [
            (
                if book_source {
                    "logical_anchor"
                } else {
                    "source_anchor"
                },
                &self.source,
            ),
            (
                if book_source {
                    "sha256"
                } else {
                    "source_sha256"
                },
                &self.source_hash,
            ),
            ("guide_anchor", &self.guide),
            ("guide_sha256", &self.guide_hash),
            ("tex_anchor", &self.tex),
            ("tex_sha256", &self.tex_hash),
        ] {
            map.insert(name.into(), value.clone().into());
        }
        Ok(())
    }
}
fn source_binding(
    book: &Path,
    volume: &str,
    chapter: &str,
    manifest: &Value,
    snapshot: &mut Snapshot,
) -> Result<Binding> {
    let (source, guide, tex, old) = if volume == "vol1" {
        let index: usize = chapter
            .strip_prefix("ch")
            .context("book_bad_chapter")?
            .parse()?;
        ensure!((1..=9).contains(&index), "book_bad_chapter");
        let source = format!("references/{VOL1}/handbook/chapters/{chapter}.tex");
        (
            source.clone(),
            format!("references/{VOL1}/{}/README.md", VOL1_CHAPTERS[index - 1]),
            source,
            manifest["chapter"]["external_specification"]["source_anchor"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
        )
    } else {
        let source = text(&manifest["book_source"], "logical_anchor")?.to_owned();
        relative(&source)?;
        let guide = posix(
            &Path::new(&source)
                .parent()
                .context("book_anchor_parent_missing")?
                .join("README.md"),
        )?;
        (
            source.clone(),
            guide,
            format!(
                "ontology-engineering/references/{VOL2}/handbook/fragments/readme-{chapter}.tex"
            ),
            source,
        )
    };
    let source_hash = snapshot.track(&anchor(book, &source)?)?;
    let guide_hash = snapshot.track(&anchor(book, &guide)?)?;
    let tex_hash = snapshot.track(&anchor(book, &tex)?)?;
    let old = [source.clone(), old]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    Ok(Binding {
        source,
        source_hash,
        guide,
        guide_hash,
        tex,
        tex_hash,
        old,
    })
}

/// Refreshes authoring metadata in an explicitly selected Semantica checkout.
/// The source lock, installed distribution and runtime registry are untouched.
pub fn rebind(book_root: &Path, semantica_root: &Path, write: bool) -> Result<Value> {
    let book = absolute(book_root)?;
    let checkout = absolute(semantica_root)?;
    ensure!(
        !checkout.starts_with(book.join("runtime")),
        "book_rebind_requires_source_checkout"
    );
    ensure!(
        !checkout
            .components()
            .any(|p| p.as_os_str() == "site-packages" || p.as_os_str() == "dist-packages"),
        "book_rebind_requires_source_checkout"
    );
    for entry in fs::read_dir(&checkout)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .context("book_non_utf8_path")?
            .to_ascii_lowercase();
        ensure!(
            !(name.starts_with("semantica-") && name.ends_with(".dist-info")),
            "book_rebind_requires_source_checkout"
        );
    }
    let packages = child(&checkout, "semantica/chapter_packages")?;
    let mut snapshot = Snapshot::default();
    let registry = read_json(&mut snapshot, &child(&packages, "registry.yaml")?)?;
    let records = registry["packages"]
        .as_array()
        .context("book_registry_packages_missing")?;
    ensure!(records.len() == 29, "book_registry_coverage_mismatch");
    let mut coordinates = BTreeSet::new();
    for record in records {
        let volume = text(record, "volume")?;
        let chapter = text(record, "chapter")?;
        let valid = match volume {
            "vol1" => (1..=9).any(|i| chapter == format!("ch{i:02}")),
            "vol2" => (1..=20).any(|i| chapter == format!("ch{i:02}")),
            _ => false,
        };
        ensure!(
            valid && coordinates.insert((volume.to_owned(), chapter.to_owned())),
            "book_registry_coverage_mismatch"
        );
        ensure!(
            text(record, "package_id")? == format!("semantica.chapter_packages.{volume}.{chapter}"),
            "book_registry_identity_mismatch"
        );
        ensure!(
            text(record, "manifest")? == format!("{volume}/{chapter}/manifest.yaml"),
            "book_registry_manifest_mismatch"
        );
    }
    let mut changes = BTreeMap::<PathBuf, Change>::new();
    let mut errors = Vec::new();
    let mut contract_hashes = BTreeMap::new();
    for record in records {
        let volume = text(record, "volume")?;
        let chapter = text(record, "chapter")?;
        let result = (|| -> Result<()> {
            let path = child(&packages, text(record, "manifest")?)?;
            let mut manifest = read_json(&mut snapshot, &path)?;
            let binding = source_binding(&book, volume, chapter, &manifest, &mut snapshot)?;
            let assets = manifest["assets"]
                .as_array()
                .context("book_assets_missing")?;
            let contracts: Vec<_> = assets
                .iter()
                .enumerate()
                .filter(|(_, a)| a["role"] == "chapter_contract")
                .collect();
            ensure!(contracts.len() == 1, "book_contract_asset_ambiguous");
            let (index, contract_asset) = contracts[0];
            let contract_id = text(contract_asset, "asset_id")?.to_owned();
            let contract_path = child(
                path.parent().context("book_package_parent_missing")?,
                text(contract_asset, "path")?,
            )?;
            ensure!(contract_path != path, "book_contract_manifest_collision");
            let mut contract = read_json(&mut snapshot, &contract_path)?;
            binding.apply(subobject(&mut contract, "external_specification")?, false)?;
            if volume == "vol1" {
                let chapter = subobject(&mut manifest, "chapter")?;
                binding.apply(subobject(chapter, "external_specification")?, false)?;
            } else {
                binding.apply(subobject(&mut contract, "book_source")?, true)?;
                contract["source_anchor"] = binding.source.clone().into();
                binding.apply(subobject(&mut manifest, "book_source")?, true)?;
            }
            let assets = manifest["assets"]
                .as_array_mut()
                .context("book_assets_missing")?;
            let mut ids = BTreeSet::new();
            let mut paths = BTreeSet::new();
            for asset in assets.iter_mut() {
                ensure!(
                    ids.insert(text(asset, "asset_id")?.to_owned())
                        && paths.insert(relative(text(asset, "path")?)?.to_owned()),
                    "book_asset_identity_duplicate"
                );
                if asset["source_anchor"]
                    .as_str()
                    .is_some_and(|s| binding.old.contains(s))
                {
                    asset["source_anchor"] = binding.source.clone().into();
                    asset["source_sha256"] = binding.source_hash.clone().into();
                }
            }
            let contract_bytes = encoded(&contract)?;
            let hash = sha256(&contract_bytes);
            assets[index]["sha256"] = hash.clone().into();
            contract_hashes.insert((text(record, "package_id")?.to_owned(), contract_id), hash);
            let manifest_bytes = encoded(&manifest)?;
            for (target, raw) in [(contract_path, contract_bytes), (path, manifest_bytes)] {
                ensure!(
                    !changes.contains_key(&target),
                    "book_write_target_duplicate"
                );
                changes.insert(target.clone(), Change::capture(target, raw)?);
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(format!("{volume}.{chapter}: {}", safe_error(&error)));
        }
    }
    let migration_path = child(&packages, "vol2/migration-map.json")?;
    let mut migration = read_json(&mut snapshot, &migration_path)?;
    if let Some(mappings) = migration["mappings"].as_array_mut() {
        for item in mappings {
            if let (Some(package), Some(asset)) =
                (item["new_package"].as_str(), item["new_asset_id"].as_str())
                && let Some(hash) = contract_hashes.get(&(package.into(), asset.into()))
            {
                item["new_sha256"] = hash.clone().into();
            }
        }
        changes.insert(
            migration_path.clone(),
            Change::capture(migration_path, encoded(&migration)?)?,
        );
    } else {
        errors.push("vol2: book_migration_mappings_missing".into());
    }
    snapshot.verify()?;
    let changes: Vec<_> = changes.into_values().collect();
    let changed: Vec<_> = changes
        .iter()
        .filter(|c| c.changed())
        .map(|c| rel(&checkout, &c.target))
        .collect::<Result<_>>()?;
    let recovery = if write && errors.is_empty() && !changed.is_empty() {
        Some(publish(&book, &changes, &snapshot)?)
    } else {
        None
    };
    if recovery
        .as_ref()
        .is_some_and(|r| r["status"] != "completed")
    {
        errors.push("book_refresh_incomplete".into());
    }
    let mut report = json!({"changed_count":changed.len(),"changed_files":changed,"error_count":errors.len(),"errors":errors,"mode":if write {"write"} else {"check"},"package_count":records.len(),"passed":errors.is_empty() && (write || changed.is_empty()),"schema_version":"1.0"});
    if let Some(recovery) = recovery {
        report["write_recovery"] = recovery;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let path = root.join(format!(
            "var/maintenance/rust-tests/book-publish-{name}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        path
    }

    #[test]
    fn interrupted_multifile_refresh_retains_actual_prefix_and_full_recovery() {
        let root = fixture("interruption");
        fs::write(root.join("a.txt"), b"old-a").unwrap();
        fs::write(root.join("b.txt"), b"old-b").unwrap();
        let changes = vec![
            Change::capture(root.join("a.txt"), b"new-a".to_vec()).unwrap(),
            Change::capture(root.join("b.txt"), b"new-b".to_vec()).unwrap(),
        ];
        let report = publish_with(&root, &changes, &Snapshot::default(), |n| {
            ensure!(n != 1, "injected_interruption");
            Ok(())
        })
        .unwrap();
        assert_eq!(report["status"], "failed");
        assert_eq!(report["atomic_batch"], false);
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"new-a");
        assert_eq!(fs::read(root.join("b.txt")).unwrap(), b"old-b");
        let recovery = root.join(report["recovery"].as_str().unwrap());
        assert_eq!(fs::read(recovery.join("000.before")).unwrap(), b"old-a");
        assert_eq!(fs::read(recovery.join("001.before")).unwrap(), b"old-b");
        assert_eq!(fs::read(recovery.join("001.after")).unwrap(), b"new-b");
        assert!(recovery.join("000.published.json").is_file());
        assert!(!recovery.join("001.published.json").exists());
        assert!(recovery.join("plan.json").is_file());
        assert_eq!(
            strict_json(&fs::read(recovery.join("result.json")).unwrap()).unwrap(),
            report
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preflight_changed_source_publishes_nothing_and_retains_recovery() {
        let root = fixture("source-drift");
        fs::write(root.join("source.txt"), b"original").unwrap();
        fs::write(root.join("target.txt"), b"old").unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.track(&root.join("source.txt")).unwrap();
        let changes = vec![Change::capture(root.join("target.txt"), b"new".to_vec()).unwrap()];
        fs::write(root.join("source.txt"), b"modified").unwrap();
        let report = publish(&root, &changes, &snapshot).unwrap();
        assert_eq!(report["status"], "failed");
        assert_eq!(fs::read(root.join("target.txt")).unwrap(), b"old");
        let recovery = root.join(report["recovery"].as_str().unwrap());
        assert!(recovery.join("000.before").is_file());
        assert!(!recovery.join("000.published.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ordered_authoring_json_preserves_python_numeric_and_unicode_bytes() {
        let value = strict_json(
            br#"{"z":1e-7,"a":[0.0,-0.0,184467440737095516160],"empty":{},"quoted":"a,\\\"b"}"#,
        )
        .unwrap();
        let bytes = encoded(&value).unwrap();
        assert_eq!(bytes,b"{\n  \"z\": 1e-07,\n  \"a\": [\n    0.0,\n    -0.0,\n    184467440737095516160\n  ],\n  \"empty\": {},\n  \"quoted\": \"a,\\\\\\\"b\"\n}\n");
        assert_eq!(strict_json(&bytes).unwrap(), value);
    }
}
