use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

pub(crate) const JSON_LIMIT: usize = 16 * 1024 * 1024;
pub(crate) const SOURCE_LIMIT: u64 = 16 * 1024 * 1024 * 1024;
pub(crate) fn entries<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
pub(crate) fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().expect("validated string field")
}
pub(crate) fn strings<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    entries(v, key)
        .iter()
        .map(|v| v.as_str().expect("validated string array"))
}
pub(crate) type Inventory<'a> = BTreeMap<&'a str, (&'static str, &'a Value)>;
pub(crate) fn inventory<'a>(
    packet: &'a Value,
    collections: &[(&str, &'static str)],
    operations: bool,
) -> Result<Inventory<'a>> {
    let mut ids = BTreeMap::new();
    for (key, kind) in collections {
        for entry in entries(packet, key) {
            ensure!(
                ids.insert(s(entry, "id"), (*kind, entry)).is_none(),
                "duplicate_entity_id:{}",
                s(entry, "id")
            );
            if operations && *kind == "route" {
                for op in entries(entry, "operations") {
                    ensure!(
                        ids.insert(s(op, "id"), ("operation", op)).is_none(),
                        "duplicate_entity_id:{}",
                        s(op, "id")
                    );
                }
            }
        }
    }
    Ok(ids)
}
pub(crate) fn expect(ids: &Inventory<'_>, owner: &str, id: &str, kinds: &[&str]) -> Result<()> {
    ensure!(
        ids.get(id).is_some_and(|(kind, _)| kinds.contains(kind)),
        "absent/wrong-kind:{owner}:{id}"
    );
    Ok(())
}
pub(crate) fn refs(ids: &Inventory<'_>, e: &Value, field: &str, kinds: &[&str]) -> Result<()> {
    for id in strings(e, field) {
        expect(ids, s(e, "id"), id, kinds)?;
    }
    Ok(())
}
pub(crate) fn path(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(
        !relative.is_empty()
            && !relative.contains(['\\', ':'])
            && !relative.starts_with('/')
            && relative.split('/').all(|s| !s.is_empty()
                && s != "."
                && s != ".."
                && !s.chars().any(char::is_control)),
        "source_path_must_be_normalized_and_relative"
    );
    let path = root.join(relative);
    oe_core::no_symlinks(&path)?;
    Ok(path)
}
pub(crate) struct Sources {
    pub docs: BTreeMap<String, Value>,
    pub json_bytes: BTreeMap<String, Vec<u8>>,
    pub results: Vec<Value>,
}
impl Sources {
    pub fn json(&self, id: &str) -> Result<&Value> {
        self.docs
            .get(id)
            .context("source_requires_application/json")
    }
}
pub(crate) fn sources(packet: &Value, root: &Path) -> Result<Sources> {
    oe_core::no_symlinks(root)?;
    ensure!(root.is_dir(), "project_root_not_directory");
    let mut docs = BTreeMap::new();
    let mut json_bytes = BTreeMap::new();
    let mut results = Vec::new();
    for source in entries(packet, "sources") {
        let id = s(source, "id");
        let relative = s(source, "path");
        let file = path(root, relative)?;
        let actual = if s(source, "media_type") == "application/json" {
            let bytes = oe_core::read_regular_bytes(&file, JSON_LIMIT)?;
            let digest = oe_wire::sha256(&bytes);
            ensure!(
                digest == s(source, "sha256"),
                "source_SHA-256_mismatch:{id}"
            );
            docs.insert(id.to_owned(), oe_wire::json::strict_json(&bytes)?);
            json_bytes.insert(id.to_owned(), bytes);
            digest
        } else {
            oe_core::fingerprint_file(&file, SOURCE_LIMIT)?.sha256
        };
        ensure!(
            actual == s(source, "sha256"),
            "source_SHA-256_mismatch:{id}"
        );
        results.push(json!({"id":id,"path":relative,"sha256":actual}));
    }
    let result = Sources {
        docs,
        json_bytes,
        results,
    };
    for source in entries(packet, "sources") {
        if let Some(embedded) = source
            .get("embedded_source_hashes")
            .and_then(Value::as_object)
        {
            for (ptr, target) in embedded {
                let target = target.as_str().context("source_target")?;
                let expected = entries(packet, "sources")
                    .iter()
                    .find(|s0| s(s0, "id") == target)
                    .context("source_target_missing")?;
                ensure!(
                    pointer(result.json(s(source, "id"))?, ptr)? == &expected["sha256"],
                    "embedded_source_hash_mismatch"
                );
            }
        }
    }
    Ok(result)
}
pub(crate) fn pointer<'a>(v: &'a Value, p: &str) -> Result<&'a Value> {
    ensure!(p.starts_with('/'), "invalid_JSON_pointer");
    // serde's JSON pointer rejects negative/leading-zero indexes. Check escapes too.
    for part in p.split('/').skip(1) {
        let mut chars = part.chars();
        while let Some(c) = chars.next() {
            if c == '~' {
                ensure!(
                    matches!(chars.next(), Some('0' | '1')),
                    "invalid_JSON_pointer_escape"
                );
            }
        }
    }
    v.pointer(p).context("JSON_pointer_does_not_resolve")
}
/// JSON number equivalence keeps integer/float native readbacks compatible,
/// without Python's accidental True == 1 coercion or lossy large-int casts.
pub(crate) fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let x = x.to_string();
            let y = y.to_string();
            let xf = x.contains(['.', 'e', 'E']);
            let yf = y.contains(['.', 'e', 'E']);
            match (xf, yf) {
                (false, false) => x == y,
                (true, true) => x.parse::<f64>().ok() == y.parse::<f64>().ok(),
                _ => {
                    let (integer, float) = if xf { (&y, &x) } else { (&x, &y) };
                    float.parse::<f64>().ok().is_some_and(|v| {
                        v.is_finite()
                            && v.fract() == 0.0
                            && if v == 0.0 {
                                integer == "0"
                            } else {
                                format!("{v:.0}") == *integer
                            }
                    })
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, a)| y.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}
pub(crate) fn adapter_hash() -> String {
    oe_wire::sha256(
        [
            include_str!("mod.rs"),
            include_str!("common.rs"),
            include_str!("schema.rs"),
            include_str!("../handoff.rs"),
        ]
        .join("\n"),
    )
}
pub(crate) fn percent(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"_.-~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
pub(crate) fn base(packet: &Value) -> String {
    format!(
        "urn:ontology-engineering:project:{}:",
        [
            "project_id",
            "configuration_id",
            "model_revision",
            "product_state"
        ]
        .iter()
        .map(|k| percent(s(&packet["project"], k)))
        .collect::<Vec<_>>()
        .join(":")
    )
}
pub(crate) fn uri(base: &str, kind: &str, id: &str) -> String {
    format!("{base}{kind}:{}", percent(id))
}
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
#[derive(Default)]
pub(crate) struct Graph(BTreeSet<String>);
impl Graph {
    pub fn iri(&mut self, a: &str, p: &str, b: &str) {
        self.0.insert(format!("<{a}> <{p}> <{b}> .\n"));
    }
    pub fn text(&mut self, a: &str, p: &str, b: &str) {
        self.0.insert(format!(
            "<{a}> <{p}> {} .\n",
            serde_json::to_string(b).expect("string JSON")
        ));
    }
    pub fn fields(&mut self, a: &str, ns: &str, e: &Value, keys: &[&str]) {
        for key in keys {
            self.text(a, &format!("{ns}{key}"), s(e, key));
        }
    }
    pub fn links(
        &mut self,
        a: &str,
        vocabulary: (&str, &str),
        predicate: &str,
        kind: &str,
        e: &Value,
        field: &str,
    ) {
        let (ns, base) = vocabulary;
        for id in strings(e, field) {
            self.iri(a, &format!("{ns}{predicate}"), &uri(base, kind, id));
        }
    }
    pub fn bytes(self) -> Vec<u8> {
        self.0.into_iter().collect::<String>().into_bytes()
    }
}
// Match Python's sort_keys=True, ensure_ascii=False object literals in the old
// projections; this only formats JSON, never interprets embedded assertions.
pub(crate) fn spaced_json(v: &Value) -> String {
    // Reuse the shared Python-compatible numeric/string formatter and sorted
    // object keys; only add the old default separator spaces outside strings.
    let compact = oe_wire::json::canonical_json(v).expect("validated finite JSON");
    let mut out = Vec::with_capacity(compact.len());
    let mut quoted = false;
    let mut escaped = false;
    for byte in compact {
        out.push(byte);
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if matches!(byte, b',' | b':') {
            out.push(b' ');
        }
    }
    String::from_utf8(out).expect("JSON UTF-8")
}
pub(crate) type Dependencies = BTreeMap<String, (String, Value, BTreeSet<String>)>;
pub(crate) fn dep_fields(e: &Value, fields: &[&str]) -> BTreeSet<String> {
    fields
        .iter()
        .flat_map(|f| strings(e, f))
        .map(str::to_owned)
        .collect()
}
pub(crate) fn impact(
    previous: &Value,
    current: &Value,
    old: Dependencies,
    new: Dependencies,
    scope_kinds: &[&str],
    record: &str,
    outputs: &[(&str, &str)],
) -> Value {
    let mut changed: BTreeSet<String> = old
        .keys()
        .chain(new.keys())
        .filter(|k| match (old.get(*k), new.get(*k)) {
            (Some(a), Some(b)) => a.0 != b.0 || !equivalent(&a.1, &b.1) || a.2 != b.2,
            _ => true,
        })
        .cloned()
        .collect();
    let identity = previous["project"] != current["project"];
    if identity {
        changed.extend(
            new.iter()
                .filter(|(_, v)| scope_kinds.is_empty() || scope_kinds.contains(&v.0.as_str()))
                .map(|(k, _)| k.clone()),
        );
    }
    let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for inv in [&old, &new] {
        for (id, (_, _, deps)) in inv {
            for dep in deps {
                reverse.entry(dep.clone()).or_default().insert(id.clone());
            }
        }
    }
    let mut affected = changed.clone();
    let mut queue: VecDeque<String> = changed.iter().cloned().collect();
    while let Some(id) = queue.pop_front() {
        if let Some(dependents) = reverse.get(&id) {
            for dependent in dependents {
                if affected.insert(dependent.clone()) {
                    queue.push_back(dependent.clone());
                }
            }
        }
    }
    let mut result = json!({"record_type":record,"identity_scope_changed":identity,"changed_ids":changed,"affected_ids":affected,"semantic_execution":"not_run","engineering_verdict":"re_review_required_not_assessed"});
    for (field, kind) in outputs {
        result[*field] = json!(
            affected
                .iter()
                .filter(|id| new
                    .get(*id)
                    .or_else(|| old.get(*id))
                    .is_some_and(|v| v.0 == *kind))
                .collect::<Vec<_>>()
        );
    }
    result
}
