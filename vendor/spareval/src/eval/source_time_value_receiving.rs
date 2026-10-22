//! Frozen original registered time constructions and their actual consumers.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixtures() -> [(&'static str, &'static str); 4] {
    [
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_01.json"
            ),
            "66e76c5147ef953c5463d06a8da54e81374e21962f939558c597d3f32d54a9c9",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_02.json"
            ),
            "afb027b1fe66d96768c04cf1ae50ef995e9cbf1fc68c6441bdc4af91119b9695",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_03.json"
            ),
            "0fd8fb0e8156c5fe7da1be4ad414e2788271e7d31a3c792fa292a059cc61628e",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_04.json"
            ),
            "a10de0b9ac332a79cb2f26242718b089cf5645c57f144868b6d40476b4ede4eb",
        ),
    ]
}
fn receive(id: &str) {
    let index: usize = id[2..].parse().unwrap();
    let (bytes, digest) = private_fixtures()[(index - 1) / 78];
    assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), digest);
    let root: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 78);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case_with_time_values(
        case,
        &root["data_inputs"],
        true,
    );
}
#[test]
fn source_time_value_tv001() {
    receive("TV001");
}
#[test]
fn source_time_value_tv002() {
    receive("TV002");
}
#[test]
fn source_time_value_tv003() {
    receive("TV003");
}
#[test]
fn source_time_value_tv004() {
    receive("TV004");
}
#[test]
fn source_time_value_tv005() {
    receive("TV005");
}
#[test]
fn source_time_value_tv006() {
    receive("TV006");
}
#[test]
fn source_time_value_tv007() {
    receive("TV007");
}
#[test]
fn source_time_value_tv008() {
    receive("TV008");
}
#[test]
fn source_time_value_tv009() {
    receive("TV009");
}
#[test]
fn source_time_value_tv010() {
    receive("TV010");
}
#[test]
fn source_time_value_tv011() {
    receive("TV011");
}
#[test]
fn source_time_value_tv012() {
    receive("TV012");
}
#[test]
fn source_time_value_tv013() {
    receive("TV013");
}
#[test]
fn source_time_value_tv014() {
    receive("TV014");
}
#[test]
fn source_time_value_tv015() {
    receive("TV015");
}
#[test]
fn source_time_value_tv016() {
    receive("TV016");
}
#[test]
fn source_time_value_tv017() {
    receive("TV017");
}
#[test]
fn source_time_value_tv018() {
    receive("TV018");
}
#[test]
fn source_time_value_tv019() {
    receive("TV019");
}
#[test]
fn source_time_value_tv020() {
    receive("TV020");
}
#[test]
fn source_time_value_tv021() {
    receive("TV021");
}
#[test]
fn source_time_value_tv022() {
    receive("TV022");
}
#[test]
fn source_time_value_tv023() {
    receive("TV023");
}
#[test]
fn source_time_value_tv024() {
    receive("TV024");
}
#[test]
fn source_time_value_tv025() {
    receive("TV025");
}
#[test]
fn source_time_value_tv026() {
    receive("TV026");
}
#[test]
fn source_time_value_tv027() {
    receive("TV027");
}
#[test]
fn source_time_value_tv028() {
    receive("TV028");
}
#[test]
fn source_time_value_tv029() {
    receive("TV029");
}
#[test]
fn source_time_value_tv030() {
    receive("TV030");
}
#[test]
fn source_time_value_tv031() {
    receive("TV031");
}
#[test]
fn source_time_value_tv032() {
    receive("TV032");
}
#[test]
fn source_time_value_tv033() {
    receive("TV033");
}
#[test]
fn source_time_value_tv034() {
    receive("TV034");
}
#[test]
fn source_time_value_tv035() {
    receive("TV035");
}
#[test]
fn source_time_value_tv036() {
    receive("TV036");
}
#[test]
fn source_time_value_tv037() {
    receive("TV037");
}
#[test]
fn source_time_value_tv038() {
    receive("TV038");
}
#[test]
fn source_time_value_tv039() {
    receive("TV039");
}
#[test]
fn source_time_value_tv040() {
    receive("TV040");
}
#[test]
fn source_time_value_tv041() {
    receive("TV041");
}
#[test]
fn source_time_value_tv042() {
    receive("TV042");
}
#[test]
fn source_time_value_tv043() {
    receive("TV043");
}
#[test]
fn source_time_value_tv044() {
    receive("TV044");
}
#[test]
fn source_time_value_tv045() {
    receive("TV045");
}
#[test]
fn source_time_value_tv046() {
    receive("TV046");
}
#[test]
fn source_time_value_tv047() {
    receive("TV047");
}
#[test]
fn source_time_value_tv048() {
    receive("TV048");
}
#[test]
fn source_time_value_tv049() {
    receive("TV049");
}
#[test]
fn source_time_value_tv050() {
    receive("TV050");
}
#[test]
fn source_time_value_tv051() {
    receive("TV051");
}
#[test]
fn source_time_value_tv052() {
    receive("TV052");
}
#[test]
fn source_time_value_tv053() {
    receive("TV053");
}
#[test]
fn source_time_value_tv054() {
    receive("TV054");
}
#[test]
fn source_time_value_tv055() {
    receive("TV055");
}
#[test]
fn source_time_value_tv056() {
    receive("TV056");
}
#[test]
fn source_time_value_tv057() {
    receive("TV057");
}
#[test]
fn source_time_value_tv058() {
    receive("TV058");
}
#[test]
fn source_time_value_tv059() {
    receive("TV059");
}
#[test]
fn source_time_value_tv060() {
    receive("TV060");
}
#[test]
fn source_time_value_tv061() {
    receive("TV061");
}
#[test]
fn source_time_value_tv062() {
    receive("TV062");
}
#[test]
fn source_time_value_tv063() {
    receive("TV063");
}
#[test]
fn source_time_value_tv064() {
    receive("TV064");
}
#[test]
fn source_time_value_tv065() {
    receive("TV065");
}
#[test]
fn source_time_value_tv066() {
    receive("TV066");
}
#[test]
fn source_time_value_tv067() {
    receive("TV067");
}
#[test]
fn source_time_value_tv068() {
    receive("TV068");
}
#[test]
fn source_time_value_tv069() {
    receive("TV069");
}
#[test]
fn source_time_value_tv070() {
    receive("TV070");
}
#[test]
fn source_time_value_tv071() {
    receive("TV071");
}
#[test]
fn source_time_value_tv072() {
    receive("TV072");
}
#[test]
fn source_time_value_tv073() {
    receive("TV073");
}
#[test]
fn source_time_value_tv074() {
    receive("TV074");
}
#[test]
fn source_time_value_tv075() {
    receive("TV075");
}
#[test]
fn source_time_value_tv076() {
    receive("TV076");
}
#[test]
fn source_time_value_tv077() {
    receive("TV077");
}
#[test]
fn source_time_value_tv078() {
    receive("TV078");
}
#[test]
fn source_time_value_tv079() {
    receive("TV079");
}
#[test]
fn source_time_value_tv080() {
    receive("TV080");
}
#[test]
fn source_time_value_tv081() {
    receive("TV081");
}
#[test]
fn source_time_value_tv082() {
    receive("TV082");
}
#[test]
fn source_time_value_tv083() {
    receive("TV083");
}
#[test]
fn source_time_value_tv084() {
    receive("TV084");
}
#[test]
fn source_time_value_tv085() {
    receive("TV085");
}
#[test]
fn source_time_value_tv086() {
    receive("TV086");
}
#[test]
fn source_time_value_tv087() {
    receive("TV087");
}
#[test]
fn source_time_value_tv088() {
    receive("TV088");
}
#[test]
fn source_time_value_tv089() {
    receive("TV089");
}
#[test]
fn source_time_value_tv090() {
    receive("TV090");
}
#[test]
fn source_time_value_tv091() {
    receive("TV091");
}
#[test]
fn source_time_value_tv092() {
    receive("TV092");
}
#[test]
fn source_time_value_tv093() {
    receive("TV093");
}
#[test]
fn source_time_value_tv094() {
    receive("TV094");
}
#[test]
fn source_time_value_tv095() {
    receive("TV095");
}
#[test]
fn source_time_value_tv096() {
    receive("TV096");
}
#[test]
fn source_time_value_tv097() {
    receive("TV097");
}
#[test]
fn source_time_value_tv098() {
    receive("TV098");
}
#[test]
fn source_time_value_tv099() {
    receive("TV099");
}
#[test]
fn source_time_value_tv100() {
    receive("TV100");
}
#[test]
fn source_time_value_tv101() {
    receive("TV101");
}
#[test]
fn source_time_value_tv102() {
    receive("TV102");
}
#[test]
fn source_time_value_tv103() {
    receive("TV103");
}
#[test]
fn source_time_value_tv104() {
    receive("TV104");
}
#[test]
fn source_time_value_tv105() {
    receive("TV105");
}
#[test]
fn source_time_value_tv106() {
    receive("TV106");
}
#[test]
fn source_time_value_tv107() {
    receive("TV107");
}
#[test]
fn source_time_value_tv108() {
    receive("TV108");
}
#[test]
fn source_time_value_tv109() {
    receive("TV109");
}
#[test]
fn source_time_value_tv110() {
    receive("TV110");
}
#[test]
fn source_time_value_tv111() {
    receive("TV111");
}
#[test]
fn source_time_value_tv112() {
    receive("TV112");
}
#[test]
fn source_time_value_tv113() {
    receive("TV113");
}
#[test]
fn source_time_value_tv114() {
    receive("TV114");
}
#[test]
fn source_time_value_tv115() {
    receive("TV115");
}
#[test]
fn source_time_value_tv116() {
    receive("TV116");
}
#[test]
fn source_time_value_tv117() {
    receive("TV117");
}
#[test]
fn source_time_value_tv118() {
    receive("TV118");
}
#[test]
fn source_time_value_tv119() {
    receive("TV119");
}
#[test]
fn source_time_value_tv120() {
    receive("TV120");
}
#[test]
fn source_time_value_tv121() {
    receive("TV121");
}
#[test]
fn source_time_value_tv122() {
    receive("TV122");
}
#[test]
fn source_time_value_tv123() {
    receive("TV123");
}
#[test]
fn source_time_value_tv124() {
    receive("TV124");
}
#[test]
fn source_time_value_tv125() {
    receive("TV125");
}
#[test]
fn source_time_value_tv126() {
    receive("TV126");
}
#[test]
fn source_time_value_tv127() {
    receive("TV127");
}
#[test]
fn source_time_value_tv128() {
    receive("TV128");
}
#[test]
fn source_time_value_tv129() {
    receive("TV129");
}
#[test]
fn source_time_value_tv130() {
    receive("TV130");
}
#[test]
fn source_time_value_tv131() {
    receive("TV131");
}
#[test]
fn source_time_value_tv132() {
    receive("TV132");
}
#[test]
fn source_time_value_tv133() {
    receive("TV133");
}
#[test]
fn source_time_value_tv134() {
    receive("TV134");
}
#[test]
fn source_time_value_tv135() {
    receive("TV135");
}
#[test]
fn source_time_value_tv136() {
    receive("TV136");
}
#[test]
fn source_time_value_tv137() {
    receive("TV137");
}
#[test]
fn source_time_value_tv138() {
    receive("TV138");
}
#[test]
fn source_time_value_tv139() {
    receive("TV139");
}
#[test]
fn source_time_value_tv140() {
    receive("TV140");
}
#[test]
fn source_time_value_tv141() {
    receive("TV141");
}
#[test]
fn source_time_value_tv142() {
    receive("TV142");
}
#[test]
fn source_time_value_tv143() {
    receive("TV143");
}
#[test]
fn source_time_value_tv144() {
    receive("TV144");
}
#[test]
fn source_time_value_tv145() {
    receive("TV145");
}
#[test]
fn source_time_value_tv146() {
    receive("TV146");
}
#[test]
fn source_time_value_tv147() {
    receive("TV147");
}
#[test]
fn source_time_value_tv148() {
    receive("TV148");
}
#[test]
fn source_time_value_tv149() {
    receive("TV149");
}
#[test]
fn source_time_value_tv150() {
    receive("TV150");
}
#[test]
fn source_time_value_tv151() {
    receive("TV151");
}
#[test]
fn source_time_value_tv152() {
    receive("TV152");
}
#[test]
fn source_time_value_tv153() {
    receive("TV153");
}
#[test]
fn source_time_value_tv154() {
    receive("TV154");
}
#[test]
fn source_time_value_tv155() {
    receive("TV155");
}
#[test]
fn source_time_value_tv156() {
    receive("TV156");
}
#[test]
fn source_time_value_tv157() {
    receive("TV157");
}
#[test]
fn source_time_value_tv158() {
    receive("TV158");
}
#[test]
fn source_time_value_tv159() {
    receive("TV159");
}
#[test]
fn source_time_value_tv160() {
    receive("TV160");
}
#[test]
fn source_time_value_tv161() {
    receive("TV161");
}
#[test]
fn source_time_value_tv162() {
    receive("TV162");
}
#[test]
fn source_time_value_tv163() {
    receive("TV163");
}
#[test]
fn source_time_value_tv164() {
    receive("TV164");
}
#[test]
fn source_time_value_tv165() {
    receive("TV165");
}
#[test]
fn source_time_value_tv166() {
    receive("TV166");
}
#[test]
fn source_time_value_tv167() {
    receive("TV167");
}
#[test]
fn source_time_value_tv168() {
    receive("TV168");
}
#[test]
fn source_time_value_tv169() {
    receive("TV169");
}
#[test]
fn source_time_value_tv170() {
    receive("TV170");
}
#[test]
fn source_time_value_tv171() {
    receive("TV171");
}
#[test]
fn source_time_value_tv172() {
    receive("TV172");
}
#[test]
fn source_time_value_tv173() {
    receive("TV173");
}
#[test]
fn source_time_value_tv174() {
    receive("TV174");
}
#[test]
fn source_time_value_tv175() {
    receive("TV175");
}
#[test]
fn source_time_value_tv176() {
    receive("TV176");
}
#[test]
fn source_time_value_tv177() {
    receive("TV177");
}
#[test]
fn source_time_value_tv178() {
    receive("TV178");
}
#[test]
fn source_time_value_tv179() {
    receive("TV179");
}
#[test]
fn source_time_value_tv180() {
    receive("TV180");
}
#[test]
fn source_time_value_tv181() {
    receive("TV181");
}
#[test]
fn source_time_value_tv182() {
    receive("TV182");
}
#[test]
fn source_time_value_tv183() {
    receive("TV183");
}
#[test]
fn source_time_value_tv184() {
    receive("TV184");
}
#[test]
fn source_time_value_tv185() {
    receive("TV185");
}
#[test]
fn source_time_value_tv186() {
    receive("TV186");
}
#[test]
fn source_time_value_tv187() {
    receive("TV187");
}
#[test]
fn source_time_value_tv188() {
    receive("TV188");
}
#[test]
fn source_time_value_tv189() {
    receive("TV189");
}
#[test]
fn source_time_value_tv190() {
    receive("TV190");
}
#[test]
fn source_time_value_tv191() {
    receive("TV191");
}
#[test]
fn source_time_value_tv192() {
    receive("TV192");
}
#[test]
fn source_time_value_tv193() {
    receive("TV193");
}
#[test]
fn source_time_value_tv194() {
    receive("TV194");
}
#[test]
fn source_time_value_tv195() {
    receive("TV195");
}
#[test]
fn source_time_value_tv196() {
    receive("TV196");
}
#[test]
fn source_time_value_tv197() {
    receive("TV197");
}
#[test]
fn source_time_value_tv198() {
    receive("TV198");
}
#[test]
fn source_time_value_tv199() {
    receive("TV199");
}
#[test]
fn source_time_value_tv200() {
    receive("TV200");
}
#[test]
fn source_time_value_tv201() {
    receive("TV201");
}
#[test]
fn source_time_value_tv202() {
    receive("TV202");
}
#[test]
fn source_time_value_tv203() {
    receive("TV203");
}
#[test]
fn source_time_value_tv204() {
    receive("TV204");
}
#[test]
fn source_time_value_tv205() {
    receive("TV205");
}
#[test]
fn source_time_value_tv206() {
    receive("TV206");
}
#[test]
fn source_time_value_tv207() {
    receive("TV207");
}
#[test]
fn source_time_value_tv208() {
    receive("TV208");
}
#[test]
fn source_time_value_tv209() {
    receive("TV209");
}
#[test]
fn source_time_value_tv210() {
    receive("TV210");
}
#[test]
fn source_time_value_tv211() {
    receive("TV211");
}
#[test]
fn source_time_value_tv212() {
    receive("TV212");
}
#[test]
fn source_time_value_tv213() {
    receive("TV213");
}
#[test]
fn source_time_value_tv214() {
    receive("TV214");
}
#[test]
fn source_time_value_tv215() {
    receive("TV215");
}
#[test]
fn source_time_value_tv216() {
    receive("TV216");
}
#[test]
fn source_time_value_tv217() {
    receive("TV217");
}
#[test]
fn source_time_value_tv218() {
    receive("TV218");
}
#[test]
fn source_time_value_tv219() {
    receive("TV219");
}
#[test]
fn source_time_value_tv220() {
    receive("TV220");
}
#[test]
fn source_time_value_tv221() {
    receive("TV221");
}
#[test]
fn source_time_value_tv222() {
    receive("TV222");
}
#[test]
fn source_time_value_tv223() {
    receive("TV223");
}
#[test]
fn source_time_value_tv224() {
    receive("TV224");
}
#[test]
fn source_time_value_tv225() {
    receive("TV225");
}
#[test]
fn source_time_value_tv226() {
    receive("TV226");
}
#[test]
fn source_time_value_tv227() {
    receive("TV227");
}
#[test]
fn source_time_value_tv228() {
    receive("TV228");
}
#[test]
fn source_time_value_tv229() {
    receive("TV229");
}
#[test]
fn source_time_value_tv230() {
    receive("TV230");
}
#[test]
fn source_time_value_tv231() {
    receive("TV231");
}
#[test]
fn source_time_value_tv232() {
    receive("TV232");
}
#[test]
fn source_time_value_tv233() {
    receive("TV233");
}
#[test]
fn source_time_value_tv234() {
    receive("TV234");
}
#[test]
fn source_time_value_tv235() {
    receive("TV235");
}
#[test]
fn source_time_value_tv236() {
    receive("TV236");
}
#[test]
fn source_time_value_tv237() {
    receive("TV237");
}
#[test]
fn source_time_value_tv238() {
    receive("TV238");
}
#[test]
fn source_time_value_tv239() {
    receive("TV239");
}
#[test]
fn source_time_value_tv240() {
    receive("TV240");
}
#[test]
fn source_time_value_tv241() {
    receive("TV241");
}
#[test]
fn source_time_value_tv242() {
    receive("TV242");
}
#[test]
fn source_time_value_tv243() {
    receive("TV243");
}
#[test]
fn source_time_value_tv244() {
    receive("TV244");
}
#[test]
fn source_time_value_tv245() {
    receive("TV245");
}
#[test]
fn source_time_value_tv246() {
    receive("TV246");
}
#[test]
fn source_time_value_tv247() {
    receive("TV247");
}
#[test]
fn source_time_value_tv248() {
    receive("TV248");
}
#[test]
fn source_time_value_tv249() {
    receive("TV249");
}
#[test]
fn source_time_value_tv250() {
    receive("TV250");
}
#[test]
fn source_time_value_tv251() {
    receive("TV251");
}
#[test]
fn source_time_value_tv252() {
    receive("TV252");
}
#[test]
fn source_time_value_tv253() {
    receive("TV253");
}
#[test]
fn source_time_value_tv254() {
    receive("TV254");
}
#[test]
fn source_time_value_tv255() {
    receive("TV255");
}
#[test]
fn source_time_value_tv256() {
    receive("TV256");
}
#[test]
fn source_time_value_tv257() {
    receive("TV257");
}
#[test]
fn source_time_value_tv258() {
    receive("TV258");
}
#[test]
fn source_time_value_tv259() {
    receive("TV259");
}
#[test]
fn source_time_value_tv260() {
    receive("TV260");
}
#[test]
fn source_time_value_tv261() {
    receive("TV261");
}
#[test]
fn source_time_value_tv262() {
    receive("TV262");
}
#[test]
fn source_time_value_tv263() {
    receive("TV263");
}
#[test]
fn source_time_value_tv264() {
    receive("TV264");
}
#[test]
fn source_time_value_tv265() {
    receive("TV265");
}
#[test]
fn source_time_value_tv266() {
    receive("TV266");
}
#[test]
fn source_time_value_tv267() {
    receive("TV267");
}
#[test]
fn source_time_value_tv268() {
    receive("TV268");
}
#[test]
fn source_time_value_tv269() {
    receive("TV269");
}
#[test]
fn source_time_value_tv270() {
    receive("TV270");
}
#[test]
fn source_time_value_tv271() {
    receive("TV271");
}
#[test]
fn source_time_value_tv272() {
    receive("TV272");
}
#[test]
fn source_time_value_tv273() {
    receive("TV273");
}
#[test]
fn source_time_value_tv274() {
    receive("TV274");
}
#[test]
fn source_time_value_tv275() {
    receive("TV275");
}
#[test]
fn source_time_value_tv276() {
    receive("TV276");
}
#[test]
fn source_time_value_tv277() {
    receive("TV277");
}
#[test]
fn source_time_value_tv278() {
    receive("TV278");
}
#[test]
fn source_time_value_tv279() {
    receive("TV279");
}
#[test]
fn source_time_value_tv280() {
    receive("TV280");
}
#[test]
fn source_time_value_tv281() {
    receive("TV281");
}
#[test]
fn source_time_value_tv282() {
    receive("TV282");
}
#[test]
fn source_time_value_tv283() {
    receive("TV283");
}
#[test]
fn source_time_value_tv284() {
    receive("TV284");
}
#[test]
fn source_time_value_tv285() {
    receive("TV285");
}
#[test]
fn source_time_value_tv286() {
    receive("TV286");
}
#[test]
fn source_time_value_tv287() {
    receive("TV287");
}
#[test]
fn source_time_value_tv288() {
    receive("TV288");
}
#[test]
fn source_time_value_tv289() {
    receive("TV289");
}
#[test]
fn source_time_value_tv290() {
    receive("TV290");
}
#[test]
fn source_time_value_tv291() {
    receive("TV291");
}
#[test]
fn source_time_value_tv292() {
    receive("TV292");
}
#[test]
fn source_time_value_tv293() {
    receive("TV293");
}
#[test]
fn source_time_value_tv294() {
    receive("TV294");
}
#[test]
fn source_time_value_tv295() {
    receive("TV295");
}
#[test]
fn source_time_value_tv296() {
    receive("TV296");
}
#[test]
fn source_time_value_tv297() {
    receive("TV297");
}
#[test]
fn source_time_value_tv298() {
    receive("TV298");
}
#[test]
fn source_time_value_tv299() {
    receive("TV299");
}
#[test]
fn source_time_value_tv300() {
    receive("TV300");
}
#[test]
fn source_time_value_tv301() {
    receive("TV301");
}
#[test]
fn source_time_value_tv302() {
    receive("TV302");
}
#[test]
fn source_time_value_tv303() {
    receive("TV303");
}
#[test]
fn source_time_value_tv304() {
    receive("TV304");
}
#[test]
fn source_time_value_tv305() {
    receive("TV305");
}
#[test]
fn source_time_value_tv306() {
    receive("TV306");
}
#[test]
fn source_time_value_tv307() {
    receive("TV307");
}
#[test]
fn source_time_value_tv308() {
    receive("TV308");
}
#[test]
fn source_time_value_tv309() {
    receive("TV309");
}
#[test]
fn source_time_value_tv310() {
    receive("TV310");
}
#[test]
fn source_time_value_tv311() {
    receive("TV311");
}
#[test]
fn source_time_value_tv312() {
    receive("TV312");
}

