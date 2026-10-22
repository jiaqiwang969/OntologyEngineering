//! Inspect acquired bytes without asserting native or engineering acceptance.
use anyhow::{Context, Result, bail, ensure};
use oe_core::{FileIdentity, fingerprint_file, no_symlinks, read_regular_bytes};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const LIMIT: usize = 50_000_000;
pub const MEMBER_LIMIT: usize = 64;

#[derive(Debug)]
pub struct InspectedDownload {
    pub receipt: Value,
    pub blob: Vec<u8>,
    pub files: BTreeMap<String, Vec<u8>>,
}

pub fn validate_part(part: &str) -> Result<()> {
    ensure!(
        !part.is_empty()
            && part.len() <= 120
            && part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
            && part != "."
            && part != "..",
        "confirmed_simple_part_required"
    );
    Ok(())
}

pub fn file_kind(label: &str) -> &'static str {
    let text = label.trim().to_ascii_uppercase();
    for family in [
        "STEP",
        "PARASOLID",
        "IGES",
        "ACIS",
        "DXF",
        "DWG",
        "PDF",
        "NX",
    ] {
        if text.starts_with(family) {
            return family;
        }
    }
    "OTHER"
}

fn safe_basename(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 255
            && !name.chars().any(|c| c.is_control())
            && !name.contains(['/', '\\', ':'])
            && !matches!(name, "." | "..")
            && !name.ends_with(['.', ' ']),
        "unsafe_supplier_filename"
    );
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    ensure!(
        !reserved.contains(&stem.as_str()),
        "unsafe_supplier_filename"
    );
    Ok(())
}

fn part_filename(name: &str, part: &str) -> bool {
    name.get(..part.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(part))
        && name
            .as_bytes()
            .get(part.len())
            .is_none_or(|c| b"_. -".contains(c))
}

fn suffix(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn ascii_trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |n| n + 1);
    &bytes[start..end]
}

fn skip_space(bytes: &[u8], mut at: usize) -> Result<usize> {
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at..at + 2) != Some(b"/*") {
            return Ok(at);
        }
        let end = bytes[at + 2..]
            .windows(2)
            .position(|w| w == b"*/")
            .context("malformed_STEP_comment")?;
        at += end + 4;
    }
}

fn quoted(bytes: &[u8], at: usize) -> Result<(Vec<u8>, usize)> {
    ensure!(bytes.get(at) == Some(&b'\''), "STEP_string_required");
    let mut result = Vec::new();
    let mut pos = at + 1;
    while let Some(c) = bytes.get(pos) {
        if *c == b'\'' {
            if bytes.get(pos + 1) == Some(&b'\'') {
                result.push(b'\'');
                pos += 2;
                continue;
            }
            return Ok((result, pos + 1));
        }
        result.push(*c);
        pos += 1;
    }
    bail!("unterminated_STEP_string")
}

/// Identify actual STEP tokens, excluding comments and quoted strings.
fn call_starts(bytes: &[u8], name: &[u8]) -> Result<Vec<usize>> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        at = skip_space(bytes, at)?;
        if at >= bytes.len() {
            break;
        }
        if bytes[at] == b'\'' {
            at = quoted(bytes, at)?.1;
            continue;
        }
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            while bytes
                .get(at)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                at += 1;
            }
            if &bytes[start..at] == name {
                let pos = skip_space(bytes, at)?;
                if bytes.get(pos) == Some(&b'(') {
                    result.push(pos + 1);
                }
            }
        } else {
            at += 1;
        }
    }
    Ok(result)
}

fn unit_definition(blob: &[u8]) -> Result<bool> {
    for at in call_starts(blob, b"SI_UNIT")? {
        let mut pos = skip_space(blob, at)?;
        if blob.get(pos..pos + 7) != Some(b".MILLI.") {
            continue;
        }
        pos = skip_space(blob, pos + 7)?;
        if blob.get(pos) != Some(&b',') {
            continue;
        }
        pos = skip_space(blob, pos + 1)?;
        if blob.get(pos..pos + 7) != Some(b".METRE.") {
            continue;
        }
        pos = skip_space(blob, pos + 7)?;
        if blob.get(pos) == Some(&b')') {
            return Ok(true);
        }
    }
    Ok(false)
}

