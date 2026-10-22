//! Exact project-record projections. These checks establish identity, not truth.
use anyhow::{Context, Result, bail, ensure};
use oe_core::json::{canonical_json, strict_json};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

pub const SCHEMA: &str = "ontology-engineering.supplier-knowledge-context/v1";
pub const PROJECTION_SCHEMA: &str = "ontology-engineering.project-context-projection/v1";
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const ROOT_TARGETS: &[&str] = &[
    "goal",
    "stage",
    "background",
    "decision_id",
    "functions",
    "statements",
    "unknowns",
    "changes",
    "alternatives",
    "prior_decisions",
    "record_refs",
    "decision_criteria",
    "tradeoffs",
];

pub(crate) fn digest(value: &Value) -> Result<String> {
    Ok(sha256(canonical_json(value)?))
}
pub(crate) fn object<'a>(value: &'a Value, code: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().with_context(|| code.to_owned())
}
pub(crate) fn text<'a>(value: &'a Value, max: usize, code: &str) -> Result<&'a str> {
    let s = value.as_str().with_context(|| code.to_owned())?;
    ensure!(s.chars().count() <= max, "{code}");
    Ok(s)
}
pub(crate) fn exact(value: &Value, fields: &[&str], code: &str) -> Result<()> {
    let o = object(value, code)?;
    ensure!(
        o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)),
        "{code}"
    );
    Ok(())
}
pub(crate) fn hash_string(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn default_text(v: &Value, key: &str, max: usize) -> Result<Value> {
    Ok(Value::String(
        text(
            v.get(key).unwrap_or(&Value::String(String::new())),
            max,
            "knowledge_context_invalid_text",
        )?
        .to_owned(),
    ))
}
fn string_list(v: &Value, count: usize, max: usize, code: &str) -> Result<Value> {
    let a = v.as_array().with_context(|| code.to_owned())?;
    ensure!(a.len() <= count, "{code}");
    Ok(Value::Array(
        a.iter()
            .map(|x| {
                text(x, max, "knowledge_context_invalid_text").map(|s| Value::String(s.to_owned()))
            })
            .collect::<Result<_>>()?,
    ))
}
fn json_space_count(v: &Value) -> usize {
    match v {
        Value::Array(a) => {
            a.len().saturating_sub(1) + a.iter().map(json_space_count).sum::<usize>()
        }
        Value::Object(o) => {
            o.len() + o.len().saturating_sub(1) + o.values().map(json_space_count).sum::<usize>()
        }
        _ => 0,
    }
}
fn legacy_json_size(v: &Value) -> Result<usize> {
    Ok(canonical_json(v)?.len() + json_space_count(v))
}
fn contains_credential(s: &str) -> bool {
    s.match_indices("apikey_").any(|(i, _)| {
        s[i + 7..]
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }) || s.match_indices("-----BEGIN ").any(|(i, _)| {
        s[i + 11..].find("PRIVATE KEY-----").is_some_and(|end| {
            s[i + 11..i + 11 + end]
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b == b' ')
        })
    })
}

