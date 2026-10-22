//! Source code parsing (`semantica/parse/code_parser.py`).
//!
//! Parses source files into their structure (functions, classes, imports), comments and
//! dependencies. Python sources go through a port of CPython 3.11's `ast` module
//! ([`crate::pyast`]): the original walks `ast.parse(code)` with `ast.walk` and unparses
//! decorators and base classes with `ast.unparse`; when the source is not valid Python it
//! falls back to the same per-line regular expressions used for the other languages.
//!
//! The regular expressions are the original's, written for Rust's `regex` crate with
//! Python's `\s`/`\S` character sets (`str.isspace()`) and `\w` as `[\p{L}\p{N}_]`. Line
//! iteration follows `str.splitlines()`.
//!
//! As in the original, `SyntaxTreeParser::extract_elements` raises `IndexError("no such
//! group")` for a language without its own patterns (`go`, `rust`, `ruby`, ...) as soon as a
//! line matches the generic `import|require|include`, `function|def` or `class|struct`
//! pattern, because the original reads `match.group(1)` of a pattern without groups.

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use sem_utils::SemanticaError;
use sem_utils::exceptions::{ProcessingError, ValidationError};
use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};

use crate::common::{
    DecodeErrors, Options, PResult, merge_config, py_exists, py_name, py_path_str, py_read_text,
    py_suffix, sub_config,
};
use crate::pyast::{self, ParseFailure, StmtKind};

/// `CodeStructure`: functions, classes and imports found in a source file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CodeStructure {
    /// One dict per function (`name`, `line_number`, and from the AST `args` and
    /// `decorators`).
    pub functions: Vec<Map<String, Value>>,
    /// One dict per class (`name`, `line_number`, and from the AST `bases`).
    pub classes: Vec<Map<String, Value>>,
    /// Imported names.
    pub imports: Vec<String>,
    /// Always empty (the original never fills it).
    pub variables: Vec<String>,
    /// Always empty (the original never fills it).
    pub constants: Vec<String>,
}

/// `CodeComment`: one comment found in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeComment {
    /// The comment text, stripped.
    pub text: String,
    /// The 1-based line number.
    pub line_number: i64,
    /// `"inline"`, `"block"` or `"docstring"`.
    #[serde(rename = "type")]
    pub r#type: String,
    /// The language the comment was extracted for.
    pub language: String,
}

/// The syntax tree [`SyntaxTreeParser::parse_syntax_tree`] returns (the original returns an
/// `ast.Module`).
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxTree {
    module: pyast::Module,
}

impl SyntaxTree {
    /// `ast.parse(code_content)`: `Ok(None)` for a `SyntaxError`.
    fn parse(code_content: &str) -> PResult<Option<SyntaxTree>> {
        match pyast::parse_module(code_content) {
            Ok(module) => Ok(Some(SyntaxTree { module })),
            Err(ParseFailure::Syntax(_)) => Ok(None),
            Err(other) => Err(parse_failure_error(other)),
        }
    }
}

/// The Python exception a non-`SyntaxError` parse failure raises.
fn parse_failure_error(failure: ParseFailure) -> SemanticaError {
    match failure {
        ParseFailure::Syntax(e) => SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: e.class.to_string(),
            message: e.to_string(),
        },
        ParseFailure::Recursion => SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "RecursionError".to_string(),
            message: "maximum recursion depth exceeded during ast construction".to_string(),
        },
        ParseFailure::Memory => SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "MemoryError".to_string(),
            message: String::new(),
        },
    }
}

// --------------------------------------------------------------- Python string helpers

/// `str.isspace()`.
fn is_py_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\x0b'
            | '\x0c'
            | '\r'
            | '\x1c'..='\x1f'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// `str.strip()`.
fn py_strip(s: &str) -> &str {
    s.trim_matches(is_py_space)
}

/// `str.splitlines()`.
fn py_splitlines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let boundary = matches!(
            c,
            '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if boundary {
            lines.push(&s[start..i]);
            let mut next = i + c.len_utf8();
            if c == '\r' && chars.peek().map(|(_, n)| *n) == Some('\n') {
                chars.next();
                next += 1;
            }
            start = next;
        }
    }
    if start < s.len() {
        lines.push(&s[start..]);
    }
    lines
}

