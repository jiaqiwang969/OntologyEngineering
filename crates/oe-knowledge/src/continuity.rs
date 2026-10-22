//! Verify private source copies; never execute or write the historical root.
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, fingerprint_file, write_new};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

fn walk(root: &Path, path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    oe_core::no_symlinks(path)?;
    for item in std::fs::read_dir(path)? {
        let p = item?.path();
        oe_core::no_symlinks(&p)?;
        if p.is_dir() {
            walk(root, &p, out)?;
        } else if p.is_file() {
            out.push(p.strip_prefix(root)?.to_owned());
        } else {
            anyhow::bail!("continuity_special_file");
        }
        ensure!(out.len() <= 10000, "continuity_file_budget");
    }
    Ok(())
}
fn acquire(source: &Path, destination: &Path) -> Result<&'static str> {
    oe_core::ensure_directory(destination.parent().context("continuity_parent")?)?;
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let a = std::ffi::CString::new(source.as_os_str().as_bytes())?;
        let b = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
        if unsafe { libc::clonefile(a.as_ptr(), b.as_ptr(), 0) } == 0 {
            return Ok("apfs_copy_on_write");
        }
        ensure!(!destination.exists(), "continuity_destination_exists");
    }
    let mut input = oe_core::open_regular_file(source, 4 * 1024 * 1024 * 1024)?;
    let mut output = oe_core::create_new_regular_file(destination)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    Ok("bounded_regular_copy")
}
/// A stage is published only after every acquired source has the expected bytes.
pub fn receive(root: &SkillRoot, old: &Path) -> Result<Value> {
    let old = SkillRoot::new(old)?;
    ensure!(old.path() != root.path(), "continuity_same_root");
    let stage = root.new_private_dir(Path::new(&format!(
        "var/builds/source-receive-{}",
        std::process::id()
    )))?;
    let mut files = Vec::new();
    walk(old.path(), &old.path().join("sources"), &mut files)?;
    for volume in ["ontology-engineering-book", "product-trustworthiness-book"] {
        walk(
            old.path(),
            &old.path().join("references").join(volume),
            &mut files,
        )?;
    }
    for p in [
        "runtime/source-delivery.json",
        "references/source-map.md",
        "references/product-trustworthiness-source-map.md",
        "references/engineering-iteration.md",
        "references/practice-consolidation.md",
        "skills/engineering-evidence-methods/SKILL.md",
        "skills/cad-agent/SKILL.md",
        "skills/manufacturing-process-cost/SKILL.md",
        "runtime/vendor/semantica-0.6.5+oe.7-py3-none-any.whl",
    ] {
        files.push(PathBuf::from(p));
    }
    files.sort();
    files.dedup();
    let mut records = Vec::new();
    for path in files {
        let source = old.path().join(&path);
        let before = fingerprint_file(&source, 4 * 1024 * 1024 * 1024)?;
        let relative = if path.starts_with("sources") {
            path.strip_prefix("sources")?.to_owned()
        } else if path.starts_with("references/ontology-engineering-book")
            || path.starts_with("references/product-trustworthiness-book")
        {
            PathBuf::from("book-authoring").join(&path)
        } else {
            PathBuf::from("history").join(&path)
        };
        let dest = stage.join(&relative);
        let mode = acquire(&source, &dest)?;
        ensure!(
            fingerprint_file(&dest, 4 * 1024 * 1024 * 1024)? == before
                && fingerprint_file(&source, 4 * 1024 * 1024 * 1024)? == before,
            "continuity_source_changed"
        );
        records.push(json!({"historical_path":oe_core::portable_source_path(old.path(),&source)?,"path":oe_core::portable_source_path(&stage,&dest)?,"sha256":before.sha256,"bytes":before.bytes,"acquisition":mode}));
    }
    let result = json!({"schema":"oev2.source-continuity/1","historical_root":old.path(),"source_files":records,
        "historical_identity":"original byte identities retained; no execution receipt re-signed","source_use":"retrieval only; original edition/page/extraction identities preserved","ontology_completion":"textbook whole-book ontologization not implemented","source_root":"sources","publication":"not_performed","originals_modified":false});
    write_new(&stage.join("continuity.json"), &result)?;
    oe_core::rename_directory_exclusive(&stage, &root.path().join("sources"))?;
    Ok(result)
}
pub fn book_root(root: &SkillRoot) -> Result<PathBuf> {
    let path = root.path().join("sources/book-authoring");
    if path.is_dir() {
        oe_core::no_symlinks(&path)?;
        Ok(path)
    } else {
        Ok(root.path().to_owned())
    }
}

