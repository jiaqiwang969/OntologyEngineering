//! The review page embeds the frozen graph and the rendering unchanged, and refuses renderings with active
//! content or identities absent from the graph. Graph from the classifier on the original public I1 drawing.
use oe_contracts::MdReviewPageRequest;
use serde_json::{Value, json};
use std::path::Path;

fn example(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/drawing-understanding/i1-stepped-shaft")
            .join(name),
    )
    .unwrap()
}

fn request() -> MdReviewPageRequest {
    MdReviewPageRequest {
        schema: oe_drawing::page::REQUEST_SCHEMA.into(),
        graph: "graph.json".into(),
        svg: "source.svg".into(),
        title: "I1 stepped shaft".into(),
        candidate: "test".into(),
        version: "test".into(),
        source_note: String::new(),
        status_note: String::new(),
        output: "page.html".into(),
    }
}

fn payload(page: &str) -> Value {
    let start = page
        .find("<script type=\"application/json\" id=\"payload\">")
        .unwrap()
        + "<script type=\"application/json\" id=\"payload\">".len();
    let end = start + page[start..].find("</script>").unwrap();
    serde_json::from_str(&page[start..end]).unwrap()
}

#[test]
fn the_review_page_keeps_source_identities_and_refuses_active_svg() {
    let dxf = example("source.dxf");
    let graph =
        serde_json::to_vec(&oe_drawing::engineering_expression::discover(&dxf).unwrap()).unwrap();
    let svg = String::from_utf8(example("source.svg")).unwrap();
    let (text, result) = oe_drawing::page::page(&graph, &svg, &request()).unwrap();
    assert!(text.contains(&svg) && !text.contains("@@DATA@@") && !text.contains("@@SVG@@"));
    let data = payload(&text);
    let g: Value = serde_json::from_slice(&graph).unwrap();
    assert_eq!(data["graph"]["members"], g["members"]);
    assert_eq!(data["graph"]["expressions"], g["expressions"]);
    assert_eq!(data["graph"]["entities"], g["source"]["entities"]);
    assert_eq!(data["graph_sha256"], oe_core::sha256(&graph));
    let rendered = oe_drawing::page::rendering(svg.as_bytes())
        .unwrap()
        .entities;
    assert_eq!(result.rendered_root_count as usize, rendered.len());
    assert_eq!(
        result.root_count as usize,
        g["source"]["entities"].as_array().unwrap().len()
    );
    assert!(!rendered.is_empty());
    // A rendering with active content, or with an identity the graph does not have, is refused.
    for (bad, reason) in [
        (
            svg.replacen("<rect", "<script>alert(1)</script><rect", 1),
            "review_page_svg_active_content",
        ),
        (
            svg.replacen("<rect", "<rect onload=\"x()\"", 1),
            "review_page_svg_active_content",
        ),
        (
            svg.replacen("<rect", "<a href=\"https://example.invalid\"></a><rect", 1),
            "review_page_svg_active_content",
        ),
        (
            svg.replacen("<rect", "<g data-entity=\"NOT-A-ROOT\"></g><rect", 1),
            "review_page_svg_roots_absent_from_graph",
        ),
    ] {
        let error = oe_drawing::page::page(&graph, &bad, &request())
            .unwrap_err()
            .to_string();
        assert!(error.starts_with(reason), "{error}");
    }
    // Partial intervals are shown only on qualified LINE sources.
    let mut changed = g.clone();
    let other = g["source"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["geometry_kind"] != "line")
        .map(|e| e["id"].clone());
    if let Some(id) = other {
        let i = g["members"]
            .as_array()
            .unwrap()
            .iter()
            .position(|m| m["source_handle"] == id)
            .unwrap();
        changed["members"][i]["interval"] = json!([0.0, 0.5]);
        let error =
            oe_drawing::page::page(&serde_json::to_vec(&changed).unwrap(), &svg, &request())
                .unwrap_err()
                .to_string();
        assert!(
            error.starts_with("review_page_partial_interval_on_unqualified_source"),
            "{error}"
        );
    }
}

/// Masks (a wipeout's, the background behind a dimension's text) are not recoloured with the
/// expression colours: a recoloured mask hides the text drawn over it in the same colour. The
/// browser check of every page mode (`review-page-checks.js`) verifies the computed colours.
#[test]
fn the_review_page_keeps_masks_in_the_paper_colour() {
    let dxf = example("source.dxf");
    let graph =
        serde_json::to_vec(&oe_drawing::engineering_expression::discover(&dxf).unwrap()).unwrap();
    let svg = String::from_utf8(example("source.svg")).unwrap();
    let (text, _) = oe_drawing::page::page(&graph, &svg, &request()).unwrap();
    let recolour = text.find("n.setAttribute(attr,'currentColor')").unwrap();
    let skip = text.find("if(isMask(n))continue;").unwrap();
    assert!(skip < recolour, "masks are skipped before recolouring");
    assert!(text.contains("MARKED?n.hasAttribute('data-mask')"));
}

/// Line widths come from the recognised line types (two widths, 2:1, d from the drawing's own dimension
/// terminators), never from constants of the page; the original view keeps the converter's rendering. The
/// behaviour itself is proved in the browser (`the_browser_checks_pass_on_a_synthetic_page_and_catch_each_broken_rule`
/// and every route run); this pins that the template has the rule and no longer the three fixed widths.
#[test]
fn the_review_page_template_takes_widths_from_line_types_and_reads_member_identity() {
    let template = String::from_utf8(
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/expression-review.html"))
            .unwrap(),
    )
    .unwrap();
    for gone in ["'.62'", "'.32'", "'.45'"] {
        assert!(!template.contains(gone), "{gone}");
    }
    for kept in [
        "ARROW_PER_D=6",
        ".canvas>svg:not(.as-converted)>g[data-entity]>[stroke-width]",
        "svg.classList.toggle('as-converted',raw)",
        "g.dataset.member",
        ".not-plotted",
        "group.dataset.seq",
    ] {
        assert!(template.contains(kept), "{kept}");
    }
    for kept in [
        "function highlightOccurrences(occurrences)",
        "id=\"complete\" checked",
        "id=\"original\">原图对照",
    ] {
        assert!(template.contains(kept), "{kept}");
    }
}

#[test]
fn drafting_issue_panel_groups_by_rule_and_keeps_interval_highlight() {
    let comparison = String::from_utf8(
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/drawing-comparison.js"))
            .unwrap(),
    )
    .unwrap();
    for kept in [
        "group.dataset.ruleId=rule",
        "first.rule_label||rule",
        "sourceView.highlightOccurrences(item.source_occurrences)",
        "i.rule_label||rule",
    ] {
        assert!(comparison.contains(kept), "{kept}");
    }
}

