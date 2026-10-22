//! Rust continuation of the frozen NX drawing-review packet method.
//! Hash/inventory/issue joins do not re-render drawings or certify manufacture.
use anyhow::{Context, Result, ensure};
use oe_core::read_regular_bytes;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
const RECORD_LIMIT: usize = 16 * 1024 * 1024;
pub mod associations;
pub mod diameter;
pub mod drafting;
pub mod intent;
pub mod pdf;
pub mod retirement;
fn s<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("drawing_string_missing:{key}"))
}
fn a<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v[key]
        .as_array()
        .with_context(|| format!("drawing_array_missing:{key}"))
}
fn hash(root: &Path, name: &str) -> Result<String> {
    Ok(oe_core::sha256(read_regular_bytes(
        &member(root, name)?,
        512 * 1024 * 1024,
    )?))
}
fn empty(v: &Value) -> bool {
    v.is_null()
        || v == false
        || v.as_str() == Some("")
        || v.as_array().is_some_and(Vec::is_empty)
        || v.as_object().is_some_and(|x| x.is_empty())
}
/// Normalize an existing Windows source identity, not a current local path.
pub fn windows_identity(s: &str) -> Result<String> {
    ensure!(
        !s.trim().is_empty() && !s.chars().any(char::is_control),
        "drawing_windows_identity_missing"
    );
    let replaced = s.replace('\\', "/");
    let mut parts = Vec::new();
    for p in replaced.split('/') {
        if p == "." || p.is_empty() {
            continue;
        }
        if p == ".." {
            if parts.last().is_some_and(|p| *p != "..") {
                parts.pop();
            } else {
                parts.push(p);
            }
        } else {
            parts.push(p);
        }
    }
    Ok(oe_wire::unicode_casefold::casefold(&parts.join("/")))
}
pub fn member(root: &Path, name: &str) -> Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    // Historical exporters use both separators, but packet members never
    // permit roots, drives, traversal, NUL or NTFS alternative data streams.
    ensure!(
        !normalized.is_empty()
            && !normalized.starts_with('/')
            && !normalized.contains(':')
            && normalized.split('/').all(|p| !p.is_empty()
                && p != "."
                && p != ".."
                && !p.chars().any(char::is_control)),
        "drawing_unsafe_member"
    );
    let path = root.join(normalized);
    oe_core::no_symlinks(&path)?;
    Ok(path)
}
fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) -> Result<()> {
    oe_core::no_symlinks(dir)?;
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        let meta = std::fs::symlink_metadata(&p)?;
        ensure!(!meta.file_type().is_symlink(), "drawing_linked_member");
        if meta.is_dir() {
            walk(root, &p, out)?;
        } else {
            ensure!(meta.is_file(), "drawing_nonregular_member");
            let name = p
                .strip_prefix(root)?
                .to_str()
                .context("drawing_member_not_utf8")?
                .replace('\\', "/");
            if name != "SHA256SUMS.txt" {
                out.insert(name);
            }
            ensure!(out.len() <= 10_000, "drawing_packet_file_budget");
        }
    }
    Ok(())
}
/// Existing packet bytes can be read from a historical root. The CLI writes
/// only a new v2 receipt; old observations retain their original identity.
pub fn audit_packet(root: &Path) -> Result<Value> {
    oe_core::no_symlinks(root)?;
    let manifest = read_regular_bytes(&root.join("SHA256SUMS.txt"), RECORD_LIMIT)?;
    let text = std::str::from_utf8(&manifest)?.trim_start_matches('\u{feff}');
    let mut sums = BTreeMap::new();
    let mut aliases = BTreeSet::new();
    for line in text.lines() {
        let (digest, name) = line.split_once("  ").context("drawing_hash_line_invalid")?;
        ensure!(
            digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "drawing_hash_invalid"
        );
        let path = member(root, name)?;
        let rel = path
            .strip_prefix(root)?
            .to_str()
            .context("drawing_path_not_utf8")?
            .replace('\\', "/");
        ensure!(
            aliases.insert(windows_identity(&rel)?),
            "drawing_case_collision"
        );
        ensure!(
            hash(root, &rel)? == digest.to_ascii_lowercase(),
            "drawing_packet_hash_mismatch:{rel}"
        );
        ensure!(
            sums.insert(rel, digest.to_ascii_lowercase()).is_none(),
            "drawing_duplicate_member"
        );
    }
    let mut actual = BTreeSet::new();
    walk(root, root, &mut actual)?;
    ensure!(
        actual == sums.keys().cloned().collect(),
        "drawing_packet_inventory_not_closed"
    );
    let read = |name: &str| -> Result<Value> {
        ensure!(sums.contains_key(name), "drawing_record_not_hash_bound");
        let raw = read_regular_bytes(&member(root, name)?, RECORD_LIMIT)?;
        Ok(oe_wire::json::strict_json(
            raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&raw),
        )?)
    };
    let native = read("验证记录/原生导出记录.json")?;
    let pdfs = read("验证记录/PDF检查.json")?;
    ensure!(
        native["finished"] == true
            && native["export_complete"] == true
            && a(&native, "errors")?.is_empty(),
        "drawing_native_export_incomplete"
    );
    let mut sets = BTreeMap::new();
    let mut expected = BTreeSet::new();
    for spec in a(&native, "sets")? {
        let name = s(spec, "set")?;
        ensure!(
            sets.insert(name, spec).is_none(),
            "drawing_duplicate_model_set"
        );
        let inspected = a(spec, "inspected_part_files")?
            .iter()
            .map(|v| windows_identity(v.as_str().context("drawing_part_identity_invalid")?))
            .collect::<Result<BTreeSet<_>>>()?;
        ensure!(
            inspected.len() == a(spec, "inspected_part_files")?.len()
                && Some(inspected.len() as u64) == spec["part_file_count"].as_u64(),
            "drawing_prt_scan_incomplete"
        );
        ensure!(
            a(spec, "load_errors")?.is_empty(),
            "drawing_prt_load_errors"
        );
        let inventory = spec["drawing_inventory"]
            .as_object()
            .context("drawing_inventory_missing")?;
        for (filename, sheets) in inventory {
            let file = windows_identity(filename)?;
            ensure!(inspected.contains(&file), "drawing_part_absent_from_scan");
            for sheet in sheets.as_array().context("drawing_sheets_invalid")? {
                let sheet = sheet.as_str().context("drawing_sheet_identity_missing")?;
                ensure!(
                    expected.insert((name.to_owned(), file.clone(), sheet.to_owned())),
                    "drawing_duplicate_native_sheet"
                );
            }
        }
    }
    let mut parts = BTreeMap::new();
    for part in a(&native, "parts")? {
        ensure!(
            parts
                .insert(
                    (
                        s(part, "set")?.to_owned(),
                        windows_identity(s(part, "file")?)?
                    ),
                    part
                )
                .is_none(),
            "drawing_duplicate_part_record"
        );
    }
    let mut checks = BTreeMap::new();
    for check in pdfs.as_array().context("drawing_pdf_checks_invalid")? {
        let name = s(check, "file")?.replace('\\', "/");
        member(root, &name)?;
        ensure!(
            sums.get(&name).map(String::as_str) == Some(s(check, "sha256")?),
            "drawing_pdf_observation_hash_mismatch"
        );
        ensure!(
            checks.insert(name, check).is_none(),
            "drawing_duplicate_pdf_check"
        );
    }
    ensure!(
        sums.contains_key("审核清单.csv"),
        "drawing_csv_not_hash_bound"
    );
    let csv = read_regular_bytes(&member(root, "审核清单.csv")?, RECORD_LIMIT)?;
    let rows = oe_wire::csv::rows(std::str::from_utf8(&csv)?.trim_start_matches('\u{feff}'))?;
    let header = rows.first().context("drawing_csv_empty")?;
    ensure!(
        header.iter().collect::<BTreeSet<_>>().len() == header.len(),
        "drawing_duplicate_csv_column"
    );
    let mut seen = BTreeSet::new();
    let mut links = BTreeSet::new();
    let mut counts = BTreeMap::from([
        ("primary_sheets", 0),
        ("association_review_sheets", 0),
        ("layout_review_sheets", 0),
        ("blank_template_sheets", 0),
        ("priority_review_sheets", 0),
        ("baseline_comparisons", 0),
        ("independent_sheets", 0),
    ]);
    for (index, row) in rows.iter().skip(1).enumerate() {
        ensure!(row.len() == header.len(), "drawing_csv_width_mismatch");
        let row: BTreeMap<&str, &str> = header
            .iter()
            .zip(row)
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let cell = |key: &str| -> Result<&str> {
            row.get(key)
                .copied()
                .with_context(|| format!("drawing_csv_field_missing:{key}"))
        };
        ensure!(
            cell("序号")?.parse::<usize>()? == index + 1,
            "drawing_index_nonsequential"
        );
        let key = (
            cell("机型")?.to_owned(),
            windows_identity(&format!("{}.prt", cell("零件")?))?,
            cell("图纸页")?.to_owned(),
        );
        ensure!(
            expected.contains(&key) && seen.insert(key.clone()),
            "drawing_duplicate_or_unexpected_sheet"
        );
        let part = parts
            .get(&(key.0.clone(), key.1.clone()))
            .context("drawing_native_part_missing")?;
        let spec = sets.get(key.0.as_str()).context("drawing_set_missing")?;
        let assembly = a(spec, "assembly_part_files")?
            .iter()
            .map(|v| windows_identity(v.as_str().context("drawing_assembly_identity_invalid")?))
            .collect::<Result<BTreeSet<_>>>()?;
        let referenced = assembly.contains(&key.1);
        ensure!(
            (cell("总装引用")? == "是") == referenced,
            "drawing_assembly_membership_mismatch"
        );
        if !referenced {
            *counts.get_mut("independent_sheets").unwrap() += 1;
        }
        let version = |kind: &str| -> Result<Option<&Value>> {
            let matches = a(part, kind)?
                .iter()
                .filter(|e| e["sheet"] == key.2)
                .collect::<Vec<_>>();
            ensure!(matches.len() <= 1, "drawing_duplicate_pdf_version");
            Ok(matches.first().copied())
        };
        let baseline = version("baseline_pdfs")?;
        let updated = version("updated_pdfs")?;
        let primary = updated.or(baseline).context("drawing_pdf_export_missing")?;
        let target = cell("图纸PDF")?.replace('\\', "/");
        member(root, &target)?;
        ensure!(!links.contains(&target), "drawing_pdf_reused");
        ensure!(
            sums.get(&target).map(String::as_str) == Some(s(primary, "sha256")?),
            "drawing_primary_pdf_sheet_mismatch"
        );
        let check = checks
            .get(&target)
            .context("drawing_content_observation_missing")?;
        let status = s(check, "content_status")?;
        ensure!(
            ["blank_template", "drawing_content_present"].contains(&status),
            "drawing_content_unknown"
        );
        let blank = status == "blank_template";
        let layout = check["layout_review_required"]
            .as_bool()
            .context("drawing_layout_observation_missing")?;
        let review = part["review_required"]
            .as_bool()
            .context("drawing_native_review_status_missing")?;
        let after = part["after"]
            .as_object()
            .context("drawing_after_observation_missing")?;
        // Unknown/stale observations cannot be represented by omitted arrays.
        ensure!(
            after.get("stale_views").is_some_and(Value::is_array)
                && after
                    .get("retained_dimensions")
                    .is_some_and(Value::is_array),
            "drawing_association_observation_missing"
        );
        let association = review
            || !empty(&part["errors"])
            || !empty(&part["refresh_error"])
            || !empty(&part["after"]["stale_views"])
            || !empty(&part["after"]["retained_dimensions"])
            || updated.is_none();
        let priority = blank || layout || association;
        let label = if blank {
            "空白图框，需确认或补图"
        } else if association {
            "优先核对"
        } else if layout {
            "优先核对（排版）"
        } else {
            "已刷新，待工程审核"
        };
        ensure!(cell("审核状态")? == label, "drawing_issue_label_mismatch");
        ensure!(
            !layout || cell("重点核对原因")?.contains("图面排版："),
            "drawing_layout_reason_missing"
        );
        links.insert(target);
        let comparison = cell("刷新前对照PDF")?;
        ensure!(
            !((association || layout) && baseline.is_some() && updated.is_some())
                || !comparison.is_empty(),
            "drawing_flagged_baseline_missing"
        );
        if !comparison.is_empty() {
            let name = comparison.replace('\\', "/");
            member(root, &name)?;
            ensure!(
                !links.contains(&name) && checks.contains_key(&name),
                "drawing_comparison_reused_or_unobserved"
            );
            ensure!(
                sums.get(&name).map(String::as_str)
                    == Some(s(
                        baseline.context("drawing_baseline_export_missing")?,
                        "sha256"
                    )?),
                "drawing_baseline_sheet_mismatch"
            );
            links.insert(name);
            *counts.get_mut("baseline_comparisons").unwrap() += 1;
        }
        for (name, yes) in [
            ("primary_sheets", true),
            ("association_review_sheets", association),
            ("layout_review_sheets", layout),
            ("blank_template_sheets", blank),
            ("priority_review_sheets", priority),
        ] {
            if yes {
                *counts.get_mut(name).unwrap() += 1;
            }
        }
    }
    ensure!(seen == expected, "drawing_inventory_omitted_sheets");
    ensure!(
        links == checks.keys().cloned().collect(),
        "drawing_pdf_observation_inventory_mismatch"
    );
    ensure!(
        Some(seen.len() as u64) == native["sheet_count"].as_u64(),
        "drawing_reported_sheet_count_mismatch"
    );
    // Re-read hashes before reporting a completed audit, including the exact
    // inventory record. This is a byte join, not a new native/render receipt.
    for (name, digest) in &sums {
        ensure!(
            &hash(root, name)? == digest,
            "drawing_packet_changed_during_audit"
        );
    }
    ensure!(
        read_regular_bytes(&root.join("SHA256SUMS.txt"), RECORD_LIMIT)? == manifest,
        "drawing_manifest_changed_during_audit"
    );
    let mut result = json!({"schema":"oev2.drawing-packet-audit/1","packet_integrity":"verified","engineering_acceptance":"not_assessed",
        "native_reexecution":false,"visual_reinspection":false,"verified_files":sums.len()+1,"verified_pdf_links":links.len(),
        "source_identity":"original native/PDF observation and delivery bytes; no historical verdict resigned",
        "scope":"file hashes, complete sheet inventory, native-export/PDF joins and preserved issue labels"});
    for (key, value) in counts {
        result[key] = json!(value);
    }
    Ok(result)
}
