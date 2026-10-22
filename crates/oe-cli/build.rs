use sha2::{Digest, Sha256};
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let contracts = root.join("generated/contracts.rs");
    println!("cargo:rerun-if-changed={}", contracts.display());
    let raw = std::fs::read(contracts)
        .expect("generated Rust contracts required; run the locked real ggen bootstrap first");
    let mut features: Vec<_> = std::env::vars()
        .filter(|(k, _)| k.starts_with("CARGO_FEATURE_"))
        .map(|(k, _)| k)
        .collect();
    features.sort();
    let record = serde_json::json!({"schema":"oev2.cli-build/1","generated_contracts_sha256":format!("{:x}",Sha256::digest(&raw)),"target":std::env::var("TARGET").unwrap(),"package_features":features,"profile":std::env::var("PROFILE").unwrap(),"scope":"CLI generated contract input and own target/features; normalizer manifest binds owned sources and Cargo inputs"});
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("cli-build.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
}
