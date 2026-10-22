//! Frozen original six-operator comparison, trap and callback observations.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixtures() -> [(&'static str, &'static str); 4] {
    [
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_01.json"
            ),
            "a0d4fe94e80a1efcae7022acf59cca24ef36baa9f7f8359dc131c5f095e5b65a",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_02.json"
            ),
            "1a7f52d3f216e541a48a3ce97067fd6a638945b1da93c3c330b640a7a2083467",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_03.json"
            ),
            "159df01d155e6aeed67aa7bcdfe7fbe03486a1031946b8abfb41cf5535563114",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_04.json"
            ),
            "b5baa050e615ee6bd5a3b3e0c2dbabf4e1b32f26d6d8acf33a8e47338475a31f",
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
fn source_comparison_exception_cx001() {
    receive("CX001");
}

#[test]
fn source_comparison_exception_cx002() {
    receive("CX002");
}

#[test]
fn source_comparison_exception_cx003() {
    receive("CX003");
}

#[test]
fn source_comparison_exception_cx004() {
    receive("CX004");
}

#[test]
fn source_comparison_exception_cx005() {
    receive("CX005");
}

#[test]
fn source_comparison_exception_cx006() {
    receive("CX006");
}

#[test]
fn source_comparison_exception_cx007() {
    receive("CX007");
}

#[test]
fn source_comparison_exception_cx008() {
    receive("CX008");
}

#[test]
fn source_comparison_exception_cx009() {
    receive("CX009");
}

#[test]
fn source_comparison_exception_cx010() {
    receive("CX010");
}

#[test]
fn source_comparison_exception_cx011() {
    receive("CX011");
}

#[test]
fn source_comparison_exception_cx012() {
    receive("CX012");
}

#[test]
fn source_comparison_exception_cx013() {
    receive("CX013");
}

#[test]
fn source_comparison_exception_cx014() {
    receive("CX014");
}

#[test]
fn source_comparison_exception_cx015() {
    receive("CX015");
}

#[test]
fn source_comparison_exception_cx016() {
    receive("CX016");
}

#[test]
fn source_comparison_exception_cx017() {
    receive("CX017");
}

#[test]
fn source_comparison_exception_cx018() {
    receive("CX018");
}

#[test]
fn source_comparison_exception_cx019() {
    receive("CX019");
}

#[test]
fn source_comparison_exception_cx020() {
    receive("CX020");
}

#[test]
fn source_comparison_exception_cx021() {
    receive("CX021");
}

#[test]
fn source_comparison_exception_cx022() {
    receive("CX022");
}

#[test]
fn source_comparison_exception_cx023() {
    receive("CX023");
}

#[test]
fn source_comparison_exception_cx024() {
    receive("CX024");
}

#[test]
fn source_comparison_exception_cx025() {
    receive("CX025");
}

#[test]
fn source_comparison_exception_cx026() {
    receive("CX026");
}

#[test]
fn source_comparison_exception_cx027() {
    receive("CX027");
}

#[test]
fn source_comparison_exception_cx028() {
    receive("CX028");
}

#[test]
fn source_comparison_exception_cx029() {
    receive("CX029");
}

#[test]
fn source_comparison_exception_cx030() {
    receive("CX030");
}

#[test]
fn source_comparison_exception_cx031() {
    receive("CX031");
}

#[test]
fn source_comparison_exception_cx032() {
    receive("CX032");
}

#[test]
fn source_comparison_exception_cx033() {
    receive("CX033");
}

#[test]
fn source_comparison_exception_cx034() {
    receive("CX034");
}

#[test]
fn source_comparison_exception_cx035() {
    receive("CX035");
}

#[test]
fn source_comparison_exception_cx036() {
    receive("CX036");
}

#[test]
fn source_comparison_exception_cx037() {
    receive("CX037");
}

#[test]
fn source_comparison_exception_cx038() {
    receive("CX038");
}

#[test]
fn source_comparison_exception_cx039() {
    receive("CX039");
}

#[test]
fn source_comparison_exception_cx040() {
    receive("CX040");
}

#[test]
fn source_comparison_exception_cx041() {
    receive("CX041");
}

#[test]
fn source_comparison_exception_cx042() {
    receive("CX042");
}

#[test]
fn source_comparison_exception_cx043() {
    receive("CX043");
}

#[test]
fn source_comparison_exception_cx044() {
    receive("CX044");
}

#[test]
fn source_comparison_exception_cx045() {
    receive("CX045");
}

#[test]
fn source_comparison_exception_cx046() {
    receive("CX046");
}

