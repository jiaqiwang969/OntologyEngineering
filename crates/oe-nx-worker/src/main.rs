#[cfg(windows)]
fn main() {
    if let Err(error) = oe_nx_worker::worker::main_entry() {
        eprintln!("native_worker_failed:{error:#}");
        std::process::exit(1)
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("native_worker_requires_windows");
    std::process::exit(2)
}
