//! Source-derived owner lifetime receiving. Expectations and this adapter have
//! one author; production carrier operations have another owner. These are real
//! tuple operations, not an AST evaluator. CO09/CO10 exercise producer protocols;
//! actual Group/VALUES wiring and the two consumer safety obligations are separate.
use super::{InternalTuple, SolutionValue};
use oxrdf::NamedNode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::rc::Rc;

type Row = InternalTuple<Rc<NamedNode>>;
const FIXTURE: &str = include_str!("fixtures/source_context_owner_cases.json");
const SLOTS: &[&str] = &["a", "b", "i", "j", "x", "y", "z", "q", "agg", "v"];

fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "e7b20d214d7eeacbde1f25e1a7ee747d54cea6b8472af6b1a032844020ecacfd"
    );
    let f: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        f["source_record_sha256"],
        "b9aeb9841ea106723b9d9249305f4b6533efac01b7165a61838d89b86a24b75c"
    );
    f
}
fn case(id: &str) -> Value {
    fixture()["payload"]["primitive_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
        .clone()
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), canonical(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn slot(name: &str) -> usize {
    SLOTS.iter().position(|s| *s == name).unwrap()
}
struct Harness {
    values: BTreeMap<String, Rc<NamedNode>>,
}
impl Harness {
    fn new() -> Self {
        let f = fixture();
        Self {
            values: f["payload"]["encoding"]["source_values"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, v)| {
                    (
                        name.clone(),
                        Rc::new(NamedNode::new(v["iri"].as_str().unwrap()).unwrap()),
                    )
                })
                .collect(),
        }
    }
    fn cell(&self, value: &Value) -> SolutionValue<Rc<NamedNode>> {
        if value.is_null() {
            SolutionValue::ExplicitNone
        } else {
            SolutionValue::Rdf(Rc::clone(&self.values[value.as_str().unwrap()]))
        }
    }
    fn row(&self, entries: &Value) -> Row {
        let mut row = Row::with_capacity(SLOTS.len());
        for pair in entries.as_array().unwrap() {
            row.set_cell(slot(pair[0].as_str().unwrap()), self.cell(&pair[1]));
        }
        row
    }
    fn label(&self, cell: &SolutionValue<Rc<NamedNode>>) -> Value {
        match cell {
            SolutionValue::ExplicitNone => Value::Null,
            SolutionValue::Rdf(value) => json!(
                self.values
                    .iter()
                    .find(|(_, other)| Rc::ptr_eq(value, other))
                    .unwrap()
                    .0
            ),
            SolutionValue::ReturnedError(_) => {
                panic!("owner fixture contains no returned-error input")
            }
        }
    }
    fn assert_local(&self, row: &Row, expected: &Value) {
        let actual: Vec<_> = row
            .source_local_order()
            .iter()
            .map(|&i| json!([SLOTS[i], self.label(row.cell(i).unwrap())]))
            .collect();
        assert_eq!(json!(actual), *expected);
    }
    fn assert_owner_bindings(&self, row: &Row, expected: &Value) {
        let snapshot = row
            .source_context_bindings()
            .expect("actual owner bindings");
        self.assert_local(&snapshot, expected);
        assert!(
            snapshot.source_thaw_owned().is_none(),
            "owner storage snapshots must be ownerless"
        );
    }
    fn context(&self, bindings: &Value, initial: &Rc<Row>) -> Row {
        self.row(bindings)
            .source_context_from_bindings(Rc::clone(initial))
    }
    fn assign(&self, current: &mut Row, name: &str, value: &str) {
        assert!(
            current
                .source_assign_by(slot(name), self.cell(&json!(value)), |a, b| {
                    Ok::<_, Infallible>(a == b)
                })
                .unwrap()
        );
    }
}

#[test]
fn source_context_owner_fixture_identity() {
    let f = fixture();
    let payload = &f["payload"];
    assert_eq!(payload["primitive_cases"].as_array().unwrap().len(), 13);
    assert_eq!(
        payload["active_boundary_contracts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(payload)).unwrap())
        ),
        f["canonical_payload_sha256"].as_str().unwrap()
    );
    assert!(!FIXTURE.contains("/Users/"));
}

#[test]
fn source_context_owner_co01_lookup_local_then_initial_only() {
    let c = case("CO01");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let owner = h.context(&input["owner_constructor_bindings"], &initial);
    let row = h.row(&input["frozen_local"]).source_with_context_of(&owner);
    h.assert_owner_bindings(&row, &e["owner_bindings"]);
    h.assert_local(&row, &e["frozen_local"]);
    assert!(row.source_context_lookup(slot("x")).is_none());
    assert_eq!(
        h.label(&row.source_context_lookup(slot("i")).unwrap()),
        e["frozen_i"]
    );
    assert_eq!(h.label(&owner.cell(slot("x")).unwrap()), e["context_x"]);
    assert!(row.source_context_owner_same(&owner));
}

#[test]
fn source_context_owner_co02_empty_thaw_restores_owner_noninitial_bindings() {
    let c = case("CO02");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let owner = h.context(&input["owner_constructor_bindings"], &initial);
    let before = owner.source_context_bindings().unwrap();
    let row = h.row(&input["frozen_local"]).source_with_context_of(&owner);
    let thawed = row.source_thaw_owned().unwrap();
    h.assert_local(&thawed, &e["D_bindings"]);
    h.assert_owner_bindings(&thawed, &e["D_bindings"]);
    assert!(!thawed.source_context_owner_same(&owner));
    assert!(thawed.source_context_initial_shared(&owner));
    assert!(owner.source_context_bindings().unwrap() == before);
}

#[test]
fn source_context_owner_co03_explicit_none_is_nonempty_for_thaw() {
    let c = case("CO03");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let owner = h.context(&input["owner_constructor_bindings"], &initial);
    let row = h.row(&input["frozen_local"]).source_with_context_of(&owner);
    let thawed = row.source_thaw_owned().unwrap();
    h.assert_local(&thawed, &e["D_bindings"]);
    assert!(!thawed.contains_cell(slot("x")));
    assert!(!thawed.source_context_owner_same(&owner));
    assert!(thawed.source_context_initial_shared(&owner));
}

#[test]
fn source_context_owner_co04_solution_project_and_transport_identity() {
    let c = case("CO04");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let owner = h.context(&input["owner_constructor_bindings"], &initial);
    let row = owner.source_solution(Some(&[]));
    let transported = row.clone();
    let empty = row.source_project(&[]);
    let projected = row.source_project(&[slot("x"), slot("i")]);
    for (r, key) in [
        (&row, "F_local"),
        (&transported, "H_local"),
        (&empty, "P_local"),
        (&projected, "Q_local"),
    ] {
        h.assert_local(r, &e[key]);
        assert!(r.source_context_owner_same(&owner));
        assert_eq!(r.source_raw_hash_cache(), None);
    }
    assert!(row.source_same_object(&transported));
    assert!(!row.source_same_object(&empty));
    assert!(!row.source_same_object(&projected));
    assert!(!empty.source_same_object(&projected));
}

#[test]
fn source_context_owner_co05_forget_remember_retain_self_owner() {
    let c = case("CO05");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let owner = h.context(&input["owner_constructor_bindings"], &initial);
    let before = h.context(&input["before_bindings"], &initial);
    let row = h.row(&input["frozen_local"]).source_with_context_of(&owner);
    let forgotten = row.source_forget(&before, &initial, &[]);
    let remembered = row.source_project(&[]);
    h.assert_local(&forgotten, &e["P_local"]);
    h.assert_local(&remembered, &e["R_local"]);
    assert!(forgotten.source_context_owner_same(&owner));
    assert!(remembered.source_context_owner_same(&owner));
    assert!(!forgotten.source_same_object(&row));
    assert!(!remembered.source_same_object(&row));
    assert!(!remembered.source_same_object(&forgotten));
    let thawed = remembered.source_thaw_owned().unwrap();
    h.assert_local(&thawed, &e["D_bindings"]);
    assert!(!thawed.source_context_owner_same(&owner));
}

#[test]
fn source_context_owner_co06_directional_merge_keeps_left_context() {
    let c = case("CO06");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["shared_initial"]));
    let left_owner = h.context(&input["left_owner_bindings"], &initial);
    let right_owner = h.context(&input["right_owner_bindings"], &initial);
    let left_before = left_owner.source_context_bindings().unwrap();
    let right_before = right_owner.source_context_bindings().unwrap();
    let left = h
        .row(&input["left_local"])
        .source_with_context_of(&left_owner);
    let right = h
        .row(&input["right_local"])
        .source_with_context_of(&right_owner);
    let merged = left.source_merge(&right);
    let reversed = right.source_merge(&left);
    h.assert_local(&merged, &e["M_local"]);
    h.assert_local(&reversed, &e["N_local"]);
    assert!(merged.source_context_owner_same(&left_owner));
    assert!(reversed.source_context_owner_same(&right_owner));
    assert!(!merged.source_same_object(&left));
    assert!(!reversed.source_same_object(&right));
    assert_eq!(merged.source_raw_hash_cache(), None);
    assert_eq!(reversed.source_raw_hash_cache(), None);
    h.assert_local(
        &merged.source_project(&[]).source_thaw_owned().unwrap(),
        &e["DM_bindings"],
    );
    h.assert_local(
        &reversed.source_project(&[]).source_thaw_owned().unwrap(),
        &e["DN_bindings"],
    );
    assert!(left_owner.source_context_bindings().unwrap() == left_before);
    assert!(right_owner.source_context_bindings().unwrap() == right_before);
}

