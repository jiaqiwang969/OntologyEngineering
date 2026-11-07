//! Wave-1 recognition from drafting knowledge cards. Every fixture here is authored from card
//! knowledge — the written forms on the concept cards (taken from the standards and textbooks),
//! symbol proportions and title-block layouts — never from a real drawing: handles, coordinates,
//! layer names and labels are invented, and no expert reference is read.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::{
    concept_status, discover, drawing_concepts, recognizer_coverage, representation_knowledge,
    text_concepts,
};
use std::collections::BTreeSet;

fn ent(kind: &str, id: &str, layer: &str, tags: &str) -> String {
    format!("0\n{kind}\n5\n{id}\n8\n{layer}\n330\nMODEL\n{tags}")
}
fn line(id: &str, layer: &str, a: [f64; 2], b: [f64; 2]) -> String {
    ent(
        "LINE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
    )
}
fn text(id: &str, layer: &str, s: &str, p: [f64; 2], h: f64, rot: f64) -> String {
    ent(
        "TEXT",
        id,
        layer,
        &format!("1\n{s}\n10\n{}\n20\n{}\n40\n{h}\n50\n{rot}\n", p[0], p[1]),
    )
}
fn styled_text(id: &str, layer: &str, s: &str, p: [f64; 2], h: f64, style: &str) -> String {
    ent(
        "TEXT",
        id,
        layer,
        &format!(
            "1\n{s}\n10\n{}\n20\n{}\n40\n{h}\n50\n0\n7\n{style}\n",
            p[0], p[1]
        ),
    )
}
fn circle(id: &str, layer: &str, c: [f64; 2], r: f64) -> String {
    ent(
        "CIRCLE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]),
    )
}
fn arc(id: &str, layer: &str, c: [f64; 2], r: f64, a0: f64, a1: f64) -> String {
    ent(
        "ARC",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]),
    )
}
fn solid(id: &str, layer: &str, tip: [f64; 2], base: [f64; 2]) -> String {
    let d = [base[0] - tip[0], base[1] - tip[1]];
    let p = [-d[1] * 0.15, d[0] * 0.15];
    let (b1, b2) = (
        [base[0] + p[0], base[1] + p[1]],
        [base[0] - p[0], base[1] - p[1]],
    );
    ent(
        "SOLID",
        id,
        layer,
        &format!(
            "10\n{}\n20\n{}\n11\n{}\n21\n{}\n12\n{}\n22\n{}\n13\n{}\n23\n{}\n",
            tip[0], tip[1], b1[0], b1[1], b2[0], b2[1], b2[0], b2[1]
        ),
    )
}
fn lwpoly(id: &str, layer: &str, pts: &[[f64; 2]], closed: bool) -> String {
    let mut t = format!("90\n{}\n70\n{}\n", pts.len(), i32::from(closed));
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
fn rect(id: &str, layer: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    lwpoly(
        id,
        layer,
        &[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]],
        false,
    )
}
fn insert(id: &str, layer: &str, name: &str, at: [f64; 2], rot: f64) -> String {
    ent(
        "INSERT",
        id,
        layer,
        &format!(
            "2\n{name}\n10\n{}\n20\n{}\n41\n1\n42\n1\n50\n{rot}\n",
            at[0], at[1]
        ),
    )
}
fn tables(style: bool) -> String {
    let mut s = String::from("0\nSECTION\n2\nTABLES\n");
    for (name, el) in [
        ("CONTINUOUS", vec![]),
        ("DASHX", vec![2., -1.]),
        ("AXISX", vec![8., -1., 1., -1.]),
    ] {
        s.push_str(&format!("0\nLTYPE\n2\n{name}\n73\n{}\n", el.len()));
        for e in el {
            s.push_str(&format!("49\n{e}\n"));
        }
    }
    for (name, color, lt) in [
        ("0", 7, "CONTINUOUS"),
        ("01", 7, "CONTINUOUS"),
        ("02", 3, "CONTINUOUS"),
        ("04", 2, "DASHX"),
        ("05", 1, "AXISX"),
        ("08", 3, "CONTINUOUS"),
        ("11", 3, "CONTINUOUS"),
    ] {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n"));
    }
    if style {
        s.push_str("0\nSTYLE\n2\nGDTSTYLE\n3\ngdt.shx\n");
    }
    s.push_str("0\nENDSEC\n");
    s
}
/// Block definitions; child handles are made unique by prefixing the block name.
fn blocks(defs: &[(&str, Vec<String>)]) -> String {
    let mut s = String::from("0\nSECTION\n2\nBLOCKS\n");
    for (name, ents) in defs {
        let body: String = ents
            .iter()
            .map(|e| e.replacen("\n5\n", &format!("\n5\n{name}"), 1))
            .collect();
        s.push_str(&format!(
            "0\nBLOCK\n2\n{name}\n10\n0\n20\n0\n{body}0\nENDBLK\n"
        ));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn run_with(style: bool, blocks: &str, ents: &[String]) -> MdEngineeringExpressionGraph {
    let bytes = format!(
        "{}{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        tables(style),
        ents.concat()
    );
    match discover(bytes.as_bytes()) {
        Ok(g) => g,
        Err(e) => panic!("{e:#}"),
    }
}
fn run(blocks: &str, ents: &[String]) -> MdEngineeringExpressionGraph {
    run_with(false, blocks, ents)
}
fn exprs_of<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
) -> Vec<&'a MdEngineeringExpression> {
    let ids: BTreeSet<&str> = g
        .members
        .iter()
        .filter(|m| m.source_handle == src)
        .map(|m| m.expression_id.as_str())
        .collect();
    g.expressions
        .iter()
        .filter(|e| ids.contains(e.id.as_str()))
        .collect()
}
fn expr_with<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
    kind: &str,
) -> Option<&'a MdEngineeringExpression> {
    exprs_of(g, src).into_iter().find(|e| e.kind == kind)
}
fn has(e: &MdEngineeringExpression, ids: &[&str]) -> bool {
    ids.iter().all(|i| e.concept_ids.iter().any(|c| c == i))
}
fn tc(card: &str, t: &str) -> BTreeSet<String> {
    text_concepts(card, t).unwrap()
}
fn has_t(card: &str, t: &str, ids: &[&str]) {
    let got = tc(card, t);
    for i in ids {
        assert!(got.contains(*i), "{card} `{t}`: {i} missing in {got:?}");
    }
}
fn lacks_t(card: &str, t: &str, ids: &[&str]) {
    let got = tc(card, t);
    for i in ids {
        assert!(
            !got.contains(*i),
            "{card} `{t}`: {i} must not be named ({got:?})"
        );
    }
}
fn native_dimension(id: &str, block: &str, txt: &str, x: f64, y: f64) -> (String, String) {
    let children = [
        line(&format!("{id}1"), "0", [x, y + 10.], [x + 30., y + 10.]),
        line(&format!("{id}2"), "0", [x, y], [x, y + 12.]),
        line(&format!("{id}3"), "0", [x + 30., y], [x + 30., y + 12.]),
        solid(&format!("{id}4"), "0", [x, y + 10.], [x + 2., y + 10.]),
        solid(
            &format!("{id}5"),
            "0",
            [x + 30., y + 10.],
            [x + 28., y + 10.],
        ),
        ent(
            "MTEXT",
            &format!("{id}6"),
            "0",
            &format!(
                "1\nshown\n10\n{}\n20\n{}\n40\n2.5\n71\n8\n",
                x + 15.,
                y + 11.
            ),
        ),
    ];
    let blk = format!(
        "0\nBLOCK\n2\n{block}\n10\n0\n20\n0\n{}0\nENDBLK\n",
        children.concat()
    );
    let dim = ent(
        "DIMENSION",
        id,
        "0",
        &format!(
            "2\n{block}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n70\n160\n1\n{txt}\n13\n{x}\n23\n{y}\n14\n{}\n24\n{y}\n42\n30\n50\n0\n",
            x + 30.,
            y + 10.,
            x + 15.,
            y + 11.,
            x + 30.
        ),
    );
    (blk, dim)
}