pub fn validate(value: &Value) -> Result<Value> {
    let v = object(value, "knowledge_context_schema_mismatch")?;
    ensure!(
        value["schema"] == SCHEMA,
        "knowledge_context_schema_mismatch"
    );
    let allowed = [
        "schema",
        "task_id",
        "project",
        "object",
        "goal",
        "stage",
        "functions",
        "statements",
        "unknowns",
        "changes",
        "alternatives",
        "prior_decisions",
        "record_refs",
        "background",
        "decision_criteria",
        "tradeoffs",
        "iteration",
        "decision_id",
        "source_bindings",
    ];
    ensure!(
        v.keys().all(|k| allowed.contains(&k.as_str())),
        "knowledge_context_unknown_fields"
    );
    let mut c = json!({"schema":SCHEMA,"task_id":default_text(value,"task_id",200)?,"goal":default_text(value,"goal",1500)?,"stage":default_text(value,"stage",300)?,"background":default_text(value,"background",1800)?});
    ensure!(
        !c["task_id"].as_str().unwrap_or_default().trim().is_empty(),
        "knowledge_context_task_id_required"
    );
    if v.contains_key("decision_id") {
        c["decision_id"] = default_text(value, "decision_id", 200)?;
        ensure!(
            !c["decision_id"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .is_empty(),
            "knowledge_context_decision_id_required"
        );
    }
    for (name, fields) in [
        ("project", &['i', 'r'][..]),
        ("object", &['i', 'n', 'r'][..]),
    ] {
        let names: Vec<&str> = fields
            .iter()
            .map(|x| match x {
                'i' => "id",
                'n' => "name",
                _ => "revision",
            })
            .collect();
        let empty = json!({});
        let item = value.get(name).unwrap_or(&empty);
        let o = object(item, "knowledge_context_invalid_identity")?;
        ensure!(
            o.keys().all(|k| names.contains(&k.as_str())),
            "knowledge_context_invalid_identity"
        );
        let mut identity = Map::new();
        for k in names {
            identity.insert(k.to_owned(), default_text(item, k, 300)?);
        }
        c[name] = Value::Object(identity);
    }
    for name in [
        "functions",
        "unknowns",
        "changes",
        "alternatives",
        "prior_decisions",
        "record_refs",
    ] {
        c[name] = string_list(
            value.get(name).unwrap_or(&json!([])),
            16,
            600,
            "knowledge_context_invalid_list",
        )?;
    }
    let empty = json!([]);
    let statements = value
        .get("statements")
        .unwrap_or(&empty)
        .as_array()
        .context("knowledge_context_invalid_statements")?;
    ensure!(
        statements.len() <= 24,
        "knowledge_context_invalid_statements"
    );
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for s in statements {
        let o = object(s, "knowledge_context_invalid_statement")?;
        ensure!(
            o.keys()
                .all(|k| ["id", "text", "status", "source_refs"].contains(&k.as_str())),
            "knowledge_context_invalid_statement"
        );
        let sid = default_text(s, "id", 200)?;
        let id = sid.as_str().unwrap_or_default();
        let status = s["status"].as_str().unwrap_or_default();
        ensure!(
            !id.is_empty()
                && seen.insert(id.to_owned())
                && [
                    "requirement",
                    "observation",
                    "hypothesis",
                    "source_report",
                    "accepted_decision",
                    "unknown"
                ]
                .contains(&status),
            "knowledge_context_invalid_statement"
        );
        result.push(json!({"id":sid,"text":default_text(s,"text",1000)?,"status":status,"source_refs":string_list(s.get("source_refs").unwrap_or(&empty),12,600,"knowledge_context_invalid_statement")?}));
    }
    c["statements"] = json!(result);
    if let Some(criteria) = v.get("decision_criteria") {
        let a = criteria
            .as_array()
            .context("knowledge_context_invalid_criteria")?;
        ensure!(a.len() <= 16, "knowledge_context_invalid_criteria");
        let mut ids = BTreeSet::new();
        let mut out = Vec::new();
        for x in a {
            let o = object(x, "knowledge_context_invalid_criterion")?;
            ensure!(
                o.keys()
                    .all(|k| ["id", "dimension", "kind", "statement", "source_refs"]
                        .contains(&k.as_str())),
                "knowledge_context_invalid_criterion"
            );
            let id = default_text(x, "id", 200)?;
            let name = id.as_str().unwrap_or_default();
            let kind = x["kind"].as_str().unwrap_or_default();
            ensure!(
                !name.is_empty()
                    && ids.insert(name.to_owned())
                    && ["hard_constraint", "objective", "preference", "unknown"].contains(&kind),
                "knowledge_context_invalid_criterion"
            );
            out.push(json!({"id":id,"dimension":default_text(x,"dimension",200)?,"kind":kind,"statement":default_text(x,"statement",1000)?,"source_refs":string_list(x.get("source_refs").unwrap_or(&empty),12,600,"knowledge_context_invalid_criterion")?}));
        }
        c["decision_criteria"] = json!(out);
    }
    if let Some(x) = v.get("tradeoffs") {
        c["tradeoffs"] = string_list(x, 16, 600, "knowledge_context_invalid_tradeoffs")?;
    }
    if let Some(x) = v.get("iteration") {
        let o = object(x, "knowledge_context_invalid_iteration")?;
        ensure!(
            o.keys()
                .all(|k| ["id", "focus", "stop_condition"].contains(&k.as_str())),
            "knowledge_context_invalid_iteration"
        );
        c["iteration"] = json!({"focus":default_text(x,"focus",600)?,"id":default_text(x,"id",600)?,"stop_condition":default_text(x,"stop_condition",600)?});
    }
    ensure!(
        legacy_json_size(&c)? <= 8000,
        "knowledge_context_too_large_use_relevant_snapshot"
    );
    ensure!(
        !contains_credential(&String::from_utf8(canonical_json(&c)?)?),
        "knowledge_context_contains_credential"
    );
    if let Some(x) = v.get("source_bindings") {
        c["source_bindings"] = validate_bindings(x)?;
        ensure!(
            legacy_json_size(x)? <= 24000,
            "knowledge_context_bindings_too_large"
        );
    }
    Ok(c)
}

pub(crate) fn project_path(value: &str) -> Result<&str> {
    let parts: Vec<_> = value.split('/').collect();
    ensure!(
        !value.is_empty()
            && value.chars().count() <= 600
            && !value.contains(['\\', '\0'])
            && parts.len() >= 3
            && parts[0] == "var"
            && parts[1] == "projects"
            && parts
                .iter()
                .all(|p| !p.is_empty() && *p != "." && *p != ".."),
        "knowledge_context_source_path_invalid"
    );
    Ok(value)
}
pub(crate) fn relative_project(root: &SkillRoot, value: &str) -> Result<String> {
    let p = Path::new(value);
    let relative = if p.is_absolute() {
        p.strip_prefix(root.path())
            .context("knowledge_feedback_path_outside_project")?
    } else {
        p
    };
    let s = relative
        .to_str()
        .context("knowledge_context_source_path_invalid")?;
    Ok(project_path(s)?.to_owned())
}
pub(crate) fn project_directory(path: &str) -> Result<String> {
    project_path(path)?;
    Ok(path.split('/').take(3).collect::<Vec<_>>().join("/"))
}
pub(crate) fn pointer(value: &str) -> Result<Vec<String>> {
    ensure!(
        value.chars().count() <= 1000
            && !value.contains('\0')
            && (value.is_empty() || value.starts_with('/')),
        "knowledge_context_pointer_invalid"
    );
    let bytes = value.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'~' {
            ensure!(
                matches!(bytes.get(i + 1), Some(b'0' | b'1')),
                "knowledge_context_pointer_invalid"
            );
        }
    }
    Ok(if value.is_empty() {
        vec![]
    } else {
        value[1..]
            .split('/')
            .map(|p| p.replace("~1", "/").replace("~0", "~"))
            .collect()
    })
}
pub(crate) fn get_pointer<'a>(value: &'a Value, ptr: &str) -> Result<&'a Value> {
    let mut current = value;
    for part in pointer(ptr)? {
        current = match current {
            Value::Object(o) => o.get(&part),
            Value::Array(a)
                if part == "0"
                    || (!part.starts_with('0')
                        && !part.is_empty()
                        && part.bytes().all(|b| b.is_ascii_digit())) =>
            {
                part.parse::<usize>().ok().and_then(|i| a.get(i))
            }
            _ => None,
        }
        .context("knowledge_context_pointer_missing")?;
    }
    Ok(current)
}
fn target(value: &str) -> Result<Vec<String>> {
    let p = pointer(value)?;
    let valid = match p.as_slice() {
        [a] => ROOT_TARGETS.contains(&a.as_str()),
        [a, b] => match a.as_str() {
            "project" => ["id", "revision"].contains(&b.as_str()),
            "object" => ["id", "revision", "name"].contains(&b.as_str()),
            "iteration" => ["id", "focus", "stop_condition"].contains(&b.as_str()),
            _ => false,
        },
        _ => false,
    };
    ensure!(valid, "knowledge_context_target_invalid");
    Ok(p)
}
pub(crate) fn read_project(root: &SkillRoot, relative: &str) -> Result<Vec<u8>> {
    project_path(relative)?;
    read_regular_bytes(&root.path().join(relative), MAX_SOURCE_BYTES).map_err(|e| {
        let code = if e
            .chain()
            .filter_map(|x| x.downcast_ref::<std::io::Error>())
            .any(|x| x.kind() == std::io::ErrorKind::NotFound)
        {
            "source_missing"
        } else if e.to_string().contains("input_exceeds_limit") {
            "source_too_large"
        } else if e.to_string().contains("input_not_regular_file") {
            "source_not_regular"
        } else {
            "source_unreadable_or_symlink"
        };
        anyhow::anyhow!("knowledge_context_{code}")
    })
}