#[test]
fn source_context_owner_co07_live_owner_mutation_preserves_frozen_snapshot() {
    let c = case("CO07");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let mut owner = h.context(&input["owner_constructor_bindings"], &initial);
    let frozen = owner.source_solution(None);
    h.assign(&mut owner, "y", "Y");
    h.assert_local(&owner, &e["C_bindings_after"]);
    h.assert_owner_bindings(&frozen, &e["C_bindings_after"]);
    h.assert_local(&frozen, &e["F_local_after"]);
    assert!(frozen.source_context_lookup(slot("y")).is_none());
    assert!(frozen.source_context_owner_same(&owner));
    let from_empty = frozen.source_project(&[]).source_thaw_owned().unwrap();
    let from_nonempty = frozen.source_thaw_owned().unwrap();
    h.assert_local(&from_empty, &e["D_bindings"]);
    h.assert_local(&from_nonempty, &e["E_bindings"]);
    assert!(!from_empty.source_context_owner_same(&owner));
    assert!(!from_nonempty.source_context_owner_same(&owner));
    assert!(!from_empty.source_context_owner_same(&from_nonempty));
    assert!(from_empty.source_context_initial_shared(&owner));
    assert!(from_nonempty.source_context_initial_shared(&owner));
}

#[test]
fn source_context_owner_co08_push_flattens_and_retains_initial_reference() {
    let c = case("CO08");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let owner = h.context(&input["constructor_bindings"], &initial);
    h.assert_local(&owner, &e["C_bindings"]);
    let pushed = owner.source_push(&initial);
    let frozen = pushed.source_solution(None);
    h.assert_local(&pushed, &e["D_bindings"]);
    h.assert_owner_bindings(&pushed, &e["D_bindings"]);
    h.assert_local(&frozen, &e["F_local"]);
    assert!(!pushed.source_context_owner_same(&owner));
    assert!(frozen.source_context_owner_same(&pushed));
    assert!(pushed.source_context_initial_shared(&owner));
    h.assert_owner_bindings(&owner, &e["C_bindings"]);
}