#[test]
fn source_comparison_exception_cx047() {
    receive("CX047");
}

#[test]
fn source_comparison_exception_cx048() {
    receive("CX048");
}

#[test]
fn source_comparison_exception_cx049() {
    receive("CX049");
}

#[test]
fn source_comparison_exception_cx050() {
    receive("CX050");
}

#[test]
fn source_comparison_exception_cx051() {
    receive("CX051");
}

#[test]
fn source_comparison_exception_cx052() {
    receive("CX052");
}

#[test]
fn source_comparison_exception_cx053() {
    receive("CX053");
}

#[test]
fn source_comparison_exception_cx054() {
    receive("CX054");
}

#[test]
fn source_comparison_exception_cx055() {
    receive("CX055");
}

#[test]
fn source_comparison_exception_cx056() {
    receive("CX056");
}

#[test]
fn source_comparison_exception_cx057() {
    receive("CX057");
}

#[test]
fn source_comparison_exception_cx058() {
    receive("CX058");
}

#[test]
fn source_comparison_exception_cx059() {
    receive("CX059");
}

#[test]
fn source_comparison_exception_cx060() {
    receive("CX060");
}

#[test]
fn source_comparison_exception_cx061() {
    receive("CX061");
}

#[test]
fn source_comparison_exception_cx062() {
    receive("CX062");
}

#[test]
fn source_comparison_exception_cx063() {
    receive("CX063");
}

#[test]
fn source_comparison_exception_cx064() {
    receive("CX064");
}

#[test]
fn source_comparison_exception_cx065() {
    receive("CX065");
}

#[test]
fn source_comparison_exception_cx066() {
    receive("CX066");
}

#[test]
fn source_comparison_exception_cx067() {
    receive("CX067");
}

#[test]
fn source_comparison_exception_cx068() {
    receive("CX068");
}

#[test]
fn source_comparison_exception_cx069() {
    receive("CX069");
}

#[test]
fn source_comparison_exception_cx070() {
    receive("CX070");
}

#[test]
fn source_comparison_exception_cx071() {
    receive("CX071");
}

#[test]
fn source_comparison_exception_cx072() {
    receive("CX072");
}

#[test]
fn source_comparison_exception_cx073() {
    receive("CX073");
}

#[test]
fn source_comparison_exception_cx074() {
    receive("CX074");
}

#[test]
fn source_comparison_exception_cx075() {
    receive("CX075");
}

#[test]
fn source_comparison_exception_cx076() {
    receive("CX076");
}

#[test]
fn source_comparison_exception_cx077() {
    receive("CX077");
}

#[test]
fn source_comparison_exception_cx078() {
    receive("CX078");
}

#[test]
fn source_comparison_exception_cx079() {
    receive("CX079");
}

#[test]
fn source_comparison_exception_cx080() {
    receive("CX080");
}

#[test]
fn source_comparison_exception_cx081() {
    receive("CX081");
}

#[test]
fn source_comparison_exception_cx082() {
    receive("CX082");
}

#[test]
fn source_comparison_exception_cx083() {
    receive("CX083");
}

#[test]
fn source_comparison_exception_cx084() {
    receive("CX084");
}

#[test]
fn source_comparison_exception_cx085() {
    receive("CX085");
}

#[test]
fn source_comparison_exception_cx086() {
    receive("CX086");
}

#[test]
fn source_comparison_exception_cx087() {
    receive("CX087");
}

#[test]
fn source_comparison_exception_cx088() {
    receive("CX088");
}

#[test]
fn source_comparison_exception_cx089() {
    receive("CX089");
}

#[test]
fn source_comparison_exception_cx090() {
    receive("CX090");
}

#[test]
fn source_comparison_exception_cx091() {
    receive("CX091");
}

#[test]
fn source_comparison_exception_cx092() {
    receive("CX092");
}

#[test]
fn source_comparison_exception_cx093() {
    receive("CX093");
}

#[test]
fn source_comparison_exception_cx094() {
    receive("CX094");
}

#[test]
fn source_comparison_exception_cx095() {
    receive("CX095");
}

#[test]
fn source_comparison_exception_cx096() {
    receive("CX096");
}

#[test]
fn source_comparison_exception_cx097() {
    receive("CX097");
}

#[test]
fn source_comparison_exception_cx098() {
    receive("CX098");
}

#[test]
fn source_comparison_exception_cx099() {
    receive("CX099");
}

#[test]
fn source_comparison_exception_cx100() {
    receive("CX100");
}

#[test]
fn source_comparison_exception_cx101() {
    receive("CX101");
}