fn contains(blob: &[u8], token: &[u8]) -> bool {
    blob.windows(token.len()).any(|s| s == token)
}

/// Actual lexical STEP envelope and schema only. Assemblies can contain many
/// PRODUCT declarations; part identity/topology/units require separate owners.
pub fn inspect_step_header(blob: &[u8]) -> Result<Value> {
    ensure!(
        !blob.is_empty() && blob.len() <= LIMIT,
        "empty_or_oversized_supplier_file"
    );
    let leading = ascii_trim(blob);
    ensure!(
        leading.starts_with(b"ISO-10303-21;") && leading.ends_with(b"END-ISO-10303-21;"),
        "incomplete_STEP"
    );
    let schemas = call_starts(blob, b"FILE_SCHEMA")?;
    ensure!(schemas.len() == 1, "STEP_schema_missing_or_ambiguous");
    let at = skip_space(blob, schemas[0])?;
    ensure!(blob.get(at) == Some(&b'('), "STEP_schema_missing");
    let schema = quoted(blob, skip_space(blob, at + 1)?)?.0;
    ensure!(
        !schema.is_empty() && schema.is_ascii(),
        "STEP_schema_invalid"
    );
    Ok(
        json!({"actual_file_schema":String::from_utf8(schema)?,"inspection_scope":"lexical_envelope_and_schema",
        "units":"native_reader_required","native_geometry_validation":"not_run"}),
    )
}

pub fn inspect_file(name: &str, blob: &[u8], part: &str, selected_label: &str) -> Result<Value> {
    validate_part(part)?;
    safe_basename(name)?;
    ensure!(
        !selected_label.trim().is_empty() && selected_label.chars().count() <= 1000,
        "selected_format_required"
    );
    ensure!(
        !blob.is_empty() && blob.len() <= LIMIT,
        "empty_or_oversized_supplier_file"
    );
    let leading = ascii_trim(blob);
    let prefix = leading
        .iter()
        .take(32)
        .map(u8::to_ascii_lowercase)
        .collect::<Vec<_>>();
    ensure!(
        !prefix.starts_with(b"<!doctype html") && !prefix.starts_with(b"<html"),
        "HTML_is_not_the_requested_artifact"
    );
    let family = file_kind(selected_label);
    let ext = suffix(name);
    let extensions: &[&str] = match family {
        "STEP" => &["stp", "step"],
        "PARASOLID" => &["x_t", "x_b", "xmt_txt", "xmt_bin"],
        "IGES" => &["igs", "iges"],
        "ACIS" => &["sat", "sab"],
        "DXF" => &["dxf"],
        "DWG" => &["dwg"],
        "PDF" => &["pdf"],
        "NX" => &["prt"],
        _ => &[],
    };
    ensure!(
        extensions.is_empty() || extensions.contains(&ext.as_str()),
        "selected_format_and_file_extension_mismatch"
    );
    let mut info = json!({"name":name,"bytes":blob.len(),"sha256":oe_core::sha256(blob),
        "requested_format":selected_label,"format_family":family,"format_check":"native_reader_required",
        "units":"unknown","part_identity":"page_and_filename_only","engineering_acceptance":"not_evaluated"});
    match family {
        "STEP" => {
            info["schema"] = inspect_step_header(blob)?["actual_file_schema"].clone();
            let products = call_starts(blob, b"PRODUCT")?;
            ensure!(products.len() == 1, "STEP_product_identity_mismatch");
            let product = quoted(blob, skip_space(blob, products[0])?)?.0;
            ensure!(
                product == part.as_bytes() || product == format!("{part}_1").as_bytes(),
                "STEP_product_identity_mismatch"
            );
            info["product"] = String::from_utf8(product)?.into();
            info["part_identity"] = "exact_or_observed_single_part_suffix".into();
            info["format_check"] = "STEP_envelope_and_schema_present".into();
            info["millimetre_unit_definition_present"] = unit_definition(blob)?.into();
        }
        "DXF" => {
            if blob.starts_with(b"AutoCAD Binary DXF") {
                info["format_check"] = "binary_DXF_signature_present".into();
            } else {
                ensure!(
                    [b"SECTION".as_slice(), b"HEADER", b"EOF"]
                        .iter()
                        .all(|s| contains(blob, s)),
                    "DXF_signature_missing"
                );
                info["format_check"] = "ASCII_DXF_structure_present".into();
                let fields = blob
                    .split(|b| b.is_ascii_whitespace())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                let codes = fields
                    .windows(3)
                    .filter(|s| s[0] == b"$INSUNITS" && s[1] == b"70")
                    .collect::<Vec<_>>();
                if codes.len() == 1
                    && let Ok(code) = std::str::from_utf8(codes[0][2])
                        .unwrap_or("")
                        .parse::<u32>()
                {
                    info["drawing_unit_code"] = code.into();
                    if code == 4 {
                        info["units"] = "mm".into();
                    }
                }
            }
        }
        "DWG" => {
            ensure!(
                blob.len() >= 6
                    && &blob[..4] == b"AC10"
                    && blob[4..6].iter().all(u8::is_ascii_digit),
                "DWG_signature_missing"
            );
            info["format_check"] = "DWG_signature_present".into();
        }
        "PDF" => {
            ensure!(
                blob.starts_with(b"%PDF-")
                    && contains(&blob[blob.len().saturating_sub(2048)..], b"%%EOF"),
                "incomplete_PDF"
            );
            info["format_check"] = "PDF_envelope_present".into();
        }
        "IGES" => {
            let sections = blob
                .split(|b| *b == b'\n')
                .filter_map(|line| line.get(72).copied())
                .collect::<BTreeSet<_>>();
            ensure!(
                b"SGDPT".iter().all(|b| sections.contains(b)),
                "IGES_sections_missing"
            );
            info["format_check"] = "IGES_sections_present".into();
        }
        _ => {}
    }
    Ok(info)
}

