use anyhow::{Context, Result, ensure};
use std::{io::Read, path::PathBuf};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 2 && args[1] == "capabilities" {
        println!(
            "{}",
            serde_json::to_string_pretty(&oe_sw_worker::capabilities())?
        );
        return Ok(());
    }
    ensure!(
        args.len() == 4 && args[1] == "run",
        "usage: oe-sw-worker capabilities | run REQUEST.json NEW_OUTPUT_DIRECTORY"
    );
    let request = PathBuf::from(&args[2]);
    let mut raw = Vec::new();
    std::fs::File::open(&request)?
        .take(oe_sw_worker::MAX_RECORD_BYTES as u64 + 1)
        .read_to_end(&mut raw)?;
    ensure!(
        raw.len() <= oe_sw_worker::MAX_RECORD_BYTES,
        "solidworks_request_budget"
    );
    let value = oe_wire::json::strict_json(&raw)?;
    let typed = serde_json::from_value(value).context("solidworks_request_parse")?;
    oe_sw_worker::validate_request(&typed)?;
    #[cfg(windows)]
    {
        oe_sw_worker::native::run(&request, &raw, &typed, &PathBuf::from(&args[3]))?;
    }
    #[cfg(not(windows))]
    anyhow::bail!(
        "solidworks_native_worker_requires_windows; request validation does not execute CAD"
    );
    #[allow(unreachable_code)]
    Ok(())
}
