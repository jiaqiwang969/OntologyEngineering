//! Explicit complete SDK8 entry. Same actual compiler/stage/index/link owner.
fn main() {
    match oe_evolution::native_runtime8::native_full_link_arguments(
        std::env::args().skip(1).collect(),
    ) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("native SDK8 linker rejected: {error:#}");
            std::process::exit(64);
        }
    }
}
