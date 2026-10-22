//! Source-derived Literal Python truth and constructor receiving.
//!
//! The oracle and this adapter share an author, independently source-reviewed;
//! production helpers have another owner. Expectations came from fixed source,
//! not candidate output. PB01/PB02 parser occurrence ownership remains separate.
use super::*;
use semantica_numeric::{DecimalValue, RoundingMode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/source_literal_truth_cases.json");

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "3603b045bbdaf2bbbedb48d1bf118688073f4553ce565a71d33cab635ce57380"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "e98fd369084f5b53b3a9f3a6498d7fc047e85024c4b0531964733fa36cdcdcce"
    );
    value
}
fn case(group: &str, id: &str) -> Value {
    fixture()["payload"][group]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap()
        .clone()
}
fn signal(name: &str) -> DecimalSignal {
    match name {
        "InvalidOperation" => DecimalSignal::InvalidOperation,
        "DivisionByZero" => DecimalSignal::DivisionByZero,
        "Overflow" => DecimalSignal::Overflow,
        _ => panic!("unreceived signal in fixture: {name}"),
    }
}
fn flags(session: &QueryNumericSession) -> Value {
    Value::Array(
        session
            .flags()
            .iter()
            .map(|s| json!(format!("{s:?}")))
            .collect(),
    )
}
fn session(case: &Value, source: bool) -> QueryNumericSession {
    let env = fixture()["payload"]["environment"].clone();
    let ctx = &env["decimal_context"];
    let traps = case
        .get("environment_overrides")
        .and_then(|v| v.get("decimal_traps"))
        .unwrap_or(&ctx["traps"]);
    let decimal = DecimalContext {
        precision: ctx["precision"].as_u64().unwrap() as usize,
        rounding: {
            assert_eq!(ctx["rounding"], "HALF_EVEN");
            RoundingMode::HalfEven
        },
        emin: BigInt::from(ctx["Emin"].as_i64().unwrap()),
        emax: BigInt::from(ctx["Emax"].as_i64().unwrap()),
        clamp: ctx["clamp"].as_u64().unwrap() != 0,
        traps: traps
            .as_array()
            .unwrap()
            .iter()
            .map(|v| signal(v.as_str().unwrap()))
            .collect(),
    };
    let limits = case
        .get("input")
        .and_then(|v| v.get("numeric_limits"))
        .unwrap_or(&env["numeric_limits"]);
    QueryNumericSession::new_with_literal_semantics(
        decimal,
        NumericLimits {
            max_input_characters: limits["max_input_characters"].as_u64().unwrap() as usize,
            max_coefficient_digits: limits["max_coefficient_digits"].as_u64().unwrap() as usize,
            max_output_characters: limits["max_output_characters"].as_u64().unwrap() as usize,
            max_materialized_power_of_ten: limits["max_materialized_power_of_ten"].as_u64().unwrap()
                as usize,
        },
        None,
        env["normalize_literals"].as_bool().unwrap(),
        QueryNumericCapabilities::MixedBinary64V1,
        QueryGroupSemantics::SourceAggregateV1,
        if source {
            QueryLiteralSemantics::SourceLiteralV1
        } else {
            QueryLiteralSemantics::RdfIdentityV1
        },
    )
    .unwrap()
}
fn source_literal(session: &QueryNumericSession, input: &Value) -> RawExpressionValue {
    let text = input["lexical"].as_str().unwrap();
    let datatype = input["datatype"]
        .as_str()
        .map(|s| NamedNode::new(s).unwrap());
    let rdf = match &datatype {
        Some(datatype) => Literal::new_typed_literal(text, datatype.clone()),
        None => Literal::new_simple_literal(text),
    };
    let origin = match input["origin"].as_str().unwrap() {
        "QueryPlain" => SourceLiteralOrigin::QueryPlain,
        "QueryTypedExistingLiteral" => SourceLiteralOrigin::QueryTypedExistingLiteral,
        "QueryBooleanToken" => SourceLiteralOrigin::QueryBooleanToken,
        other => panic!("unsupported input origin: {other}"),
    };
    session
        .construct_source_literal(&ParsedSourceLiteral {
            rdf,
            datatype: datatype.map_or(SourceDatatype::Absent, SourceDatatype::Specified),
            language_original: None,
            origin,
            occurrence: None,
        })
        .unwrap()
}
fn metadata(value: &RawExpressionValue) -> &SourceLiteralMetadata {
    let LiteralSourceState::Literal(metadata) = value.literal_source() else {
        panic!("literal facts missing")
    };
    metadata
}
fn actual_family(value: &RawExpressionValue) -> &'static str {
    match &metadata(value).cached_value_kind {
        SourceFact::Known(SourceLiteralCacheKind::String) => {
            assert!(matches!(value.numeric(), NumericCacheState::NotNumeric));
            "String"
        }
        SourceFact::Known(SourceLiteralCacheKind::None) => {
            assert!(matches!(value.numeric(), NumericCacheState::CastFailed));
            "None"
        }
        SourceFact::Known(SourceLiteralCacheKind::Boolean(_)) => {
            assert!(matches!(value.numeric(), NumericCacheState::NotNumeric));
            "Boolean"
        }
        SourceFact::Known(SourceLiteralCacheKind::Numeric) => {
            let NumericCacheState::Known(cache) = value.numeric() else {
                panic!("known numeric metadata without cache")
            };
            match cache.value() {
                NumericValue::Integer(_) => "Integer",
                NumericValue::Decimal(DecimalValue::Finite { .. }) => "DecimalFinite",
                NumericValue::Decimal(DecimalValue::NaN {
                    signaling: false, ..
                }) => "DecimalQuietNaN",
                NumericValue::Decimal(DecimalValue::NaN {
                    signaling: true, ..
                }) => "DecimalSignalingNaN",
                NumericValue::Decimal(DecimalValue::Infinity { .. }) => "DecimalInfinity",
                NumericValue::Binary64(v) if v.is_nan() => "Binary64NaN",
                NumericValue::Binary64(v) if v.is_infinite() => "Binary64Infinity",
                NumericValue::Binary64(_) => "Binary64",
            }
        }
        _ => panic!("unreceived cache family"),
    }
}
fn assert_same_transport(before: &RawExpressionValue, after: &RawExpressionValue) {
    assert_eq!(before.rdf().unwrap(), after.rdf().unwrap());
    assert_eq!(before.literal_source(), after.literal_source());
    assert!(before.same_source_object(after));
    if let (NumericCacheState::Known(a), NumericCacheState::Known(b)) =
        (before.numeric(), after.numeric())
    {
        assert!(std::sync::Arc::ptr_eq(a, b));
    }
}
fn run_truth(id: &str) {
    let c = case("truth_cases", id);
    let session = session(&c, true);
    let raw = source_literal(&session, &c["input"]);
    let before = raw.clone();
    assert_eq!(actual_family(&raw), c["expected"]["cached_family"]);
    assert_eq!(flags(&session), c["expected"]["flags_after_construction"]);
    let reads = c["expected"]["truth_reads"].as_u64().unwrap();
    assert_eq!(reads, 2);
    for _ in 0..reads {
        assert_eq!(
            session.source_literal_python_truth(&raw).unwrap(),
            c["expected"]["python_truth"]
        );
        assert_eq!(
            flags(&session),
            c["expected"]["flags_after_each_truth_read"]
        );
        assert_same_transport(&before, &raw);
    }
    assert_eq!(c["expected"]["lexical_and_source_metadata_unchanged"], true);
    assert_eq!(c["expected"]["selected_source_object_unchanged"], true);
}
fn run_boolean_result(id: &str) {
    let c = case("boolean_result_cases", id);
    let session = session(&c, true);
    let value = c["input"]["value"].as_bool().unwrap();
    let first = session.construct_source_boolean_result(value).unwrap();
    let second = session.construct_source_boolean_result(value).unwrap();
    let Term::Literal(rdf) = first.rdf().unwrap() else {
        panic!("Boolean result not literal")
    };
    assert_eq!(rdf.value(), c["expected"]["lexical"]);
    assert_eq!(
        metadata(&first),
        &SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(
                NamedNode::new(c["expected"]["source_datatype"]["iri"].as_str().unwrap()).unwrap()
            ),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(None),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Boolean(
                c["expected"]["cached_boolean"].as_bool().unwrap()
            )),
        }
    );
    assert!(c["expected"]["language_original"].is_null());
    assert!(c["expected"]["ill_typed"].is_null());
    assert_eq!(c["expected"]["numeric_cache"], "NotNumeric");
    assert!(matches!(first.numeric(), NumericCacheState::NotNumeric));
    assert!(!first.same_source_object(&second));
    assert_same_transport(&first, &first.clone());
    assert_eq!(
        c["expected"]["two_actual_constructions_have_distinct_source_objects"],
        true
    );
    assert_eq!(
        c["expected"]["clone_of_one_result_preserves_its_source_object"],
        true
    );
    assert_eq!(session.source_literal_python_truth(&first).unwrap(), value);
    assert_eq!(flags(&session), c["expected"]["flags_delta"]);
}
fn run_active(id: &str) {
    let c = case("active_contract_cases", id);
    let source = session(&c, true);
    match id {
        "TA01" | "TA02" => {
            let input = &c["input"];
            let raw = RawExpressionValue::from_import(
                Literal::new_typed_literal(
                    input["lexical"].as_str().unwrap(),
                    NamedNode::new(input["datatype"].as_str().unwrap()).unwrap(),
                )
                .into(),
            );
            assert_eq!(
                c["expected"]["semantic_error"],
                "unknown_source_literal_truth"
            );
            assert_eq!(
                source.source_literal_python_truth(&raw),
                Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source literal Python truth requires cached value facts"
                ))
            );
        }
        "TA03" => {
            let raw = source.construct_source_boolean_result(false).unwrap();
            let before = raw.clone();
            let legacy = session(&c, false);
            assert_eq!(
                c["expected"]["semantic_error"],
                "source_literal_capability_not_enabled"
            );
            assert_eq!(
                legacy.source_literal_python_truth(&raw),
                Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "SourceLiteralV1 is not enabled"
                ))
            );
            assert_same_transport(&raw, &before);
            assert_eq!(flags(&legacy), c["expected"]["flags_delta"]);
        }
        "TA04" => {
            let raw = RawExpressionValue::from_import(
                NamedNode::new(c["input"]["value"]["iri"].as_str().unwrap())
                    .unwrap()
                    .into(),
            );
            assert_eq!(
                c["expected"]["semantic_error"],
                "source_literal_truth_requires_literal"
            );
            assert_eq!(
                source.source_literal_python_truth(&raw),
                Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source literal Python truth requires a Literal"
                ))
            );
        }
        "TA05" => {
            assert_eq!(
                c["input"]["operations"],
                json!([{ "source_boolean_result": true }, { "source_boolean_result": false }])
            );
            let first = source.construct_source_boolean_result(true).unwrap();
            let Term::Literal(rdf) = first.rdf().unwrap() else {
                panic!("Boolean result not literal")
            };
            assert_eq!(rdf.value(), c["expected"]["results"][0]["lexical"]);
            assert!(matches!(
                source.construct_source_boolean_result(false),
                Err(RawNumericFault::Core(NumericFault::ResourceLimit {
                    operation: "output_characters"
                }))
            ));
            assert_eq!(
                c["expected"]["results"][1]["semantic_error"],
                "output_characters_resource_limit"
            );
        }
        _ => panic!("unreceived active case: {id}"),
    }
    assert_eq!(flags(&source), c["expected"]["flags_delta"]);
}

