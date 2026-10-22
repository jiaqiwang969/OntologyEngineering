//! Windows can distribute the CLI without claiming the macOS /6 native ABI.
//! These tests call the real public entry points, with no context or native SDK.
#![cfg(windows)]

#[test]
fn unsupported_native_loader_entry_refuses_before_context_lookup() {
    let error = oe_evolution::native_loader6::compiler_exec_entry().unwrap_err();
    assert_eq!(error.to_string(), "loader6_platform_unsupported");
}

#[test]
fn unsupported_native_image_observation_stays_unknown() {
    let error = oe_evolution::native_loader6::current_images().unwrap_err();
    assert_eq!(error.to_string(), "loader6_platform_unsupported");
}
