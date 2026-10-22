//! Bounded derived design artifact profile, shared by build and runtime readers.
//! This is an explicit resource change, not an engineering acceptance rule.
use std::{ffi::OsStr, path::Path};
pub const PROFILE: &str = "oev2.derived-design-model-resource/2";
pub const PUBLIC_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const DERIVED_MODEL_BYTES: u64 = 32 * 1024 * 1024;
/// Only the canonical public artifact obtains the derived-model resource limit.
/// No basename, extension, dot-component or caller-selected private input exception.
pub fn public_file_bytes(relative: &Path) -> u64 {
    let name = relative.as_os_str();
    let canonical = name == OsStr::new("generated/model.ttl");
    #[cfg(windows)]
    let canonical = canonical || name == OsStr::new("generated\\model.ttl");
    if canonical {
        DERIVED_MODEL_BYTES
    } else {
        PUBLIC_FILE_BYTES
    }
}
/// Generation-stage relative role. Nested/renamed artifacts keep the default.
pub fn generated_file_bytes(relative: &Path) -> u64 {
    if relative.as_os_str() == OsStr::new("model.ttl") {
        DERIVED_MODEL_BYTES
    } else {
        PUBLIC_FILE_BYTES
    }
}