/// A group drawn for a primitive nested in a block reference or a native dimension may name its native
/// member (data-member). The page accepts the identity only when the graph has that member nested in the
/// root the group names, embeds the kinds of the nested members, and still refuses active content.
#[test]
fn the_review_page_accepts_a_member_identity_only_inside_its_own_root() {
    let dxf = example("source.dxf");
    let graph =
        serde_json::to_vec(&oe_drawing::engineering_expression::discover(&dxf).unwrap()).unwrap();
    let g: Value = serde_json::from_slice(&graph).unwrap();
    let svg = String::from_utf8(example("source.svg")).unwrap();
    let rendered = oe_drawing::page::rendering(svg.as_bytes()).unwrap();
    assert!(rendered.members.is_empty());
    // Two rendered roots with nested native members.
    let nested: Vec<(String, String)> = g["source"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| rendered.entities.contains(e["id"].as_str().unwrap()))
        .filter_map(|e| {
            let id = e["id"].as_str().unwrap();
            e["member_ids"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .find(|m| *m != id)
                .map(|m| (id.to_owned(), m.to_owned()))
        })
        .collect();
    assert!(nested.len() >= 2, "{nested:?}");
    let ((root, member), other_member) = (&nested[0], &nested[1].1);
    let named = |attributes: &str| {
        let plain = format!("<g data-entity=\"{root}\">");
        assert!(svg.contains(&plain));
        svg.replacen(
            &plain,
            &format!("<g data-entity=\"{root}\"{attributes}>"),
            1,
        )
    };
    let with_member = named(&format!(" data-member=\"{member}\""));
    let shown = oe_drawing::page::rendering(with_member.as_bytes()).unwrap();
    assert_eq!(
        shown.members.iter().collect::<Vec<_>>(),
        [&(root.clone(), member.clone())]
    );
    assert_eq!(shown.entities, rendered.entities);
    let (text, _) = oe_drawing::page::page(&graph, &with_member, &request()).unwrap();
    let data = payload(&text);
    assert_eq!(data["rendered_member_identities"], 1);
    assert!(data["native_member_kinds"][member].is_string(), "{member}");
    assert!(
        data["native_member_kinds"][root].is_null(),
        "roots are not nested members"
    );
    // The members reach the page with the native member each one names.
    assert!(
        data["graph"]["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["native_member_id"] == member.as_str())
    );
    for (bad, reason) in [
        (
            named(&format!(" data-member=\"{root}/NOT-A-MEMBER\"")),
            "review_page_svg_member_absent_from_root",
        ),
        // A member of another root.
        (
            named(&format!(" data-member=\"{other_member}\"")),
            "review_page_svg_member_absent_from_root",
        ),
        // The root itself is not a nested member.
        (
            named(&format!(" data-member=\"{root}\"")),
            "review_page_svg_member_absent_from_root",
        ),
        // A member identity without the root it is drawn for.
        (
            svg.replacen(
                "<rect",
                &format!("<g data-member=\"{other_member}\"></g><rect"),
                1,
            ),
            "review_page_svg_member_absent_from_root",
        ),
        // Active content stays refused beside a valid member identity.
        (
            named(&format!(" data-member=\"{member}\" onclick=\"x()\"")),
            "review_page_svg_active_content",
        ),
    ] {
        let error = oe_drawing::page::page(&graph, &bad, &request())
            .unwrap_err()
            .to_string();
        assert!(error.starts_with(reason), "{error}");
    }
}

/// A synthetic drawing for the page: invented handles and numbers, built as a graph and the rendering a
/// converter would give for it. Outline with a hidden line, a centre line and a leader on the same source
/// line; two native dimensions with filled terminators 9 long and a text background each (the second one's
/// own line runs through its number); a requirement block whose nested texture symbol has a value with a
/// background; a detail view stored as a block (outline, section lines, boundary circle); a block the
/// rendering gives no member identity for; a leader note stored as a block, whose text background lies over
/// its own leader and over a section line of another expression; a note with a background; a cutting-symbol
/// letter that names a view of another sheet; the frame; a line on a layer that never plots.
struct Synthetic {
    top: f64,
    svg: String,
    identity: bool,
    entities: Vec<Value>,
    natives: Vec<Value>,
    members: Vec<Value>,
    expressions: Vec<Value>,
    styles: Vec<Value>,
    lines: Vec<Value>,
}

type Pt = (f64, f64);
/// (source, role, line type, native member, interval, evidence)
type Part<'a> = (
    &'a str,
    &'a str,
    &'a str,
    Option<&'a str>,
    [f64; 2],
    &'a [&'a str],
);