#[test]
fn source_context_owner_co09_group_output_constructor_uses_call_context() {
    let c = case("CO09");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["shared_initial"]));
    let group_call = h.context(&input["G_bindings"], &initial);
    let child = h.context(&input["V_bindings"], &initial);
    // Actual FrozenBindings(ctx, bindings) carrier boundary, not a simulated
    // aggregate. Full query receiving must independently check producer wiring.
    let output = h
        .row(&input["aggregate_finished_local"])
        .source_with_context_of(&group_call);
    let empty = h.row(&json!([])).source_with_context_of(&group_call);
    h.assert_local(&output, &e["F_local"]);
    h.assert_local(&empty, &e["E_local"]);
    assert!(output.source_context_owner_same(&group_call));
    assert!(empty.source_context_owner_same(&group_call));
    assert!(!output.source_context_owner_same(&child));
    assert!(!empty.source_context_owner_same(&child));
    assert!(!output.source_same_object(&empty));
    h.assert_local(&empty.source_thaw_owned().unwrap(), &e["D_bindings"]);
}

#[test]
fn source_context_owner_co10_values_push_assignment_owns_incoming_object() {
    let c = case("CO10");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let parent = h.context(&input["constructor_bindings"], &initial);
    let mut outputs = Vec::new();
    for record in input["values_records"].as_array().unwrap() {
        let mut pushed = parent.source_push(&initial);
        let name = record[0][0].as_str().unwrap();
        let value = record[0][1].as_str().unwrap();
        h.assign(&mut pushed, name, value);
        let frozen = pushed.source_solution(None);
        assert!(frozen.source_context_owner_same(&pushed));
        assert!(frozen.source_context_initial_shared(&parent));
        outputs.push(frozen);
    }
    h.assert_local(&outputs[0], &e["first_local"]);
    h.assert_local(&outputs[1], &e["second_local"]);
    h.assert_owner_bindings(&outputs[0], &e["first_owner_bindings"]);
    h.assert_owner_bindings(&outputs[1], &e["second_owner_bindings"]);
    assert!(!outputs[0].source_context_owner_same(&outputs[1]));
    assert!(!outputs[0].source_context_owner_same(&parent));
    assert!(!outputs[1].source_context_owner_same(&parent));
    h.assert_local(&parent, &e["parent_bindings"]);
    h.assert_owner_bindings(&parent, &e["parent_bindings"]);
}