#[test]
fn source_comparison_exception_cx102() {
    receive("CX102");
}

#[test]
fn source_comparison_exception_cx103() {
    receive("CX103");
}

#[test]
fn source_comparison_exception_cx104() {
    receive("CX104");
}

#[test]
fn source_comparison_exception_cx105() {
    receive("CX105");
}

#[test]
fn source_comparison_exception_cx106() {
    receive("CX106");
}

#[test]
fn source_comparison_exception_cx107() {
    receive("CX107");
}

#[test]
fn source_comparison_exception_cx108() {
    receive("CX108");
}

#[test]
fn source_comparison_exception_cx109() {
    receive("CX109");
}

#[test]
fn source_comparison_exception_cx110() {
    receive("CX110");
}

#[test]
fn source_comparison_exception_cx111() {
    receive("CX111");
}

#[test]
fn source_comparison_exception_cx112() {
    receive("CX112");
}

#[test]
fn source_comparison_exception_cx113() {
    receive("CX113");
}

#[test]
fn source_comparison_exception_cx114() {
    receive("CX114");
}

#[test]
fn source_comparison_exception_cx115() {
    receive("CX115");
}

#[test]
fn source_comparison_exception_cx116() {
    receive("CX116");
}

#[test]
fn source_comparison_exception_cx117() {
    receive("CX117");
}

#[test]
fn source_comparison_exception_cx118() {
    receive("CX118");
}

#[test]
fn source_comparison_exception_cx119() {
    receive("CX119");
}

#[test]
fn source_comparison_exception_cx120() {
    receive("CX120");
}

#[test]
fn source_comparison_exception_cx121() {
    receive("CX121");
}

#[test]
fn source_comparison_exception_cx122() {
    receive("CX122");
}

#[test]
fn source_comparison_exception_cx123() {
    receive("CX123");
}

#[test]
fn source_comparison_exception_cx124() {
    receive("CX124");
}

#[test]
fn source_comparison_exception_cx125() {
    receive("CX125");
}

#[test]
fn source_comparison_exception_cx126() {
    receive("CX126");
}

#[test]
fn source_comparison_exception_cx127() {
    receive("CX127");
}

#[test]
fn source_comparison_exception_cx128() {
    receive("CX128");
}

#[test]
fn source_comparison_exception_cx129() {
    receive("CX129");
}

#[test]
fn source_comparison_exception_cx130() {
    receive("CX130");
}

#[test]
fn source_comparison_exception_cx131() {
    receive("CX131");
}

#[test]
fn source_comparison_exception_cx132() {
    receive("CX132");
}

#[test]
fn source_comparison_exception_cx133() {
    receive("CX133");
}

#[test]
fn source_comparison_exception_cx134() {
    receive("CX134");
}

#[test]
fn source_comparison_exception_cx135() {
    receive("CX135");
}

#[test]
fn source_comparison_exception_cx136() {
    receive("CX136");
}

#[test]
fn source_comparison_exception_cx137() {
    receive("CX137");
}

#[test]
fn source_comparison_exception_cx138() {
    receive("CX138");
}

#[test]
fn source_comparison_exception_cx139() {
    receive("CX139");
}

#[test]
fn source_comparison_exception_cx140() {
    receive("CX140");
}

#[test]
fn source_comparison_exception_cx141() {
    receive("CX141");
}

#[test]
fn source_comparison_exception_cx142() {
    receive("CX142");
}

#[test]
fn source_comparison_exception_cx143() {
    receive("CX143");
}

#[test]
fn source_comparison_exception_cx144() {
    receive("CX144");
}

#[test]
fn source_comparison_exception_cx145() {
    receive("CX145");
}

#[test]
fn source_comparison_exception_cx146() {
    receive("CX146");
}

#[test]
fn source_comparison_exception_cx147() {
    receive("CX147");
}

#[test]
fn source_comparison_exception_cx148() {
    receive("CX148");
}

#[test]
fn source_comparison_exception_cx149() {
    receive("CX149");
}

#[test]
fn source_comparison_exception_cx150() {
    receive("CX150");
}

#[test]
fn source_comparison_exception_cx151() {
    receive("CX151");
}

#[test]
fn source_comparison_exception_cx152() {
    receive("CX152");
}

#[test]
fn source_comparison_exception_cx153() {
    receive("CX153");
}

#[test]
fn source_comparison_exception_cx154() {
    receive("CX154");
}

#[test]
fn source_comparison_exception_cx155() {
    receive("CX155");
}

#[test]
fn source_comparison_exception_cx156() {
    receive("CX156");
}