// ---------------------------------------------------------------- written forms of the concept cards

/// One trailing full-width parenthetical note is removed when text precedes it.
fn form(s: &str) -> String {
    let t = s.trim();
    if t.ends_with('）')
        && let Some(i) = t.rfind('（')
        && i > 0
    {
        return t[..i].trim().to_string();
    }
    t.to_string()
}

#[test]
fn text_forms_of_text_cards_are_recognised() {
    let k = representation_knowledge().unwrap();
    let concepts: Vec<_> = drawing_concepts()
        .unwrap()
        .iter()
        .flat_map(|g| g.concepts.iter())
        .collect();
    for card in ["dimension_text", "technical_note", "title_block_field"] {
        let c = k.cards.iter().find(|c| c.id == card).unwrap();
        let outside: Vec<(String, String)> = c
            .outside_forms
            .iter()
            .flatten()
            .map(|o| (o.concept.clone(), o.form.clone()))
            .collect();
        let (mut total, mut listed) = (0usize, 0usize);
        for id in &c.concepts {
            let concept = concepts.iter().find(|x| &x.id == id).unwrap();
            for f in &concept.text_forms {
                let f = form(f);
                total += 1;
                if outside.iter().any(|(cc, ff)| cc == id && *ff == f) {
                    listed += 1;
                    continue;
                }
                let got = tc(card, &f);
                assert!(
                    got.contains(id),
                    "{card}: written form `{f}` of {id} not recognised ({got:?})"
                );
            }
        }
        for (cc, ff) in &outside {
            let concept = concepts.iter().find(|x| &x.id == cc).unwrap();
            assert!(
                concept.text_forms.iter().any(|f| form(f) == *ff),
                "{card}: outside form {ff} is not a form of {cc}"
            );
        }
        assert!(
            listed * 10 <= total,
            "{card}: {listed} of {total} forms listed outside the grammar"
        );
    }
}

// ---------------------------------------------------------------- dimension_text

#[test]
fn dimension_text_size_symbols_name_their_concepts() {
    has_t(
        "dimension_text",
        "Ø30",
        &["DC-G4-055", "DC-G4-030", "DC-G4-017", "DC-G4-099"],
    );
    has_t("dimension_text", "%%c30", &["DC-G4-055", "DC-G4-030"]);
    has_t("dimension_text", "SØ20", &["DC-G4-055"]);
    has_t("dimension_text", "R20", &["DC-G4-041", "DC-G4-006"]);
    has_t("dimension_text", "C1", &["DC-G4-040", "DC-G3-091"]);
    has_t("dimension_text", "1×45°", &["DC-G4-040", "DC-G3-091"]);
    has_t("dimension_text", "1.5×30°", &["DC-G3-091"]);
    lacks_t("dimension_text", "1.5×30°", &["DC-G4-040", "DC-G4-094"]);
    lacks_t("dimension_text", "58", &["DC-G4-055", "DC-G4-041"]);
}

#[test]
fn native_diameter_dimension_names_its_text_concepts() {
    let (blk, dim) = native_dimension("DA", "*DW1", "%%C30H7", 0., 0.);
    let g = run(&format!("0\nSECTION\n2\nBLOCKS\n{blk}0\nENDSEC\n"), &[dim]);
    let e = expr_with(&g, "DA", "dimension_diameter").expect("diameter dimension");
    assert!(
        has(
            e,
            &[
                "DC-G4-030",
                "DC-G4-055",
                "DC-G4-114",
                "DC-G4-059",
                "DC-G4-058",
                "DC-G4-017"
            ]
        ),
        "{:?}",
        e.concept_ids
    );
    assert!(e.evidence_ids.contains(&"card:dimension_text".into()));
    assert!(
        !e.evidence_ids.iter().any(|x| x.starts_with("concept:")),
        "concept evidence is moved into concept_ids"
    );
}

#[test]
fn dimension_text_counts_old_and_new() {
    has_t("dimension_text", "4×Ø8", &["DC-G4-094", "DC-G4-055"]);
    has_t("dimension_text", "8-Ø8", &["DC-G4-094"]);
    has_t("dimension_text", "6-M8X1", &["DC-G4-094", "DC-G3-088"]);
    lacks_t("dimension_text", "20-0.1", &["DC-G4-094"]);
    lacks_t("dimension_text", "1×45°", &["DC-G4-094"]);
}

#[test]
fn dimension_text_tolerance_classes() {
    for t in ["Ø65H7", "165js10", "Ø20h6Ⓔ", "IT7", "32 +0.025/0 (H7)"] {
        has_t("dimension_text", t, &["DC-G4-114"]);
    }
    for t in ["C1", "R5", "M10-6H", "58"] {
        lacks_t("dimension_text", t, &["DC-G4-114"]);
    }
}

#[test]
fn dimension_text_deviations_stacked_symmetric_and_zero() {
    has_t("dimension_text", "Ø30 +0.041/+0.020", &["DC-G4-132"]);
    lacks_t(
        "dimension_text",
        "Ø30 +0.041/+0.020",
        &["DC-G4-103", "DC-G4-113"],
    );
    has_t(
        "dimension_text",
        "Ø30 0/-0.021",
        &["DC-G4-132", "DC-G4-103"],
    );
    has_t(
        "dimension_text",
        "%%c30{\\H0.7x;\\S+0.021^-0.008;}",
        &["DC-G4-132"],
    );
    has_t("dimension_text", "50±0.31", &["DC-G4-126"]);
    has_t("dimension_text", "74%%p0.05", &["DC-G4-126"]);
    has_t(
        "dimension_text",
        "Ø65H7(+0.030/0)",
        &["DC-G4-113", "DC-G4-114", "DC-G4-132", "DC-G4-103"],
    );
    lacks_t("dimension_text", "G1/2", &["DC-G4-132"]);
}

