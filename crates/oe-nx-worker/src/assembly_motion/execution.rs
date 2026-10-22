//! Query lifecycle, not a wire protocol or an engineering clearance decision.
use super::{InterferenceStatus, interference_status};
use anyhow::{Result, ensure};
pub trait InterferenceApi {
    fn query(&mut self, a: u32, b: u32) -> (i32, i32);
    fn cleanup_update(&mut self) -> i32;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairReceipt {
    pub a: u32,
    pub b: u32,
    pub query_code: i32,
    pub result_raw: i32,
    pub cleanup_code: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairEvent {
    Pending { a: u32, b: u32 },
    Returned(PairReceipt),
}
/// The sink must persist Pending before native work, and Returned before any
/// classification is accepted. Every issued query is followed by SDK cleanup,
/// including failed queries and unrecognized result codes. Sink failure stops
/// subsequent work, and never skips cleanup already owed by an issued query.
pub fn check_pair(
    api: &mut impl InterferenceApi,
    a: u32,
    b: u32,
    mut sink: impl FnMut(PairEvent) -> Result<()>,
) -> Result<(PairReceipt, InterferenceStatus)> {
    ensure!(
        a != 0 && b != 0 && a != b,
        "motion_interference_pair_identity_invalid"
    );
    sink(PairEvent::Pending { a, b })?;
    let (query_code, result_raw) = api.query(a, b);
    let cleanup_code = api.cleanup_update();
    let receipt = PairReceipt {
        a,
        b,
        query_code,
        result_raw,
        cleanup_code,
    };
    sink(PairEvent::Returned(receipt))?;
    ensure!(
        cleanup_code == 0,
        "motion_interference_cleanup_failed:{cleanup_code}"
    );
    Ok((receipt, interference_status(query_code, result_raw)?))
}
