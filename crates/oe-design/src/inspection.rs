//! Static implementation clues are deliberately distinct from executed evidence.
use crate::Design;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::parse::Parser;
use syn::visit::Visit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub classification: String,
    pub subject: String,
    pub basis: String,
    pub impact: Vec<String>,
    pub verification: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct CodeFile {
    pub path: String,
    pub sha256: String,
    pub functions: BTreeSet<String>,
    pub tests: BTreeSet<String>,
    pub public_types: BTreeSet<String>,
    pub calls: BTreeSet<String>,
    pub trait_implementations: BTreeSet<String>,
}
#[derive(Default)]
struct Symbols {
    functions: BTreeSet<String>,
    tests: BTreeSet<String>,
    types: BTreeSet<String>,
    calls: BTreeSet<String>,
    trait_implementations: BTreeSet<String>,
    test_context: usize,
}
impl<'ast> Visit<'ast> for Symbols {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Test scaffolding must not count as production constraint execution.
        let only_test = excluded_from_production(&node.attrs);
        if only_test {
            self.test_context += 1;
        }
        syn::visit::visit_item_mod(self, node);
        if only_test {
            self.test_context -= 1;
        }
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if node.attrs.iter().any(|a| a.path().is_ident("test")) {
            self.tests.insert(node.sig.ident.to_string());
            return;
        }
        if self.test_context > 0 {
            return;
        }
        if excluded_from_production(&node.attrs) {
            return;
        }
        self.functions.insert(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
    }
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let excluded = excluded_from_production(&node.attrs);
        if !excluded && self.test_context == 0 {
            if let (Some((_, trait_path, _)), syn::Type::Path(self_type)) =
                (&node.trait_, node.self_ty.as_ref())
            {
                if let (Some(t), Some(s)) =
                    (trait_path.segments.last(), self_type.path.segments.last())
                {
                    self.trait_implementations
                        .insert(format!("{} for {}", t.ident, s.ident));
                }
            }
        }
        if excluded {
            self.test_context += 1;
        }
        syn::visit::visit_item_impl(self, node);
        if excluded {
            self.test_context -= 1;
        }
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if self.test_context > 0 || excluded_from_production(&node.attrs) {
            return;
        }
        self.functions.insert(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
    }
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if self.test_context == 0
            && !excluded_from_production(&node.attrs)
            && matches!(node.vis, syn::Visibility::Public(_))
        {
            self.types.insert(node.ident.to_string());
        }
    }
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if self.test_context == 0
            && !excluded_from_production(&node.attrs)
            && matches!(node.vis, syn::Visibility::Public(_))
        {
            self.types.insert(node.ident.to_string());
        }
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = node.func.as_ref() {
            self.calls.insert(
                p.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        }
        syn::visit::visit_expr_call(self, node);
    }
}
// Evaluate only what follows from test=false. Other platform/features stay unknown;
// their symbol presence is a static clue, never proof of a compiled target.
fn cfg_value(meta: &syn::Meta) -> Option<bool> {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => Some(false),
        syn::Meta::List(list) => {
            let args = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .ok()?;
            let values: Vec<_> = args.iter().map(cfg_value).collect();
            if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|v| *v == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|v| *v == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else if list.path.is_ident("not") && values.len() == 1 {
                values[0].map(|v| !v)
            } else {
                None
            }
        }
        _ => None,
    }
}
fn attribute_value(meta: &syn::Meta) -> Option<bool> {
    let syn::Meta::List(list) = meta else {
        return Some(true);
    };
    if list.path.is_ident("cfg") {
        return cfg_value(&syn::parse2::<syn::Meta>(list.tokens.clone()).ok()?);
    }
    if list.path.is_ident("cfg_attr") {
        let args = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
            .parse2(list.tokens.clone())
            .ok()?;
        let mut args = args.iter();
        let condition = cfg_value(args.next()?);
        let values: Vec<_> = args.map(attribute_value).collect();
        let attributes = if values.contains(&Some(false)) {
            Some(false)
        } else if values.iter().all(|v| *v == Some(true)) {
            Some(true)
        } else {
            None
        };
        return match condition {
            Some(false) => Some(true),
            Some(true) => attributes,
            None if attributes == Some(true) => Some(true),
            None => None,
        };
    }
    Some(true)
}
fn excluded_from_production(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| attribute_value(&a.meta) == Some(false))
}
pub fn files(root: &Path, directory: &Path) -> Result<Vec<PathBuf>> {
    oe_core::no_symlinks(directory)?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "source_symlink_refused");
        if kind.is_dir() {
            if !["var", "sources", ".git", "target"]
                .contains(&entry.file_name().to_string_lossy().as_ref())
            {
                out.extend(files(root, &path)?);
            }
        } else if kind.is_file() {
            ensure!(path.starts_with(root), "source_outside_root");
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}
pub fn inventory(root: &Path) -> Result<Vec<CodeFile>> {
    let mut rows = Vec::new();
    let mut paths = files(root, &root.join("crates"))?;
    // The pinned parser and evaluator are already in the compiled input
    // closure. Their actual production symbols belong to this same inventory.
    for dependency in [
        "vendor/spargebra",
        "vendor/spareval",
        "vendor/sparopt",
        "vendor/oxttl",
        "vendor/oxrdfxml",
        "vendor/oxjsonld",
    ] {
        if root.join(dependency).is_dir() {
            paths.extend(files(root, &root.join(dependency))?);
        }
    }
    for path in paths {
        if path.extension().is_none_or(|s| s != "rs") {
            continue;
        }
        let bytes = oe_core::read_regular_bytes(&path, 4 * 1024 * 1024)?;
        let parsed = syn::parse_file(std::str::from_utf8(&bytes)?)?;
        let mut s = Symbols::default();
        s.visit_file(&parsed);
        rows.push(CodeFile {
            path: oe_core::portable_source_path(root, &path)?,
            sha256: oe_core::sha256(&bytes),
            functions: s.functions,
            tests: s.tests,
            public_types: s.types,
            calls: s.calls,
            trait_implementations: s.trait_implementations,
        });
    }
    Ok(rows)
}
/// Derived source facts, never a second manually edited design model.
pub fn rdf(root: &Path, design: &Design) -> Result<(String, BTreeMap<String, String>)> {
    let mut triples = BTreeSet::new();
    let mut hashes = BTreeMap::new();
    let iri = source_iri;
    let lit = |text: &str| serde_json::to_string(text).expect("string serialization");
    let mut add = |id: &str, p: &str, value: String| {
        triples.insert(format!("{} {} {value} .", iri(id), source_predicate(p)));
    };
    for f in inventory(root)? {
        let id = format!("code_{}", oe_core::sha256(f.path.as_bytes()));
        hashes.insert(f.path.clone(), f.sha256.clone());
        add(&id, "kind", lit("RustCodeFile"));
        add(&id, "path", lit(&f.path));
        add(&id, "sha256", lit(&f.sha256));
        let production = !f
            .path
            .split('/')
            .any(|s| ["tests", "examples", "benches"].contains(&s));
        add(&id, "production", production.to_string());
        // A crate may implement several declared logical modules. Record all
        // matching path memberships; choosing the first declaration loses
        // associations and makes these static facts depend on module order.
        // Membership alone does not prove exclusive functional ownership or
        // behavior. The separate checker/implementation edges carry those
        // narrower declared relations.
        for owner in design
            .modules
            .iter()
            .filter(|m| f.path.starts_with(&format!("{}/", m.rust_path)))
        {
            add(&id, "module", iri(&owner.id));
        }
        for name in &f.functions {
            let function = format!(
                "function_{}",
                oe_core::sha256(format!("{}::{name}", f.path).as_bytes())
            );
            add(&function, "kind", lit("RustFunction"));
            add(&function, "name", lit(name));
            add(&function, "file", iri(&id));
            for constraint in design
                .constraints
                .iter()
                .filter(|c| production && c.rust_path == f.path && c.check == *name)
            {
                add(&function, "executes", iri(&constraint.id));
            }
        }
        for name in &f.tests {
            add(&id, "test", lit(name));
        }
        for name in &f.public_types {
            add(&id, "publicType", lit(name));
        }
        for name in &f.calls {
            add(&id, "calls", lit(name));
        }
        for interface in &design.interfaces {
            if production
                && interface.rust_path.as_ref() == Some(&f.path)
                && interface.implementation.as_ref().is_some_and(|symbol| {
                    f.trait_implementations
                        .contains(&format!("{} for {}", interface.name, symbol))
                })
            {
                add(
                    &id,
                    "implements",
                    iri(&format!("interface_{}", interface.name)),
                );
            }
        }
    }
    // Manifests, toolchains and the declared dependency patch affect execution too.
    let mut build_paths = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("rust-toolchain.toml"),
        root.join(".cargo/config.toml"),
    ];
    for m in &design.modules {
        build_paths.push(root.join(&m.rust_path).join("Cargo.toml"));
    }
    // Reuse the compiled-input authority for JSON inputs embedded in tests and
    // source checks; their identities must also reach the real ggen source map.
    for path in crate::provenance::compiled()?["files"]
        .as_object()
        .unwrap()
        .keys()
    {
        // Migration assets already have dedicated identity and size contracts
        // in model(); preserve those owners instead of applying a second generic
        // build-input reader to the complete 4.57 MiB original source inventory.
        if !path.ends_with(".rs")
            && ![
                "runtime/semantica-scope.json",
                "runtime/semantica-scope-lock.json",
                "runtime/semantica-migration-mappings.json",
            ]
            .contains(&path.as_str())
        {
            build_paths.push(root.join(path));
        }
    }
    if root.join("vendor").is_dir() {
        build_paths.extend(files(root, &root.join("vendor"))?);
    }
    for p in build_paths.into_iter().filter(|p| p.is_file()) {
        let path = oe_core::portable_source_path(root, &p)?;
        let digest = oe_core::sha256(oe_core::read_regular_bytes(&p, 4 * 1024 * 1024)?);
        hashes.insert(path.clone(), digest.clone());
        let id = format!("build_{}", oe_core::sha256(path.as_bytes()));
        add(&id, "kind", lit("RustBuildInput"));
        add(&id, "path", lit(&path));
        add(&id, "sha256", lit(&digest));
    }
    Ok((
        format!(
            "# Rust source facts derived by syn; existence is a static clue, not behavior evidence.\n{SOURCE_PREFIXES}{}\n",
            triples.into_iter().collect::<Vec<_>>().join("\n")
        ),
        hashes,
    ))
}
pub fn inspect(root: &Path, design: &Design) -> Result<serde_json::Value> {
    design.validate()?;
    let code = inventory(root)?;
    let mut findings = Vec::new();
    let mut emit = |code: &str, subject: &str, basis: String, verification: &str| {
        findings.push(Finding {
            code: code.into(),
            classification: "static_clue".into(),
            subject: subject.into(),
            basis,
            impact: design
                .impact(&[subject.to_owned()].into_iter().collect())
                .into_iter()
                .collect(),
            verification: verification.into(),
        });
    };
    for m in &design.modules {
        if !root.join(&m.rust_path).exists() {
            emit(
                "implementation_missing",
                &m.id,
                m.rust_path.clone(),
                "implement and run declared independent behavioral case",
            );
        }
    }
    for interface in &design.interfaces {
        match (&interface.rust_path, &interface.implementation) {
            (Some(path), Some(symbol))
                if code.iter().any(|f| {
                    f.path == *path
                        && f.trait_implementations
                            .contains(&format!("{} for {}", interface.name, symbol))
                }) =>
            {
                ()
            }
            (Some(path), Some(symbol)) => emit(
                "interface_implementation_missing",
                &format!("interface_{}", interface.name),
                format!("{path}: {} for {symbol}", interface.name),
                "implement the generated trait in its production owner and execute boundary cases",
            ),
            _ => emit(
                "interface_implementation_unmapped",
                &format!("interface_{}", interface.name),
                interface.module.clone(),
                "bind actual generated trait implementation, not its declaration",
            ),
        }
    }
    for c in &design.constraints {
        if !code
            .iter()
            .any(|f| f.path == c.rust_path && f.functions.contains(&c.check))
        {
            emit(
                "constraint_execution_missing",
                &c.id,
                format!("{}::{}", c.rust_path, c.check),
                "identify actual production checker and exercise a single-fault counterexample",
            );
        }
        if !design.validations.iter().any(|v| v.constraint == c.id) {
            emit(
                "constraint_validation_missing",
                &c.id,
                c.statement.clone(),
                "add independent positive, single-fault and unknown cases",
            );
        }
    }
    for v in &design.validations {
        if !code
            .iter()
            .any(|f| f.path == v.rust_path && f.tests.contains(&v.behavior))
        {
            emit(
                "behavior_case_missing",
                &v.id,
                format!("{}::{}", v.rust_path, v.behavior),
                "run declared case and record kernel/spec/source identities",
            );
        }
    }
    let declared: BTreeSet<_> = design
        .modules
        .iter()
        .map(|m| m.rust_path.as_str())
        .collect();
    for f in &code {
        if !declared.iter().any(|p| f.path.starts_with(*p)) {
            emit(
                "source_outside_design",
                &f.path,
                f.sha256.clone(),
                "classify module, responsibilities, interfaces, effects and tests in design",
            );
        }
    }
    let mut names: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in &design.types {
        for f in &t.fields {
            names.entry(&f.name).or_default().push(&f.meaning);
        }
    }
    for (name, meanings) in names {
        if meanings.iter().copied().collect::<BTreeSet<_>>().len() > 1 {
            emit(
                "field_meaning_conflict",
                name,
                meanings.join(" | "),
                "establish explicit semantic identity or use distinct field names",
            );
        }
    }
    Ok(
        serde_json::json!({"schema":"oev2.design-inspection/1","status":if findings.is_empty(){"no_static_findings"}else{"findings"},"scope":"declared design and parsed Rust symbols; no behavior pass implied","files":code,"findings":findings}),
    )
}