#[test]
fn stacked_mtext_deviation_dimension_names_deviation_concepts() {
    let ents = vec![
        line("V1", "08", [150., 0.], [150., 35.]),
        solid("VA", "08", [150., 0.], [150., 3.]),
        solid("VB", "08", [150., 35.], [150., 32.]),
        text(
            "VT",
            "08",
            "%%C28\u{e1d6}-0.02\u{e1da}\u{e1d7}-0.08\u{e1d9}",
            [149.4, 12.],
            2.5,
            90.,
        ),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "VT", "dimension_diameter").expect("diameter dimension");
    assert!(
        has(e, &["DC-G4-132", "DC-G4-055", "DC-G4-030", "DC-G4-019"]),
        "{:?}",
        e.concept_ids
    );
    assert!(!has(e, &["DC-G4-103"]));
}

#[test]
fn dimension_text_thread_designations() {
    has_t(
        "dimension_text",
        "M10-6g",
        &["DC-G3-088", "DC-G3-223", "DC-G3-232"],
    );
    has_t(
        "dimension_text",
        "M20×2LH-6H/6g",
        &["DC-G3-088", "DC-G3-232"],
    );
    has_t("dimension_text", "Tr32×6-7e", &["DC-G3-088", "DC-G3-232"]);
    lacks_t("dimension_text", "Tr32×6-7e", &["DC-G3-223"]);
    has_t("dimension_text", "G1/2", &["DC-G3-088"]);
    has_t("dimension_text", "M10-6H↧15", &["DC-G3-089"]);
    lacks_t("dimension_text", "M10", &["DC-G4-041", "DC-G4-114"]);
}

#[test]
fn dimension_text_hole_callouts() {
    for t in ["4×Ø4↧10", "8×Ø6.4 ⌴Ø12↧4.5", "6×Ø6.5 ⌵Ø10×90°", "4×M6↧8"] {
        has_t("dimension_text", t, &["DC-G4-088"]);
    }
    lacks_t("dimension_text", "4×Ø8", &["DC-G4-088"]);
}

#[test]
fn dimension_text_angles_and_units() {
    for t in ["30°", "1°54′33″", "30%%d", "60°±10′"] {
        has_t("dimension_text", t, &["DC-G4-032", "DC-G4-099"]);
    }
    has_t("dimension_text", "单位：mm", &["DC-G4-099"]);
    lacks_t("dimension_text", "1×45°", &["DC-G4-032"]);
}

#[test]
fn center_hole_mark_is_not_a_buttress_thread() {
    for t in ["B4", "2-B4", "B3.15"] {
        lacks_t("dimension_text", t, &["DC-G3-088"]);
    }
    has_t("dimension_text", "B40×7-7A", &["DC-G3-088"]);
}

#[test]
fn general_tolerance_reference_is_not_a_tolerance_class() {
    lacks_t("dimension_text", "GB/T 1804-m", &["DC-G4-114"]);
    lacks_t("dimension_text", "GB/T 1184-K", &["DC-G4-114"]);
}

#[test]
fn roughness_parameter_is_not_a_radius() {
    for t in ["Ra 3.2", "Rz 6.3", "58~62HRC"] {
        lacks_t("dimension_text", t, &["DC-G4-041", "DC-G4-006"]);
    }
}

// ---------------------------------------------------------------- technical_note

#[test]
fn technical_note_wording_names_concepts() {
    has_t(
        "technical_note",
        "未注倒角C1",
        &[
            "DC-G9-002",
            "DC-G1-070",
            "DC-G9-003",
            "DC-G4-068",
            "DC-G4-040",
        ],
    );
    has_t(
        "technical_note",
        "未注铸造圆角R1~R3",
        &["DC-G1-071", "DC-G9-004", "DC-G3-112", "DC-G9-009"],
    );
    has_t(
        "technical_note",
        "未注尺寸公差按GB/T 1804-m",
        &["DC-G4-061", "DC-G4-070"],
    );
    has_t(
        "technical_note",
        "未注几何公差按GB/T 1184-K",
        &["DC-G5-025", "DC-G5-026"],
    );
    has_t("technical_note", "去毛刺、锐边", &["DC-G9-010"]);
    has_t("technical_note", "其余加工面 Ra 12.5", &["DC-G6-001"]);
    lacks_t("technical_note", "其余倒角C1", &["DC-G6-001"]);
}

#[test]
fn technical_requirement_block_items_name_concepts() {
    let ents = vec![
        text("H", "11", "技术要求", [100., 100.], 7., 0.),
        text("N1", "11", " 1.", [90., 88.], 5., 0.),
        text("I1", "11", "调质处理220~250HBW；", [100., 88.], 5., 0.),
        text("N2", "11", " 2.", [90., 78.], 5., 0.),
        text("I2", "11", "未注倒角C1；", [100., 78.], 5., 0.),
        text("N3", "11", " 3.", [90., 68.], 5., 0.),
        text(
            "I3",
            "11",
            "未注尺寸公差按GB/T 1804-m。",
            [100., 68.],
            5.,
            0.,
        ),
    ];
    let g = run("", &ents);
    let h = expr_with(&g, "H", "technical_general").unwrap();
    assert_eq!(h.concept_ids, vec!["DC-G9-002".to_string()]);
    let i1 = expr_with(&g, "I1", "material_heat").expect("heat item keeps its topic kind");
    assert!(
        has(i1, &["DC-G9-002", "DC-G7-025", "DC-G7-089"]),
        "{:?}",
        i1.concept_ids
    );
    let i2 = exprs_of(&g, "I2")
        .into_iter()
        .find(|e| e.family == "note")
        .unwrap();
    assert!(
        has(i2, &["DC-G9-003", "DC-G1-070", "DC-G4-040"]),
        "{:?}",
        i2.concept_ids
    );
    assert!(
        i2.evidence_ids.contains(&"card:dimension_text".into()),
        "the C1 token is named by the dimension-text card"
    );
    let i3 = exprs_of(&g, "I3")
        .into_iter()
        .find(|e| e.family == "note")
        .unwrap();
    assert!(has(i3, &["DC-G4-061", "DC-G4-070"]));
}

#[test]
fn technical_note_heat_treatment_and_hardness() {
    has_t("technical_note", "58~62HRC", &["DC-G7-089"]);
    has_t("technical_note", "≤550HV10", &["DC-G7-089"]);
    has_t(
        "technical_note",
        "表面硬化和回火 (700+100/0)HV30 CHD=0.8+0.4",
        &["DC-G7-025", "DC-G7-026", "DC-G7-089"],
    );
    has_t("technical_note", "渗碳 CD0.35=1.2+0.5", &["DC-G7-026"]);
    lacks_t("technical_note", "去毛刺", &["DC-G7-089", "DC-G7-025"]);
}

#[test]
fn heading_alone_is_not_an_untoleranced_note() {
    assert_eq!(
        tc("technical_note", "技术要求"),
        BTreeSet::from(["DC-G9-002".to_string()])
    );
}

#[test]
fn general_tolerance_note_is_not_a_geometric_one() {
    lacks_t(
        "technical_note",
        "未注尺寸公差按GB/T 1804-m",
        &["DC-G5-025", "DC-G5-026"],
    );
    lacks_t(
        "technical_note",
        "未注几何公差按GB/T 1184-K",
        &["DC-G4-061"],
    );
}

// ---------------------------------------------------------------- title_block_field

