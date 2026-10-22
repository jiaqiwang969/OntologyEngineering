//! Private diagnostic-content executable; no process owner or dispatch changes.
use anyhow::{Result, ensure};
use oe_core::read_regular_bytes;
use std::path::Path;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(args.len() == 3, "native5_content_arguments");
    let raw = read_regular_bytes(Path::new(&args[2]), 4 * 1024 * 1024)?;
    let report =
        oe_evolution::native_runtime5_content::inspect_loader_diagnostic_bytes(&args[1], &raw)?;
    oe_evolution::native_runtime5_content::verify_loader_diagnostic_bytes(&args[1], &raw, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
