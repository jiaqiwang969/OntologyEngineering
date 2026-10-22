//! Unchanged original relational value and error-consumption observations.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixtures() -> [(&'static str, &'static str); 4] {
    [
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_01.json"
            ),
            "88cda05a610f161cee2b40a8ce6d8376e4cf659e0a44c59bcecfe70a357c15b7",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_02.json"
            ),
            "0d42f06b8d8ac690b92ec222bf5b53be623b60feffdbd67a2cafe09d811b677a",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_03.json"
            ),
            "2ab2b10c5e68b27fc130b94e31ac55d26fc5b9da7030f303503fa9819498c093",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_04.json"
            ),
            "26b9556aa568c6b41e7bef714409821a38769e5ab51ddc7e5770888aa8612e10",
        ),
    ]
}
fn receive(id: &str) {
    let index: usize = id[2..].parse().unwrap();
    let (bytes, digest) = private_fixtures()[(index - 1) / 90];
    assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), digest);
    let root: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 90);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}
#[test]
fn source_relational_value_rv001() {
    receive("RV001");
}
#[test]
fn source_relational_value_rv002() {
    receive("RV002");
}
#[test]
fn source_relational_value_rv003() {
    receive("RV003");
}
#[test]
fn source_relational_value_rv004() {
    receive("RV004");
}
#[test]
fn source_relational_value_rv005() {
    receive("RV005");
}
#[test]
fn source_relational_value_rv006() {
    receive("RV006");
}
#[test]
fn source_relational_value_rv007() {
    receive("RV007");
}
#[test]
fn source_relational_value_rv008() {
    receive("RV008");
}
#[test]
fn source_relational_value_rv009() {
    receive("RV009");
}
#[test]
fn source_relational_value_rv010() {
    receive("RV010");
}
#[test]
fn source_relational_value_rv011() {
    receive("RV011");
}
#[test]
fn source_relational_value_rv012() {
    receive("RV012");
}
#[test]
fn source_relational_value_rv013() {
    receive("RV013");
}
#[test]
fn source_relational_value_rv014() {
    receive("RV014");
}
#[test]
fn source_relational_value_rv015() {
    receive("RV015");
}
#[test]
fn source_relational_value_rv016() {
    receive("RV016");
}
#[test]
fn source_relational_value_rv017() {
    receive("RV017");
}
#[test]
fn source_relational_value_rv018() {
    receive("RV018");
}
#[test]
fn source_relational_value_rv019() {
    receive("RV019");
}
#[test]
fn source_relational_value_rv020() {
    receive("RV020");
}
#[test]
fn source_relational_value_rv021() {
    receive("RV021");
}
#[test]
fn source_relational_value_rv022() {
    receive("RV022");
}
#[test]
fn source_relational_value_rv023() {
    receive("RV023");
}
#[test]
fn source_relational_value_rv024() {
    receive("RV024");
}
#[test]
fn source_relational_value_rv025() {
    receive("RV025");
}
#[test]
fn source_relational_value_rv026() {
    receive("RV026");
}
#[test]
fn source_relational_value_rv027() {
    receive("RV027");
}
#[test]
fn source_relational_value_rv028() {
    receive("RV028");
}
#[test]
fn source_relational_value_rv029() {
    receive("RV029");
}
#[test]
fn source_relational_value_rv030() {
    receive("RV030");
}
#[test]
fn source_relational_value_rv031() {
    receive("RV031");
}
#[test]
fn source_relational_value_rv032() {
    receive("RV032");
}
#[test]
fn source_relational_value_rv033() {
    receive("RV033");
}
#[test]
fn source_relational_value_rv034() {
    receive("RV034");
}
#[test]
fn source_relational_value_rv035() {
    receive("RV035");
}
#[test]
fn source_relational_value_rv036() {
    receive("RV036");
}
#[test]
fn source_relational_value_rv037() {
    receive("RV037");
}
#[test]
fn source_relational_value_rv038() {
    receive("RV038");
}
#[test]
fn source_relational_value_rv039() {
    receive("RV039");
}
#[test]
fn source_relational_value_rv040() {
    receive("RV040");
}
#[test]
fn source_relational_value_rv041() {
    receive("RV041");
}
#[test]
fn source_relational_value_rv042() {
    receive("RV042");
}
#[test]
fn source_relational_value_rv043() {
    receive("RV043");
}
#[test]
fn source_relational_value_rv044() {
    receive("RV044");
}
#[test]
fn source_relational_value_rv045() {
    receive("RV045");
}
#[test]
fn source_relational_value_rv046() {
    receive("RV046");
}
#[test]
fn source_relational_value_rv047() {
    receive("RV047");
}
#[test]
fn source_relational_value_rv048() {
    receive("RV048");
}
#[test]
fn source_relational_value_rv049() {
    receive("RV049");
}
#[test]
fn source_relational_value_rv050() {
    receive("RV050");
}
#[test]
fn source_relational_value_rv051() {
    receive("RV051");
}
#[test]
fn source_relational_value_rv052() {
    receive("RV052");
}
#[test]
fn source_relational_value_rv053() {
    receive("RV053");
}
#[test]
fn source_relational_value_rv054() {
    receive("RV054");
}
#[test]
fn source_relational_value_rv055() {
    receive("RV055");
}
#[test]
fn source_relational_value_rv056() {
    receive("RV056");
}
#[test]
fn source_relational_value_rv057() {
    receive("RV057");
}
#[test]
fn source_relational_value_rv058() {
    receive("RV058");
}
#[test]
fn source_relational_value_rv059() {
    receive("RV059");
}
#[test]
fn source_relational_value_rv060() {
    receive("RV060");
}
#[test]
fn source_relational_value_rv061() {
    receive("RV061");
}
#[test]
fn source_relational_value_rv062() {
    receive("RV062");
}
#[test]
fn source_relational_value_rv063() {
    receive("RV063");
}
#[test]
fn source_relational_value_rv064() {
    receive("RV064");
}
#[test]
fn source_relational_value_rv065() {
    receive("RV065");
}
#[test]
fn source_relational_value_rv066() {
    receive("RV066");
}
#[test]
fn source_relational_value_rv067() {
    receive("RV067");
}
#[test]
fn source_relational_value_rv068() {
    receive("RV068");
}
#[test]
fn source_relational_value_rv069() {
    receive("RV069");
}
#[test]
fn source_relational_value_rv070() {
    receive("RV070");
}
#[test]
fn source_relational_value_rv071() {
    receive("RV071");
}
#[test]
fn source_relational_value_rv072() {
    receive("RV072");
}
#[test]
fn source_relational_value_rv073() {
    receive("RV073");
}
#[test]
fn source_relational_value_rv074() {
    receive("RV074");
}
#[test]
fn source_relational_value_rv075() {
    receive("RV075");
}
#[test]
fn source_relational_value_rv076() {
    receive("RV076");
}
#[test]
fn source_relational_value_rv077() {
    receive("RV077");
}
#[test]
fn source_relational_value_rv078() {
    receive("RV078");
}
#[test]
fn source_relational_value_rv079() {
    receive("RV079");
}
#[test]
fn source_relational_value_rv080() {
    receive("RV080");
}
#[test]
fn source_relational_value_rv081() {
    receive("RV081");
}
#[test]
fn source_relational_value_rv082() {
    receive("RV082");
}
#[test]
fn source_relational_value_rv083() {
    receive("RV083");
}
#[test]
fn source_relational_value_rv084() {
    receive("RV084");
}
#[test]
fn source_relational_value_rv085() {
    receive("RV085");
}
#[test]
fn source_relational_value_rv086() {
    receive("RV086");
}
#[test]
fn source_relational_value_rv087() {
    receive("RV087");
}
#[test]
fn source_relational_value_rv088() {
    receive("RV088");
}
#[test]
fn source_relational_value_rv089() {
    receive("RV089");
}
#[test]
fn source_relational_value_rv090() {
    receive("RV090");
}
#[test]
fn source_relational_value_rv091() {
    receive("RV091");
}
#[test]
fn source_relational_value_rv092() {
    receive("RV092");
}
#[test]
fn source_relational_value_rv093() {
    receive("RV093");
}
#[test]
fn source_relational_value_rv094() {
    receive("RV094");
}
#[test]
fn source_relational_value_rv095() {
    receive("RV095");
}
#[test]
fn source_relational_value_rv096() {
    receive("RV096");
}
#[test]
fn source_relational_value_rv097() {
    receive("RV097");
}
#[test]
fn source_relational_value_rv098() {
    receive("RV098");
}
#[test]
fn source_relational_value_rv099() {
    receive("RV099");
}
#[test]
fn source_relational_value_rv100() {
    receive("RV100");
}
#[test]
fn source_relational_value_rv101() {
    receive("RV101");
}
#[test]
fn source_relational_value_rv102() {
    receive("RV102");
}
#[test]
fn source_relational_value_rv103() {
    receive("RV103");
}
#[test]
fn source_relational_value_rv104() {
    receive("RV104");
}
#[test]
fn source_relational_value_rv105() {
    receive("RV105");
}
#[test]
fn source_relational_value_rv106() {
    receive("RV106");
}
#[test]
fn source_relational_value_rv107() {
    receive("RV107");
}
#[test]
fn source_relational_value_rv108() {
    receive("RV108");
}
#[test]
fn source_relational_value_rv109() {
    receive("RV109");
}
#[test]
fn source_relational_value_rv110() {
    receive("RV110");
}
#[test]
fn source_relational_value_rv111() {
    receive("RV111");
}
#[test]
fn source_relational_value_rv112() {
    receive("RV112");
}
#[test]
fn source_relational_value_rv113() {
    receive("RV113");
}
#[test]
fn source_relational_value_rv114() {
    receive("RV114");
}
#[test]
fn source_relational_value_rv115() {
    receive("RV115");
}
#[test]
fn source_relational_value_rv116() {
    receive("RV116");
}
#[test]
fn source_relational_value_rv117() {
    receive("RV117");
}
#[test]
fn source_relational_value_rv118() {
    receive("RV118");
}
#[test]
fn source_relational_value_rv119() {
    receive("RV119");
}
#[test]
fn source_relational_value_rv120() {
    receive("RV120");
}
#[test]
fn source_relational_value_rv121() {
    receive("RV121");
}
#[test]
fn source_relational_value_rv122() {
    receive("RV122");
}
#[test]
fn source_relational_value_rv123() {
    receive("RV123");
}
#[test]
fn source_relational_value_rv124() {
    receive("RV124");
}
#[test]
fn source_relational_value_rv125() {
    receive("RV125");
}
#[test]
fn source_relational_value_rv126() {
    receive("RV126");
}
#[test]
fn source_relational_value_rv127() {
    receive("RV127");
}
#[test]
fn source_relational_value_rv128() {
    receive("RV128");
}
#[test]
fn source_relational_value_rv129() {
    receive("RV129");
}
#[test]
fn source_relational_value_rv130() {
    receive("RV130");
}
#[test]
fn source_relational_value_rv131() {
    receive("RV131");
}
#[test]
fn source_relational_value_rv132() {
    receive("RV132");
}
#[test]
fn source_relational_value_rv133() {
    receive("RV133");
}
#[test]
fn source_relational_value_rv134() {
    receive("RV134");
}
#[test]
fn source_relational_value_rv135() {
    receive("RV135");
}
#[test]
fn source_relational_value_rv136() {
    receive("RV136");
}
#[test]
fn source_relational_value_rv137() {
    receive("RV137");
}
#[test]
fn source_relational_value_rv138() {
    receive("RV138");
}
#[test]
fn source_relational_value_rv139() {
    receive("RV139");
}
#[test]
fn source_relational_value_rv140() {
    receive("RV140");
}
#[test]
fn source_relational_value_rv141() {
    receive("RV141");
}
#[test]
fn source_relational_value_rv142() {
    receive("RV142");
}
#[test]
fn source_relational_value_rv143() {
    receive("RV143");
}
#[test]
fn source_relational_value_rv144() {
    receive("RV144");
}
#[test]
fn source_relational_value_rv145() {
    receive("RV145");
}
#[test]
fn source_relational_value_rv146() {
    receive("RV146");
}
#[test]
fn source_relational_value_rv147() {
    receive("RV147");
}
#[test]
fn source_relational_value_rv148() {
    receive("RV148");
}
#[test]
fn source_relational_value_rv149() {
    receive("RV149");
}
#[test]
fn source_relational_value_rv150() {
    receive("RV150");
}
#[test]
fn source_relational_value_rv151() {
    receive("RV151");
}
#[test]
fn source_relational_value_rv152() {
    receive("RV152");
}
#[test]
fn source_relational_value_rv153() {
    receive("RV153");
}
#[test]
fn source_relational_value_rv154() {
    receive("RV154");
}
#[test]
fn source_relational_value_rv155() {
    receive("RV155");
}
#[test]
fn source_relational_value_rv156() {
    receive("RV156");
}
#[test]
fn source_relational_value_rv157() {
    receive("RV157");
}
#[test]
fn source_relational_value_rv158() {
    receive("RV158");
}
#[test]
fn source_relational_value_rv159() {
    receive("RV159");
}
#[test]
fn source_relational_value_rv160() {
    receive("RV160");
}
#[test]
fn source_relational_value_rv161() {
    receive("RV161");
}
#[test]
fn source_relational_value_rv162() {
    receive("RV162");
}
#[test]
fn source_relational_value_rv163() {
    receive("RV163");
}
#[test]
fn source_relational_value_rv164() {
    receive("RV164");
}
#[test]
fn source_relational_value_rv165() {
    receive("RV165");
}
#[test]
fn source_relational_value_rv166() {
    receive("RV166");
}
#[test]
fn source_relational_value_rv167() {
    receive("RV167");
}
#[test]
fn source_relational_value_rv168() {
    receive("RV168");
}
#[test]
fn source_relational_value_rv169() {
    receive("RV169");
}
#[test]
fn source_relational_value_rv170() {
    receive("RV170");
}
#[test]
fn source_relational_value_rv171() {
    receive("RV171");
}
#[test]
fn source_relational_value_rv172() {
    receive("RV172");
}
#[test]
fn source_relational_value_rv173() {
    receive("RV173");
}
#[test]
fn source_relational_value_rv174() {
    receive("RV174");
}
#[test]
fn source_relational_value_rv175() {
    receive("RV175");
}
#[test]
fn source_relational_value_rv176() {
    receive("RV176");
}
#[test]
fn source_relational_value_rv177() {
    receive("RV177");
}
#[test]
fn source_relational_value_rv178() {
    receive("RV178");
}
#[test]
fn source_relational_value_rv179() {
    receive("RV179");
}
#[test]
fn source_relational_value_rv180() {
    receive("RV180");
}
#[test]
fn source_relational_value_rv181() {
    receive("RV181");
}
#[test]
fn source_relational_value_rv182() {
    receive("RV182");
}
#[test]
fn source_relational_value_rv183() {
    receive("RV183");
}
#[test]
fn source_relational_value_rv184() {
    receive("RV184");
}
#[test]
fn source_relational_value_rv185() {
    receive("RV185");
}
#[test]
fn source_relational_value_rv186() {
    receive("RV186");
}
#[test]
fn source_relational_value_rv187() {
    receive("RV187");
}
#[test]
fn source_relational_value_rv188() {
    receive("RV188");
}
#[test]
fn source_relational_value_rv189() {
    receive("RV189");
}
#[test]
fn source_relational_value_rv190() {
    receive("RV190");
}
#[test]
fn source_relational_value_rv191() {
    receive("RV191");
}
#[test]
fn source_relational_value_rv192() {
    receive("RV192");
}
#[test]
fn source_relational_value_rv193() {
    receive("RV193");
}
#[test]
fn source_relational_value_rv194() {
    receive("RV194");
}
#[test]
fn source_relational_value_rv195() {
    receive("RV195");
}
#[test]
fn source_relational_value_rv196() {
    receive("RV196");
}
#[test]
fn source_relational_value_rv197() {
    receive("RV197");
}
#[test]
fn source_relational_value_rv198() {
    receive("RV198");
}
#[test]
fn source_relational_value_rv199() {
    receive("RV199");
}
#[test]
fn source_relational_value_rv200() {
    receive("RV200");
}
#[test]
fn source_relational_value_rv201() {
    receive("RV201");
}
#[test]
fn source_relational_value_rv202() {
    receive("RV202");
}
#[test]
fn source_relational_value_rv203() {
    receive("RV203");
}
#[test]
fn source_relational_value_rv204() {
    receive("RV204");
}
#[test]
fn source_relational_value_rv205() {
    receive("RV205");
}
#[test]
fn source_relational_value_rv206() {
    receive("RV206");
}
#[test]
fn source_relational_value_rv207() {
    receive("RV207");
}
#[test]
fn source_relational_value_rv208() {
    receive("RV208");
}
#[test]
fn source_relational_value_rv209() {
    receive("RV209");
}
#[test]
fn source_relational_value_rv210() {
    receive("RV210");
}
#[test]
fn source_relational_value_rv211() {
    receive("RV211");
}
#[test]
fn source_relational_value_rv212() {
    receive("RV212");
}
#[test]
fn source_relational_value_rv213() {
    receive("RV213");
}
#[test]
fn source_relational_value_rv214() {
    receive("RV214");
}
#[test]
fn source_relational_value_rv215() {
    receive("RV215");
}
#[test]
fn source_relational_value_rv216() {
    receive("RV216");
}
#[test]
fn source_relational_value_rv217() {
    receive("RV217");
}
#[test]
fn source_relational_value_rv218() {
    receive("RV218");
}
#[test]
fn source_relational_value_rv219() {
    receive("RV219");
}
#[test]
fn source_relational_value_rv220() {
    receive("RV220");
}
#[test]
fn source_relational_value_rv221() {
    receive("RV221");
}
#[test]
fn source_relational_value_rv222() {
    receive("RV222");
}
#[test]
fn source_relational_value_rv223() {
    receive("RV223");
}
#[test]
fn source_relational_value_rv224() {
    receive("RV224");
}
#[test]
fn source_relational_value_rv225() {
    receive("RV225");
}
#[test]
fn source_relational_value_rv226() {
    receive("RV226");
}
#[test]
fn source_relational_value_rv227() {
    receive("RV227");
}
#[test]
fn source_relational_value_rv228() {
    receive("RV228");
}
#[test]
fn source_relational_value_rv229() {
    receive("RV229");
}
#[test]
fn source_relational_value_rv230() {
    receive("RV230");
}
#[test]
fn source_relational_value_rv231() {
    receive("RV231");
}
#[test]
fn source_relational_value_rv232() {
    receive("RV232");
}
#[test]
fn source_relational_value_rv233() {
    receive("RV233");
}
#[test]
fn source_relational_value_rv234() {
    receive("RV234");
}
#[test]
fn source_relational_value_rv235() {
    receive("RV235");
}
#[test]
fn source_relational_value_rv236() {
    receive("RV236");
}
#[test]
fn source_relational_value_rv237() {
    receive("RV237");
}
#[test]
fn source_relational_value_rv238() {
    receive("RV238");
}
#[test]
fn source_relational_value_rv239() {
    receive("RV239");
}
#[test]
fn source_relational_value_rv240() {
    receive("RV240");
}
#[test]
fn source_relational_value_rv241() {
    receive("RV241");
}
#[test]
fn source_relational_value_rv242() {
    receive("RV242");
}
#[test]
fn source_relational_value_rv243() {
    receive("RV243");
}
#[test]
fn source_relational_value_rv244() {
    receive("RV244");
}
#[test]
fn source_relational_value_rv245() {
    receive("RV245");
}
#[test]
fn source_relational_value_rv246() {
    receive("RV246");
}
#[test]
fn source_relational_value_rv247() {
    receive("RV247");
}
#[test]
fn source_relational_value_rv248() {
    receive("RV248");
}
#[test]
fn source_relational_value_rv249() {
    receive("RV249");
}
#[test]
fn source_relational_value_rv250() {
    receive("RV250");
}
#[test]
fn source_relational_value_rv251() {
    receive("RV251");
}
#[test]
fn source_relational_value_rv252() {
    receive("RV252");
}
#[test]
fn source_relational_value_rv253() {
    receive("RV253");
}
#[test]
fn source_relational_value_rv254() {
    receive("RV254");
}
#[test]
fn source_relational_value_rv255() {
    receive("RV255");
}
#[test]
fn source_relational_value_rv256() {
    receive("RV256");
}
#[test]
fn source_relational_value_rv257() {
    receive("RV257");
}
#[test]
fn source_relational_value_rv258() {
    receive("RV258");
}
#[test]
fn source_relational_value_rv259() {
    receive("RV259");
}
#[test]
fn source_relational_value_rv260() {
    receive("RV260");
}
#[test]
fn source_relational_value_rv261() {
    receive("RV261");
}
#[test]
fn source_relational_value_rv262() {
    receive("RV262");
}
#[test]
fn source_relational_value_rv263() {
    receive("RV263");
}
#[test]
fn source_relational_value_rv264() {
    receive("RV264");
}
#[test]
fn source_relational_value_rv265() {
    receive("RV265");
}
#[test]
fn source_relational_value_rv266() {
    receive("RV266");
}
#[test]
fn source_relational_value_rv267() {
    receive("RV267");
}
#[test]
fn source_relational_value_rv268() {
    receive("RV268");
}
#[test]
fn source_relational_value_rv269() {
    receive("RV269");
}
#[test]
fn source_relational_value_rv270() {
    receive("RV270");
}
#[test]
fn source_relational_value_rv271() {
    receive("RV271");
}
#[test]
fn source_relational_value_rv272() {
    receive("RV272");
}
#[test]
fn source_relational_value_rv273() {
    receive("RV273");
}
#[test]
fn source_relational_value_rv274() {
    receive("RV274");
}
#[test]
fn source_relational_value_rv275() {
    receive("RV275");
}
#[test]
fn source_relational_value_rv276() {
    receive("RV276");
}
#[test]
fn source_relational_value_rv277() {
    receive("RV277");
}
#[test]
fn source_relational_value_rv278() {
    receive("RV278");
}
#[test]
fn source_relational_value_rv279() {
    receive("RV279");
}
#[test]
fn source_relational_value_rv280() {
    receive("RV280");
}
#[test]
fn source_relational_value_rv281() {
    receive("RV281");
}
#[test]
fn source_relational_value_rv282() {
    receive("RV282");
}
#[test]
fn source_relational_value_rv283() {
    receive("RV283");
}
#[test]
fn source_relational_value_rv284() {
    receive("RV284");
}
#[test]
fn source_relational_value_rv285() {
    receive("RV285");
}
#[test]
fn source_relational_value_rv286() {
    receive("RV286");
}
#[test]
fn source_relational_value_rv287() {
    receive("RV287");
}
#[test]
fn source_relational_value_rv288() {
    receive("RV288");
}
#[test]
fn source_relational_value_rv289() {
    receive("RV289");
}
#[test]
fn source_relational_value_rv290() {
    receive("RV290");
}
#[test]
fn source_relational_value_rv291() {
    receive("RV291");
}
#[test]
fn source_relational_value_rv292() {
    receive("RV292");
}
#[test]
fn source_relational_value_rv293() {
    receive("RV293");
}
#[test]
fn source_relational_value_rv294() {
    receive("RV294");
}
#[test]
fn source_relational_value_rv295() {
    receive("RV295");
}
#[test]
fn source_relational_value_rv296() {
    receive("RV296");
}
#[test]
fn source_relational_value_rv297() {
    receive("RV297");
}
#[test]
fn source_relational_value_rv298() {
    receive("RV298");
}
#[test]
fn source_relational_value_rv299() {
    receive("RV299");
}
#[test]
fn source_relational_value_rv300() {
    receive("RV300");
}
#[test]
fn source_relational_value_rv301() {
    receive("RV301");
}
#[test]
fn source_relational_value_rv302() {
    receive("RV302");
}
#[test]
fn source_relational_value_rv303() {
    receive("RV303");
}
#[test]
fn source_relational_value_rv304() {
    receive("RV304");
}
#[test]
fn source_relational_value_rv305() {
    receive("RV305");
}
#[test]
fn source_relational_value_rv306() {
    receive("RV306");
}
#[test]
fn source_relational_value_rv307() {
    receive("RV307");
}
#[test]
fn source_relational_value_rv308() {
    receive("RV308");
}
#[test]
fn source_relational_value_rv309() {
    receive("RV309");
}
#[test]
fn source_relational_value_rv310() {
    receive("RV310");
}
#[test]
fn source_relational_value_rv311() {
    receive("RV311");
}
#[test]
fn source_relational_value_rv312() {
    receive("RV312");
}
#[test]
fn source_relational_value_rv313() {
    receive("RV313");
}
#[test]
fn source_relational_value_rv314() {
    receive("RV314");
}
#[test]
fn source_relational_value_rv315() {
    receive("RV315");
}
#[test]
fn source_relational_value_rv316() {
    receive("RV316");
}
#[test]
fn source_relational_value_rv317() {
    receive("RV317");
}
#[test]
fn source_relational_value_rv318() {
    receive("RV318");
}
#[test]
fn source_relational_value_rv319() {
    receive("RV319");
}
#[test]
fn source_relational_value_rv320() {
    receive("RV320");
}
#[test]
fn source_relational_value_rv321() {
    receive("RV321");
}
#[test]
fn source_relational_value_rv322() {
    receive("RV322");
}
#[test]
fn source_relational_value_rv323() {
    receive("RV323");
}
#[test]
fn source_relational_value_rv324() {
    receive("RV324");
}
#[test]
fn source_relational_value_rv325() {
    receive("RV325");
}
#[test]
fn source_relational_value_rv326() {
    receive("RV326");
}
#[test]
fn source_relational_value_rv327() {
    receive("RV327");
}
#[test]
fn source_relational_value_rv328() {
    receive("RV328");
}
#[test]
fn source_relational_value_rv329() {
    receive("RV329");
}
#[test]
fn source_relational_value_rv330() {
    receive("RV330");
}
#[test]
fn source_relational_value_rv331() {
    receive("RV331");
}
#[test]
fn source_relational_value_rv332() {
    receive("RV332");
}
#[test]
fn source_relational_value_rv333() {
    receive("RV333");
}
#[test]
fn source_relational_value_rv334() {
    receive("RV334");
}
#[test]
fn source_relational_value_rv335() {
    receive("RV335");
}
#[test]
fn source_relational_value_rv336() {
    receive("RV336");
}
#[test]
fn source_relational_value_rv337() {
    receive("RV337");
}
#[test]
fn source_relational_value_rv338() {
    receive("RV338");
}
#[test]
fn source_relational_value_rv339() {
    receive("RV339");
}
#[test]
fn source_relational_value_rv340() {
    receive("RV340");
}
#[test]
fn source_relational_value_rv341() {
    receive("RV341");
}
#[test]
fn source_relational_value_rv342() {
    receive("RV342");
}
#[test]
fn source_relational_value_rv343() {
    receive("RV343");
}
#[test]
fn source_relational_value_rv344() {
    receive("RV344");
}
#[test]
fn source_relational_value_rv345() {
    receive("RV345");
}
#[test]
fn source_relational_value_rv346() {
    receive("RV346");
}
#[test]
fn source_relational_value_rv347() {
    receive("RV347");
}
#[test]
fn source_relational_value_rv348() {
    receive("RV348");
}
#[test]
fn source_relational_value_rv349() {
    receive("RV349");
}
#[test]
fn source_relational_value_rv350() {
    receive("RV350");
}
#[test]
fn source_relational_value_rv351() {
    receive("RV351");
}
#[test]
fn source_relational_value_rv352() {
    receive("RV352");
}
#[test]
fn source_relational_value_rv353() {
    receive("RV353");
}
#[test]
fn source_relational_value_rv354() {
    receive("RV354");
}
#[test]
fn source_relational_value_rv355() {
    receive("RV355");
}
#[test]
fn source_relational_value_rv356() {
    receive("RV356");
}
#[test]
fn source_relational_value_rv357() {
    receive("RV357");
}
#[test]
fn source_relational_value_rv358() {
    receive("RV358");
}
#[test]
fn source_relational_value_rv359() {
    receive("RV359");
}
#[test]
fn source_relational_value_rv360() {
    receive("RV360");
}

