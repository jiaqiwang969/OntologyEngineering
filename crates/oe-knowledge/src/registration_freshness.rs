//! Per-document registration currentness, separate from historical source integrity.
//! No whole-library digest comparison can invalidate an otherwise unchanged book.
use crate::sources::Provider;
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    LocalPdfSourceRegistrationBinding, LocalPdfSourceRegistrationIdentity,
    SourceRegistrationFreshness,
};
use oe_core::{SkillRoot, json::canonical_digest};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

pub const BINDING_SCHEMA: &str = "ontology-engineering.local-pdf-source-binding/v1";
pub const FRESHNESS_SCHEMA: &str = "ontology-engineering.source-registration-freshness/v1";
const MAX_INDEX_DIRECTORIES: usize = 96;
const SOURCE_FIELDS: &[&str] = &[
    "id",
    "title",
    "edition",
    "path",
    "sha256",
    "bytes",
    "page_count",
    "chapters_sha256",
];

fn exact_fields(value: &Value, fields: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .context("source_registration_binding_invalid")?;
    ensure!(
        object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field)),
        "source_registration_binding_invalid"
    );
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.chars().count() <= 2000)
        .context("source_registration_binding_invalid")
}

fn digest(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        .context("source_registration_binding_invalid")
}

fn relative_path(value: &str, directory: bool) -> Result<()> {
    ensure!(
        !value.contains('\\')
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != "..")
            && if directory {
                value.starts_with("sources/.indexes/")
            } else {
                value.starts_with("sources/")
                    && value != "sources/.indexes"
                    && !value.starts_with("sources/.indexes/")
            },
        "source_registration_binding_path_invalid"
    );
    Ok(())
}

fn validate_binding(binding: &Value) -> Result<()> {
    exact_fields(binding, &["schema", "index_directory", "source", "sha256"])?;
    ensure!(
        binding["schema"] == BINDING_SCHEMA,
        "source_registration_binding_invalid"
    );
    relative_path(text(binding, "index_directory")?, true)?;
    let source = &binding["source"];
    exact_fields(source, SOURCE_FIELDS)?;
    let id = text(source, "id")?;
    ensure!(
        id.len() <= 120
            && id.as_bytes()[0].is_ascii_alphanumeric()
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-')),
        "source_registration_binding_invalid"
    );
    text(source, "title")?;
    text(source, "edition")?;
    relative_path(text(source, "path")?, false)?;
    digest(&source["sha256"])?;
    digest(&source["chapters_sha256"])?;
    ensure!(
        source["bytes"]
            .as_u64()
            .is_some_and(|n| n > 0 && n <= crate::citations::MAX_SOURCE_BYTES)
            && source["page_count"]
                .as_u64()
                .is_some_and(|n| n > 0 && n <= 20000),
        "source_registration_binding_invalid"
    );
    let mut unsigned = binding.clone();
    unsigned
        .as_object_mut()
        .expect("validated object")
        .remove("sha256");
    ensure!(
        digest(&binding["sha256"])? == canonical_digest(&unsigned)?,
        "source_registration_binding_digest_mismatch"
    );
    let typed: LocalPdfSourceRegistrationBinding = serde_json::from_value(binding.clone())?;
    ensure!(
        typed.validate().is_ok(),
        "source_registration_binding_contract_invalid"
    );
    Ok(())
}

fn root_relative_path(root: &SkillRoot, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root.path())
        .context("source_registration_binding_path_invalid")?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!("source_registration_binding_path_invalid");
        };
        parts.push(
            name.to_str()
                .context("source_registration_binding_path_invalid")?,
        );
    }
    ensure!(
        !parts.is_empty(),
        "source_registration_binding_path_invalid"
    );
    Ok(parts.join("/"))
}

/// Bind the complete registered navigation by digest, without copying it into each page.
pub(crate) fn capture(root: &SkillRoot, directory: &Path, source: &Value) -> Result<Value> {
    let path = crate::sources::resolve_original(root, directory, text(source, "path")?)?;
    ensure!(
        source["chapters"].is_array(),
        "source_registration_navigation_invalid"
    );
    let identity = LocalPdfSourceRegistrationIdentity {
        id: text(source, "id")?.to_owned(),
        title: text(source, "title")?.to_owned(),
        edition: text(source, "edition")?.to_owned(),
        path: root_relative_path(root, &path)?,
        sha256: digest(&source["sha256"])?.to_owned(),
        bytes: source["bytes"]
            .as_u64()
            .context("source_registration_binding_invalid")?,
        page_count: source["page_count"]
            .as_u64()
            .context("source_registration_binding_invalid")?,
        chapters_sha256: canonical_digest(&source["chapters"])?,
    };
    let index_directory = root_relative_path(root, directory)?;
    let sha256 = canonical_digest(
        &json!({"schema":BINDING_SCHEMA,"index_directory":index_directory,"source":identity}),
    )?;
    let binding = serde_json::to_value(LocalPdfSourceRegistrationBinding {
        schema: BINDING_SCHEMA.into(),
        index_directory,
        source: identity,
        sha256,
    })?;
    validate_binding(&binding)?;
    Ok(binding)
}

