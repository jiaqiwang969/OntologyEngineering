//! Original algebra.triples receiving shares the actual parser's row owner and
//! the compiled evaluator's ordering owner. Expectations precede this code.
use super::source_bgp_receiving::{order_input, order_term_json, session};
use crate::numeric::source_bgp_order::{SourceTripleOrderCell, source_reorder_triples};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spargebra::{SourceTripleRow, SourceTripleRowsError, source_group_triple_rows};

const FIXTURE: &str = include_str!("fixtures/source_triple_rows_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "53d15a47ffb1c58c269fee089288a230966b8d021733205d9b53ee54873a51cd"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(
        root["source_records"]["rows"],
        "2bcb2f21c9497fb0bb3b6f396a18bbf65dec2ee21893ba116c61a1dae56e4414"
    );
    assert_eq!(
        root["source_records"]["bgp_order"],
        "59643189443a2bb79f7b715de23df615badb3e952384f8ef51ea77794a813e45"
    );
    root
}
fn receive_error(error: SourceTripleRowsError, case: &Value) {
    let (module, class, argument) = error
        .source_exception()
        .expect("original exception, not resource policy");
    assert_eq!(
        json!({"module":module,"class_name":class,"args":[argument]}),
        case["error"],
        "{}",
        case["input"]["id"]
    );
    assert_eq!(case["status"], "original_error");
    assert_eq!(case["error_stage"], "algebra_triples");
    assert_eq!(case["original_boundary"], "algebra.triples");
}

#[test]
fn source_triple_rows_frozen_protocols() {
    let root = fixture();
    let cases = root["protocols"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    let mut errors = 0;
    for case in cases {
        let s = session();
        let rows: Vec<Vec<_>> = case["input"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row["terms"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|term| order_input(term, &s))
                    .collect()
            })
            .collect();
        let encode = || {
            rows.iter()
                .map(|row| {
                    row.iter()
                        .map(|term| order_term_json(term, &s))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let before = encode();
        assert_eq!(json!(before), case["constructed"]);
        let borrowed: Vec<_> = rows
            .iter()
            .zip(case["input"]["rows"].as_array().unwrap())
            .map(|(cells, row)| match row["kind"].as_str().unwrap() {
                "list" => SourceTripleRow::List(cells.as_slice()),
                "tuple" => SourceTripleRow::Tuple(cells.as_slice()),
                _ => panic!("unreceived row kind"),
            })
            .collect();
        let input_types: Vec<_> = borrowed
            .iter()
            .map(|row| match row {
                SourceTripleRow::List(_) => "builtins.list",
                SourceTripleRow::Tuple(_) => "builtins.tuple",
            })
            .collect();
        assert_eq!(json!(input_types), case["input_container_types"]);
        match source_group_triple_rows(&borrowed) {
            Ok(grouped) => {
                let order = source_reorder_triples(&s, &grouped).unwrap();
                let output: Vec<_> = order
                    .iter()
                    .map(|&at| {
                        grouped[at]
                            .iter()
                            .map(|term| order_term_json(term, &s))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                assert_eq!(case["status"], "observed_complete");
                assert_eq!(json!(output), case["ordered"], "{}", case["input"]["id"]);
                // The owner returns fixed arity triples, never original list rows.
                let output_types: Vec<_> = grouped
                    .iter()
                    .map(|_: &[SourceTripleOrderCell; 3]| "builtins.tuple")
                    .collect();
                assert_eq!(json!(output_types), case["container_types"]);
            }
            Err(error) => {
                errors += 1;
                receive_error(error, case);
            }
        }
        assert_eq!(before, encode());
        assert_eq!(case["input_rows_unchanged"], true);
        assert_eq!(s.flags(), semantica_numeric::DecimalSignals::default());
    }
    assert_eq!(errors, 7);
}

#[test]
fn source_triple_rows_original_bo13_length_error() {
    let root = fixture();
    let case = root["prior_protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["input"]["id"] == "BO13")
        .unwrap();
    let s = session();
    let mut cells: Vec<_> = case["input"]["triples"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|term| order_input(term, &s))
        .collect();
    let constructed: Vec<_> = cells.iter().map(|term| order_term_json(term, &s)).collect();
    assert_eq!(json!([constructed]), case["constructed"]);
    // The original observer appends this actual fourth term before calling
    // algebra.triples. A malformed individual row alone is not its criterion.
    cells.push(SourceTripleOrderCell::Variable(
        oxrdf::Variable::new("extra").unwrap(),
    ));
    let error = source_group_triple_rows(&[SourceTripleRow::List(&cells)]).unwrap_err();
    receive_error(error, case);
    assert_eq!(cells.len(), 4);
    assert_eq!(s.flags(), semantica_numeric::DecimalSignals::default());
}

#[test]
fn source_triple_rows_preserve_constructor_occurrence_across_boundary() {
    let s = session();
    let literal = json!({"kind":"Literal","value":"01","datatype":"http://www.w3.org/2001/XMLSchema#integer","language":null,"normalize":false});
    let first = vec![order_input(&literal, &s)];
    let second = vec![order_input(&literal, &s), order_input(&literal, &s)];
    let grouped = source_group_triple_rows(&[
        SourceTripleRow::List(&first),
        SourceTripleRow::List(&second),
    ])
    .unwrap();
    let original = [&first[0], &second[0], &second[1]];
    for (output, input) in grouped[0].iter().zip(original) {
        let (SourceTripleOrderCell::Rdf(output), SourceTripleOrderCell::Rdf(input)) =
            (output, input)
        else {
            panic!()
        };
        assert!(output.same_source_object(input));
    }
    let (SourceTripleOrderCell::Rdf(a), SourceTripleOrderCell::Rdf(b)) =
        (&grouped[0][0], &grouped[0][1])
    else {
        panic!()
    };
    assert!(
        !a.same_source_object(b),
        "equal terms must not merge constructor occurrences"
    );
}