#[test]
fn source_literal_truth_fixture_identity() {
    let f = fixture();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&f["payload"])).unwrap())
        ),
        f["payload_sha256"]
    );
    assert_eq!(f["payload"]["truth_cases"].as_array().unwrap().len(), 23);
    assert_eq!(
        f["payload"]["boolean_result_cases"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        f["payload"]["active_contract_cases"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        f["excluded_pending_parser_controls"],
        json!(["PB01", "PB02"])
    );
}

#[test]
fn source_literal_truth_st01() {
    run_truth("ST01");
}

#[test]
fn source_literal_truth_st02() {
    run_truth("ST02");
}

#[test]
fn source_literal_truth_st03() {
    run_truth("ST03");
}

#[test]
fn source_literal_truth_st04() {
    run_truth("ST04");
}

#[test]
fn source_literal_truth_st05() {
    run_truth("ST05");
}

#[test]
fn source_literal_truth_st06() {
    run_truth("ST06");
}

#[test]
fn source_literal_truth_st07() {
    run_truth("ST07");
}

#[test]
fn source_literal_truth_st08() {
    run_truth("ST08");
}

#[test]
fn source_literal_truth_st09() {
    run_truth("ST09");
}

#[test]
fn source_literal_truth_st10() {
    run_truth("ST10");
}

#[test]
fn source_literal_truth_st11() {
    run_truth("ST11");
}

#[test]
fn source_literal_truth_st12() {
    run_truth("ST12");
}

#[test]
fn source_literal_truth_st13() {
    run_truth("ST13");
}

#[test]
fn source_literal_truth_st14() {
    run_truth("ST14");
}

#[test]
fn source_literal_truth_st15() {
    run_truth("ST15");
}

#[test]
fn source_literal_truth_st16() {
    run_truth("ST16");
}

#[test]
fn source_literal_truth_st17() {
    run_truth("ST17");
}

#[test]
fn source_literal_truth_st18() {
    run_truth("ST18");
}

#[test]
fn source_literal_truth_st19() {
    run_truth("ST19");
}

#[test]
fn source_literal_truth_st20() {
    run_truth("ST20");
}

#[test]
fn source_literal_truth_st21() {
    run_truth("ST21");
}

#[test]
fn source_literal_truth_st22() {
    run_truth("ST22");
}

#[test]
fn source_literal_truth_st23() {
    run_truth("ST23");
}

#[test]
fn source_literal_truth_br01() {
    run_boolean_result("BR01");
}

#[test]
fn source_literal_truth_br02() {
    run_boolean_result("BR02");
}

#[test]
fn source_literal_truth_ta01() {
    run_active("TA01");
}

#[test]
fn source_literal_truth_ta02() {
    run_active("TA02");
}

#[test]
fn source_literal_truth_ta03() {
    run_active("TA03");
}

#[test]
fn source_literal_truth_ta04() {
    run_active("TA04");
}

#[test]
fn source_literal_truth_ta05() {
    run_active("TA05");
}

// Actual token factory receiving, distinct from pending parser callbacks.
const TOKEN_FIXTURE: &str = include_str!("fixtures/source_literal_numeric_token_cases.json");
fn token_fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(TOKEN_FIXTURE.as_bytes())),
        "5c65bf4b248a4741dd6cde5706d698a9a16275f1582bfc23587b35ad708ddd96"
    );
    let value: Value = serde_json::from_str(TOKEN_FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "e6aed3f57c5d36cf45637813556866f9198304572840797a71bc92d5e84db994"
    );
    value
}
fn token_parts(input: &Value) -> (NumericTokenKind, NumericTokenSign, &str) {
    let kind = match input["kind"].as_str().unwrap() {
        "Integer" => NumericTokenKind::Integer,
        "Decimal" => NumericTokenKind::Decimal,
        "Double" => NumericTokenKind::Double,
        _ => panic!("unreceived numeric token kind"),
    };
    let sign = match input["sign"].as_str().unwrap() {
        "Unsigned" => NumericTokenSign::Unsigned,
        "Positive" => NumericTokenSign::Positive,
        "Negative" => NumericTokenSign::Negative,
        _ => panic!("unreceived numeric token sign"),
    };
    (kind, sign, input["magnitude"].as_str().unwrap())
}
fn numeric_cache_value(raw: &RawExpressionValue) -> Value {
    let NumericCacheState::Known(cache) = raw.numeric() else {
        panic!("actual token requires known numeric cache")
    };
    assert_eq!(cache.origin(), NumericValueOrigin::QueryNumericToken);
    match cache.value() {
        NumericValue::Integer(n) => json!({"Integer": n.to_string()}),
        NumericValue::Decimal(DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        }) => {
            json!({"Decimal": {"negative": negative, "coefficient": coefficient.to_string(), "exponent": exponent.to_string()}})
        }
        NumericValue::Binary64(n) => json!({"Binary64Bits": format!("{:016x}", n.to_bits())}),
        _ => panic!("unreceived token cache family"),
    }
}
fn run_token(id: &str) {
    let f = token_fixture();
    let c = f["payload"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap();
    assert_eq!(
        f["payload"]["environment"],
        fixture()["payload"]["environment"]
    );
    let session = session(c, true);
    let (kind, sign, magnitude) = token_parts(&c["input"]);
    let result = session.construct_source_numeric_token(kind, sign, magnitude);
    if c["expected"].get("semantic_error").is_some() {
        assert_eq!(
            c["expected"]["semantic_error"],
            "original_numeric_token_literal_type_error"
        );
        assert!(matches!(result, Err(RawNumericFault::ParserLiteralType)));
        assert_eq!(flags(&session), c["expected"]["flags_after_failure"]);
        return;
    }
    let raw = result.unwrap();
    let before = raw.clone();
    let expected = &c["expected"];
    let Term::Literal(rdf) = raw.rdf().unwrap() else {
        panic!("token is not literal")
    };
    assert_eq!(rdf.value(), expected["lexical"]);
    assert_eq!(rdf.datatype().as_str(), expected["datatype"]);
    assert_eq!(
        metadata(&raw),
        &SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(
                NamedNode::new(expected["datatype"].as_str().unwrap()).unwrap()
            ),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(expected["ill_typed"].as_bool()),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
        }
    );
    assert!(expected["language_original"].is_null());
    assert_eq!(numeric_cache_value(&raw), expected["numeric_cache"]);
    assert_eq!(expected["cache_origin"], "QueryNumericToken");
    assert_eq!(flags(&session), expected["flags_after_construction"]);
    for _ in 0..2 {
        assert_eq!(
            session.source_literal_python_truth(&raw).unwrap(),
            expected["python_truth"]
        );
        assert_same_transport(&before, &raw);
        assert_eq!(flags(&session), expected["flags_after_two_truth_reads"]);
    }
    let second = session
        .construct_source_numeric_token(kind, sign, magnitude)
        .unwrap();
    assert!(!raw.same_source_object(&second));
    assert_eq!(expected["transport_clone_preserves_source_object"], true);
    assert_eq!(expected["second_actual_construction_is_new_object"], true);
}
#[test]
fn source_literal_truth_numeric_token_fixture_identity() {
    let f = token_fixture();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&f["payload"])).unwrap())
        ),
        f["payload_sha256"]
    );
    assert_eq!(f["payload"]["cases"].as_array().unwrap().len(), 9);
}

