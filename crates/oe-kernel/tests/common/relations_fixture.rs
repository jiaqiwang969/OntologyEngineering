//! Synthetic positioning checks. These numbers are test inputs, not measurements.
use oe_contracts::ObjectIdentity;
use serde_json::{Value, json};

pub fn basis(
    o: &ObjectIdentity,
    source: &str,
    purpose: &str,
    decision: &str,
    criterion: &str,
) -> Value {
    let q = |v| json!({"value":v,"unit":"mm","precision":"synthetic test input"});
    let fact = |property: &str, value: Value| json!({"property":property,"value":value,"status":"declared","source_refs":[source]});
    let mut facts = vec![
        fact("state", json!("loaded")),
        fact("datum", json!("assembly-frame")),
        fact("diameter", q(10.0)),
        fact("section", json!("solid_round")),
    ];
    let mut assertions = vec![
        json!({"assertion_id":"mechanism","kind":"model_boundary","subject":"shaft-model",
        "requirements":{"method":"round-shaft-second-moment/1","inputs":{"diameter":q(10.0)}},"source_refs":[source],"affected_decisions":[decision]}),
    ];
    for id in ["part-a", "part-b", "alignment"] {
        let property = format!("acceptance:{id}");
        let r = json!({"schema":"oev2.numeric-acceptance/1","requirement_id":id,"requirement_property":property,
            "object":o,"state":"loaded","datum":"assembly-frame","metric":"absolute","relation":"maximum",
            "lower":null,"upper":q(0.03),"target":null,"tolerance":null,"definition_status":"confirmed",
            "coverage_status":"declared_complete","evidence_role":"measurement","source_refs":[source]});
        facts.push(fact(&property, r.clone()));
        assertions.push(json!({"assertion_id":id,"kind":"numeric_acceptance","subject":id,"source_refs":[source],"affected_decisions":[decision],
            "requirements":{"schema":"oev2.numeric-assessment/1","requirement":r,"calculation":null,
                "observation":{"object":o,"state":"loaded","datum":"assembly-frame","metric":"absolute","evidence_role":"measurement",
                    "values":[q(0.01)],"component_names":[id],"expanded_uncertainty":q(0.001),"uncertainty_basis":"synthetic interval, not calibration","source_refs":[source]},
                "condition_values":{},"source_versions":{source:o.source_sha256}}}));
    }
    let snapshot: serde_json::Map<String, Value> = facts
        .iter()
        .map(|f| (f["property"].as_str().unwrap().into(), f["value"].clone()))
        .collect();
    for a in &mut assertions[1..] {
        a["requirements"]["condition_values"] = json!(snapshot);
    }
    json!({"schema":"oev2.engineering-basis/1","object_id":o.object_id,"object_revision":o.revision,
        "purpose":purpose,"acceptance":[criterion],"facts":facts,"assertions":assertions})
}

pub fn composition(o: &ObjectIdentity, source: &str, decision: &str, criterion: &str) -> Value {
    let reference = |id| json!({"object":o,"assertion_id":id});
    json!({"assertion_id":"whole","kind":"claim_composition","subject":decision,"source_refs":[source],"affected_decisions":[decision],
        "requirements":{"schema":"oev2.claim-composition/1","decision_id":decision,"object":o,"criterion":criterion,
            "scope":"system","coverage":"declared_complete","all_of":[reference("part-a"),reference("part-b")],
            "any_of":[],"interfaces":[reference("alignment")]}})
}
