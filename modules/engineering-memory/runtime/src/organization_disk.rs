//! Small durable records, with no aggregate JSON document or task-size ceiling.
use crate::store::{id, private_dir, sha};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, Write},
    path::{Path, PathBuf},
};

/// New records have no inherited 16 MiB aggregate ceiling. Source/event/work
/// collections are individual files. Caller controls request page sizes.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "organization_record_symlink"
    );
    Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("organization_parent_missing")?;
    private_dir(parent)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let stage = parent.join(format!(
        ".{}.{}.{stamp}.pending",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&stage)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&stage, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn directory(home: &Path, task: &str) -> Result<PathBuf> {
    Ok(home.join("organizations").join(id(task)?))
}
pub fn lock(dir: &Path, name: &str) -> Result<File> {
    private_dir(dir)?;
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(format!("{}.lock", id(name)?)))?;
    f.try_lock_exclusive().context("organization_owner_busy")?;
    Ok(f)
}
pub fn immutable<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if path.exists() {
        let old: Value = read_json(path)?;
        ensure!(
            old == serde_json::to_value(value)?,
            "immutable_identity_changed:{}",
            path.display()
        );
        return Ok(());
    }
    atomic_json(path, value)
}
pub fn key(value: &impl Serialize) -> Result<String> {
    Ok(sha(serde_json::to_vec(value)?))
}
pub fn event(dir: &Path, kind: &str, payload: Value) -> Result<String> {
    // Content identity makes replay after any checkpoint idempotent.
    let data =
        json!({"schema":"engineering-memory.organization-event/1","kind":kind,"payload":payload});
    let identity = key(&data)?;
    immutable(&dir.join("events").join(format!("{identity}.json")), &data)?;
    Ok(identity)
}
pub fn each<T: DeserializeOwned>(
    dir: &Path,
    mut visitor: impl FnMut(T) -> Result<()>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|s| s == "json") && path.is_file() {
            visitor(read_json(&path)?)?;
        }
    }
    Ok(())
}
