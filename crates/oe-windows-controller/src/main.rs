#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    oe_windows_controller::controller::main_entry()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows controller requires actual Windows");
    std::process::exit(2);
}
