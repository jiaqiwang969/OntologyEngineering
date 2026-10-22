//! CAD/process handoff integrity, explicit ABox and q21 authoring views. These
//! functions neither execute Semantica nor accept a manufacturing claim.
use crate::evidence::{self, common::*, schema};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
const CP: &str = "urn:ontology-engineering:cad-process:v1:";
const COLLECTIONS: &[(&str, &str)] = &[
    ("sources", "source"),
    ("features", "feature"),
    ("functions", "function"),
    ("routes", "route"),
    ("coverage_assertions", "coverage"),
    ("claims", "claim"),
    ("questions", "question"),
    ("assumptions", "assumption"),
    ("downstream_outputs", "downstream_output"),
];
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
        project(&self.packet, s(&self.audit, "packet_sha256"))
    }
    pub fn project_q21(&self, claim_id: &str) -> Result<Q21Projection> {
        q21(&self.packet, claim_id, s(&self.audit, "packet_sha256"))
    }
    pub fn requests(&self, direction: QuestionDirection) -> Value {
        let direction = match direction {
            QuestionDirection::CadToProcess => "cad_to_process",
            QuestionDirection::ProcessToCad => "process_to_cad",
        };
        json!({"record_type":"ontology-engineering.cad-process-requests/v1","handoff_id":self.packet["handoff_id"],"packet_sha256":self.audit["packet_sha256"],"direction":direction,"questions":entries(&self.packet,"questions").iter().filter(|q|s(q,"direction")==direction).collect::<Vec<_>>(),"engineering_verdict":"not_assessed"})
    }
}
#[derive(Debug, Clone, Copy)]
pub enum QuestionDirection {
    CadToProcess,
    ProcessToCad,
}
#[derive(Debug)]
pub struct Q21Projection {
    pub abox: Vec<u8>,
    pub focus: String,
    pub audit: Value,
}
pub fn verify(packet_path: &Path, project_root: &Path) -> Result<VerifiedPacket> {
    let bytes = oe_core::read_regular_bytes(packet_path, JSON_LIMIT)?;
    let packet = oe_wire::json::strict_json(&bytes)?;
    let ids = validate_packet(&packet)?;
    let source = sources(&packet, project_root)?;
    for e in entries(&packet, "features") {
        if s(e, "basis") == "native_geometry_readback" {
            let observed = pointer(source.json(s(e, "source_id"))?, s(e, "source_locator"))?
                .as_object()
                .context("native_readback_locator_not_object")?;
            if let Some(attributes) = e.get("attributes").and_then(Value::as_object) {
                for (k, v) in attributes {
                    ensure!(
                        observed.get(k).is_some_and(|actual| equivalent(actual, v)),
                        "native_readback_attribute_mismatch:{}:{k}",
                        s(e, "id")
                    );
                }
            }
        }
    }
    let mut cad_sha = None;
    if let Some(link) = packet.get("cad_evidence") {
        let bytes = source
            .json_bytes
            .get(s(link, "source_id"))
            .context("CAD_evidence_source_requires_application/json")?;
        let cad = evidence::verify_bytes(bytes, project_root)?;
        ensure!(
            cad.packet()["project"] == packet["project"],
            "CAD_evidence_project_identity/state_differs"
        );
        for l in entries(link, "links") {
            for id in strings(l, "cad_object_ids") {
                ensure!(
                    entries(cad.packet(), "objects")
                        .iter()
                        .any(|o| s(o, "id") == id),
                    "CAD_evidence_missing_object:{id}"
                );
            }
        }
        cad_sha = Some(cad.audit()["packet_sha256"].clone());
    }
    let mut audit = json!({"record_type":"ontology-engineering.cad-process-handoff-integrity/v1","handoff_id":packet["handoff_id"],"packet_sha256":oe_wire::sha256(&bytes),"schema_sha256":oe_wire::sha256(schema::HANDOFF),"adapter_sha256":adapter_hash(),"source_integrity":"verified","source_results":source.results,"entity_count":ids.len(),"semantic_execution":"not_run","engineering_verdict":"not_assessed"});
    if let Some(sha) = cad_sha {
        audit["cad_evidence_packet_sha256"] = sha;
    }
    Ok(VerifiedPacket { packet, audit })
}
fn validate_packet(packet: &Value) -> Result<Inventory<'_>> {
    schema::validate(packet, schema::HANDOFF)?;
    let ids = inventory(packet, COLLECTIONS, true)?;
    for e in entries(packet, "sources") {
        if let Some(embedded) = e.get("embedded_source_hashes").and_then(Value::as_object) {
            for id in embedded.values() {
                expect(&ids, s(e, "id"), id.as_str().unwrap(), &["source"])?;
            }
        }
    }
    for e in entries(packet, "features") {
        expect(&ids, s(e, "id"), s(e, "source_id"), &["source"])?;
        let role = s(ids[s(e, "source_id")].1, "role");
        let basis = s(e, "basis");
        ensure!(
            basis != "native_geometry_readback" || role == "native_readback",
            "claims_native_readback_from_wrong_role"
        );
        ensure!(
            basis != "drawing_review" || role == "drawing",
            "claims_drawing_review_from_wrong_role"
        );
    }
    for e in entries(packet, "functions") {
        refs(&ids, e, "source_ids", &["source"])?;
        refs(&ids, e, "feature_ids", &["feature"])?;
    }
    let mut operation_routes = BTreeMap::new();
    for route in entries(packet, "routes") {
        refs(&ids, route, "source_ids", &["source"])?;
        for e in entries(route, "operations") {
            operation_routes.insert(s(e, "id"), s(route, "id"));
            refs(&ids, e, "target_feature_ids", &["feature"])?;
            refs(&ids, e, "intended_function_ids", &["function"])?;
            for field in ["source_ids", "validation_source_ids"] {
                refs(&ids, e, field, &["source"])?;
            }
        }
    }
    for e in entries(packet, "claims") {
        refs(&ids, e, "source_ids", &["source"])?;
        refs(&ids, e, "depends_on_function_ids", &["function"])?;
        refs(&ids, e, "depends_on_route_ids", &["route"])?;
        for field in ["baseline_route_id", "target_route_id"] {
            if let Some(id) = e.get(field).and_then(Value::as_str) {
                expect(&ids, s(e, "id"), id, &["route"])?;
            }
        }
        refs(
            &ids,
            e,
            "function_list_completeness_source_ids",
            &["source"],
        )?;
        ensure!(
            entries(e, "function_list_completeness_source_ids").is_empty()
                != e.get("function_list_completeness_scope").is_some(),
            "completeness_evidence_and_scope_must_occur_together"
        );
    }
    for e in entries(packet, "coverage_assertions") {
        for (field, kind) in [
            ("claim_id", "claim"),
            ("function_id", "function"),
            ("operation_id", "operation"),
        ] {
            expect(&ids, s(e, "id"), s(e, field), &[kind])?;
        }
        for field in [
            "source_ids",
            "validation_source_ids",
            "scope_checked_source_ids",
        ] {
            refs(&ids, e, field, &["source"])?;
        }
        let claim = ids[s(e, "claim_id")].1;
        ensure!(
            strings(claim, "depends_on_function_ids").any(|id| id == s(e, "function_id"))
                && strings(claim, "depends_on_route_ids")
                    .any(|id| id == operation_routes[s(e, "operation_id")]),
            "coverage_outside_claim_function/route_scope"
        );
        if s(e, "status") == "verified" {
            ensure!(
                !entries(e, "scope_checked_source_ids").is_empty()
                    && strings(e, "validation_source_ids")
                        .any(|id| s(ids[id].1, "role") == "test_result"),
                "verified_coverage_requires_scoped_physical_test_evidence"
            );
        }
    }
    for e in entries(packet, "questions") {
        refs(&ids, e, "source_ids", &["source"])?;
        refs(
            &ids,
            e,
            "target_ids",
            &[
                "feature",
                "function",
                "route",
                "claim",
                "operation",
                "coverage",
            ],
        )?;
    }
    for e in entries(packet, "assumptions") {
        refs(&ids, e, "source_ids", &["source"])?;
        refs(&ids, e, "applies_to_claim_ids", &["claim"])?;
    }
    for e in entries(packet, "downstream_outputs") {
        refs(&ids, e, "source_ids", &["source"])?;
        refs(&ids, e, "depends_on_claim_ids", &["claim"])?;
    }
    if let Some(link) = packet.get("cad_evidence") {
        expect(&ids, "cad_evidence", s(link, "source_id"), &["source"])?;
        let mut seen = BTreeSet::new();
        for e in entries(link, "links") {
            let id = s(e, "handoff_entity_id");
            expect(
                &ids,
                "cad_evidence",
                id,
                &[
                    "feature",
                    "function",
                    "route",
                    "coverage",
                    "claim",
                    "question",
                    "assumption",
                    "downstream_output",
                    "operation",
                ],
            )?;
            ensure!(seen.insert(id), "duplicate_CAD_evidence_handoff_link");
        }
    }
    Ok(ids)
}
fn project(packet: &Value, digest: &str) -> Vec<u8> {
    let base = base(packet);
    let snapshot = uri(&base, "snapshot", s(packet, "handoff_id"));
    let mut g = Graph::default();
    g.iri(&snapshot, RDF_TYPE, &format!("{CP}ProjectSnapshot"));
    g.text(&snapshot, &format!("{CP}packet_sha256"), digest);
    g.fields(
        &snapshot,
        CP,
        &packet["project"],
        &[
            "project_id",
            "configuration_id",
            "model_revision",
            "product_state",
        ],
    );
    let inventory = inventory(packet, COLLECTIONS, true).expect("verified references");
    for (key, kind, class, predicate, fields) in [
        (
            "sources",
            "source",
            "EvidenceSource",
            "hasSource",
            &["id", "path", "sha256", "media_type", "role"][..],
        ),
        (
            "features",
            "feature",
            "CadFeature",
            "hasFeature",
            &[
                "id",
                "part_id",
                "kind",
                "statement",
                "geometry_state",
                "basis",
                "source_locator",
            ][..],
        ),
        (
            "functions",
            "function",
            "FunctionRequirement",
            "hasFunction",
            &["id", "name", "status", "basis"][..],
        ),
        (
            "routes",
            "route",
            "ProcessRoute",
            "hasRoute",
            &["id", "name", "status"][..],
        ),
        (
            "coverage_assertions",
            "coverage",
            "FunctionCoverageAssertion",
            "hasCoverageAssertion",
            &["id", "status", "mechanism_statement"][..],
        ),
        (
            "claims",
            "claim",
            "DecisionClaim",
            "hasClaim",
            &["id", "statement", "maturity"][..],
        ),
        (
            "questions",
            "question",
            "EngineeringQuestion",
            "hasQuestion",
            &["id", "direction", "text", "status"][..],
        ),
        (
            "assumptions",
            "assumption",
            "ModelAssumption",
            "hasAssumption",
            &["id", "statement", "scope"][..],
        ),
        (
            "downstream_outputs",
            "downstream_output",
            "DownstreamOutput",
            "hasDownstreamOutput",
            &["id", "kind", "statement", "state"][..],
        ),
    ] {
        for e in entries(packet, key) {
            let subject = uri(&base, kind, s(e, "id"));
            g.iri(&snapshot, &format!("{CP}{predicate}"), &subject);
            g.iri(&subject, RDF_TYPE, &format!("{CP}{class}"));
            g.fields(&subject, CP, e, fields);
            g.links(
                &subject,
                (CP, &base),
                "wasDerivedFrom",
                "source",
                e,
                "source_ids",
            );
            match kind {
                "feature" => {
                    g.iri(
                        &subject,
                        &format!("{CP}wasDerivedFrom"),
                        &uri(&base, "source", s(e, "source_id")),
                    );
                    if let Some(a) = e
                        .get("attributes")
                        .filter(|a| a.as_object().is_some_and(|o| !o.is_empty()))
                    {
                        g.text(&subject, &format!("{CP}attributesJson"), &spaced_json(a));
                    }
                }
                "function" => g.links(
                    &subject,
                    (CP, &base),
                    "appliesToFeature",
                    "feature",
                    e,
                    "feature_ids",
                ),
                "route" => {
                    for op in entries(e, "operations") {
                        let u = uri(&base, "operation", s(op, "id"));
                        g.iri(&subject, &format!("{CP}hasOperation"), &u);
                        g.iri(&u, RDF_TYPE, &format!("{CP}ProcessOperation"));
                        g.fields(&u, CP, op, &["id", "process_type", "mechanism_statement"]);
                        for (field, pred, kind) in [
                            ("target_feature_ids", "targetsFeature", "feature"),
                            ("intended_function_ids", "intendsFunction", "function"),
                            ("source_ids", "wasDerivedFrom", "source"),
                            ("validation_source_ids", "hasValidationSource", "source"),
                        ] {
                            g.links(&u, (CP, &base), pred, kind, op, field);
                        }
                    }
                }
                "coverage" => {
                    for (field, pred, kind) in [
                        ("claim_id", "forClaim", "claim"),
                        ("function_id", "forFunction", "function"),
                        ("operation_id", "formedByOperation", "operation"),
                    ] {
                        g.iri(
                            &subject,
                            &format!("{CP}{pred}"),
                            &uri(&base, kind, s(e, field)),
                        );
                    }
                    for (field, pred) in [
                        ("validation_source_ids", "hasValidationSource"),
                        ("scope_checked_source_ids", "hasScopeCheckSource"),
                    ] {
                        g.links(&subject, (CP, &base), pred, "source", e, field);
                    }
                }
                "claim" => {
                    for (field, pred, kind) in [
                        ("depends_on_function_ids", "dependsOnFunction", "function"),
                        ("depends_on_route_ids", "dependsOnRoute", "route"),
                        (
                            "function_list_completeness_source_ids",
                            "functionListCompletenessEvidence",
                            "source",
                        ),
                    ] {
                        g.links(&subject, (CP, &base), pred, kind, e, field);
                    }
                    for (field, pred) in [
                        ("baseline_route_id", "baselineRoute"),
                        ("target_route_id", "targetRoute"),
                    ] {
                        if let Some(id) = e.get(field).and_then(Value::as_str) {
                            g.iri(&subject, &format!("{CP}{pred}"), &uri(&base, "route", id));
                        }
                    }
                    if let Some(scope) = e
                        .get("function_list_completeness_scope")
                        .and_then(Value::as_str)
                    {
                        g.text(
                            &subject,
                            &format!("{CP}functionListCompletenessScope"),
                            scope,
                        );
                    }
                }
                "question" => {
                    for id in strings(e, "target_ids") {
                        g.iri(
                            &subject,
                            &format!("{CP}targets"),
                            &uri(&base, inventory[id].0, id),
                        );
                    }
                    for text in strings(e, "requested_evidence") {
                        g.text(&subject, &format!("{CP}requestsEvidence"), text);
                    }
                }
                "assumption" => g.links(
                    &subject,
                    (CP, &base),
                    "limitsClaimEvidence",
                    "claim",
                    e,
                    "applies_to_claim_ids",
                ),
                "downstream_output" => g.links(
                    &subject,
                    (CP, &base),
                    "dependsOnClaim",
                    "claim",
                    e,
                    "depends_on_claim_ids",
                ),
                _ => (),
            }
        }
    }
    if let Some(link) = packet.get("cad_evidence") {
        g.iri(
            &snapshot,
            &format!("{CP}hasCadEvidenceSource"),
            &uri(&base, "source", s(link, "source_id")),
        );
        for e in entries(link, "links") {
            let id = s(e, "handoff_entity_id");
            for object in strings(e, "cad_object_ids") {
                g.iri(
                    &uri(&base, inventory[id].0, id),
                    &format!("{CP}referencesCadObject"),
                    &uri(&base, "cad_object", object),
                );
            }
        }
    }
    g.bytes()
}
fn q21(packet: &Value, claim_id: &str, digest: &str) -> Result<Q21Projection> {
    const P: &str = "urn:mfg:loop:";
    let claim = entries(packet, "claims")
        .iter()
        .find(|c| s(c, "id") == claim_id)
        .context("q21_requires_batch_delivery_claim")?;
    ensure!(
        s(claim, "maturity") == "batch_delivery",
        "q21_requires_batch_delivery_claim"
    );
    let baseline = claim
        .get("baseline_route_id")
        .and_then(Value::as_str)
        .context("q21_requires_explicit_baseline_and_target_routes")?;
    let target = claim
        .get("target_route_id")
        .and_then(Value::as_str)
        .context("q21_requires_explicit_baseline_and_target_routes")?;
    ensure!(
        strings(claim, "depends_on_route_ids").any(|id| id == target),
        "target_route_not_claim_dependency"
    );
    // The separator must never survive inside a component. Bind the same
    // four-part identity as the CAD/process ABox so states cannot share claims.
    let component = |value: &str| percent(value).replace('.', "%2E");
    let prefix = [
        "project_id",
        "configuration_id",
        "model_revision",
        "product_state",
    ]
    .iter()
    .map(|k| component(s(&packet["project"], k)))
    .collect::<Vec<_>>()
    .join(".");
    let u = |kind: &str, id: &str| format!("{P}{prefix}.{kind}.{}", component(id));
    let mut g = Graph::default();
    let focus = u("claim", claim_id);
    let snapshot = u("snapshot", s(packet, "handoff_id"));
    g.iri(&focus, RDF_TYPE, &format!("{P}ProcessReplacementClaim"));
    g.iri(&focus, &format!("{P}baselineRoute"), &u("route", baseline));
    g.iri(&focus, &format!("{P}targetRoute"), &u("route", target));
    g.text(&focus, &format!("{P}decisionLevel"), "production");
    g.iri(&focus, &format!("{P}sourceSnapshot"), &snapshot);
    g.text(
        &focus,
        &format!("{P}recordKind"),
        "project_input_projection_not_release",
    );
    for (pred, value) in [
        ("digest", digest),
        ("productVersion", s(&packet["project"], "model_revision")),
        ("testedState", s(&packet["project"], "product_state")),
    ] {
        g.text(&snapshot, &format!("{P}{pred}"), value);
    }
    for id in strings(claim, "function_list_completeness_source_ids") {
        g.iri(
            &focus,
            &format!("{P}completenessEvidence"),
            &u("source", id),
        );
    }
    if let Some(scope) = claim
        .get("function_list_completeness_scope")
        .and_then(Value::as_str)
    {
        g.text(&focus, &format!("{P}scope"), scope);
    }
    for id in strings(claim, "depends_on_function_ids") {
        let f = entries(packet, "functions")
            .iter()
            .find(|f| s(f, "id") == id)
            .context("function_missing")?;
        ensure!(
            s(f, "status") == "required",
            "q21_required_function_is_only_candidate"
        );
        let subject = u("function", id);
        g.iri(&focus, &format!("{P}requiredFunction"), &subject);
        g.iri(&subject, RDF_TYPE, &format!("{P}RequiredFunction"));
        g.text(&subject, &format!("{P}scope"), s(f, "basis"));
        for id in strings(f, "source_ids") {
            g.iri(&subject, &format!("{P}source"), &u("source", id));
        }
    }
    for c in entries(packet, "coverage_assertions")
        .iter()
        .filter(|c| s(c, "claim_id") == claim_id)
    {
        let subject = u("coverage", s(c, "id"));
        g.iri(&subject, RDF_TYPE, &format!("{P}FunctionCoverage"));
        g.iri(&subject, &format!("{P}forClaim"), &focus);
        g.iri(
            &subject,
            &format!("{P}forFunction"),
            &u("function", s(c, "function_id")),
        );
        g.iri(
            &subject,
            &format!("{P}mechanism"),
            &u("operation", s(c, "operation_id")),
        );
        g.text(&subject, &format!("{P}status"), s(c, "status"));
        for (field, pred) in [
            ("validation_source_ids", "validatedBy"),
            ("scope_checked_source_ids", "scopeChecked"),
        ] {
            for id in strings(c, field) {
                g.iri(&subject, &format!("{P}{pred}"), &u("source", id));
            }
        }
    }
    for source in entries(packet, "sources") {
        let subject = u("source", s(source, "id"));
        g.text(&subject, &format!("{P}digest"), s(source, "sha256"));
        g.text(&subject, &format!("{P}recordKind"), s(source, "role"));
    }
    let abox = g.bytes();
    let audit = json!({"record_type":"ontology-engineering.mfg-q21-authoring-projection/v1","identity_scheme":"project-configuration-revision-state/dot-escaped-v2","packet_sha256":digest,"source_integrity":"verified","project_abox_focus":focus,"focus_type":format!("{P}ProcessReplacementClaim"),"package_id":"semantica.manufacturing.process-cost-loop","proposed_version":"0.1.3","proposed_delta_sha256":"73bdf2e23f73d64b98aa5fc0223ee1d842fccfdbf0d2ae8ccc2c031e6fbd4dd8","query_asset_id":"q21","shape_asset_id":"causal-decision-shape","view_sha256":oe_wire::sha256(&abox),"package_state":"proposed_not_promoted","semantic_execution":"not_run","engineering_verdict":"not_assessed"});
    Ok(Q21Projection { abox, focus, audit })
}
/// Compare declared dependencies only; source verification is a separate step.
pub fn compare_packets(previous: &Value, current: &Value) -> Result<Value> {
    validate_packet(previous)?;
    validate_packet(current)?;
    let entities = |p: &Value| -> Dependencies {
        let mut result = Dependencies::new();
        for (key, kind) in COLLECTIONS {
            for e in entries(p, key) {
                let mut deps = dep_fields(e, &["source_ids"]);
                match *kind {
                    "source" => {
                        if let Some(o) = e.get("embedded_source_hashes").and_then(Value::as_object)
                        {
                            deps.extend(o.values().map(|v| v.as_str().unwrap().to_owned()));
                        }
                    }
                    "feature" => {
                        deps.insert(s(e, "source_id").into());
                    }
                    "function" => deps.extend(dep_fields(e, &["feature_ids"])),
                    "route" => {
                        for op in entries(e, "operations") {
                            deps.insert(s(op, "id").into());
                            result.insert(
                                s(op, "id").into(),
                                (
                                    "operation".into(),
                                    op.clone(),
                                    dep_fields(
                                        op,
                                        &[
                                            "source_ids",
                                            "validation_source_ids",
                                            "target_feature_ids",
                                            "intended_function_ids",
                                        ],
                                    ),
                                ),
                            );
                        }
                    }
                    "coverage" => {
                        deps.extend([
                            s(e, "claim_id").into(),
                            s(e, "function_id").into(),
                            s(e, "operation_id").into(),
                        ]);
                        deps.extend(dep_fields(
                            e,
                            &["validation_source_ids", "scope_checked_source_ids"],
                        ));
                    }
                    "claim" => {
                        deps.extend(dep_fields(
                            e,
                            &[
                                "depends_on_function_ids",
                                "depends_on_route_ids",
                                "function_list_completeness_source_ids",
                            ],
                        ));
                        for c in entries(p, "coverage_assertions")
                            .iter()
                            .filter(|c| s(c, "claim_id") == s(e, "id"))
                        {
                            deps.insert(s(c, "id").into());
                        }
                        for key in ["baseline_route_id", "target_route_id"] {
                            if let Some(id) = e.get(key).and_then(Value::as_str) {
                                deps.insert(id.into());
                            }
                        }
                    }
                    "question" => deps.extend(dep_fields(e, &["target_ids"])),
                    "downstream_output" => deps.extend(dep_fields(e, &["depends_on_claim_ids"])),
                    _ => (),
                }
                result.insert(s(e, "id").into(), ((*kind).into(), e.clone(), deps));
            }
        }
        for e in entries(p, "assumptions") {
            for claim in strings(e, "applies_to_claim_ids") {
                result.get_mut(claim).unwrap().2.insert(s(e, "id").into());
            }
        }
        if let Some(link) = p.get("cad_evidence") {
            for e in entries(link, "links") {
                result
                    .get_mut(s(e, "handoff_entity_id"))
                    .unwrap()
                    .2
                    .insert(s(link, "source_id").into());
            }
        }
        result
    };
    Ok(impact(
        previous,
        current,
        entities(previous),
        entities(current),
        &["feature", "function", "route", "claim"],
        "ontology-engineering.cad-process-dependency-impact/v1",
        &[
            ("affected_claim_ids", "claim"),
            ("affected_question_ids", "question"),
            ("affected_output_ids", "downstream_output"),
        ],
    ))
}