#[test]
fn title_block_labels_name_their_zones() {
    has_t("title_block_field", "设计", &["DC-G9-024", "DC-G9-028"]);
    has_t(
        "title_block_field",
        "标记 处数 分区 更改文件号 签名 年、月、日",
        &["DC-G9-023"],
    );
    has_t("title_block_field", "比例", &["DC-G9-025"]);
    has_t("title_block_field", "共 1 张 第 1 张", &["DC-G9-028"]);
}

#[test]
fn title_block_values_name_their_fields() {
    has_t("title_block_field", "1:2", &["DC-G9-025", "DC-G1-114"]);
    for m in ["40Cr", "45", "HT200", "ZG230-450", "Q235", "80圆钢 35"] {
        has_t("title_block_field", m, &["DC-G9-022"]);
    }
    has_t("title_block_field", "ZDY70.09", &["DC-G9-018"]);
    has_t("title_block_field", "阀芯", &["DC-G9-019"]);
    has_t("title_block_field", "××机械有限公司", &["DC-G9-017"]);
}

fn title_block(code: &str) -> (String, Vec<String>) {
    let grid = vec![
        line("g1", "0", [0., 0.], [60., 0.]),
        line("g2", "0", [0., 24.], [60., 24.]),
        line("g3", "0", [0., 0.], [0., 24.]),
        line("g4", "0", [60., 0.], [60., 24.]),
        line("g5", "0", [0., 8.], [60., 8.]),
        line("g6", "0", [0., 16.], [60., 16.]),
        line("g7", "0", [15., 0.], [15., 24.]),
        line("g8", "0", [30., 0.], [30., 24.]),
        line("g9", "0", [45., 0.], [45., 24.]),
        text("l1", "0", "设计", [1., 18.], 3., 0.),
        text("l2", "0", "审核", [1., 10.], 3., 0.),
        text("l3", "0", "比例", [31., 18.], 3., 0.),
    ];
    let blk = blocks(&[("TBLK", grid)]);
    let ents = vec![
        insert("TB", "0", "TBLK", [200., 0.], 0.),
        text("V1", "11", "张三", [216., 18.], 3., 0.),
        text("V2", "11", "1:2", [246., 18.], 3., 0.),
        text("V3", "11", "HT200", [246., 10.], 3., 0.),
        text("V4", "11", code, [216., 2.], 3., 0.),
        text("V5", "11", "端盖", [246., 2.], 3., 0.),
    ];
    (blk, ents)
}

#[test]
fn title_block_on_a_drawing_names_its_fields() {
    let (blk, ents) = title_block("AB-01.02");
    let g = run(&blk, &ents);
    let e = expr_with(&g, "TB", "title_block").expect("title block");
    assert!(
        has(
            e,
            &[
                "DC-G9-028",
                "DC-G9-024",
                "DC-G9-025",
                "DC-G1-114",
                "DC-G9-022",
                "DC-G9-018",
                "DC-G9-019"
            ]
        ),
        "{:?}",
        e.concept_ids
    );
    let field = |src: &str| {
        g.members
            .iter()
            .find(|m| m.expression_id == e.id && m.source_handle == src)
            .map(|m| m.evidence_ids.clone())
            .unwrap_or_default()
    };
    assert!(
        field("V1").contains(&"field:DC-G9-024".into()),
        "a name next to 设计 is a signature: {:?}",
        field("V1")
    );
    assert!(field("V3").contains(&"field:DC-G9-022".into()));
    assert!(field("V4").contains(&"field:DC-G9-018".into()));
    assert!(field("V5").contains(&"field:DC-G9-019".into()));
}

#[test]
fn repeated_drawing_code_with_range_separator_is_recognised() {
    let (blk, mut ents) = title_block("AB.100.1~2");
    ents.push(text("RC", "11", "AB.100.1-2", [20., 280.], 5., 180.));
    let g = run(&blk, &ents);
    let e = expr_with(&g, "RC", "title_block").expect("repeated drawing code");
    assert_eq!(e.rule, "repeated_document_number");
    assert!(has(e, &["DC-G9-018"]));
}

#[test]
fn date_and_count_are_not_drawing_codes() {
    lacks_t("title_block_field", "20311231", &["DC-G9-018"]);
    assert!(tc("title_block_field", "1").is_empty());
}

// ---------------------------------------------------------------- series_table

fn grid(prefix: &str, x0: f64, y0: f64, cols: usize, rows: usize, w: f64, h: f64) -> Vec<String> {
    let mut v = Vec::new();
    for r in 0..=rows {
        let y = y0 + r as f64 * h;
        v.push(line(
            &format!("{prefix}h{r}"),
            "02",
            [x0, y],
            [x0 + cols as f64 * w, y],
        ));
    }
    for c in 0..=cols {
        let x = x0 + c as f64 * w;
        v.push(line(
            &format!("{prefix}v{c}"),
            "02",
            [x, y0],
            [x, y0 + rows as f64 * h],
        ));
    }
    v
}

#[test]
fn series_table_links_its_variables_to_dimensions() {
    let (b1, d1) = native_dimension("DL1", "*DS1", "L1", 0., 0.);
    let (b2, d2) = native_dimension("DL2", "*DS2", "L2", 0., 30.);
    // a variable symbol may carry its own deviations
    let (b3, d3) = native_dimension("DX1", "*DS3", "X1{\\H0.6x;\\S0^-0.5;}", 0., 60.);
    let mut ents = vec![d1, d2, d3];
    ents.extend(grid("T", 100., 0., 4, 3, 20., 7.));
    for (id, s, c, r) in [
        ("T00", "代号", 0, 2),
        ("T01", "L1", 1, 2),
        ("T02", "L2", 2, 2),
        ("T03", "X1", 3, 2),
        ("T10", "A-01", 0, 1),
        ("T11", "30", 1, 1),
        ("T12", "40", 2, 1),
        ("T13", "52", 3, 1),
        ("T20", "A-02", 0, 0),
        ("T21", "35", 1, 0),
        ("T22", "45", 2, 0),
        ("T23", "56", 3, 0),
    ] {
        ents.push(text(
            id,
            "11",
            s,
            [103. + c as f64 * 20., 2. + r as f64 * 7.],
            3.,
            0.,
        ));
    }
    let g = run(
        &format!("0\nSECTION\n2\nBLOCKS\n{b1}{b2}{b3}0\nENDSEC\n"),
        &ents,
    );
    let e = expr_with(&g, "T01", "series_table").expect("series table");
    assert!(e.rule.ends_with("variables_named_by_dimensions"));
    assert!(has(e, &["DC-G4-098"]));
    for s in ["Th0", "Th3", "Tv0", "Tv3", "T00", "T22"] {
        assert!(
            expr_with(&g, s, "series_table").is_some(),
            "{s} not in the table"
        );
    }
    let linked: Vec<&String> = e
        .evidence_ids
        .iter()
        .filter(|x| x.starts_with("series_variable_of:"))
        .collect();
    assert_eq!(linked.len(), 3, "{:?}", e.evidence_ids);
    let x1 = expr_with(&g, "DX1", "dimension_linear").unwrap();
    assert!(
        linked.iter().any(|x| x.ends_with(&x1.id)),
        "symbol with deviations: {}",
        x1.label
    );
    let l1 = expr_with(&g, "DL1", "dimension_linear").unwrap();
    assert!(linked.iter().any(|x| x.ends_with(&l1.id)));
}

