//! Frozen original time comparison, ordered expressions, sorting and extrema.
use super::*;
use crate::numeric::SourceOrderKey;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixtures() -> [(&'static str, &'static str); 4] {
    [
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_01.json"
            ),
            "556048533b8819cbc479190d459e48b7fcdb4a5c430a034dc01e6dfb82d9b960",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_02.json"
            ),
            "ec817a030dc816ac30e6b3e7718c9810f9329e3f4015807e1f6300a727415ab5",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_03.json"
            ),
            "bb09e7b548fc74bd52bae283cc855d1ead0009df68686a0db64ea66f1bc30100",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_04.json"
            ),
            "164fd8cd84db26af0438c00d6cbd75bb014701c6f7a68159960a8ec3604f05be",
        ),
    ]
}
fn receive(id: &str) {
    let index: usize = id[2..].parse().unwrap();
    let (bytes, digest) = if id.starts_with("TD") {
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_supplement_query_cases.json"
            ),
            "8b8ee673236526ae844b249171c3035d5c4d6889f0c70d8e860725053426b93c",
        )
    } else {
        private_fixtures()[(index - 1) / 78]
    };
    assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), digest);
    let root: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(
        root["graph_queries"].as_array().unwrap().len(),
        if id.starts_with("TD") { 24 } else { 78 }
    );
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case_with_time_comparisons(
        case,
        &root["data_inputs"],
    );
}
#[test]
fn source_time_comparison_tc001() {
    receive("TC001");
}
#[test]
fn source_time_comparison_tc002() {
    receive("TC002");
}
#[test]
fn source_time_comparison_tc003() {
    receive("TC003");
}
#[test]
fn source_time_comparison_tc004() {
    receive("TC004");
}
#[test]
fn source_time_comparison_tc005() {
    receive("TC005");
}
#[test]
fn source_time_comparison_tc006() {
    receive("TC006");
}
#[test]
fn source_time_comparison_tc007() {
    receive("TC007");
}
#[test]
fn source_time_comparison_tc008() {
    receive("TC008");
}
#[test]
fn source_time_comparison_tc009() {
    receive("TC009");
}
#[test]
fn source_time_comparison_tc010() {
    receive("TC010");
}
#[test]
fn source_time_comparison_tc011() {
    receive("TC011");
}
#[test]
fn source_time_comparison_tc012() {
    receive("TC012");
}
#[test]
fn source_time_comparison_tc013() {
    receive("TC013");
}
#[test]
fn source_time_comparison_tc014() {
    receive("TC014");
}
#[test]
fn source_time_comparison_tc015() {
    receive("TC015");
}
#[test]
fn source_time_comparison_tc016() {
    receive("TC016");
}
#[test]
fn source_time_comparison_tc017() {
    receive("TC017");
}
#[test]
fn source_time_comparison_tc018() {
    receive("TC018");
}
#[test]
fn source_time_comparison_tc019() {
    receive("TC019");
}
#[test]
fn source_time_comparison_tc020() {
    receive("TC020");
}
#[test]
fn source_time_comparison_tc021() {
    receive("TC021");
}
#[test]
fn source_time_comparison_tc022() {
    receive("TC022");
}
#[test]
fn source_time_comparison_tc023() {
    receive("TC023");
}
#[test]
fn source_time_comparison_tc024() {
    receive("TC024");
}
#[test]
fn source_time_comparison_tc025() {
    receive("TC025");
}
#[test]
fn source_time_comparison_tc026() {
    receive("TC026");
}
#[test]
fn source_time_comparison_tc027() {
    receive("TC027");
}
#[test]
fn source_time_comparison_tc028() {
    receive("TC028");
}
#[test]
fn source_time_comparison_tc029() {
    receive("TC029");
}
#[test]
fn source_time_comparison_tc030() {
    receive("TC030");
}
#[test]
fn source_time_comparison_tc031() {
    receive("TC031");
}
#[test]
fn source_time_comparison_tc032() {
    receive("TC032");
}
#[test]
fn source_time_comparison_tc033() {
    receive("TC033");
}
#[test]
fn source_time_comparison_tc034() {
    receive("TC034");
}
#[test]
fn source_time_comparison_tc035() {
    receive("TC035");
}
#[test]
fn source_time_comparison_tc036() {
    receive("TC036");
}
#[test]
fn source_time_comparison_tc037() {
    receive("TC037");
}
#[test]
fn source_time_comparison_tc038() {
    receive("TC038");
}
#[test]
fn source_time_comparison_tc039() {
    receive("TC039");
}
#[test]
fn source_time_comparison_tc040() {
    receive("TC040");
}
#[test]
fn source_time_comparison_tc041() {
    receive("TC041");
}
#[test]
fn source_time_comparison_tc042() {
    receive("TC042");
}
#[test]
fn source_time_comparison_tc043() {
    receive("TC043");
}
#[test]
fn source_time_comparison_tc044() {
    receive("TC044");
}
#[test]
fn source_time_comparison_tc045() {
    receive("TC045");
}
#[test]
fn source_time_comparison_tc046() {
    receive("TC046");
}
#[test]
fn source_time_comparison_tc047() {
    receive("TC047");
}
#[test]
fn source_time_comparison_tc048() {
    receive("TC048");
}
#[test]
fn source_time_comparison_tc049() {
    receive("TC049");
}
#[test]
fn source_time_comparison_tc050() {
    receive("TC050");
}
#[test]
fn source_time_comparison_tc051() {
    receive("TC051");
}
#[test]
fn source_time_comparison_tc052() {
    receive("TC052");
}
#[test]
fn source_time_comparison_tc053() {
    receive("TC053");
}
#[test]
fn source_time_comparison_tc054() {
    receive("TC054");
}
#[test]
fn source_time_comparison_tc055() {
    receive("TC055");
}
#[test]
fn source_time_comparison_tc056() {
    receive("TC056");
}
#[test]
fn source_time_comparison_tc057() {
    receive("TC057");
}
#[test]
fn source_time_comparison_tc058() {
    receive("TC058");
}
#[test]
fn source_time_comparison_tc059() {
    receive("TC059");
}
#[test]
fn source_time_comparison_tc060() {
    receive("TC060");
}
#[test]
fn source_time_comparison_tc061() {
    receive("TC061");
}
#[test]
fn source_time_comparison_tc062() {
    receive("TC062");
}
#[test]
fn source_time_comparison_tc063() {
    receive("TC063");
}
#[test]
fn source_time_comparison_tc064() {
    receive("TC064");
}
#[test]
fn source_time_comparison_tc065() {
    receive("TC065");
}
#[test]
fn source_time_comparison_tc066() {
    receive("TC066");
}
#[test]
fn source_time_comparison_tc067() {
    receive("TC067");
}
#[test]
fn source_time_comparison_tc068() {
    receive("TC068");
}
#[test]
fn source_time_comparison_tc069() {
    receive("TC069");
}
#[test]
fn source_time_comparison_tc070() {
    receive("TC070");
}
#[test]
fn source_time_comparison_tc071() {
    receive("TC071");
}
#[test]
fn source_time_comparison_tc072() {
    receive("TC072");
}
#[test]
fn source_time_comparison_tc073() {
    receive("TC073");
}
#[test]
fn source_time_comparison_tc074() {
    receive("TC074");
}
#[test]
fn source_time_comparison_tc075() {
    receive("TC075");
}
#[test]
fn source_time_comparison_tc076() {
    receive("TC076");
}
#[test]
fn source_time_comparison_tc077() {
    receive("TC077");
}
#[test]
fn source_time_comparison_tc078() {
    receive("TC078");
}
#[test]
fn source_time_comparison_tc079() {
    receive("TC079");
}
#[test]
fn source_time_comparison_tc080() {
    receive("TC080");
}
#[test]
fn source_time_comparison_tc081() {
    receive("TC081");
}
#[test]
fn source_time_comparison_tc082() {
    receive("TC082");
}
#[test]
fn source_time_comparison_tc083() {
    receive("TC083");
}
#[test]
fn source_time_comparison_tc084() {
    receive("TC084");
}
#[test]
fn source_time_comparison_tc085() {
    receive("TC085");
}
#[test]
fn source_time_comparison_tc086() {
    receive("TC086");
}
#[test]
fn source_time_comparison_tc087() {
    receive("TC087");
}
#[test]
fn source_time_comparison_tc088() {
    receive("TC088");
}
#[test]
fn source_time_comparison_tc089() {
    receive("TC089");
}
#[test]
fn source_time_comparison_tc090() {
    receive("TC090");
}
#[test]
fn source_time_comparison_tc091() {
    receive("TC091");
}
#[test]
fn source_time_comparison_tc092() {
    receive("TC092");
}
#[test]
fn source_time_comparison_tc093() {
    receive("TC093");
}
#[test]
fn source_time_comparison_tc094() {
    receive("TC094");
}
#[test]
fn source_time_comparison_tc095() {
    receive("TC095");
}
#[test]
fn source_time_comparison_tc096() {
    receive("TC096");
}
#[test]
fn source_time_comparison_tc097() {
    receive("TC097");
}
#[test]
fn source_time_comparison_tc098() {
    receive("TC098");
}
#[test]
fn source_time_comparison_tc099() {
    receive("TC099");
}
#[test]
fn source_time_comparison_tc100() {
    receive("TC100");
}
#[test]
fn source_time_comparison_tc101() {
    receive("TC101");
}
#[test]
fn source_time_comparison_tc102() {
    receive("TC102");
}
#[test]
fn source_time_comparison_tc103() {
    receive("TC103");
}
#[test]
fn source_time_comparison_tc104() {
    receive("TC104");
}
#[test]
fn source_time_comparison_tc105() {
    receive("TC105");
}
#[test]
fn source_time_comparison_tc106() {
    receive("TC106");
}
#[test]
fn source_time_comparison_tc107() {
    receive("TC107");
}
#[test]
fn source_time_comparison_tc108() {
    receive("TC108");
}
#[test]
fn source_time_comparison_tc109() {
    receive("TC109");
}
#[test]
fn source_time_comparison_tc110() {
    receive("TC110");
}
#[test]
fn source_time_comparison_tc111() {
    receive("TC111");
}
#[test]
fn source_time_comparison_tc112() {
    receive("TC112");
}
#[test]
fn source_time_comparison_tc113() {
    receive("TC113");
}
#[test]
fn source_time_comparison_tc114() {
    receive("TC114");
}
#[test]
fn source_time_comparison_tc115() {
    receive("TC115");
}
#[test]
fn source_time_comparison_tc116() {
    receive("TC116");
}
#[test]
fn source_time_comparison_tc117() {
    receive("TC117");
}
#[test]
fn source_time_comparison_tc118() {
    receive("TC118");
}
#[test]
fn source_time_comparison_tc119() {
    receive("TC119");
}
#[test]
fn source_time_comparison_tc120() {
    receive("TC120");
}
#[test]
fn source_time_comparison_tc121() {
    receive("TC121");
}
#[test]
fn source_time_comparison_tc122() {
    receive("TC122");
}
#[test]
fn source_time_comparison_tc123() {
    receive("TC123");
}
#[test]
fn source_time_comparison_tc124() {
    receive("TC124");
}
#[test]
fn source_time_comparison_tc125() {
    receive("TC125");
}
#[test]
fn source_time_comparison_tc126() {
    receive("TC126");
}
#[test]
fn source_time_comparison_tc127() {
    receive("TC127");
}
#[test]
fn source_time_comparison_tc128() {
    receive("TC128");
}
#[test]
fn source_time_comparison_tc129() {
    receive("TC129");
}
#[test]
fn source_time_comparison_tc130() {
    receive("TC130");
}
#[test]
fn source_time_comparison_tc131() {
    receive("TC131");
}
#[test]
fn source_time_comparison_tc132() {
    receive("TC132");
}
#[test]
fn source_time_comparison_tc133() {
    receive("TC133");
}
#[test]
fn source_time_comparison_tc134() {
    receive("TC134");
}
#[test]
fn source_time_comparison_tc135() {
    receive("TC135");
}
#[test]
fn source_time_comparison_tc136() {
    receive("TC136");
}
#[test]
fn source_time_comparison_tc137() {
    receive("TC137");
}
#[test]
fn source_time_comparison_tc138() {
    receive("TC138");
}
#[test]
fn source_time_comparison_tc139() {
    receive("TC139");
}
#[test]
fn source_time_comparison_tc140() {
    receive("TC140");
}
#[test]
fn source_time_comparison_tc141() {
    receive("TC141");
}
#[test]
fn source_time_comparison_tc142() {
    receive("TC142");
}
#[test]
fn source_time_comparison_tc143() {
    receive("TC143");
}
#[test]
fn source_time_comparison_tc144() {
    receive("TC144");
}
#[test]
fn source_time_comparison_tc145() {
    receive("TC145");
}
#[test]
fn source_time_comparison_tc146() {
    receive("TC146");
}
#[test]
fn source_time_comparison_tc147() {
    receive("TC147");
}
#[test]
fn source_time_comparison_tc148() {
    receive("TC148");
}
#[test]
fn source_time_comparison_tc149() {
    receive("TC149");
}
#[test]
fn source_time_comparison_tc150() {
    receive("TC150");
}
#[test]
fn source_time_comparison_tc151() {
    receive("TC151");
}
#[test]
fn source_time_comparison_tc152() {
    receive("TC152");
}
#[test]
fn source_time_comparison_tc153() {
    receive("TC153");
}
#[test]
fn source_time_comparison_tc154() {
    receive("TC154");
}
#[test]
fn source_time_comparison_tc155() {
    receive("TC155");
}
#[test]
fn source_time_comparison_tc156() {
    receive("TC156");
}
#[test]
fn source_time_comparison_tc157() {
    receive("TC157");
}
#[test]
fn source_time_comparison_tc158() {
    receive("TC158");
}
#[test]
fn source_time_comparison_tc159() {
    receive("TC159");
}
#[test]
fn source_time_comparison_tc160() {
    receive("TC160");
}
#[test]
fn source_time_comparison_tc161() {
    receive("TC161");
}
#[test]
fn source_time_comparison_tc162() {
    receive("TC162");
}
#[test]
fn source_time_comparison_tc163() {
    receive("TC163");
}
#[test]
fn source_time_comparison_tc164() {
    receive("TC164");
}
#[test]
fn source_time_comparison_tc165() {
    receive("TC165");
}
#[test]
fn source_time_comparison_tc166() {
    receive("TC166");
}
#[test]
fn source_time_comparison_tc167() {
    receive("TC167");
}
#[test]
fn source_time_comparison_tc168() {
    receive("TC168");
}
#[test]
fn source_time_comparison_tc169() {
    receive("TC169");
}
#[test]
fn source_time_comparison_tc170() {
    receive("TC170");
}
#[test]
fn source_time_comparison_tc171() {
    receive("TC171");
}
#[test]
fn source_time_comparison_tc172() {
    receive("TC172");
}
#[test]
fn source_time_comparison_tc173() {
    receive("TC173");
}
#[test]
fn source_time_comparison_tc174() {
    receive("TC174");
}
#[test]
fn source_time_comparison_tc175() {
    receive("TC175");
}
#[test]
fn source_time_comparison_tc176() {
    receive("TC176");
}
#[test]
fn source_time_comparison_tc177() {
    receive("TC177");
}
#[test]
fn source_time_comparison_tc178() {
    receive("TC178");
}
#[test]
fn source_time_comparison_tc179() {
    receive("TC179");
}
#[test]
fn source_time_comparison_tc180() {
    receive("TC180");
}
#[test]
fn source_time_comparison_tc181() {
    receive("TC181");
}
#[test]
fn source_time_comparison_tc182() {
    receive("TC182");
}
#[test]
fn source_time_comparison_tc183() {
    receive("TC183");
}
#[test]
fn source_time_comparison_tc184() {
    receive("TC184");
}
#[test]
fn source_time_comparison_tc185() {
    receive("TC185");
}
#[test]
fn source_time_comparison_tc186() {
    receive("TC186");
}
#[test]
fn source_time_comparison_tc187() {
    receive("TC187");
}
#[test]
fn source_time_comparison_tc188() {
    receive("TC188");
}
#[test]
fn source_time_comparison_tc189() {
    receive("TC189");
}
#[test]
fn source_time_comparison_tc190() {
    receive("TC190");
}
#[test]
fn source_time_comparison_tc191() {
    receive("TC191");
}
#[test]
fn source_time_comparison_tc192() {
    receive("TC192");
}
#[test]
fn source_time_comparison_tc193() {
    receive("TC193");
}
#[test]
fn source_time_comparison_tc194() {
    receive("TC194");
}
#[test]
fn source_time_comparison_tc195() {
    receive("TC195");
}
#[test]
fn source_time_comparison_tc196() {
    receive("TC196");
}
#[test]
fn source_time_comparison_tc197() {
    receive("TC197");
}
#[test]
fn source_time_comparison_tc198() {
    receive("TC198");
}
#[test]
fn source_time_comparison_tc199() {
    receive("TC199");
}
#[test]
fn source_time_comparison_tc200() {
    receive("TC200");
}
#[test]
fn source_time_comparison_tc201() {
    receive("TC201");
}
#[test]
fn source_time_comparison_tc202() {
    receive("TC202");
}
#[test]
fn source_time_comparison_tc203() {
    receive("TC203");
}
#[test]
fn source_time_comparison_tc204() {
    receive("TC204");
}
#[test]
fn source_time_comparison_tc205() {
    receive("TC205");
}
#[test]
fn source_time_comparison_tc206() {
    receive("TC206");
}
#[test]
fn source_time_comparison_tc207() {
    receive("TC207");
}
#[test]
fn source_time_comparison_tc208() {
    receive("TC208");
}
#[test]
fn source_time_comparison_tc209() {
    receive("TC209");
}
#[test]
fn source_time_comparison_tc210() {
    receive("TC210");
}
#[test]
fn source_time_comparison_tc211() {
    receive("TC211");
}
#[test]
fn source_time_comparison_tc212() {
    receive("TC212");
}
#[test]
fn source_time_comparison_tc213() {
    receive("TC213");
}
#[test]
fn source_time_comparison_tc214() {
    receive("TC214");
}
#[test]
fn source_time_comparison_tc215() {
    receive("TC215");
}
#[test]
fn source_time_comparison_tc216() {
    receive("TC216");
}
#[test]
fn source_time_comparison_tc217() {
    receive("TC217");
}
#[test]
fn source_time_comparison_tc218() {
    receive("TC218");
}
#[test]
fn source_time_comparison_tc219() {
    receive("TC219");
}
#[test]
fn source_time_comparison_tc220() {
    receive("TC220");
}
#[test]
fn source_time_comparison_tc221() {
    receive("TC221");
}
#[test]
fn source_time_comparison_tc222() {
    receive("TC222");
}
#[test]
fn source_time_comparison_tc223() {
    receive("TC223");
}
#[test]
fn source_time_comparison_tc224() {
    receive("TC224");
}
#[test]
fn source_time_comparison_tc225() {
    receive("TC225");
}
#[test]
fn source_time_comparison_tc226() {
    receive("TC226");
}
#[test]
fn source_time_comparison_tc227() {
    receive("TC227");
}
#[test]
fn source_time_comparison_tc228() {
    receive("TC228");
}
#[test]
fn source_time_comparison_tc229() {
    receive("TC229");
}
#[test]
fn source_time_comparison_tc230() {
    receive("TC230");
}
#[test]
fn source_time_comparison_tc231() {
    receive("TC231");
}
#[test]
fn source_time_comparison_tc232() {
    receive("TC232");
}
#[test]
fn source_time_comparison_tc233() {
    receive("TC233");
}
#[test]
fn source_time_comparison_tc234() {
    receive("TC234");
}
#[test]
fn source_time_comparison_tc235() {
    receive("TC235");
}
#[test]
fn source_time_comparison_tc236() {
    receive("TC236");
}
#[test]
fn source_time_comparison_tc237() {
    receive("TC237");
}
#[test]
fn source_time_comparison_tc238() {
    receive("TC238");
}
#[test]
fn source_time_comparison_tc239() {
    receive("TC239");
}
#[test]
fn source_time_comparison_tc240() {
    receive("TC240");
}
#[test]
fn source_time_comparison_tc241() {
    receive("TC241");
}
#[test]
fn source_time_comparison_tc242() {
    receive("TC242");
}
#[test]
fn source_time_comparison_tc243() {
    receive("TC243");
}
#[test]
fn source_time_comparison_tc244() {
    receive("TC244");
}
#[test]
fn source_time_comparison_tc245() {
    receive("TC245");
}
#[test]
fn source_time_comparison_tc246() {
    receive("TC246");
}
#[test]
fn source_time_comparison_tc247() {
    receive("TC247");
}
#[test]
fn source_time_comparison_tc248() {
    receive("TC248");
}
#[test]
fn source_time_comparison_tc249() {
    receive("TC249");
}
#[test]
fn source_time_comparison_tc250() {
    receive("TC250");
}
#[test]
fn source_time_comparison_tc251() {
    receive("TC251");
}
#[test]
fn source_time_comparison_tc252() {
    receive("TC252");
}
#[test]
fn source_time_comparison_tc253() {
    receive("TC253");
}
#[test]
fn source_time_comparison_tc254() {
    receive("TC254");
}
#[test]
fn source_time_comparison_tc255() {
    receive("TC255");
}
#[test]
fn source_time_comparison_tc256() {
    receive("TC256");
}
#[test]
fn source_time_comparison_tc257() {
    receive("TC257");
}
#[test]
fn source_time_comparison_tc258() {
    receive("TC258");
}
#[test]
fn source_time_comparison_tc259() {
    receive("TC259");
}
#[test]
fn source_time_comparison_tc260() {
    receive("TC260");
}
#[test]
fn source_time_comparison_tc261() {
    receive("TC261");
}
#[test]
fn source_time_comparison_tc262() {
    receive("TC262");
}
#[test]
fn source_time_comparison_tc263() {
    receive("TC263");
}
#[test]
fn source_time_comparison_tc264() {
    receive("TC264");
}
#[test]
fn source_time_comparison_tc265() {
    receive("TC265");
}
#[test]
fn source_time_comparison_tc266() {
    receive("TC266");
}
#[test]
fn source_time_comparison_tc267() {
    receive("TC267");
}
#[test]
fn source_time_comparison_tc268() {
    receive("TC268");
}
#[test]
fn source_time_comparison_tc269() {
    receive("TC269");
}
#[test]
fn source_time_comparison_tc270() {
    receive("TC270");
}
#[test]
fn source_time_comparison_tc271() {
    receive("TC271");
}
#[test]
fn source_time_comparison_tc272() {
    receive("TC272");
}
#[test]
fn source_time_comparison_tc273() {
    receive("TC273");
}
#[test]
fn source_time_comparison_tc274() {
    receive("TC274");
}
#[test]
fn source_time_comparison_tc275() {
    receive("TC275");
}
#[test]
fn source_time_comparison_tc276() {
    receive("TC276");
}
#[test]
fn source_time_comparison_tc277() {
    receive("TC277");
}
#[test]
fn source_time_comparison_tc278() {
    receive("TC278");
}
#[test]
fn source_time_comparison_tc279() {
    receive("TC279");
}
#[test]
fn source_time_comparison_tc280() {
    receive("TC280");
}
#[test]
fn source_time_comparison_tc281() {
    receive("TC281");
}
#[test]
fn source_time_comparison_tc282() {
    receive("TC282");
}
#[test]
fn source_time_comparison_tc283() {
    receive("TC283");
}
#[test]
fn source_time_comparison_tc284() {
    receive("TC284");
}
#[test]
fn source_time_comparison_tc285() {
    receive("TC285");
}
#[test]
fn source_time_comparison_tc286() {
    receive("TC286");
}
#[test]
fn source_time_comparison_tc287() {
    receive("TC287");
}
#[test]
fn source_time_comparison_tc288() {
    receive("TC288");
}
#[test]
fn source_time_comparison_tc289() {
    receive("TC289");
}
#[test]
fn source_time_comparison_tc290() {
    receive("TC290");
}
#[test]
fn source_time_comparison_tc291() {
    receive("TC291");
}
#[test]
fn source_time_comparison_tc292() {
    receive("TC292");
}
#[test]
fn source_time_comparison_tc293() {
    receive("TC293");
}
#[test]
fn source_time_comparison_tc294() {
    receive("TC294");
}
#[test]
fn source_time_comparison_tc295() {
    receive("TC295");
}
#[test]
fn source_time_comparison_tc296() {
    receive("TC296");
}
#[test]
fn source_time_comparison_tc297() {
    receive("TC297");
}
#[test]
fn source_time_comparison_tc298() {
    receive("TC298");
}
#[test]
fn source_time_comparison_tc299() {
    receive("TC299");
}
#[test]
fn source_time_comparison_tc300() {
    receive("TC300");
}
#[test]
fn source_time_comparison_tc301() {
    receive("TC301");
}
#[test]
fn source_time_comparison_tc302() {
    receive("TC302");
}
#[test]
fn source_time_comparison_tc303() {
    receive("TC303");
}
#[test]
fn source_time_comparison_tc304() {
    receive("TC304");
}
#[test]
fn source_time_comparison_tc305() {
    receive("TC305");
}
#[test]
fn source_time_comparison_tc306() {
    receive("TC306");
}
#[test]
fn source_time_comparison_tc307() {
    receive("TC307");
}
#[test]
fn source_time_comparison_tc308() {
    receive("TC308");
}
#[test]
fn source_time_comparison_tc309() {
    receive("TC309");
}
#[test]
fn source_time_comparison_tc310() {
    receive("TC310");
}
#[test]
fn source_time_comparison_tc311() {
    receive("TC311");
}
#[test]
fn source_time_comparison_tc312() {
    receive("TC312");
}