// Compact only generated digest identities. Semantic IRIs and source literals
// remain exact; explicit ASCII validation makes these legal Turtle PN_LOCALs.
const SOURCE_PREFIXES: &str = "@prefix code: <urn:oev2:design:code_> .\n@prefix function: <urn:oev2:design:function_> .\n@prefix sp: <urn:oev2:design:> .\n";
// Only fixed source-property names use this prefix. Raw source values and
// arbitrary semantic identifiers never pass through a text replacement.
fn source_predicate(name: &str) -> String {
    match name {
        "kind" | "path" | "sha256" | "production" | "module" | "name" | "file" | "executes"
        | "test" | "publicType" | "calls" | "implements" => format!("sp:{name}"),
        _ => format!("<{}{name}>", crate::NS),
    }
}
fn source_iri(name: &str) -> String {
    for prefix in ["code", "function"] {
        if let Some(digest) = name.strip_prefix(&format!("{prefix}_")) {
            if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return format!("{prefix}:{digest}");
            }
        }
    }
    format!("<{}{name}>", crate::NS)
}
#[cfg(test)]
mod transport_tests {
    use super::*;
    #[test]
    fn source_predicate_prefix_preserves_expanded_terms_and_literals() {
        use oxigraph::io::{RdfFormat, RdfParser};
        let properties = [
            "kind",
            "path",
            "sha256",
            "production",
            "module",
            "name",
            "file",
            "executes",
            "test",
            "publicType",
            "calls",
            "implements",
            "future_property",
        ];
        let literal =
            serde_json::to_string("sp:kind <urn:oev2:design:kind> 中文 \"quoted\"\n\\").unwrap();
        let parse = |s: &str| {
            RdfParser::from_format(RdfFormat::Turtle)
                .for_slice(s.as_bytes())
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
        };
        let mut expanded = String::new();
        let mut compact = SOURCE_PREFIXES.to_string();
        for p in properties {
            expanded.push_str(&format!("<urn:source> <urn:oev2:design:{p}> {literal} .\n"));
            compact.push_str(&format!(
                "<urn:source> {} {literal} .\n",
                source_predicate(p)
            ));
        }
        assert_eq!(parse(&expanded), parse(&compact));
        assert_eq!(
            source_predicate("future_property"),
            "<urn:oev2:design:future_property>"
        );
        assert_eq!(compact.matches(&literal).count(), 13);
        assert!(compact.len() < expanded.len());
    }
    #[test]
    fn source_prefix_preserves_terms_and_non_digest_names() {
        use oxigraph::io::{RdfFormat, RdfParser};
        let mut old = String::new();
        let mut new = SOURCE_PREFIXES.to_string();
        for name in [
            format!("code_{}", "0".repeat(64)),
            format!("function_{}", "a".repeat(64)),
            "code_literal".into(),
            "interface_other".into(),
        ] {
            let literal = serde_json::to_string(&format!(
                "code:literal <urn:oev2:design:{name}> 中文 \"quoted\""
            ))
            .unwrap();
            old.push_str(&format!(
                "<{}{name}> <urn:oev2:design:path> {literal} .\n",
                crate::NS
            ));
            new.push_str(&format!(
                "{} <urn:oev2:design:path> {literal} .\n",
                source_iri(&name)
            ));
        }
        let parse = |s: &str| {
            RdfParser::from_format(RdfFormat::Turtle)
                .for_slice(s.as_bytes())
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(parse(&old), parse(&new));
        assert_eq!(source_iri("code_literal"), "<urn:oev2:design:code_literal>");
        assert_eq!(
            source_iri(&format!("code_{}", "x".repeat(64))),
            format!("<urn:oev2:design:code_{}>", "x".repeat(64))
        );
    }
}
