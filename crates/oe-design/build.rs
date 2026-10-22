#[path = "../../build_support/design_composition.rs"]
#[allow(dead_code)] // Build capture consumes files; runtime consumes all source metadata.
mod composition;
#[path = "../../build_support/provenance.rs"]
mod inventory;
#[path = "../../build_support/migration_pins.rs"]
mod migration_pins;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest).parent().unwrap().parent().unwrap();
    let mut files = BTreeMap::new();
    for p in inventory::inputs(root).expect("compiled input inventory") {
        println!("cargo:rerun-if-changed={}", root.join(&p).display());
        let bytes = std::fs::read(root.join(&p)).unwrap();
        let name = p
            .components()
            .map(|p| p.as_os_str().to_str().unwrap())
            .collect::<Vec<_>>()
            .join("/");
        files.insert(name, serde_json::json!({"sha256":format!("{:x}",Sha256::digest(&bytes)),"bytes":bytes.len()}));
    }
    for d in [
        "crates",
        "vendor/spargebra",
        "vendor/spareval",
        "vendor/sparopt",
        "build_support",
        "runtime",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(d).display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        root.join("specs/engineering.toml").display()
    );
    let composition_files: BTreeMap<_,_> = composition::current_capture(root)
        .expect("bounded current composition input capture").into_iter().map(|(path,bytes)| {
            println!("cargo:rerun-if-changed={}",root.join(&path).display());
            (path,serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}))
        }).collect();
    let compiler = std::process::Command::new(std::env::var("RUSTC").unwrap())
        .arg("-Vv")
        .output()
        .unwrap();
    assert!(compiler.status.success());
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(k, _)| k.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let record = serde_json::json!({"schema":"oev2.compiled-source/1","files":files,"composition_files":composition_files,
        "compiler":String::from_utf8(compiler.stdout).unwrap(),"target":std::env::var("TARGET").unwrap(),
        "package_features":features,"profile":std::env::var("PROFILE").unwrap(),"opt_level":std::env::var("OPT_LEVEL").unwrap(),
        "rustflags_sha256":format!("{:x}",Sha256::digest(flags.as_bytes())),
        "scope":"owned workspace Rust/TOML/build rules, local parser/evaluator patches, Cargo.lock/toolchain/config; editing spec and templates separately bound at execution",
        "uncovered":["external crate source cache bytes","transitive resolved feature graph","native SDK and runtime libraries","generated contracts used by other crates","unlisted compile-time environment"]});
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("compiled-source.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    // The Semantica port ledger and recorded evidence present at build time become compiled
    // pins: files edited after this build cannot complete the migration (migration_ledger.rs).
    let mut pins = String::new();
    for (name, dir) in [
        ("COMPILED_LEDGER_SHA256", migration_pins::LEDGER_DIR),
        ("COMPILED_EVIDENCE_SHA256", migration_pins::EVIDENCE_DIR),
    ] {
        let path = root.join(dir);
        println!("cargo:rerun-if-changed={}", path.display());
        let value = match migration_pins::directory_digest(&path).expect("migration pin digest") {
            Some(digest) => format!("Some(\"{digest}\")"),
            None => "None".to_owned(),
        };
        pins.push_str(&format!(
            "pub(crate) const {name}: Option<&str> = {value};\n"
        ));
    }
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("migration-pins.rs"),
        pins,
    )
    .unwrap();
}