pub fn inspect_download(
    filename: &str,
    blob: Vec<u8>,
    part: &str,
    selected_label: &str,
) -> Result<InspectedDownload> {
    validate_part(part)?;
    safe_basename(filename)?;
    ensure!(
        part_filename(filename, part),
        "download_part_filename_mismatch"
    );
    ensure!(
        !blob.is_empty() && blob.len() <= LIMIT,
        "supplier_download_too_large_or_empty"
    );
    let zipped = filename.to_ascii_lowercase().ends_with(".zip");
    let mut files = BTreeMap::new();
    if zipped {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(&blob)).context("supplier_archive_invalid")?;
        ensure!(
            !archive.is_empty() && archive.len() <= MEMBER_LIMIT,
            "supplier_archive_limits_exceeded"
        );
        let mut total = 0_u64;
        let mut names = BTreeSet::new();
        for index in 0..archive.len() {
            let mut member = archive
                .by_index(index)
                .context("supplier_archive_member_invalid")?;
            let name = member.name().to_owned();
            // A directory may not hide traversal, unsupported Unix types or repeated names.
            let checked = name.strip_suffix('/').unwrap_or(&name);
            safe_basename(checked)?;
            ensure!(
                names.insert(checked.to_lowercase()),
                "duplicate_supplier_archive_member"
            );
            let mode = member.unix_mode().unwrap_or(0) & 0o170000;
            ensure!(
                mode == 0 || mode == 0o100000 || (mode == 0o040000 && member.is_dir()),
                "unsafe_archive_member_type"
            );
            total = total
                .checked_add(member.size())
                .context("supplier_archive_size_overflow")?;
            ensure!(total <= LIMIT as u64, "supplier_archive_limits_exceeded");
            if member.is_dir() {
                continue;
            }
            ensure!(part_filename(&name, part), "archive_member_part_mismatch");
            let mut bytes = Vec::new();
            (&mut member)
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .context("archive_CRC_or_read_failure")?;
            ensure!(
                bytes.len() <= LIMIT && bytes.len() as u64 == member.size(),
                "supplier_archive_member_size_mismatch"
            );
            files.insert(name, bytes);
        }
    } else {
        files.insert(filename.to_owned(), blob.clone());
    }
    ensure!(!files.is_empty(), "empty_supplier_archive");
    let receipts = files
        .iter()
        .map(|(name, bytes)| inspect_file(name, bytes, part, selected_label))
        .collect::<Result<Vec<_>>>()?;
    let receipt = json!({"part":part,"download_name":filename,"bytes":blob.len(),"sha256":oe_core::sha256(&blob),
        "zip_crc_valid":if zipped {json!(true)}else{Value::Null},"files":receipts,"status":"artifact_acquired",
        "engineering_acceptance":"not_evaluated"});
    Ok(InspectedDownload {
        receipt,
        blob,
        files,
    })
}

