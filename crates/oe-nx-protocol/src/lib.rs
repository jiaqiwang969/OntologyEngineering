//! Shared NX v2 wire integrity and single-dispatch rules. No filesystem, process,
//! network, SDK or semantic execution lives here. OS callers supply observed
//! identities and inventories; validation cannot prove those observations true.
pub mod bundle;
pub mod frame;
pub mod identity;
pub mod job;
pub mod legacy;
pub mod legacy_root;
pub mod response;
pub mod segmented;
pub mod state;
pub mod writer_release;
use oe_wire::unicode_casefold;

/// Keep the NX record budget at this boundary while sharing the parser.
pub mod json {
    pub fn strict_json(bytes: &[u8]) -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(
            bytes.len() <= super::MAX_RECORD_BYTES,
            "record exceeds bound"
        );
        Ok(oe_wire::json::strict_json(bytes)?)
    }
}

pub use bundle::Package;
pub use frame::*;
pub use identity::*;
pub use job::*;
pub use legacy::*;
pub use legacy_root::*;
pub use response::*;
pub use state::*;
pub use writer_release::*;

pub const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PAYLOAD_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_FILES: usize = 10_000;
pub const PATH_COMPARISON_UNICODE_VERSION: &str = unicode_casefold::UNICODE_VERSION;

pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> anyhow::Result<T> {
    anyhow::ensure!(bytes.len() <= MAX_RECORD_BYTES, "record exceeds bound");
    Ok(serde_json::from_value(json::strict_json(bytes)?)?)
}

/// Canonical wire-record digest, not an executable/source-file byte digest.
/// Records use strings and integer identities; opaque worker parameters remain
/// separately hashed raw files and are never normalized by this function.
pub fn record_digest<T: serde::Serialize>(record: &T) -> anyhow::Result<Digest> {
    let mut value = serde_json::to_value(record)?;
    fn sort(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                for item in fields.values_mut() {
                    sort(item);
                }
                fields.sort_keys();
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(sort),
            _ => {}
        }
    }
    sort(&mut value);
    Ok(Digest::of(&serde_json::to_vec(&value)?))
}

pub mod transfer;

pub mod native_capacity;
pub use native_capacity::{LARGE_JOB_SCHEMA, NativeAuxiliaryCapacity, NativeCapacity};

pub mod published_payload;

pub mod published_state;
pub use published_state::{PUBLISHED_STATE_SCHEMA, PublishedStageBinding};

pub mod published_stage;

pub mod large_fetch;
