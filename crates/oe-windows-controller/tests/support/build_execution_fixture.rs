// Shared anonymous source fixture. It never compiles or executes these bytes.
use oe_nx_protocol::*;
use oe_windows_controller::{build::*, build_execution::*, build_source::*};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{ZipWriter, write::SimpleFileOptions};
fn installed(path: &str) -> InstalledFile {
    InstalledFile {
        path: path.into(),
        identity: BlobIdentity::of(path.as_bytes()),
    }
}
pub fn fixture() -> (ExecutionSpecV2, VerifiedSourceArchive) {
    fixture_config(None, false, false)
}
pub fn fixture_config(
    config: Option<(&str, &[u8])>,
    registry: bool,
    vendor: bool,
) -> (ExecutionSpecV2, VerifiedSourceArchive) {
    let mut source = BTreeMap::new();
    source.insert("Cargo.toml".to_owned(), format!(
        "[workspace]\nmembers = [{}]\nresolver = \"3\"\n[workspace.package]\nversion = \"0.0.1\"\n",
        WORKSPACE_MEMBERS.iter().map(|n| format!("\"crates/{n}\"")).collect::<Vec<_>>().join(",")
    ).into_bytes());
    let mut lock = "version = 4\n".to_owned();
    for name in WORKSPACE_MEMBERS {
        source.insert(
            format!("crates/{name}/Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion.workspace = true\n").into_bytes(),
        );
        source.insert(
            format!("crates/{name}/src/lib.rs"),
            b"// anonymous fixture, not compiled\n".to_vec(),
        );
        lock.push_str(&format!(
            "\n[[package]]\nname = \"{name}\"\nversion = \"0.0.1\"\n"
        ));
    }
    if registry {
        lock.push_str("\n[[package]]\nname = \"synthetic-dep\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n");
    }
    if vendor {
        source.insert(
            "vendor/synthetic-dep/src/lib.rs".into(),
            b"// synthetic vendor, not resolved\n".to_vec(),
        );
    }
    if let Some((path, bytes)) = config {
        source.insert(path.into(), bytes.to_vec());
    }
    source.insert("Cargo.lock".into(), lock.as_bytes().to_vec());
    for name in [
        "crates/oe-nx-worker/src/main.rs",
        "crates/oe-nx-worker/shim/nx_shim.cpp",
        "crates/oe-nx-worker/shim/nx_shim.h",
    ] {
        source.insert(
            name.into(),
            b"// synthetic source, never executed\n".to_vec(),
        );
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in &source {
        zip.start_file(
            format!("workspace/{name}"),
            SimpleFileOptions::default().unix_permissions(0o600),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    let archive = zip.finish().unwrap().into_inner();
    let inventory = serde_json::to_vec(&SourceManifest {
        schema: SOURCE_SCHEMA.into(),
        archive_root: RelativePath::new("workspace").unwrap(),
        files: source
            .iter()
            .map(|(n, b)| FileEntry::from_bytes(n, b).unwrap())
            .collect(),
    })
    .unwrap();
    let files = BTreeMap::from([
        ("sources/workspace.zip", archive),
        ("sources/workspace-files.json", inventory),
        ("sources/Cargo.lock", lock.into_bytes()),
    ]);
    let entry = |name: &str| FileEntry::from_bytes(name, &files[name]).unwrap();
    let mut spec = BuildSpec {
        schema: BUILD_SPEC_SCHEMA.into(),
        build_id: JobId::new("fixture-build").unwrap(),
        root_directory: "C:/接收 space/worker-builds".into(),
        root_identity: RootIdentity {
            host_id: "synthetic-host".into(),
            volume_id: "synthetic-volume".into(),
            directory_id: "synthetic-directory".into(),
        },
        source_archive: entry("sources/workspace.zip"),
        source_inventory: entry("sources/workspace-files.json"),
        cargo_lock: entry("sources/Cargo.lock"),
        expanded_source_files: 11_077,
        expanded_source_bytes: 220_150_236,
        tools: BuildTools {
            cargo: installed("C:/Rust tools/bin/cargo.exe"),
            rustc: installed("C:/Rust tools/bin/rustc.exe"),
            gnu_linker: installed("C:/GNU/bin/gcc.exe"),
            msvc_cl: installed("C:/MSVC/bin/cl.exe"),
            msvc_link: installed("C:/MSVC/bin/link.exe"),
            windows_directory: "C:/Windows".into(),
            include_directories: vec!["C:/MSVC/include".into(), "C:/Windows Kits/Include".into()],
            library_directories: vec!["C:/MSVC/lib/x64".into(), "C:/Windows Kits/Lib/x64".into()],
        },
        sdk: InstalledSdk {
            root: "C:/Program Files/Siemens/NX 2412".into(),
            files: [
                "UGOPEN/uf.h",
                "UGOPEN/uf_defs.h",
                "UGOPEN/uf_part.h",
                "UGOPEN/uf_modl_types.h",
                "UGOPEN/uf_object_types.h",
                "UGOPEN/NXOpen/Session.hxx",
                "UGOPEN/libufun.lib",
                "UGOPEN/libnxopencpp.lib",
                "UGOPEN/libnxopencpp_assemblies.lib",
            ]
            .into_iter()
            .map(|name| FileEntry::from_bytes(name, b"declared-only-SDK-identity").unwrap())
            .collect(),
            runtime: RuntimeIdentity {
                product: "Siemens NX".into(),
                product_label: "synthetic-no-installation".into(),
                architecture: "x86_64".into(),
                libraries: vec![
                    FileEntry::from_bytes("NXBIN/libufun.dll", b"declared-only-runtime").unwrap(),
                    FileEntry::from_bytes("NXBIN/libnxopencpp.dll", b"declared-only-cpp-runtime")
                        .unwrap(),
                    FileEntry::from_bytes(
                        "NXBIN/libnxopencpp_assemblies.dll",
                        b"declared-only-assemblies-runtime",
                    )
                    .unwrap(),
                ],
            },
        },
        profile: BuildProfile::Debug,
        jobs: 4,
        max_seconds: 300,
        worker_id: "synthetic-builder-worker".into(),
        worker_version: "fixture".into(),
        declared_capabilities: vec![Capability {
            action: "inspect".into(),
            params_schema: "cad-agent.nx-native-acceptance-params/v2".into(),
        }],
    };

    spec.expanded_source_files = source.len() as u32;
    spec.expanded_source_bytes = source.values().map(|b| b.len() as u64).sum();
    let verified = spec
        .verify_sources(
            &files["sources/workspace.zip"],
            &files["sources/workspace-files.json"],
            &files["sources/Cargo.lock"],
        )
        .unwrap();
    (
        ExecutionSpecV2 {
            schema: EXECUTION_SPEC_SCHEMA.into(),
            build: spec,
            network_policy: NetworkPolicy::CargoOfflineOnly,
        },
        verified,
    )
}
