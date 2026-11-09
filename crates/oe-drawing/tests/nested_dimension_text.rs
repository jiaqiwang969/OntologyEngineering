//! A dimension whose displayed text sits in block references nested in its own block, the way CAXA
//! writes a toleranced value: "%%C55(", the deviations "+0.03" over "0" stacked, and ")", each in a
//! block of its own. Authored synthetic source: handles, coordinates and block names are invented.
use oe_contracts::MdEngineeringExpressionGraph;
use oe_drawing::engineering_expression::discover;

fn ent(kind: &str, id: &str, layer: &str, tags: &str) -> String {
    format!("0\n{kind}\n5\n{id}\n8\n{layer}\n330\nMODEL\n{tags}")
}
fn line(id: &str, a: [f64; 2], b: [f64; 2]) -> String {
    ent(
        "LINE",
        id,
        "0",
        &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
    )
}
fn insert(id: &str, block: &str, at: [f64; 2], rotation: f64) -> String {
    ent(
        "INSERT",
        id,
        "0",
        &format!(
            "2\n{block}\n10\n{}\n20\n{}\n41\n1\n42\n1\n50\n{rotation}\n",
            at[0], at[1]
        ),
    )
}
/// One MTEXT piece at `at` (block coordinates), rotated by `rotation` degrees.
fn piece(id: &str, content: &str, at: [f64; 2], rotation: f64) -> String {
    ent(
        "MTEXT",
        id,
        "0",
        &format!(
            "1\n{content}\n10\n{}\n20\n{}\n40\n2.5\n50\n{rotation}\n71\n1\n",
            at[0], at[1]
        ),
    )
}
/// The dimension; `turn` rotates its block reference, `text_turn` the pieces themselves (as
/// CAXA writes a vertical dimension: references unrotated, MTEXT at 90 degrees).
fn source(turn: f64, text_turn: f64) -> Vec<u8> {
    let rot = |deg: f64, x: f64, y: f64| {
        let r = deg.to_radians();
        [x * r.cos() - y * r.sin(), x * r.sin() + y * r.cos()]
    };
    let at = |x: f64, y: f64| rot(turn + text_turn, x, y);
    let tat = |x: f64, y: f64| rot(text_turn, x, y);
    // CAXA's pieces: the deviations centred one above the other, off the line of the value.
    let pieces = [
        piece("B1", "\\T1.1;%%C55(", tat(10., 40.), text_turn),
        piece(
            "B2",
            "\\T1.1;{\\H0.707107x;+0.03}",
            tat(19., 41.6),
            text_turn,
        ),
        piece("B3", "\\T1.1;{\\H0.707107x;0}", tat(20.4, 38.4), text_turn),
        piece("B4", "\\T1.1;)", tat(22.5, 40.), text_turn),
    ];
    let texts = format!(
        "0\nBLOCK\n2\n*X16\n10\n0\n20\n0\n{}0\nENDBLK\n",
        pieces.concat()
    );
    let own = [
        line("C1", at(0., 38.), at(55., 38.)),
        line("C2", at(0., 30.), at(0., 39.)),
        line("C3", at(55., 30.), at(55., 39.)),
        insert("C4", "*X16", [0., 0.], turn),
    ];
    let dim_block = format!(
        "0\nBLOCK\n2\n*X15\n10\n0\n20\n0\n{}0\nENDBLK\n",
        own.concat()
    );
    let (p13, p14, p10) = (at(0., 30.), at(55., 30.), at(55., 38.));
    let dim = ent(
        "DIMENSION",
        "D1",
        "0",
        &format!(
            "2\n*X15\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n70\n32\n13\n{}\n23\n{}\n14\n{}\n24\n{}\n42\n55\n50\n{}\n",
            p10[0],
            p10[1],
            at(27.5, 40.)[0],
            at(27.5, 40.)[1],
            p13[0],
            p13[1],
            p14[0],
            p14[1],
            turn + text_turn
        ),
    );
    format!(
        "0\nSECTION\n2\nTABLES\n0\nLTYPE\n2\nCONTINUOUS\n73\n0\n0\nLAYER\n2\n0\n62\n7\n6\nCONTINUOUS\n0\nENDSEC\n\
         0\nSECTION\n2\nBLOCKS\n{texts}{dim_block}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{dim}0\nENDSEC\n0\nEOF\n"
    )
    .into_bytes()
}
fn dimension(g: &MdEngineeringExpressionGraph) -> &oe_contracts::MdEngineeringExpression {
    g.expressions
        .iter()
        .find(|e| e.evidence_ids.iter().any(|x| x == "source:D1"))
        .expect("the dimension's expression")
}

#[test]
fn a_dimension_text_in_nested_blocks_is_read_with_its_stacked_tolerance() {
    for (turn, text_turn) in [(0., 0.), (90., 0.), (0., 90.)] {
        let g = discover(&source(turn, text_turn)).unwrap();
        let e = dimension(&g);
        assert_eq!(e.kind, "dimension_diameter", "{turn} {text_turn}");
        assert_eq!(e.label, "Ø55( +0.03/0)", "{turn} {text_turn}");
        assert!(
            e.qualifiers.contains(&"toleranced".into()),
            "{:?}",
            e.qualifiers
        );
        assert!(
            e.evidence_ids
                .iter()
                .any(|x| x == "display_text_nested_blocks"),
            "{:?}",
            e.evidence_ids
        );
    }
}