/// Python's `\s` for `str` patterns.
const PS: &str = r"[\t\n\x0B\x0C\r\x1C-\x1F \x{85}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}]";
/// Python's `\S` for `str` patterns.
const PNS: &str = r"[^\t\n\x0B\x0C\r\x1C-\x1F \x{85}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}]";
/// Python's `\w` for `str` patterns.
const PW: &str = r"[\p{L}\p{N}_]";

/// Compile a pattern written with Python's `\s`, `\S` and `\w` classes.
fn py_regex(pattern: &str, flags: &str) -> Regex {
    let translated = pattern
        .replace(r"\s", PS)
        .replace(r"\S", PNS)
        .replace(r"\w", PW);
    Regex::new(&format!("{flags}{translated}")).expect("valid pattern")
}

/// `match.group(i)`: `IndexError` when the pattern has no such group.
fn group<'h>(re: &Regex, caps: &Captures<'h>, i: usize) -> PResult<Option<&'h str>> {
    if i >= re.captures_len() {
        return Err(SemanticaError::Index("no such group".to_string()));
    }
    Ok(caps.get(i).map(|m| m.as_str()))
}

/// `match.group(1) or match.group(2)`: the first non-empty group (evaluating `group(2)`
/// only when `group(1)` is empty or missing).
fn first_group(re: &Regex, caps: &Captures<'_>) -> PResult<Option<String>> {
    if let Some(g) = group(re, caps, 1)?.filter(|g| !g.is_empty()) {
        return Ok(Some(g.to_string()));
    }
    Ok(group(re, caps, 2)?
        .filter(|g| !g.is_empty())
        .map(str::to_string))
}

/// The per-language regular expressions of `_extract_with_regex`.
struct RegexPatterns {
    import: Regex,
    function: Regex,
    class: Regex,
}

fn language_patterns(language: &str) -> &'static RegexPatterns {
    static PYTHON: Lazy<RegexPatterns> = Lazy::new(|| RegexPatterns {
        import: py_regex(r"^import\s+(\S+)|^from\s+(\S+)\s+import", "(?i)"),
        function: py_regex(r"^def\s+(\w+)", "(?i)"),
        class: py_regex(r"^class\s+(\w+)", "(?i)"),
    });
    static JAVASCRIPT: Lazy<RegexPatterns> = Lazy::new(|| RegexPatterns {
        import: py_regex(
            r#"^import\s+.*?from\s+['"](\S+)['"]|^require\(['"](\S+)['"]"#,
            "(?i)",
        ),
        function: py_regex(r"^function\s+(\w+)|^(\w+)\s*=\s*function", "(?i)"),
        class: py_regex(r"^class\s+(\w+)", "(?i)"),
    });
    static JAVA: Lazy<RegexPatterns> = Lazy::new(|| RegexPatterns {
        import: py_regex(r"^import\s+(\S+);", "(?i)"),
        function: py_regex(
            r"^\s*(?:public|private|protected)?\s*\w+\s+(\w+)\s*\(",
            "(?i)",
        ),
        class: py_regex(r"^public\s+class\s+(\w+)", "(?i)"),
    });
    static C: Lazy<RegexPatterns> = Lazy::new(|| RegexPatterns {
        import: py_regex(r#"^#include\s+[<"](\S+)[>"]"#, "(?i)"),
        function: py_regex(r"^\w+\s+(\w+)\s*\(", "(?i)"),
        class: py_regex(r"^struct\s+(\w+)|^class\s+(\w+)", "(?i)"),
    });
    static DEFAULT: Lazy<RegexPatterns> = Lazy::new(|| RegexPatterns {
        import: py_regex(r"import|require|include", "(?i)"),
        function: py_regex(r"function|def", "(?i)"),
        class: py_regex(r"class|struct", "(?i)"),
    });
    match language {
        "python" => &PYTHON,
        "javascript" => &JAVASCRIPT,
        "java" => &JAVA,
        "c" => &C,
        _ => &DEFAULT,
    }
}

// ------------------------------------------------------------------------- CodeParser

/// `CodeParser(config=None, **kwargs)`: source code parsing handler.
#[derive(Debug, Clone)]
pub struct CodeParser {
    /// `self.logger` (`semantica.code_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.syntax_parser` (built from `config["syntax"]`).
    pub syntax_parser: SyntaxTreeParser,
    /// `self.comment_extractor` (built from `config["comments"]`).
    pub comment_extractor: CommentExtractor,
    /// `self.dependency_analyzer` (built from `config["dependencies"]`).
    pub dependency_analyzer: DependencyAnalyzer,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
    /// `self.supported_languages`: file extension -> language.
    pub supported_languages: IndexMap<String, String>,
}