impl Synthetic {
    fn points(&self, pts: &[Pt], join: &str, pair: &str) -> String {
        pts.iter()
            .map(|(x, y)| format!("{x:.6}{pair}{:.6}", self.top - y))
            .collect::<Vec<_>>()
            .join(join)
    }
    fn group(&mut self, root: &str, member: Option<&str>, child: String) {
        let member = match member {
            Some(m) if self.identity => format!(" data-member=\"{m}\""),
            _ => String::new(),
        };
        self.svg
            .push_str(&format!("<g data-entity=\"{root}\"{member}>{child}</g>"));
    }
    fn stroke(&mut self, root: &str, member: Option<&str>, pts: &[Pt]) {
        let child = format!(
            "<polyline fill=\"none\" stroke=\"#000000\" stroke-width=\"0.200000\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"{} \"/>",
            self.points(pts, " ", ",")
        );
        self.group(root, member, child);
    }
    fn fill(&mut self, root: &str, member: Option<&str>, pts: &[Pt]) {
        let child = format!(
            "<path fill=\"#000000\" d=\"M{}Z\"/>",
            self.points(pts, "L", " ")
        );
        self.group(root, member, child);
    }
    fn background(&mut self, root: &str, member: Option<&str>, b: [f64; 4]) {
        let child = format!(
            "<path data-mask=\"background\" fill=\"#ffffff\" d=\"M{}Z\"/>",
            self.points(&rectangle(b), "L", " ")
        );
        self.group(root, member, child);
    }
    fn entity(&mut self, id: &str, kind: &str, line: Option<[f64; 4]>, nested: &[(&str, &str)]) {
        let mut ids = vec![id.to_owned()];
        ids.extend(nested.iter().map(|(n, _)| format!("{id}/{n}")));
        self.entities.push(json!({"id":id,"handle":id,"kind":kind,"layer":"L","line_type":"Continuous",
            "geometry_kind":if line.is_some(){"line"}else{"unqualified"},"geometry_limit":Value::Null,
            "coordinates":line.map(|c|c.to_vec()).unwrap_or_default(),"member_ids":ids,"text":Value::Null}));
        self.natives.push(
            json!({"id":id,"handle":id,"kind":kind,"block_path":[id],"raw_text":Value::Null,
            "role":"native_member","source_layer":"L"}),
        );
        for (n, k) in nested {
            let mut path = vec![id];
            path.extend(n.split('/'));
            self.natives.push(json!({"id":format!("{id}/{n}"),"handle":n.rsplit('/').next().unwrap(),"kind":k,
                "block_path":path,"raw_text":Value::Null,"role":"native_member","source_layer":"L"}));
        }
        self.styles.push(
            json!({"source_handle":id,"pattern":"continuous","line_type":"continuous",
            "basis":"synthetic","layer_rule":Value::Null}),
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn expression(
        &mut self,
        kind: &str,
        family: &str,
        label: &str,
        view: Option<&str>,
        parts: &[Part],
        evidence: &[&str],
        qualifiers: &[&str],
    ) -> Vec<String> {
        let id = format!("expression:{}", self.expressions.len());
        let mut ids = Vec::new();
        for (k, (source, role, line_type, native, interval, proof)) in parts.iter().enumerate() {
            let member = format!("{id}:m{k}");
            self.members.push(json!({"id":member,"expression_id":id,"source_handle":source,"interval":interval,
                "line_type":line_type,"member_role":role,"native_member_id":native,"evidence_ids":proof,
                "relation_ids":[],"state":"candidate","unknowns":[],"view_id":view,
                "view_relation_role":"unknown","composition_view_ids":[],"view_component_kind":"unknown"}));
            ids.push(member);
        }
        self.expressions.push(json!({"id":id,"kind":kind,"family":family,"label":label,"view_id":view,
            "member_ids":ids,"evidence_ids":evidence,"qualifiers":qualifiers,"rule":format!("synthetic_{kind}"),
            "knowledge_ids":["K0"],"concept_ids":[],"state":"candidate","unknowns":[],"engineering_target":Value::Null,
            "view_mark_kind":"unknown","defines_view_ids":[],"references_view_ids":[]}));
        ids
    }
    fn structural(&mut self, member: &str, class: &str, line_type: &str, points: [f64; 4]) {
        self.lines.push(json!({"id":format!("line:{}",self.lines.len()),"line_class":class,"geometry":"line",
            "line_type":line_type,"view_id":"view:principal","points":points,"arc":Value::Null,
            "pieces":[format!("member:{member}")],"coincident":[],"trimmed":[],"evidence_ids":[],"unknowns":[],
            "state":"candidate"}));
    }
    /// A native dimension: line, two terminators, two extension lines, the text's background and two glyphs.
    #[allow(clippy::too_many_arguments)]
    fn dimension(
        &mut self,
        id: &str,
        label: &str,
        line: [Pt; 2],
        arrows: [[Pt; 3]; 2],
        extension: [[Pt; 2]; 2],
        text_box: [f64; 4],
        glyphs: [[f64; 4]; 2],
    ) {
        let n: Vec<String> = (1..=6).map(|k| format!("{}{k}", &id[..1])).collect();
        let kinds = ["LINE", "SOLID", "SOLID", "LINE", "LINE", "MTEXT"];
        let nested: Vec<(&str, &str)> = n.iter().map(String::as_str).zip(kinds).collect();
        self.entity(id, "DIMENSION", None, &nested);
        let m: Vec<String> = n.iter().map(|n| format!("{id}/{n}")).collect();
        self.stroke(id, Some(&m[0]), &line);
        self.fill(id, Some(&m[1]), &arrows[0]);
        self.fill(id, Some(&m[2]), &arrows[1]);
        self.stroke(id, Some(&m[3]), &extension[0]);
        self.stroke(id, Some(&m[4]), &extension[1]);
        self.background(id, Some(&m[5]), text_box);
        for g in glyphs {
            self.fill(id, Some(&m[5]), &rectangle(g));
        }
        let source = format!("source:{id}");
        let proof = [source.as_str()];
        self.expression(
            "dimension_linear",
            "dimension",
            label,
            Some("view:principal"),
            &[
                (id, "native_dimension", "dimension", None, WHOLE, &proof),
                (
                    id,
                    "dimension_line",
                    "thin_continuous",
                    Some(&m[0]),
                    WHOLE,
                    &[],
                ),
                (id, "arrowhead", "fill", Some(&m[1]), WHOLE, &[]),
                (id, "arrowhead", "fill", Some(&m[2]), WHOLE, &[]),
                (
                    id,
                    "extension_line",
                    "thin_continuous",
                    Some(&m[3]),
                    WHOLE,
                    &[],
                ),
                (
                    id,
                    "extension_line",
                    "thin_continuous",
                    Some(&m[4]),
                    WHOLE,
                    &[],
                ),
                (id, "dimension_text", "text", Some(&m[5]), WHOLE, &[]),
            ],
            &proof,
            &[],
        );
    }
}

const WHOLE: [f64; 2] = [0., 1.];
fn rectangle(b: [f64; 4]) -> [Pt; 4] {
    [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])]
}

