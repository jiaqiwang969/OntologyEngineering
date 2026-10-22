//! Fresh Rust execution behind the retained v1 projection and feedback contract.
//! DTO compatibility does not identify this execution as the historical engine.
use anyhow::{Context, Result, ensure};
use oe_core::{read_regular_bytes, write_new};
use oe_semantic::review::{NativeReviewExecutor, NativeReviewInput};
use serde_json::{Value, json};
use std::path::Path;

pub struct RustExecutor;
impl NativeReviewExecutor for RustExecutor {
    fn review(&self, root: &Path, input: &NativeReviewInput) -> Result<Value> {
        let binding_raw = read_regular_bytes(&input.binding, crate::MAX_BYTES)?;
        let binding = oe_core::json::strict_json(&binding_raw)?;
        let task_raw = read_regular_bytes(&input.task, crate::MAX_BYTES)?;
        let task = oe_core::json::strict_json(&task_raw)?;
        let evidence = read_regular_bytes(&input.evidence_file, crate::MAX_BYTES)?;
        ensure!(
            oe_core::sha256(&evidence) == task["evidence"][0]["sha256"],
            "review_evidence_hash_mismatch"
        );
        let lock = oe_core::json::strict_json(&read_regular_bytes(
            &root.join("runtime/semantic-bundles.json"),
            crate::MAX_BYTES,
        )?)?;
        let name = lock["bundles"]
            .as_object()
            .context("bundle_lock_invalid")?
            .iter()
            .find(|(_, v)| {
                v["package_id"] == binding["semantic_target"]["package_id"]
                    && v["package_version"] == binding["baseline"]["version"]
                    && v["package_sha256"] == binding["baseline"]["digest"]
            })
            .map(|(k, _)| k.as_str())
            .context("rust_review_exact_package_missing")?;
        let bundle = oe_semantic::load_bundle(root, name)?;
        oe_semantic::review::validate_binding(
            &binding,
            task["project"].as_str(),
            Some(&bundle.spec),
        )?;
        let focus = oxigraph::model::NamedNode::new(&input.focus_iri)?;
        let class = oxigraph::model::NamedNode::new(&input.focus_type_iri)?;
        let store = crate::graph::load(&evidence)?;
        let present =
            crate::graph::query(&store, &format!("ASK {{ {focus} ?p ?o }}"))?["value"] == true;
        let typed =
            crate::graph::query(&store, &format!("ASK {{ {focus} a {class} }}"))?["value"] == true;
        let instances = crate::graph::select(
            &store,
            &format!("SELECT DISTINCT ?s WHERE {{ ?s a {class} }}"),
        )?
        .len();
        let mut checks = crate::packages::project_checks(
            root,
            name,
            &evidence,
            std::slice::from_ref(&input.query_asset_id),
            std::slice::from_ref(&input.shape_asset_id),
        )?;
        checks.push(crate::CheckResult {
            id: "focus_binding".into(),
            execution: "completed".into(),
            conclusion: if present && typed && instances == 1 {
                "pass"
            } else {
                "unknown"
            }
            .into(),
            coverage: vec![input.focus_iri.clone(), input.focus_type_iri.clone()],
            failures: Vec::new(),
            result: json!({"focus_present":present,"type_present":typed,"instances":instances}),
        });
        let rust_binding = crate::Binding {
            project_context: None,
            rdf_focus: None,
            project_id: task["project"]
                .as_str()
                .context("review_project_missing")?
                .into(),
            object_id: input.focus_iri.clone(),
            object_revision: oe_core::sha256(&binding_raw),
            package_id: bundle.spec["package_id"]
                .as_str()
                .context("package_id_missing")?
                .into(),
            package_version: bundle.spec["package_version"]
                .as_str()
                .context("package_version_missing")?
                .into(),
            package_sha256: bundle.spec["sha256"]
                .as_str()
                .context("package_hash_missing")?
                .into(),
            source_sha256: oe_core::sha256(&evidence),
            condition_scope: input.scope_id.clone(),
            required_checks: [
                input.query_asset_id.clone(),
                input.shape_asset_id.clone(),
                "focus_binding".into(),
            ]
            .into_iter()
            .collect(),
            authority_id: task["actor_id"]
                .as_str()
                .context("review_actor_missing")?
                .into(),
        };
        let receipt = crate::evaluate(rust_binding, &evidence, checks)?;
        let destination = input.task.parent().context("review_destination_missing")?;
        write_new(
            &destination.join("native-execution.json"),
            &serde_json::to_value(&receipt)?,
        )?;
        let query = receipt
            .checks
            .iter()
            .find(|c| c.id == input.query_asset_id)
            .context("query_check_missing")?;
        let shape = receipt
            .checks
            .iter()
            .find(|c| c.id == input.shape_asset_id)
            .context("shape_check_missing")?;
        let asset_hash = |id: &str| -> Result<Value> {
            Ok(bundle.manifest["assets"]
                .as_array()
                .context("assets_missing")?
                .iter()
                .find(|a| a["asset_id"] == id)
                .context("asset_missing")?["sha256"]
                .clone())
        };
        let findings = if query.conclusion == "pass" {
            json!([])
        } else {
            query.result["rows"].clone()
        };
        let mut review = json!({"contract":"semantica.ontology.decision-review/v1","status":if receipt.conclusion=="pass"{"clear"}else{"blocked"},
            "engine_id":crate::ENGINE_ID,"engine_version":crate::ENGINE_VERSION,"dto_compatibility":"historical projection/transport contract only",
            "scope_id":input.scope_id,"focus_iri":input.focus_iri,"focus_type_iri":input.focus_type_iri,
            "evidence":{"id":input.source_id,"sha256":receipt.evidence_sha256,"uri":task["evidence"][0]["uri"],"captured_at":task["evidence"][0]["captured_at"],"format":"turtle","input_quad_count":store.len()},
            "package":{"id":bundle.spec["package_id"],"version":bundle.spec["package_version"],"registry_package_sha256":bundle.spec["package_sha256"],"origin":"promoted_registry","execution_source":"immutable verified historical package bytes","query_asset_id":input.query_asset_id,"shape_asset_id":input.shape_asset_id,"query_asset_sha256":asset_hash(&input.query_asset_id)?,"shape_asset_sha256":asset_hash(&input.shape_asset_id)?},
            "focus_present":present,"focus_type_present":typed,"focus_instance_count":instances,"shape_conforms":shape.result["conforms"],
            "findings":if findings.is_array(){findings}else{json!([])},"violations":shape.result["findings"].as_array().cloned().unwrap_or_default(),
            "execution":receipt.execution,"conclusion":receipt.conclusion,"coverage":receipt.coverage,"failures":receipt.failures,"adoption_conditions":receipt.adoption_conditions,
            "meaning":"fresh Rust scoped record consistency; engineering adoption and promotion remain separate"});
        review["report_sha256"] = oe_core::sha256(oe_core::json::canonical_json(&review)?).into();
        ensure!(
            read_regular_bytes(&input.binding, crate::MAX_BYTES)? == binding_raw
                && read_regular_bytes(&input.task, crate::MAX_BYTES)? == task_raw
                && read_regular_bytes(&input.evidence_file, crate::MAX_BYTES)? == evidence,
            "review_execution_input_changed"
        );
        Ok(
            json!({"execution":{"status":if receipt.conclusion=="pass"{"passed"}else{"blocked"},"operation":"rust_decision_review","task_sha256":oe_core::sha256(&task_raw),"review":review},"engine_id":crate::ENGINE_ID,"fact_admitted":false}),
        )
    }
}