#[test]
fn series_table_with_header_words_and_values() {
    let mut ents = grid("S", 0., 0., 3, 3, 20., 7.);
    for (id, s, c, r) in [
        ("S00", "型号", 0, 2),
        ("S01", "B", 1, 2),
        ("S02", "H", 2, 2),
        ("S10", "X1", 0, 1),
        ("S11", "12", 1, 1),
        ("S12", "20", 2, 1),
        ("S20", "X2", 0, 0),
        ("S21", "16", 1, 0),
        ("S22", "25", 2, 0),
    ] {
        ents.push(text(
            id,
            "11",
            s,
            [3. + c as f64 * 20., 2. + r as f64 * 7.],
            3.,
            0.,
        ));
    }
    let g = run("", &ents);
    let e = expr_with(&g, "S01", "series_table").expect("series table");
    assert!(
        e.rule.ends_with("header_words_and_numeric_rows"),
        "{}",
        e.rule
    );
}

#[test]
fn title_block_grid_is_not_a_series_table() {
    let mut ents = grid("Q", 0., 0., 3, 3, 20., 7.);
    for (id, s, c, r) in [
        ("Q00", "设计", 0, 2),
        ("Q01", "审核", 1, 2),
        ("Q02", "批准", 2, 2),
        ("Q10", "日期", 0, 1),
    ] {
        ents.push(text(
            id,
            "11",
            s,
            [3. + c as f64 * 20., 2. + r as f64 * 7.],
            3.,
            0.,
        ));
    }
    let g = run("", &ents);
    assert!(!g.expressions.iter().any(|e| e.kind == "series_table"));
}

#[test]
fn grid_without_values_is_not_a_series_table() {
    let mut ents = grid("W", 0., 0., 3, 3, 20., 7.);
    for (id, s, c) in [("W0", "代号", 0), ("W1", "L", 1), ("W2", "B", 2)] {
        ents.push(text(id, "11", s, [3. + c as f64 * 20., 16.], 3., 0.));
    }
    let g = run("", &ents);
    assert!(!g.expressions.iter().any(|e| e.kind == "series_table"));
}

// ---------------------------------------------------------------- gdt_characteristic and frame content

/// A frame of three polyline cells (10 high) with a glyph block in the first cell.
fn frame(prefix: &str, x: f64, glyph_block: &str, value: &str, datums: &[&str]) -> Vec<String> {
    let mut v = vec![
        insert(&format!("{prefix}G"), "01", glyph_block, [x, 0.], 0.),
        rect(&format!("{prefix}C1"), "02", x, 0., x + 10., 10.),
        rect(&format!("{prefix}C2"), "02", x + 10., 0., x + 26., 10.),
        text(&format!("{prefix}V"), "11", value, [x + 12., 3.], 4., 0.),
    ];
    for (i, d) in datums.iter().enumerate() {
        let x0 = x + 26. + 8. * i as f64;
        v.push(rect(
            &format!("{prefix}C{}", 3 + i),
            "02",
            x0,
            0.,
            x0 + 8.,
            10.,
        ));
        v.push(text(
            &format!("{prefix}D{i}"),
            "11",
            d,
            [x0 + 2., 3.],
            4.,
            0.,
        ));
    }
    v
}
fn glyph_blocks() -> Vec<(&'static str, &'static str, Vec<String>)> {
    let u = [0.5f64, 0.866];
    let n = [-0.866f64, 0.5];
    let c = [5.0f64, 5.0];
    let tangent = |k: f64, id: &str| {
        line(
            id,
            "0",
            [
                c[0] + n[0] * 3. * k - u[0] * 4.5,
                c[1] + n[1] * 3. * k - u[1] * 4.5,
            ],
            [
                c[0] + n[0] * 3. * k + u[0] * 4.5,
                c[1] + n[1] * 3. * k + u[1] * 4.5,
            ],
        )
    };
    let head = |id: &str, tip: [f64; 2]| {
        lwpoly(
            id,
            "0",
            &[
                tip,
                [tip[0] - 1.6, tip[1] - 0.6],
                [tip[0] - 0.6, tip[1] - 1.6],
                tip,
            ],
            false,
        )
    };
    vec![
        (
            "gps_straightness",
            "GSTR",
            vec![line("a", "0", [1., 5.], [9., 5.])],
        ),
        (
            "gps_flatness",
            "GFLT",
            vec![
                line("a", "0", [1., 2.], [7., 2.]),
                line("b", "0", [7., 2.], [9., 8.]),
                line("c", "0", [9., 8.], [3., 8.]),
                line("d", "0", [3., 8.], [1., 2.]),
            ],
        ),
        (
            "gps_circularity",
            "GCIR",
            vec![circle("a", "0", [5., 5.], 3.5)],
        ),
        (
            "gps_cylindricity",
            "GCYL",
            vec![circle("a", "0", c, 3.), tangent(1., "b"), tangent(-1., "c")],
        ),
        (
            "gps_line_profile",
            "GLPR",
            vec![arc("a", "0", [5., 3.], 4., 0., 180.)],
        ),
        (
            "gps_surface_profile",
            "GSPR",
            vec![
                arc("a", "0", [5., 3.], 4., 0., 180.),
                line("b", "0", [1., 3.], [9., 3.]),
            ],
        ),
        (
            "gps_parallelism",
            "GPAR",
            vec![
                line("a", "0", [1., 1.], [5., 9.]),
                line("b", "0", [5., 1.], [9., 9.]),
            ],
        ),
        (
            "gps_perpendicularity",
            "GPER",
            vec![
                line("a", "0", [1., 2.], [9., 2.]),
                line("b", "0", [5., 2.], [5., 9.]),
            ],
        ),
        (
            "gps_angularity",
            "GANG",
            vec![
                line("a", "0", [1., 2.], [9., 2.]),
                line("b", "0", [1., 2.], [8.8, 6.5]),
            ],
        ),
        (
            "gps_position",
            "GPOS",
            vec![
                circle("a", "0", [5., 5.], 2.5),
                line("b", "0", [1., 5.], [9., 5.]),
                line("c", "0", [5., 1.], [5., 9.]),
            ],
        ),
        (
            "gps_coaxiality",
            "GCOA",
            vec![
                circle("a", "0", [5., 5.], 1.8),
                circle("b", "0", [5., 5.], 3.6),
            ],
        ),
        (
            "gps_symmetry",
            "GSYM",
            vec![
                line("a", "0", [2., 2.], [8., 2.]),
                line("b", "0", [0.5, 5.], [9.5, 5.]),
                line("c", "0", [2., 8.], [8., 8.]),
            ],
        ),
        (
            "gps_runout",
            "GRUN",
            vec![line("a", "0", [2., 1.], [8., 8.]), head("b", [8., 8.])],
        ),
        (
            "gps_total_runout",
            "GTRU",
            vec![
                line("a", "0", [1., 1.], [5., 8.]),
                head("b", [5., 8.]),
                line("c", "0", [4., 1.], [8., 8.]),
                head("d", [8., 8.]),
                line("e", "0", [1., 1.], [9., 1.]),
            ],
        ),
    ]
}