impl Default for CodeParser {
    fn default() -> Self {
        CodeParser::new(None, Options::new())
    }
}

impl CodeParser {
    /// `CodeParser(config=None, **kwargs)`. A non-mapping `syntax`/`comments`/
    /// `dependencies` entry counts as empty (Python would raise `TypeError`).
    pub fn new(config: Option<Options>, kwargs: Options) -> Self {
        let config = merge_config(config, kwargs);
        let supported_languages = [
            (".py", "python"),
            (".js", "javascript"),
            (".java", "java"),
            (".cpp", "cpp"),
            (".c", "c"),
            (".cs", "csharp"),
            (".rb", "ruby"),
            (".go", "go"),
            (".rs", "rust"),
            (".php", "php"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        CodeParser {
            logger: get_logger("code_parser"),
            syntax_parser: SyntaxTreeParser::new(sub_config(&config, "syntax")),
            comment_extractor: CommentExtractor::new(sub_config(&config, "comments")),
            dependency_analyzer: DependencyAnalyzer::new(sub_config(&config, "dependencies")),
            progress_tracker: get_progress_tracker(),
            supported_languages,
            config,
        }
    }

    /// `parse_code(file_path, language=None, **options)`: parse a source file into
    /// `{"file_path", "language", "structure", "comments", "dependencies", "line_count",
    /// "size"}`. Every failure (including a missing file) is reported as
    /// `ProcessingError("Failed to parse code: ...")`.
    pub fn parse_code(
        &self,
        file_path: impl AsRef<Path>,
        language: Option<&str>,
        _options: &Options,
    ) -> PResult<Map<String, Value>> {
        let file_path = file_path.as_ref();
        let path_text = py_path_str(file_path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&path_text),
            Some("parse"),
            Some("CodeParser"),
            &format!("Code: {}", py_name(file_path)),
            None,
        );
        let result = (|| -> PResult<Map<String, Value>> {
            if !py_exists(file_path)? {
                return Err(ValidationError::new(format!("Code file not found: {path_text}")).into());
            }
            let language = match language {
                Some(l) => l.to_string(),
                None => self.detect_language(file_path),
            };
            if !self.supported_languages.values().any(|l| *l == language) {
                self.logger
                    .warning(format!("Language {language} may not be fully supported"));
            }
            let inner = (|| -> PResult<Map<String, Value>> {
                let code_content = py_read_text(file_path, "utf-8", DecodeErrors::Ignore)?;
                self.progress_tracker.update_tracking(
                    &tracking_id,
                    None,
                    &format!("Analyzing {language} code..."),
                );
                let structure = self.extract_structure(&code_content, &language)?;
                let comments = self.extract_comments(&code_content, &language);
                let dependencies = self.analyze_dependencies(&code_content, &language)?;
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!(
                        "Parsed {language} code: {} functions, {} classes",
                        structure.functions.len(),
                        structure.classes.len()
                    ),
                    None,
                );
                let mut out = Map::new();
                out.insert("file_path".to_string(), Value::String(path_text.clone()));
                out.insert("language".to_string(), Value::String(language.clone()));
                out.insert(
                    "structure".to_string(),
                    serde_json::to_value(&structure).unwrap_or(Value::Null),
                );
                out.insert(
                    "comments".to_string(),
                    serde_json::to_value(&comments).unwrap_or(Value::Null),
                );
                out.insert("dependencies".to_string(), Value::Object(dependencies));
                out.insert(
                    "line_count".to_string(),
                    json!(py_splitlines(&code_content).len()),
                );
                out.insert("size".to_string(), json!(code_content.chars().count()));
                Ok(out)
            })();
            if let Err(e) = &inner {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
            }
            inner
        })();
        result.map_err(|e| {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
            self.logger
                .error(format!("Failed to parse code {path_text}: {e}"));
            ProcessingError::new(format!("Failed to parse code: {e}")).into()
        })
    }

    /// `extract_structure(code_content, language)`: [`SyntaxTreeParser::extract_elements`].
    pub fn extract_structure(&self, code_content: &str, language: &str) -> PResult<CodeStructure> {
        self.syntax_parser.extract_elements(code_content, language)
    }

    /// `extract_comments(code_content, language)`: [`CommentExtractor::extract_comments`].
    pub fn extract_comments(&self, code_content: &str, language: &str) -> Vec<CodeComment> {
        self.comment_extractor.extract_comments(code_content, language)
    }

