//! Selected original CAD archives and their full member inventory. This reader
//! adapts the original misumi-incad mirror/CAD ZIP rules; it never runs a CAD API.
use super::{MAX_FILE, MAX_TOTAL, Reader, VerifiedIntake, inventory, path, text};
use anyhow::{Context, Result, ensure};
use oe_core::{FileIdentity, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::PathBuf,
};

pub struct SelectedFile {
    pub relative_path: String,
    pub role: &'static str,
    pub format: String,
    pub bytes: Vec<u8>,
    pub identity: FileIdentity,
}
/// Only the original verifier constructs this current, source-bound selection.
pub struct VerifiedSelection {
    files: Vec<SelectedFile>,
    sources: BTreeMap<PathBuf, FileIdentity>,
    audit: Value,
}
impl VerifiedSelection {
    pub fn files(&self) -> &[SelectedFile] {
        &self.files
    }
    pub fn audit(&self) -> &Value {
        &self.audit
    }
    pub fn ensure_current(&self) -> Result<()> {
        for (p, wanted) in &self.sources {
            let raw = read_regular_bytes(p, MAX_FILE)?;
            ensure!(
                raw.len() as u64 == wanted.bytes && sha256(raw) == wanted.sha256,
                "incad_cad_original_changed:{}",
                p.display()
            );
        }
        Ok(())
    }
}

