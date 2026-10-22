//! Lock the actual Rust adapter and the retained upstream DOM observer.
use crate::SnapshotSource;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Map, Value, json};

macro_rules! sources {
    ($($path:literal),+ $(,)?) => {
        pub const SOURCE_FILES: &[(&str, &[u8])] = &[
            $(($path, include_bytes!(concat!("../../../", $path)))),+
        ];
    };
}
sources!(
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "crates/oe-cli/Cargo.toml",
    "crates/oe-cli/src/main.rs",
    "crates/oe-cli/src/browser.rs",
    "crates/oe-cli/src/v2.rs",
    "crates/oe-cli/src/supplier.rs",
    "crates/oe-browser/Cargo.toml",
    "crates/oe-contracts/Cargo.toml",
    "crates/oe-contracts/src/lib.rs",
    "generated/contracts.rs",
    "crates/oe-browser/src/lib.rs",
    "crates/oe-browser/src/types.rs",
    "crates/oe-browser/src/task.rs",
    "crates/oe-browser/src/source.rs",
    "crates/oe-browser/src/backend.rs",
    "crates/oe-browser/src/backend/dom.rs",
    "crates/oe-browser/src/backend/ax.rs",
    "crates/oe-browser/src/backend/ipc.rs",
    "crates/oe-browser/src/backend/focus.rs",
    "crates/oe-browser/src/backend/peekaboo.rs",
    "runtime/peekaboo/bridge.mjs",
    "runtime/peekaboo/focus-policy.mjs",
    "runtime/peekaboo/provider-lock.json",
    "crates/oe-core/Cargo.toml",
    "crates/oe-core/src/lib.rs",
    "crates/oe-core/src/json.rs",
    "crates/oe-wire/Cargo.toml",
    "crates/oe-wire/src/lib.rs",
    "crates/oe-wire/src/json.rs",
    "crates/oe-wire/src/unicode_casefold.rs",
    "crates/oe-core/src/process.rs",
    "runtime/browser-observer/source-lock.json",
    "runtime/browser-observer/upstream/LICENSE",
    "runtime/browser-observer/upstream/snapshot.js"
);

pub fn identity(root: &SkillRoot) -> Result<Value> {
    let mut files = Map::new();
    for (relative, compiled) in SOURCE_FILES {
        let actual = read_regular_bytes(&root.path().join(relative), 8 * 1024 * 1024)?;
        ensure!(actual == *compiled, "browser_compiled_source_changed");
        files.insert(
            (*relative).into(),
            json!({"sha256":sha256(&actual),"bytes":actual.len()}),
        );
    }
    let lock_path = root
        .path()
        .join("runtime/browser-observer/source-lock.json");
    let lock = oe_core::json::strict_json(&read_regular_bytes(&lock_path, 1024 * 1024)?)?;
    ensure!(
        lock["schema"] == "oev2.browser-observer-source/1"
            && lock["repository"] == "https://github.com/browser-use/jev-ultrafast"
            && lock["commit"] == "1231850a0bf1a0c0341fe408ef1668dbbfdfac46"
            && lock["license"] == "MIT",
        "browser_observer_source_lock_mismatch"
    );
    let snapshot = lock["files"]
        .as_array()
        .context("browser_source_files_missing")?
        .iter()
        .filter(|v| v["path"] == "snapshot.js")
        .collect::<Vec<_>>();
    ensure!(
        snapshot.len() == 1
            && snapshot[0]["sha256"]
                == files["runtime/browser-observer/upstream/snapshot.js"]["sha256"],
        "browser_snapshot_lock_mismatch"
    );
    Ok(json!({"schema":"oev2.native-browser-source/3",
        "provider":"host_agent","protocol":"observed-actions",
        "files":files,"upstream_repository":lock["repository"],"upstream_commit":lock["commit"],
        "external_code_used":"locked MIT DOM-only snapshot.js and pinned Peekaboo/Node thin MCP transport; no upstream Python agent execution",
        "upstream_role":"retained DOM observation algorithm and license provenance; never the retired Jev model service",
        "current_browser_backend":"Rust Peekaboo background adapter",
        "native_adapter":"oe-browser","semantic_authority":false}))
}

pub fn ensure_current(root: &SkillRoot, expected: &Value) -> Result<()> {
    ensure!(
        identity(root)? == *expected,
        "browser_source_identity_changed"
    );
    Ok(())
}

pub fn snapshot(root: &SkillRoot) -> Result<SnapshotSource> {
    let identity = identity(root)?;
    let name = "runtime/browser-observer/upstream/snapshot.js";
    let raw = read_regular_bytes(&root.path().join(name), 1024 * 1024)?;
    ensure!(
        identity["files"][name]["sha256"] == sha256(&raw),
        "browser_snapshot_changed"
    );
    Ok(SnapshotSource {
        sha256: sha256(&raw),
        javascript: String::from_utf8(raw).context("browser_snapshot_not_utf8")?,
    })
}

pub fn doctor(root: &SkillRoot) -> Result<Value> {
    Ok(
        json!({"status":"native_adapter_ready","source":identity(root)?,
        "backend_support":{"peekaboo_background":cfg!(target_os="macos"),"peekaboo_ax_background":cfg!(target_os="macos"),"peekaboo_dom_verified_existing_cdp_only":cfg!(target_os="macos"),"chrome_apple_events":false,"browser_harness_ipc":false},
        "historical_backends":{"status":"source_and_receipts_retained_current_live_dispatch_forbidden","values":["chrome_apple_events","browser_harness"]},
        "browser_connection":"not_checked","website_account":"not_checked","live_task":"not_run",
        "external_runtime":"Current default observes existing Chrome through Peekaboo CLI background AX; DOM requires Rust-verified explicit loopback CDP listener owned by current Chrome before Peekaboo MCP starts; no profile discovery/Chrome launch/configuration or legacy fallback",
        "engineering_acceptance":"not_evaluated"}),
    )
}