/// Historical AP203 comparison only. Millimetre entities do not establish model units.
pub fn inspect_ap203_archive(blob: Vec<u8>, part: &str) -> Result<InspectedDownload> {
    ensure!(blob.len() <= 20_000_000, "archive_too_large");
    {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(&blob)).context("supplier_archive_invalid")?;
        ensure!(archive.len() == 1, "expected_one_part_STEP");
        let member = archive
            .by_index(0)
            .context("supplier_archive_member_invalid")?;
        ensure!(
            member.name() == format!("{part}.stp") && member.size() <= 20_000_000,
            "part_or_member_path_mismatch"
        );
    }
    let inspected = inspect_download(&format!("{part}_STEP.zip"), blob, part, "STEP AP203")?;
    ensure!(
        inspected.files.len() == 1 && inspected.files.contains_key(&format!("{part}.stp")),
        "expected_one_part_STEP"
    );
    let file = &inspected.receipt["files"][0];
    ensure!(
        file["schema"] == "CONFIG_CONTROL_DESIGN",
        "STEP_AP203_schema_missing"
    );
    ensure!(file["product"] == part, "STEP_product_identity_mismatch");
    ensure!(
        file["millimetre_unit_definition_present"] == true,
        "STEP_millimetre_unit_definition_missing"
    );
    ensure!(
        inspected.files.values().next().unwrap().len() <= 20_000_000,
        "STEP_too_large"
    );
    Ok(inspected)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified_ns: u128,
    device: u64,
    inode: u64,
}

fn stamp(path: &Path) -> Result<Stamp> {
    no_symlinks(path)?;
    let meta = fs::symlink_metadata(path)?;
    ensure!(meta.is_file(), "supplier_download_not_regular_file");
    #[cfg(unix)]
    let (device, inode) = {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino())
    };
    #[cfg(not(unix))]
    let (device, inode) = (0, 0);
    Ok(Stamp {
        size: meta.len(),
        modified_ns: meta.modified()?.duration_since(UNIX_EPOCH)?.as_nanos(),
        device,
        inode,
    })
}

#[derive(Debug)]
pub struct FreshDownload {
    directory: PathBuf,
    part: String,
    label: String,
    before: BTreeMap<String, Stamp>,
    last: BTreeMap<String, Stamp>,
    ap203_only: bool,
}

impl FreshDownload {
    pub fn new(directory: &Path, part: &str, selected_label: &str) -> Result<Self> {
        validate_part(part)?;
        no_symlinks(directory)?;
        ensure!(directory.is_dir(), "supplier_download_directory_required");
        let mut result = Self {
            directory: fs::canonicalize(directory)?,
            part: part.into(),
            label: selected_label.into(),
            before: BTreeMap::new(),
            last: BTreeMap::new(),
            ap203_only: false,
        };
        result.before = result.candidates()?;
        Ok(result)
    }

    pub fn new_ap203(directory: &Path, part: &str) -> Result<Self> {
        let mut watch = Self::new(directory, part, "STEP AP203")?;
        watch.ap203_only = true;
        watch.before = watch.candidates()?;
        Ok(watch)
    }

