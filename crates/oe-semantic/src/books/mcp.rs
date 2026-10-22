//! The existing approved archive adapter is the sole source for book MCP reads.
use super::rust::ArchiveProvider;
use semantica_contracts::{
    SemanticBookCatalogPort, SemanticBookCatalogRequest, SemanticBookCatalogResult,
    SemanticRuntimeFailure,
};
use std::path::Path;

pub struct RegisteredBookCatalog {
    provider: ArchiveProvider,
    root: std::path::PathBuf,
}
impl RegisteredBookCatalog {
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        let provider = ArchiveProvider::open(root)?;
        provider.catalog()?;
        provider.verify()?;
        Ok(Self {
            provider,
            root: root.to_path_buf(),
        })
    }
    fn read_bound(
        &self,
        request: &SemanticBookCatalogRequest,
    ) -> anyhow::Result<SemanticBookCatalogResult> {
        self.provider.verify()?;
        let catalog = self.provider.catalog()?;
        let data = match request.operation.as_str() {
            "verify_sources" => self.verify_sources(&request.arguments)?,
            "run" | "verify" => self.run_package(&request.operation, &request.arguments)?,
            _ => semantica_service::books::catalog_operation(&catalog, request)?,
        };
        self.provider.verify()?;
        let result = SemanticBookCatalogResult {
            schema: "semantica.book-catalog-result/1".into(),
            source_sha256: self.provider.archive_sha256().into(),
            data,
        };
        anyhow::ensure!(
            result.validate().is_ok(),
            "book_catalog_result_contract_invalid"
        );
        Ok(result)
    }
    fn verify_sources(&self, args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        use serde_json::json;
        let bad = |message: &str| json!({"ok":false,"schema_version":"1.0","error":{"code":"invalid_argument","message":message}});
        let Some(object) = args.as_object() else {
            return Ok(bad("tool arguments must be an object"));
        };
        if object
            .keys()
            .any(|key| !["book_root", "volume"].contains(&key.as_str()))
        {
            return Ok(bad("Unsupported book-verification argument."));
        }
        let Some(book_root) = args["book_root"].as_str().filter(|s| !s.trim().is_empty()) else {
            return Ok(bad("book_root must be a non-empty string"));
        };
        let volume = match args.get("volume") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(s)) if ["vol1", "vol2"].contains(&s.as_str()) => {
                Some(s.as_str())
            }
            _ => return Ok(bad("volume must be 'vol1' or 'vol2'")),
        };
        // Wire paths select the already injected registration, not a new source authority.
        let selected = std::path::Path::new(book_root.trim());
        oe_core::no_symlinks(selected)?;
        if selected.canonicalize()? != self.root.canonicalize()? {
            return Ok(bad(
                "book_root must match the host-injected registered book root.",
            ));
        }
        let report = super::rust::verify_sources_with_volume(&self.root, volume)?;
        let mut data = json!({"ok":report["passed"]==true,"operation":"verify_book_sources","result":report,"schema_version":"1.0"});
        if data["ok"] != true {
            data["error"] = json!({"code":"book_source_binding_failed","message":"Book-to-Semantica source verification is blocked."});
        }
        Ok(data)
    }
    fn run_package(
        &self,
        operation: &str,
        args: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let catalog = self.provider.catalog()?;
        let root = oe_core::SkillRoot::new(&self.root)?;
        let identity = super::rust::source_execution::runtime_identity(&root)?;
        let result = semantica_service::books::package_operation(
            &catalog,
            &self.provider,
            operation,
            args,
            &identity,
        )?;
        let current = super::rust::source_execution::runtime_identity(&root)?;
        for key in [
            "source_sha256",
            "runtime_commit",
            "runtime_artifact_sha256",
            "runtime_artifact_size_bytes",
        ] {
            anyhow::ensure!(
                current[key] == identity[key],
                "book_task_runtime_observation_drift"
            );
        }
        Ok(result)
    }
}
impl SemanticBookCatalogPort for RegisteredBookCatalog {
    fn read(
        &self,
        request: &SemanticBookCatalogRequest,
    ) -> Result<SemanticBookCatalogResult, SemanticRuntimeFailure> {
        self.read_bound(request)
            .map_err(|_| SemanticRuntimeFailure {
                code: "book_catalog_source_unavailable".into(),
                phase: "source".into(),
                message: "The registered book source did not pass its source-bound read.".into(),
                retryable: false,
            })
    }
}