#[test]
fn source_comparison_exception_cx157() {
    receive("CX157");
}

#[test]
fn source_comparison_exception_cx158() {
    receive("CX158");
}

#[test]
fn source_comparison_exception_cx159() {
    receive("CX159");
}

#[test]
fn source_comparison_exception_cx160() {
    receive("CX160");
}

#[test]
fn source_comparison_exception_cx161() {
    receive("CX161");
}

#[test]
fn source_comparison_exception_cx162() {
    receive("CX162");
}

#[test]
fn source_comparison_exception_cx163() {
    receive("CX163");
}

#[test]
fn source_comparison_exception_cx164() {
    receive("CX164");
}

#[test]
fn source_comparison_exception_cx165() {
    receive("CX165");
}

#[test]
fn source_comparison_exception_cx166() {
    receive("CX166");
}

#[test]
fn source_comparison_exception_cx167() {
    receive("CX167");
}

#[test]
fn source_comparison_exception_cx168() {
    receive("CX168");
}

#[test]
fn source_comparison_exception_cx169() {
    receive("CX169");
}

#[test]
fn source_comparison_exception_cx170() {
    receive("CX170");
}

#[test]
fn source_comparison_exception_cx171() {
    receive("CX171");
}

#[test]
fn source_comparison_exception_cx172() {
    receive("CX172");
}

#[test]
fn source_comparison_exception_cx173() {
    receive("CX173");
}

#[test]
fn source_comparison_exception_cx174() {
    receive("CX174");
}

#[test]
fn source_comparison_exception_cx175() {
    receive("CX175");
}

#[test]
fn source_comparison_exception_cx176() {
    receive("CX176");
}

#[test]
fn source_comparison_exception_cx177() {
    receive("CX177");
}

#[test]
fn source_comparison_exception_cx178() {
    receive("CX178");
}

#[test]
fn source_comparison_exception_cx179() {
    receive("CX179");
}

#[test]
fn source_comparison_exception_cx180() {
    receive("CX180");
}

#[test]
fn source_comparison_exception_cx181() {
    receive("CX181");
}

#[test]
fn source_comparison_exception_cx182() {
    receive("CX182");
}

#[test]
fn source_comparison_exception_cx183() {
    receive("CX183");
}

#[test]
fn source_comparison_exception_cx184() {
    receive("CX184");
}

#[test]
fn source_comparison_exception_cx185() {
    receive("CX185");
}

#[test]
fn source_comparison_exception_cx186() {
    receive("CX186");
}

#[test]
fn source_comparison_exception_cx187() {
    receive("CX187");
}

#[test]
fn source_comparison_exception_cx188() {
    receive("CX188");
}

#[test]
fn source_comparison_exception_cx189() {
    receive("CX189");
}

#[test]
fn source_comparison_exception_cx190() {
    receive("CX190");
}

#[test]
fn source_comparison_exception_cx191() {
    receive("CX191");
}

#[test]
fn source_comparison_exception_cx192() {
    receive("CX192");
}

#[test]
fn source_comparison_exception_cx193() {
    receive("CX193");
}

#[test]
fn source_comparison_exception_cx194() {
    receive("CX194");
}

#[test]
fn source_comparison_exception_cx195() {
    receive("CX195");
}

#[test]
fn source_comparison_exception_cx196() {
    receive("CX196");
}

#[test]
fn source_comparison_exception_cx197() {
    receive("CX197");
}

#[test]
fn source_comparison_exception_cx198() {
    receive("CX198");
}

#[test]
fn source_comparison_exception_cx199() {
    receive("CX199");
}

#[test]
fn source_comparison_exception_cx200() {
    receive("CX200");
}

#[test]
fn source_comparison_exception_cx201() {
    receive("CX201");
}

#[test]
fn source_comparison_exception_cx202() {
    receive("CX202");
}

#[test]
fn source_comparison_exception_cx203() {
    receive("CX203");
}

#[test]
fn source_comparison_exception_cx204() {
    receive("CX204");
}

#[test]
fn source_comparison_exception_cx205() {
    receive("CX205");
}

#[test]
fn source_comparison_exception_cx206() {
    receive("CX206");
}

#[test]
fn source_comparison_exception_cx207() {
    receive("CX207");
}

#[test]
fn source_comparison_exception_cx208() {
    receive("CX208");
}

#[test]
fn source_comparison_exception_cx209() {
    receive("CX209");
}

#[test]
fn source_comparison_exception_cx210() {
    receive("CX210");
}

#[test]
fn source_comparison_exception_cx211() {
    receive("CX211");
}