    fn matching_name(&self, name: &str) -> bool {
        if !part_filename(name, &self.part) {
            return false;
        }
        if !self.ap203_only {
            return true;
        }
        let lower = name.to_ascii_lowercase();
        let prefix = format!("{}_step", self.part.to_ascii_lowercase());
        let Some(middle) = lower
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(".zip"))
        else {
            return false;
        };
        middle.is_empty()
            || middle
                .strip_prefix(" (")
                .and_then(|s| s.strip_suffix(')'))
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    }

    fn candidates(&self) -> Result<BTreeMap<String, Stamp>> {
        no_symlinks(&self.directory)?;
        let mut result = BTreeMap::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("supplier_download_filename_utf8_required"))?;
            if !self.matching_name(&name)
                || [".crdownload", ".tmp"].iter().any(|s| name.ends_with(s))
            {
                continue;
            }
            ensure!(
                !entry.file_type()?.is_symlink(),
                "supplier_download_symlink_rejected"
            );
            if !entry.file_type()?.is_file() {
                continue;
            }
            safe_basename(&name)?;
            result.insert(name, stamp(&entry.path())?);
        }
        Ok(result)
    }

    pub fn baseline(&self) -> Value {
        Value::Object(self.before.iter().map(|(name,s)| (name.clone(),json!({"bytes":s.size,"modified_ns":s.modified_ns.to_string(),"device":s.device,"inode":s.inode}))).collect())
    }

    /// Reject a racing file before dispatch even when it has not yet settled.
    pub fn has_changes(&self) -> Result<bool> {
        for entry in fs::read_dir(&self.directory)? {
            let name = entry?.file_name();
            let name = name
                .to_str()
                .context("supplier_download_filename_utf8_required")?;
            let base = name
                .strip_suffix(".crdownload")
                .or_else(|| name.strip_suffix(".tmp"))
                .unwrap_or(name);
            if self.matching_name(base)
                && [".crdownload", ".tmp"]
                    .iter()
                    .any(|suffix| name.ends_with(suffix))
            {
                return Ok(true);
            }
        }
        Ok(self
            .candidates()?
            .iter()
            .any(|(name, stamp)| self.before.get(name) != Some(stamp)))
    }

    pub fn poll(&mut self) -> Result<Option<InspectedDownload>> {
        let current = self.candidates()?;
        let mut fresh = current
            .into_iter()
            .filter(|(name, s)| self.before.get(name) != Some(s))
            .collect::<Vec<_>>();
        if fresh
            .iter()
            .any(|(name, _)| name.to_ascii_lowercase().ends_with(".zip"))
        {
            fresh.retain(|(name, _)| name.to_ascii_lowercase().ends_with(".zip"));
        }
        ensure!(fresh.len() <= 1, "multiple_new_downloads_ambiguous");
        let Some((name, state)) = fresh.pop() else {
            return Ok(None);
        };
        let path = self.directory.join(&name);
        if self
            .directory
            .join(format!("{name}.crdownload"))
            .symlink_metadata()
            .is_ok()
            || self.last.get(&name) != Some(&state)
        {
            self.last.insert(name, state);
            return Ok(None);
        }
        ensure!(state.size <= LIMIT as u64, "supplier_download_too_large");
        let identity: FileIdentity = fingerprint_file(&path, LIMIT as u64)?;
        let blob = read_regular_bytes(&path, LIMIT)?;
        if stamp(&path)? != state || identity.sha256 != oe_core::sha256(&blob) {
            return Ok(None);
        }
        let mut inspected = if self.ap203_only {
            inspect_ap203_archive(blob, &self.part)?
        } else {
            inspect_download(&name, blob, &self.part, &self.label)?
        };
        inspected.receipt["download_name"] = name.into();
        inspected.receipt["original_download"] = path
            .to_str()
            .context("supplier_download_path_utf8_required")?
            .into();
        inspected.receipt["file_stamp"] = json!({"bytes":state.size,"modified_ns":state.modified_ns.to_string(),"device":state.device,"inode":state.inode});
        Ok(Some(inspected))
    }
}
