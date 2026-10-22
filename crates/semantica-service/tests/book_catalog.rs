use semantica_contracts::{
    SemanticBookCatalogPort, SemanticBookCatalogRequest, SemanticBookCatalogResult,
    SemanticRuntimeFailure,
};
use semantica_runtime::{
    books::{Catalog, SourceProvider},
    dataset::DatasetSession,
};
use semantica_service::{McpService, NativeServiceBudget};
use serde_json::{Value, json};

fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/original-book-catalog-oracle.json")).unwrap()
}

struct FrozenSource(Value);
impl SourceProvider for FrozenSource {
    fn read(&self, id: &str) -> anyhow::Result<Vec<u8>> {
        let raw = self.0["provider_files"][id]["utf8"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("unregistered fixture source"))?
            .as_bytes()
            .to_vec();
        anyhow::ensure!(
            semantica_runtime::sha256(&raw) == self.0["provider_files"][id]["sha256"],
            "fixture source drift"
        );
        Ok(raw)
    }
}
impl SemanticBookCatalogPort for FrozenSource {
    fn read(
        &self,
        request: &SemanticBookCatalogRequest,
    ) -> Result<SemanticBookCatalogResult, SemanticRuntimeFailure> {
        let catalog = Catalog::read(self, "semantica/chapter_packages/registry.yaml").unwrap();
        Ok(SemanticBookCatalogResult {
            schema: "semantica.book-catalog-result/1".into(),
            source_sha256: self.0["archive_sha256"].as_str().unwrap().into(),
            data: semantica_service::books::catalog_operation(&catalog, request).unwrap(),
        })
    }
}

#[test]
fn original_47_book_catalog_cases_and_34_resources() {
    let expected = oracle();
    let mut session = DatasetSession::new();
    let before = session.snapshot().unwrap();
    let budget = NativeServiceBudget::bounded();
    let source = FrozenSource(expected.clone());
    {
        let service = McpService::new(&mut session, &budget).with_book_catalog(&source);
        for case in expected["cases"].as_array().unwrap() {
            let name = if case["operation"] == "list" {
                "list_chapter_packages"
            } else {
                "get_chapter_package"
            };
            let output = service.handle(&json!({"id":1,"method":"tools/call","params":{"name":name,"arguments":case["arguments"]}})).unwrap().unwrap();
            let actual: Value =
                serde_json::from_str(output["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(actual, case["expected"], "original case {}", case["id"]);
        }
        let output = service
            .handle(&json!({"id":2,"method":"resources/list"}))
            .unwrap()
            .unwrap();
        let actual: Vec<Value> = output["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let mut r = r.clone();
                r.as_object_mut().unwrap().remove("_meta");
                r
            })
            .collect();
        assert_eq!(
            actual,
            expected["listed_resources"].as_array().unwrap().clone()
        );
        for case in expected["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["id"].as_str().unwrap().starts_with("get-"))
        {
            let uri = format!(
                "semantica://packages/manifest/{}",
                case["arguments"]["package_id"].as_str().unwrap()
            );
            let output = service
                .handle(&json!({"id":3,"method":"resources/read","params":{"uri":uri}}))
                .unwrap()
                .unwrap();
            let actual: Value =
                serde_json::from_str(output["result"]["contents"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(
                actual, case["expected"],
                "original manifest resource {}",
                case["id"]
            );
            assert_eq!(output["result"]["contents"][0]["uri"], uri);
        }
    }
    assert_eq!(before, session.snapshot().unwrap());
}

#[test]
fn exact_manifest_ids_are_not_normalized_or_used_as_paths() {
    let mut session = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let source = FrozenSource(oracle());
    let service = McpService::new(&mut session, &budget).with_book_catalog(&source);
    for id in [
        "../vol1/ch01",
        "semantica.chapter_packages.vol1.ch01/../ch02",
        "semantica.chapter_packages.vol1.ch01%2F",
        "semantica://packages/manifest/semantica.chapter_packages.vol1.ch01",
        "/tmp/manifest.yaml",
    ] {
        let uri = format!("semantica://packages/manifest/{id}");
        let output = service
            .handle(&json!({"id":1,"method":"resources/read","params":{"uri":uri}}))
            .unwrap()
            .unwrap();
        let data: Value =
            serde_json::from_str(output["result"]["contents"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(data["error"]["code"], "package_not_found");
    }
    let tools = service.tools().unwrap();
    assert_eq!(tools.len(), 17);
    assert_eq!(
        tools
            .iter()
            .filter(|t| t.availability == "available")
            .count(),
        3
    );
    for name in [
        "run_chapter_package",
        "verify_chapter_package",
        "verify_book_sources",
    ] {
        assert_eq!(
            tools.iter().find(|t| t.name == name).unwrap().availability,
            "unavailable"
        );
    }
}

#[test]
fn provider_failure_invalid_result_and_budget_leave_dataset_unchanged() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct BadSource {
        calls: AtomicUsize,
        invalid: bool,
    }
    impl SemanticBookCatalogPort for BadSource {
        fn read(
            &self,
            _: &SemanticBookCatalogRequest,
        ) -> Result<SemanticBookCatalogResult, SemanticRuntimeFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.invalid {
                return Ok(SemanticBookCatalogResult {
                    schema: "semantica.book-catalog-result/1".into(),
                    source_sha256: "forged".into(),
                    data: json!({"ok":true,"passed":true}),
                });
            }
            Err(SemanticRuntimeFailure {
                code: "/private/source/password".into(),
                phase: "source".into(),
                message: "private content".into(),
                retryable: false,
            })
        }
    }
    for invalid in [false, true] {
        let mut session = DatasetSession::new();
        let before = session.snapshot().unwrap();
        let source = BadSource {
            calls: AtomicUsize::new(0),
            invalid,
        };
        let budget = NativeServiceBudget::bounded();
        {
            let service = McpService::new(&mut session, &budget).with_book_catalog(&source);
            let output=service.handle(&json!({"id":1,"method":"tools/call","params":{"name":"list_chapter_packages"}})).unwrap().unwrap();
            assert_eq!(output["result"]["isError"], true);
            let data: Value =
                serde_json::from_str(output["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(data["error"]["code"], "package_operation_failed");
            assert!(!data.to_string().contains("private"));
            let resources = service
                .handle(&json!({"id":2,"method":"resources/list"}))
                .unwrap()
                .unwrap();
            assert_eq!(
                resources["result"]["resources"].as_array().unwrap().len(),
                4
            );
        }
        assert_eq!(before, session.snapshot().unwrap());
        let budget = NativeServiceBudget::new(0, 10000, 10000);
        let calls = source.calls.load(Ordering::SeqCst);
        let service = McpService::new(&mut session, &budget).with_book_catalog(&source);
        assert_eq!(
            service
                .handle(
                    &json!({"id":1,"method":"tools/call","params":{"name":"list_chapter_packages"}})
                )
                .unwrap_err()
                .code,
            "service_budget_exceeded"
        );
        assert_eq!(source.calls.load(Ordering::SeqCst), calls);
    }
}