#[test]
fn source_comparison_exception_cx212() {
    receive("CX212");
}

#[test]
fn source_comparison_exception_cx213() {
    receive("CX213");
}

#[test]
fn source_comparison_exception_cx214() {
    receive("CX214");
}

#[test]
fn source_comparison_exception_cx215() {
    receive("CX215");
}

#[test]
fn source_comparison_exception_cx216() {
    receive("CX216");
}

#[test]
fn source_comparison_exception_cx217() {
    receive("CX217");
}

#[test]
fn source_comparison_exception_cx218() {
    receive("CX218");
}

#[test]
fn source_comparison_exception_cx219() {
    receive("CX219");
}

#[test]
fn source_comparison_exception_cx220() {
    receive("CX220");
}

#[test]
fn source_comparison_exception_cx221() {
    receive("CX221");
}

#[test]
fn source_comparison_exception_cx222() {
    receive("CX222");
}

#[test]
fn source_comparison_exception_cx223() {
    receive("CX223");
}

#[test]
fn source_comparison_exception_cx224() {
    receive("CX224");
}

#[test]
fn source_comparison_exception_cx225() {
    receive("CX225");
}

#[test]
fn source_comparison_exception_cx226() {
    receive("CX226");
}

#[test]
fn source_comparison_exception_cx227() {
    receive("CX227");
}

#[test]
fn source_comparison_exception_cx228() {
    receive("CX228");
}

#[test]
fn source_comparison_exception_cx229() {
    receive("CX229");
}

#[test]
fn source_comparison_exception_cx230() {
    receive("CX230");
}

#[test]
fn source_comparison_exception_cx231() {
    receive("CX231");
}

#[test]
fn source_comparison_exception_cx232() {
    receive("CX232");
}

#[test]
fn source_comparison_exception_cx233() {
    receive("CX233");
}

#[test]
fn source_comparison_exception_cx234() {
    receive("CX234");
}

#[test]
fn source_comparison_exception_cx235() {
    receive("CX235");
}

#[test]
fn source_comparison_exception_cx236() {
    receive("CX236");
}

#[test]
fn source_comparison_exception_cx237() {
    receive("CX237");
}

#[test]
fn source_comparison_exception_cx238() {
    receive("CX238");
}

#[test]
fn source_comparison_exception_cx239() {
    receive("CX239");
}

#[test]
fn source_comparison_exception_cx240() {
    receive("CX240");
}

#[test]
fn source_comparison_exception_cx241() {
    receive("CX241");
}

#[test]
fn source_comparison_exception_cx242() {
    receive("CX242");
}

#[test]
fn source_comparison_exception_cx243() {
    receive("CX243");
}

#[test]
fn source_comparison_exception_cx244() {
    receive("CX244");
}

#[test]
fn source_comparison_exception_cx245() {
    receive("CX245");
}

#[test]
fn source_comparison_exception_cx246() {
    receive("CX246");
}

#[test]
fn source_comparison_exception_cx247() {
    receive("CX247");
}

#[test]
fn source_comparison_exception_cx248() {
    receive("CX248");
}

#[test]
fn source_comparison_exception_cx249() {
    receive("CX249");
}

#[test]
fn source_comparison_exception_cx250() {
    receive("CX250");
}

#[test]
fn source_comparison_exception_cx251() {
    receive("CX251");
}

#[test]
fn source_comparison_exception_cx252() {
    receive("CX252");
}

#[test]
fn source_comparison_exception_cx253() {
    receive("CX253");
}

#[test]
fn source_comparison_exception_cx254() {
    receive("CX254");
}

#[test]
fn source_comparison_exception_cx255() {
    receive("CX255");
}

#[test]
fn source_comparison_exception_cx256() {
    receive("CX256");
}

#[test]
fn source_comparison_exception_cx257() {
    receive("CX257");
}

#[test]
fn source_comparison_exception_cx258() {
    receive("CX258");
}

#[test]
fn source_comparison_exception_cx259() {
    receive("CX259");
}

#[test]
fn source_comparison_exception_cx260() {
    receive("CX260");
}

#[test]
fn source_comparison_exception_cx261() {
    receive("CX261");
}

#[test]
fn source_comparison_exception_cx262() {
    receive("CX262");
}

#[test]
fn source_comparison_exception_cx263() {
    receive("CX263");
}

#[test]
fn source_comparison_exception_cx264() {
    receive("CX264");
}

#[test]
fn source_comparison_exception_cx265() {
    receive("CX265");
}

#[test]
fn source_comparison_exception_cx266() {
    receive("CX266");
}

