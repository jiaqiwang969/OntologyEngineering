//! Fixed original direct-initial constructors and their actual query consumers.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixtures() -> [(&'static str, &'static str); 4] {
    [
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_01.json"
            ),
            "cc7ce99509909f32b1d867f338265c8b284caedc0e89593e37a59e29678fc2e2",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_02.json"
            ),
            "3a77f9bd649a2db5d846ec1c37393918d4b6a4dfc874b819ed5f6da1efd954c8",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_03.json"
            ),
            "aa33550602abfc48524eb3f275261a074681224e854bf919b6ec45ab6a05beb3",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_04.json"
            ),
            "55c40545acc91207904e59c55eeff027ad383f083d557947589d496310d1f13c",
        ),
    ]
}
fn receive(id: &str) {
    let index: usize = id[2..].parse().unwrap();
    let (bytes, digest) = private_fixtures()[(index - 1) / 96];
    assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), digest);
    let root: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 96);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}
#[test]
fn source_initial_value_iv001() {
    receive("IV001");
}
#[test]
fn source_initial_value_iv002() {
    receive("IV002");
}
#[test]
fn source_initial_value_iv003() {
    receive("IV003");
}
#[test]
fn source_initial_value_iv004() {
    receive("IV004");
}
#[test]
fn source_initial_value_iv005() {
    receive("IV005");
}
#[test]
fn source_initial_value_iv006() {
    receive("IV006");
}
#[test]
fn source_initial_value_iv007() {
    receive("IV007");
}
#[test]
fn source_initial_value_iv008() {
    receive("IV008");
}
#[test]
fn source_initial_value_iv009() {
    receive("IV009");
}
#[test]
fn source_initial_value_iv010() {
    receive("IV010");
}
#[test]
fn source_initial_value_iv011() {
    receive("IV011");
}
#[test]
fn source_initial_value_iv012() {
    receive("IV012");
}
#[test]
fn source_initial_value_iv013() {
    receive("IV013");
}
#[test]
fn source_initial_value_iv014() {
    receive("IV014");
}
#[test]
fn source_initial_value_iv015() {
    receive("IV015");
}
#[test]
fn source_initial_value_iv016() {
    receive("IV016");
}
#[test]
fn source_initial_value_iv017() {
    receive("IV017");
}
#[test]
fn source_initial_value_iv018() {
    receive("IV018");
}
#[test]
fn source_initial_value_iv019() {
    receive("IV019");
}
#[test]
fn source_initial_value_iv020() {
    receive("IV020");
}
#[test]
fn source_initial_value_iv021() {
    receive("IV021");
}
#[test]
fn source_initial_value_iv022() {
    receive("IV022");
}
#[test]
fn source_initial_value_iv023() {
    receive("IV023");
}
#[test]
fn source_initial_value_iv024() {
    receive("IV024");
}
#[test]
fn source_initial_value_iv025() {
    receive("IV025");
}
#[test]
fn source_initial_value_iv026() {
    receive("IV026");
}
#[test]
fn source_initial_value_iv027() {
    receive("IV027");
}
#[test]
fn source_initial_value_iv028() {
    receive("IV028");
}
#[test]
fn source_initial_value_iv029() {
    receive("IV029");
}
#[test]
fn source_initial_value_iv030() {
    receive("IV030");
}
#[test]
fn source_initial_value_iv031() {
    receive("IV031");
}
#[test]
fn source_initial_value_iv032() {
    receive("IV032");
}
#[test]
fn source_initial_value_iv033() {
    receive("IV033");
}
#[test]
fn source_initial_value_iv034() {
    receive("IV034");
}
#[test]
fn source_initial_value_iv035() {
    receive("IV035");
}
#[test]
fn source_initial_value_iv036() {
    receive("IV036");
}
#[test]
fn source_initial_value_iv037() {
    receive("IV037");
}
#[test]
fn source_initial_value_iv038() {
    receive("IV038");
}
#[test]
fn source_initial_value_iv039() {
    receive("IV039");
}
#[test]
fn source_initial_value_iv040() {
    receive("IV040");
}
#[test]
fn source_initial_value_iv041() {
    receive("IV041");
}
#[test]
fn source_initial_value_iv042() {
    receive("IV042");
}
#[test]
fn source_initial_value_iv043() {
    receive("IV043");
}
#[test]
fn source_initial_value_iv044() {
    receive("IV044");
}
#[test]
fn source_initial_value_iv045() {
    receive("IV045");
}
#[test]
fn source_initial_value_iv046() {
    receive("IV046");
}
#[test]
fn source_initial_value_iv047() {
    receive("IV047");
}
#[test]
fn source_initial_value_iv048() {
    receive("IV048");
}
#[test]
fn source_initial_value_iv049() {
    receive("IV049");
}
#[test]
fn source_initial_value_iv050() {
    receive("IV050");
}
#[test]
fn source_initial_value_iv051() {
    receive("IV051");
}
#[test]
fn source_initial_value_iv052() {
    receive("IV052");
}
#[test]
fn source_initial_value_iv053() {
    receive("IV053");
}
#[test]
fn source_initial_value_iv054() {
    receive("IV054");
}
#[test]
fn source_initial_value_iv055() {
    receive("IV055");
}
#[test]
fn source_initial_value_iv056() {
    receive("IV056");
}
#[test]
fn source_initial_value_iv057() {
    receive("IV057");
}
#[test]
fn source_initial_value_iv058() {
    receive("IV058");
}
#[test]
fn source_initial_value_iv059() {
    receive("IV059");
}
#[test]
fn source_initial_value_iv060() {
    receive("IV060");
}
#[test]
fn source_initial_value_iv061() {
    receive("IV061");
}
#[test]
fn source_initial_value_iv062() {
    receive("IV062");
}
#[test]
fn source_initial_value_iv063() {
    receive("IV063");
}
#[test]
fn source_initial_value_iv064() {
    receive("IV064");
}
#[test]
fn source_initial_value_iv065() {
    receive("IV065");
}
#[test]
fn source_initial_value_iv066() {
    receive("IV066");
}
#[test]
fn source_initial_value_iv067() {
    receive("IV067");
}
#[test]
fn source_initial_value_iv068() {
    receive("IV068");
}
#[test]
fn source_initial_value_iv069() {
    receive("IV069");
}
#[test]
fn source_initial_value_iv070() {
    receive("IV070");
}
#[test]
fn source_initial_value_iv071() {
    receive("IV071");
}
#[test]
fn source_initial_value_iv072() {
    receive("IV072");
}
#[test]
fn source_initial_value_iv073() {
    receive("IV073");
}
#[test]
fn source_initial_value_iv074() {
    receive("IV074");
}
#[test]
fn source_initial_value_iv075() {
    receive("IV075");
}
#[test]
fn source_initial_value_iv076() {
    receive("IV076");
}
#[test]
fn source_initial_value_iv077() {
    receive("IV077");
}
#[test]
fn source_initial_value_iv078() {
    receive("IV078");
}
#[test]
fn source_initial_value_iv079() {
    receive("IV079");
}
#[test]
fn source_initial_value_iv080() {
    receive("IV080");
}
#[test]
fn source_initial_value_iv081() {
    receive("IV081");
}
#[test]
fn source_initial_value_iv082() {
    receive("IV082");
}
#[test]
fn source_initial_value_iv083() {
    receive("IV083");
}
#[test]
fn source_initial_value_iv084() {
    receive("IV084");
}
#[test]
fn source_initial_value_iv085() {
    receive("IV085");
}
#[test]
fn source_initial_value_iv086() {
    receive("IV086");
}
#[test]
fn source_initial_value_iv087() {
    receive("IV087");
}
#[test]
fn source_initial_value_iv088() {
    receive("IV088");
}
#[test]
fn source_initial_value_iv089() {
    receive("IV089");
}
#[test]
fn source_initial_value_iv090() {
    receive("IV090");
}
#[test]
fn source_initial_value_iv091() {
    receive("IV091");
}
#[test]
fn source_initial_value_iv092() {
    receive("IV092");
}
#[test]
fn source_initial_value_iv093() {
    receive("IV093");
}
#[test]
fn source_initial_value_iv094() {
    receive("IV094");
}
#[test]
fn source_initial_value_iv095() {
    receive("IV095");
}
#[test]
fn source_initial_value_iv096() {
    receive("IV096");
}
#[test]
fn source_initial_value_iv097() {
    receive("IV097");
}
#[test]
fn source_initial_value_iv098() {
    receive("IV098");
}
#[test]
fn source_initial_value_iv099() {
    receive("IV099");
}
#[test]
fn source_initial_value_iv100() {
    receive("IV100");
}
#[test]
fn source_initial_value_iv101() {
    receive("IV101");
}
#[test]
fn source_initial_value_iv102() {
    receive("IV102");
}
#[test]
fn source_initial_value_iv103() {
    receive("IV103");
}
#[test]
fn source_initial_value_iv104() {
    receive("IV104");
}
#[test]
fn source_initial_value_iv105() {
    receive("IV105");
}
#[test]
fn source_initial_value_iv106() {
    receive("IV106");
}
#[test]
fn source_initial_value_iv107() {
    receive("IV107");
}
#[test]
fn source_initial_value_iv108() {
    receive("IV108");
}
#[test]
fn source_initial_value_iv109() {
    receive("IV109");
}
#[test]
fn source_initial_value_iv110() {
    receive("IV110");
}
#[test]
fn source_initial_value_iv111() {
    receive("IV111");
}
#[test]
fn source_initial_value_iv112() {
    receive("IV112");
}
#[test]
fn source_initial_value_iv113() {
    receive("IV113");
}
#[test]
fn source_initial_value_iv114() {
    receive("IV114");
}
#[test]
fn source_initial_value_iv115() {
    receive("IV115");
}
#[test]
fn source_initial_value_iv116() {
    receive("IV116");
}
#[test]
fn source_initial_value_iv117() {
    receive("IV117");
}
#[test]
fn source_initial_value_iv118() {
    receive("IV118");
}
#[test]
fn source_initial_value_iv119() {
    receive("IV119");
}
#[test]
fn source_initial_value_iv120() {
    receive("IV120");
}
#[test]
fn source_initial_value_iv121() {
    receive("IV121");
}
#[test]
fn source_initial_value_iv122() {
    receive("IV122");
}
#[test]
fn source_initial_value_iv123() {
    receive("IV123");
}
#[test]
fn source_initial_value_iv124() {
    receive("IV124");
}
#[test]
fn source_initial_value_iv125() {
    receive("IV125");
}
#[test]
fn source_initial_value_iv126() {
    receive("IV126");
}
#[test]
fn source_initial_value_iv127() {
    receive("IV127");
}
#[test]
fn source_initial_value_iv128() {
    receive("IV128");
}
#[test]
fn source_initial_value_iv129() {
    receive("IV129");
}
#[test]
fn source_initial_value_iv130() {
    receive("IV130");
}
#[test]
fn source_initial_value_iv131() {
    receive("IV131");
}
#[test]
fn source_initial_value_iv132() {
    receive("IV132");
}
#[test]
fn source_initial_value_iv133() {
    receive("IV133");
}
#[test]
fn source_initial_value_iv134() {
    receive("IV134");
}
#[test]
fn source_initial_value_iv135() {
    receive("IV135");
}
#[test]
fn source_initial_value_iv136() {
    receive("IV136");
}
#[test]
fn source_initial_value_iv137() {
    receive("IV137");
}
#[test]
fn source_initial_value_iv138() {
    receive("IV138");
}
#[test]
fn source_initial_value_iv139() {
    receive("IV139");
}
#[test]
fn source_initial_value_iv140() {
    receive("IV140");
}
#[test]
fn source_initial_value_iv141() {
    receive("IV141");
}
#[test]
fn source_initial_value_iv142() {
    receive("IV142");
}
#[test]
fn source_initial_value_iv143() {
    receive("IV143");
}
#[test]
fn source_initial_value_iv144() {
    receive("IV144");
}
#[test]
fn source_initial_value_iv145() {
    receive("IV145");
}
#[test]
fn source_initial_value_iv146() {
    receive("IV146");
}
#[test]
fn source_initial_value_iv147() {
    receive("IV147");
}
#[test]
fn source_initial_value_iv148() {
    receive("IV148");
}
#[test]
fn source_initial_value_iv149() {
    receive("IV149");
}
#[test]
fn source_initial_value_iv150() {
    receive("IV150");
}
#[test]
fn source_initial_value_iv151() {
    receive("IV151");
}
#[test]
fn source_initial_value_iv152() {
    receive("IV152");
}
#[test]
fn source_initial_value_iv153() {
    receive("IV153");
}
#[test]
fn source_initial_value_iv154() {
    receive("IV154");
}
#[test]
fn source_initial_value_iv155() {
    receive("IV155");
}
#[test]
fn source_initial_value_iv156() {
    receive("IV156");
}
#[test]
fn source_initial_value_iv157() {
    receive("IV157");
}
#[test]
fn source_initial_value_iv158() {
    receive("IV158");
}
#[test]
fn source_initial_value_iv159() {
    receive("IV159");
}
#[test]
fn source_initial_value_iv160() {
    receive("IV160");
}
#[test]
fn source_initial_value_iv161() {
    receive("IV161");
}
#[test]
fn source_initial_value_iv162() {
    receive("IV162");
}
#[test]
fn source_initial_value_iv163() {
    receive("IV163");
}
#[test]
fn source_initial_value_iv164() {
    receive("IV164");
}
#[test]
fn source_initial_value_iv165() {
    receive("IV165");
}
#[test]
fn source_initial_value_iv166() {
    receive("IV166");
}
#[test]
fn source_initial_value_iv167() {
    receive("IV167");
}
#[test]
fn source_initial_value_iv168() {
    receive("IV168");
}
#[test]
fn source_initial_value_iv169() {
    receive("IV169");
}
#[test]
fn source_initial_value_iv170() {
    receive("IV170");
}
#[test]
fn source_initial_value_iv171() {
    receive("IV171");
}
#[test]
fn source_initial_value_iv172() {
    receive("IV172");
}
#[test]
fn source_initial_value_iv173() {
    receive("IV173");
}
#[test]
fn source_initial_value_iv174() {
    receive("IV174");
}
#[test]
fn source_initial_value_iv175() {
    receive("IV175");
}
#[test]
fn source_initial_value_iv176() {
    receive("IV176");
}
#[test]
fn source_initial_value_iv177() {
    receive("IV177");
}
#[test]
fn source_initial_value_iv178() {
    receive("IV178");
}
#[test]
fn source_initial_value_iv179() {
    receive("IV179");
}
#[test]
fn source_initial_value_iv180() {
    receive("IV180");
}
#[test]
fn source_initial_value_iv181() {
    receive("IV181");
}
#[test]
fn source_initial_value_iv182() {
    receive("IV182");
}
#[test]
fn source_initial_value_iv183() {
    receive("IV183");
}
#[test]
fn source_initial_value_iv184() {
    receive("IV184");
}
#[test]
fn source_initial_value_iv185() {
    receive("IV185");
}
#[test]
fn source_initial_value_iv186() {
    receive("IV186");
}
#[test]
fn source_initial_value_iv187() {
    receive("IV187");
}
#[test]
fn source_initial_value_iv188() {
    receive("IV188");
}
#[test]
fn source_initial_value_iv189() {
    receive("IV189");
}
#[test]
fn source_initial_value_iv190() {
    receive("IV190");
}
#[test]
fn source_initial_value_iv191() {
    receive("IV191");
}
#[test]
fn source_initial_value_iv192() {
    receive("IV192");
}
#[test]
fn source_initial_value_iv193() {
    receive("IV193");
}
#[test]
fn source_initial_value_iv194() {
    receive("IV194");
}
#[test]
fn source_initial_value_iv195() {
    receive("IV195");
}
#[test]
fn source_initial_value_iv196() {
    receive("IV196");
}
#[test]
fn source_initial_value_iv197() {
    receive("IV197");
}
#[test]
fn source_initial_value_iv198() {
    receive("IV198");
}
#[test]
fn source_initial_value_iv199() {
    receive("IV199");
}
#[test]
fn source_initial_value_iv200() {
    receive("IV200");
}
#[test]
fn source_initial_value_iv201() {
    receive("IV201");
}
#[test]
fn source_initial_value_iv202() {
    receive("IV202");
}
#[test]
fn source_initial_value_iv203() {
    receive("IV203");
}
#[test]
fn source_initial_value_iv204() {
    receive("IV204");
}
#[test]
fn source_initial_value_iv205() {
    receive("IV205");
}
#[test]
fn source_initial_value_iv206() {
    receive("IV206");
}
#[test]
fn source_initial_value_iv207() {
    receive("IV207");
}
#[test]
fn source_initial_value_iv208() {
    receive("IV208");
}
#[test]
fn source_initial_value_iv209() {
    receive("IV209");
}
#[test]
fn source_initial_value_iv210() {
    receive("IV210");
}
#[test]
fn source_initial_value_iv211() {
    receive("IV211");
}
#[test]
fn source_initial_value_iv212() {
    receive("IV212");
}
#[test]
fn source_initial_value_iv213() {
    receive("IV213");
}
#[test]
fn source_initial_value_iv214() {
    receive("IV214");
}
#[test]
fn source_initial_value_iv215() {
    receive("IV215");
}
#[test]
fn source_initial_value_iv216() {
    receive("IV216");
}
#[test]
fn source_initial_value_iv217() {
    receive("IV217");
}
#[test]
fn source_initial_value_iv218() {
    receive("IV218");
}
#[test]
fn source_initial_value_iv219() {
    receive("IV219");
}
#[test]
fn source_initial_value_iv220() {
    receive("IV220");
}
#[test]
fn source_initial_value_iv221() {
    receive("IV221");
}
#[test]
fn source_initial_value_iv222() {
    receive("IV222");
}
#[test]
fn source_initial_value_iv223() {
    receive("IV223");
}
#[test]
fn source_initial_value_iv224() {
    receive("IV224");
}
#[test]
fn source_initial_value_iv225() {
    receive("IV225");
}
#[test]
fn source_initial_value_iv226() {
    receive("IV226");
}
#[test]
fn source_initial_value_iv227() {
    receive("IV227");
}
#[test]
fn source_initial_value_iv228() {
    receive("IV228");
}
#[test]
fn source_initial_value_iv229() {
    receive("IV229");
}
#[test]
fn source_initial_value_iv230() {
    receive("IV230");
}
#[test]
fn source_initial_value_iv231() {
    receive("IV231");
}
#[test]
fn source_initial_value_iv232() {
    receive("IV232");
}
#[test]
fn source_initial_value_iv233() {
    receive("IV233");
}
#[test]
fn source_initial_value_iv234() {
    receive("IV234");
}
#[test]
fn source_initial_value_iv235() {
    receive("IV235");
}
#[test]
fn source_initial_value_iv236() {
    receive("IV236");
}
#[test]
fn source_initial_value_iv237() {
    receive("IV237");
}
#[test]
fn source_initial_value_iv238() {
    receive("IV238");
}
#[test]
fn source_initial_value_iv239() {
    receive("IV239");
}
#[test]
fn source_initial_value_iv240() {
    receive("IV240");
}
#[test]
fn source_initial_value_iv241() {
    receive("IV241");
}
#[test]
fn source_initial_value_iv242() {
    receive("IV242");
}
#[test]
fn source_initial_value_iv243() {
    receive("IV243");
}
#[test]
fn source_initial_value_iv244() {
    receive("IV244");
}
#[test]
fn source_initial_value_iv245() {
    receive("IV245");
}
#[test]
fn source_initial_value_iv246() {
    receive("IV246");
}
#[test]
fn source_initial_value_iv247() {
    receive("IV247");
}
#[test]
fn source_initial_value_iv248() {
    receive("IV248");
}
#[test]
fn source_initial_value_iv249() {
    receive("IV249");
}
#[test]
fn source_initial_value_iv250() {
    receive("IV250");
}
#[test]
fn source_initial_value_iv251() {
    receive("IV251");
}
#[test]
fn source_initial_value_iv252() {
    receive("IV252");
}
#[test]
fn source_initial_value_iv253() {
    receive("IV253");
}
#[test]
fn source_initial_value_iv254() {
    receive("IV254");
}
#[test]
fn source_initial_value_iv255() {
    receive("IV255");
}
#[test]
fn source_initial_value_iv256() {
    receive("IV256");
}
#[test]
fn source_initial_value_iv257() {
    receive("IV257");
}
#[test]
fn source_initial_value_iv258() {
    receive("IV258");
}
#[test]
fn source_initial_value_iv259() {
    receive("IV259");
}
#[test]
fn source_initial_value_iv260() {
    receive("IV260");
}
#[test]
fn source_initial_value_iv261() {
    receive("IV261");
}
#[test]
fn source_initial_value_iv262() {
    receive("IV262");
}
#[test]
fn source_initial_value_iv263() {
    receive("IV263");
}
#[test]
fn source_initial_value_iv264() {
    receive("IV264");
}
#[test]
fn source_initial_value_iv265() {
    receive("IV265");
}
#[test]
fn source_initial_value_iv266() {
    receive("IV266");
}
#[test]
fn source_initial_value_iv267() {
    receive("IV267");
}
#[test]
fn source_initial_value_iv268() {
    receive("IV268");
}
#[test]
fn source_initial_value_iv269() {
    receive("IV269");
}
#[test]
fn source_initial_value_iv270() {
    receive("IV270");
}
#[test]
fn source_initial_value_iv271() {
    receive("IV271");
}
#[test]
fn source_initial_value_iv272() {
    receive("IV272");
}
#[test]
fn source_initial_value_iv273() {
    receive("IV273");
}
#[test]
fn source_initial_value_iv274() {
    receive("IV274");
}
#[test]
fn source_initial_value_iv275() {
    receive("IV275");
}
#[test]
fn source_initial_value_iv276() {
    receive("IV276");
}
#[test]
fn source_initial_value_iv277() {
    receive("IV277");
}
#[test]
fn source_initial_value_iv278() {
    receive("IV278");
}
#[test]
fn source_initial_value_iv279() {
    receive("IV279");
}
#[test]
fn source_initial_value_iv280() {
    receive("IV280");
}
#[test]
fn source_initial_value_iv281() {
    receive("IV281");
}
#[test]
fn source_initial_value_iv282() {
    receive("IV282");
}
#[test]
fn source_initial_value_iv283() {
    receive("IV283");
}
#[test]
fn source_initial_value_iv284() {
    receive("IV284");
}
#[test]
fn source_initial_value_iv285() {
    receive("IV285");
}
#[test]
fn source_initial_value_iv286() {
    receive("IV286");
}
#[test]
fn source_initial_value_iv287() {
    receive("IV287");
}
#[test]
fn source_initial_value_iv288() {
    receive("IV288");
}
#[test]
fn source_initial_value_iv289() {
    receive("IV289");
}
#[test]
fn source_initial_value_iv290() {
    receive("IV290");
}
#[test]
fn source_initial_value_iv291() {
    receive("IV291");
}
#[test]
fn source_initial_value_iv292() {
    receive("IV292");
}
#[test]
fn source_initial_value_iv293() {
    receive("IV293");
}
#[test]
fn source_initial_value_iv294() {
    receive("IV294");
}
#[test]
fn source_initial_value_iv295() {
    receive("IV295");
}
#[test]
fn source_initial_value_iv296() {
    receive("IV296");
}
#[test]
fn source_initial_value_iv297() {
    receive("IV297");
}
#[test]
fn source_initial_value_iv298() {
    receive("IV298");
}
#[test]
fn source_initial_value_iv299() {
    receive("IV299");
}
#[test]
fn source_initial_value_iv300() {
    receive("IV300");
}
#[test]
fn source_initial_value_iv301() {
    receive("IV301");
}
#[test]
fn source_initial_value_iv302() {
    receive("IV302");
}
#[test]
fn source_initial_value_iv303() {
    receive("IV303");
}
#[test]
fn source_initial_value_iv304() {
    receive("IV304");
}
#[test]
fn source_initial_value_iv305() {
    receive("IV305");
}
#[test]
fn source_initial_value_iv306() {
    receive("IV306");
}
#[test]
fn source_initial_value_iv307() {
    receive("IV307");
}
#[test]
fn source_initial_value_iv308() {
    receive("IV308");
}
#[test]
fn source_initial_value_iv309() {
    receive("IV309");
}
#[test]
fn source_initial_value_iv310() {
    receive("IV310");
}
#[test]
fn source_initial_value_iv311() {
    receive("IV311");
}
#[test]
fn source_initial_value_iv312() {
    receive("IV312");
}
#[test]
fn source_initial_value_iv313() {
    receive("IV313");
}
#[test]
fn source_initial_value_iv314() {
    receive("IV314");
}
#[test]
fn source_initial_value_iv315() {
    receive("IV315");
}
#[test]
fn source_initial_value_iv316() {
    receive("IV316");
}
#[test]
fn source_initial_value_iv317() {
    receive("IV317");
}
#[test]
fn source_initial_value_iv318() {
    receive("IV318");
}
#[test]
fn source_initial_value_iv319() {
    receive("IV319");
}
#[test]
fn source_initial_value_iv320() {
    receive("IV320");
}
#[test]
fn source_initial_value_iv321() {
    receive("IV321");
}
#[test]
fn source_initial_value_iv322() {
    receive("IV322");
}
#[test]
fn source_initial_value_iv323() {
    receive("IV323");
}
#[test]
fn source_initial_value_iv324() {
    receive("IV324");
}
#[test]
fn source_initial_value_iv325() {
    receive("IV325");
}
#[test]
fn source_initial_value_iv326() {
    receive("IV326");
}
#[test]
fn source_initial_value_iv327() {
    receive("IV327");
}
#[test]
fn source_initial_value_iv328() {
    receive("IV328");
}
#[test]
fn source_initial_value_iv329() {
    receive("IV329");
}
#[test]
fn source_initial_value_iv330() {
    receive("IV330");
}
#[test]
fn source_initial_value_iv331() {
    receive("IV331");
}
#[test]
fn source_initial_value_iv332() {
    receive("IV332");
}
#[test]
fn source_initial_value_iv333() {
    receive("IV333");
}
#[test]
fn source_initial_value_iv334() {
    receive("IV334");
}
#[test]
fn source_initial_value_iv335() {
    receive("IV335");
}
#[test]
fn source_initial_value_iv336() {
    receive("IV336");
}
#[test]
fn source_initial_value_iv337() {
    receive("IV337");
}
#[test]
fn source_initial_value_iv338() {
    receive("IV338");
}
#[test]
fn source_initial_value_iv339() {
    receive("IV339");
}
#[test]
fn source_initial_value_iv340() {
    receive("IV340");
}
#[test]
fn source_initial_value_iv341() {
    receive("IV341");
}
#[test]
fn source_initial_value_iv342() {
    receive("IV342");
}
#[test]
fn source_initial_value_iv343() {
    receive("IV343");
}
#[test]
fn source_initial_value_iv344() {
    receive("IV344");
}
#[test]
fn source_initial_value_iv345() {
    receive("IV345");
}
#[test]
fn source_initial_value_iv346() {
    receive("IV346");
}
#[test]
fn source_initial_value_iv347() {
    receive("IV347");
}
#[test]
fn source_initial_value_iv348() {
    receive("IV348");
}
#[test]
fn source_initial_value_iv349() {
    receive("IV349");
}
#[test]
fn source_initial_value_iv350() {
    receive("IV350");
}
#[test]
fn source_initial_value_iv351() {
    receive("IV351");
}
#[test]
fn source_initial_value_iv352() {
    receive("IV352");
}
#[test]
fn source_initial_value_iv353() {
    receive("IV353");
}
#[test]
fn source_initial_value_iv354() {
    receive("IV354");
}
#[test]
fn source_initial_value_iv355() {
    receive("IV355");
}
#[test]
fn source_initial_value_iv356() {
    receive("IV356");
}
#[test]
fn source_initial_value_iv357() {
    receive("IV357");
}
#[test]
fn source_initial_value_iv358() {
    receive("IV358");
}
#[test]
fn source_initial_value_iv359() {
    receive("IV359");
}
#[test]
fn source_initial_value_iv360() {
    receive("IV360");
}
#[test]
fn source_initial_value_iv361() {
    receive("IV361");
}
#[test]
fn source_initial_value_iv362() {
    receive("IV362");
}
#[test]
fn source_initial_value_iv363() {
    receive("IV363");
}
#[test]
fn source_initial_value_iv364() {
    receive("IV364");
}
#[test]
fn source_initial_value_iv365() {
    receive("IV365");
}
#[test]
fn source_initial_value_iv366() {
    receive("IV366");
}
#[test]
fn source_initial_value_iv367() {
    receive("IV367");
}
#[test]
fn source_initial_value_iv368() {
    receive("IV368");
}
#[test]
fn source_initial_value_iv369() {
    receive("IV369");
}
#[test]
fn source_initial_value_iv370() {
    receive("IV370");
}
#[test]
fn source_initial_value_iv371() {
    receive("IV371");
}
#[test]
fn source_initial_value_iv372() {
    receive("IV372");
}
#[test]
fn source_initial_value_iv373() {
    receive("IV373");
}
#[test]
fn source_initial_value_iv374() {
    receive("IV374");
}
#[test]
fn source_initial_value_iv375() {
    receive("IV375");
}
#[test]
fn source_initial_value_iv376() {
    receive("IV376");
}
#[test]
fn source_initial_value_iv377() {
    receive("IV377");
}
#[test]
fn source_initial_value_iv378() {
    receive("IV378");
}
#[test]
fn source_initial_value_iv379() {
    receive("IV379");
}
#[test]
fn source_initial_value_iv380() {
    receive("IV380");
}
#[test]
fn source_initial_value_iv381() {
    receive("IV381");
}
#[test]
fn source_initial_value_iv382() {
    receive("IV382");
}
#[test]
fn source_initial_value_iv383() {
    receive("IV383");
}
#[test]
fn source_initial_value_iv384() {
    receive("IV384");
}

