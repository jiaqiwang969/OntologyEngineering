//! Thin audit of Semantica's 29-chapter package registry.
//!
//! Ported from scripts/validate_chapter_contracts.py. Delegates all
//! contract validation to the native Semantica runtime via NativeRuntime.
//! No OE ledger is read; no semantic rules are executed here.

use anyhow::Result;
use serde_json::{Value, json};

pub const EXPECTED_CHAPTER_COUNT: usize = 29;

// ---------------------------------------------------------------------------
// Validation report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub contract_count: usize,
    pub errors: Vec<String>,
    pub blockers: Vec<String>,
    pub complete_contracts: Vec<String>,
}

impl ValidationReport {
    pub fn structurally_valid(&self) -> bool {
        self.errors.is_empty() && self.contract_count == EXPECTED_CHAPTER_COUNT
    }

    pub fn release_ready(&self) -> bool {
        self.structurally_valid() && self.blockers.is_empty()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "contract_count": self.contract_count,
            "structurally_valid": self.structurally_valid(),
            "release_ready": self.release_ready(),
            "complete_contracts": self.complete_contracts,
            "blocked_contract_count": self.blockers.len(),
            "errors": self.errors,
            "blockers": self.blockers,
        })
    }
}

// ---------------------------------------------------------------------------
// Validate via native runtime
// ---------------------------------------------------------------------------

/// Delegate all contract validation to the source-locked Semantica runtime.
/// Invokes `list_chapter_packages` and `validate_chapter_registry` operations.
pub trait ChapterContractRuntime {
    fn book_api(&self, input: &Value) -> crate::native::NativeExecution;
}
impl ChapterContractRuntime for crate::native::NativeRuntime {
    fn book_api(&self, input: &Value) -> crate::native::NativeExecution {
        crate::native::NativeRuntime::book_api(self, input)
    }
}
impl ChapterContractRuntime for crate::refinery_runtime::RustRuntime {
    fn book_api(&self, input: &Value) -> crate::native::NativeExecution {
        crate::refinery_runtime::RustRuntime::book_api(self, input)
    }
}

pub fn validate(runtime: &impl ChapterContractRuntime) -> Result<ValidationReport> {
    // list_chapter_packages
    let packages_result = runtime.book_api(&json!({
        "operation": "list_chapter_packages"
    }));
    let packages_value = packages_result.into_result()?;

    let packages = packages_value
        .get("packages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let contract_count = packages.len();

    let blockers: Vec<String> = packages
        .iter()
        .filter(|p| {
            p.get("release_status")
                .and_then(Value::as_str)
                .unwrap_or("")
                != "complete"
        })
        .filter_map(|p| {
            p.get("package_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();

    let complete_contracts: Vec<String> = packages
        .iter()
        .filter(|p| {
            p.get("release_status")
                .and_then(Value::as_str)
                .unwrap_or("")
                == "complete"
        })
        .filter_map(|p| {
            p.get("package_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();

    // validate_chapter_registry
    let registry_result = runtime.book_api(&json!({
        "operation": "validate_chapter_registry"
    }));
    let errors: Vec<String> = match registry_result.into_result() {
        Ok(v) => v
            .get("issues")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        Err(e) => vec![format!("registry_validation_failed: {e:#}")],
    };

    Ok(ValidationReport {
        contract_count,
        errors,
        blockers,
        complete_contracts,
    })
}
