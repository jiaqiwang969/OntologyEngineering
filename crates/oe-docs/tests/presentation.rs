use oe_contracts::{PresentationRequest1, PresentationScene1};
use oe_core::SkillRoot;
use oe_docs::presentation::{generate, verify};
use oe_engineering::presentation::bind;
use oe_geometry::presentation::{evaluate, project};
use oe_kernel::project::Project;
use serde_json::{Value, json};
use std::{fs, path::Path};
fn example() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/presentation/linear-stage.json"
    ))
    .unwrap()
}
fn scene() -> PresentationScene1 {
    serde_json::from_value(example()["scene"].clone()).unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: SkillRoot,
    project: Project,
    request: PresentationRequest1,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("SKILL.md"), "test").unwrap();
        fs::create_dir(temp.path().join("var")).unwrap();
        let root = SkillRoot::new(fs::canonicalize(temp.path()).unwrap()).unwrap();
        fs::write(
            root.path().join("var/source.json"),
            serde_json::to_vec_pretty(&example()).unwrap(),
        )
        .unwrap();
        let source = oe_core::sha256(fs::read(root.path().join("var/source.json")).unwrap());
        let project:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"presentation-test","revision":"1","purpose":"Finite illustrative stage","objects":{"stage":"1"},"conditions":{"illustrative":"No physical/native acceptance"},"evidence":{"scene":source},"decisions":[],"dependencies":{"stage":["scene"],"figure":["stage"]}})).unwrap();
        let request = bind(
            &root,
            &project,
            Path::new("var/source.json"),
            "stage",
            "illustrative",
            "直线滑台",
        )
        .unwrap();
        Self {
            _temp: temp,
            root,
            project,
            request,
        }
    }
}
#[test]
fn projection_and_state_coverage() {
    for (p, expected) in [
        ([1., 0., 0.], [0.894427190999916, 0.447213595499958]),
        ([0., 1., 0.], [-0.894427190999916, 0.447213595499958]),
        ([0., 0., 1.], [0., -1.095445115010332]),
    ] {
        let actual = project(p);
        for k in 0..2 {
            assert!((actual[k] - expected[k]).abs() < 1e-12);
        }
    }
    let evaluated = evaluate(&scene()).unwrap();
    assert_eq!(evaluated.frames.len(), 3);
    assert_eq!(evaluated.pair_check_count, 45);
    assert!(evaluated.possible_overlaps.is_empty());
    assert_eq!(evaluated.frames[0].parts[3].origin, [12., 8., 14.]);
    assert_eq!(evaluated.frames[1].parts[3].origin, [32., 8., 14.]);
    assert_eq!(evaluated.frames[2].parts[3].origin, [52., 8., 14.]);
}
#[test]
fn malformed_and_incomplete_scenes_are_rejected() {
    let original = scene();
    let mut cases = Vec::new();
    let mut s = original.clone();
    s.parts[1].id = s.parts[0].id.clone();
    cases.push(s);
    let mut s = original.clone();
    s.states[1].id = s.states[0].id.clone();
    cases.push(s);
    let mut s = original.clone();
    s.states[0].values.clear();
    cases.push(s);
    let mut s = original.clone();
    let duplicate = s.states[0].values[0].clone();
    s.states[0].values.push(duplicate);
    cases.push(s);
    let mut s = original.clone();
    s.states[0].values[0].parameter_id = "extra".into();
    cases.push(s);
    let mut s = original.clone();
    s.states[0].values[0].value = 41.;
    cases.push(s);
    let mut s = original.clone();
    s.states[0].values[0].value = f64::NAN;
    cases.push(s);
    let mut s = original.clone();
    s.parts[0].origin[0] = f64::INFINITY;
    cases.push(s);
    let mut s = original.clone();
    s.parts[3].axis = vec![2., 0., 0.];
    cases.push(s);
    let mut s = original.clone();
    s.parts[0].axis = vec![1., 0., 0.];
    cases.push(s);
    let mut s = original.clone();
    s.parts[4].size[1] = 15.;
    cases.push(s);
    let mut s = original.clone();
    s.parts[0].representation = "native_cad".into();
    cases.push(s);
    let mut s = original.clone();
    s.parts[0].primitive = "mesh".into();
    cases.push(s);
    let mut s = original.clone();
    s.unit = "inch".into();
    cases.push(s);
    let mut s = original.clone();
    s.parameters[0].unit = "mm".into();
    cases.push(s);
    let mut s = original.clone();
    s.parts.clear();
    cases.push(s);
    let mut s = original.clone();
    s.states.clear();
    cases.push(s);
    let mut s = original.clone();
    s.states = vec![s.states[0].clone(); 33];
    cases.push(s);
    for (i, s) in cases.iter().enumerate() {
        assert!(evaluate(s).is_err(), "invalid scene case {i}");
    }
}
#[test]
fn positive_overlap_is_a_candidate_and_touching_is_clear() {
    let mut s = scene();
    s.parts[3].origin[2] = 13.;
    let e = evaluate(&s).unwrap();
    assert_eq!(e.possible_overlaps.len(), 6);
    assert_eq!(e.pair_check_count, 45);
    assert!(
        e.possible_overlaps
            .iter()
            .any(|s| s == "position_0:rail_rear:carriage")
    );
}
#[test]
fn delivery_bytes_are_recomputed() {
    let f = Fixture::new();
    let out = Path::new("var/delivery");
    let receipt = generate(&f.root, &f.project, &f.request, out).unwrap();
    assert_eq!(verify(&f.root, &f.project, out).unwrap(), receipt);
    assert!(generate(&f.root, &f.project, &f.request, out).is_err());
    let files = receipt.files.clone();
    for file in files {
        let path = f.root.path().join(out).join(file.path);
        let original = fs::read(&path).unwrap();
        fs::write(&path, b"tampered").unwrap();
        assert!(verify(&f.root, &f.project, out).is_err());
        fs::write(path, original).unwrap();
    }
    let extra = f.root.path().join(out).join("unrecorded.txt");
    fs::write(&extra, b"extra").unwrap();
    assert!(verify(&f.root, &f.project, out).is_err());
    fs::remove_file(extra).unwrap();
    let path = f.root.path().join(out).join("state-01.svg");
    fs::remove_file(path).unwrap();
    assert!(verify(&f.root, &f.project, out).is_err());
}
#[test]
fn binding_changes_invalidate_and_rebuild() {
    let mut f = Fixture::new();
    let old = Path::new("var/old");
    generate(&f.root, &f.project, &f.request, old).unwrap();
    let old_bytes = fs::read(f.root.path().join(old).join("index.html")).unwrap();
    let mut changed = f.project.clone();
    changed
        .conditions
        .insert("illustrative".into(), "Changed scope".into());
    assert!(verify(&f.root, &changed, old).is_err());
    changed = f.project.clone();
    changed.revision = "2".into();
    assert!(verify(&f.root, &changed, old).is_err());
    changed = f.project.clone();
    changed.objects.insert("stage".into(), "2".into());
    assert!(verify(&f.root, &changed, old).is_err());
    let mut source = example();
    source["scene"]["states"][1]["values"][0]["value"] = json!(25.);
    let bytes = serde_json::to_vec_pretty(&source).unwrap();
    fs::write(f.root.path().join("var/source.json"), &bytes).unwrap();
    assert!(verify(&f.root, &f.project, old).is_err());
    f.project
        .evidence
        .insert("scene".into(), oe_core::sha256(&bytes));
    f.project.revision = "2".into();
    f.project.objects.insert("stage".into(), "2".into());
    let new_request = bind(
        &f.root,
        &f.project,
        Path::new("var/source.json"),
        "stage",
        "illustrative",
        "更新后的滑台",
    )
    .unwrap();
    generate(&f.root, &f.project, &new_request, Path::new("var/new")).unwrap();
    verify(&f.root, &f.project, Path::new("var/new")).unwrap();
    assert!(verify(&f.root, &f.project, old).is_err());
    assert_eq!(
        fs::read(f.root.path().join(old).join("index.html")).unwrap(),
        old_bytes
    );
    assert!(
        fs::read_to_string(f.root.path().join("var/new/index.html"))
            .unwrap()
            .contains("<strong>25</strong>")
    );
}
#[test]
fn labels_are_escaped_and_states_have_matching_controls() {
    let mut f = Fixture::new();
    f.request.title = "<script>alert('x')</script>".into();
    generate(&f.root, &f.project, &f.request, Path::new("var/html")).unwrap();
    let html = fs::read_to_string(f.root.path().join("var/html/index.html")).unwrap();
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(!html.contains("src=\"http"));
    assert_eq!(html.matches("type=\"radio\"").count(), 3);
    assert_eq!(html.matches(" checked").count(), 1);
    for i in 0..3 {
        assert!(html.contains(&format!("#choice-{i}:checked ~ .frames .frame-{i}")));
        assert!(html.contains(&format!("id=\"choice-{i}\"")));
    }
    for v in [0, 20, 40] {
        assert!(html.contains(&format!("<strong>{v}</strong>")));
    }
}
#[test]
fn receipts_and_requests_cannot_claim_other_sources() {
    let mut f = Fixture::new();
    f.request.source.sha256 = "0".repeat(64);
    assert!(generate(&f.root, &f.project, &f.request, Path::new("var/no")).is_err());
    assert!(!f.root.path().join("var/no").exists());
    f.request = bind(
        &f.root,
        &f.project,
        Path::new("var/source.json"),
        "stage",
        "illustrative",
        "stage",
    )
    .unwrap();
    generate(&f.root, &f.project, &f.request, Path::new("var/receipt")).unwrap();
    let path = f.root.path().join("var/receipt/receipt.json");
    let mut v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    v["pair_check_count"] = json!(0);
    fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(verify(&f.root, &f.project, Path::new("var/receipt")).is_err());
}

#[test]
fn support_faces_do_not_hide_supported_parts() {
    let scene = scene();
    let evaluated = evaluate(&scene).unwrap();
    let svg = oe_geometry::presentation::render(&scene, &evaluated, 0);
    let base_last = svg.rfind("<title>底座</title>").unwrap();
    let rail_first = svg.find("<title>后导轨</title>").unwrap();
    assert!(
        base_last < rail_first,
        "supporting base is drawn over its supported rear rail"
    );
}
