//! Obligations close the four rings: each one belongs to a constraint and names who executes it (generated
//! validation, a Rust check, an independent check, a browser check, an external tool, a runtime receipt, a
//! qualification or an engineering review), at which entry, and by which run evidence it holds. Evidence is a
//! typed predicate over a declared contract: the document schema, a JSON pointer that resolves through the
//! contract's fields, and an operation. A held obligation says its declared evidence was present and satisfied
//! in one run; it does not say the obligation is sufficient or the engineering result correct.
use crate::{Design, identifier, safe_relative};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const REPORT_SCHEMA: &str = "oev2.design-obligation-evidence/1";
/// Executors that run code named by a Rust path and symbol inside their module.
pub const CODE_KINDS: [&str; 5] = [
    "rust_check",
    "independent_check",
    "browser_check",
    "runtime_receipt",
    "external_tool",
];
/// Executors whose result lies outside a single run: shown, never judged from run documents.
pub const OUTSIDE_RUN_KINDS: [&str; 2] = ["qualification", "engineering_review"];
pub const OPERATIONS: [&str; 3] = ["equals", "at_least", "present"];
const DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
const DOCUMENT_COUNT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Declared contract type the document follows.
    pub contract: String,
    /// Value of the document's `schema` field.
    pub schema: String,
    /// JSON pointer into the document, resolved through the contract's fields.
    pub pointer: String,
    pub op: String,
    #[serde(default)]
    pub value: Option<Value>,
    /// Documents of this schema the run must contain; every one must satisfy the predicate.
    #[serde(default = "one")]
    pub min_documents: u64,
}
fn one() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executor {
    pub kind: String,
    pub module: String,
    #[serde(default)]
    pub rust_path: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    /// Command, interface method or procedure through which it runs.
    pub entry: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    pub id: String,
    pub constraint: String,
    pub statement: String,
    pub executors: Vec<Executor>,
    /// What the declared executors do not establish.
    #[serde(default)]
    pub gaps: Vec<String>,
}

fn meaningful(s: &str) -> bool {
    !s.trim().is_empty() && !s.contains('\0')
}

