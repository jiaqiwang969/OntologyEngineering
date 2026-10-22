//! Transactional acknowledgement of already bounded native diagnostics.
//! The native producer retains its existing 512KiB admission guard.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
pub fn persist_batch(
    queue: &mut Vec<Value>,
    used: &mut usize,
    record: &mut dyn FnMut(Value) -> Result<()>,
) -> Result<()> {
    let actual = queue.iter().try_fold(0usize, |sum, v| -> Result<usize> {
        sum.checked_add(serde_json::to_vec(v)?.len())
            .context("load_diagnostic_size_overflow")
    })?;
    ensure!(actual == *used, "load_diagnostic_accounting_differs");
    if queue.is_empty() {
        return Ok(());
    }
    let batch = json!({"schema":"oev2.nx-load-diagnostic-batch/1","observations":queue,
  "observation_bytes":actual,"ordering":"native_emission_order"});
    ensure!(
        serde_json::to_vec(&batch)?.len() <= oe_nx_protocol::MAX_RECORD_BYTES / 2,
        "load_diagnostic_batch_record_budget"
    );
    record(batch)?;
    // No remove/reset before the durable writer acknowledges the complete batch.
    queue.clear();
    *used = 0;
    Ok(())
}
