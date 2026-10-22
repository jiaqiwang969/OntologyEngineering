//! Explicit mixed inventory for preservation, not an independent design oracle.
//! Legacy linear signatures retain their exact bytes and refusal scope.
use super::drawing_retirement;
use anyhow::{Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn linear(d: &Value) -> bool {
    d["computed_size"]["metadata"]["type_query"]["subtype_raw"] == 3
}
pub fn signature(d: &Value, unit: &str) -> Result<Value> {
    if linear(d) {
        return drawing_retirement::signature(d, unit);
    }
    let s =
        drawing_retirement::signature_for_kind(d, unit, 10, "oev2.drawing-dimension-signature/2")?;
    ensure!(
        d["computed_size"]["value"]
            .as_f64()
            .is_some_and(|x| x > 0.0),
        "mixed_inventory_diameter_value_invalid"
    );
    let entries = d["associativity_parents"]["entries"].as_array().unwrap();
    ensure!(entries.len() == 1, "mixed_inventory_single_circle_required");
    let e = &entries[0];
    let p = &e["object_1"];
    ensure!(
        p["present"] == true
            && p["metadata"]["type_query"]["type_raw"] == 70
            && p["metadata"]["type_query"]["subtype_raw"] == 3
            && p["geometry"]["called"] == true
            && e["object_2"]["present"] == false
            && e["object_2"]["tag"] == 0
            && e["view"]["present"] == true
            && e["point_type_raw"] == 0
            && e["line_type_raw"] == 0
            && e["coordinate_basis"] == "UF_DRF assoc_point absolute coordinates",
        "mixed_inventory_whole_circle_association_required"
    );
    Ok(s)
}
pub fn inventory(part: &Value) -> Result<BTreeMap<String, Value>> {
    drawing_retirement::inventory_using(part, signature)
}
