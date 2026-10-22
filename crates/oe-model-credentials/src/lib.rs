//! Explicit owner-selected credentials and ordinary chat/embedding transport.
//! No specialized judgment provider, automatic credential discovery, or account rotation.
mod credential_file;
mod pool;
pub mod provider_transport;
mod retry;
pub use credential_file::CredentialFileError;
pub use credential_file::CredentialFileError as CredentialError;
pub use pool::{CredentialLease, CredentialPool, PoolError, PoolStatus};
pub use retry::{retry_after, retry_after_at};
