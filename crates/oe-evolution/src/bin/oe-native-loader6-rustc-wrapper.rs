fn main() {
    if let Err(e) = oe_evolution::native_loader6::compiler_exec_entry() {
        eprintln!("native loader6 compiler wrapper rejected: {e:#}");
        std::process::exit(64);
    }
}