    /// `analyze_dependencies(code_content, language)`:
    /// [`DependencyAnalyzer::analyze_dependencies`].
    pub fn analyze_dependencies(
        &self,
        code_content: &str,
        language: &str,
    ) -> PResult<Map<String, Value>> {
        self.dependency_analyzer
            .analyze_dependencies(code_content, language)
    }

    /// `_detect_language`: the language for the file extension, else `"unknown"`.
    fn detect_language(&self, file_path: &Path) -> String {
        let suffix = py_suffix(file_path).to_lowercase();
        self.supported_languages
            .get(&suffix)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }
}

// ------------------------------------------------------------------- SyntaxTreeParser

/// `SyntaxTreeParser(**config)`: syntax tree parsing engine.
#[derive(Debug, Clone)]
pub struct SyntaxTreeParser {
    /// `self.logger` (`semantica.syntax_tree_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
}

impl Default for SyntaxTreeParser {
    fn default() -> Self {
        SyntaxTreeParser::new(Options::new())
    }
}

impl SyntaxTreeParser {
    /// `SyntaxTreeParser(**config)`.
    pub fn new(config: Options) -> Self {
        SyntaxTreeParser {
            logger: get_logger("syntax_tree_parser"),
            config,
        }
    }

    /// `parse_syntax_tree(code_content, language)`: the syntax tree of Python code, `None`
    /// for other languages and for invalid Python (logged as a warning). Python's other
    /// parser exceptions (`RecursionError`, `MemoryError` for absurdly nested code)
    /// propagate.
    pub fn parse_syntax_tree(&self, code_content: &str, language: &str) -> PResult<Option<SyntaxTree>> {
        if language != "python" {
            return Ok(None);
        }
        match pyast::parse_module(code_content) {
            Ok(module) => Ok(Some(SyntaxTree { module })),
            Err(ParseFailure::Syntax(e)) => {
                self.logger
                    .warning(format!("Syntax error in Python code: {e}"));
                Ok(None)
            }
            Err(other) => Err(parse_failure_error(other)),
        }
    }

    /// `extract_elements(code_content, language)`: functions, classes and imports. Python
    /// code is read from its syntax tree in `ast.walk` order (falling back to the regular
    /// expressions for invalid Python); other languages use the regular expressions.
    pub fn extract_elements(&self, code_content: &str, language: &str) -> PResult<CodeStructure> {
        if language != "python" {
            return self.extract_with_regex(code_content, language);
        }
        let Some(tree) = SyntaxTree::parse(code_content)? else {
            return self.extract_with_regex(code_content, language);
        };
        let mut structure = CodeStructure::default();
        for node in tree.module.walk() {
            match &node.kind {
                StmtKind::FunctionDef {
                    name,
                    args,
                    decorators,
                    is_async: false,
                    ..
                } => {
                    let mut decos = Vec::new();
                    for d in decorators {
                        match d {
                            Ok(text) => decos.push(Value::String(text.clone())),
                            Err(msg) => return Err(SemanticaError::Value(msg.clone())),
                        }
                    }
                    let mut f = Map::new();
                    f.insert("name".to_string(), Value::String(name.clone()));
                    f.insert("line_number".to_string(), json!(node.lineno));
                    f.insert("args".to_string(), json!(args));
                    f.insert("decorators".to_string(), Value::Array(decos));
                    structure.functions.push(f);
                }
                StmtKind::ClassDef { name, bases, .. } => {
                    let mut bs = Vec::new();
                    for b in bases {
                        match b {
                            Ok(text) => bs.push(Value::String(text.clone())),
                            Err(msg) => return Err(SemanticaError::Value(msg.clone())),
                        }
                    }
                    let mut c = Map::new();
                    c.insert("name".to_string(), Value::String(name.clone()));
                    c.insert("line_number".to_string(), json!(node.lineno));
                    c.insert("bases".to_string(), Value::Array(bs));
                    structure.classes.push(c);
                }
                StmtKind::Import { names } => structure.imports.extend(names.iter().cloned()),
                StmtKind::ImportFrom { module, names } => {
                    let module = module.as_deref().unwrap_or("");
                    for n in names {
                        structure.imports.push(format!("{module}.{n}"));
                    }
                }
                _ => {}
            }
        }
        Ok(structure)
    }

