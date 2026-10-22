//! Immutable original identity observations, including explicit constructor input replay.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn receive(id: &str) {
    let (bytes, digest) = match &id[..2] {
        "CI" => (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_constructor_identity_repeats.json"
            ),
            "5fcda18c388765cb302201cc98e4d1102ff88a9d295a61dd36035b7775b95a6c",
        ),
        "DI" => (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_constructor_identity_datatype.json"
            ),
            "4c8470d73bc435ae9e443cacd4ee74d848959f3032f9e0c678d7763eddf9da79",
        ),
        "DJ" => (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_constructor_identity_supplement.json"
            ),
            "ea5172d389be123635a7989b309f1e4693acdc7d8168c3c7627b3b3de1762ffa",
        ),
        _ => panic!("undeclared original identity input"),
    };
    assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), digest);
    let root: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    if id.starts_with("CI") {
        source_syntax_protocol_receiving::receive_case(case, &root["data_inputs"]);
    } else {
        source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
    }
}
#[test]
fn source_constructor_identity_ci01() {
    receive("CI01");
}
#[test]
fn source_constructor_identity_ci02() {
    receive("CI02");
}
#[test]
fn source_constructor_identity_ci03() {
    receive("CI03");
}
#[test]
fn source_constructor_identity_ci04() {
    receive("CI04");
}
#[test]
fn source_constructor_identity_di001() {
    receive("DI001");
}
#[test]
fn source_constructor_identity_di002() {
    receive("DI002");
}
#[test]
fn source_constructor_identity_di003() {
    receive("DI003");
}
#[test]
fn source_constructor_identity_di004() {
    receive("DI004");
}
#[test]
fn source_constructor_identity_di005() {
    receive("DI005");
}
#[test]
fn source_constructor_identity_di006() {
    receive("DI006");
}
#[test]
fn source_constructor_identity_di007() {
    receive("DI007");
}
#[test]
fn source_constructor_identity_di008() {
    receive("DI008");
}
#[test]
fn source_constructor_identity_di009() {
    receive("DI009");
}
#[test]
fn source_constructor_identity_di010() {
    receive("DI010");
}
#[test]
fn source_constructor_identity_di011() {
    receive("DI011");
}
#[test]
fn source_constructor_identity_di012() {
    receive("DI012");
}
#[test]
fn source_constructor_identity_di013() {
    receive("DI013");
}
#[test]
fn source_constructor_identity_di014() {
    receive("DI014");
}
#[test]
fn source_constructor_identity_di015() {
    receive("DI015");
}
#[test]
fn source_constructor_identity_di016() {
    receive("DI016");
}
#[test]
fn source_constructor_identity_di017() {
    receive("DI017");
}
#[test]
fn source_constructor_identity_di018() {
    receive("DI018");
}
#[test]
fn source_constructor_identity_di019() {
    receive("DI019");
}
#[test]
fn source_constructor_identity_di020() {
    receive("DI020");
}
#[test]
fn source_constructor_identity_di021() {
    receive("DI021");
}
#[test]
fn source_constructor_identity_di022() {
    receive("DI022");
}
#[test]
fn source_constructor_identity_di023() {
    receive("DI023");
}
#[test]
fn source_constructor_identity_di024() {
    receive("DI024");
}
#[test]
fn source_constructor_identity_di025() {
    receive("DI025");
}
#[test]
fn source_constructor_identity_di026() {
    receive("DI026");
}
#[test]
fn source_constructor_identity_di027() {
    receive("DI027");
}
#[test]
fn source_constructor_identity_di028() {
    receive("DI028");
}
#[test]
fn source_constructor_identity_di029() {
    receive("DI029");
}
#[test]
fn source_constructor_identity_di030() {
    receive("DI030");
}
#[test]
fn source_constructor_identity_di031() {
    receive("DI031");
}
#[test]
fn source_constructor_identity_di032() {
    receive("DI032");
}
#[test]
fn source_constructor_identity_di033() {
    receive("DI033");
}
#[test]
fn source_constructor_identity_di034() {
    receive("DI034");
}
#[test]
fn source_constructor_identity_di035() {
    receive("DI035");
}
#[test]
fn source_constructor_identity_di036() {
    receive("DI036");
}
#[test]
fn source_constructor_identity_di037() {
    receive("DI037");
}
#[test]
fn source_constructor_identity_di038() {
    receive("DI038");
}
#[test]
fn source_constructor_identity_di039() {
    receive("DI039");
}
#[test]
fn source_constructor_identity_dj001() {
    receive("DJ001");
}
#[test]
fn source_constructor_identity_dj002() {
    receive("DJ002");
}
#[test]
fn source_constructor_identity_dj003() {
    receive("DJ003");
}
#[test]
fn source_constructor_identity_dj004() {
    receive("DJ004");
}
#[test]
fn source_constructor_identity_dj005() {
    receive("DJ005");
}
#[test]
fn source_constructor_identity_dj006() {
    receive("DJ006");
}

#[test]
fn source_constructor_identity_unknown_origin() {
    let session = source_bgp_receiving::session();
    let raw = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("1", oxrdf::vocab::xsd::INTEGER).into(),
    );
    assert!(session.source_datatype(&SolutionValue::Rdf(raw)).is_err());
}

#[test]
fn source_constructor_identity_copy_and_detached() {
    let session = source_bgp_receiving::session();
    let original = session
        .construct_source_direct_string(
            "01",
            Some(oxrdf::vocab::xsd::INTEGER.into_owned()),
            None,
            false,
        )
        .unwrap();
    let copy = session.copy_source_literal(&original).unwrap();
    assert!(!original.same_source_object(&copy));
    let datatype = session
        .source_datatype(&SolutionValue::Rdf(original.clone()))
        .unwrap();
    let copied_datatype = session.source_datatype(&SolutionValue::Rdf(copy)).unwrap();
    assert!(datatype.same_source_object(&copied_datatype));
    let (rdf, cache, metadata) = original.into_source_parts().unwrap();
    let detached = RawExpressionValue::from_source_parts(rdf, cache, metadata).unwrap();
    assert!(
        session
            .source_datatype(&SolutionValue::Rdf(detached))
            .is_err()
    );
}

#[test]
fn source_constructor_identity_namespace_and_distinct() {
    let session = source_bgp_receiving::session();
    let datatype = |value| session.source_datatype(&SolutionValue::Rdf(value)).unwrap();
    let plain = || {
        session
            .construct_source_direct_string("x", None, None, false)
            .unwrap()
    };
    let typed = || {
        session
            .construct_source_direct_string(
                "x",
                Some(oxrdf::vocab::xsd::STRING.into_owned()),
                None,
                false,
            )
            .unwrap()
    };
    let first = datatype(typed());
    let second = datatype(typed());
    let fallback = datatype(plain());
    assert_eq!(first.rdf().unwrap(), second.rdf().unwrap());
    assert_eq!(first.rdf().unwrap(), fallback.rdf().unwrap());
    assert!(!first.same_source_object(&second));
    assert!(!first.same_source_object(&fallback));
    assert!(fallback.same_source_object(&datatype(plain())));
}