#[test]
fn every_characteristic_stroke_signature_gives_its_kind() {
    let defs = glyph_blocks();
    let blk = blocks(
        &defs
            .iter()
            .map(|(_, n, e)| (*n, e.clone()))
            .collect::<Vec<_>>(),
    );
    let mut ents = Vec::new();
    for (i, (_, n, _)) in defs.iter().enumerate() {
        ents.extend(frame(&format!("F{i}"), i as f64 * 60., n, "0.02", &["A"]));
    }
    let g = run(&blk, &ents);
    let card = representation_knowledge()
        .unwrap()
        .cards
        .iter()
        .find(|c| c.id == "gdt_characteristic")
        .unwrap();
    for (i, (kind, _, _)) in defs.iter().enumerate() {
        let e = expr_with(&g, &format!("F{i}G"), kind).unwrap_or_else(|| {
            panic!(
                "{kind}: {:?}",
                exprs_of(&g, &format!("F{i}G"))
                    .iter()
                    .map(|e| &e.kind)
                    .collect::<Vec<_>>()
            )
        });
        let concept = card
            .lexicon
            .iter()
            .flatten()
            .find(|l| l.entry == "kind" && l.values.contains(&kind.to_string()))
            .unwrap()
            .concept
            .clone();
        assert!(
            e.concept_ids.contains(&concept) && has(e, &["DC-G5-090", "DC-G5-091", "DC-G5-003"]),
            "{kind}: {:?}",
            e.concept_ids
        );
    }
}

#[test]
fn flatness_frame_with_stroke_glyph_block() {
    let defs = glyph_blocks();
    let (_, n, e) = defs.iter().find(|d| d.0 == "gps_flatness").unwrap();
    let g = run(&blocks(&[(n, e.clone())]), &frame("P", 0., n, "0.06", &[]));
    let x = expr_with(&g, "PG", "gps_flatness").expect("flatness frame");
    assert!(has(x, &["DC-G5-061", "DC-G5-090", "DC-G5-091"]));
    assert!(x.label.starts_with('⏥'), "{}", x.label);
}

#[test]
fn characteristic_written_as_text_glyph() {
    let mut ents = vec![
        rect("C1", "02", 0., 0., 10., 10.),
        rect("C2", "02", 10., 0., 26., 10.),
        text("V", "11", "0.06", [12., 3.], 4., 0.),
        text("T", "11", "⏥", [2.5, 2.5], 6., 0.),
    ];
    ents.extend([
        rect("K1", "02", 100., 0., 110., 10.),
        rect("K2", "02", 110., 0., 126., 10.),
        text("KV", "11", "0.01", [112., 3.], 4., 0.),
        styled_text("KT", "11", "u", [102.5, 2.5], 6., "GDTSTYLE"),
    ]);
    let g = run_with(true, "", &ents);
    assert!(
        expr_with(&g, "T", "gps_flatness").is_some(),
        "{:?}",
        exprs_of(&g, "T")
            .iter()
            .map(|e| &e.kind)
            .collect::<Vec<_>>()
    );
    assert!(
        expr_with(&g, "KT", "gps_straightness").is_some(),
        "{:?}",
        exprs_of(&g, "KT")
            .iter()
            .map(|e| &e.kind)
            .collect::<Vec<_>>()
    );
}

#[test]
fn right_angle_rectangle_is_not_flatness() {
    let sq = vec![
        line("a", "0", [2., 2.], [8., 2.]),
        line("b", "0", [8., 2.], [8., 8.]),
        line("c", "0", [8., 8.], [2., 8.]),
        line("d", "0", [2., 8.], [2., 2.]),
    ];
    let g = run(&blocks(&[("GSQ", sq)]), &frame("R", 0., "GSQ", "0.06", &[]));
    assert!(!g.expressions.iter().any(|e| e.kind == "gps_flatness"));
}

#[test]
fn roughness_v_is_not_angularity() {
    let v = vec![
        line("a", "0", [0., 0.], [-2.5, 4.33]),
        line("b", "0", [0., 0.], [5., 8.66]),
    ];
    let g = run(&blocks(&[("GV", v)]), &frame("Q", 0., "GV", "0.06", &[]));
    assert!(!g.expressions.iter().any(|e| e.kind == "gps_angularity"));
}

#[test]
fn frame_fields_name_value_zone_and_datums() {
    let defs = glyph_blocks();
    let pos = defs.iter().find(|d| d.0 == "gps_position").unwrap();
    let run_out = defs.iter().find(|d| d.0 == "gps_runout").unwrap();
    let mut ents = frame("P", 0., pos.1, "Ø0.1", &["A", "B"]);
    ents.extend(frame("R", 100., run_out.1, "0.04", &["A-B"]));
    let g = run(
        &blocks(&[(pos.1, pos.2.clone()), (run_out.1, run_out.2.clone())]),
        &ents,
    );
    let p = expr_with(&g, "PG", "gps_position").expect("position frame");
    assert!(
        has(
            p,
            &[
                "DC-G5-041",
                "DC-G5-090",
                "DC-G5-091",
                "DC-G5-089",
                "DC-G5-003",
                "DC-G5-004"
            ]
        ),
        "{:?}",
        p.concept_ids
    );
    let r = expr_with(&g, "RG", "gps_runout").expect("runout frame");
    assert!(
        has(r, &["DC-G5-057", "DC-G5-002"]) && !has(r, &["DC-G5-004"]),
        "{:?}",
        r.concept_ids
    );
}

#[test]
fn frame_drawn_as_outline_with_dividers() {
    let defs = glyph_blocks();
    let sym = defs.iter().find(|d| d.0 == "gps_symmetry").unwrap();
    let ents = vec![
        insert("OG", "01", sym.1, [0., 0.], 0.),
        lwpoly(
            "OO",
            "02",
            &[[0., 0.], [34., 0.], [34., 10.], [0., 10.]],
            true,
        ),
        line("OD1", "02", [10., 0.], [10., 10.]),
        line("OD2", "02", [26., 0.], [26., 10.]),
        text("OV", "11", "0.06", [12., 3.], 4., 0.),
        text("OR", "11", "D", [28., 3.], 4., 0.),
    ];
    let g = run(&blocks(&[(sym.1, sym.2.clone())]), &ents);
    let e = expr_with(&g, "OG", "gps_symmetry").expect("frame drawn as one outline with dividers");
    assert!(
        e.evidence_ids.contains(&"variant:outline_dividers".into()),
        "{:?}",
        e.evidence_ids
    );
    for s in ["OO", "OD1", "OD2", "OV", "OR"] {
        assert!(
            expr_with(&g, s, "gps_symmetry").is_some(),
            "{s} not in the frame"
        );
    }
    assert!(has(
        e,
        &["DC-G5-059", "DC-G5-090", "DC-G5-091", "DC-G5-003"]
    ));
}