/// Verify every member of each selected format, retaining the original ZIP.
/// `formats` selects original supplier option codes, never infers a file type.
pub fn verify(intake: &VerifiedIntake, formats: &[String]) -> Result<VerifiedSelection> {
    ensure!(
        !formats.is_empty() && formats.len() <= 16,
        "incad_cad_format_budget"
    );
    let mut unique = BTreeSet::new();
    for code in formats {
        ensure!(
            !code.is_empty()
                && code.len() <= 64
                && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && unique.insert(code.clone()),
            "incad_cad_format_invalid_or_duplicate"
        );
    }
    intake.ensure_current()?;
    let downloads = intake.case_record["cad_downloads"]
        .as_array()
        .context("incad_cad_downloads_missing")?;
    ensure!(
        downloads.len() <= 256,
        "incad_cad_download_inventory_budget"
    );
    let catalog = inventory(&intake.case_manifest)?;
    let mut reader = Reader {
        sources: BTreeMap::new(),
        total: 0,
    };
    let mut files = Vec::new();
    let mut selections = Vec::new();
    let mut selected_names = BTreeSet::new();
    let mut payload_total = 0usize;
    for code in formats {
        let candidates: Vec<_> = downloads
            .iter()
            .filter(|d| d["code"].as_str() == Some(code))
            .collect();
        ensure!(
            candidates.len() == 1,
            "incad_cad_option_missing_or_ambiguous:{code}"
        );
        let d = candidates[0];
        ensure!(
            d["status"] == "downloaded",
            "incad_cad_option_not_downloaded:{code}"
        );
        let original = text(d, "original")?;
        ensure!(
            original == format!("cad/{code}/original.zip"),
            "incad_cad_archive_layout_changed"
        );
        let zip_bytes = reader.member(&intake.case_folder, &catalog, original)?;
        ensure!(
            d["original_bytes"].as_u64() == Some(zip_bytes.len() as u64)
                && text(d, "original_sha256")? == sha256(&zip_bytes),
            "incad_cad_archive_record_mismatch"
        );
        let members = d["members"]
            .as_array()
            .context("incad_cad_member_inventory_missing")?;
        ensure!(
            !members.is_empty() && members.len() <= 1000,
            "incad_cad_member_budget"
        );
        let prefix = format!("cad/{code}/files/");
        let mut declared = BTreeMap::new();
        for member in members {
            let name = text(member, "archive_name")?;
            path(&intake.case_folder, name)?;
            ensure!(
                text(member, "path")? == format!("{prefix}{name}"),
                "incad_cad_member_path_record_mismatch"
            );
            ensure!(
                declared.insert(name.to_owned(), member).is_none(),
                "incad_cad_duplicate_declared_member"
            );
        }
        let mut zip =
            zip::ZipArchive::new(Cursor::new(&zip_bytes)).context("incad_cad_zip_invalid")?;
        ensure!(
            !zip.is_empty() && zip.len() <= 1000,
            "incad_cad_zip_inventory_budget"
        );
        let mut zip_names = BTreeSet::new();
        let mut actual_members = BTreeSet::new();
        let mut rows = Vec::new();
        let mut expanded = 0usize;
        for i in 0..zip.len() {
            let mut member = zip.by_index(i).context("incad_cad_zip_member_invalid")?;
            let name = member.name().to_owned();
            let checked = name.strip_suffix('/').unwrap_or(&name);
            path(&intake.case_folder, checked)?;
            ensure!(
                !checked.chars().any(char::is_control) && zip_names.insert(checked.to_lowercase()),
                "incad_cad_zip_duplicate_or_control_path"
            );
            let mode = member.unix_mode().unwrap_or(0) & 0o170000;
            ensure!(
                mode == 0 || mode == 0o100000 || (member.is_dir() && mode == 0o040000),
                "incad_cad_zip_member_type_invalid"
            );
            if member.is_dir() {
                ensure!(member.size() == 0, "incad_cad_zip_directory_payload");
                continue;
            }
            let expected = declared
                .get(&name)
                .context("incad_cad_zip_undeclared_member")?;
            let size = usize::try_from(member.size())?;
            ensure!(size > 0 && size <= MAX_FILE, "incad_cad_member_size_budget");
            expanded = expanded
                .checked_add(size)
                .context("incad_cad_expanded_size_overflow")?;
            ensure!(expanded <= MAX_TOTAL, "incad_cad_expanded_total_budget");
            let mut raw = Vec::new();
            (&mut member)
                .take(size as u64 + 1)
                .read_to_end(&mut raw)
                .context("incad_cad_zip_crc_or_read_failure")?;
            ensure!(
                raw.len() == size
                    && expected["bytes"].as_u64() == Some(size as u64)
                    && text(expected, "sha256")? == sha256(&raw),
                "incad_cad_zip_record_identity_mismatch"
            );
            let relative = text(expected, "path")?;
            let disk = reader.member(&intake.case_folder, &catalog, relative)?;
            ensure!(disk == raw, "incad_cad_unpacked_bytes_differ");
            ensure!(
                actual_members.insert(name.clone()) && selected_names.insert(relative.to_owned()),
                "incad_cad_selected_path_duplicate"
            );
            payload_total = payload_total
                .checked_add(size)
                .context("incad_cad_payload_overflow")?;
            ensure!(
                payload_total <= MAX_TOTAL && files.len() < 1024,
                "incad_cad_selection_total_budget"
            );
            let identity = FileIdentity {
                bytes: size as u64,
                sha256: sha256(&raw),
            };
            rows.push(json!({"archive_name":name,"path":relative,"identity":identity,
                "original_inspection":expected["inspection"],"inspection_authority":"retained_supplier_archive_record",
                "zip_crc_rechecked":true,"native_validation":"not_run"}));
            files.push(SelectedFile {
                relative_path: relative.into(),
                role: "archive_member",
                format: code.clone(),
                bytes: raw,
                identity,
            });
        }
        ensure!(
            actual_members.len() == declared.len()
                && declared.keys().all(|k| actual_members.contains(k)),
            "incad_cad_archive_inventory_incomplete"
        );
        drop(zip);
        ensure!(
            selected_names.insert(original.to_owned()),
            "incad_cad_duplicate_archive"
        );
        payload_total = payload_total
            .checked_add(zip_bytes.len())
            .context("incad_cad_payload_overflow")?;
        ensure!(
            payload_total <= MAX_TOTAL && files.len() < 1024,
            "incad_cad_selection_total_budget"
        );
        let identity = FileIdentity {
            bytes: zip_bytes.len() as u64,
            sha256: sha256(&zip_bytes),
        };
        let archive_record = catalog
            .get(original)
            .context("incad_cad_archive_not_in_manifest")?;
        selections.push(json!({"supplier_option":code,"supplier_label":text(d,"label")?,
            "archive":{ "path":original,"identity":identity,"source_url":archive_record["source_url"]},
            "members":rows,"member_count":declared.len(),"original_native_validation":d["native_validation"],
            "file_format_and_units":"native_reader_required","complete_selected_archive_inventory":true}));
        files.push(SelectedFile {
            relative_path: original.into(),
            role: "original_archive",
            format: code.clone(),
            bytes: zip_bytes,
            identity,
        });
    }
    let audit = json!({"schema":"oev2.incad-cad-selection-audit/1","case_number":intake.case_record["number"],
        "selections":selections,"files":files.iter().map(|f|json!({"path":f.relative_path,"role":f.role,"format":f.format,"identity":f.identity})).collect::<Vec<_>>(),
        "file_count":files.len(),"payload_bytes":payload_total,"source_files":reader.sources,
        "bom":{ "original":intake.case_record["official_bom"],"visible":intake.case_record["visible_bom"],
            "link_urls":intake.case_record["bom_product_urls"],"interpretation":"not_reviewed"},
        "original_archive_status":intake.audit["archive_status"],"reference_exposure":"development_replay",
        "role":"supplier_reference_assets","project_mutated":false,"native_execution":"not_run",
        "assembly_reference_closure":"not_verified_by_native_reader","engineering_applicability":"not_reviewed","adopted":false});
    let result = VerifiedSelection {
        files,
        sources: reader.sources,
        audit,
    };
    intake.ensure_current()?;
    result.ensure_current()?;
    Ok(result)
}
