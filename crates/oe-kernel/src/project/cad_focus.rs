//! Source identity for explicit CAD association, not native or engineering acceptance.
use super::{Project, check_current_context};
use anyhow::{Result, ensure};
use oe_contracts::{CadEngineeringFocusContext2, ObjectIdentity, ProjectReviewContext};
use oxigraph::model::{Literal, NamedNode};

const PREFIX: &str = "urn:oev2:cad-engineering-focus:2:";
const LIMIT: usize = 8 * 1024;

/// Only the kernel constructor can create this capability. A serialized focus
/// context remains a declaration and cannot deserialize into a verified value.
#[derive(Debug, Clone)]
pub struct ValidatedCadFocus {
    context: CadEngineeringFocusContext2,
    iri: String,
}
impl ValidatedCadFocus {
    pub fn context(&self) -> &CadEngineeringFocusContext2 {
        &self.context
    }
    pub fn iri(&self) -> &str {
        &self.iri
    }
    pub fn ensure_current(&self, project_bytes: &[u8]) -> Result<()> {
        let actual = bind_cad_focus(
            project_bytes,
            &self.context.object,
            &self.context.project_context,
        )?;
        ensure!(actual.iri == self.iri, "cad_focus_project_bytes_changed");
        Ok(())
    }
    /// Explicit association declarations are separate from the original CAD
    /// ABox. These statements do not claim to be native observations.
    pub fn declaration_graph(&self) -> Result<Vec<u8>> {
        let focus = NamedNode::new(&self.iri)?;
        let values = [
            ("rawObjectId", self.context.object.object_id.as_str()),
            ("objectRevision", self.context.object.revision.as_str()),
            (
                "conditionScope",
                self.context.object.condition_scope.as_str(),
            ),
            (
                "conditionSha256",
                self.context.project_context.condition_sha256.as_str(),
            ),
            (
                "projectBytesSha256",
                self.context.project_bytes_sha256.as_str(),
            ),
            ("sourceSha256", self.context.object.source_sha256.as_str()),
        ];
        let mut output = String::new();
        for (predicate, value) in values {
            output.push_str(&format!(
                "{focus} <urn:oev2:project-context:{predicate}> {} .\n",
                Literal::new_simple_literal(value)
            ));
        }
        Ok(output.into_bytes())
    }
}

pub fn bind_cad_focus(
    project_bytes: &[u8],
    object: &ObjectIdentity,
    context: &ProjectReviewContext,
) -> Result<ValidatedCadFocus> {
    ensure!(
        project_bytes.len() <= 16 * 1024 * 1024,
        "cad_focus_project_budget"
    );
    let project: Project = serde_json::from_value(oe_core::json::strict_json(project_bytes)?)?;
    ensure!(
        check_current_context(&project, object, Some(context)).conclusion == "pass",
        "cad_focus_current_context_mismatch"
    );
    let context = CadEngineeringFocusContext2 {
        schema: "oev2.cad-engineering-focus-context/2".into(),
        project_bytes_sha256: oe_core::sha256(project_bytes),
        object: object.clone(),
        project_context: context.clone(),
    };
    context
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_focus_contract_invalid"))?;
    let canonical = oe_wire::json::canonical_json(&serde_json::to_value(&context)?)?;
    ensure!(canonical.len() <= LIMIT, "cad_focus_payload_budget");
    let mut iri = String::with_capacity(PREFIX.len() + canonical.len() * 2);
    iri.push_str(PREFIX);
    for byte in &canonical {
        use std::fmt::Write;
        write!(&mut iri, "{byte:02x}")?;
    }
    NamedNode::new(&iri)?;
    ensure!(
        decode_focus(&iri)? == context,
        "cad_focus_roundtrip_mismatch"
    );
    Ok(ValidatedCadFocus { context, iri })
}

/// Decode an untrusted declaration. This never constructs ValidatedCadFocus.
pub fn decode_focus(iri: &str) -> Result<CadEngineeringFocusContext2> {
    let encoded = iri
        .strip_prefix(PREFIX)
        .ok_or_else(|| anyhow::anyhow!("cad_focus_prefix_invalid"))?;
    ensure!(
        !encoded.is_empty() && encoded.len() <= LIMIT * 2 && encoded.len() % 2 == 0,
        "cad_focus_encoded_budget_or_length"
    );
    ensure!(
        encoded
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
        "cad_focus_hex_noncanonical"
    );
    let bytes: Vec<u8> = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |v: u8| {
                if v.is_ascii_digit() {
                    v - b'0'
                } else {
                    v - b'a' + 10
                }
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect();
    let value = oe_core::json::strict_json(&bytes)?;
    let context: CadEngineeringFocusContext2 = serde_json::from_value(value.clone())?;
    context
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_focus_contract_invalid"))?;
    ensure!(
        context.schema == "oev2.cad-engineering-focus-context/2"
            && oe_wire::json::canonical_json(&value)? == bytes,
        "cad_focus_payload_noncanonical"
    );
    Ok(context)
}

/// Verify the source-bound declaration without invoking formal review or CAD.
pub fn check_declaration(focus: &ValidatedCadFocus, evidence: &[u8]) -> crate::CheckResult {
    let checked = (|| -> Result<bool> {
        let data = crate::graph::load(evidence)?;
        let subject = NamedNode::new(focus.iri())?;
        let expected = [
            ("rawObjectId", focus.context.object.object_id.as_str()),
            ("objectRevision", focus.context.object.revision.as_str()),
            (
                "conditionScope",
                focus.context.object.condition_scope.as_str(),
            ),
            (
                "conditionSha256",
                focus.context.project_context.condition_sha256.as_str(),
            ),
            (
                "projectBytesSha256",
                focus.context.project_bytes_sha256.as_str(),
            ),
            ("sourceSha256", focus.context.object.source_sha256.as_str()),
        ];
        for (predicate, value) in expected {
            let predicate = format!("urn:oev2:project-context:{predicate}");
            let found: Vec<_> = data
                .iter()
                .filter(|q| {
                    q.subject == oxigraph::model::NamedOrBlankNodeRef::NamedNode(subject.as_ref())
                        && q.predicate.as_str() == predicate
                })
                .map(|q| q.object.into_owned())
                .collect();
            if found
                != vec![oxigraph::model::Term::Literal(Literal::new_simple_literal(
                    value,
                ))]
            {
                return Ok(false);
            }
        }
        Ok(true)
    })();
    let (execution, conclusion, failures) = match checked {
        Ok(true) => ("completed", "pass", vec![]),
        Ok(false) => (
            "completed",
            "violated",
            vec!["cad_focus_declaration_mismatch".into()],
        ),
        Err(error) => ("failed", "unknown", vec![error.to_string()]),
    };
    crate::CheckResult {
        id: "cad_focus_source_declaration".into(),
        execution: execution.into(),
        conclusion: conclusion.into(),
        coverage: vec![focus.iri.clone()],
        failures,
        result: serde_json::json!({"authority":"source_association_declaration_only","native_adapter":"not_run","formal_semantic_review":"not_run","physical_acceptance":"not_established"}),
    }
}
