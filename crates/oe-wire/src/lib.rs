//! Shared byte and text identities. No filesystem, database, process, network,
//! model decisions or executable semantic authority is selected by this crate.
pub mod csv;
pub mod json;
pub mod unicode_casefold;

use sha2::{Digest, Sha256};

pub fn sha256(data: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(data.as_ref()))
}
