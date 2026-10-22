//! Task Scheduler InteractiveToken entry. GUI subsystem prevents a console window.
#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    let result = oe_sw_worker::session_host::run(std::env::args_os().collect());
    #[cfg(not(windows))]
    let result: anyhow::Result<()> =
        Err(anyhow::anyhow!("solidworks_session_host_requires_windows"));
    if result.is_err() {
        // No dialog, user input, focus change, kill, retry, or COM fallback.
        std::process::exit(1);
    }
}
