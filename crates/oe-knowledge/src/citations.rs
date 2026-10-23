//! Original-byte references and whole-page views; no visual interpretation.
use crate::{
    poppler::{self, PopplerLimits, PopplerTools},
    source_pack::{self, Stage, file_hash, publish_same_or_new, within_root},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Maximum full original PDF size; source-pack members have a separate 256MiB cap.
pub const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

pub fn file_uri(path: &Path) -> Result<String> {
    ensure!(path.is_absolute(), "source_uri_requires_absolute_path");
    let text = path.to_str().context("source_path_not_utf8")?;
    let mut uri = String::from("file://");
    for b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(b) {
            uri.push(*b as char)
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(uri)
}
fn source_path(root: &Path, page: &Value) -> Result<PathBuf> {
    let path = within_root(
        root,
        Path::new(page["source_pdf"].as_str().context("source_pdf_missing")?),
    )?;
    let root = oe_core::SkillRoot::new(root)?;
    ensure!(
        path.starts_with(root.path().join("sources"))
            || path.starts_with(root.path().join("var/cache/knowledge")),
        "source_path_outside_library_or_cache"
    );
    Ok(path)
}

/// Validate only locator claims and their relationship to the candidate. No file I/O.
pub(crate) fn declared_locator(result: &Value) -> Result<Value> {
    let sha = result["source_sha256"]
        .as_str()
        .context("source_locator_hash_missing")?;
    ensure!(
        source_pack::valid_sha(sha),
        "source_locator_document_changed"
    );
    let locator = result.get("source_locator").filter(|x| !x.is_null());
    ensure!(
        !result["catalog"]
            .as_str()
            .is_some_and(|s| s.starts_with("local_pdf:"))
            || locator.is_some(),
        "source_locator_missing"
    );
    let declared = if let Some(locator) = locator {
        ensure!(
            locator["schema"] == "ontology-engineering.source-page/v1"
                && locator["kind"] == "local_pdf",
            "source_locator_schema_or_kind_mismatch"
        );
        let page = locator["physical_page"]
            .as_u64()
            .context("source_locator_page_mismatch")?;
        let count = locator["page_count"]
            .as_u64()
            .context("source_locator_page_mismatch")?;
        ensure!(
            page > 0
                && page <= count
                && result["pdf_page"].as_u64() == Some(page)
                && locator["document_sha256"] == sha,
            "source_locator_page_mismatch"
        );
        for k in ["document_id", "title", "edition"] {
            ensure!(
                locator[k].as_str().is_some_and(|s| !s.trim().is_empty()),
                "source_locator_identity_missing"
            );
        }
        ensure!(
            locator["printed_page_status"] == "unverified"
                && locator.get("printed_page") == Some(&Value::Null)
                && result.get("printed_page") == Some(&Value::Null),
            "source_locator_printed_page_not_verified"
        );
        locator.clone()
    } else {
        ensure!(
            result["pdf_page"].as_u64().is_some_and(|p| p > 0),
            "source_view_requires_explicit_page_locator"
        );
        Value::Null
    };
    Ok(declared)
}

pub fn verify_locator(root: &Path, result: &Value) -> Result<Value> {
    verify_locator_with_tools(root, result, &PopplerTools::default())
}
pub fn verify_locator_with_tools(
    root: &Path,
    result: &Value,
    tools: &PopplerTools,
) -> Result<Value> {
    let path = source_path(root, result)?;
    let sha = result["source_sha256"]
        .as_str()
        .context("source_locator_hash_missing")?;
    let original = file_hash(&path, MAX_SOURCE_BYTES)?;
    ensure!(
        source_pack::valid_sha(sha) && original.0 == sha,
        "source_locator_document_changed"
    );
    if let Some(bytes) = result.get("source_bytes") {
        ensure!(
            bytes.as_u64() == Some(original.1),
            "source_locator_size_mismatch"
        );
    }
    let info = poppler::inspect(root, &path, tools, &PopplerLimits::default())?;
    let verified = declared_locator(result)?;
    if verified.is_null() {
        ensure!(
            info.page_count == 1,
            "source_view_requires_explicit_page_locator"
        );
    } else {
        ensure!(
            verified["page_count"].as_u64() == Some(info.page_count),
            "source_locator_page_mismatch"
        );
    }
    ensure!(
        file_hash(&path, MAX_SOURCE_BYTES)? == original,
        "source_locator_document_changed"
    );
    Ok(verified)
}

/// The provider reloads the index row before this function. This boundary then
/// verifies the actual file and physical locator independently of caller text.
pub fn resolved_result(
    root: &Path,
    page: &Value,
    query: &str,
    tools: &PopplerTools,
) -> Result<Value> {
    let mut result = page.clone();
    ensure!(result.is_object(), "source_page_not_object");
    let body = page["body"].as_str().context("source_body_missing")?;
    ensure!(
        page["text_sha256"] == oe_core::sha256(body),
        "source_text_hash_mismatch"
    );
    if page["source_kind"] == "local_pdf" || page["provider"] == "local_pdf" {
        result
            .as_object_mut()
            .expect("validated object")
            .remove("source_relative_path");
        let source = &page["source_registration"];
        ensure!(
            source["sha256"] == page["source_sha256"],
            "local_pdf_source_identity_mismatch"
        );
        result["catalog_title"] = source["title"].clone();
        result["printed_page"] = Value::Null;
        result["source_locator"] = json!({"schema":"ontology-engineering.source-page/v1","kind":"local_pdf","document_id":source["id"],"document_sha256":source["sha256"],"physical_page":page["pdf_page"],"page_count":source["page_count"],"title":source["title"],"edition":source["edition"],"printed_page":null,"printed_page_status":"unverified"});
        result["text_origin"] = json!("poppler_native_text");
        result["source_url"] = json!("");
        result["series_references"] = json!([]);
    } else if page["source_pack_manifest"].is_string() {
        let manifest = Path::new(
            page["source_pack_manifest"]
                .as_str()
                .context("source_pack_manifest_missing")?,
        );
        let restored = source_pack::restore_selected(
            root,
            manifest,
            page["source_relative_path"]
                .as_str()
                .context("source_relative_path_missing")?,
            page["source_sha256"]
                .as_str()
                .context("source_hash_missing")?,
        )?;
        result["source_pdf"] = json!(restored.path);
        result["source_file_page"] = json!(1);
        result["catalog_source_locator"] = page["source_locator"].clone();
        result["source_locator"] = Value::Null;
        let pack = source_pack::load_manifest(root, manifest)?;
        let catalog = &page["catalog_registration"];
        let relative = catalog["pdf_relative_path"]
            .as_str()
            .context("assembled_pdf_locator_missing")?;
        source_pack::relative_path(relative)?;
        let assembly = pack["assembled_pdfs"]
            .as_array()
            .context("source_pack_assembled_inventory_missing")?
            .iter()
            .find(|a| a["catalog"] == page["catalog"] && a["path"] == relative)
            .context("source_pack_assembled_identity_mismatch")?;
        ensure!(
            assembly["sha256"] == catalog["pdf_sha256"],
            "source_pack_assembled_identity_mismatch"
        );
        let assembled = within_root(
            root,
            &manifest
                .parent()
                .context("source_pack_parent")?
                .join("cache/archive")
                .join(relative),
        )?;
        let exists = assembled.exists();
        let verified = exists
            && file_hash(&assembled, MAX_SOURCE_BYTES).is_ok_and(|(h, n)| {
                assembly["sha256"] == h && assembly["bytes"].as_u64() == Some(n)
            });
        let packed = pack
            .get("full_pdfs_archive")
            .and_then(|v| v["path"].as_str())
            .map(|p| {
                source_pack::relative_path(p)
                    .and_then(|p| within_root(root, &manifest.parent().expect("parent").join(p)))
            })
            .transpose()?
            .is_some_and(|p| p.is_file());
        result["assembled_pdf"] = json!(
            assembled
                .strip_prefix(oe_core::SkillRoot::new(root)?.path())?
                .to_str()
                .context("source_path_not_utf8")?
        );
        let manifest_absolute = within_root(root, manifest)?;
        result["source_pack_manifest"] = json!(
            manifest_absolute
                .strip_prefix(oe_core::SkillRoot::new(root)?.path())?
                .to_str()
                .context("source_path_not_utf8")?
        );
        result["source_pack_manifest_sha256"] =
            json!(file_hash(&manifest_absolute, source_pack::MAX_MANIFEST_BYTES as u64)?.0);
        result["assembled_relative_path"] = json!(relative);
        result["assembled_pdf_exists"] = json!(exists);
        result["assembled_pdf_status"] = json!(if verified {
            "verified_present"
        } else if exists {
            "hash_mismatch"
        } else if packed {
            "not_restored"
        } else {
            "not_downloaded"
        });
        result["assembled_pdf_uri"] = if verified {
            json!(file_uri(&assembled)?)
        } else {
            Value::Null
        };
        result["assembled_pdf_download_url"] =
            assembly.get("download_url").cloned().unwrap_or(Value::Null);
        result["assembled_pdf_sha256_at_index_build"] = catalog["pdf_sha256"].clone();
        result["series_references"] = page
            .get("series_json")
            .and_then(Value::as_str)
            .map(|s| oe_core::json::strict_json(s.as_bytes()))
            .transpose()?
            .unwrap_or(json!([]));
    } else {
        let archive = within_root(
            root,
            Path::new(
                page["source_archive_root"]
                    .as_str()
                    .context("source_archive_root_missing")?,
            ),
        )?;
        let skill = oe_core::SkillRoot::new(root)?;
        ensure!(
            archive.starts_with(skill.path().join("sources")),
            "source_archive_outside_library"
        );
        let relative = page["source_relative_path"]
            .as_str()
            .context("source_relative_path_missing")?;
        source_pack::relative_path(relative)?;
        result["source_pdf"] = json!(within_root(root, &archive.join(relative))?);
        result["source_file_page"] = json!(1);
        result["catalog_source_locator"] = page["source_locator"].clone();
        result["source_locator"] = Value::Null;
        let catalog = &page["catalog_registration"];
        let assembled_relative = catalog["pdf_relative_path"]
            .as_str()
            .context("assembled_pdf_locator_missing")?;
        source_pack::relative_path(assembled_relative)?;
        let assembled = within_root(root, &archive.join(assembled_relative))?;
        let exists = assembled.try_exists()?;
        let verified = exists
            && file_hash(&assembled, MAX_SOURCE_BYTES)
                .is_ok_and(|(h, _)| catalog["pdf_sha256"] == h);
        result["assembled_pdf"] = json!(
            assembled
                .strip_prefix(skill.path())?
                .to_str()
                .context("source_path_not_utf8")?
        );
        result["assembled_relative_path"] = json!(assembled_relative);
        result["assembled_pdf_exists"] = json!(exists);
        result["assembled_pdf_status"] = json!(if verified {
            "verified_present"
        } else if exists {
            "hash_mismatch"
        } else {
            "not_downloaded"
        });
        result["assembled_pdf_uri"] = if verified {
            json!(file_uri(&assembled)?)
        } else {
            Value::Null
        };
        result["assembled_pdf_download_url"] = Value::Null;
        result["assembled_pdf_sha256_at_index_build"] = catalog["pdf_sha256"].clone();
        result["series_references"] = page
            .get("series_json")
            .and_then(Value::as_str)
            .map(|s| oe_core::json::strict_json(s.as_bytes()))
            .transpose()?
            .unwrap_or(json!([]));
        result
            .as_object_mut()
            .expect("validated object")
            .remove("source_archive_root");
    }
    let locator = verify_locator_with_tools(root, &result, tools)?;
    let path = source_path(root, &result)?;
    let file_page = locator["physical_page"].as_u64().unwrap_or(1);
    result["pdf_uri"] = if let Some(uri) = result["assembled_pdf_uri"].as_str() {
        json!(format!("{uri}#page={}", page["pdf_page"]))
    } else {
        json!(format!("{}#page={file_page}", file_uri(&path)?))
    };
    result["headings"] = page
        .get("titles_json")
        .and_then(Value::as_str)
        .map(|s| oe_core::json::strict_json(s.as_bytes()))
        .transpose()?
        .unwrap_or(json!([]))
        .as_array()
        .context("source_titles_invalid")?
        .iter()
        .map(|x| x.get("text").cloned().unwrap_or(Value::Null))
        .collect::<Vec<_>>()
        .into();
    result["native_text_missing"] = json!(body.trim().is_empty());
    result["navigation_only"] = json!(body.trim().is_empty());
    result["native_text_sparse"] = json!(
        page["native_text_sparse"]
            .as_bool()
            .unwrap_or_else(|| page["native_text_sparse"].as_i64().unwrap_or(0) != 0)
    );
    result["excerpt"] = crate::text::excerpt(body, query, 700);
    result["parameter_verification"] = json!({"status":"not_run","product_match":"unknown"});
    result["original_verified"] = json!(true);
    let canonical_root = oe_core::SkillRoot::new(root)?;
    result["source_pdf"] = json!(
        path.strip_prefix(canonical_root.path())?
            .to_str()
            .context("source_path_not_utf8")?
    );
    Ok(result)
}

pub fn view(root: &Path, result: &Value, dpi: u32, tools: &PopplerTools) -> Result<Value> {
    ensure!((72..=300).contains(&dpi), "source_view_dpi_out_of_range");
    let locator = verify_locator_with_tools(root, result, tools)?;
    let source = source_path(root, result)?;
    let original = file_hash(&source, MAX_SOURCE_BYTES)?;
    let page = locator["physical_page"].as_u64().unwrap_or(1);
    let rendered = poppler::render(root, &source, page, dpi, tools)?;
    ensure!(
        file_hash(&source, MAX_SOURCE_BYTES)? == original,
        "source_view_source_changed"
    );
    let identity = json!({"source_sha256":original.0,"source_file_page":page,"dpi":dpi,"renderer":rendered.tool.version,"renderer_identity":rendered.tool,"whole_page":true});
    let key = oe_core::json::canonical_digest(&identity)?;
    let image = within_root(
        root,
        &PathBuf::from(format!("var/cache/knowledge/page-views/{key}.png")),
    )?;
    let record = image.with_extension("json");
    let sha = oe_core::sha256(&rendered.bytes);
    let valid = json!({"identity":identity,"image_sha256":sha,"width":rendered.width,"height":rendered.height});
    let stage = Stage::new(root, "view")?;
    let staged = stage.path.join("page.png");
    oe_core::write_new_bytes(&staged, &rendered.bytes)?;
    publish_same_or_new(&staged, &image, &sha, rendered.bytes.len() as u64)?;
    let mut raw = serde_json::to_vec_pretty(&valid)?;
    raw.push(b'\n');
    let staged_record = stage.path.join("page.json");
    oe_core::write_new_bytes(&staged_record, &raw)?;
    publish_same_or_new(
        &staged_record,
        &record,
        &oe_core::sha256(&raw),
        raw.len() as u64,
    )?;
    Ok(
        json!({"schema":"ontology-engineering.source-page-view/v1","identity":valid["identity"],"image_sha256":sha,"width":rendered.width,"height":rendered.height,"source_locator":locator,"source_pdf":source,"source_pdf_uri":format!("{}#page={page}",file_uri(&source)?),"catalog_pdf_page":result["pdf_page"],"printed_page":result["printed_page"],"image":image,"image_uri":file_uri(&image)?,"interpretation":"not_run","numeric_verification":"not_run"}),
    )
}
