//! Observe this dependency's effective Cargo features, including unified ones.
fn main() {
    let mut features: Vec<_> = std::env::vars_os()
        .filter_map(|(key, _)| key.into_string().ok())
        .filter(|key| key.starts_with("CARGO_FEATURE_"))
        .collect();
    features.sort();
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("compiled-features.txt"), features.join("\n")).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