fn private_consumers() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_consumers_01.json"
    )
}
fn receive_consumer(id: &str) {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_consumers().as_bytes())),
        "4f02882ed3825db71f1e6aa2bd4ee4b5f40812d92484152a3fbe3322f972fe44"
    );
    let root: Value = serde_json::from_str(private_consumers()).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 62);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}
#[test]
fn source_initial_value_ic001() {
    receive_consumer("IC001");
}
#[test]
fn source_initial_value_ic002() {
    receive_consumer("IC002");
}
#[test]
fn source_initial_value_ic003() {
    receive_consumer("IC003");
}
#[test]
fn source_initial_value_ic004() {
    receive_consumer("IC004");
}
#[test]
fn source_initial_value_ic005() {
    receive_consumer("IC005");
}
#[test]
fn source_initial_value_ic006() {
    receive_consumer("IC006");
}
#[test]
fn source_initial_value_ic007() {
    receive_consumer("IC007");
}
#[test]
fn source_initial_value_ic008() {
    receive_consumer("IC008");
}
#[test]
fn source_initial_value_ic009() {
    receive_consumer("IC009");
}
#[test]
fn source_initial_value_ic010() {
    receive_consumer("IC010");
}
#[test]
fn source_initial_value_ic011() {
    receive_consumer("IC011");
}
#[test]
fn source_initial_value_ic012() {
    receive_consumer("IC012");
}
#[test]
fn source_initial_value_ic013() {
    receive_consumer("IC013");
}
#[test]
fn source_initial_value_ic014() {
    receive_consumer("IC014");
}
#[test]
fn source_initial_value_ic015() {
    receive_consumer("IC015");
}
#[test]
fn source_initial_value_ic016() {
    receive_consumer("IC016");
}
#[test]
fn source_initial_value_ic017() {
    receive_consumer("IC017");
}
#[test]
fn source_initial_value_ic018() {
    receive_consumer("IC018");
}
#[test]
fn source_initial_value_ic019() {
    receive_consumer("IC019");
}
#[test]
fn source_initial_value_ic020() {
    receive_consumer("IC020");
}
#[test]
fn source_initial_value_ic021() {
    receive_consumer("IC021");
}
#[test]
fn source_initial_value_ic022() {
    receive_consumer("IC022");
}
#[test]
fn source_initial_value_ic023() {
    receive_consumer("IC023");
}
#[test]
fn source_initial_value_ic024() {
    receive_consumer("IC024");
}
#[test]
fn source_initial_value_ic025() {
    receive_consumer("IC025");
}
#[test]
fn source_initial_value_ic026() {
    receive_consumer("IC026");
}
#[test]
fn source_initial_value_ic027() {
    receive_consumer("IC027");
}
#[test]
fn source_initial_value_ic028() {
    receive_consumer("IC028");
}
#[test]
fn source_initial_value_ic029() {
    receive_consumer("IC029");
}
#[test]
fn source_initial_value_ic030() {
    receive_consumer("IC030");
}
#[test]
fn source_initial_value_ic031() {
    receive_consumer("IC031");
}
#[test]
fn source_initial_value_ic032() {
    receive_consumer("IC032");
}
#[test]
fn source_initial_value_ic033() {
    receive_consumer("IC033");
}
#[test]
fn source_initial_value_ic034() {
    receive_consumer("IC034");
}
#[test]
fn source_initial_value_ic035() {
    receive_consumer("IC035");
}
#[test]
fn source_initial_value_ic036() {
    receive_consumer("IC036");
}
#[test]
fn source_initial_value_ic037() {
    receive_consumer("IC037");
}
#[test]
fn source_initial_value_ic038() {
    receive_consumer("IC038");
}
#[test]
fn source_initial_value_ic039() {
    receive_consumer("IC039");
}
#[test]
fn source_initial_value_ic040() {
    receive_consumer("IC040");
}
#[test]
fn source_initial_value_ic041() {
    receive_consumer("IC041");
}
#[test]
fn source_initial_value_ic042() {
    receive_consumer("IC042");
}
#[test]
fn source_initial_value_ic043() {
    receive_consumer("IC043");
}
#[test]
fn source_initial_value_ic044() {
    receive_consumer("IC044");
}
#[test]
fn source_initial_value_ic045() {
    receive_consumer("IC045");
}
#[test]
fn source_initial_value_ic046() {
    receive_consumer("IC046");
}
#[test]
fn source_initial_value_ic047() {
    receive_consumer("IC047");
}
#[test]
fn source_initial_value_ic048() {
    receive_consumer("IC048");
}
#[test]
fn source_initial_value_ic049() {
    receive_consumer("IC049");
}
#[test]
fn source_initial_value_ic050() {
    receive_consumer("IC050");
}
#[test]
fn source_initial_value_ic051() {
    receive_consumer("IC051");
}
#[test]
fn source_initial_value_ic052() {
    receive_consumer("IC052");
}
#[test]
fn source_initial_value_ic053() {
    receive_consumer("IC053");
}
#[test]
fn source_initial_value_ic054() {
    receive_consumer("IC054");
}
#[test]
fn source_initial_value_ic055() {
    receive_consumer("IC055");
}
#[test]
fn source_initial_value_ic056() {
    receive_consumer("IC056");
}
#[test]
fn source_initial_value_ic057() {
    receive_consumer("IC057");
}
#[test]
fn source_initial_value_ic058() {
    receive_consumer("IC058");
}
#[test]
fn source_initial_value_ic059() {
    receive_consumer("IC059");
}
#[test]
fn source_initial_value_ic060() {
    receive_consumer("IC060");
}
#[test]
fn source_initial_value_ic061() {
    receive_consumer("IC061");
}
#[test]
fn source_initial_value_ic062() {
    receive_consumer("IC062");
}