pub fn validate_bindings(bindings: &Value) -> Result<Value> {
    let sources = bindings
        .as_array()
        .context("knowledge_context_bindings_invalid")?;
    ensure!(sources.len() <= 16, "knowledge_context_bindings_invalid");
    ensure!(
        canonical_json(bindings)?.len() <= 24000,
        "knowledge_context_bindings_too_large"
    );
    let mut paths = BTreeSet::new();
    let mut targets = BTreeSet::new();
    let mut count = 0;
    for source in sources {
        exact(
            source,
            &["path", "sha256", "mappings"],
            "knowledge_context_binding_invalid",
        )?;
        let p = text(
            &source["path"],
            600,
            "knowledge_context_source_path_invalid",
        )?;
        project_path(p)?;
        ensure!(
            paths.insert(p),
            "knowledge_context_binding_duplicate_source"
        );
        ensure!(
            hash_string(&source["sha256"]),
            "knowledge_context_binding_hash_invalid"
        );
        let mappings = source["mappings"]
            .as_array()
            .context("knowledge_context_binding_mappings_invalid")?;
        ensure!(
            !mappings.is_empty(),
            "knowledge_context_binding_mappings_invalid"
        );
        for mapping in mappings {
            count += 1;
            ensure!(count <= 64, "knowledge_context_binding_mapping_limit");
            exact(
                mapping,
                &["pointer", "target", "value_sha256"],
                "knowledge_context_binding_mapping_invalid",
            )?;
            pointer(text(
                &mapping["pointer"],
                1000,
                "knowledge_context_pointer_invalid",
            )?)?;
            let t = text(&mapping["target"], 1000, "knowledge_context_target_invalid")?;
            target(t)?;
            ensure!(
                targets.insert(t),
                "knowledge_context_binding_duplicate_target"
            );
            ensure!(
                hash_string(&mapping["value_sha256"]),
                "knowledge_context_binding_hash_invalid"
            );
        }
    }
    Ok(bindings.clone())
}