/// The synthetic graph and its rendering; `identity` false gives the rendering of a converter that does not
/// name nested members.
fn synthetic(identity: bool) -> (Vec<u8>, String) {
    let mut s = Synthetic {
        top: 200.,
        svg: String::new(),
        identity,
        entities: Vec::new(),
        natives: Vec::new(),
        members: Vec::new(),
        expressions: Vec::new(),
        styles: Vec::new(),
        lines: Vec::new(),
    };
    let main = Some("view:principal");
    let (sheet, detail) = (Some("view:sheet"), Some("view:B2.5:1"));
    for (id, c) in [
        ("L1", [40., 40., 140., 40.]),
        ("L2", [140., 40., 140., 130.]),
        ("L3", [140., 100., 40., 100.]),
        ("L4", [40., 100., 40., 40.]),
        ("C1", [20., 70., 160., 70.]),
        ("H1", [60., 40., 60., 100.]),
    ] {
        s.entity(id, "LINE", Some(c), &[]);
        s.stroke(id, None, &[(c[0], c[1]), (c[2], c[3])]);
    }
    let third = 2. / 3.;
    let outline = s.expression(
        "visible_contour",
        "geometry",
        "主视图 · 可见轮廓",
        main,
        &[
            (
                "L1",
                "contour_segment",
                "thick_continuous",
                None,
                WHOLE,
                &[],
            ),
            (
                "L2",
                "contour_segment",
                "thick_continuous",
                None,
                [0., third],
                &[],
            ),
            (
                "L3",
                "contour_segment",
                "thick_continuous",
                None,
                WHOLE,
                &[],
            ),
            (
                "L4",
                "contour_segment",
                "thick_continuous",
                None,
                WHOLE,
                &[],
            ),
        ],
        &[],
        &[],
    );
    for (member, points) in outline.iter().zip([
        [40., 40., 140., 40.],
        [140., 40., 140., 100.],
        [140., 100., 40., 100.],
        [40., 100., 40., 40.],
    ]) {
        s.structural(member, "visible_outline", "thick_continuous", points);
    }
    let hidden = s.expression(
        "hidden_contour",
        "geometry",
        "主视图 · 不可见轮廓",
        main,
        &[("H1", "hidden_segment", "thin_dashed", None, WHOLE, &[])],
        &[],
        &[],
    );
    s.structural(
        &hidden[0],
        "hidden_outline",
        "thin_dashed",
        [60., 40., 60., 100.],
    );
    s.expression(
        "center_axis",
        "geometry",
        "轴线",
        main,
        &[("C1", "center_line", "thin_dash_dot", None, WHOLE, &[])],
        &[],
        &[],
    );
    s.expression(
        "leader",
        "dimension",
        "引出线",
        main,
        &[(
            "L2",
            "leader_line",
            "thin_continuous",
            None,
            [third, 1.],
            &[],
        )],
        &[],
        &[],
    );
    s.dimension(
        "D1",
        "100",
        [(40., 25.), (140., 25.)],
        [
            [(40., 25.), (49., 23.5), (49., 26.5)],
            [(140., 25.), (131., 23.5), (131., 26.5)],
        ],
        [[(40., 38.), (40., 22.)], [(140., 38.), (140., 22.)]],
        [78., 25., 102., 37.],
        [[84., 28., 89., 34.], [91., 28., 96., 34.]],
    );
    // Its own line runs through its number.
    s.dimension(
        "E1",
        "60",
        [(25., 40.), (25., 100.)],
        [
            [(25., 40.), (23.5, 49.), (26.5, 49.)],
            [(25., 100.), (23.5, 91.), (26.5, 91.)],
        ],
        [[(38., 40.), (22., 40.)], [(38., 100.), (22., 100.)]],
        [19., 64., 31., 76.],
        [[22.5, 66., 27.5, 69.], [22.5, 71., 27.5, 74.]],
    );
    s.entity(
        "B1",
        "INSERT",
        None,
        &[
            ("T1", "MTEXT"),
            ("T2", "MTEXT"),
            ("S1", "INSERT"),
            ("S1/P1", "LINE"),
            ("S1/P2", "LINE"),
            ("S1/T3", "MTEXT"),
        ],
    );
    for b in [
        [190., 170., 194., 176.],
        [196., 170., 200., 176.],
        [202., 170., 206., 176.],
    ] {
        s.fill("B1", Some("B1/T1"), &rectangle(b));
    }
    for b in [[190., 158., 194., 164.], [196., 158., 200., 164.]] {
        s.fill("B1", Some("B1/T2"), &rectangle(b));
    }
    s.stroke("B1", Some("B1/S1/P1"), &[(188., 150.), (192., 142.)]);
    s.stroke("B1", Some("B1/S1/P2"), &[(192., 142.), (200., 156.)]);
    s.background("B1", Some("B1/S1/T3"), [194., 140., 216., 150.]);
    for b in [[204., 142., 208., 148.], [210., 142., 214., 148.]] {
        s.fill("B1", Some("B1/S1/T3"), &rectangle(b));
    }
    s.expression(
        "technical_general",
        "note",
        "技术要求",
        sheet,
        &[("B1", "note_text", "text", Some("B1/T1"), WHOLE, &[])],
        &[],
        &[],
    );
    s.expression(
        "process_requirement",
        "note",
        "1.去除毛刺",
        sheet,
        &[("B1", "note_text", "text", Some("B1/T2"), WHOLE, &[])],
        &[],
        &[],
    );
    s.expression(
        "surface_roughness",
        "surface",
        "其余 12.5",
        sheet,
        &[
            (
                "B1",
                "roughness_symbol",
                "thin_continuous",
                Some("B1/S1/P1"),
                WHOLE,
                &[],
            ),
            (
                "B1",
                "roughness_symbol",
                "thin_continuous",
                Some("B1/S1/P2"),
                WHOLE,
                &[],
            ),
            (
                "B1",
                "roughness_value",
                "text",
                Some("B1/S1/T3"),
                WHOLE,
                &[],
            ),
        ],
        &[],
        &[],
    );
    // A block the rendering gives no member identity for.
    s.entity(
        "B2",
        "INSERT",
        None,
        &[("X1", "LINE"), ("X2", "LINE"), ("X3", "MTEXT")],
    );
    s.stroke("B2", None, &[(60., 150.), (130., 150.)]);
    s.stroke("B2", None, &[(70., 150.), (85., 165.)]);
    s.background("B2", None, [90., 146., 110., 160.]);
    s.fill("B2", None, &rectangle([97., 152., 103., 158.]));
    s.expression(
        "visible_contour",
        "geometry",
        "块 · 可见轮廓",
        main,
        &[(
            "B2",
            "native_member",
            "thick_continuous",
            Some("B2/X1"),
            WHOLE,
            &[],
        )],
        &[],
        &[],
    );
    s.expression(
        "section_hatch",
        "geometry",
        "块 · 剖面线 45° 间距 3",
        main,
        &[(
            "B2",
            "hatch_fill",
            "thin_continuous",
            Some("B2/X2"),
            WHOLE,
            &[],
        )],
        &[],
        &[],
    );
    s.expression(
        "leader_note",
        "note",
        "块内说明",
        main,
        &[("B2", "note_text", "text", Some("B2/X3"), WHOLE, &[])],
        &[],
        &[],
    );
    // A detail view stored as a block.
    s.entity(
        "B3",
        "INSERT",
        None,
        &[
            ("V1", "INSERT"),
            ("V1/K1", "LINE"),
            ("V1/K2", "LINE"),
            ("V1/K3", "LINE"),
            ("V1/K4", "LINE"),
            ("V1/K5", "LINE"),
            ("R1", "CIRCLE"),
        ],
    );
    s.stroke("B3", Some("B3/V1/K1"), &[(190., 30.), (250., 30.)]);
    s.stroke("B3", Some("B3/V1/K2"), &[(250., 30.), (250., 80.)]);
    for (k, x) in [(3, 200.), (4, 215.), (5, 230.)] {
        s.stroke(
            "B3",
            Some(&format!("B3/V1/K{k}")),
            &[(x, 30.), (x + 15., 45.)],
        );
    }
    s.stroke(
        "B3",
        Some("B3/R1"),
        &[
            (270., 55.),
            (257., 87.),
            (225., 100.),
            (193., 87.),
            (180., 55.),
            (193., 23.),
            (225., 10.),
            (257., 23.),
            (270., 55.),
        ],
    );
    s.expression(
        "visible_contour",
        "geometry",
        "B 2.5:1 · 可见轮廓",
        detail,
        &[
            (
                "B3",
                "native_member",
                "thick_continuous",
                Some("B3/V1/K1"),
                WHOLE,
                &[],
            ),
            (
                "B3",
                "native_member",
                "thick_continuous",
                Some("B3/V1/K2"),
                WHOLE,
                &[],
            ),
        ],
        &[],
        &[],
    );
    s.expression(
        "section_hatch",
        "geometry",
        "B 2.5:1 · 剖面线 45° 间距 3",
        detail,
        &[
            (
                "B3",
                "hatch_fill",
                "thin_continuous",
                Some("B3/V1/K3"),
                WHOLE,
                &[],
            ),
            (
                "B3",
                "hatch_fill",
                "thin_continuous",
                Some("B3/V1/K4"),
                WHOLE,
                &[],
            ),
            (
                "B3",
                "hatch_fill",
                "thin_continuous",
                Some("B3/V1/K5"),
                WHOLE,
                &[],
            ),
        ],
        &[],
        &[],
    );
    s.expression(
        "view_callout",
        "view",
        "B",
        detail,
        &[(
            "B3",
            "detail_circle",
            "thin_continuous",
            Some("B3/R1"),
            WHOLE,
            &[],
        )],
        &["references_view:view:B2.5:1:by_elimination"],
        &["detail_indicator"],
    );
    // A leader note stored as a block: a section line of another expression runs through the margin of the
    // text's background, the note's own leader starts inside it; neither touches the glyphs.
    s.entity(
        "B4",
        "INSERT",
        None,
        &[("Y1", "LINE"), ("Y2", "LINE"), ("Y3", "MTEXT")],
    );
    s.stroke("B4", Some("B4/Y1"), &[(160., 111.), (186., 111.)]);
    s.stroke("B4", Some("B4/Y2"), &[(182., 116.), (195., 130.)]);
    s.background("B4", Some("B4/Y3"), [166., 110., 184., 122.]);
    for b in [[170., 113., 174., 119.], [176., 113., 180., 119.]] {
        s.fill("B4", Some("B4/Y3"), &rectangle(b));
    }
    s.expression(
        "section_hatch",
        "geometry",
        "C · 剖面线 45° 间距 2",
        main,
        &[(
            "B4",
            "hatch_fill",
            "thin_continuous",
            Some("B4/Y1"),
            WHOLE,
            &[],
        )],
        &[],
        &[],
    );
    s.expression(
        "leader_note",
        "note",
        "引出说明",
        main,
        &[
            (
                "B4",
                "leader_line",
                "thin_continuous",
                Some("B4/Y2"),
                WHOLE,
                &[],
            ),
            ("B4", "note_text", "text", Some("B4/Y3"), WHOLE, &[]),
        ],
        &[],
        &[],
    );
    s.entity("T9", "MTEXT", None, &[]);
    s.background("T9", None, [20., 150., 50., 170.]);
    s.fill("T9", None, &rectangle([24., 156., 30., 164.]));
    s.fill("T9", None, &rectangle([34., 156., 40., 164.]));
    s.expression(
        "technical_general",
        "note",
        "未注圆角 R3",
        sheet,
        &[("T9", "note_text", "text", None, WHOLE, &[])],
        &[],
        &[],
    );
    s.entity("V1", "TEXT", None, &[]);
    s.fill("V1", None, &rectangle([150., 118., 154., 124.]));
    s.expression(
        "view_callout",
        "view",
        "A",
        main,
        &[("V1", "view_letter", "text", None, WHOLE, &[])],
        &["references_view:view:sheet-2:principal"],
        &["cutting_plane"],
    );
    s.entity("F1", "LWPOLYLINE", None, &[]);
    s.stroke(
        "F1",
        None,
        &[(5., 5.), (295., 5.), (295., 195.), (5., 195.), (5., 5.)],
    );
    s.expression(
        "sheet_frame",
        "sheet",
        "图框",
        sheet,
        &[("F1", "frame_border", "thick_continuous", None, WHOLE, &[])],
        &[],
        &[],
    );
    s.entity("N1", "LINE", Some([10., 190., 60., 190.]), &[]);
    s.stroke("N1", None, &[(10., 190.), (60., 190.)]);
    s.expression(
        "construction_reference",
        "construction",
        "不打印层上的线",
        None,
        &[(
            "N1",
            "native_member",
            "continuous",
            None,
            WHOLE,
            &["not_plotted"],
        )],
        &[],
        &["not_plotted"],
    );
    let view = |id: &str, kind: &str, label: &str, bbox: [f64; 4]| {
        json!({"id":id,"kind":kind,"label":label,"bbox":bbox,"evidence_ids":[],"label_source_ids":[],
            "scale_text":Value::Null,"state":"candidate","unknowns":[]})
    };
    let graph = json!({"schema":"oev2.drawing-expression-graph/1","profile":"synthetic","source_sha256":"0".repeat(64),
        "layer_convention":"not_checked","layer_convention_evidence":[],
        "views":[view("view:principal","principal","主视图",[15.,15.,165.,135.]),
            view("view:sheet-2:principal","section_view","图纸 2 · A-A",[0.,0.,1.,1.]),
            view("view:B2.5:1","detail_view","B 2.5:1",[175.,5.,275.,105.]),
            view("view:sheet","sheet","图纸",[0.,0.,300.,200.])],
        "line_styles":s.styles,"members":s.members,"expressions":s.expressions,"unknowns":[],
        "structure_ends":[],"implied_spans":[],"structural_lines":s.lines,
        "source":{"schema":"synthetic","entities":s.entities,"members":s.natives,"relations":[]}});
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 300.000000 200.000000\" data-drawing-origin-x=\"0.000000\" data-drawing-top-y=\"200.000000\" width=\"1600\" height=\"1067\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{}</svg>",
        s.svg
    );
    (serde_json::to_vec(&graph).unwrap(), svg)
}