#[test]
fn source_comparison_exception_cx267() {
    receive("CX267");
}

#[test]
fn source_comparison_exception_cx268() {
    receive("CX268");
}

#[test]
fn source_comparison_exception_cx269() {
    receive("CX269");
}

#[test]
fn source_comparison_exception_cx270() {
    receive("CX270");
}

#[test]
fn source_comparison_exception_cx271() {
    receive("CX271");
}

#[test]
fn source_comparison_exception_cx272() {
    receive("CX272");
}

#[test]
fn source_comparison_exception_cx273() {
    receive("CX273");
}

#[test]
fn source_comparison_exception_cx274() {
    receive("CX274");
}

#[test]
fn source_comparison_exception_cx275() {
    receive("CX275");
}

#[test]
fn source_comparison_exception_cx276() {
    receive("CX276");
}

#[test]
fn source_comparison_exception_cx277() {
    receive("CX277");
}

#[test]
fn source_comparison_exception_cx278() {
    receive("CX278");
}

#[test]
fn source_comparison_exception_cx279() {
    receive("CX279");
}

#[test]
fn source_comparison_exception_cx280() {
    receive("CX280");
}

#[test]
fn source_comparison_exception_cx281() {
    receive("CX281");
}

#[test]
fn source_comparison_exception_cx282() {
    receive("CX282");
}

#[test]
fn source_comparison_exception_cx283() {
    receive("CX283");
}

#[test]
fn source_comparison_exception_cx284() {
    receive("CX284");
}

#[test]
fn source_comparison_exception_cx285() {
    receive("CX285");
}

#[test]
fn source_comparison_exception_cx286() {
    receive("CX286");
}

#[test]
fn source_comparison_exception_cx287() {
    receive("CX287");
}

#[test]
fn source_comparison_exception_cx288() {
    receive("CX288");
}

#[test]
fn source_comparison_exception_cx289() {
    receive("CX289");
}

#[test]
fn source_comparison_exception_cx290() {
    receive("CX290");
}

#[test]
fn source_comparison_exception_cx291() {
    receive("CX291");
}

#[test]
fn source_comparison_exception_cx292() {
    receive("CX292");
}

#[test]
fn source_comparison_exception_cx293() {
    receive("CX293");
}

#[test]
fn source_comparison_exception_cx294() {
    receive("CX294");
}

#[test]
fn source_comparison_exception_cx295() {
    receive("CX295");
}

#[test]
fn source_comparison_exception_cx296() {
    receive("CX296");
}

#[test]
fn source_comparison_exception_cx297() {
    receive("CX297");
}

#[test]
fn source_comparison_exception_cx298() {
    receive("CX298");
}

#[test]
fn source_comparison_exception_cx299() {
    receive("CX299");
}

#[test]
fn source_comparison_exception_cx300() {
    receive("CX300");
}

#[test]
fn source_comparison_exception_cx301() {
    receive("CX301");
}

#[test]
fn source_comparison_exception_cx302() {
    receive("CX302");
}

#[test]
fn source_comparison_exception_cx303() {
    receive("CX303");
}

#[test]
fn source_comparison_exception_cx304() {
    receive("CX304");
}

#[test]
fn source_comparison_exception_cx305() {
    receive("CX305");
}

#[test]
fn source_comparison_exception_cx306() {
    receive("CX306");
}

#[test]
fn source_comparison_exception_cx307() {
    receive("CX307");
}

#[test]
fn source_comparison_exception_cx308() {
    receive("CX308");
}

#[test]
fn source_comparison_exception_cx309() {
    receive("CX309");
}

#[test]
fn source_comparison_exception_cx310() {
    receive("CX310");
}

#[test]
fn source_comparison_exception_cx311() {
    receive("CX311");
}

#[test]
fn source_comparison_exception_cx312() {
    receive("CX312");
}

#[test]
fn source_comparison_exception_cx313() {
    receive("CX313");
}

#[test]
fn source_comparison_exception_cx314() {
    receive("CX314");
}

#[test]
fn source_comparison_exception_cx315() {
    receive("CX315");
}

#[test]
fn source_comparison_exception_cx316() {
    receive("CX316");
}

#[test]
fn source_comparison_exception_cx317() {
    receive("CX317");
}

#[test]
fn source_comparison_exception_cx318() {
    receive("CX318");
}

#[test]
fn source_comparison_exception_cx319() {
    receive("CX319");
}

#[test]
fn source_comparison_exception_cx320() {
    receive("CX320");
}

#[test]
fn source_comparison_exception_cx321() {
    receive("CX321");
}

