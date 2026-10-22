//! Public book discovery uses the existing source Catalog and original MCP DTOs.
//! The application injects source access; this module owns no source registry.
use anyhow::{Result, ensure};
use semantica_contracts::SemanticBookCatalogRequest;
use semantica_runtime::books::Catalog;
use serde_json::{Value, json};

pub const MANIFEST_PREFIX: &str = "semantica://packages/manifest/";
pub const BOOK_TOOLS: [&str; 2] = ["list_chapter_packages", "get_chapter_package"];
pub const ALL_BOOK_TOOLS: [&str; 5] = [
    "list_chapter_packages",
    "get_chapter_package",
    "verify_book_sources",
    "run_chapter_package",
    "verify_chapter_package",
];

fn error(code: &str, message: &str) -> Value {
    json!({"schema_version":"1.0","ok":false,"error":{"code":code,"message":message}})
}
fn records(catalog: &Catalog, volume: Option<&str>, include_domains: bool) -> Result<Vec<Value>> {
    let mut records = Vec::new();
    for d in catalog.list_chapter_packages(volume) {
        records.push(json!({"chapter":d["chapter"],"key":format!("{}.{}",d["volume"].as_str().unwrap_or(""),d["chapter"].as_str().unwrap_or("")),"kind":"chapter","package_id":d["package_id"],"release_status":d["release_status"],"status":d["status"],"title":d["title"],"version":d["version"],"volume":d["volume"]}));
    }
    if include_domains && volume.is_none_or(|v| v == "vol2") {
        // The original MCP surface exposes the one fixed normative package.
        let manifest = catalog.read_manifest("semantica.chapter_packages.vol2.normative")?;
        let title = manifest["title"]
            .as_str()
            .filter(|v| !v.is_empty())
            .or_else(|| manifest["domain"].as_str().filter(|v| !v.is_empty()))
            .unwrap_or("ISO 26262 normative derived layer");
        records.push(json!({"chapter":null,"key":"vol2.normative","kind":"domain","package_id":"semantica.chapter_packages.vol2.normative","release_status":manifest.get("release_status").unwrap_or(&json!("blocked")),"status":manifest.get("status").unwrap_or(&json!("partial")),"title":title,"version":manifest.get("version").unwrap_or(&json!("")),"volume":"vol2"}));
    }
    Ok(records)
}

pub fn catalog_operation(catalog: &Catalog, input: &SemanticBookCatalogRequest) -> Result<Value> {
    ensure!(input.validate().is_ok(), "book_catalog_contract_invalid");
    let Some(args) = input.arguments.as_object() else {
        return Ok(error(
            "invalid_argument",
            "tool arguments must be an object",
        ));
    };
    match input.operation.as_str() {
        "list" => {
            if args
                .keys()
                .any(|k| k != "include_domain_packages" && k != "volume")
            {
                return Ok(error(
                    "invalid_argument",
                    "Unsupported package-list argument.",
                ));
            }
            let include_domains = match args.get("include_domain_packages") {
                None => true,
                Some(Value::Bool(v)) => *v,
                _ => {
                    return Ok(error(
                        "invalid_argument",
                        "include_domain_packages must be boolean",
                    ));
                }
            };
            let volume = match args.get("volume") {
                None | Some(Value::Null) => None,
                Some(Value::String(v)) if v == "vol1" || v == "vol2" => Some(v.as_str()),
                _ => return Ok(error("invalid_argument", "volume must be 'vol1' or 'vol2'")),
            };
            let packages = records(catalog, volume, include_domains)?;
            let chapters = packages.iter().filter(|p| p["kind"] == "chapter").count();
            Ok(
                json!({"chapter_package_count":chapters,"domain_package_count":packages.len()-chapters,"ok":true,"package_count":packages.len(),"packages":packages,"schema_version":"1.0"}),
            )
        }
        "get" => {
            if args.keys().any(|k| k != "package_id") {
                return Ok(error(
                    "invalid_argument",
                    "Unsupported package-get argument.",
                ));
            }
            let Some(id) = args
                .get("package_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                return Ok(error(
                    "invalid_argument",
                    "package_id must be a non-empty string",
                ));
            };
            let record = records(catalog, None, true)?
                .into_iter()
                .find(|r| r["package_id"] == id);
            let Some(record) = record else {
                return Ok(error(
                    "package_not_found",
                    &format!("Unknown built-in package ID: {id}"),
                ));
            };
            Ok(
                json!({"manifest":catalog.read_manifest(id)?,"ok":true,"package":record,"schema_version":"1.0"}),
            )
        }
        _ => anyhow::bail!("book_catalog_operation_unsupported"),
    }
}