fn initial_value(
    text: &str,
    datatype: &str,
    normalize: bool,
) -> crate::numeric::NumericInitialValue {
    crate::numeric::NumericInitialValue::SourceDirectString {
        lexical: text.into(),
        datatype: Some(NamedNode::new(datatype).unwrap()),
        language: None,
        normalize,
    }
}
fn limited_session(
    limits: semantica_numeric::NumericLimits,
) -> Rc<crate::numeric::QueryNumericSession> {
    use crate::numeric::*;
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            semantica_numeric::DecimalContext::default(),
            limits,
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
            QueryContextSemantics::SourceContextV1,
            Some(Rc::new(
                SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap(),
            )),
        )
        .unwrap(),
    )
}
#[test]
fn source_initial_value_origin_metadata() {
    use crate::numeric::*;
    let session = source_bgp_receiving::session();
    for (text, datatype, ill_typed) in [
        ("2024-W09-4", oxrdf::vocab::xsd::DATE.as_str(), Some(false)),
        (
            "bad",
            oxrdf::vocab::xsd::DAY_TIME_DURATION.as_str(),
            Some(true),
        ),
        ("unknown", "urn:custom:unmapped", None),
    ] {
        let value = session
            .construct_source_graph_initial(initial_value(text, datatype, true))
            .unwrap();
        let LiteralSourceState::Literal(metadata) = value.literal_source() else {
            panic!("literal expected")
        };
        assert_eq!(metadata.ill_typed, SourceFact::Known(ill_typed));
        let mut wrong = metadata.clone();
        wrong.ill_typed = SourceFact::Known(Some(!ill_typed.unwrap_or(false)));
        assert!(matches!(
            value.clone().with_source_metadata(wrong),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
        let unknown = RawExpressionValue::from_import(value.rdf().unwrap().clone());
        assert!(matches!(
            session.source_datatype(&SolutionValue::Rdf(unknown)),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnknownSourceLiteralIdentity
            ))
        ));
    }
}
#[test]
fn source_initial_value_registered_and_calendar() {
    use crate::numeric::*;
    let session = source_bgp_receiving::session();
    for (text, dt, reason) in [
        (
            "P1Y",
            oxrdf::vocab::xsd::DAY_TIME_DURATION.as_str(),
            "calendar duration source cache",
        ),
        (
            "12:00:00",
            oxrdf::vocab::xsd::TIME.as_str(),
            "source string datatype constructor",
        ),
        (
            "<a/>",
            oxrdf::vocab::rdf::XML_LITERAL.as_str(),
            "source string datatype constructor",
        ),
    ] {
        assert!(
            matches!(session.construct_source_graph_initial(initial_value(text, dt, true)), Err(RawNumericFault::UnsupportedSourceLiteralOperator(value)) if value == reason)
        );
    }
    let input = NumericInitialValue::SourceDirectString {
        lexical: "2024-02-29".into(),
        datatype: Some(oxrdf::vocab::xsd::DATE.into_owned()),
        language: Some("EN".into()),
        normalize: true,
    };
    assert!(matches!(
        session.construct_source_graph_initial(input),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
}
#[test]
fn source_initial_value_resource_limits() {
    use crate::numeric::*;
    use semantica_numeric::{NumericFault, NumericLimits};
    for (limits, text, dt, reason) in [
        (
            NumericLimits {
                max_input_characters: 8,
                ..NumericLimits::default()
            },
            "2024-02-29".to_owned(),
            oxrdf::vocab::xsd::DATE.as_str().to_owned(),
            "input_characters",
        ),
        (
            NumericLimits {
                max_input_characters: 64,
                ..NumericLimits::default()
            },
            "x".to_owned(),
            format!("urn:{}", "a".repeat(65)),
            "input_characters",
        ),
        (
            NumericLimits {
                max_output_characters: 8,
                ..NumericLimits::default()
            },
            "2024-W09-4".to_owned(),
            oxrdf::vocab::xsd::DATE.as_str().to_owned(),
            "output_characters",
        ),
        (
            NumericLimits {
                max_input_characters: 12000,
                ..NumericLimits::default()
            },
            "P".repeat(10001),
            oxrdf::vocab::xsd::DAY_TIME_DURATION.as_str().to_owned(),
            "source_duration_input_bytes",
        ),
    ] {
        let result =
            limited_session(limits).construct_source_graph_initial(initial_value(&text, &dt, true));
        assert!(
            matches!(result, Err(RawNumericFault::Core(NumericFault::ResourceLimit { operation })) if operation == reason),
            "{result:?}"
        );
    }
}
#[test]
fn source_initial_value_closed_entries() {
    for expression in ["DATATYPE(1)", "IF(true,1,0)"] {
        let session = source_bgp_receiving::session();
        let query = numeric_query::prepare_source_frontend_numeric_query(
            &format!("SELECT ({expression} AS ?v) WHERE {{}}"),
            source_bgp_receiving::dataset(&session, None),
            session,
            Vec::new(),
        )
        .unwrap();
        let error = match query.execute_source_presence() {
            Ok(_) => panic!("graph-only expression escaped"),
            Err(e) => e,
        };
        assert_eq!(error.to_string(), "unsupported nullable expression");
    }
    let session = source_bgp_receiving::session();
    for dt in [
        oxrdf::vocab::xsd::DATE.as_str(),
        oxrdf::vocab::xsd::DAY_TIME_DURATION.as_str(),
        "urn:custom:unmapped",
    ] {
        assert!(matches!(
            session.construct_initial(initial_value("bad", dt, false)),
            Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "source string datatype constructor"
            ))
        ));
    }
}
