//! Explicit complete compiler-input /7 entry; shared actual parent/stage owner.
fn main() {
    match oe_evolution::native_runtime7::native_full_link_arguments(
        std::env::args().skip(1).collect(),
    ) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("native compiler7 linker rejected: {error:#}");
            std::process::exit(64);
        }
    }
}
