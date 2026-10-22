//! Characterisation tests for `code_parser` (CodeParser, SyntaxTreeParser, CommentExtractor,
//! DependencyAnalyzer, CodeStructure, CodeComment) and the Python `ast` port behind it,
//! against the original (fixtures/code): 39 real Python files of the original source tree,
//! snippets with modern syntax, syntax errors and other languages, and string-level calls.

mod common;

use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{Value, json};

use sem_parse::common::{DecodeErrors, py_read_text};
use sem_parse::{
    CodeComment, CodeParser, CodeStructure, CommentExtractor, DependencyAnalyzer, Options,
    SyntaxTreeParser,
};
use sem_utils::SemanticaError;

static ORACLE: Lazy<Value> = Lazy::new(|| common::load_oracle("code"));

fn dir() -> PathBuf {
    common::fixture("code")
}

fn outcome<T: Serialize>(result: Result<T, SemanticaError>) -> Value {
    let dir = dir().display().to_string();
    let v = match result {
        Ok(v) => json!({ "ok": serde_json::to_value(v).expect("serialisable") }),
        Err(e) => common::err_json(&e),
    };
    common::replace_in_strings(v, &dir, "$FILES")
}

fn ok<T: Serialize>(v: T) -> Value {
    outcome(Ok::<_, SemanticaError>(v))
}

/// The per-file record of the generator.
fn per_file(path: &Path) -> Value {
    let cp = CodeParser::default();
    let stp = SyntaxTreeParser::default();
    let ce = CommentExtractor::default();
    let content = py_read_text(path, "utf-8", DecodeErrors::Ignore).unwrap();
    let (none, analysis) = match stp.parse_syntax_tree(&content, "python") {
        Ok(tree) => (
            json!(tree.is_none()),
            Value::Object(stp.analyze_structure(tree.as_ref())),
        ),
        Err(e) => (common::err_json(&e), Value::Null),
    };
    json!({
        "parse_code": outcome(cp.parse_code(path, None, &Options::new())),
        "syntax_tree_none": none,
        "analyze_structure": analysis,
        "docstrings": ok(ce.extract_docstrings(&content, "python")),
    })
}

#[test]
fn test_corpus_of_original_sources_matches_oracle() {
    common::quiet_progress();
    let corpus = ORACLE["corpus"].as_object().unwrap();
    assert!(corpus.len() >= 30);
    for (name, expected) in corpus {
        let actual = per_file(&dir().join("corpus").join(name));
        assert_eq!(&actual, expected, "corpus file {name}");
    }
}

#[test]
fn test_snippets_match_oracle() {
    common::quiet_progress();
    for (name, expected) in ORACLE["snippets"].as_object().unwrap() {
        let actual = per_file(&dir().join("snippets").join(name));
        assert_eq!(&actual, expected, "snippet {name}");
    }
}

#[test]
fn test_languages_match_oracle() {
    common::quiet_progress();
    let cp = CodeParser::new(None, Options::new());
    let modern = std::fs::read_to_string(dir().join("snippets").join("modern.py")).unwrap();
    let src = modern.split("\nprint(").next().unwrap();
    let js = std::fs::read_to_string(dir().join("snippets").join("script.js")).unwrap();
    let c = std::fs::read_to_string(dir().join("snippets").join("prog.c")).unwrap();
    for (lang, expected) in ORACLE["languages"].as_object().unwrap() {
        let actual = json!({
            "structure": outcome(cp.extract_structure(src, lang)),
            "comments": ok(cp.extract_comments(src, lang)),
            "dependencies": outcome(cp.analyze_dependencies(src, lang)),
            "js_src_structure": outcome(cp.extract_structure(&js, lang)),
            "c_src_comments": ok(cp.extract_comments(&c, lang)),
        });
        assert_eq!(&actual, expected, "language {lang}");
    }
}

#[test]
fn test_misc_cases_match_oracle() {
    common::quiet_progress();
    let misc = &ORACLE["misc"];
    let cp = CodeParser::default();
    let stp = SyntaxTreeParser::new(Options::new());
    let snip = |n: &str| dir().join("snippets").join(n);
    let none = Options::new();
    assert_eq!(outcome(cp.parse_code(snip("missing.py"), None, &none)), misc["missing_file"]);
    assert_eq!(outcome(cp.parse_code(snip("script.js"), Some("python"), &none)), misc["explicit_language"]);
    assert_eq!(outcome(cp.parse_code(snip("notes.txt"), Some("cobol"), &none)), misc["unsupported_language"]);
    assert_eq!(Value::Object(stp.analyze_structure(None)), misc["analyze_structure_none"]);
    assert_eq!(serde_json::to_value(&cp.supported_languages).unwrap(), misc["supported_languages"]);
    let many: String = (0..12).map(|i| format!("def f{i}(): pass")).collect::<Vec<_>>().join("\n");
    let tree = stp.parse_syntax_tree(&many, "python").unwrap();
    assert_eq!(Value::Object(stp.analyze_structure(tree.as_ref())), misc["analyze_many"]);
    let high: String = (0..55).map(|i| format!("class C{i}: pass")).collect::<Vec<_>>().join("\n");
    let tree = stp.parse_syntax_tree(&high, "python").unwrap();
    assert_eq!(Value::Object(stp.analyze_structure(tree.as_ref())), misc["analyze_high"]);
    let da = DependencyAnalyzer::new(Options::new());
    assert_eq!(
        outcome(da.analyze_dependencies(
            "import os.path, collectionsx\nfrom . import y\nfrom xml.dom import minidom\nimport requests\n",
            "python"
        )),
        misc["dependencies_stdlib"]
    );
    // Non-Python languages have no syntax tree.
    assert!(stp.parse_syntax_tree("def f(): pass", "javascript").unwrap().is_none());
}

#[test]
fn test_dataclasses_and_sub_parsers() {
    let structure = CodeStructure::default();
    assert!(structure.functions.is_empty() && structure.variables.is_empty() && structure.constants.is_empty());
    let comment = CodeComment {
        text: "t".into(),
        line_number: 1,
        r#type: "inline".into(),
        language: "python".into(),
    };
    assert_eq!(serde_json::to_value(&comment).unwrap()["type"], "inline");
    let mut config = Options::new();
    config.insert("syntax".into(), json!({"k": 1}));
    let cp = CodeParser::new(Some(config), Options::new());
    assert_eq!(cp.syntax_parser.config["k"], 1);
    assert_eq!(cp.logger.name(), "semantica.code_parser");
    let ce = CommentExtractor::new(Options::new());
    let docs = ce.extract_docstrings("x = 1  # c\n\"\"\"d\"\"\"\n", "python");
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].text, "d");
}