/// Structural rules checked with the rest of the design; symbol presence and pointer typing are static
/// findings of `design inspect`, because they depend on parsed sources and may be work in progress.
pub fn validate(design: &Design) -> Result<()> {
    let constraints: BTreeSet<&str> = design.constraints.iter().map(|c| c.id.as_str()).collect();
    let modules: BTreeMap<&str, &str> = design
        .modules
        .iter()
        .map(|m| (m.id.as_str(), m.rust_path.as_str()))
        .collect();
    let types: BTreeSet<&str> = design.types.iter().map(|t| t.name.as_str()).collect();
    let mut ids = BTreeSet::new();
    for o in &design.obligations {
        ensure!(
            identifier(&o.id) && ids.insert(o.id.as_str()),
            "obligation_id_invalid:{}",
            o.id
        );
        ensure!(
            constraints.contains(o.constraint.as_str()),
            "obligation_constraint_missing:{}",
            o.id
        );
        ensure!(
            meaningful(&o.statement) && !o.executors.is_empty(),
            "obligation_incomplete:{}",
            o.id
        );
        ensure!(
            o.gaps.iter().all(|g| meaningful(g)),
            "obligation_gap_empty:{}",
            o.id
        );
        for e in &o.executors {
            let code = CODE_KINDS.contains(&e.kind.as_str());
            let outside = OUTSIDE_RUN_KINDS.contains(&e.kind.as_str());
            ensure!(
                code || outside || e.kind == "generated_validation",
                "obligation_executor_kind:{}:{}",
                o.id,
                e.kind
            );
            let owner = modules.get(e.module.as_str()).with_context(|| {
                format!("obligation_executor_module_missing:{}:{}", o.id, e.module)
            })?;
            ensure!(
                meaningful(&e.entry),
                "obligation_executor_entry_missing:{}",
                o.id
            );
            if code {
                let path = e
                    .rust_path
                    .as_deref()
                    .with_context(|| format!("obligation_executor_path_missing:{}", o.id))?;
                safe_relative(path)?;
                ensure!(
                    path.starts_with(&format!("{owner}/src/")),
                    "obligation_executor_path_outside_module:{}:{path}",
                    o.id
                );
                ensure!(
                    e.symbol.as_deref().is_some_and(identifier),
                    "obligation_executor_symbol_invalid:{}",
                    o.id
                );
            } else if e.kind == "generated_validation" {
                ensure!(
                    e.rust_path.is_none(),
                    "obligation_generated_validation_path:{}",
                    o.id
                );
                ensure!(
                    e.symbol.as_deref().is_some_and(|s| types.contains(s)),
                    "obligation_generated_validation_type:{}",
                    o.id
                );
            } else {
                ensure!(
                    e.rust_path.is_none() && e.symbol.is_none() && e.evidence.is_empty(),
                    "obligation_outside_run_executor_carries_code_or_evidence:{}",
                    o.id
                );
            }
            for v in &e.evidence {
                ensure!(
                    types.contains(v.contract.as_str()),
                    "obligation_evidence_contract_missing:{}:{}",
                    o.id,
                    v.contract
                );
                ensure!(
                    meaningful(&v.schema) && v.min_documents >= 1,
                    "obligation_evidence_schema:{}",
                    o.id
                );
                ensure!(
                    pointer_tokens(&v.pointer).is_some(),
                    "obligation_evidence_pointer_syntax:{}:{}",
                    o.id,
                    v.pointer
                );
                ensure!(
                    OPERATIONS.contains(&v.op.as_str()),
                    "obligation_evidence_op:{}:{}",
                    o.id,
                    v.op
                );
                match v.op.as_str() {
                    "equals" => ensure!(
                        v.value.is_some(),
                        "obligation_evidence_value_missing:{}",
                        o.id
                    ),
                    "at_least" => ensure!(
                        v.value.as_ref().and_then(Value::as_f64).is_some(),
                        "obligation_evidence_value_not_a_number:{}",
                        o.id
                    ),
                    _ => ensure!(
                        v.value.is_none(),
                        "obligation_evidence_value_unused:{}",
                        o.id
                    ),
                }
            }
        }
    }
    Ok(())
}

/// RFC 6901 tokens of a JSON pointer.
pub fn pointer_tokens(pointer: &str) -> Option<Vec<String>> {
    if pointer.is_empty() {
        return Some(Vec::new());
    }
    let rest = pointer.strip_prefix('/')?;
    rest.split('/')
        .map(|t| {
            let mut out = String::new();
            let mut chars = t.chars();
            while let Some(c) = chars.next() {
                if c == '~' {
                    match chars.next() {
                        Some('0') => out.push('~'),
                        Some('1') => out.push('/'),
                        _ => return None,
                    }
                } else {
                    out.push(c);
                }
            }
            Some(out)
        })
        .collect()
}

/// Why a pointer does not resolve through the declared contract fields, or None when it does. A
/// `serde_json::Value` field accepts any remaining tokens; a vector takes an index; a string-keyed map takes
/// any key.
pub fn untyped(design: &Design, contract: &str, pointer: &str) -> Option<String> {
    let types: BTreeMap<&str, &crate::Type> =
        design.types.iter().map(|t| (t.name.as_str(), t)).collect();
    let tokens = pointer_tokens(pointer)?;
    let mut current = contract.to_owned();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let mut rust = current.trim().to_owned();
        if let Some(inner) = rust
            .strip_prefix("Option<")
            .and_then(|s| s.strip_suffix('>'))
        {
            rust = inner.trim().to_owned();
        }
        if rust == "serde_json::Value" || rust.starts_with("crate::json::") {
            return None;
        }
        if let Some(inner) = rust.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
            if token.parse::<usize>().is_err() {
                return Some(format!("{pointer}: {token} is not an index of {rust}"));
            }
            current = inner.to_owned();
            index += 1;
            continue;
        }
        if let Some(inner) = rust
            .strip_prefix("std::collections::BTreeMap<String,")
            .and_then(|s| s.strip_suffix('>'))
        {
            current = inner.trim().to_owned();
            index += 1;
            continue;
        }
        let Some(t) = types.get(rust.as_str()) else {
            return Some(format!("{pointer}: {rust} has no fields for {token}"));
        };
        let Some(field) = t.fields.iter().find(|f| f.name == *token) else {
            return Some(format!("{pointer}: {rust} has no field {token}"));
        };
        current = field.rust_type.clone();
        index += 1;
    }
    None
}