#[test]
fn source_comparison_exception_cx322() {
    receive("CX322");
}

#[test]
fn source_comparison_exception_cx323() {
    receive("CX323");
}

#[test]
fn source_comparison_exception_cx324() {
    receive("CX324");
}

#[test]
fn source_comparison_exception_cx325() {
    receive("CX325");
}

#[test]
fn source_comparison_exception_cx326() {
    receive("CX326");
}

#[test]
fn source_comparison_exception_cx327() {
    receive("CX327");
}

#[test]
fn source_comparison_exception_cx328() {
    receive("CX328");
}

#[test]
fn source_comparison_exception_cx329() {
    receive("CX329");
}

#[test]
fn source_comparison_exception_cx330() {
    receive("CX330");
}

#[test]
fn source_comparison_exception_cx331() {
    receive("CX331");
}

#[test]
fn source_comparison_exception_cx332() {
    receive("CX332");
}

#[test]
fn source_comparison_exception_cx333() {
    receive("CX333");
}

#[test]
fn source_comparison_exception_cx334() {
    receive("CX334");
}

#[test]
fn source_comparison_exception_cx335() {
    receive("CX335");
}

#[test]
fn source_comparison_exception_cx336() {
    receive("CX336");
}

#[test]
fn source_comparison_exception_cx337() {
    receive("CX337");
}

#[test]
fn source_comparison_exception_cx338() {
    receive("CX338");
}

#[test]
fn source_comparison_exception_cx339() {
    receive("CX339");
}

#[test]
fn source_comparison_exception_cx340() {
    receive("CX340");
}

#[test]
fn source_comparison_exception_cx341() {
    receive("CX341");
}

#[test]
fn source_comparison_exception_cx342() {
    receive("CX342");
}

#[test]
fn source_comparison_exception_cx343() {
    receive("CX343");
}

#[test]
fn source_comparison_exception_cx344() {
    receive("CX344");
}

#[test]
fn source_comparison_exception_cx345() {
    receive("CX345");
}

#[test]
fn source_comparison_exception_cx346() {
    receive("CX346");
}

#[test]
fn source_comparison_exception_cx347() {
    receive("CX347");
}

#[test]
fn source_comparison_exception_cx348() {
    receive("CX348");
}

#[test]
fn source_comparison_exception_cx349() {
    receive("CX349");
}

#[test]
fn source_comparison_exception_cx350() {
    receive("CX350");
}

#[test]
fn source_comparison_exception_cx351() {
    receive("CX351");
}

#[test]
fn source_comparison_exception_cx352() {
    receive("CX352");
}

#[test]
fn source_comparison_exception_cx353() {
    receive("CX353");
}

#[test]
fn source_comparison_exception_cx354() {
    receive("CX354");
}

#[test]
fn source_comparison_exception_cx355() {
    receive("CX355");
}

#[test]
fn source_comparison_exception_cx356() {
    receive("CX356");
}

#[test]
fn source_comparison_exception_cx357() {
    receive("CX357");
}

#[test]
fn source_comparison_exception_cx358() {
    receive("CX358");
}

#[test]
fn source_comparison_exception_cx359() {
    receive("CX359");
}

#[test]
fn source_comparison_exception_cx360() {
    receive("CX360");
}

#[test]
fn source_comparison_exception_without_registry() {
    use semantica_numeric::{DecimalContext, DecimalSignal};
    for id in ["CX127", "CX130", "CX133", "CX136", "CX139", "CX142"] {
        let index: usize = id[2..].parse().unwrap();
        let (bytes, digest) = private_fixtures()[(index - 1) / 90];
        assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), digest);
        let root: Value = serde_json::from_str(bytes).unwrap();
        let case = root["graph_queries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["input"]["id"] == id)
            .unwrap();
        let session = source_bgp_receiving::session_with_decimal(DecimalContext {
            precision: 3,
            emin: (-2).into(),
            emax: 2.into(),
            traps: [
                DecimalSignal::FloatOperation,
                DecimalSignal::InvalidOperation,
            ]
            .into_iter()
            .collect(),
            ..DecimalContext::default()
        });
        assert!(session.source_functions().is_none());
        source_bgp_receiving::receive_query_case_without_registry(
            case,
            &root["data_inputs"],
            session,
        );
    }
}
#[test]
fn source_comparison_exception_sealed_frontend_closed() {
    for operator in ["<", ">=", "<="] {
        let session = source_bgp_receiving::session();
        let query = numeric_query::prepare_source_frontend_numeric_query(
            &format!("SELECT (1.0 {operator} 2.0 AS ?v) WHERE {{}}"),
            source_bgp_receiving::dataset(&session, None),
            session,
            Vec::new(),
        )
        .unwrap();
        let error = match query.execute_source_presence() {
            Ok(_) => panic!("comparison granted sealed frontend execution"),
            Err(e) => e,
        };
        assert_eq!(error.to_string(), "unsupported nullable expression");
    }
}
#[test]
fn source_comparison_exception_legacy_scope_closed() {
    use crate::numeric::{NumericEvaluationError, SourceRelationalOrder};
    use semantica_numeric::{DecimalContext, NumericLimits};
    let session = QueryNumericSession::new(
        DecimalContext::default(),
        NumericLimits::default(),
        None,
        true,
    )
    .unwrap();
    for operator in [
        SourceRelationalOrder::Greater,
        SourceRelationalOrder::Less,
        SourceRelationalOrder::GreaterOrEqual,
        SourceRelationalOrder::LessOrEqual,
    ] {
        assert!(matches!(
            session.source_relational_order(
                &SolutionValue::ExplicitNone,
                &SolutionValue::ExplicitNone,
                operator
            ),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnsupportedSourceLiteralOperator("source relation")
            ))
        ));
    }
}

