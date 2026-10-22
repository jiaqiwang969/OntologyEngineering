//! Fixed Rust adapter for the explicit runtime reproduction /2 only.
use std::{io::Write, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["identity"] {
        return match oe_evolution::compiled().and_then(|v| Ok(serde_json::to_vec(&v)?)) {
            Ok(bytes) => {
                let _ = std::io::stdout().lock().write_all(&bytes);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e:#}");
                ExitCode::FAILURE
            }
        };
    }
    match oe_evolution::runtime_reproduction2::linker_entry(args) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
