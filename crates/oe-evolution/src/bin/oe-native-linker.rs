//! Explicit rustc linker entry. No shell or second version owner.
fn main() {
    match oe_evolution::native_runtime::native_link_entry() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("native_link_rejected:{error:#}");
            std::process::exit(64);
        }
    }
}