#[test]
fn source_context_owner_co11_explicit_thaw_receiver_differs_from_owned_thaw() {
    let c = case("CO11");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let receiver_initial = Rc::new(h.row(&input["receiver_initial"]));
    let frozen_initial = Rc::new(h.row(&input["B_initial"]));
    let receiver = h.context(&input["receiver_bindings"], &receiver_initial);
    let other = h.context(&input["B_bindings"], &frozen_initial);
    let frozen = h.row(&input["frozen_local"]).source_with_context_of(&other);
    let explicit = receiver.source_thaw(&frozen, &receiver_initial);
    let owned = frozen.source_thaw_owned().unwrap();
    h.assert_local(&explicit, &e["D_bindings"]);
    h.assert_local(&owned, &e["E_bindings"]);
    assert!(explicit.source_context_initial_shared(&receiver));
    assert!(owned.source_context_initial_shared(&other));
    assert!(!explicit.source_context_initial_shared(&other));
    assert!(!owned.source_context_initial_shared(&receiver));
    assert!(!explicit.source_context_owner_same(&receiver));
    assert!(!owned.source_context_owner_same(&other));
    assert!(!explicit.source_context_owner_same(&owned));
}

#[test]
fn source_context_owner_co12_clean_has_false_empty_clone_fallback() {
    let c = case("CO12");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["initial"]));
    let owner = h.context(&input["owner_constructor_bindings"], &initial);
    let cleaned = owner.source_context_clean().unwrap();
    h.assert_local(&cleaned, &e["D_bindings"]);
    assert!(!cleaned.source_context_owner_same(&owner));
    assert!(cleaned.source_context_initial_shared(&owner));
}

#[test]
fn source_context_owner_co13_owner_does_not_enter_local_equality_or_hash() {
    let c = case("CO13");
    let h = Harness::new();
    let input = &c["input"];
    let e = &c["expected"];
    let initial = Rc::new(h.row(&input["shared_initial"]));
    let mut left_owner = h.context(&input["left_owner_bindings"], &initial);
    let mut right_owner = h.context(&input["right_owner_bindings"], &initial);
    let left = h
        .row(&input["left_local"])
        .source_with_context_of(&left_owner);
    let right = h
        .row(&input["right_local"])
        .source_with_context_of(&right_owner);
    assert!(!left.source_same_object(&right));
    assert!(
        left.source_mapping_equal_by(&right, |a, b| Ok::<_, Infallible>(a == b))
            .unwrap()
    );
    let calls = Cell::new(0);
    for row in [&left, &right] {
        let hash = row
            .source_hash_by(
                |_| {
                    calls.set(calls.get() + 1);
                    Ok::<_, Infallible>(input["hash_callback_words"]["key_x"].as_i64().unwrap())
                },
                |_| {
                    calls.set(calls.get() + 1);
                    Ok::<_, Infallible>(input["hash_callback_words"]["value_L"].as_i64().unwrap())
                },
            )
            .unwrap();
        assert_eq!(hash, e["raw_hash_each"].as_i64().unwrap());
    }
    assert_eq!(calls.get(), 4);
    h.assign(&mut left_owner, "z", "Z");
    h.assign(&mut right_owner, "z", "Z");
    for row in [&left, &right] {
        assert_eq!(
            row.source_raw_hash_cache(),
            e["owner_change_keeps_local_hash_cache"].as_i64()
        );
        assert_eq!(
            row.source_hash_by::<Infallible>(
                |_| panic!("cached row rehashed key"),
                |_| panic!("cached row rehashed value")
            )
            .unwrap(),
            e["raw_hash_each"].as_i64().unwrap()
        );
    }
    h.assert_local(&left, &input["left_local"]);
    h.assert_local(&right, &input["right_local"]);
}