fn time_input(text: &str, normalize: bool) -> crate::numeric::NumericInitialValue {
    crate::numeric::NumericInitialValue::SourceDirectString {
        lexical: text.into(),
        datatype: Some(oxrdf::vocab::xsd::TIME.into_owned()),
        language: None,
        normalize,
    }
}
fn time_session(limits: semantica_numeric::NumericLimits) -> Rc<QueryNumericSession> {
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
        .unwrap()
        .with_source_time_values_v1()
        .unwrap(),
    )
}
#[test]
fn source_time_value_metadata() {
    use crate::numeric::*;
    let session = time_session(semantica_numeric::NumericLimits::default());
    for (text, valid) in [("00", true), ("24:00", false)] {
        let raw = session
            .construct_source_graph_initial(time_input(text, true))
            .unwrap();
        let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
            panic!("literal required")
        };
        assert_eq!(metadata.ill_typed, SourceFact::Known(Some(!valid)));
        let mut wrong = metadata.clone();
        wrong.ill_typed = SourceFact::Known(Some(valid));
        assert!(matches!(
            raw.clone().with_source_metadata(wrong),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
        let mut wrong = metadata.clone();
        wrong.cached_value_kind = SourceFact::Known(SourceLiteralCacheKind::Time(
            semantica_temporal::parse_time("23"),
        ));
        assert!(matches!(
            raw.clone().with_source_metadata(wrong),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
        let unknown = RawExpressionValue::from_import(raw.rdf().unwrap().clone());
        assert!(
            session
                .source_datatype(&SolutionValue::Rdf(unknown))
                .is_err()
        );
        let first = session
            .source_datatype(&SolutionValue::Rdf(raw.clone()))
            .unwrap();
        let second = session.source_datatype(&SolutionValue::Rdf(raw)).unwrap();
        assert!(first.same_source_object(&second));
    }
}
#[test]
fn source_time_value_resources() {
    use crate::numeric::*;
    use semantica_numeric::{NumericFault, NumericLimits};
    for (limits, text, reason) in [
        (
            NumericLimits {
                max_input_characters: 8,
                ..NumericLimits::default()
            },
            "12:34:56.123456",
            "input_characters",
        ),
        (
            NumericLimits {
                max_output_characters: 7,
                ..NumericLimits::default()
            },
            "12",
            "output_characters",
        ),
    ] {
        let result = time_session(limits).construct_source_graph_initial(time_input(text, true));
        assert!(
            matches!(result, Err(RawNumericFault::Core(NumericFault::ResourceLimit { operation })) if operation == reason),
            "{result:?}"
        );
    }
    let session = time_session(NumericLimits::default());
    let mut input = time_input("12", true);
    if let NumericInitialValue::SourceDirectString { language, .. } = &mut input {
        *language = Some("EN".into());
    }
    assert!(matches!(
        session.construct_source_graph_initial(input),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
}
#[test]
fn source_time_value_closed_and_order() {
    use crate::numeric::*;
    use semantica_numeric::{DecimalContext, NumericLimits};
    let legacy = QueryNumericSession::new(
        DecimalContext::default(),
        NumericLimits::default(),
        None,
        true,
    )
    .unwrap();
    assert!(legacy.with_source_time_values_v1().is_err());
    let previous = source_bgp_receiving::session();
    assert!(matches!(
        previous.construct_source_graph_initial(time_input("12", false)),
        Err(RawNumericFault::UnsupportedSourceLiteralOperator(
            "source string datatype constructor"
        ))
    ));
    let query = numeric_query::prepare_source_graph_numeric_query(
        r#"SELECT ("12"^^<http://www.w3.org/2001/XMLSchema#time> AS ?v) WHERE {}"#,
        source_bgp_receiving::dataset(&previous, None),
        previous,
        Vec::new(),
    );
    assert!(matches!(
        query,
        Err(numeric_query::NumericPrepareError::LiteralConstruction(
            RawNumericFault::UnsupportedSourceLiteralOperator("query source datatype constructor")
        ))
    ));
    let session = time_session(NumericLimits::default());
    assert!(session.construct_initial(time_input("12", false)).is_err());
    let a = session
        .construct_source_graph_initial(time_input("12", false))
        .unwrap();
    let b = session
        .construct_source_graph_initial(time_input("13", false))
        .unwrap();
    assert!(
        session
            .source_order_key_less(&SourceOrderKey::Rdf(a), &SourceOrderKey::Rdf(b))
            .is_err()
    );
}
