"""Thin source-corpus adapter for Semantica's built-in ontology packages."""


__all__ = [
    "NATIVE_REFINERY_ASSET_CATEGORIES",
    "NATIVE_REFINERY_BOOK_IMPACTS",
    "NATIVE_REFINERY_CASE_KINDS",
    "NATIVE_REFINERY_CONTRACT",
    "NATIVE_REFINERY_OPERATIONS",
    "NATIVE_REFINERY_REGRESSION_CHECK_IDS",
    "NATIVE_REFINERY_RELEASE_CHECK_IDS",
    "NATIVE_REFINERY_TRANSITION_CONTEXT_ACTIONS",
    "NATIVE_REFINERY_TRANSITION_CONTEXT_REQUIRED_OPERATIONS",
    "NATIVE_REFINERY_RUNNER_CONTRACT",
    "NATIVE_REFINERY_STATES",
    "RUNTIME_ID",
    "SOURCE_LOCK_SCHEMA",
    "STAGING_RUNTIME_SCHEMA",
    "RuntimeSourceLock",
    "StagingRuntimeDescriptor",
    "chapter_asset_text",
    "create_package_runner",
    "create_runtime",
    "installed_runtime_artifact_sha256",
    "installed_runtime_version",
    "list_chapter_packages",
    "list_domain_packages",
    "native_refinery_acceptance_delta",
    "native_refinery_commit_candidate",
    "native_refinery_discover_package",
    "native_refinery_empty_package_sha256",
    "native_refinery_history",
    "native_refinery_open_engagement",
    "native_refinery_promote_candidate",
    "native_refinery_propose_candidate",
    "native_refinery_run_package",
    "native_refinery_verify_candidate",
    "normative_engraver_main",
    "package_asset_text",
    "package_binding_metadata",
    "read_migration_map",
    "read_runtime_source_lock",
    "read_staging_runtime_descriptor",
    "resolve_migration_successor",
    "run_and_verify_package",
    "run_package",
    "semantic_refinery_capabilities",
    "validate_chapter_registry",
    "validate_domain_packages",
    "verify_runtime_source_identity",
    "verify_installed_runtime_record",
    "verify_book_source_bindings",
]


def __getattr__(name):
    """Load semantic exports only when used; routing can help repair the runtime."""
    if name not in __all__:
        raise AttributeError(name)
    from . import semantica_runtime
    value = getattr(semantica_runtime, name)
    globals()[name] = value
    return value
