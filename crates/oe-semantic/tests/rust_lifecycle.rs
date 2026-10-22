use anyhow::Result;
use oe_core::SkillRoot;
use oe_semantic::{
    native::{OpenEngagementRequest, ProposeRequest, RegistryRequest},
    refinery_runtime::RustRuntime,
    refinery_storage::FileStore,
};
use semantica_runtime::refinery::{RefineryEngine, RefineryStore};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
fn original() -> Value {
    serde_json::from_str(ORIGINAL).unwrap()
}
fn private_run() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let path = root.join("var/lifecycle-test").join(format!(
        "{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
#[test]
fn current_rust_lifecycle_records_blocked_observation_and_rejects_caller_passed_receipt() {
    let input = original();
    let runtime = RustRuntime::new(&root()).unwrap();
    let path = private_run().join("registry");
    let open = OpenEngagementRequest {
        workspace: path.clone(),
        binding: input["binding"].clone(),
        envelope: input["candidate_envelope"].clone(),
    };
    let result = runtime.open_engagement(&open);
    assert_eq!(result.semantic_execution, "completed", "{:?}", result);
    assert!(
        result.runtime_identity.is_none(),
        "No legacy Python/wheel proof invented"
    );
    let value = result.result.unwrap();
    assert_eq!(value["recorded"], true);
    assert_eq!(value["ontology_truth_mutated"], false);
    assert_eq!(value["engagement"]["status"], "blocked");
    assert_eq!(value["engagement"]["learning"]["status"], "no_delta");
    assert_eq!(
        value["engagement"]["profile"],
        "semantica.refinery-rust-observation/1"
    );
    assert_eq!(
        value["engagement"]["runtime_source"]["compilation_origin"],
        "unknown"
    );
    assert_eq!(value["source_recheck"]["status"], "matched");
    let receipt =
        semantica_runtime::refinery_contracts::rust_engagement(&value["engagement"]).unwrap();
    assert_eq!(receipt, value["engagement"]);
    let registry: Value =
        serde_json::from_slice(&fs::read(path.join("registry.json")).unwrap()).unwrap();
    assert_eq!(registry["packages"], json!({}));
    let files = fs::read_dir(path.join("engagements")).unwrap().count();
    assert_eq!(files, 1);
    let again = runtime.open_engagement(&open);
    assert_eq!(again.semantic_execution, "completed");
    assert_eq!(
        fs::read_dir(path.join("engagements")).unwrap().count(),
        1,
        "Exact retry creates no second observation"
    );
    let proposed = runtime.propose_candidate(&ProposeRequest {
        registry: RegistryRequest {
            workspace: path.clone(),
            binding: input["binding"].clone(),
        },
        delta: input["delta"].clone(),
        candidate_envelope: input["candidate_envelope"].clone(),
        proposed_envelope: input["proposed_context"]["envelope"].clone(),
        engagement: input["complete_synthetic_original_engagement"].clone(),
        recorded_at: Some(input["fixed_time"].as_str().unwrap().into()),
    });
    assert_eq!(proposed.semantic_execution, "failed");
    assert!(
        proposed
            .error
            .unwrap()
            .contains("refinery_caller_passed_receipt_is_untrusted")
    );
    assert_eq!(
        proposed.result.unwrap()["source_recheck"]["status"],
        "matched"
    );
    assert_eq!(fs::read_dir(path.join("engagements")).unwrap().count(), 1);
    assert_eq!(fs::read_dir(path.join("refinements")).unwrap().count(), 0);
}
struct Failure<'a> {
    store: &'a FileStore,
    fail: Cell<bool>,
}
impl RefineryStore for Failure<'_> {
    fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
        self.store.read(p)
    }
    fn write_new(&self, p: &str, b: &[u8]) -> Result<()> {
        if p.ends_with("/delta.json") && self.fail.replace(false) {
            anyhow::bail!("independent_storage_write_fault")
        }
        self.store.write_new(p, b)
    }
    fn replace(&self, p: &str, e: &[u8], b: &[u8]) -> Result<()> {
        self.store.replace(p, e, b)
    }
    fn remove(&self, p: &str, e: &[u8]) -> Result<()> {
        self.store.remove(p, e)
    }
    fn list(&self, p: &str) -> Result<Vec<String>> {
        self.store.list(p)
    }
    fn create_dir(&self, p: &str) -> Result<()> {
        self.store.create_dir(p)
    }
    fn ensure_dir(&self, p: &str) -> Result<()> {
        self.store.ensure_dir(p)
    }
    fn publish_dir(&self, a: &str, b: &str) -> Result<()> {
        self.store.publish_dir(a, b)
    }
    fn discard_stage(&self, p: &str) -> Result<()> {
        self.store.discard_stage(p)
    }
}
#[test]
fn real_private_store_prevalidates_time_cleans_unpublished_failure_and_retries() {
    let input = original();
    let run = private_run();
    let path = run.join("registry");
    let skill = SkillRoot::new(root()).unwrap();
    assert!(FileStore::open(&skill, &path, "registry-1", "bad-time", true).is_err());
    assert!(!path.exists());
    assert!(
        FileStore::open(
            &skill,
            &path,
            "../unsafe",
            input["fixed_time"].as_str().unwrap(),
            true
        )
        .is_err()
    );
    assert!(!path.exists());
    let store = FileStore::open(
        &skill,
        &path,
        "registry-1",
        input["fixed_time"].as_str().unwrap(),
        true,
    )
    .unwrap();
    let engine = RefineryEngine::new(&store, "registry-1").unwrap();
    assert!(
        engine
            .register_data(
                &input["delta"],
                &input["candidate_envelope"],
                &input["binding"],
                &input["complete_synthetic_original_engagement"],
                "bad-time"
            )
            .is_err()
    );
    assert_eq!(fs::read_dir(path.join("engagements")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(path.join("refinements")).unwrap().count(), 0);
    let failing = Failure {
        store: &store,
        fail: Cell::new(true),
    };
    let faulty = RefineryEngine::new(&failing, "registry-1").unwrap();
    let error = faulty
        .register_data(
            &input["delta"],
            &input["candidate_envelope"],
            &input["binding"],
            &input["complete_synthetic_original_engagement"],
            input["fixed_time"].as_str().unwrap(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("independent_storage_write_fault"));
    assert_eq!(
        fs::read_dir(path.join("refinements")).unwrap().count(),
        0,
        "Fresh stage cleaned; retained CAS and engagement are not reset"
    );
    let state = engine
        .register_data(
            &input["delta"],
            &input["candidate_envelope"],
            &input["binding"],
            &input["complete_synthetic_original_engagement"],
            input["fixed_time"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(state["state"], "candidate");
    assert_eq!(fs::read_dir(path.join("refinements")).unwrap().count(), 1);
    assert_eq!(fs::read_dir(path.join("engagements")).unwrap().count(), 1);
    let registry: Value =
        serde_json::from_slice(&fs::read(path.join("registry.json")).unwrap()).unwrap();
    assert_eq!(registry["packages"], json!({}));
    assert!(
        store
            .discard_stage("refinements/ordinary-candidate")
            .is_err()
    );
}

const ORIGINAL: &str = r########"{"schema":"semantica.lifecycle.original-control-inputs/1","source_commit":"8b2ed123c380fc8abc78507e6e6648e6f5c0052c","source_freeze_sha256":"079241c9ee971e5c6dafed6c1918c8e86a02cb5c1e9878c981a1c70208d14caa","case_ids":["original-pure-five-assertion-functions","original-open-complete-synthetic-test-input","original-open-blocked-runtime-input","original-propose-context-rejections","original-propose-two-event-chain","original-propose-retry-exact","original-propose-retry-different-context","original-commit-authority-rejections","original-commit-materialization","original-execute-dependency-boundary","original-delta-four-book-impacts","original-delta-invalid-book-impact","original-delta-missing-array","original-delta-declared-hash-tamper"],"declared_scope":"finite original refinery control/DTO/filesystem profile; synthetic original test inputs; RDF runner not supplied","delta":{"schema_version":"1.0","package_id":"industry.example","base_version":"0","base_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","target_version":"1.0.0","rationale":"Repeated engineering practice merits a governed candidate.","created_by":"fact.authority","created_at":"2026-08-19T00:00:00Z","required_capabilities":["semantic.package.load"],"source_evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"book_impact":"none","ontology":[{"category":"ontology","asset_id":"ontology-core","operation":"add","media_type":"text/turtle","sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","content_base64":"QHByZWZpeCBvd2w6IDxodHRwOi8vd3d3LnczLm9yZy8yMDAyLzA3L293bCM+IC4KPHVybjppbmR1c3RyeTpleGFtcGxlPiBhIG93bDpPbnRvbG9neSAuCg==","replaces_sha256":null,"role":null,"case_kind":null}],"competency_questions":[{"category":"competency_questions","asset_id":"cq-registry","operation":"add","media_type":"application/json","sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","content_base64":"eyJjb21wZXRlbmN5X3F1ZXN0aW9ucyI6W3siaWQiOiJjcS1jdXJyZW50IiwicXVlc3Rpb24iOiJDYW4gdGhlIGN1cnJlbnQgcmV1c2FibGUgY2FzZSBiZSByZXRyaWV2ZWQ/Iiwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLWN1cnJlbnQiXX0seyJpZCI6ImNxLXByaW9yIiwicXVlc3Rpb24iOiJEb2VzIHRoZSBwcmlvci1yZWxlYXNlIGNhc2Ugc3RpbGwgZXhlY3V0ZT8iLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX1dLCJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"shapes":[{"category":"shapes","asset_id":"quality-shapes","operation":"add","media_type":"text/turtle","sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","content_base64":"QHByZWZpeCBzaDogPGh0dHA6Ly93d3cudzMub3JnL25zL3NoYWNsIz4gLgpAcHJlZml4IGV4OiA8dXJuOmV4YW1wbGU6PiAuCmV4Ok5hbWVkU2hhcGUgYSBzaDpOb2RlU2hhcGUgOyBzaDp0YXJnZXRTdWJqZWN0c09mIGV4Om5hbWUgLgo=","replaces_sha256":null,"role":null,"case_kind":null}],"queries":[{"category":"queries","asset_id":"cq-primary","operation":"add","media_type":"application/sparql-query","sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","content_base64":"U0VMRUNUID9uYW1lIFdIRVJFIHsgP3MgPHVybjpleGFtcGxlOm5hbWU+ID9uYW1lIC4gfSBPUkRFUiBCWSA/bmFtZQo=","replaces_sha256":null,"role":null,"case_kind":null}],"rules":[{"category":"rules","asset_id":"engineering-rules","operation":"add","media_type":"text/plain","sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","content_base64":"SUYgb2JzZXJ2ZWQoP3gpIFRIRU4gcmV1c2FibGUoP3gpCg==","replaces_sha256":null,"role":null,"case_kind":null}],"cases":[{"category":"cases","asset_id":"case-positive","operation":"add","media_type":"text/turtle","sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","content_base64":"PHVybjpjYXNlOnBvc2l0aXZlPiA8dXJuOmV4YW1wbGU6bmFtZT4gInBvc2l0aXZlIiAuCg==","replaces_sha256":null,"role":null,"case_kind":"positive"},{"category":"cases","asset_id":"case-negative","operation":"add","media_type":"text/turtle","sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","content_base64":"IyBkZWxpYmVyYXRlbHkgZW1wdHkgbmVnYXRpdmUgZ3JhcGgK","replaces_sha256":null,"role":null,"case_kind":"negative"},{"category":"cases","asset_id":"case-ambiguity","operation":"add","media_type":"text/turtle","sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","content_base64":"PHVybjpjYXNlOmFtYmlndWl0eT4gPHVybjpleGFtcGxlOm5hbWU+ICJhbWJpZ3VpdHkiIC4K","replaces_sha256":null,"role":null,"case_kind":"ambiguity"},{"category":"cases","asset_id":"case-prior_release","operation":"add","media_type":"text/turtle","sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","content_base64":"PHVybjpjYXNlOnByaW9yX3JlbGVhc2U+IDx1cm46ZXhhbXBsZTpuYW1lPiAicHJpb3IiIC4K","replaces_sha256":null,"role":null,"case_kind":"prior_release"}],"contract":[{"category":"contract","asset_id":"package-contract","operation":"add","media_type":"application/json","sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","content_base64":"eyJhc3NldHMiOnsiY2FzZS1hbWJpZ3VpdHkiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNhc2UifSwiY2FzZS1uZWdhdGl2ZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjYXNlLXBvc2l0aXZlIjp7ImZvcm1hdCI6InR1cnRsZSIsImtpbmQiOiJyZGYiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjYXNlIn0sImNhc2UtcHJpb3JfcmVsZWFzZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjcS1wcmltYXJ5Ijp7ImZvcm1hdCI6InNwYXJxbCIsImtpbmQiOiJ0ZXh0IiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic3BhcnFsIn0sImNxLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNvbXBldGVuY3lfcXVlc3Rpb25zIn0sImVuZ2luZWVyaW5nLXJ1bGVzIjp7ImZvcm1hdCI6InRleHQiLCJraW5kIjoidGV4dCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImVuZ2luZWVyaW5nX3J1bGVzIn0sIm9udG9sb2d5LWNvcmUiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0Ijp0cnVlLCJyb2xlIjoib250b2xvZ3kifSwicGFja2FnZS1jb250cmFjdCI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjb250cmFjdCJ9LCJxdWFsaXR5LXNoYXBlcyI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic2hhcGVzIn0sInJpZ2h0cy1ldmlkZW5jZSI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJwcm92ZW5hbmNlIn0sInNjZW5hcmlvLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InNjZW5hcmlvX3JlZ2lzdHJ5In0sInNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIjp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InByb3ZlbmFuY2UifX0sImV4ZWN1dGlvbiI6eyJjcV9yZWdpc3RyeV9hc3NldF9pZCI6ImNxLXJlZ2lzdHJ5Iiwic2NlbmFyaW9fcmVnaXN0cnlfYXNzZXRfaWQiOiJzY2VuYXJpby1yZWdpc3RyeSJ9LCJnYXRlX3N1aXRlIjp7ImNhc2UuYW1iaWd1aXR5Ijp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtYW1iaWd1aXR5Il0sImNxX2lkcyI6W10sInNjZW5hcmlvX2lkcyI6WyJzY2VuYXJpby1hbWJpZ3VpdHkiXX0sImNhc2UubmVnYXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1uZWdhdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tbmVnYXRpdmUiXX0sImNhc2UucG9zaXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1wb3NpdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY2FzZS5wcmlvcl9yZWxlYXNlIjp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtcHJpb3JfcmVsZWFzZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX0sImNxLmN1cnJlbnQiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtY3VycmVudCJdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY3EucHJpb3IiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtcHJpb3IiXSwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLXByaW9yIl19fSwibmFtZXNwYWNlIjoidXJuOmluZHVzdHJ5OmV4YW1wbGU6IiwicHJvdmVuYW5jZV9ldmlkZW5jZV9hc3NldF9pZHMiOlsic291cmNlLWV2aWRlbmNlLWJpbmRpbmciXSwicmVsZWFzZV9zdGF0dXMiOiJjb21wbGV0ZSIsInJpZ2h0c19ldmlkZW5jZV9hc3NldF9pZHMiOlsicmlnaHRzLWV2aWRlbmNlIl0sInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"provenance":[{"category":"provenance","asset_id":"scenario-registry","operation":"add","media_type":"application/json","sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","content_base64":"eyJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjZW5hcmlvcyI6W3siZXhlY3V0aW9uIjp7Im9wZXJhdGlvbiI6InNlbGVjdF9leGFjdF9tdWx0aXNldCIsInByb3ZpZGVyIjoiU2VtYW50aWNSdW50aW1lIiwicXVlcnlfYXNzZXRfaWQiOiJjcS1wcmltYXJ5In0sImlkIjoic2NlbmFyaW8tY3VycmVudCIsImlucHV0cyI6WyJjYXNlLXBvc2l0aXZlIiwiY2FzZS1uZWdhdGl2ZSJdLCJvcmFjbGUiOnsiaWQiOiJvcmFjbGUtc2NlbmFyaW8tY3VycmVudCIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicG9zaXRpdmUiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLW5lZ2F0aXZlIiwiaW5wdXRzIjpbImNhc2UtbmVnYXRpdmUiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1uZWdhdGl2ZSIsInBvc2l0aXZlIjp7InJvd19jb3VudCI6MH0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLWFtYmlndWl0eSIsImlucHV0cyI6WyJjYXNlLWFtYmlndWl0eSIsImNhc2UtbmVnYXRpdmUiXSwib3JhY2xlIjp7ImlkIjoib3JhY2xlLXNjZW5hcmlvLWFtYmlndWl0eSIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsiYW1iaWd1aXR5Il19LCJyb3dfY291bnQiOjF9LCJzaW5nbGVfZmF1bHRfbmVnYXRpdmUiOnsicm93X2NvdW50IjowfSwic3RhdHVzIjoicmVhZHkifSwic3RhdHVzIjoibmF0aXZlIn0seyJleGVjdXRpb24iOnsib3BlcmF0aW9uIjoic2VsZWN0X2V4YWN0X211bHRpc2V0IiwicHJvdmlkZXIiOiJTZW1hbnRpY1J1bnRpbWUiLCJxdWVyeV9hc3NldF9pZCI6ImNxLXByaW1hcnkifSwiaWQiOiJzY2VuYXJpby1wcmlvciIsImlucHV0cyI6WyJjYXNlLXByaW9yX3JlbGVhc2UiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1wcmlvciIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicHJpb3IiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifV0sInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"rights-evidence","operation":"add","media_type":"application/json","sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","content_base64":"eyJzY2hlbWFfdmVyc2lvbiI6IjEuMCIsInNvdXJjZV9pZCI6ImVuZ2luZWVyaW5nLXJlY29yZCIsInNvdXJjZV9zaGEyNTYiOiJhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhIiwic3RhdGVtZW50IjoiUmlnaHRzIGV2aWRlbmNlIHJldGFpbmVkOyBubyBsZWdhbCBjb25jbHVzaW9uLiJ9","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"source-evidence-binding","operation":"add","media_type":"application/json","sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","content_base64":"eyJraW5kIjoic2VtYW50aWNhLnNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIiwic2NoZW1hX3ZlcnNpb24iOiIxLjAiLCJzb3VyY2VfZXZpZGVuY2UiOlt7ImNhcHR1cmVkX2F0IjoiMjAyNi0wOC0xOVQwMDowMDowMFoiLCJtZWRpYV90eXBlIjoiYXBwbGljYXRpb24vanNvbiIsInNoYTI1NiI6ImFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWEiLCJzb3VyY2VfaWQiOiJlbmdpbmVlcmluZy1yZWNvcmQiLCJ1cmkiOiJ1cm46ZXhhbXBsZTpldmlkZW5jZS1yb290OmVuZ2luZWVyaW5nLXJlY29yZCJ9XX0=","replaces_sha256":null,"role":null,"case_kind":null}],"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"candidate_envelope":{"schema_version":"1.0","task_id":"task-candidate","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["candidate"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436"},"complete_synthetic_original_engagement":{"schema_version":"1.0","contract":"semantica.ontology.refinery/v1","engagement_id":"engagement-1","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee","required_capabilities":["semantic.package.load"],"runtime_source":{"runtime_commit":"1111111111111111111111111111111111111111","runtime_artifact_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","runtime_version":"0.6.5+oe.2"},"execution":{"name":"execution","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"regression":{"name":"regression","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"receipt":{"name":"receipt","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"release":{"name":"release","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"learning":{"status":"candidate","rationale":"The lesson is reusable outside this project.","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960"},"execution_receipts":[{"receipt_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","package_id":"semantica.chapter.reference","package_version":"1","package_digest":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}],"status":"complete","blocked_reasons":[],"created_at":"2026-08-19T00:00:00Z","receipt_sha256":"2e345ecbbdd7143d356bfb3fe321bc78c4354e3b9d80c053bfe5f650e1ceaeac"},"proposed_context":{"schema_version":"1.0","action":"proposed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"task-proposed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"a3004835a439319ab1e63ec0f3a33cff7a169e83ce335b25f55033eb6bebe617"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"context_sha256":"4dd2b54f1990466e84bd53ab765a7b54db9e1fb43c9c70f8b3ccf525b6700452"},"commit_authorization":{"authorization_id":"authorization-commit","action":"commit","actor_id":"decision.authority","authority":"decision.authority","package_id":"industry.example","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","promotion_target":"industry-registry","reason":"Reviewed against the bound evidence and project authority.","source":{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"},"issued_at":"2026-08-19T00:00:00Z","decisions":[],"authorization_sha256":"ab604f3e094adaa464a12600fb6ee7e8c7b5b1393cb7fad452f66d5499295067"},"fixed_time":"2026-08-19T00:00:00Z","original_constructor_bypassed":false,"business_stubs":false,"source_or_assertions_modified":false}"########;

#[test]
fn actual_nonzero_chat_candidate_records_conditionally_bound_subject_checks_without_adoption() {
    let fixture: Value = serde_json::from_str(NONZERO).unwrap();
    let input = original();
    let runtime = RustRuntime::new(&root()).unwrap();
    let run = private_run();
    let path = run.join("registry");
    let subject = run.join("subject");
    fs::create_dir(&subject).unwrap();
    for (name, value) in fixture["files"].as_object().unwrap() {
        fs::write(subject.join(name), value.as_str().unwrap()).unwrap();
    }
    let manifest = subject.join("manifest.json");
    fs::write(&manifest, serde_json::to_vec(&fixture["manifest"]).unwrap()).unwrap();
    let delta = semantica_runtime::refinery_contracts::delta(&fixture["delta"]).unwrap();
    let open = runtime.open_engagement(&OpenEngagementRequest {
        workspace: path.clone(),
        binding: input["binding"].clone(),
        envelope: input["candidate_envelope"].clone(),
    });
    assert_eq!(open.semantic_execution, "completed");
    let mut request = ProposeRequest {
        registry: RegistryRequest {
            workspace: path.clone(),
            binding: input["binding"].clone(),
        },
        delta,
        candidate_envelope: input["candidate_envelope"].clone(),
        proposed_envelope: input["proposed_context"]["envelope"].clone(),
        engagement: json!({"schema":"semantica.refinery-observation-request/1","manifest_path":manifest.to_str().unwrap(),"scenario_ids":["s"]}),
        recorded_at: Some(input["fixed_time"].as_str().unwrap().into()),
    };
    let result = runtime.propose_candidate(&request);
    assert_eq!(result.semantic_execution, "completed", "{:?}", result);
    let value = result.result.unwrap();
    assert_eq!(value["candidate_recorded"], true);
    assert_eq!(value["candidate_state"], "not_registered");
    assert!(value["state"].is_null());
    assert_eq!(value["ontology_truth_mutated"], false);
    let receipt = &value["engagement"];
    assert_eq!(receipt["status"], "blocked");
    assert_eq!(receipt["learning"]["status"], "candidate");
    assert_eq!(
        receipt["learning"]["delta_sha256"],
        request.delta["delta_sha256"]
    );
    let check = &receipt["execution"]["details"]["actual_authoring_checks"][0];
    assert_eq!(check["execution"]["status"], "passed");
    assert_eq!(
        check["execution"]["operations"][1]["payload"]["row_count"],
        1
    );
    assert_eq!(
        check["execution"]["operations"][3]["payload"]["row_count"],
        0
    );
    assert_eq!(check["dataset"]["quad_count"], 2);
    assert_eq!(check["verification"]["status"], "blocked");
    assert_eq!(
        check["verification"]["receipt_sha256"],
        check["execution"]["receipt"]["receipt_sha256"]
    );
    assert_eq!(check["execution"]["created_at"], input["fixed_time"]);
    assert_eq!(check["engineering_adoption"], "unknown");
    assert_eq!(
        receipt["execution"]["details"]["conditional_delta"],
        request.delta
    );
    let count = fs::read_dir(path.join("engagements")).unwrap().count();
    assert_eq!(count, 2);
    assert_eq!(fs::read_dir(path.join("refinements")).unwrap().count(), 0);
    let registry: Value =
        serde_json::from_slice(&fs::read(path.join("registry.json")).unwrap()).unwrap();
    assert_eq!(registry["packages"], json!({}));
    let mut bad = fixture["manifest"].clone();
    bad["assets"][0]["role"] = json!("shapes");
    fs::write(&manifest, serde_json::to_vec(&bad).unwrap()).unwrap();
    let error = runtime.propose_candidate(&request);
    assert_eq!(error.semantic_execution, "failed");
    assert!(
        error
            .error
            .unwrap()
            .contains("refinery_delta_manifest_asset_binding_mismatch")
    );
    assert_eq!(
        fs::read_dir(path.join("engagements")).unwrap().count(),
        count
    );
    fs::write(&manifest, serde_json::to_vec(&fixture["manifest"]).unwrap()).unwrap();
    request.engagement["status"] = json!("passed");
    let error = runtime.propose_candidate(&request);
    assert_eq!(error.semantic_execution, "failed");
    assert_eq!(
        fs::read_dir(path.join("engagements")).unwrap().count(),
        count
    );
}

const NONZERO: &str = r########"{"profile":"current-rust-observation independent fixed finite subject; existing raw select fixture copied unchanged","original_book_runner_test_source_sha256":"1c67ace3675a74e932a7424671264aa7a7a9db03da3d428f4ecc781c5b04a285","files":{"positive.ttl":"@prefix x:<urn:x:>. x:a a x:Type; x:name \"ready\" .","negative.ttl":"@prefix x:<urn:x:>. x:a a x:Type .","query.rq":"PREFIX x:<urn:x:> SELECT ?value WHERE { ?s x:name ?value }","shapes.ttl":"@prefix x:<urn:x:>. @prefix sh:<http://www.w3.org/ns/shacl#>. x:Shape a sh:NodeShape; sh:targetClass x:Type; sh:property [sh:path x:name; sh:minCount 1; sh:message \"required field\"] .","facts.txt":"# original text boundary\nA(a)\n","rules.txt":"IF A(?x) THEN B(?x)\nIF B(?x) THEN C(?x)\n","contract.json":"{\"status\":\"partial\"}","scenarios.json":"{\"runner_contract\":\"semantica.chapter_packages.SemanticPackageRunner/v1\",\"scenarios\":[{\"id\":\"s\",\"inputs\":[\"positive\",\"negative\"],\"execution\":{\"operation\":\"select_exact_multiset\",\"query_asset_id\":\"query\"},\"oracle\":{\"status\":\"ready\",\"positive\":{\"row_count\":1,\"bindings\":{\"value\":[\"ready\"]}},\"single_fault_negative\":{\"row_count\":0}}}]}","cqs.json":"{\"competency_questions\":[{\"id\":\"cq\",\"scenario_ids\":[\"s\"]}]}"},"manifest":{"schema_version":"1.0","package_id":"industry.example","version":"1.0.0","namespace":"urn:x:","release_status":"partial","assets":[{"asset_id":"positive","role":"data","path":"positive.ttl","format":"turtle","load_into_dataset":false,"sha256":"b905780e32be0ac65cd2a7761e110b1e147de767a109ffef668b0c012dd34156"},{"asset_id":"negative","role":"data","path":"negative.ttl","format":"turtle","load_into_dataset":false,"sha256":"6179826fd5f58b54b1c93455f4f87ea5db4ce73f115fe40aa8d7e4b6381c0ebb"},{"asset_id":"query","role":"sparql","path":"query.rq","format":"sparql","load_into_dataset":false,"sha256":"00dd08438fe37e7bed33a25bbec5945af7453f837ffdf2417d1af0e69fde9daf"},{"asset_id":"shapes","role":"shapes","path":"shapes.ttl","format":"turtle","load_into_dataset":false,"sha256":"bee4e4fe6748c235058ad39581e52c1e7302893dad55eb962cb7a1494abf5d04"},{"asset_id":"facts","role":"facts","path":"facts.txt","format":"semantica-fact","load_into_dataset":false,"sha256":"a20c8a0a6e328b73063fffa41db2cece273cecabe5b5ad137780fa995071fe45"},{"asset_id":"rules","role":"rules","path":"rules.txt","format":"semantica-rule","load_into_dataset":false,"sha256":"fae16254205230269ea18e4263a1a1ea07d7e50b1a7bc3e92a7cede9523085bf"},{"asset_id":"contract","role":"chapter_contract","path":"contract.json","format":"json","load_into_dataset":false,"sha256":"9cfc0bebd9ff537aea2ef9262e3de4c13d0bfe29e7b7d03f49d5de8eda0e6497"},{"asset_id":"scenarios","role":"scenario_registry","path":"scenarios.json","format":"json","load_into_dataset":false,"sha256":"02085dd751558586db7845ad7dc1406aa0f75990dc7c6af58303ffe7f64bfaa7"},{"asset_id":"cqs","role":"cq_registry","path":"cqs.json","format":"json","load_into_dataset":false,"sha256":"91daec6906c1a606fe7000beb0804f39cbba3bba06c23f3d144dcf07bf814642"}],"execution":{"scenario_registry_asset_id":"scenarios","cq_registry_asset_id":"cqs"}},"delta":{"schema_version":"1.0","package_id":"industry.example","base_version":"0","base_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","target_version":"1.0.0","rationale":"Repeated engineering practice merits a governed candidate.","created_by":"fact.authority","created_at":"2026-08-19T00:00:00Z","required_capabilities":["semantic.package.load"],"source_evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"book_impact":"none","ontology":[{"category":"ontology","asset_id":"facts","operation":"add","media_type":"text/plain","sha256":"a20c8a0a6e328b73063fffa41db2cece273cecabe5b5ad137780fa995071fe45","content_base64":"IyBvcmlnaW5hbCB0ZXh0IGJvdW5kYXJ5CkEoYSkK","replaces_sha256":null,"role":"facts","case_kind":null}],"competency_questions":[{"category":"competency_questions","asset_id":"cqs","operation":"add","media_type":"application/json","sha256":"91daec6906c1a606fe7000beb0804f39cbba3bba06c23f3d144dcf07bf814642","content_base64":"eyJjb21wZXRlbmN5X3F1ZXN0aW9ucyI6W3siaWQiOiJjcSIsInNjZW5hcmlvX2lkcyI6WyJzIl19XX0=","replaces_sha256":null,"role":"cq_registry","case_kind":null}],"shapes":[{"category":"shapes","asset_id":"shapes","operation":"add","media_type":"text/turtle","sha256":"bee4e4fe6748c235058ad39581e52c1e7302893dad55eb962cb7a1494abf5d04","content_base64":"QHByZWZpeCB4Ojx1cm46eDo+LiBAcHJlZml4IHNoOjxodHRwOi8vd3d3LnczLm9yZy9ucy9zaGFjbCM+LiB4OlNoYXBlIGEgc2g6Tm9kZVNoYXBlOyBzaDp0YXJnZXRDbGFzcyB4OlR5cGU7IHNoOnByb3BlcnR5IFtzaDpwYXRoIHg6bmFtZTsgc2g6bWluQ291bnQgMTsgc2g6bWVzc2FnZSAicmVxdWlyZWQgZmllbGQiXSAu","replaces_sha256":null,"role":"shapes","case_kind":null}],"queries":[{"category":"queries","asset_id":"query","operation":"add","media_type":"text/plain","sha256":"00dd08438fe37e7bed33a25bbec5945af7453f837ffdf2417d1af0e69fde9daf","content_base64":"UFJFRklYIHg6PHVybjp4Oj4gU0VMRUNUID92YWx1ZSBXSEVSRSB7ID9zIHg6bmFtZSA/dmFsdWUgfQ==","replaces_sha256":null,"role":"sparql","case_kind":null}],"rules":[{"category":"rules","asset_id":"rules","operation":"add","media_type":"text/plain","sha256":"fae16254205230269ea18e4263a1a1ea07d7e50b1a7bc3e92a7cede9523085bf","content_base64":"SUYgQSg/eCkgVEhFTiBCKD94KQpJRiBCKD94KSBUSEVOIEMoP3gpCg==","replaces_sha256":null,"role":"rules","case_kind":null}],"cases":[{"category":"cases","asset_id":"positive","operation":"add","media_type":"text/turtle","sha256":"b905780e32be0ac65cd2a7761e110b1e147de767a109ffef668b0c012dd34156","content_base64":"QHByZWZpeCB4Ojx1cm46eDo+LiB4OmEgYSB4OlR5cGU7IHg6bmFtZSAicmVhZHkiIC4=","replaces_sha256":null,"role":"data","case_kind":"positive"},{"category":"cases","asset_id":"negative","operation":"add","media_type":"text/turtle","sha256":"6179826fd5f58b54b1c93455f4f87ea5db4ce73f115fe40aa8d7e4b6381c0ebb","content_base64":"QHByZWZpeCB4Ojx1cm46eDo+LiB4OmEgYSB4OlR5cGUgLg==","replaces_sha256":null,"role":"data","case_kind":"negative"}],"contract":[{"category":"contract","asset_id":"contract","operation":"add","media_type":"application/json","sha256":"9cfc0bebd9ff537aea2ef9262e3de4c13d0bfe29e7b7d03f49d5de8eda0e6497","content_base64":"eyJzdGF0dXMiOiJwYXJ0aWFsIn0=","replaces_sha256":null,"role":"chapter_contract","case_kind":null},{"category":"contract","asset_id":"scenarios","operation":"add","media_type":"application/json","sha256":"02085dd751558586db7845ad7dc1406aa0f75990dc7c6af58303ffe7f64bfaa7","content_base64":"eyJydW5uZXJfY29udHJhY3QiOiJzZW1hbnRpY2EuY2hhcHRlcl9wYWNrYWdlcy5TZW1hbnRpY1BhY2thZ2VSdW5uZXIvdjEiLCJzY2VuYXJpb3MiOlt7ImlkIjoicyIsImlucHV0cyI6WyJwb3NpdGl2ZSIsIm5lZ2F0aXZlIl0sImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJxdWVyeV9hc3NldF9pZCI6InF1ZXJ5In0sIm9yYWNsZSI6eyJzdGF0dXMiOiJyZWFkeSIsInBvc2l0aXZlIjp7InJvd19jb3VudCI6MSwiYmluZGluZ3MiOnsidmFsdWUiOlsicmVhZHkiXX19LCJzaW5nbGVfZmF1bHRfbmVnYXRpdmUiOnsicm93X2NvdW50IjowfX19XX0=","replaces_sha256":null,"role":"scenario_registry","case_kind":null}],"provenance":[]},"expected":{"positive_rows":1,"negative_rows":0,"quads_after_isolated_negative":2,"actual_scenario_status":"passed","original_release":"blocked","candidate_recorded":true,"candidate_state":"not_registered","engagement":"blocked","industry_truth":{},"engineering_adoption":"unknown"}}"########;

#[test]
fn current_facade_projects_nested_rust_receipts_and_observation_plans_without_adoption() {
    let run = private_run();
    let binding_path = run.join("binding.json");
    let task_path = run.join("task.json");
    fs::write(&binding_path, CURRENT_BINDING).unwrap();
    fs::write(&task_path, CURRENT_TASK).unwrap();
    let workspace = run.join("registry");
    let observed =
        oe_semantic::lifecycle::open(&root(), &binding_path, &task_path, &workspace, None).unwrap();
    assert_eq!(observed["transport"]["status"], "completed");
    assert_eq!(observed["command_verdict"], "blocked");
    assert_eq!(
        observed["diagnostics"],
        json!([]),
        "Actual nested Rust receipt must be projected, not read as its wrapper"
    );
    assert_eq!(observed["learning"]["verdict"], "no_delta");
    assert_eq!(observed["receipt"]["status"], "source_bound");
    let receipt = &observed["execution"]["native_engagement_receipt"];
    assert_eq!(receipt["status"], "blocked");
    assert_eq!(
        observed["receipt"]["receipt_sha256"],
        receipt["receipt_sha256"]
    );
    assert_eq!(
        observed["execution"]["native_envelope_sha256"],
        receipt["envelope_sha256"]
    );
    assert!(receipt["receipt_sha256"].as_str().unwrap().len() == 64);
    assert_eq!(observed["execution"]["ontology_truth_mutated"], false);
    let fixture: Value = serde_json::from_str(NONZERO).unwrap();
    let subject = run.join("subject");
    fs::create_dir(&subject).unwrap();
    for (name, content) in fixture["files"].as_object().unwrap() {
        fs::write(subject.join(name), content.as_str().unwrap()).unwrap();
    }
    let manifest = subject.join("manifest.json");
    fs::write(&manifest, serde_json::to_vec(&fixture["manifest"]).unwrap()).unwrap();
    let plan_path = run.join("observation.json");
    let plan = json!({"schema":"semantica.refinery-observation-request/1","manifest_path":manifest,"scenario_ids":["s"]});
    fs::write(&plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let executed = oe_semantic::lifecycle::open(
        &root(),
        &binding_path,
        &task_path,
        &workspace,
        Some(&plan_path),
    )
    .unwrap();
    assert_eq!(executed["diagnostics"], json!([]));
    assert_eq!(executed["command_verdict"], "blocked");
    assert_eq!(executed["learning"]["verdict"], "no_delta");
    let mut task: Value = serde_json::from_str(CURRENT_TASK).unwrap();
    task["requested_actions"] = json!(["propose"]);
    fs::write(&task_path, serde_json::to_vec(&task).unwrap()).unwrap();
    let delta_path = run.join("delta.json");
    fs::write(&delta_path, serde_json::to_vec(&fixture["delta"]).unwrap()).unwrap();
    let proposed = oe_semantic::lifecycle::propose_at(
        &root(),
        &binding_path,
        &task_path,
        &workspace,
        &delta_path,
        &plan_path,
        Some("2026-08-19T00:00:00Z"),
    )
    .unwrap();
    assert_eq!(
        proposed["diagnostics"],
        json!([]),
        "Actual nonzero candidate is retained only as conditional intent"
    );
    assert_eq!(proposed["command_verdict"], "blocked");
    assert_eq!(proposed["learning"]["verdict"], "candidate");
    assert_eq!(
        proposed["learning"]["candidate"]["record_status"],
        "intent_retained"
    );
    assert_eq!(proposed["execution"]["candidate_state"], "not_registered");
    assert_eq!(proposed["regression"]["status"], "blocked");
    assert_eq!(proposed["release"]["status"], "blocked");
    assert_eq!(proposed["release"]["engineering_adoption"], "unknown");
    let registry: Value =
        serde_json::from_slice(&fs::read(workspace.join("registry.json")).unwrap()).unwrap();
    assert_eq!(registry["packages"], json!({}));
    assert_eq!(
        fs::read_dir(workspace.join("refinements")).unwrap().count(),
        0
    );
}

const CURRENT_BINDING: &str = r########"{
  "$schema": "ontology-engineering.semantic-project-binding/v1",
  "allowed_actions": [
    "doctor",
    "discover",
    "open",
    "propose",
    "commit",
    "verify",
    "promote",
    "history"
  ],
  "authority": {
    "decision": {
      "authority_id": "decision.authority",
      "scope": [
        "commit",
        "promote"
      ]
    },
    "fact": {
      "authority_id": "fact.authority",
      "scope": [
        "record_candidate"
      ]
    }
  },
  "baseline": {
    "digest": "fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947",
    "version": "0"
  },
  "binding_id": "binding-1",
  "created_at": "2026-08-19T00:00:00Z",
  "evidence": {
    "logical_root": "urn:example:evidence-root:engineering-record"
  },
  "lifecycle_actions": [
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted"
  ],
  "project": {
    "domain": "example-industry",
    "project_id": "project-1"
  },
  "promotion": {
    "requires_decision_authority": true,
    "target": "industry-registry"
  },
  "semantic_api": "semantica.ontology.refinery/v1",
  "semantic_target": {
    "kind": "workspace",
    "package_id": "industry.example",
    "workspace_id": "registry-1"
  }
}
"########;
const CURRENT_TASK: &str = r########"{
  "$schema": "ontology-engineering.semantic-task-envelope/v1",
  "actor_id": "fact.authority",
  "created_at": "2026-08-19T00:00:00Z",
  "domain": "example-industry",
  "evidence": [
    {
      "captured_at": "2026-08-19T00:00:00Z",
      "media_type": "application/json",
      "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "source_id": "engineering-record",
      "uri": "urn:example:evidence-root:engineering-record"
    }
  ],
  "intent": "Record explicitly conditional chat experience; no engineering adoption",
  "project": "project-1",
  "requested_actions": [
    "open"
  ],
  "requested_decision": "Retain candidate, verify before governed adoption",
  "required_capabilities": [
    "semantic.package.load"
  ],
  "task_id": "baseline-open",
  "task_kind": "engineering-refinement"
}
"########;
