fn main() {
    match oe_evolution::native_probe_exec9::compiler_probe_entry9() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error:#}");
            std::process::exit(101);
        }
    }
}