/// Invented page-only drawing, never classified: two views, a cutting symbol hosted by the first but
/// composing the second, a located dimension, and one source with component/located/unknown intervals.
fn composition_synthetic() -> (Vec<u8>, String) {
    let mut s = Synthetic {
        top: 180.,
        svg: String::new(),
        identity: true,
        entities: Vec::new(),
        natives: Vec::new(),
        members: Vec::new(),
        expressions: Vec::new(),
        styles: Vec::new(),
        lines: Vec::new(),
    };
    let first = "view:principal";
    let second = "view:offset";
    let assign = |s: &mut Synthetic, ids: Vec<String>, role: &str, targets: &[&str], kind: &str| {
        for id in ids {
            let m = s.members.iter_mut().find(|m| m["id"] == id).unwrap();
            m["view_relation_role"] = json!(role);
            m["composition_view_ids"] = json!(targets);
            m["view_component_kind"] = json!(kind);
        }
    };
    for (id, points, view) in [
        ("XA", [36., 48., 96., 48.], first),
        ("XB", [170., 62., 244., 62.], second),
    ] {
        s.entity(id, "LINE", Some(points), &[]);
        s.stroke(id, None, &[(points[0], points[1]), (points[2], points[3])]);
        let ids = s.expression(
            "visible_contour",
            "geometry",
            "轮廓",
            Some(view),
            &[(id, "contour_segment", "thick_continuous", None, WHOLE, &[])],
            &[],
            &[],
        );
        s.structural(&ids[0], "visible_outline", "thick_continuous", points);
        assign(&mut s, ids, "component", &[view], "view_geometry");
    }
    s.entity("XC", "LINE", Some([56., 78., 56., 100.]), &[]);
    s.stroke("XC", None, &[(56., 78.), (56., 100.)]);
    s.entity("XR", "SOLID", None, &[]);
    s.fill("XR", None, &[(56., 78.), (54., 88.), (58., 88.)]);
    s.entity("XT", "TEXT", None, &[]);
    s.fill("XT", None, &rectangle([52., 106., 59., 114.]));
    let ids = s.expression(
        "view_callout",
        "view",
        "Q 剖切符号",
        Some(first),
        &[
            ("XC", "cutting_stroke", "thick_continuous", None, WHOLE, &[]),
            ("XR", "view_arrow", "fill", None, WHOLE, &[]),
            ("XT", "view_letter", "text", None, WHOLE, &[]),
        ],
        &["references_view:view:offset"],
        &["cutting_plane"],
    );
    for (id, kind) in
        ids.into_iter()
            .zip(["cutting_location", "projection_direction", "mark_letters"])
    {
        assign(&mut s, vec![id], "component", &[second], kind);
    }
    s.dimension(
        "XD",
        "48",
        [(36., 32.), (96., 32.)],
        [
            [(36., 32.), (45., 30.5), (45., 33.5)],
            [(96., 32.), (87., 30.5), (87., 33.5)],
        ],
        [[(36., 46.), (36., 29.)], [(96., 46.), (96., 29.)]],
        [58., 32., 77., 44.],
        [[60., 35., 65., 41.], [69., 35., 74., 41.]],
    );
    let ids = s
        .members
        .iter()
        .filter(|m| m["source_handle"] == "XD")
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    assign(&mut s, ids, "located_only", &[], "auxiliary_annotation");
    s.entity("XS", "LINE", Some([32., 144., 136., 144.]), &[]);
    s.stroke("XS", None, &[(32., 144.), (136., 144.)]);
    for (interval, role, targets, kind) in [
        (
            [0.18, 0.64],
            "component",
            vec![first, second],
            "view_geometry",
        ),
        ([0.64, 1.], "located_only", vec![], "auxiliary_annotation"),
        ([0., 0.18], "unknown", vec![], "unknown"),
    ] {
        let ids = s.expression(
            "center_axis",
            "geometry",
            "有界共享来源",
            Some(first),
            &[("XS", "center_line", "thin_dash_dot", None, interval, &[])],
            &[],
            &[],
        );
        assign(&mut s, ids, role, &targets, kind);
    }
    // Two native members of a single INSERT have different view relations.
    s.entity("XN", "INSERT", None, &[("K", "LINE"), ("J", "TEXT")]);
    s.stroke("XN", Some("XN/K"), &[(186., 96.), (242., 96.)]);
    s.fill("XN", Some("XN/J"), &rectangle([204., 102., 214., 110.]));
    for (native, role, targets) in [
        ("XN/K", "component", vec![second]),
        ("XN/J", "located_only", vec![]),
    ] {
        let ids = s.expression(
            "view_callout",
            "view",
            "嵌套成员",
            Some(first),
            &[(
                "XN",
                "native_member",
                "thin_continuous",
                Some(native),
                WHOLE,
                &[],
            )],
            &[],
            &[],
        );
        assign(
            &mut s,
            ids,
            role,
            &targets,
            if role == "component" {
                "view_title"
            } else {
                "auxiliary_annotation"
            },
        );
    }
    let cutting = s
        .expressions
        .iter_mut()
        .find(|e| e["label"] == "Q 剖切符号")
        .unwrap();
    cutting["view_mark_kind"] = json!("cutting_symbol");
    cutting["references_view_ids"] = json!([second]);
    let view = |id: &str, label: &str, bbox: [f64; 4]| {
        json!({
            "id":id, "kind":"principal", "label":label, "bbox":bbox, "position_role":"unknown",
            "representation_kind":"unknown", "section_extent":"unknown", "cutting_surface_kind":"unknown",
            "label_source_ids":[], "scale_text":null, "state":"candidate", "evidence_ids":[], "unknowns":[],
        })
    };
    let graph = json!({"schema":"oev2.drawing-expression-graph/1", "profile":"synthetic-composition",
        "source_sha256":"3".repeat(64), "layer_convention":"not_checked", "layer_convention_evidence":[],
        "views":[view(first,"甲视图",[20.,20.,145.,160.]),view(second,"乙视图",[158.,40.,260.,125.])],
        "line_styles":s.styles, "members":s.members, "expressions":s.expressions, "unknowns":[],
        "structure_ends":[], "implied_spans":[], "structural_lines":s.lines,
        "source":{"schema":"synthetic", "entities":s.entities, "members":s.natives, "relations":[]}});
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 280 180\" data-drawing-origin-x=\"0\" data-drawing-top-y=\"180\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{}</svg>",
        s.svg
    );
    (serde_json::to_vec(&graph).unwrap(), svg)
}

