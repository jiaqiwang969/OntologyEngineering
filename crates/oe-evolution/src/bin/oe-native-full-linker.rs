//! Frozen full /3 adapter. Rustc passes original linker arguments directly.
fn main() {
    match oe_evolution::native_runtime3::native_full_link_entry() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("native full linker rejected: {error:#}");
            std::process::exit(64);
        }
    }
}
