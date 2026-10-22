//! Original snapshot integrity operations. Content integrity does not establish
//! engineering validity, source applicability or an adoption decision.
use crate::{
    runtime::RustSemanticRuntime,
    version_storage_codec::{Encoder, Error, Limits},
};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticSnapshotChecksumPort, SemanticSnapshotChecksumRequest,
    SemanticSnapshotChecksumResult,
};
use semantica_json::native_value::{NativeLimits, NativeValue};
use serde_json::json;
use std::time::Duration;

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

fn failure(code: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: format!("snapshot_checksum_{code}"),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}

impl SemanticSnapshotChecksumPort for RustSemanticRuntime {
    fn snapshot_checksum(
        &self,
        request: &SemanticSnapshotChecksumRequest,
    ) -> Result<SemanticSnapshotChecksumResult, SemanticRuntimeFailure> {
        crate::datalog_port::check_serialized(request, MAX_INPUT_BYTES, false)
            .map_err(|_| failure("invalid", "Complete request exceeds16MiB"))?;
        if request.schema != "semantica.snapshot-checksum-request/1"
            || !matches!(
                request.operation.as_str(),
                "compute_checksum" | "verify_checksum"
            )
        {
            return Err(failure("invalid", "Unknown checksum schema or operation"));
        }
        let limits = NativeLimits {
            max_nodes: 65536,
            max_links: 131072,
            max_scalar_bytes: 4 * 1024 * 1024,
            max_depth: 128,
            max_wire_bytes: MAX_INPUT_BYTES,
        };
        let value = match request.representation.as_str() {
            "plain_json" => {
                crate::temporal_version_records::original_json_ingress(&request.value)
                    .map_err(|e| failure("ingress_failure", e.message))?;
                NativeValue::from_plain_json(&request.value, limits)
            }
            "native_value" => NativeValue::from_envelope(&request.value, limits),
            _ => return Err(failure("invalid", "Unknown input representation")),
        }
        .map_err(|e| failure("native_admission", e))?;
        let encoder = Encoder::new(
            &value,
            Limits {
                visits: 16777216,
                output_bytes: MAX_OUTPUT_BYTES,
                depth: 128,
                integer_digits: 65536,
                duration: Duration::from_secs(5),
            },
        );
        let observed = match request.operation.as_str() {
            "compute_checksum" => encoder
                .compute()
                .map(|(digest, text)| (json!(digest), Some(text))),
            "verify_checksum" => encoder.verify().map(|v| (json!(v), None)),
            _ => unreachable!(),
        };
        let (result, serialized_ascii, source_failure) = match observed {
            Ok((result, text)) => (Some(result), text, None),
            Err(Error::Source { class, message }) => (
                None,
                None,
                Some(failure("source_failure", format!("{class}: {message}"))),
            ),
            Err(Error::Refused(reason)) => return Err(failure("resource_refusal", reason)),
        };
        let input = serde_json::to_value(request).map_err(|e| failure("invalid", e))?;
        let out = SemanticSnapshotChecksumResult {
            schema: "semantica.snapshot-checksum-result/1".into(),
            profile: "semantica.source-snapshot-checksum-cpython313/1".into(),
            request_sha256: crate::sha256(
                semantica_json::canonical_json(&input).map_err(|e| failure("invalid", e))?,
            ),
            operation: request.operation.clone(),
            result,
            failure: source_failure,
            serialized_ascii,
        };
        crate::datalog_port::check_serialized(&out, MAX_OUTPUT_BYTES - 1, true)
            .map_err(|_| failure("resource_refusal", "Complete result exceeds16MiB"))?;
        Ok(out)
    }
}