    /// `_extract_with_regex`.
    fn extract_with_regex(&self, code_content: &str, language: &str) -> PResult<CodeStructure> {
        let patterns = language_patterns(language);
        let lines = py_splitlines(code_content);
        let mut structure = CodeStructure::default();
        for line in &lines {
            if let Some(caps) = patterns.import.captures(line) {
                let name = match first_group(&patterns.import, &caps)? {
                    Some(g) => g,
                    None => caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default(),
                };
                structure.imports.push(name);
            }
        }
        for (i, line) in lines.iter().enumerate() {
            if let Some(caps) = patterns.function.captures(line) {
                let name = first_group(&patterns.function, &caps)?
                    .unwrap_or_else(|| format!("function_{}", i + 1));
                let mut f = Map::new();
                f.insert("name".to_string(), Value::String(name));
                f.insert("line_number".to_string(), json!(i + 1));
                structure.functions.push(f);
            }
        }
        for (i, line) in lines.iter().enumerate() {
            if let Some(caps) = patterns.class.captures(line) {
                let name = first_group(&patterns.class, &caps)?
                    .unwrap_or_else(|| format!("class_{}", i + 1));
                let mut c = Map::new();
                c.insert("name".to_string(), Value::String(name));
                c.insert("line_number".to_string(), json!(i + 1));
                structure.classes.push(c);
            }
        }
        Ok(structure)
    }

    /// `analyze_structure(syntax_tree)`: function/class/import counts and a complexity
    /// grade (`{}` without a tree).
    pub fn analyze_structure(&self, syntax_tree: Option<&SyntaxTree>) -> Map<String, Value> {
        let Some(tree) = syntax_tree else {
            return Map::new();
        };
        let (mut functions, mut classes, mut imports) = (0usize, 0usize, 0usize);
        for node in tree.module.walk() {
            match node.kind {
                StmtKind::FunctionDef { is_async: false, .. } => functions += 1,
                StmtKind::ClassDef { .. } => classes += 1,
                StmtKind::Import { .. } | StmtKind::ImportFrom { .. } => imports += 1,
                _ => {}
            }
        }
        let complexity = if functions + classes < 10 {
            "low"
        } else if functions + classes < 50 {
            "medium"
        } else {
            "high"
        };
        let mut out = Map::new();
        out.insert("function_count".to_string(), json!(functions));
        out.insert("class_count".to_string(), json!(classes));
        out.insert("import_count".to_string(), json!(imports));
        out.insert("complexity".to_string(), json!(complexity));
        out
    }
}

// ------------------------------------------------------------------- CommentExtractor

/// `CommentExtractor(**config)`: code comment extraction engine.
#[derive(Debug, Clone)]
pub struct CommentExtractor {
    /// `self.logger` (`semantica.comment_extractor`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
}

impl Default for CommentExtractor {
    fn default() -> Self {
        CommentExtractor::new(Options::new())
    }
}

fn comment_patterns(language: &str) -> &'static [(Regex, &'static str)] {
    static PYTHON: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
        vec![
            (py_regex(r"#(.+?)$", "(?ms)"), "inline"),
            (py_regex(r#""""(.*?)""""#, "(?ms)"), "docstring"),
            (py_regex(r"'''(.*?)'''", "(?ms)"), "docstring"),
        ]
    });
    static C_STYLE: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
        vec![
            (py_regex(r"//(.+?)$", "(?ms)"), "inline"),
            (py_regex(r"/\*(.*?)\*/", "(?ms)"), "block"),
        ]
    });
    match language {
        "python" => &PYTHON,
        _ => &C_STYLE,
    }
}

impl CommentExtractor {
    /// `CommentExtractor(**config)`.
    pub fn new(config: Options) -> Self {
        CommentExtractor {
            logger: get_logger("comment_extractor"),
            config,
        }
    }

