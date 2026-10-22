fn main() {
    match oe_evolution::native_runtime6::native_full_link_entry() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("native loader6 linker rejected: {e:#}");
            std::process::exit(64);
        }
    }
}