/// Validate a declared binding against this historical candidate. Missing binding
/// remains explicitly unknown; it must never be inferred from today's default index.
pub(crate) fn validate_result(root: &SkillRoot, result: &Value) -> Result<Option<Value>> {
    let Some(binding) = result.get("source_registration_binding") else {
        return Ok(None);
    };
    validate_binding(binding)?;
    let source = &binding["source"];
    let locator = &result["source_locator"];
    let page = result["pdf_page"]
        .as_u64()
        .context("source_registration_binding_candidate_mismatch")?;
    let id = source["id"].as_str().expect("validated id");
    ensure!(
        result["catalog"] == format!("local_pdf:{id}")
            && result["id"] == format!("local_pdf:{id}:page:{page}")
            && result["catalog_title"] == source["title"]
            && result["source_sha256"] == source["sha256"]
            && locator["schema"] == "ontology-engineering.source-page/v1"
            && locator["kind"] == "local_pdf"
            && locator["document_id"] == source["id"]
            && locator["document_sha256"] == source["sha256"]
            && locator["title"] == source["title"]
            && locator["edition"] == source["edition"]
            && locator["page_count"] == source["page_count"]
            && locator["physical_page"].as_u64() == Some(page)
            && page > 0
            && page <= source["page_count"].as_u64().expect("validated count")
            && crate::packet::source_path(root, &result["source_pdf"])?
                == root
                    .path()
                    .join(source["path"].as_str().expect("validated path")),
        "source_registration_binding_candidate_mismatch"
    );
    if let Some(bytes) = result.get("source_bytes") {
        ensure!(
            *bytes == source["bytes"],
            "source_registration_binding_candidate_mismatch"
        );
    }
    // Packet candidates omit source_bytes, so check the already bounded original's
    // actual length as well. Its SHA is independently checked by packet verification.
    let original = crate::packet::source_path(root, &result["source_pdf"])?;
    ensure!(
        std::fs::metadata(original)?.len() == source["bytes"].as_u64().expect("validated bytes"),
        "source_registration_binding_candidate_mismatch"
    );
    Ok(Some(binding.clone()))
}