    /// `extract_comments(code_content, language)`: comments found line by line (`#`
    /// comments and `"""`/`'''` strings for Python, `//` and `/* */` otherwise).
    pub fn extract_comments(&self, code_content: &str, language: &str) -> Vec<CodeComment> {
        let patterns = comment_patterns(language);
        let mut comments = Vec::new();
        for (i, line) in py_splitlines(code_content).into_iter().enumerate() {
            for (re, kind) in patterns {
                for caps in re.captures_iter(line) {
                    let text = py_strip(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
                    if !text.is_empty() {
                        comments.push(CodeComment {
                            text: text.to_string(),
                            line_number: (i + 1) as i64,
                            r#type: kind.to_string(),
                            language: language.to_string(),
                        });
                    }
                }
            }
        }
        comments
    }

    /// `extract_docstrings(code_content, language)`: the `"docstring"` comments.
    pub fn extract_docstrings(&self, code_content: &str, language: &str) -> Vec<CodeComment> {
        self.extract_comments(code_content, language)
            .into_iter()
            .filter(|c| c.r#type == "docstring")
            .collect()
    }
}

// ----------------------------------------------------------------- DependencyAnalyzer

/// `DependencyAnalyzer(**config)`: code dependency analysis engine.
#[derive(Debug, Clone)]
pub struct DependencyAnalyzer {
    /// `self.logger` (`semantica.dependency_analyzer`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
}

impl Default for DependencyAnalyzer {
    fn default() -> Self {
        DependencyAnalyzer::new(Options::new())
    }
}

const STDLIB_MODULES: &[&str] = &[
    "os",
    "sys",
    "json",
    "csv",
    "xml",
    "html",
    "urllib",
    "http",
    "email",
    "datetime",
    "time",
    "re",
    "math",
    "random",
    "collections",
    "itertools",
    "functools",
    "operator",
    "pathlib",
    "shutil",
];

impl DependencyAnalyzer {
    /// `DependencyAnalyzer(**config)`.
    pub fn new(config: Options) -> Self {
        DependencyAnalyzer {
            logger: get_logger("dependency_analyzer"),
            config,
        }
    }

    /// `analyze_dependencies(code_content, language)`: `{"imports", "external",
    /// "internal", "standard_library"}`. Valid Python contributes its `import`
    /// statements (classified by `_is_standard_library`); then, for every language, each
    /// line is scanned for `import|from|require|include` references, which are appended to
    /// `imports` (so Python imports usually appear twice).
    pub fn analyze_dependencies(
        &self,
        code_content: &str,
        language: &str,
    ) -> PResult<Map<String, Value>> {
        let mut imports: Vec<String> = Vec::new();
        let mut external: Vec<String> = Vec::new();
        let internal: Vec<String> = Vec::new();
        let mut standard: Vec<String> = Vec::new();
        if language == "python" {
            if let Some(tree) = SyntaxTree::parse(code_content)? {
                for node in tree.module.walk() {
                    match &node.kind {
                        StmtKind::Import { names } => {
                            for name in names {
                                imports.push(name.clone());
                                if is_standard_library(name) {
                                    standard.push(name.clone());
                                } else {
                                    external.push(name.clone());
                                }
                            }
                        }
                        StmtKind::ImportFrom { module, names } => {
                            let module = module.as_deref().unwrap_or("");
                            for name in names {
                                let full = if module.is_empty() {
                                    name.clone()
                                } else {
                                    format!("{module}.{name}")
                                };
                                imports.push(full.clone());
                                let probe = if module.is_empty() { name.as_str() } else { module };
                                if is_standard_library(probe) {
                                    standard.push(full);
                                } else {
                                    external.push(full);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        static IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
            py_regex(
                r#"(?:import|from|require|include)\s+['"](.*?)['"]|(?:import|from)\s+(\S+)"#,
                "(?i)",
            )
        });
        for line in py_splitlines(code_content) {
            for caps in IMPORT_RE.captures_iter(line) {
                if let Some(dep) = first_group(&IMPORT_RE, &caps)? {
                    imports.push(dep);
                }
            }
        }
        let mut out = Map::new();
        out.insert("imports".to_string(), json!(imports));
        out.insert("external".to_string(), json!(external));
        out.insert("internal".to_string(), json!(internal));
        out.insert("standard_library".to_string(), json!(standard));
        Ok(out)
    }
}

/// `_is_standard_library`: a prefix match against a short module list (so `osmnx` and
/// `requests` count as standard library, as in the original).
fn is_standard_library(module_name: &str) -> bool {
    STDLIB_MODULES.iter().any(|m| module_name.starts_with(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(py_splitlines(""), Vec::<&str>::new());
        assert_eq!(py_splitlines("a\n"), vec!["a"]);
        assert_eq!(py_splitlines("\n"), vec![""]);
        assert_eq!(py_splitlines("a\r\nb\rc\x0bd\x1ce\u{2028}f"), vec!["a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn strip_uses_python_whitespace() {
        assert_eq!(py_strip("\x1f a \u{3000}"), "a");
    }
}