const ERROR_FIXTURE: &str = include_str!("fixtures/source_literal_error_truth_cases.json");
fn error_fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(ERROR_FIXTURE.as_bytes())),
        "32da0b42e713a605304ac6d651d50423ed42e699e473773cd60c1be1a7ad8d01"
    );
    let value: Value = serde_json::from_str(ERROR_FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "c407fab01a3232abd195842ee6bc5707232d574bdf03e6b26ffd7b3b9119f502"
    );
    value
}
#[test]
fn source_literal_truth_error_fixture_identity() {
    let f = error_fixture();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&f["payload"])).unwrap())
        ),
        f["payload_sha256"]
    );
    assert_eq!(f["payload"]["cases"].as_array().unwrap().len(), 1);
}
#[test]
fn source_literal_truth_et01() {
    let f = error_fixture();
    let c = &f["payload"]["cases"][0];
    assert_eq!(c["id"], "ET01");
    assert_eq!(
        f["payload"]["environment"],
        fixture()["payload"]["environment"]
    );
    let session = session(c, true);
    assert_eq!(c["input"]["source_exception"]["class"], "SPARQLError");
    let error = ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        "adapter diagnostic is deliberately different from source text",
        SourceException {
            class: SourceExceptionClass::Sparql,
            argument: SourceExceptionArgument::Text(
                c["input"]["source_exception"]["argument"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ),
        },
    );
    let raw = session
        .construct_source_extremum_result(&SolutionValue::ReturnedError(error.clone()))
        .unwrap()
        .unwrap();
    let before = raw.clone();
    let Term::Literal(rdf) = raw.rdf().unwrap() else {
        panic!("Literal(exception) did not make literal")
    };
    assert_eq!(rdf.value(), c["expected"]["lexical"]);
    assert_eq!(
        metadata(&raw),
        &SourceLiteralMetadata {
            datatype: SourceDatatype::Absent,
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(None),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::ErrorObject),
        }
    );
    assert_eq!(c["expected"]["source_datatype"]["state"], "absent");
    assert!(c["expected"]["language_original"].is_null());
    assert!(c["expected"]["ill_typed"].is_null());
    assert_eq!(c["expected"]["cached_family"], "ErrorObject");
    assert_eq!(flags(&session), c["expected"]["flags_after_construction"]);
    for _ in 0..c["expected"]["truth_reads"].as_u64().unwrap() {
        assert_eq!(
            session.source_literal_python_truth(&raw).unwrap(),
            c["expected"]["python_truth"]
        );
        let NumericCacheState::ErrorObject(cache) = raw.numeric() else {
            panic!("source exception cache was replaced")
        };
        assert_eq!(cache, &error); // ErrorValue equality is the actual Arc identity.
        assert_same_transport(&before, &raw);
        assert_eq!(
            flags(&session),
            c["expected"]["flags_after_each_truth_read"]
        );
    }
    assert_eq!(c["expected"]["same_original_exception_instance"], true);
    assert_eq!(c["expected"]["literal_source_object_unchanged"], true);
}

#[test]
fn source_literal_truth_nt01() {
    run_token("NT01");
}

#[test]
fn source_literal_truth_nt02() {
    run_token("NT02");
}

#[test]
fn source_literal_truth_nt03() {
    run_token("NT03");
}

#[test]
fn source_literal_truth_nt04() {
    run_token("NT04");
}

#[test]
fn source_literal_truth_nt05() {
    run_token("NT05");
}

#[test]
fn source_literal_truth_nt06() {
    run_token("NT06");
}

#[test]
fn source_literal_truth_nt07() {
    run_token("NT07");
}

#[test]
fn source_literal_truth_nt08() {
    run_token("NT08");
}

#[test]
fn source_literal_truth_nt09() {
    run_token("NT09");
}