#[test]
fn single_datum_frame_is_not_a_datum_system() {
    let defs = glyph_blocks();
    let sym = defs.iter().find(|d| d.0 == "gps_symmetry").unwrap();
    let g = run(
        &blocks(&[(sym.1, sym.2.clone())]),
        &frame("S", 0., sym.1, "0.02", &["D"]),
    );
    let e = expr_with(&g, "SG", "gps_symmetry").unwrap();
    assert!(has(e, &["DC-G5-003"]) && !has(e, &["DC-G5-004", "DC-G5-002"]));
}

// ---------------------------------------------------------------- surface_texture_symbol

/// Upright symbol: tip at the origin, short arm up-left (5), long arm up-right (10), 60° apart.
fn texture(extra: Vec<String>) -> Vec<String> {
    let mut v = vec![
        line("s", "0", [0., 0.], [-2.5, 4.33]),
        line("l", "0", [0., 0.], [5., 8.66]),
    ];
    v.extend(extra);
    v
}

#[test]
fn texture_symbol_types_name_their_concepts() {
    let defs = [
        (
            "RBAS",
            texture(vec![text("t", "0", "3.2", [-1., 6.], 2.5, 0.)]),
        ),
        (
            "RREM",
            texture(vec![
                line("bar", "0", [-2.5, 4.33], [2.5, 4.33]),
                text("t", "0", "3.2", [-1., 6.], 2.5, 0.),
            ]),
        ),
        (
            "RNMR",
            texture(vec![
                circle("o", "0", [0., 2.6], 1.2),
                text("t", "0", "3.2", [-1., 6.], 2.5, 0.),
            ]),
        ),
        (
            "RCMP",
            texture(vec![
                line("ext", "0", [5., 8.66], [13., 8.66]),
                text("t", "0", "3.2", [6., 6.], 2.5, 0.),
            ]),
        ),
    ];
    let blk = blocks(
        &defs
            .iter()
            .map(|(n, e)| (*n, e.clone()))
            .collect::<Vec<_>>(),
    );
    let ents: Vec<String> = defs
        .iter()
        .enumerate()
        .map(|(i, (n, _))| insert(&format!("I{n}"), "11", n, [i as f64 * 50., 100.], 0.))
        .collect();
    let g = run(&blk, &ents);
    let get = |n: &str| {
        expr_with(&g, &format!("I{n}"), "surface_roughness").unwrap_or_else(|| panic!("{n}"))
    };
    assert!(
        has(get("RBAS"), &["DC-G6-071", "DC-G6-045", "DC-G6-027"])
            && !has(get("RBAS"), &["DC-G6-072"])
    );
    assert!(has(get("RREM"), &["DC-G6-072"]));
    assert!(has(get("RNMR"), &["DC-G6-063"]));
    assert!(has(get("RCMP"), &["DC-G6-069"]));
    assert!(
        !has(get("RBAS"), &["DC-G6-049"]),
        "upright symbols are read from below"
    );
}

#[test]
fn texture_value_editions() {
    let defs = [
        (
            "RNEW",
            texture(vec![
                line("ext", "0", [5., 8.66], [13., 8.66]),
                text("t", "0", "Ra 3.2", [6., 6.], 2.5, 0.),
            ]),
        ),
        (
            "ROLD",
            texture(vec![text("t", "0", "6.3", [-1., 6.], 2.5, 0.)]),
        ),
    ];
    let blk = blocks(
        &defs
            .iter()
            .map(|(n, e)| (*n, e.clone()))
            .collect::<Vec<_>>(),
    );
    let g = run(
        &blk,
        &[
            insert("INEW", "11", "RNEW", [0., 0.], 0.),
            insert("IOLD", "11", "ROLD", [50., 0.], 0.),
        ],
    );
    let new = expr_with(&g, "INEW", "surface_roughness").expect("parameter value in the block");
    assert!(
        new.rule.ends_with("block_with_parameter_value"),
        "{}",
        new.rule
    );
    assert!(
        has(new, &["DC-G6-046", "DC-G6-075", "DC-G6-045", "DC-G6-069"])
            && !has(new, &["DC-G6-027"]),
        "{:?}",
        new.concept_ids
    );
    let old = expr_with(&g, "IOLD", "surface_roughness").unwrap();
    assert!(has(old, &["DC-G6-027"]) && !has(old, &["DC-G6-075"]));
}

#[test]
fn rotated_texture_block_with_value_beside_it() {
    // the block opens upwards; inserted at 90° it opens to the left, the value written beside it
    let g = run(
        &blocks(&[(
            "RBLK",
            texture(vec![line("bar", "0", [-2.5, 4.33], [2.5, 4.33])]),
        )]),
        &[
            insert("RI", "11", "RBLK", [100., 50.], 90.),
            text("RV", "11", "1.6", [91., 49.], 2.5, 90.),
        ],
    );
    let e = expr_with(&g, "RI", "surface_roughness").expect("symbol with its value beside it");
    assert!(e.rule.ends_with("block_value_apart"));
    assert!(
        expr_with(&g, "RV", "surface_roughness").is_some(),
        "value joins the symbol"
    );
    assert!(
        has(e, &["DC-G6-049", "DC-G6-072", "DC-G6-027"]),
        "{:?}",
        e.concept_ids
    );
}

#[test]
fn texture_symbol_drawn_with_loose_strokes() {
    let ents = vec![
        line("LS", "02", [50., 50.], [47.5, 54.33]),
        line("LL", "02", [50., 50.], [55., 58.66]),
        text("LV", "11", "6.3", [48., 56.], 2.5, 0.),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "LS", "surface_roughness").expect("loose strokes symbol");
    assert!(e.rule.ends_with("loose_strokes"));
    assert!(
        expr_with(&g, "LL", "surface_roughness").is_some()
            && expr_with(&g, "LV", "surface_roughness").is_some()
    );
}

#[test]
fn texture_tip_on_outline_and_extension_line() {
    let sym = texture(vec![text("t", "0", "3.2", [-1., 6.], 2.5, 0.)]);
    let ents = vec![
        line("OL", "01", [0., 0.], [80., 0.]),
        insert("SO", "11", "RTIP", [40., 0.], 0.),
        // exploded dimension whose extension line carries the second symbol
        line("H1", "08", [100., 50.], [120., 50.]),
        solid("HA", "08", [100., 50.], [103., 50.]),
        solid("HB", "08", [120., 50.], [117., 50.]),
        text("HT", "08", "20", [108.6, 50.4], 2.5, 0.),
        line("HE1", "02", [100., 30.], [100., 52.]),
        line("HE2", "02", [120., 30.], [120., 52.]),
        insert("SE", "11", "RTIP", [100., 38.], 90.),
    ];
    let g = run(&blocks(&[("RTIP", sym)]), &ents);
    let o = expr_with(&g, "SO", "surface_roughness").unwrap();
    assert!(has(o, &["DC-G6-053"]), "{:?}", o.concept_ids);
    let e = expr_with(&g, "SE", "surface_roughness").unwrap();
    assert!(
        has(e, &["DC-G6-052"]),
        "{:?} {:?}",
        e.concept_ids,
        exprs_of(&g, "HE1")
            .iter()
            .map(|x| &x.kind)
            .collect::<Vec<_>>()
    );
}

