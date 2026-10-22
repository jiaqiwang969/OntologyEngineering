//! Declared closure only; each actual DLL identity is verified separately on NX.
use oe_nx_protocol::{FileEntry, RuntimeIdentity};
use oe_nx_worker::identity::validate_native_runtime;

fn runtime() -> RuntimeIdentity {
    RuntimeIdentity {
        product: "Siemens NX".into(),
        product_label: "synthetic-not-loaded".into(),
        architecture: "x86_64".into(),
        libraries: [
            "libufun.dll",
            "libnxopencpp.dll",
            "libnxopencpp_assemblies.dll",
            "libnxopencpp_annotations.dll",
        ]
        .into_iter()
        .map(|name| {
            FileEntry::from_bytes(&format!("NXBIN/{name}"), b"synthetic-not-loaded").unwrap()
        })
        .collect(),
    }
}
#[test]
fn computed_size_dependency_requires_exact_runtime_closure_without_accepting_missing_or_extra_pins()
{
    let mut r = runtime();
    validate_native_runtime(&r).unwrap();
    r.libraries.reverse();
    validate_native_runtime(&r).unwrap();
    let mut missing = runtime();
    missing.libraries.pop();
    assert!(validate_native_runtime(&missing).is_err());
    let mut replaced = runtime();
    replaced.libraries[3] = FileEntry::from_bytes("NXBIN/arbitrary.dll", b"synthetic").unwrap();
    assert!(validate_native_runtime(&replaced).is_err());
    let mut duplicate = runtime();
    duplicate.libraries[3] = duplicate.libraries[2].clone();
    assert!(validate_native_runtime(&duplicate).is_err());
    let mut extra = runtime();
    extra
        .libraries
        .push(FileEntry::from_bytes("NXBIN/arbitrary.dll", b"synthetic").unwrap());
    assert!(validate_native_runtime(&extra).is_err());
    let mut foreign = runtime();
    foreign.architecture = "x86".into();
    assert!(validate_native_runtime(&foreign).is_err());
}
