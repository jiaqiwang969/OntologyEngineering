//! Compile-bound owned supplier implementation plus its shared browser observation closure.
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Map, Value, json};

macro_rules! sources {
    ($($path:literal),+ $(,)?) => {
        pub const SOURCE_FILES: &[(&str, &[u8])] = &[
            $(($path, include_bytes!(concat!("../../../../", $path)))),+
        ];
    };
}
sources!(
    "crates/oe-cad/Cargo.toml",
    "crates/oe-cad/src/lib.rs",
    "crates/oe-cad/src/supplier/mod.rs",
    "crates/oe-cad/src/supplier/source.rs",
    "crates/oe-cad/src/supplier/artifacts.rs",
    "crates/oe-cad/src/supplier/archive.rs",
    "crates/oe-cad/src/supplier/session.rs",
    "crates/oe-cad/src/supplier/unicode_casefold.rs",
    "crates/oe-wire/Cargo.toml",
    "crates/oe-wire/src/lib.rs",
    "crates/oe-wire/src/unicode_casefold.rs"
);

pub fn identity(root: &SkillRoot) -> Result<Value> {
    let browser = oe_browser::source::identity(root)?;
    let mut files = Map::new();
    for (relative, compiled) in SOURCE_FILES {
        let actual = read_regular_bytes(&root.path().join(relative), 8 * 1024 * 1024)?;
        ensure!(actual == *compiled, "supplier_compiled_source_changed");
        files.insert(
            (*relative).into(),
            json!({"sha256":sha256(&actual),"bytes":actual.len()}),
        );
    }
    Ok(
        json!({"schema":"cad-agent.native-supplier-source/v1","files":files,"browser":browser,"semantic_authority":false}),
    )
}

pub fn ensure_current(root: &SkillRoot, expected: &Value) -> Result<()> {
    ensure!(
        identity(root)? == *expected,
        "supplier_source_identity_changed"
    );
    Ok(())
}

pub fn source_url(value: &str) -> Result<reqwest::Url> {
    ensure!(
        value.len() <= 16384 && !value.chars().any(char::is_control),
        "supplier_source_url_invalid"
    );
    let url = reqwest::Url::parse(value).context("supplier_source_url_invalid")?;
    ensure!(
        ["https", "http"].contains(&url.scheme())
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "supplier_source_url_invalid"
    );
    Ok(url)
}

pub(crate) fn error_code(error: &anyhow::Error) -> String {
    let text = error.to_string();
    if !text.is_empty()
        && text.len() <= 160
        && text.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        text
    } else {
        "supplier_operation_unconfirmed".into()
    }
}