#[test]
fn source_time_comparison_td001() {
    receive("TD001");
}

#[test]
fn source_time_comparison_td002() {
    receive("TD002");
}

#[test]
fn source_time_comparison_td003() {
    receive("TD003");
}

#[test]
fn source_time_comparison_td004() {
    receive("TD004");
}

#[test]
fn source_time_comparison_td005() {
    receive("TD005");
}

#[test]
fn source_time_comparison_td006() {
    receive("TD006");
}

#[test]
fn source_time_comparison_td007() {
    receive("TD007");
}

#[test]
fn source_time_comparison_td008() {
    receive("TD008");
}

#[test]
fn source_time_comparison_td009() {
    receive("TD009");
}

#[test]
fn source_time_comparison_td010() {
    receive("TD010");
}

#[test]
fn source_time_comparison_td011() {
    receive("TD011");
}

#[test]
fn source_time_comparison_td012() {
    receive("TD012");
}

#[test]
fn source_time_comparison_td013() {
    receive("TD013");
}

#[test]
fn source_time_comparison_td014() {
    receive("TD014");
}

#[test]
fn source_time_comparison_td015() {
    receive("TD015");
}

#[test]
fn source_time_comparison_td016() {
    receive("TD016");
}

#[test]
fn source_time_comparison_td017() {
    receive("TD017");
}