/// Execute the canonical built-in allowlist in a caller-owned scoped task session.
/// Caller pins compare actual source/executable observations, never self-certify origin.
pub fn package_operation(
    catalog: &Catalog,
    provider: &dyn semantica_runtime::books::SourceProvider,
    operation: &str,
    args: &Value,
    runtime_identity: &Value,
) -> Result<Value> {
    let Some(object) = args.as_object() else {
        return Ok(error(
            "invalid_argument",
            "tool arguments must be an object",
        ));
    };
    if object.keys().any(|k| {
        ![
            "package_id",
            "scenario_id",
            "runtime_commit",
            "runtime_artifact_sha256",
        ]
        .contains(&k.as_str())
    }) {
        return Ok(error(
            "invalid_argument",
            "Unsupported package execution argument.",
        ));
    }
    let Some(package_id) = args["package_id"].as_str().filter(|v| !v.is_empty()) else {
        return Ok(error(
            "invalid_argument",
            "package_id must be a non-empty string",
        ));
    };
    if !records(catalog, None, true)?
        .iter()
        .any(|r| r["package_id"] == package_id)
    {
        return Ok(error(
            "package_not_found",
            &format!("Unknown built-in package ID: {package_id}"),
        ));
    }
    let scenario = match args.get("scenario_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.as_str()),
        _ => {
            return Ok(error(
                "invalid_argument",
                "scenario_id must be a string when provided",
            ));
        }
    };
    let Some(commit) = args["runtime_commit"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
    else {
        return Ok(error(
            "invalid_argument",
            "runtime_commit must be a non-empty string",
        ));
    };
    let Some(digest) = args["runtime_artifact_sha256"].as_str() else {
        return Ok(error(
            "invalid_argument",
            "runtime_artifact_sha256 must be a string",
        ));
    };
    let digest = digest.to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(error(
            "invalid_argument",
            "runtime_artifact_sha256 must be exactly 64 hexadecimal characters",
        ));
    }
    if runtime_identity["runtime_commit"] != commit
        || runtime_identity["runtime_artifact_sha256"] != digest
    {
        return Ok(error(
            "runtime_identity_mismatch",
            "Requested runtime pins do not match the host-injected current Rust execution.",
        ));
    }
    if !["run", "verify"].contains(&operation) {
        return Ok(error("invalid_argument", "Unknown package operation."));
    }
    // This task session mirrors the original per-run isolated runtime. It is
    // not an authoritative Project or a second credential/version registry.
    let mut session = semantica_runtime::dataset::DatasetSession::new();
    let mut state = semantica_runtime::book_runner::BookRunnerState::default();
    let execution = semantica_runtime::book_runner::run(
        &mut state,
        catalog,
        provider,
        &mut session,
        package_id,
        scenario,
        runtime_identity,
    )?;
    let mut payload = if operation == "verify" {
        let verdict = semantica_runtime::book_runner::verify(
            &state,
            &mut session,
            &execution,
            runtime_identity,
        )?;
        json!({"ok":verdict["status"]=="complete","operation":operation,"package_id":package_id,"schema_version":"1.0","result":{"execution":execution,"release_verdict":verdict}})
    } else {
        json!({"ok":true,"operation":operation,"package_id":package_id,"schema_version":"1.0","result":execution})
    };
    if payload["ok"] != true {
        payload["error"] =
            json!({"code":"release_blocked","message":"Package release verification is blocked."});
    }
    Ok(payload)
}