#[test]
fn remaining_surfaces_texture_note() {
    let sym = texture(vec![
        text("t", "0", "12.5", [-1., 6.], 2.5, 0.),
        text("q", "0", "其余", [-14., 2.], 3.5, 0.),
    ]);
    let g = run(
        &blocks(&[("RREST", sym)]),
        &[insert("RR", "11", "RREST", [300., 300.], 0.)],
    );
    let e = expr_with(&g, "RR", "surface_roughness").unwrap();
    assert!(has(e, &["DC-G6-006"]), "{:?}", e.concept_ids);
}

#[test]
fn texture_symbol_without_value() {
    let g = run(
        &blocks(&[("RONLY", texture(vec![]))]),
        &[insert("RO", "11", "RONLY", [0., 0.], 0.)],
    );
    let e = expr_with(&g, "RO", "surface_roughness").expect("bare symbol");
    assert!(
        e.label == "√" && has(e, &["DC-G6-071"]) && !has(e, &["DC-G6-045"]),
        "{} {:?}",
        e.label,
        e.concept_ids
    );
}

#[test]
fn chamfer_lines_are_not_a_texture_symbol() {
    let ents = vec![
        line("CA", "02", [0., 0.], [4., 4.]),
        line("CB", "02", [0., 0.], [8., 0.]),
        text("CT", "11", "C1", [5., 5.], 2.5, 0.),
    ];
    let g = run("", &ents);
    assert!(!g.expressions.iter().any(|e| e.kind == "surface_roughness"));
}

// ---------------------------------------------------------------- view_callout

fn callout(stroke: [[f64; 2]; 2]) -> Vec<String> {
    vec![
        line("AS", "02", [8., 0.], [17.5, 0.]),
        solid("AH", "02", [20., 0.], [17., 0.]),
        text("AL", "11", "A", [5.5, 2.], 3.5, 0.),
        line("AK", "01", stroke[0], stroke[1]),
    ]
}

#[test]
fn cutting_plane_stroke_joins_the_arrow_and_letter() {
    let g = run("", &callout([[8., -6.], [8., 0.]]));
    let e = expr_with(&g, "AL", "view_callout").expect("viewing direction");
    assert!(g.members.iter().any(|m| m.expression_id == e.id
        && m.source_handle == "AK"
        && m.member_role == "cutting_stroke"));
    assert!(
        has(e, &["DC-G2-043", "DC-G1-007", "DC-G2-072"]),
        "{:?}",
        e.concept_ids
    );
}

#[test]
fn stroke_far_from_arrow_stays_apart() {
    let g = run("", &callout([[30., -6.], [30., 0.]]));
    assert!(
        !g.members
            .iter()
            .any(|m| m.source_handle == "AK" && m.member_role == "cutting_stroke")
    );
}

#[test]
fn section_name_titles_a_section_view() {
    let ents = vec![
        line("B1", "01", [0., 0.], [40., 0.]),
        line("B2", "01", [40., 0.], [40., 20.]),
        line("B3", "01", [40., 20.], [0., 20.]),
        line("B4", "01", [0., 20.], [0., 0.]),
        text("SN", "11", "A—A", [14., 26.], 5., 0.),
        line("C1", "01", [100., 0.], [140., 0.]),
        line("C2", "01", [140., 0.], [140., 20.]),
        line("C3", "01", [140., 20.], [100., 20.]),
        line("C4", "01", [100., 20.], [100., 0.]),
        text("SM", "11", "B-B 2:1", [110., 26.], 5., 0.),
    ];
    let g = run("", &ents);
    let a = expr_with(&g, "SN", "view_callout").expect("section name");
    assert!(has(a, &["DC-G2-065"]) && !has(a, &["DC-G2-068"]));
    let b = expr_with(&g, "SM", "view_callout").expect("section name with scale");
    assert!(has(b, &["DC-G2-065", "DC-G2-068", "DC-G1-115"]));
    assert!(
        g.views
            .iter()
            .any(|v| v.kind == "section_view" && v.label == "A—A"),
        "{:?}",
        g.views
            .iter()
            .map(|v| (&v.kind, &v.label))
            .collect::<Vec<_>>()
    );
}

#[test]
fn direction_view_names_are_tagged() {
    let g = run(
        "",
        &[
            text("DV", "11", "C向", [0., 30.], 5., 0.),
            line("X1", "01", [0., 0.], [20., 0.]),
            line("X2", "01", [20., 0.], [20., 20.]),
        ],
    );
    let e = expr_with(&g, "DV", "view_callout").unwrap();
    assert!(has(e, &["DC-G2-072"]));
}

// ---------------------------------------------------------------- coverage table and concept statuses

#[test]
fn coverage_table_lists_rules_found_in_the_sources() {
    let k = recognizer_coverage().expect("coverage table parses and is consistent with the cards");
    let src = [
        include_str!("../src/engineering_expression.rs"),
        include_str!("../src/representation.rs"),
        include_str!("../src/notation.rs"),
        include_str!("../src/symbols.rs"),
    ]
    .concat();
    let cards = representation_knowledge().unwrap();
    for r in &k.rules {
        if let Some(rest) = r.rule.strip_prefix("card:") {
            let (card, variant) = rest.split_once('/').unwrap_or((rest, ""));
            let c = cards
                .cards
                .iter()
                .find(|c| c.id == card)
                .unwrap_or_else(|| panic!("{}", r.rule));
            assert!(
                variant.is_empty() || c.variants.iter().any(|v| v.id == variant),
                "{}",
                r.rule
            );
        } else {
            assert!(
                src.contains(&format!("\"{}\"", r.rule)),
                "{} not in the sources",
                r.rule
            );
        }
        assert!(!r.kinds.is_empty(), "{}", r.rule);
    }
    for c in &k.concepts {
        assert!(
            c.level == "full" || c.missing.as_deref().is_some_and(|m| !m.is_empty()),
            "{}",
            c.id
        );
    }
}

#[test]
fn concept_statuses_follow_the_coverage_table() {
    let groups = drawing_concepts().expect("catalogue statuses agree with the coverage table");
    let mut n = std::collections::BTreeMap::new();
    for c in groups.iter().flat_map(|g| g.concepts.iter()) {
        assert_eq!(
            concept_status(&c.id, !c.recognition_cards.is_empty()).unwrap(),
            c.recognizer,
            "{}",
            c.id
        );
        *n.entry(c.recognizer.clone()).or_insert(0) += 1;
    }
    assert!(n.get("card").copied().unwrap_or(0) >= 80, "{n:?}");
    let k = representation_knowledge().unwrap();
    for l in k.cards.iter().flat_map(|c| c.lexicon.iter().flatten()) {
        assert!(
            groups
                .iter()
                .flat_map(|g| g.concepts.iter())
                .any(|c| c.id == l.concept),
            "lexicon concept {}",
            l.concept
        );
        assert!(!l.values.is_empty() && !l.meaning.is_empty());
    }
}
