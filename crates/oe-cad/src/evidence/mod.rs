//! Source-bound CAD evidence. Verification proves packet/source consistency;
//! projections serialize supplied claims only. Semantica execution is not run.
pub(crate) mod common;
pub(crate) mod schema;
pub mod correspondence;
use anyhow::{Context, Result, ensure};
use common::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

const CE: &str = "urn:ontology-engineering:cad-evidence:v1:";
const COLLECTIONS: &[(&str, &str)] = &[
    ("sources", "source"),
    ("objects", "object"),
    ("relations", "relation"),
    ("requirements", "requirement"),
    ("assertions", "assertion"),
    ("questions", "question"),
];

/// Immutable, source-verified snapshot. No Deserialize implementation or public
/// fields can turn unverified caller data into a verified packet.
#[derive(Debug)]
pub struct VerifiedPacket {
    packet: Value,
    audit: Value,
}
impl VerifiedPacket {
    pub fn packet(&self) -> &Value {
        &self.packet
    }
    pub fn audit(&self) -> &Value {
        &self.audit
    }
    pub fn project_abox(&self) -> Vec<u8> {
        project(&self.packet, Some(s(&self.audit, "packet_sha256")))
    }
}
pub fn verify(packet_path: &Path, project_root: &Path) -> Result<VerifiedPacket> {
    let bytes = oe_core::read_regular_bytes(packet_path, JSON_LIMIT)?;
    verify_bytes(&bytes, project_root)
}
pub(crate) fn verify_bytes(bytes: &[u8], root: &Path) -> Result<VerifiedPacket> {
    ensure!(bytes.len() <= JSON_LIMIT, "packet_size_limit");
    let packet = oe_wire::json::strict_json(bytes)?;
    let ids = validate_packet(&packet)?;
    let source = sources(&packet, root)?;
    for entry in entries(&packet, "sources") {
        if s(entry, "role") != "analysis_record" {
            continue;
        }
        let record = source.json(s(entry, "id"))?;
        schema::validate(record, schema::ANALYSIS).context("analysis_record_schema")?;
        let bound = record["source_hashes"]
            .as_object()
            .context("source_hashes")?;
        for (id, digest) in bound {
            expect(&ids, s(entry, "id"), id, &["source"])?;
            ensure!(
                id != s(entry, "id") && ids[id.as_str()].1["sha256"] == *digest,
                "analysis_source_hash_mismatch"
            );
        }
        for input in entries(record, "inputs") {
            let id = s(input, "source_id");
            ensure!(bound.contains_key(id), "analysis_input_source_not_bound");
            if s(input, "provenance") == "unknown" {
                ensure!(
                    input["value"].is_null(),
                    "unknown_analysis_input_has_a_value"
                );
            }
            if s(input, "provenance") == "measured" {
                ensure!(
                    s(ids[id].1, "role") == "measurement",
                    "measured_analysis_input_requires_measurement_source"
                );
            }
        }
        let calc = &record["calculation"];
        let implementation = calc.get("implementation_source_id").and_then(Value::as_str);
        ensure!(
            s(calc, "method") != "numerical" || implementation.is_some(),
            "numerical_analysis_implementation_not_bound"
        );
        if let Some(id) = implementation {
            ensure!(bound.contains_key(id), "analysis_implementation_not_bound");
        }
    }
    for key in ["objects", "relations"] {
        for e in entries(&packet, key) {
            let basis = s(e, "basis");
            let src = ids[s(e, "source_id")].1;
            let expected_role = match basis {
                "native_readback" => Some("native_readback"),
                "drawing_review" => Some("drawing"),
                "bom_review" => Some("bom"),
                "simulation_result" => Some("simulation_result"),
                "measurement" => Some("measurement"),
                "visual_review" => Some("reference_media"),
                "patent_review" => Some("patent_document"),
                "analytic_derivation" => Some("analysis_record"),
                _ => None,
            };
            if let Some(role) = expected_role {
                ensure!(
                    s(src, "role") == role,
                    "claims_{basis}_from_wrong_source_role"
                );
            }
            if basis == "visual_review" {
                ensure!(
                    s(src, "media_type").starts_with("image/")
                        || s(src, "media_type").starts_with("video/"),
                    "visual_review_requires_image_or_video"
                );
            }
            if ["native_readback", "analytic_derivation"].contains(&basis) {
                let observed = pointer(source.json(s(e, "source_id"))?, s(e, "source_locator"))?;
                if basis == "analytic_derivation" {
                    ensure!(
                        e.get("expected").is_some(),
                        "analytic_derivation_expected_required"
                    );
                }
                if let Some(expected) = e.get("expected") {
                    let object = observed
                        .as_object()
                        .context("readback_locator_not_object")?;
                    for (k, v) in expected.as_object().context("expected_object")? {
                        ensure!(
                            object.get(k).is_some_and(|actual| equivalent(actual, v)),
                            "{basis}_value_mismatch:{}:{k}",
                            s(e, "id")
                        );
                    }
                }
            }
        }
    }
    let audit = json!({"record_type":"ontology-engineering.cad-evidence-integrity/v1","packet_id":packet["packet_id"],"packet_sha256":oe_wire::sha256(bytes),"schema_sha256":oe_wire::sha256(schema::CAD),"analysis_schema_sha256":oe_wire::sha256(schema::ANALYSIS),"adapter_sha256":adapter_hash(),"source_integrity":"verified","source_results":source.results,"entity_count":ids.len(),"semantic_execution":"not_run","engineering_verdict":"not_assessed"});
    Ok(VerifiedPacket { packet, audit })
}
pub(crate) fn validate_packet(packet: &Value) -> Result<Inventory<'_>> {
    schema::validate(packet, schema::CAD)?;
    let ids = inventory(packet, COLLECTIONS, false)?;
    for source in entries(packet, "sources") {
        if let Some(embedded) = source
            .get("embedded_source_hashes")
            .and_then(Value::as_object)
        {
            for target in embedded.values() {
                expect(&ids, s(source, "id"), target.as_str().unwrap(), &["source"])?;
            }
        }
    }
    for key in ["objects", "relations"] {
        for e in entries(packet, key) {
            expect(&ids, s(e, "id"), s(e, "source_id"), &["source"])?;
            let basis = s(e, "basis");
            ensure!(
                !["visual_review", "patent_review", "analytic_derivation"].contains(&basis)
                    || e.get("status").is_some(),
                "reconstructed_fact_requires_explicit_status"
            );
            ensure!(
                !["engineering_inference", "analytic_derivation"].contains(&basis)
                    || e.get("status").and_then(Value::as_str) != Some("observed"),
                "inferred_fact_cannot_be_observed"
            );
            if key == "relations" {
                for field in ["subject_id", "object_id"] {
                    expect(&ids, s(e, "id"), s(e, field), &["object"])?;
                }
            }
        }
    }
    for e in entries(packet, "requirements") {
        refs(&ids, e, "source_ids", &["source"])?;
        refs(&ids, e, "target_ids", &["object"])?;
    }
    for e in entries(packet, "assertions") {
        for f in ["source_ids", "challenge_source_ids"] {
            refs(&ids, e, f, &["source"])?;
        }
        refs(&ids, e, "target_ids", &["object"])?;
        refs(
            &ids,
            e,
            "depends_on_ids",
            &["object", "relation", "requirement", "assertion"],
        )?;
    }
    for e in entries(packet, "questions") {
        refs(&ids, e, "source_ids", &["source"])?;
        refs(
            &ids,
            e,
            "target_ids",
            &["object", "relation", "requirement", "assertion"],
        )?;
    }
    // Iterative DFS, bounded by packet size rather than process stack depth.
    let mut done = BTreeSet::new();
    for assertion in entries(packet, "assertions") {
        let mut active = BTreeSet::new();
        let mut stack = vec![(s(assertion, "id"), false)];
        while let Some((id, leaving)) = stack.pop() {
            if leaving {
                active.remove(id);
                done.insert(id);
                continue;
            }
            if done.contains(id) {
                continue;
            }
            ensure!(active.insert(id), "circular_assertion_dependency:{id}");
            stack.push((id, true));
            for dep in strings(ids[id].1, "depends_on_ids") {
                if ids[dep].0 == "assertion" {
                    stack.push((dep, false));
                }
            }
        }
    }
    Ok(ids)
}
fn project(packet: &Value, digest: Option<&str>) -> Vec<u8> {
    let base = base(packet);
    let mut g = Graph::default();
    let snapshot = uri(&base, "cad_snapshot", s(packet, "packet_id"));
    g.iri(&snapshot, RDF_TYPE, &format!("{CE}CadEvidenceSnapshot"));
    if let Some(d) = digest {
        g.text(&snapshot, &format!("{CE}packet_sha256"), d);
    }
    let kind_for = |id: &str| {
        COLLECTIONS
            .iter()
            .find_map(|(key, kind)| {
                entries(packet, key)
                    .iter()
                    .any(|e| s(e, "id") == id)
                    .then_some(*kind)
            })
            .map(|kind| format!("cad_{kind}"))
            .expect("validated target")
    };
    for (key, class, kind, pred) in [
        ("sources", "EvidenceSource", "source", "hasSource"),
        ("objects", "CadObject", "cad_object", "hasObject"),
        ("relations", "CadRelation", "cad_relation", "hasRelation"),
        (
            "requirements",
            "DesignRequirement",
            "cad_requirement",
            "hasRequirement",
        ),
        (
            "assertions",
            "CadAssertion",
            "cad_assertion",
            "hasAssertion",
        ),
        (
            "questions",
            "EngineeringQuestion",
            "cad_question",
            "hasQuestion",
        ),
    ] {
        for e in entries(packet, key) {
            let subject = uri(&base, kind, s(e, "id"));
            g.iri(&snapshot, &format!("{CE}{pred}"), &subject);
            g.iri(&subject, RDF_TYPE, &format!("{CE}{class}"));
            for (field, value) in e.as_object().unwrap() {
                if ["expected", "embedded_source_hashes"].contains(&field.as_str()) {
                    g.text(&subject, &format!("{CE}{field}Json"), &spaced_json(value));
                } else if let Some(text) = value.as_str() {
                    g.text(&subject, &format!("{CE}{field}"), text);
                }
            }
            if ["cad_object", "cad_relation"].contains(&kind) {
                g.iri(
                    &subject,
                    &format!("{CE}wasDerivedFrom"),
                    &uri(&base, "source", s(e, "source_id")),
                );
            }
            if kind == "cad_relation" {
                for (f, p) in [("subject_id", "subject"), ("object_id", "object")] {
                    g.iri(
                        &subject,
                        &format!("{CE}{p}"),
                        &uri(&base, "cad_object", s(e, f)),
                    );
                }
            }
            for field in [
                "target_ids",
                "source_ids",
                "challenge_source_ids",
                "depends_on_ids",
            ] {
                for id in strings(e, field) {
                    let target = if ["source_ids", "challenge_source_ids"].contains(&field) {
                        "source".into()
                    } else {
                        kind_for(id)
                    };
                    g.iri(
                        &subject,
                        &format!(
                            "{CE}{}",
                            if field == "depends_on_ids" {
                                "dependsOn"
                            } else {
                                field
                            }
                        ),
                        &uri(&base, &target, id),
                    );
                }
            }
            for text in strings(e, "requested_evidence") {
                g.text(&subject, &format!("{CE}requestsEvidence"), text);
            }
        }
    }
    g.bytes()
}
/// Structural comparison, not a statement that either version's source bytes
/// were verified. Call verify separately before publishing project evidence.
pub fn compare_packets(previous: &Value, current: &Value) -> Result<Value> {
    validate_packet(previous)?;
    validate_packet(current)?;
    let entities = |p: &Value| -> Dependencies {
        let mut result = Dependencies::new();
        for (key, kind) in COLLECTIONS {
            for e in entries(p, key) {
                let mut deps = dep_fields(
                    e,
                    &[
                        "source_ids",
                        "challenge_source_ids",
                        "target_ids",
                        "depends_on_ids",
                    ],
                );
                if let Some(id) = e.get("source_id").and_then(Value::as_str) {
                    deps.insert(id.into());
                }
                if *kind == "source"
                    && let Some(o) = e.get("embedded_source_hashes").and_then(Value::as_object)
                {
                    deps.extend(o.values().map(|v| v.as_str().unwrap().to_owned()));
                }
                if *kind == "relation" {
                    deps.extend([s(e, "subject_id").into(), s(e, "object_id").into()]);
                }
                result.insert(s(e, "id").into(), ((*kind).into(), e.clone(), deps));
            }
        }
        result
    };
    Ok(impact(
        previous,
        current,
        entities(previous),
        entities(current),
        &[],
        "ontology-engineering.cad-evidence-impact/v1",
        &[
            ("affected_assertion_ids", "assertion"),
            ("affected_question_ids", "question"),
        ],
    ))
}
