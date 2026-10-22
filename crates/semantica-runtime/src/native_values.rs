//! Generated transport contract over the single neutral native-value owner.
//! Limits come from the calling host; no Project, graph, registry or budget is created.
use anyhow::{Result, ensure};
use semantica_contracts::SemanticNativeValueEnvelope;
use semantica_json::native_value::{NativeLimits, NativeValue, SCHEMA};
pub fn decode(input: &SemanticNativeValueEnvelope, limits: NativeLimits) -> Result<NativeValue> {
    ensure!(input.validate().is_ok(), "native_value_contract_invalid");
    let root = usize::try_from(input.root)?;
    Ok(NativeValue::from_flat_nodes(root, &input.nodes, limits)?)
}
pub fn encode(value: &NativeValue, limits: NativeLimits) -> Result<SemanticNativeValueEnvelope> {
    let wire = value.to_envelope(limits)?;
    let envelope: SemanticNativeValueEnvelope = serde_json::from_value(wire)?;
    ensure!(
        envelope.schema == SCHEMA && envelope.validate().is_ok(),
        "native_value_contract_invalid"
    );
    Ok(envelope)
}
pub fn decode_wire(bytes: &[u8], limits: NativeLimits) -> Result<SemanticNativeValueEnvelope> {
    // Closed raw-wire shape and duplicate-key checks happen before DTO conversion.
    encode(&NativeValue::decode(bytes, limits)?, limits)
}
/// Historical import syntax is explicit and never used for contract/credential
/// JSON. Parsing retains types before the existing generated envelope is used.
/// No graph or HTTP mutation is performed here.
pub fn parse_import_json(
    bytes: &[u8],
    limits: semantica_json::original_json::ParseLimits,
) -> Result<SemanticNativeValueEnvelope> {
    let value = semantica_json::original_json::parse_original_json(bytes, limits)?;
    encode(&value, limits.native)
}