/// Supplement missing method resources without replacing an earlier receipt or
/// turning historical Python/Fusion commands into execution authorization.
pub fn receive_methods(root: &SkillRoot, old: &Path) -> Result<Value> {
    let old = SkillRoot::new(old)?;
    ensure!(old.path() != root.path(), "continuity_same_root");
    let mut paths = vec![PathBuf::from("SKILL.md")];
    for relative in [
        "skills/cad-agent/SKILL.md",
        "skills/cad-agent/CONTRIBUTING.md",
        "skills/cad-agent/AGENTS.md",
        "references/supplier-knowledge.md",
    ] {
        paths.push(relative.into());
    }
    for directory in [
        "skills/cad-agent/references",
        "skills/cad-agent/mechanism",
        "skills/cad-agent/assembly",
        "skills/engineering-evidence-methods/references",
        "skills/manufacturing-process-cost/references",
    ] {
        let path = old.path().join(directory);
        if path.is_dir() {
            walk(old.path(), &path, &mut paths)?;
        }
    }
    paths.retain(|p| {
        matches!(
            p.extension().and_then(|x| x.to_str()),
            Some("md" | "json" | "yaml" | "toml" | "txt")
        )
    });
    paths.sort();
    paths.dedup();
    ensure!(paths.len() <= 1024, "method_source_file_budget");
    let mut inputs = Vec::new();
    let mut total = 0usize;
    for path in &paths {
        let bytes = oe_core::read_regular_bytes(&old.path().join(path), 16 * 1024 * 1024)?;
        total = total
            .checked_add(bytes.len())
            .context("method_source_byte_budget")?;
        ensure!(total <= 64 * 1024 * 1024, "method_source_byte_budget");
        let destination = root.path().join("sources/history").join(path);
        if destination.exists() {
            ensure!(
                oe_core::read_regular_bytes(&destination, 16 * 1024 * 1024)? == bytes,
                "method_existing_copy_drift"
            );
        }
        inputs.push((path.clone(), bytes));
    }
    // Check every collision before adding a file; commit the receipt last.
    let mut records = Vec::new();
    let mut added = 0;
    for (path, bytes) in &inputs {
        let destination = root.path().join("sources/history").join(path);
        let already = destination.exists();
        if !already {
            oe_core::ensure_directory(destination.parent().context("method_parent")?)?;
            oe_core::write_new_bytes(&destination, bytes)?;
            added += 1;
        }
        ensure!(
            oe_core::read_regular_bytes(&destination, 16 * 1024 * 1024)? == *bytes
                && oe_core::read_regular_bytes(&old.path().join(path), 16 * 1024 * 1024)? == *bytes,
            "method_source_changed"
        );
        records.push(json!({"historical_path":path,"path":oe_core::portable_source_path(root.path(),&destination)?,"sha256":oe_core::sha256(bytes),"bytes":bytes.len(),"acquisition":if already{"verified_existing"}else{"verified_regular_copy"}}));
    }
    let mut links = Vec::new();
    let mut seen = BTreeSet::new();
    for (path, bytes) in &inputs {
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        for tail in std::str::from_utf8(bytes)?.split("](").skip(1) {
            let target = tail
                .split(')')
                .next()
                .unwrap_or("")
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(['<', '>']);
            if target.is_empty()
                || target.contains("://")
                || !seen.insert((path.clone(), target.to_owned()))
            {
                continue;
            }
            let resolved = path.parent().unwrap_or(Path::new("")).join(target);
            let original = old.path().join(&resolved);
            let identity = original
                .canonicalize()
                .ok()
                .filter(|p| p.starts_with(old.path()) && p.is_file())
                .and_then(|p| fingerprint_file(&p, 16 * 1024 * 1024).ok().map(|i| (p, i)));
            let row = if let Some((p, i)) = identity {
                let relative = p.strip_prefix(old.path())?;
                let copy = root.path().join("sources/history").join(relative);
                let same = fingerprint_file(&copy, 16 * 1024 * 1024).is_ok_and(|x| x == i);
                json!({"from":path,"target":target,"historical_resolved":relative,"original_sha256":i.sha256,"copy":oe_core::portable_source_path(root.path(),&copy)?,"status":if same{"verified"}else{"not_acquired"}})
            } else {
                json!({"from":path,"target":target,"status":"historical_target_unavailable_or_outside_root"})
            };
            links.push(row);
        }
    }
    let report = json!({"schema":"oev2.method-continuity/1","source_files":records,"added_files":added,"links":links,"historical_identity":"original files retained; old commands and Semantica/Fusion authority are historical text only","formal_execution":"not_run","native_tools":"not_verified_by_copy","publication":"not_performed","originals_modified":false});
    let receipt = root.new_private_dir(Path::new(&format!(
        "var/source-receipts/methods-{}",
        std::process::id()
    )))?;
    write_new(&receipt.join("receipt.json"), &report)?;
    Ok(report)
}

/// Read and verify each methodological chapter actually used by the pattern map.
pub fn mapped_sources(root: &SkillRoot) -> Result<Value> {
    let raw = oe_core::read_regular_bytes(
        &root
            .path()
            .join("references/judgment-pattern-source-map.json"),
        4 * 1024 * 1024,
    )?;
    let map = oe_core::json::strict_json(&raw)?;
    let mut rows = Vec::new();
    for (id, source) in map["sources"]
        .as_object()
        .context("continuity_map_missing")?
    {
        let path = source["path"].as_str().context("continuity_source_path")?;
        oe_semantic::safe_relative(path)?;
        let resolved = book_root(root)?.join(path);
        let result = oe_core::read_regular_bytes(&resolved, 16 * 1024 * 1024);
        let row = match result {
            Ok(bytes) => {
                json!({"id":id,"historical_path":path,"current_path":oe_core::portable_source_path(root.path(),&resolved)?,
                "expected_sha256":source["sha256"],"sha256":oe_core::sha256(&bytes),"status":if source["sha256"]==oe_core::sha256(&bytes){"verified"}else{"drift"},
                "role":source["role"],"text_bytes":bytes.len()})
            }
            Err(_) => {
                json!({"id":id,"historical_path":path,"expected_sha256":source["sha256"],"status":"unavailable"})
            }
        };
        rows.push(row);
    }
    Ok(
        json!({"schema":"oev2.mapped-source-continuity/1","source_map_sha256":oe_core::sha256(raw),
        "status":if rows.iter().all(|r|r["status"]=="verified"){"verified"}else{"incomplete"},"sources":rows,"engineering_adoption":"not_established"}),
    )
}