/// Triples of every obligation, executor and evidence predicate for the RDF model the generator reads.
pub fn rdf(design: &Design, triple: &mut dyn FnMut(&str, &str, String)) -> Result<()> {
    let lit = |s: &str| serde_json::to_string(s).expect("string JSON");
    let iri = |s: &str| format!("<{}{s}>", crate::NS);
    for o in &design.obligations {
        let id = format!("obligation_{}", o.id);
        triple(&id, "kind", lit("Obligation"));
        triple(&id, "name", lit(&o.id));
        triple(&id, "statement", lit(&o.statement));
        triple(&id, "constraint", iri(&o.constraint));
        for gap in &o.gaps {
            triple(&id, "gap", lit(gap));
        }
        for (n, e) in o.executors.iter().enumerate() {
            let eid = format!("{id}_executor_{n:02}");
            triple(&eid, "kind", lit("ObligationExecutor"));
            triple(&eid, "obligation", iri(&id));
            triple(&eid, "ordinal", lit(&format!("{n:02}")));
            triple(&eid, "executorKind", lit(&e.kind));
            triple(&eid, "module", iri(&e.module));
            triple(&eid, "entry", lit(&e.entry));
            if let Some(path) = &e.rust_path {
                triple(&eid, "rustPath", lit(path));
            }
            if let Some(symbol) = &e.symbol {
                triple(&eid, "symbol", lit(symbol));
            }
            for (m, v) in e.evidence.iter().enumerate() {
                let vid = format!("{eid}_evidence_{m:02}");
                triple(&vid, "kind", lit("ObligationEvidence"));
                triple(&vid, "executor", iri(&eid));
                triple(&vid, "ordinal", lit(&format!("{m:02}")));
                triple(&vid, "contract", lit(&v.contract));
                triple(&vid, "documentSchema", lit(&v.schema));
                triple(&vid, "pointer", lit(&v.pointer));
                triple(&vid, "op", lit(&v.op));
                triple(
                    &vid,
                    "valueJson",
                    lit(&match &v.value {
                        Some(value) => serde_json::to_string(value)?,
                        None => "null".into(),
                    }),
                );
                triple(&vid, "minDocuments", lit(&v.min_documents.to_string()));
            }
        }
    }
    Ok(())
}

/// JSON documents of one run with a top-level `schema`, by schema.
fn documents(run: &Path) -> Result<BTreeMap<String, Vec<(String, Value)>>> {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                walk(&path, out)?;
            } else if path.extension().is_some_and(|x| x == "json")
                && meta.len() as usize <= DOCUMENT_BYTES
            {
                out.push(path);
            }
            ensure!(out.len() <= DOCUMENT_COUNT, "obligation_run_document_limit");
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(run, &mut files)?;
    let mut out: BTreeMap<String, Vec<(String, Value)>> = BTreeMap::new();
    for path in files {
        let Ok(value) =
            oe_core::json::strict_json(&oe_core::read_regular_bytes(&path, DOCUMENT_BYTES)?)
        else {
            continue;
        };
        if let Some(schema) = value.get("schema").and_then(Value::as_str) {
            let relative = path
                .strip_prefix(run)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.entry(schema.to_owned())
                .or_default()
                .push((relative, value));
        }
    }
    Ok(out)
}

fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) if a.is_number() && b.is_number() => x == y,
        _ => a == b,
    }
}

/// Judge every obligation against the documents of one run.
pub fn evaluate(design: &Design, run: &Path) -> Result<Value> {
    let design = design.clone().normalize()?;
    ensure!(run.is_dir(), "obligation_run_directory_missing");
    // The caller names the run directory; its real path is walked, and links inside it are skipped.
    let run = std::fs::canonicalize(run)?;
    let docs = documents(&run)?;
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut results = Vec::new();
    for o in &design.obligations {
        let mut executors = Vec::new();
        let (mut failed, mut absent, mut held, mut observable) = (false, false, 0, 0);
        for e in &o.executors {
            if e.evidence.is_empty() {
                // Qualification and review lie outside a run; any other executor simply declared no run evidence.
                let status = if OUTSIDE_RUN_KINDS.contains(&e.kind.as_str()) {
                    "outside_run"
                } else {
                    "no_run_evidence"
                };
                executors.push(json!({"kind":e.kind,"module":e.module,"symbol":e.symbol,"entry":e.entry,"status":status}));
                continue;
            }
            observable += 1;
            let mut predicates = Vec::new();
            let mut status = "held";
            for v in &e.evidence {
                let found = docs.get(&v.schema).map(Vec::as_slice).unwrap_or_default();
                if (found.len() as u64) < v.min_documents {
                    predicates.push(json!({"schema":v.schema,"pointer":v.pointer,"status":"absent","documents":found.len(),"min_documents":v.min_documents}));
                    if status == "held" {
                        status = "absent";
                    }
                    continue;
                }
                let mut misses = Vec::new();
                for (path, doc) in found {
                    let observed = doc.pointer(&v.pointer);
                    let holds = match (v.op.as_str(), observed) {
                        ("present", Some(x)) => !x.is_null(),
                        ("equals", Some(x)) => v.value.as_ref().is_some_and(|want| same(x, want)),
                        ("at_least", Some(x)) => x
                            .as_f64()
                            .zip(v.value.as_ref().and_then(Value::as_f64))
                            .is_some_and(|(a, b)| a >= b),
                        _ => false,
                    };
                    if !holds {
                        misses.push(json!({"document":path,"observed":observed}));
                    }
                }
                let ok = misses.is_empty();
                predicates.push(json!({"schema":v.schema,"pointer":v.pointer,"op":v.op,"value":v.value,
                    "documents":found.len(),"status":if ok {"held"} else {"failed"},"misses":misses}));
                if !ok {
                    status = "failed";
                }
            }
            match status {
                "failed" => failed = true,
                "absent" => absent = true,
                _ => held += 1,
            }
            executors.push(json!({"kind":e.kind,"module":e.module,"symbol":e.symbol,"entry":e.entry,"status":status,"evidence":predicates}));
        }
        let status = if failed {
            "failed"
        } else if absent {
            "absent"
        } else if observable == 0 {
            "outside_run"
        } else {
            debug_assert_eq!(held, observable);
            "held"
        };
        *counts.entry(status).or_default() += 1;
        results.push(
            json!({"id":o.id,"constraint":o.constraint,"statement":o.statement,"status":status,
            "executors":executors,"gaps":o.gaps}),
        );
    }
    Ok(
        json!({"schema":REPORT_SCHEMA,"design_sha256":oe_core::sha256(design.canonical()?),
        "documents":docs.iter().map(|(k, v)| (k.clone(), json!(v.len()))).collect::<serde_json::Map<_, _>>(),
        "counts":counts,"obligations":results,
        "scope":"Declared run evidence of declared obligations in one run directory. held: every run-observable executor's predicates were satisfied by all documents of its schema; failed: a predicate was not satisfied; absent: too few documents of a schema; outside_run: no executor declares run evidence (only qualification, review or executors without run evidence). Not sufficiency of the obligations, not engineering correctness."}),
    )
}
