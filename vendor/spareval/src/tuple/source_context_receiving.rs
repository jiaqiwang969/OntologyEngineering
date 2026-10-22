//! Independent frozen binding-domain receiving. The oracle and this adapter
//! share an author; production tuple/context methods have another owner.
//! This file invokes the real tuple methods and real cached-term equality.
//! It does not implement a context evaluator. Graph/time/bnode context objects
//! and query startup are outside these primitive tests.
use super::{ErrorValue, InternalTuple, SolutionValue};
use crate::dataset::nullable_numeric::{CachedInternalTerm, NumericDataset};
use crate::dataset::{InternalQuad, QueryableDataset};
use crate::numeric::{
    LiteralSourceState, NumericCacheState, QueryGroupSemantics, QueryLiteralSemantics,
    QueryNumericCapabilities, QueryNumericSession, RawExpressionValue, RawNumericFault,
    SourceDatatype, SourceFact, SourceHashPolicy, SourceHashRuntime, SourceLiteralMetadata,
};
use oxrdf::{Literal, NamedNode, Term};
use semantica_numeric::{DecimalContext, NumericLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::rc::Rc;

type Cell = SolutionValue<CachedInternalTerm<Term>>;
type Row = InternalTuple<CachedInternalTerm<Term>>;
const FIXTURE: &str = include_str!("fixtures/source_context_primitives_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "a9ad3a9ee760ebc89e3541f73a6d67cea0d4a91a5d590bb348c5c5ed427d8790"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "4cc85999a7f5926fc9f89f74ecad3ce424255d6bd2dc614d17600a7488fa5773"
    );
    value
}
fn case(id: &str) -> Value {
    let root = fixture();
    ["source_cases", "active_safety_cases"]
        .into_iter()
        .flat_map(|key| root[key].as_array().unwrap())
        .find(|c| c["id"] == id)
        .unwrap()
        .clone()
}
struct IdentityDataset;
impl<'a> QueryableDataset<'a> for IdentityDataset {
    type InternalTerm = Term;
    type Error = Infallible;
    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Infallible>> + use<'a> {
        std::iter::empty()
    }
    fn internalize_term(&self, term: Term) -> Result<Term, Infallible> {
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Infallible> {
        Ok(term)
    }
}
struct Harness {
    slots: Vec<String>,
    values: BTreeMap<String, Cell>,
    dataset: NumericDataset<'static, IdentityDataset>,
    session: QueryNumericSession,
}
impl Harness {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            values: BTreeMap::new(),
            dataset: NumericDataset::new_with_literal_semantics(
                IdentityDataset,
                QueryLiteralSemantics::SourceLiteralV1,
            ),
            session: QueryNumericSession::new_with_literal_semantics(
                DecimalContext::default(),
                NumericLimits::default(),
                None,
                true,
                QueryNumericCapabilities::MixedBinary64V1,
                QueryGroupSemantics::SourceAggregateV1,
                QueryLiteralSemantics::SourceLiteralV1,
            )
            .unwrap(),
        }
    }
    fn slot(&mut self, name: &str) -> usize {
        if let Some(i) = self.slots.iter().position(|x| x == name) {
            i
        } else {
            self.slots.push(name.to_owned());
            self.slots.len() - 1
        }
    }
    fn value(&mut self, token: &Value) -> Cell {
        if token.is_null() || token == "None" {
            return SolutionValue::ExplicitNone;
        }
        let name = token.as_str().unwrap();
        if let Some(v) = self.values.get(name) {
            return v.clone();
        }
        let value = if name.starts_with("ERROR_") {
            SolutionValue::ReturnedError(ErrorValue::new("same source message"))
        } else {
            let raw = match name {
                "LANG_EN" => self
                    .session
                    .construct_source_direct_string("v", None, Some("EN".into()), true)
                    .unwrap(),
                "LANG_en" => self
                    .session
                    .construct_source_direct_string("v", None, Some("en".into()), true)
                    .unwrap(),
                "OTHER_LANG" => self
                    .session
                    .construct_source_direct_string("different", None, Some("en".into()), true)
                    .unwrap(),
                _ => RawExpressionValue::from_import(
                    NamedNode::new(format!("urn:context:{name}"))
                        .unwrap()
                        .into(),
                ),
            };
            SolutionValue::Rdf(self.dataset.internalize_raw(raw).unwrap())
        };
        self.values.insert(name.to_owned(), value.clone());
        value
    }
    fn row(&mut self, input: &Value) -> Row {
        let mut row = Row::with_capacity(16);
        let entries: Vec<(String, Value)> = if let Some(array) = input.as_array() {
            array
                .iter()
                .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
                .collect()
        } else {
            // The six reused maps contain only single-digit slots; order is
            // asserted against the separately frozen ordered_local arrays.
            input
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        for (name, value) in entries {
            let slot = self.slot(&name);
            let value = self.value(&value);
            row.set_cell(slot, value);
        }
        row
    }
    fn label(&self, value: &Cell) -> Value {
        if matches!(value, SolutionValue::ExplicitNone) {
            return Value::Null;
        }
        for (name, original) in &self.values {
            let same = match (value, original) {
                (SolutionValue::Rdf(a), SolutionValue::Rdf(b)) => a.same_transport_instance(b),
                (SolutionValue::ReturnedError(a), SolutionValue::ReturnedError(b)) => a == b,
                _ => false,
            };
            if same {
                return json!(name);
            }
        }
        panic!("output replaced an original cell object");
    }
    fn ordered(&self, row: &Row) -> Value {
        Value::Array(
            row.source_local_order()
                .iter()
                .map(|&slot| json!([self.slots[slot], self.label(row.cell(slot).unwrap())]))
                .collect(),
        )
    }
    fn columns(&mut self, value: &Value) -> Vec<usize> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|x| self.slot(x.as_str().unwrap()))
            .collect()
    }
}
fn equal(a: &Cell, b: &Cell) -> Result<bool, RawNumericFault> {
    Ok(match (a, b) {
        (SolutionValue::Rdf(a), SolutionValue::Rdf(b)) => a.source_context_equal(b)?,
        (SolutionValue::ExplicitNone, SolutionValue::ExplicitNone) => true,
        (SolutionValue::ReturnedError(a), SolutionValue::ReturnedError(b)) => a == b,
        _ => false,
    })
}
fn prime(row: &Row) {
    row.source_hash_by(|_| Ok::<_, Infallible>(1), |_| Ok(2))
        .unwrap();
}
fn fresh(from: &Row, to: &Row) {
    assert!(!from.source_same_object(to));
    assert_eq!(to.source_raw_hash_cache(), None);
}
fn native_row_hash(h: &Harness, row: &Row) -> i64 {
    let runtime = SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap();
    row.source_hash_by(
        |slot| Ok::<_, RawNumericFault>(runtime.variable_hash(&h.slots[slot])),
        |cell| match cell {
            SolutionValue::Rdf(value) => runtime.term_hash(value.source_raw()?),
            SolutionValue::ExplicitNone => Ok(crate::source_hash::hash_source_none()),
            SolutionValue::ReturnedError(error) => Ok(error.source_native_identity_hash()),
        },
    )
    .unwrap()
}
fn receive(id: &str) {
    let c = case(id);
    let input = &c["input"];
    let e = &c["expected"];
    let mut h = Harness::new();
    match id {
        "CP01" | "CP02" | "CP03" | "CP04" | "CP05" | "CP06" => {
            let current = h.row(&input["current"]);
            let initial = h.row(&input["initial"]);
            let frozen = h.row(&input["frozen"]);
            let out = current.source_thaw(&frozen, &initial);
            assert_eq!(h.ordered(&out), e["ordered_local"]);
            fresh(&current, &out);
            fresh(&frozen, &out);
        }
        "CP07" => {
            let base = h.row(&input["bindings"]);
            let initial = h.row(&input["initial"]);
            let before = h.ordered(&initial);
            let out = base.source_push(&initial);
            assert_eq!(h.ordered(&out), e["ordered_current"]);
            assert_eq!(h.ordered(&initial), before);
            fresh(&base, &out);
        }
        "CP08" => {
            let current = h.row(&input["current"]);
            prime(&current);
            let a = h.slot("a");
            let b = h.slot("b");
            let rows = [
                current.source_solution(None),
                current.source_solution(Some(&[])),
                current.source_solution(Some(&[a])),
                current.source_solution(None).source_project(&[]),
                current.source_solution(None).source_project(&[a, b]),
            ];
            for (row, expected) in rows.iter().zip(e["ordered_locals"].as_array().unwrap()) {
                assert_eq!(h.ordered(row), *expected);
                fresh(&current, row);
            }
            for (i, a) in rows.iter().enumerate() {
                for b in &rows[i + 1..] {
                    assert!(!a.source_same_object(b));
                }
            }
        }
        "CP09" => {
            let initial = h.row(&input["initial"]);
            let row = h.row(&input["local"]);
            for (name, expected) in input["lookups"]
                .as_array()
                .unwrap()
                .iter()
                .zip(e["results"].as_array().unwrap())
            {
                let slot = h.slot(name.as_str().unwrap());
                let actual = row.lookup_cell(slot, &initial);
                if expected.get("exception").is_some() {
                    assert!(actual.is_none());
                } else {
                    assert_eq!(h.label(&actual.unwrap()), expected["value"]);
                }
            }
            assert_eq!(
                row.source_local_order().len(),
                e["local_length"].as_u64().unwrap() as usize
            );
            let names: Vec<_> = row
                .source_local_order()
                .iter()
                .map(|&i| h.slots[i].clone())
                .collect();
            assert_eq!(json!(names), e["local_keys"]);
        }
        "CP10" => {
            let row = h.row(&input["current"]);
            for (i, name) in input["lookups"].as_array().unwrap().iter().enumerate() {
                let slot = h.slot(name.as_str().unwrap());
                assert_eq!(
                    row.contains_cell(slot),
                    e["membership"][i].as_bool().unwrap()
                );
                assert!(matches!(
                    row.cell(slot),
                    None | Some(SolutionValue::ExplicitNone)
                ));
            }
        }
        "CP11" => {
            let mut row = h.row(&input["current"]);
            for (i, step) in input["sequence"].as_array().unwrap().iter().enumerate() {
                let slot = h.slot(step["key"].as_str().unwrap());
                let value = h.value(&step["value"]);
                assert_eq!(row.source_assign_by(slot, value, equal).unwrap(), i < 2);
                if i == 1 {
                    assert_eq!(h.ordered(&row), e["after_success"]);
                }
            }
            assert_eq!(h.ordered(&row), e["after_failure"]);
        }
        "CP12" => {
            for (run, expected) in input["runs"]
                .as_array()
                .unwrap()
                .iter()
                .zip(e["runs"].as_array().unwrap())
            {
                let mut row = h.row(&run["current"]);
                let slot = h.slot(run["key"].as_str().unwrap());
                let value = h.value(&run["value"]);
                let accepted = row.source_assign_by(slot, value, equal).unwrap();
                assert_eq!(accepted, expected.get("exception").is_none());
                assert_eq!(h.ordered(&row), expected["ordered_current"]);
            }
        }
        "CP13" => {
            let row = h.row(&input["row_local"]);
            let initial = h.row(&input["row_initial"]);
            let before = h.row(&input["before_current"]);
            let except = h.columns(&input["except"]);
            prime(&row);
            let out = row.source_forget(&before, &initial, &except);
            assert_eq!(h.ordered(&out), e["ordered_local"]);
            fresh(&row, &out);
        }
        "CP14" => {
            let row = h.row(&input["row_local"]);
            prime(&row);
            for (columns, expected) in input["runs"]
                .as_array()
                .unwrap()
                .iter()
                .zip(e["runs"].as_array().unwrap())
            {
                let columns = h.columns(columns);
                let out = row.source_project(&columns);
                assert_eq!(h.ordered(&out), expected["ordered_local"]);
                fresh(&row, &out);
            }
        }
        "CP15" => {
            let left = h.row(&input["left"]["local"]);
            let right = h.row(&input["right"]["local"]);
            prime(&left);
            prime(&right);
            let a = left.source_merge(&right);
            let b = right.source_merge(&left);
            assert_eq!(h.ordered(&a), e["left_merge_right"]["local"]);
            assert_eq!(h.ordered(&b), e["right_merge_left"]["local"]);
            fresh(&left, &a);
            fresh(&right, &b);
            // Context ownership is caller state, not a made-up field/token on
            // this row. The integration receiver must check that separately.
        }
        "CP16" => {
            let left = h.row(&input["left"]["local"]);
            let li = h.row(&input["left"]["initial"]);
            for (i, run) in input["right_runs"].as_array().unwrap().iter().enumerate() {
                let right = h.row(&run["local"]);
                let ri = h.row(&run["initial"]);
                assert_eq!(
                    left.source_compatible_by(&right, &ri, equal).unwrap(),
                    e["compatible_left_to_right"][i].as_bool().unwrap()
                );
                assert_eq!(
                    right.source_compatible_by(&left, &li, equal).unwrap(),
                    e["compatible_right_to_left"][i].as_bool().unwrap()
                );
                assert_eq!(
                    left.domain_disjoint(&right),
                    e["disjoint_local_domains"][i].as_bool().unwrap()
                );
            }
        }
        "CP17" => {
            for (i, run) in input["runs"].as_array().unwrap().iter().enumerate() {
                let left = h.row(&run["left"]["local"]);
                let right = h.row(&run["right"]["local"]);
                assert_eq!(
                    left.source_mapping_equal_by(&right, equal).unwrap(),
                    e["equal"][i].as_bool().unwrap()
                );
                if e["equal"][i] == true {
                    assert_eq!(native_row_hash(&h, &left), native_row_hash(&h, &right));
                }
            }
        }
        "CP18" | "CP19" | "CP20" => receive_hash(&mut h, input, e, id),
        "CP21" => {
            let row = h.row(&json!([["x", "T"], ["y", "U"]]));
            prime(&row);
            let cloned = row.clone();
            assert!(row.source_same_object(&cloned));
            assert_eq!(row.source_raw_hash_cache(), cloned.source_raw_hash_cache());
            let empty = h.row(&json!([]));
            let columns = row.source_local_order().to_vec();
            let outputs = [
                row.source_project(&columns),
                row.source_merge(&empty),
                row.source_project(&columns),
                row.source_forget(&empty, &empty, &[]),
                row.source_solution(None),
            ];
            for out in outputs {
                assert_eq!(h.ordered(&row), h.ordered(&out));
                fresh(&row, &out);
            }
        }
        "CP22" => {
            let base = h.row(&input["current"]);
            let initial = h.row(&input["initial"]);
            let current = base.source_push(&initial);
            assert_eq!(h.ordered(&current), e["current_before_push"]);
            let out = current.source_push(&initial);
            assert_eq!(h.ordered(&out), e["current_after_push"]);
            fresh(&current, &out);
        }
        "CP23" => {
            for (i, pair) in input["runs"].as_array().unwrap().iter().enumerate() {
                let a = h.value(&pair[0]);
                let b = h.value(&pair[1]);
                let mut left = Row::with_capacity(1);
                let mut right = Row::with_capacity(1);
                left.set_cell(0, a);
                right.set_cell(0, b);
                let initial = Row::with_capacity(0);
                assert_eq!(
                    left.source_compatible_by(&right, &initial, equal).unwrap(),
                    e["equal"][i].as_bool().unwrap()
                );
                assert_eq!(
                    left.source_mapping_equal_by(&right, equal).unwrap(),
                    e["equal"][i].as_bool().unwrap()
                );
            }
        }
        _ => panic!("unreceived {id}"),
    }
}
#[derive(Debug)]
struct HashMarker;
fn receive_hash(h: &mut Harness, input: &Value, e: &Value, id: &str) {
    let words: BTreeMap<String, i64> = input["component_hash_words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_owned(), p[1].as_i64().unwrap()))
        .collect();
    let failures: Vec<Option<&str>> = if id == "CP20" {
        input["runs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["raise_at"].as_str())
            .collect()
    } else {
        vec![None]
    };
    for (index, failure) in failures.into_iter().enumerate() {
        let row = h.row(&input["local"]);
        let marker = Rc::new(HashMarker);
        let calls = RefCell::new(Vec::<String>::new());
        let hash = |name: String| -> Result<i64, Rc<HashMarker>> {
            calls.borrow_mut().push(name.clone());
            if failure == Some(name.as_str()) {
                Err(Rc::clone(&marker))
            } else {
                Ok(words[&name])
            }
        };
        let first = row.source_hash_by(
            |slot| hash(h.slots[slot].clone()),
            |value| hash(h.label(value).as_str().unwrap().to_owned()),
        );
        if id == "CP20" {
            assert!(Rc::ptr_eq(&first.unwrap_err(), &marker));
            assert_eq!(json!(*calls.borrow()), e["first_calls"][index]);
            assert_eq!(
                row.source_raw_hash_cache().unwrap(),
                e["raw_cache_after_error"][index].as_i64().unwrap()
            );
        } else {
            assert_eq!(
                first.unwrap(),
                e["returned_normalized_words"][0].as_i64().unwrap()
            );
            assert_eq!(json!(*calls.borrow()), e["component_calls"][0]);
            assert_eq!(
                row.source_raw_hash_cache().unwrap(),
                e["raw_cache_after_calls"][0].as_i64().unwrap()
            );
        }
        calls.borrow_mut().clear();
        let second = row
            .source_hash_by(
                |slot| hash(h.slots[slot].clone()),
                |value| hash(h.label(value).as_str().unwrap().to_owned()),
            )
            .unwrap();
        assert!(calls.borrow().is_empty());
        let expected = if id == "CP20" {
            &e["second_returned_normalized_words"][index]
        } else {
            &e["returned_normalized_words"][1]
        };
        assert_eq!(second, expected.as_i64().unwrap());
    }
}
fn partial_literal(
    lexical: &str,
    datatype: SourceDatatype,
    language: SourceFact<Option<String>>,
) -> RawExpressionValue {
    RawExpressionValue::from_source_parts(
        Literal::new_simple_literal(lexical).into(),
        NumericCacheState::NotNumeric,
        LiteralSourceState::Literal(SourceLiteralMetadata {
            datatype,
            language_original: language,
            ill_typed: SourceFact::Unknown,
            cached_value_kind: SourceFact::Unknown,
        }),
    )
    .unwrap()
}
fn receive_active(id: &str) {
    let frozen = case(id);
    let expected = &frozen["expected"];
    let h = Harness::new();
    let internal = |raw| h.dataset.internalize_raw(raw).unwrap();
    match id {
        "CPA01" => {
            let a = internal(RawExpressionValue::from_import(
                Literal::new_simple_literal("unknown").into(),
            ));
            let b = a.clone();
            assert!(a.same_transport_instance(&b));
            assert_eq!(
                a.source_context_equal(&b).unwrap(),
                expected["equal"].as_bool().unwrap()
            );
        }
        "CPA02" => {
            let a = internal(RawExpressionValue::from_import(
                Literal::new_simple_literal("unknown").into(),
            ));
            let b = internal(RawExpressionValue::from_import(
                NamedNode::new("urn:node").unwrap().into(),
            ));
            assert_eq!(
                a.source_context_equal(&b).unwrap(),
                expected["equal"].as_bool().unwrap()
            );
        }
        "CPA03" => {
            let a = internal(partial_literal(
                "same",
                SourceDatatype::Absent,
                SourceFact::Unknown,
            ));
            let b = internal(partial_literal(
                "same",
                SourceDatatype::Specified(
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
                ),
                SourceFact::Unknown,
            ));
            assert_eq!(
                a.source_context_equal(&b).unwrap(),
                expected["equal"].as_bool().unwrap()
            );
        }
        "CPA04" => {
            assert_eq!(expected["fault"], "UnknownSourceLiteralIdentity");
            let a = internal(partial_literal(
                "same",
                SourceDatatype::Absent,
                SourceFact::Unknown,
            ));
            let b = internal(partial_literal(
                "same",
                SourceDatatype::Absent,
                SourceFact::Known(None),
            ));
            assert!(matches!(
                a.source_context_equal(&b),
                Err(RawNumericFault::UnknownSourceLiteralIdentity)
            ));
        }
        "CPA05" => {
            let a = internal(partial_literal(
                "a",
                SourceDatatype::Absent,
                SourceFact::Known(None),
            ));
            let b = internal(partial_literal(
                "b",
                SourceDatatype::Absent,
                SourceFact::Known(None),
            ));
            assert_eq!(
                a.source_context_equal(&b).unwrap(),
                expected["equal"].as_bool().unwrap()
            );
        }
        _ => panic!("unreceived {id}"),
    }
    // Unknown facts act as real refusal sentinels. No synthetic property-read
    // counter claims to observe Rust field accesses or a Python query.
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

#[test]
fn source_context_primitive_fixture_identity() {
    let f = fixture();
    assert_eq!(f["source_cases"].as_array().unwrap().len(), 23);
    assert_eq!(f["active_safety_cases"].as_array().unwrap().len(), 5);
    let payload = json!({"value_dictionary":f["value_dictionary"],"source_cases":f["source_cases"],"active_safety_cases":f["active_safety_cases"]});
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&payload)).unwrap())
        ),
        f["canonical_payload_sha256"].as_str().unwrap()
    );
}
#[test]
fn source_context_primitive_cp01() {
    receive("CP01");
}
#[test]
fn source_context_primitive_cp02() {
    receive("CP02");
}
#[test]
fn source_context_primitive_cp03() {
    receive("CP03");
}
#[test]
fn source_context_primitive_cp04() {
    receive("CP04");
}
#[test]
fn source_context_primitive_cp05() {
    receive("CP05");
}
#[test]
fn source_context_primitive_cp06() {
    receive("CP06");
}
#[test]
fn source_context_primitive_cp07() {
    receive("CP07");
}
#[test]
fn source_context_primitive_cp08() {
    receive("CP08");
}
#[test]
fn source_context_primitive_cp09() {
    receive("CP09");
}
#[test]
fn source_context_primitive_cp10() {
    receive("CP10");
}
#[test]
fn source_context_primitive_cp11() {
    receive("CP11");
}
#[test]
fn source_context_primitive_cp12() {
    receive("CP12");
}
#[test]
fn source_context_primitive_cp13() {
    receive("CP13");
}
#[test]
fn source_context_primitive_cp14() {
    receive("CP14");
}
#[test]
fn source_context_primitive_cp15() {
    receive("CP15");
}
#[test]
fn source_context_primitive_cp16() {
    receive("CP16");
}
#[test]
fn source_context_primitive_cp17() {
    receive("CP17");
}
#[test]
fn source_context_primitive_cp18() {
    receive("CP18");
}
#[test]
fn source_context_primitive_cp19() {
    receive("CP19");
}
#[test]
fn source_context_primitive_cp20() {
    receive("CP20");
}
#[test]
fn source_context_primitive_cp21() {
    receive("CP21");
}
#[test]
fn source_context_primitive_cp22() {
    receive("CP22");
}
#[test]
fn source_context_primitive_cp23() {
    receive("CP23");
}
#[test]
fn source_context_primitive_cpa01() {
    receive_active("CPA01");
}
#[test]
fn source_context_primitive_cpa02() {
    receive_active("CPA02");
}
#[test]
fn source_context_primitive_cpa03() {
    receive_active("CPA03");
}
#[test]
fn source_context_primitive_cpa04() {
    receive_active("CPA04");
}
#[test]
fn source_context_primitive_cpa05() {
    receive_active("CPA05");
}