pub fn project(root: &SkillRoot, spec: &Value) -> Result<Value> {
    exact(
        spec,
        &["schema", "base", "fields"],
        "knowledge_context_projection_spec_invalid",
    )?;
    ensure!(
        spec["schema"] == PROJECTION_SCHEMA,
        "knowledge_context_projection_schema_invalid"
    );
    exact(
        &spec["base"],
        &["schema", "task_id"],
        "knowledge_context_projection_base_invalid",
    )?;
    ensure!(
        spec["base"]["schema"] == SCHEMA
            && spec["base"]["task_id"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
        "knowledge_context_projection_base_invalid"
    );
    let fields = spec["fields"]
        .as_array()
        .context("knowledge_context_projection_fields_invalid")?;
    ensure!(
        !fields.is_empty() && fields.len() <= 64,
        "knowledge_context_projection_fields_invalid"
    );
    let mut context = spec["base"].clone();
    let mut documents = HashMap::new();
    let mut bindings = Vec::<Value>::new();
    let mut indexes = HashMap::new();
    let mut targets = BTreeSet::new();
    for field in fields {
        exact(
            field,
            &["source", "pointer", "target"],
            "knowledge_context_projection_field_invalid",
        )?;
        let path = text(
            &field["source"],
            600,
            "knowledge_context_source_path_invalid",
        )?;
        project_path(path)?;
        let t = text(&field["target"], 1000, "knowledge_context_target_invalid")?;
        let parts = target(t)?;
        ensure!(
            targets.insert(t),
            "knowledge_context_binding_duplicate_target"
        );
        if !documents.contains_key(path) {
            ensure!(
                documents.len() < 16,
                "knowledge_context_binding_source_limit"
            );
            let raw = read_project(root, path)?;
            let doc = strict_json(&raw).context("knowledge_context_source_json_invalid")?;
            documents.insert(path.to_owned(), doc);
            indexes.insert(path.to_owned(), bindings.len());
            bindings.push(json!({"path":path,"sha256":sha256(raw),"mappings":[]}));
        }
        let ptr = text(&field["pointer"], 1000, "knowledge_context_pointer_invalid")?;
        let value = get_pointer(&documents[path], ptr)?.clone();
        if parts.len() == 1 {
            context[&parts[0]] = value.clone();
        } else {
            if context.get(&parts[0]).is_none() {
                context[&parts[0]] = json!({});
            }
            context[&parts[0]][&parts[1]] = value.clone();
        }
        let i = indexes[path];
        bindings[i]["mappings"]
            .as_array_mut()
            .context("knowledge_context_internal_binding")?
            .push(json!({"pointer":ptr,"target":t,"value_sha256":digest(&value)?}));
    }
    let mut context = validate(&context)?;
    let checked = validate_bindings(&json!(bindings))?;
    for source in checked
        .as_array()
        .context("knowledge_context_bindings_invalid")?
    {
        for mapping in source["mappings"]
            .as_array()
            .context("knowledge_context_binding_mappings_invalid")?
        {
            ensure!(
                digest(get_pointer(
                    &context,
                    mapping["target"].as_str().unwrap_or_default()
                )?)? == mapping["value_sha256"],
                "knowledge_context_projection_normalization_changed_source_value"
            );
        }
    }
    context["source_bindings"] = checked;
    validate(&context)
}

pub fn model_view(value: &Value) -> Result<Value> {
    let context = validate(value)?;
    let mut c = context.clone();
    let o = c
        .as_object_mut()
        .context("knowledge_context_invalid_context")?;
    o.remove("record_refs");
    o.remove("source_bindings");
    for key in ["statements", "decision_criteria"] {
        if let Some(items) = o.get_mut(key).and_then(Value::as_array_mut) {
            for item in items {
                item.as_object_mut()
                    .context("knowledge_context_invalid_context")?
                    .remove("source_refs");
            }
        }
    }
    c["context_sha256"] = json!(digest(&context)?);
    Ok(c)
}

pub fn verify_sources(root: &SkillRoot, context: &Value) -> Result<Value> {
    object(context, "knowledge_context_invalid_context")?;
    let bindings = validate_bindings(context.get("source_bindings").unwrap_or(&json!([])))?;
    let sources = bindings
        .as_array()
        .context("knowledge_context_bindings_invalid")?;
    let mut targets = BTreeSet::new();
    for source in sources {
        for map in source["mappings"]
            .as_array()
            .context("knowledge_context_binding_mappings_invalid")?
        {
            targets.insert(map["target"].as_str().unwrap_or_default().to_owned());
        }
    }
    let mut unmapped = BTreeSet::new();
    if !sources.is_empty() {
        for (k, value) in context
            .as_object()
            .context("knowledge_context_invalid_context")?
        {
            if ["schema", "task_id", "source_bindings", "record_refs"].contains(&k.as_str()) {
                continue;
            }
            let pairs =
                if ["project", "object", "iteration"].contains(&k.as_str()) && value.is_object() {
                    value
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(leaf, v)| (format!("/{k}/{leaf}"), v))
                        .collect::<Vec<_>>()
                } else {
                    vec![(format!("/{k}"), value)]
                };
            for (t, v) in pairs {
                let empty = v == ""
                    || v.as_array().is_some_and(Vec::is_empty)
                    || v.as_object().is_some_and(Map::is_empty);
                if !targets.contains(&t) && !empty {
                    unmapped.insert(t);
                }
            }
        }
    }
    let mut results = Vec::new();
    for binding in sources {
        let mut reasons = BTreeSet::new();
        let mappings = binding["mappings"]
            .as_array()
            .context("knowledge_context_binding_mappings_invalid")?;
        for map in mappings {
            if get_pointer(context, map["target"].as_str().unwrap_or_default())
                .and_then(digest)
                .ok()
                .as_deref()
                != map["value_sha256"].as_str()
            {
                reasons.insert("context_value_changed".to_owned());
            }
        }
        match read_project(root, binding["path"].as_str().unwrap_or_default()) {
            Ok(raw) => {
                if sha256(&raw) != binding["sha256"] {
                    reasons.insert("source_changed".to_owned());
                } else {
                    match strict_json(&raw) {
                        Ok(doc) => {
                            for map in mappings {
                                if get_pointer(&doc, map["pointer"].as_str().unwrap_or_default())
                                    .and_then(digest)
                                    .ok()
                                    .as_deref()
                                    != map["value_sha256"].as_str()
                                {
                                    reasons.insert("source_value_changed".to_owned());
                                }
                            }
                        }
                        Err(_) => {
                            reasons.insert("source_json_invalid".to_owned());
                        }
                    }
                }
            }
            Err(e) => {
                reasons.insert(
                    e.to_string()
                        .trim_start_matches("knowledge_context_")
                        .to_owned(),
                );
            }
        }
        results.push(json!({"path":binding["path"],"status":if reasons.is_empty(){"current"}else{"stale"},"reasons":reasons}));
    }
    let status = if sources.is_empty() {
        "not_bound"
    } else if !unmapped.is_empty() || results.iter().any(|r| r["status"] == "stale") {
        "stale"
    } else {
        "current"
    };
    Ok(
        json!({"status":status,"sources":results,"reasons":if unmapped.is_empty(){vec![]}else{vec!["unmapped_context_field"]},"unmapped_context_fields":unmapped,"meaning":"Exact selected project-record bytes and mapped values only; no fact acceptance or project identity inference."}),
    )
}

pub(crate) fn owner(context: &Value) -> Result<String> {
    let mut owners = BTreeSet::new();
    for binding in context["source_bindings"]
        .as_array()
        .context("knowledge_feedback_project_owner_required")?
    {
        if binding["mappings"]
            .as_array()
            .is_some_and(|a| a.iter().any(|m| m["target"] == "/project/id"))
        {
            owners.insert(project_directory(
                binding["path"].as_str().unwrap_or_default(),
            )?);
        }
    }
    if owners.len() != 1 {
        bail!("knowledge_feedback_project_owner_required");
    }
    Ok(owners.into_iter().next().unwrap())
}
