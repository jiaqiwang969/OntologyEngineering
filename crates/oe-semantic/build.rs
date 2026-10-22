#[path = "../../build_support/book_source.rs"]
mod inventory;

use std::{fs, path::Path};

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest).parent().unwrap().parent().unwrap();
    let inputs = inventory::inputs(root).unwrap();
    // This uses the existing owned compiler-input authority. The emitted table
    // is source identity metadata, never another contract or book algorithm.
    let mut output = String::from("&[\n");
    for relative in &inputs {
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert!(
            metadata.len() <= 32 * 1024 * 1024,
            "book_compile_input_budget"
        );
        let name = relative
            .to_str()
            .unwrap()
            .replace(std::path::MAIN_SEPARATOR, "/");
        println!("cargo:rerun-if-changed={}", path.display());
        output.push_str(&format!(
            "({name:?}, include_bytes!({:?}) as &[u8]),\n",
            path.to_str().unwrap()
        ));
    }
    for name in [
        "crates",
        "vendor",
        "build_support",
        "runtime",
        "templates",
        "generated",
        "specs",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(name).display());
    }
    output.push_str("]\n");
    fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("book-source-inputs.rs"),
        output,
    )
    .unwrap();
}
