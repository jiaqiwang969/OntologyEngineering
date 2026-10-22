//! A child-only DLL search path; the controller and parent environment stay intact.
use anyhow::Result;
use std::{ffi::OsString, path::Path};

pub fn worker_search_path(nx_root: &Path, inherited: Option<OsString>) -> Result<OsString> {
    let mut paths = vec![nx_root.join("NXBIN")];
    if let Some(value) = inherited {
        paths.extend(std::env::split_paths(&value));
    }
    Ok(std::env::join_paths(paths)?)
}