#[test]
fn source_comparison_exception_cannot_swallow_resource_failure() {
    use crate::numeric::source_registry::*;
    use crate::numeric::{
        QueryContextSemantics, QueryGroupSemantics, QueryNumericCapabilities, SourceHashPolicy,
        SourceHashRuntime,
    };
    use semantica_numeric::{DecimalContext, NumericFault, NumericLimits};
    use std::cell::Cell;
    for operator in ["=", "!=", ">", "<", ">=", "<="] {
        for substitute_exception in [false, true] {
            let registry = Rc::new(SourceFunctionRegistry::stock());
            let session = Rc::new(
                QueryNumericSession::new_with_context_semantics(
                    DecimalContext {
                        precision: 3,
                        ..DecimalContext::default()
                    },
                    NumericLimits {
                        max_coefficient_digits: 3,
                        ..NumericLimits::default()
                    },
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
                .with_source_functions(Rc::clone(&registry))
                .unwrap(),
            );
            let called = Rc::new(Cell::new(false));
            let observed = Rc::clone(&called);
            registry
                .register(
                    NamedNode::new("urn:f").unwrap(),
                    SourceFunctionDefinition {
                        raw: true,
                        callable: SourceCallable::Native {
                            qualified_name: Rc::from("receiving.resource_swallow"),
                            callback: Rc::new(move |invocation| {
                                observed.set(true);
                                let SourceFunctionInvocation::Raw(ctx) = invocation else {
                                    panic!("raw callback");
                                };
                                assert!(matches!(
                                    ctx.arguments(),
                                    Err(SourceFunctionFailure::Host)
                                ));
                                assert_eq!(
                                    ctx.session().flags(),
                                    semantica_numeric::DecimalSignal::FloatOperation.into()
                                );
                                if substitute_exception {
                                    Err(SourceFunctionFailure::Exception(SourceFunctionException {
                                        kind: SourceFunctionExceptionKind::Decimal(
                                            semantica_numeric::DecimalSignal::InvalidOperation,
                                        ),
                                        arguments: vec![
                                            SourceFunctionArgument::DecimalSignalClasses(vec![
                                                semantica_numeric::DecimalSignal::InvalidOperation,
                                            ]),
                                        ],
                                    }))
                                } else {
                                    Ok(SolutionValue::ExplicitNone)
                                }
                            }),
                        },
                    },
                    false,
                )
                .unwrap();
            let query = numeric_query::prepare_source_graph_numeric_query(
                &format!("SELECT (<urn:f>(0.1 {operator} 0.1e0) AS ?v) WHERE {{}}"),
                source_bgp_receiving::dataset(&session, None),
                session,
                Vec::new(),
            )
            .unwrap();
            let mut rows = query.execute_source_presence().unwrap();
            let error = rows
                .next()
                .unwrap()
                .err()
                .expect("callback hid a host resource limit");
            assert!(
                called.get(),
                "resource failure did not originate in the callback read"
            );
            assert!(
                matches!(
                    crate::numeric::raw_numeric_fault(&error),
                    Some(RawNumericFault::Core(NumericFault::ResourceLimit {
                        operation: "coefficient_digits"
                    }))
                ),
                "original resource failure changed: {error:?}"
            );
            for _ in 0..3 {
                assert!(rows.next().is_none());
            }
        }
    }
}