fn receive_negation(id: &str) {
    let bytes = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relation_negation_query_cases_01.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes.as_bytes())),
        "0694d90933067b26df1f19f1829466599e134bc4a0746f2db4e5fb5a960a6021"
    );
    let root: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 36);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}
#[test]
fn source_relational_value_rn001() {
    receive_negation("RN001");
}
#[test]
fn source_relational_value_rn002() {
    receive_negation("RN002");
}
#[test]
fn source_relational_value_rn003() {
    receive_negation("RN003");
}
#[test]
fn source_relational_value_rn004() {
    receive_negation("RN004");
}
#[test]
fn source_relational_value_rn005() {
    receive_negation("RN005");
}
#[test]
fn source_relational_value_rn006() {
    receive_negation("RN006");
}
#[test]
fn source_relational_value_rn007() {
    receive_negation("RN007");
}
#[test]
fn source_relational_value_rn008() {
    receive_negation("RN008");
}
#[test]
fn source_relational_value_rn009() {
    receive_negation("RN009");
}
#[test]
fn source_relational_value_rn010() {
    receive_negation("RN010");
}
#[test]
fn source_relational_value_rn011() {
    receive_negation("RN011");
}
#[test]
fn source_relational_value_rn012() {
    receive_negation("RN012");
}
#[test]
fn source_relational_value_rn013() {
    receive_negation("RN013");
}
#[test]
fn source_relational_value_rn014() {
    receive_negation("RN014");
}
#[test]
fn source_relational_value_rn015() {
    receive_negation("RN015");
}
#[test]
fn source_relational_value_rn016() {
    receive_negation("RN016");
}
#[test]
fn source_relational_value_rn017() {
    receive_negation("RN017");
}
#[test]
fn source_relational_value_rn018() {
    receive_negation("RN018");
}
#[test]
fn source_relational_value_rn019() {
    receive_negation("RN019");
}
#[test]
fn source_relational_value_rn020() {
    receive_negation("RN020");
}
#[test]
fn source_relational_value_rn021() {
    receive_negation("RN021");
}
#[test]
fn source_relational_value_rn022() {
    receive_negation("RN022");
}
#[test]
fn source_relational_value_rn023() {
    receive_negation("RN023");
}
#[test]
fn source_relational_value_rn024() {
    receive_negation("RN024");
}
#[test]
fn source_relational_value_rn025() {
    receive_negation("RN025");
}
#[test]
fn source_relational_value_rn026() {
    receive_negation("RN026");
}
#[test]
fn source_relational_value_rn027() {
    receive_negation("RN027");
}
#[test]
fn source_relational_value_rn028() {
    receive_negation("RN028");
}
#[test]
fn source_relational_value_rn029() {
    receive_negation("RN029");
}
#[test]
fn source_relational_value_rn030() {
    receive_negation("RN030");
}
#[test]
fn source_relational_value_rn031() {
    receive_negation("RN031");
}
#[test]
fn source_relational_value_rn032() {
    receive_negation("RN032");
}
#[test]
fn source_relational_value_rn033() {
    receive_negation("RN033");
}
#[test]
fn source_relational_value_rn034() {
    receive_negation("RN034");
}
#[test]
fn source_relational_value_rn035() {
    receive_negation("RN035");
}
#[test]
fn source_relational_value_rn036() {
    receive_negation("RN036");
}