const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

#[test]
fn section_browser_checks_detect_a_boundary_drawn_beyond_its_evidence() {
    if !Path::new(CHROME).is_file() {
        eprintln!("skipped: no Chrome at {CHROME}");
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let scratch = Scratch::new(&source);
    let root = oe_core::SkillRoot::new(&source).unwrap().with_project_data_root(&scratch.0).unwrap();
    let (bytes, svg) = synthetic(true);
    let mut g: Value = serde_json::from_slice(&bytes).unwrap();
    let hatch = g["expressions"].as_array_mut().unwrap().iter_mut().find(|e| e["kind"] == "section_hatch").unwrap();
    hatch["evidence_ids"].as_array_mut().unwrap().push(json!("cross_section_type:unknown"));
    let region = hatch["id"].clone();
    let src = g["source"]["entities"].as_array().unwrap().iter().find(|e| e["geometry_kind"] == "line").unwrap();
    let handle = src["id"].clone();
    let p: Vec<f64> = src["coordinates"].as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect();
    let point = |t: f64| [p[0] + t * (p[2] - p[0]), p[1] + t * (p[3] - p[1])];
    let span = json!({"region":region,"interval":[0.2,0.6],"parameter":"source_line","points":[point(0.2),point(0.6)],"carrier":"source_line"});
    let m = g["members"].as_array_mut().unwrap().iter_mut().find(|m| m["source_handle"] == handle).unwrap();
    m["evidence_ids"].as_array_mut().unwrap().push(json!(format!("section_boundary:{span}")));
    let (page, _) = oe_drawing::page::page(&serde_json::to_vec(&g).unwrap(), &svg, &request()).unwrap();
    let broken = page.replace("p[1][0]-originX);n.setAttribute('y2'", "p[1][0]-originX+7);n.setAttribute('y2'");
    assert_ne!(page, broken);
    for (n, text) in [page, broken].into_iter().enumerate() {
        let file = format!("section-page-{n}.html");
        std::fs::write(scratch.0.join(&file), text).unwrap();
        let q = oe_contracts::MdReviewPageCheckRequest {
            schema: oe_drawing_check::browser::REQUEST_SCHEMA.into(), page: file,
            output_directory: format!("section-check-{n}"), chrome: CHROME.into(),
            mode: "kind".into(), region: None, focus_label: None,
        };
        let result = oe_drawing_check::browser::check_page(&root, &q);
        if n == 0 {
            assert_eq!(result.unwrap().facts["section_regions"]["boundary_spans"], 1);
        } else {
            assert!(result.unwrap_err().to_string().contains("Boundary endpoints match evidence"));
        }
    }
}

/// A project data directory under the skill's private `var`, removed when the test ends.
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(source: &Path) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Two browser tests may start within one clock tick. Time alone is not a
        // unique owner: one Scratch must never remove the other test's files.
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = source
            .join("var")
            .join(format!("review-page-test-{}-{nanos}-{serial}", std::process::id()));
        std::fs::create_dir_all(source.join("var")).unwrap();
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The route's own browser checks, run in Chrome on the synthetic page: they pass in all seven modes with
/// the facts the drawing was built to give, and each of them fails when the page breaks its rule. Skipped
/// where Chrome is not installed (the route then does not check pages either).
#[test]
fn the_browser_checks_pass_on_a_synthetic_page_and_catch_each_broken_rule() {
    if !Path::new(CHROME).is_file() {
        eprintln!("skipped: no Chrome at {CHROME}");
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = Scratch::new(&source);
    let root = oe_core::SkillRoot::new(&source)
        .unwrap()
        .with_project_data_root(&scratch.0)
        .unwrap();
    let mut serial = 0;
    let mut check = |text: &str, mode: &str| {
        serial += 1;
        let page = format!("page-{serial}.html");
        std::fs::write(scratch.0.join(&page), text).unwrap();
        let q = oe_contracts::MdReviewPageCheckRequest {
            schema: oe_drawing_check::browser::REQUEST_SCHEMA.into(),
            page,
            output_directory: format!("browser-{serial}"),
            chrome: CHROME.into(),
            mode: mode.into(),
            region: None,
            focus_label: None,
        };
        oe_drawing_check::browser::check_page(&root, &q)
    };
    let (graph, svg) = synthetic(true);
    let (text, _) = oe_drawing::page::page(&graph, &svg, &request()).unwrap();
    for mode in oe_drawing_check::browser::MODES {
        let facts = check(&text, mode)
            .unwrap_or_else(|e| panic!("{mode}: {e}"))
            .facts;
        // d from the four terminators, 9 long: 9 / 6.
        assert_eq!(facts["widths"]["d"], 1.5, "{mode}");
        assert_eq!(facts["widths"]["basis"], "terminator_length/6", "{mode}");
        assert_eq!(facts["widths"]["terminators"], 4, "{mode}");
        // Text backgrounds of the two dimensions, the nested texture value, the leader note's text and the
        // note are their annotation's own; the block without member identity keeps its background as
        // converted.
        let masks = &facts["masks_own"];
        assert_eq!(masks["roots"], 6, "{mode}");
        assert_eq!(masks["own_masks"], 5, "{mode}");
        // In front: 5 pieces of the first dimension, 4 of the second, the two symbol strokes, the note's
        // leader. Behind: the second dimension's line, which runs through its own number. The section line
        // through the leader note's background is not the note's own and is not counted: it keeps its
        // converted place behind the background.
        assert_eq!(masks["in_front"], 12, "{mode}");
        assert_eq!(masks["kept_behind"], 1, "{mode}");
        let shown = &facts["member_display"];
        assert_eq!(shown["roots_with_member_identity"], 5, "{mode}");
        assert_eq!(shown["groups_with_member"], 37, "{mode}");
        // One kind at a time: of the requirement block only the texture symbol and its value; of the
        // detail view only its section lines, beside the whole block that has no member identity.
        let kinds = &facts["by_kind"];
        assert_eq!(
            kinds["surface_roughness"],
            json!({"groups_shown":5,"groups_with_member_shown":5}),
            "{mode}"
        );
        assert_eq!(
            kinds["process_requirement"],
            json!({"groups_shown":2,"groups_with_member_shown":2}),
            "{mode}"
        );
        assert_eq!(
            kinds["section_hatch"],
            json!({"groups_shown":8,"groups_with_member_shown":4}),
            "{mode}"
        );
        // Only the outline in the complete structure: four structural lines carry the four outline
        // sources, and the page does not say that nothing is visible.
        let outline = &facts["complete_by_kind"]["visible_contour"];
        assert_eq!(outline["structural_lines"], 4, "{mode}");
        assert_eq!(outline["sources_drawn_by_lines_only"], 4, "{mode}");
        assert_eq!(outline["empty_notice"], false, "{mode}");
        assert_eq!(
            facts["complete_by_kind"]["hidden_contour"],
            json!({"groups_shown":0,"structural_lines":1,"sources_drawn":0,"sources_drawn_by_lines_only":1,"empty_notice":false}),
            "{mode}"
        );
        assert_eq!(facts["not_plotted"]["sources"], 1, "{mode}");
        assert_eq!(facts["references"]["views_named"], 2, "{mode}");
        assert_eq!(facts["original_view"]["order_as_converted"], true, "{mode}");
    }
    // The same drawing rendered without member identity: every block is one piece, as before; the
    // backgrounds of the two dimensions and of the note are still their own.
    let (graph, plain) = synthetic(false);
    let (plain, _) = oe_drawing::page::page(&graph, &plain, &request()).unwrap();
    let facts = check(&plain, "kind").unwrap().facts;
    assert_eq!(facts["member_display"]["roots_with_member_identity"], 0);
    assert_eq!(
        facts["by_kind"]["surface_roughness"],
        json!({"groups_shown":10,"groups_with_member_shown":0})
    );
    assert_eq!(facts["masks_own"]["own_masks"], 3);
    assert_eq!(facts["masks_own"]["in_front"], 9);
    assert_eq!(facts["masks_own"]["kept_behind"], 1);
    // Each rule broken in the page text is caught by the checks.
    for (from, to, caught) in [
        // every member claims every group of its root
        (
            "claimsByMember.set(key,ms.filter(m=>!m.native_member_id||D===m.native_member_id||D.startsWith(m.native_member_id+'/')))",
            "claimsByMember.set(key,ms)",
            "against the members that claim it",
        ),
        // text backgrounds stay where the converter painted them
        (
            "svg.classList.toggle('as-converted',raw);placeMasks(raw);",
            "svg.classList.toggle('as-converted',raw);",
            "A text background hides ink of its own annotation that does not cross the text",
        ),
        // the whole block counts as the text's annotation: the foreign section line comes in front
        (
            "scope:one?()=>true:g=>claims.get(g).some(m=>ex.has(m.expression_id))",
            "scope:()=>true",
            "left its converted position",
        ),
        // own ink comes in front although it crosses its own number
        (
            "if(after.every(m=>m.own&&m.scope(g)&&!crosses(g,m.glyphs)))",
            "if(after.every(m=>m.own&&m.scope(g)))",
            "Ink that a text background must keep hidden is drawn over it",
        ),
        // one width for thick and thin
        (
            ".canvas svg .w-thin{stroke-width:calc(var(--d)*var(--zoom)*.5px)}",
            ".canvas svg .w-thin{stroke-width:calc(var(--d)*var(--zoom)*1px)}",
            " wide",
        ),
        // a nested group does not take its member's width
        (
            "(THICK(topMember(cm,named).line_type)?1:.5):lw)",
            "1:lw)",
            "its member rule gives",
        ),
        // d from another divisor than the standard's
        (
            "ARROW_PER_D=6,",
            "ARROW_PER_D=8,",
            "d is one sixth of the median dimension terminator",
        ),
        // the empty notice counts sources only
        (
            "$('empty').hidden=drawn>0;",
            "$('empty').hidden=roots>0;",
            "the empty notice is shown with",
        ),
        // the status line counts sources drawn by themselves only
        (
            "显示 ${roots+byLine} / ",
            "显示 ${roots} / ",
            "the status line does not count",
        ),
        // the original view keeps the page's widths
        (
            "svg.classList.toggle('as-converted',raw);placeMasks(raw);",
            "placeMasks(raw);",
            "Original view changes a converter stroke width",
        ),
        // the original view keeps the page's drawing order
        (
            "function placeMasks(raw){if(pageOrder===!raw)return;",
            "function placeMasks(raw){if(pageOrder)return;",
            "Original view keeps the converted drawing order",
        ),
        // content that never plots is drawn like the rest
        (
            ".not-plotted{opacity:calc(var(--op,1)*.4)}",
            ".not-plotted{opacity:calc(var(--op,1)*1)}",
            "the opacity of N1",
        ),
        // a referenced view is cut at the second colon of its id
        (
            "id=x.slice(16,guess?-15:undefined)",
            "id=x.split(':').slice(1,3).join(':')",
            "do not name the referenced view",
        ),
    ] {
        assert_eq!(text.matches(from).count(), 1, "{from}");
        let error = check(&text.replacen(from, to, 1), "kind")
            .expect_err(from)
            .to_string();
        assert!(error.contains(caught), "{from}: {error}");
    }
}

#[test]
fn the_browser_checks_use_view_composition_and_located_overlay_on_a_synthetic_page() {
    if !Path::new(CHROME).is_file() {
        eprintln!("skipped: no Chrome at {CHROME}");
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = Scratch::new(&source);
    // Optional durable evidence for the lane report. Ordinary test runs still own and remove their scratch.
    let evidence = std::env::var_os("OE_DRAWING_PAGE_EVIDENCE_DIR").map(std::path::PathBuf::from);
    if let Some(path) = &evidence {
        assert!(path.is_absolute());
        std::fs::create_dir(path).unwrap();
    }
    let data_root = evidence.as_deref().unwrap_or(&scratch.0);
    let root = oe_core::SkillRoot::new(&source)
        .unwrap()
        .with_project_data_root(data_root)
        .unwrap();
    let (graph, svg) = composition_synthetic();
    let mut page_request = request();
    page_request.title = "合成图：两个视图的组成与所在标注".into();
    page_request.version = "甲段合成样例".into();
    page_request.source_note = "手工关系图与中性矢量；不经过读图识别".into();
    page_request.status_note = "页面逻辑合成测试，非工程结论".into();
    let (page, _) = oe_drawing::page::page(&graph, &svg, &page_request).unwrap();
    std::fs::write(data_root.join("composition-graph.json"), &graph).unwrap();
    std::fs::write(data_root.join("composition-source.svg"), &svg).unwrap();
    std::fs::write(data_root.join("composition-page.html"), &page).unwrap();
    let result = oe_drawing_check::browser::check_page(
        &root,
        &oe_contracts::MdReviewPageCheckRequest {
            schema: oe_drawing_check::browser::REQUEST_SCHEMA.into(),
            page: "composition-page.html".into(),
            output_directory: "composition-check".into(),
            chrome: CHROME.into(),
            mode: "view".into(),
            region: None,
            focus_label: None,
        },
    )
    .unwrap();
    assert_eq!(result.facts["view_composition"]["views"], 2);
    assert!(
        result.facts["view_composition"]["component_sources"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        result.facts["view_composition"]["located_sources"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        result.facts["view_composition"]["by_view"]["view:offset"]["composition"]["source_ids"],
        json!(["XB", "XC", "XN", "XR", "XS", "XT"])
    );
    assert_eq!(
        result.facts["view_composition"]["by_view"]["view:principal"]["composition"]["source_ids"],
        json!(["XA", "XS"])
    );
    assert_eq!(
        result.facts["view_composition"]["by_view"]["view:principal"]["overlay"]["source_ids"],
        json!(["XA", "XD", "XN", "XS"])
    );
    // A check must reject the old location-based selection, annotation leakage while off, wrong target
    // colour, an unbounded source interval and a faint overlay painted at full opacity.
    for (n, (from, to, caught)) in [
        (
            "composition.passes(m,enabled.view,$('viewOverlay').checked)",
            "enabled.view.has(locatedView(m))",
            "source coverage",
        ),
        (
            "composition.passes(m,enabled.view,$('viewOverlay').checked)",
            "composition.passes(m,enabled.view,true)",
            "source coverage",
        ),
        (
            "const targetForMember=m=>composition.colorTarget(m,enabled.view);",
            "const targetForMember=m=>locatedView(m);",
            "target view colour",
        ),
        (
            "out.push({interval:[a,b],members:claiming})",
            "out.push({interval:[0,1],members:claiming})",
            "bounded member span",
        ),
        (
            ".located-overlay{opacity:calc(var(--op,1)*.38)}",
            ".located-overlay{opacity:calc(var(--op,1)*1)}",
            "faint display",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(page.matches(from).count(), 1, "{from}");
        let file = format!("composition-broken-{n}.html");
        std::fs::write(data_root.join(&file), page.replacen(from, to, 1)).unwrap();
        let q = oe_contracts::MdReviewPageCheckRequest {
            schema: oe_drawing_check::browser::REQUEST_SCHEMA.into(),
            page: file,
            output_directory: format!("composition-broken-{n}"),
            chrome: CHROME.into(),
            mode: "view".into(),
            region: None,
            focus_label: None,
        };
        let error = oe_drawing_check::browser::check_page(&root, &q)
            .unwrap_err()
            .to_string();
        assert!(error.contains(caught), "{from}: {error}");
    }
}

#[test]
fn the_browser_check_requires_the_first_drafting_group_to_contain_certain() {
    if !Path::new(CHROME).is_file() {
        eprintln!("skipped: no Chrome at {CHROME}");
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = Scratch::new(&source);
    let root = oe_core::SkillRoot::new(&source)
        .unwrap()
        .with_project_data_root(&scratch.0)
        .unwrap();
    let (graph, svg) = synthetic(true);
    let graph_value: Value = serde_json::from_slice(&graph).unwrap();
    let source_handle = graph_value["source"]["entities"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let issue = |id: &str, rule: &str, label: &str, confidence: &str| {
        json!({
            "id": id,
            "location": source_handle.clone(),
            "source_occurrences": [source_handle.clone()],
            "observed": [label],
            "rule": rule,
            "clause": format!("clause:{rule}"),
            "concept_ids": [],
            "confidence": confidence,
            "alternatives": [],
            "proposed_correction": "请复核",
            "rule_label": label,
            "classification": "source_deviation_candidate",
            "layers": ["source"],
            "details": {"findings": [{"source_handles": [source_handle.clone()]}]}
        })
    };
    // This is the order produced by the route sorter: a later-numbered certain rule precedes an
    // earlier-numbered likely rule, while the browser check verifies the visible first group.
    let issues = vec![
        issue("certain-id", "G4-043-certain", "确定规则", "certain"),
        issue("likely-id", "G1-028-likely", "很可能规则", "likely"),
    ];
    let text = issues
        .iter()
        .map(|issue| format!("{} {}", issue["id"], issue["clause"]))
        .collect::<Vec<_>>()
        .join("\n");
    let review = json!({
        "issues": issues,
        "text": text,
        "receipt": {"status": "not_needed", "corrected_sha256": null, "applied": [], "remaining_issue_ids": ["certain-id", "likely-id"]}
    });
    let (page, _) = oe_drawing::page::page_with_review(
        &graph,
        &svg,
        &request(),
        Some(&review),
    )
    .unwrap();
    std::fs::write(scratch.0.join("drafting-order-page.html"), page).unwrap();
    let result = oe_drawing_check::browser::check_page(
        &root,
        &oe_contracts::MdReviewPageCheckRequest {
            schema: oe_drawing_check::browser::REQUEST_SCHEMA.into(),
            page: "drafting-order-page.html".into(),
            output_directory: "drafting-order-check".into(),
            chrome: CHROME.into(),
            mode: "kind".into(),
            region: None,
            focus_label: None,
        },
    )
    .unwrap();
    assert_eq!(result.facts["drafting_group_order"]["groups"], 2);
    assert_eq!(
        result.facts["drafting_group_order"]["first_contains_certain"],
        true
    );
}