/// Reconcile only explicitly bound documents, sharing a verified read-only provider
/// per index. Provider snapshots are temporary and are checked again before return.
pub fn audit(root: &SkillRoot, results: &[Value]) -> Result<Value> {
    let mut bindings = Vec::new();
    let mut providers: BTreeMap<String, Option<Provider>> = BTreeMap::new();
    for result in results {
        let binding = validate_result(root, result)?;
        if let Some(binding) = &binding {
            let directory = binding["index_directory"]
                .as_str()
                .expect("validated directory");
            // Bound new index work, not the size of an older historical packet.
            // Every declared binding is still validated even after this budget.
            if !providers.contains_key(directory) && providers.len() < MAX_INDEX_DIRECTORIES {
                providers.insert(
                    directory.to_owned(),
                    Provider::open(root, "local_pdf", Some(Path::new(directory))).ok(),
                );
            }
        }
        bindings.push(binding);
    }
    let mut candidates = Vec::new();
    let mut current: BTreeMap<(String, String), Option<Value>> = BTreeMap::new();
    for (result, binding) in results.iter().zip(bindings) {
        let mut candidate = json!({"candidate_id":result["id"],"changed_fields":[]});
        let Some(binding) = binding else {
            candidate["status"] = json!(if result["catalog"]
                .as_str()
                .is_some_and(|s| s.starts_with("local_pdf:"))
                || result["source_locator"]["kind"] == "local_pdf"
            {
                "not_bound"
            } else {
                "not_applicable"
            });
            candidates.push(candidate);
            continue;
        };
        let directory = binding["index_directory"]
            .as_str()
            .expect("validated directory");
        let id = binding["source"]["id"].as_str().expect("validated id");
        candidate["index_directory"] = json!(directory);
        candidate["document_id"] = json!(id);
        candidate["expected_source"] = binding["source"].clone();
        candidate["expected_binding_sha256"] = binding["sha256"].clone();
        let Some(provider) = providers.get(directory).and_then(Option::as_ref) else {
            candidate["status"] = json!("unavailable");
            candidate["reason"] = json!(if providers.contains_key(directory) {
                "registered_index_missing_invalid_or_unavailable"
            } else {
                "registered_index_directory_budget_exhausted"
            });
            candidates.push(candidate);
            continue;
        };
        let key = (directory.to_owned(), id.to_owned());
        if !current.contains_key(&key) {
            match provider.registered_source_binding(id) {
                Ok(binding) => {
                    current.insert(key.clone(), binding);
                }
                Err(_) => {
                    candidate["status"] = json!("unavailable");
                    candidate["reason"] = json!("registered_source_identity_unavailable");
                    candidates.push(candidate);
                    continue;
                }
            }
        }
        if let Some(now) = &current[&key] {
            candidate["current_source"] = now["source"].clone();
            candidate["current_binding_sha256"] = now["sha256"].clone();
            let changed = SOURCE_FIELDS
                .iter()
                .filter(|field| binding["source"][**field] != now["source"][**field])
                .copied()
                .collect::<Vec<_>>();
            candidate["status"] = json!(if changed.is_empty() {
                "current"
            } else {
                "changed"
            });
            candidate["changed_fields"] = json!(changed);
        } else {
            candidate["status"] = json!("removed");
        }
        candidates.push(candidate);
    }
    for (directory, provider) in &providers {
        if provider
            .as_ref()
            .is_some_and(|p| p.verify_snapshot().is_err())
        {
            for candidate in &mut candidates {
                if candidate["index_directory"] == *directory {
                    candidate["status"] = json!("unavailable");
                    candidate["reason"] = json!("registered_index_changed_during_verification");
                }
            }
        }
    }
    let status = if candidates.is_empty() {
        "not_bound"
    } else {
        let first = candidates[0]["status"].as_str().expect("status");
        if candidates
            .iter()
            .all(|candidate| candidate["status"] == first)
        {
            first
        } else {
            "mixed"
        }
    };
    let report = SourceRegistrationFreshness {
        schema: FRESHNESS_SCHEMA.into(), status: status.into(), candidates,
        meaning: "Currentness of explicitly bound local-PDF registrations only. Historical original integrity and Project context are checked separately. Whole-library changes do not invalidate unchanged documents. No applicability, source correctness, engineering adoption or execution authority.".into(),
    };
    ensure!(
        report.validate().is_ok(),
        "source_registration_freshness_contract_invalid"
    );
    Ok(serde_json::to_value(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_relative_paths_use_portable_separators_and_preserve_names() {
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let root = SkillRoot::new(producer).unwrap();
        let relative = root
            .path()
            .join("sources")
            .join("books")
            .join("工程 原件.pdf");
        assert_eq!(
            root_relative_path(&root, &relative).unwrap(),
            "sources/books/工程 原件.pdf"
        );
        assert!(
            root_relative_path(&root, &root.path().join("sources").join("..").join("var")).is_err()
        );
        #[cfg(not(windows))]
        assert_eq!(
            root_relative_path(
                &root,
                &root.path().join("sources").join("books").join("part:1.pdf")
            )
            .unwrap(),
            "sources/books/part:1.pdf"
        );
    }

    #[test]
    fn legacy_packets_above_query_limit_keep_unknown_registration_without_index_reads() {
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let root = SkillRoot::new(producer).unwrap();
        for (catalog, status) in [
            ("local_pdf:historical", "not_bound"),
            ("historical_supplier", "not_applicable"),
        ] {
            let mut results = (0..128)
                .map(|n| json!({"id":format!("legacy-{n}"),"catalog":catalog}))
                .collect::<Vec<_>>();
            let report = audit(&root, &results).unwrap();
            assert_eq!(report["status"], status);
            assert_eq!(report["candidates"].as_array().unwrap().len(), 128);
            assert!(
                report["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|c| c["status"] == status)
            );
            // A late malformed declaration cannot hide behind the larger packet.
            results[127]["source_registration_binding"] = json!({"schema":BINDING_SCHEMA});
            assert!(audit(&root, &results).is_err());
        }
    }
}