#[test]
fn source_relational_value_constructor_admission() {
    use crate::numeric::NumericInitialValue;
    let session = source_bgp_receiving::session();
    for dt in [
        "urn:d",
        "http://www.w3.org/2001/XMLSchema#date",
        "http://www.w3.org/2001/XMLSchema#dayTimeDuration",
    ] {
        assert!(matches!(
            session.construct_initial(NumericInitialValue::SourceDirectString {
                lexical: "2020-01-01".to_owned(),
                datatype: Some(NamedNode::new(dt).unwrap()),
                language: None,
                normalize: false,
            }),
            Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                "source string datatype constructor"
            ))
        ));
    }
    for dt in [
        "http://www.w3.org/2001/XMLSchema#time",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral",
    ] {
        let result = numeric_query::prepare_source_graph_numeric_query(
            &format!("SELECT (\"x\"^^<{dt}> = \"y\"^^<{dt}> AS ?v) WHERE {{}}"),
            source_bgp_receiving::dataset(&session, None),
            Rc::clone(&session),
            Vec::new(),
        );
        let err = match result {
            Ok(_) => panic!("unreceived registered converter became opaque"),
            Err(err) => err,
        };
        assert!(format!("{err:?}").contains("query source datatype constructor"));
    }
}

#[test]
fn source_relational_value_unknown_metadata_rejected() {
    use crate::numeric::{
        LiteralSourceState, SourceDatatype, SourceFact, SourceLiteralCacheKind,
        SourceLiteralMetadata,
    };
    let session = source_bgp_receiving::session();
    let raw = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("2020-01-01", oxrdf::vocab::xsd::DATE).into(),
    );
    assert!(matches!(
        raw.literal_source(),
        LiteralSourceState::Literal(_)
    ));
    let left = SolutionValue::Rdf(raw.clone());
    for result in [
        session.source_relational_equal(&left, &left),
        session.source_relational_not_equal(&left, &left),
    ] {
        assert!(matches!(
            result,
            Err(crate::numeric::NumericEvaluationError::Fatal(
                RawNumericFault::UnknownSourceLiteralIdentity
            ))
        ));
    }
    assert!(matches!(
        raw.with_source_metadata(SourceLiteralMetadata {
            datatype: SourceDatatype::Specified(oxrdf::vocab::xsd::DATE.into_owned()),
            language_original: SourceFact::Known(None),
            ill_typed: SourceFact::Known(None),
            cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::UnmappedDatatype),
        }),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
}

#[test]
fn source_relational_value_not_equal_sealed_frontend() {
    let session = source_bgp_receiving::session();
    let query = numeric_query::prepare_source_frontend_numeric_query(
        "SELECT (1 != 2 AS ?v) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    let err = match query.execute_source_presence() {
        Ok(_) => panic!("source != escaped graph admission"),
        Err(err) => err,
    };
    assert_eq!(err.to_string(), "unsupported nullable expression");
}
