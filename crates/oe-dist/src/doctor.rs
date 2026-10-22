//! Runtime identity has one implementation owner. Reuse the installed RECORD,
//! wheel, bounded external probe and lock verification in oe-semantic::runtime.
pub use oe_semantic::runtime::{
    Check, InstalledRuntimeObservation, RuntimeSourceLock, RuntimeVerification, doctor_preflight,
    probe_external_runtime, read_runtime_source_lock, read_wheel_identity,
    verify_installed_runtime_record, verify_runtime_source_identity,
};