#[test]
fn source_time_comparison_td018() {
    receive("TD018");
}

#[test]
fn source_time_comparison_td019() {
    receive("TD019");
}

#[test]
fn source_time_comparison_td020() {
    receive("TD020");
}

#[test]
fn source_time_comparison_td021() {
    receive("TD021");
}

#[test]
fn source_time_comparison_td022() {
    receive("TD022");
}

#[test]
fn source_time_comparison_td023() {
    receive("TD023");
}

#[test]
fn source_time_comparison_td024() {
    receive("TD024");
}

fn comparison_session() -> Rc<QueryNumericSession> {
    Rc::new(
        Rc::try_unwrap(source_bgp_receiving::session())
            .unwrap()
            .with_source_time_comparisons_v1()
            .unwrap(),
    )
}
fn time_value(session: &QueryNumericSession, text: &str) -> RawExpressionValue {
    session
        .construct_source_graph_initial(crate::numeric::NumericInitialValue::SourceDirectString {
            lexical: text.into(),
            datatype: Some(oxrdf::vocab::xsd::TIME.into_owned()),
            language: None,
            normalize: false,
        })
        .unwrap()
}
#[test]
fn source_time_comparison_closed_entries() {
    use crate::numeric::*;
    let previous = Rc::try_unwrap(source_bgp_receiving::session())
        .unwrap()
        .with_source_time_values_v1()
        .unwrap();
    let a = time_value(&previous, "12");
    let b = time_value(&previous, "13");
    assert!(
        previous
            .source_order_key_less(&SourceOrderKey::Rdf(a), &SourceOrderKey::Rdf(b))
            .is_err()
    );
    let legacy = QueryNumericSession::new(
        semantica_numeric::DecimalContext::default(),
        semantica_numeric::NumericLimits::default(),
        None,
        true,
    )
    .unwrap();
    assert!(legacy.with_source_time_comparisons_v1().is_err());
    let current = comparison_session();
    assert!(
        current
            .construct_initial(NumericInitialValue::SourceDirectString {
                lexical: "12".into(),
                datatype: Some(oxrdf::vocab::xsd::TIME.into_owned()),
                language: None,
                normalize: false,
            })
            .is_err()
    );
}
#[test]
fn source_time_comparison_session_isolation() {
    let previous = Rc::try_unwrap(source_bgp_receiving::session())
        .unwrap()
        .with_source_time_values_v1()
        .unwrap();
    let current = comparison_session();
    let a = time_value(&current, "12");
    let b = time_value(&current, "13");
    let ka = SourceOrderKey::Rdf(a);
    let kb = SourceOrderKey::Rdf(b);
    assert_eq!(current.source_order_key_less(&ka, &kb).unwrap(), true);
    assert!(previous.source_order_key_less(&ka, &kb).is_err());
}
#[test]
fn source_time_comparison_unknown_origin() {
    let current = comparison_session();
    let a = time_value(&current, "12");
    let b = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("13", oxrdf::vocab::xsd::TIME).into(),
    );
    assert!(
        current
            .source_order_key_less(
                &SourceOrderKey::Rdf(a.clone()),
                &SourceOrderKey::Rdf(b.clone())
            )
            .is_err()
    );
    assert!(
        current
            .source_relational_equal(&SolutionValue::Rdf(a), &SolutionValue::Rdf(b))
            .is_err()
    );
}
