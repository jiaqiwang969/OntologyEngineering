use anyhow::{Context, Result};
use semantica_contracts::SemanticPackageInput;
use semantica_runtime::{books::SourceProvider, dataset::DatasetSession, packages};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl SourceProvider for Memory {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        self.0.get(id).cloned().context("test source missing")
    }
}
fn input(manifest: Value) -> SemanticPackageInput {
    SemanticPackageInput {
        schema: packages::INPUT_SCHEMA.into(),
        manifest: Some(manifest),
        manifest_path: None,
        base_path: None,
        inline_bytes: BTreeMap::new(),
    }
}
fn manifest(id: &str, assets: Value) -> Value {
    json!({"schema_version":"1.0","package_id":id,"version":"1","assets":assets})
}
const RDF: &str = "<urn:item> <urn:status> \"ready\" .";
fn load(
    s: &mut DatasetSession,
    m: Value,
) -> Result<semantica_contracts::SemanticPackageLoadResult> {
    s.load_package(&input(m), &Memory::default())
}

#[test]
fn inline_empty_actual_digest_without_declared_sha_has_independent_golden() {
    let mut session = DatasetSession::new();
    let r = load(
        &mut session,
        manifest("p", json!([{"asset_id":"empty","role":"sparql","data":""}])),
    )
    .unwrap();
    assert_eq!(
        r.identity.digest,
        "ee9a15f7a692401ef552b30a80bcc87e4e380eda86fe436b2b0b15c15042c186"
    );
    assert_eq!(
        r.assets[0].sha256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!((r.rdf_added, r.revision), (0, 0));
    assert!(r.assets[0].content.is_empty());
    assert_eq!(r.assets[0].source_authenticity, "unknown");
    assert_eq!(r.assets[0].expected_sha256, None);
    assert_eq!(r.assets[0].format, None);
    assert_eq!(r.identity.namespace, None);
    assert!(!r.assets[0].loaded_into_dataset);
    assert_eq!(session.package_identity("p", "1").unwrap(), r.identity);
    assert_eq!(session.packages(), vec![r.identity]);
    let empty_rdf = load(
        &mut session,
        manifest(
            "empty-rdf",
            json!([{"asset_id":"rdf","role":"rdf","data":""}]),
        ),
    )
    .unwrap();
    assert_eq!((empty_rdf.rdf_added, empty_rdf.revision), (0, 0));
    assert!(empty_rdf.assets[0].loaded_into_dataset);
}

#[test]
fn native_bytes_and_text_path_share_actual_identity_and_retain_first_input() {
    let content = vec![0, 255, 10, 0];
    let mut provider = Memory::default();
    provider.0.insert("asset.bin".into(), content.clone());
    let mut session = DatasetSession::new();
    let mut path = input(manifest(
        "p",
        json!([{"asset_id":"binary","role":"view","path":"asset.bin"}]),
    ));
    path.base_path = Some("".into());
    let first = packages::load_package(&mut session, &provider, &path).unwrap();
    let mut native = input(manifest(
        "p",
        json!([{"asset_id":"binary","role":"view","data":""}]),
    ));
    native.inline_bytes.insert("binary".into(), content.clone());
    let repeated = packages::load_package(&mut session, &provider, &native).unwrap();
    assert_eq!(repeated.identity, first.identity);
    assert_eq!(repeated.assets, first.assets);
    assert_eq!(repeated.assets[0].data_origin, "path");
    assert_eq!(
        session.package_asset("p", "1", "binary").unwrap().content,
        content
    );
    native.manifest.as_mut().unwrap()["assets"][0]["sha256"] =
        json!(first.assets[0].sha256.to_uppercase());
    assert_eq!(
        packages::load_package(&mut session, &provider, &native)
            .unwrap()
            .assets[0]
            .expected_sha256,
        None
    );
    native.manifest.as_mut().unwrap()["assets"][0]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    native.inline_bytes.get_mut("binary").unwrap().push(1);
    assert!(
        packages::load_package(&mut session, &provider, &native)
            .unwrap_err()
            .to_string()
            .contains("digest_conflict")
    );
    assert_eq!(
        session.package_asset("p", "1", "binary").unwrap(),
        first.assets[0]
    );
    assert!(session.package_asset("p", "1", "missing").is_err());
    let mut text_session = DatasetSession::new();
    let from_text = load(
        &mut text_session,
        manifest(
            "t",
            json!([{"asset_id":"text","role":"view","data":"全文😀\n"}]),
        ),
    )
    .unwrap();
    let mut text_native = input(manifest(
        "t",
        json!([{"asset_id":"text","role":"view","data":""}]),
    ));
    text_native
        .inline_bytes
        .insert("text".into(), "全文😀\n".as_bytes().to_vec());
    assert_eq!(
        packages::load_package(&mut text_session, &provider, &text_native)
            .unwrap()
            .identity,
        from_text.identity
    );
}

#[test]
fn json_yaml_and_file_parent_override_share_package_semantics() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("package")).unwrap();
    fs::write(dir.path().join("package/data.txt"), RDF).unwrap();
    let raw = manifest(
        " files ",
        json!([{"asset_id":" rdf ","role":" ontology ","path":"data.txt"}]),
    );
    fs::write(
        dir.path().join("package/package.JSON"),
        serde_json::to_vec(&raw).unwrap(),
    )
    .unwrap();
    let provider = packages::FileProvider::new(dir.path()).unwrap();
    let mut request = input(Value::Null);
    request.manifest = None;
    request.manifest_path = Some("package/package.JSON".into());
    request.base_path = Some("wrong-must-be-ignored".into());
    let mut session = DatasetSession::new();
    let first = packages::load_package(&mut session, &provider, &request).unwrap();
    assert_eq!(first.identity.package_id, "files");
    assert_eq!(first.assets[0].asset_id, "rdf");
    assert_eq!(first.assets[0].role, "ontology");
    assert_eq!(first.assets[0].format, None); // execution defaults Turtle, DTO keeps null
    assert_eq!((first.rdf_added, first.revision), (1, 1));
    let yaml = format!(
        "schema_version: '1.0'\npackage_id: files\nversion: '1'\ndigest: {}\nassets:\n  - asset_id: rdf\n    role: ontology\n    path: data.txt\n    sha256: {}\n",
        first.identity.digest.to_uppercase(),
        first.assets[0].sha256.to_uppercase()
    );
    fs::write(dir.path().join("package/package.YML"), yaml).unwrap();
    request.manifest_path = Some("package/package.YML".into());
    let repeat = packages::load_package(&mut session, &provider, &request).unwrap();
    assert_eq!(repeat.assets, first.assets);
    assert_eq!((repeat.rdf_added, repeat.revision), (0, 1));
    let mut direct = DatasetSession::new();
    assert_eq!(
        packages::load_package_file(&mut direct, &dir.path().join("package/package.YML"))
            .unwrap()
            .identity,
        first.identity
    );
    let mut mapped = input(raw);
    mapped.base_path = Some("package".into());
    assert_eq!(
        packages::load_package(&mut direct, &provider, &mapped)
            .unwrap()
            .identity,
        first.identity
    );
}

#[test]
fn flags_format_aliases_and_named_graph_follow_original_semantics() {
    let mut session = DatasetSession::new();
    let aliases = [
        ("TTL", "turtle"),
        ("turtle", "turtle"),
        ("nt", "nt"),
        ("ntriples", "nt"),
        (" n-triples ", "nt"),
        ("RDF/XML", "xml"),
        ("rdfxml", "xml"),
        ("xml", "xml"),
        ("jsonld", "json-ld"),
        ("json_ld", "json-ld"),
        ("json-ld", "json-ld"),
        ("TriX", "trix"),
        ("nq", "nquads"),
        ("n-quads", "nquads"),
        ("nquads", "nquads"),
        ("TRIG", "trig"),
    ];
    for (i, (alias, expected)) in aliases.iter().enumerate() {
        let r=load(&mut session,manifest(&format!("alias{i}"),json!([{"asset_id":"shape","role":"SHAPES","data":"retained unparsed","format":alias,"graph_name":"invalid graph"}]))).unwrap();
        assert_eq!(r.assets[0].format.as_deref(), Some(*expected));
        assert!(!r.assets[0].loaded_into_dataset);
    }
    for alias in ["n3", "rdf-xml"] {
        assert!(
            load(
                &mut session,
                manifest(
                    alias,
                    json!([{"asset_id":"shape","role":"shapes","data":"","format":alias}])
                )
            )
            .is_err()
        );
    }
    let off=load(&mut session,manifest("null",json!([{"asset_id":"rdf","role":"ontology","data":"not RDF {","load_into_dataset":null}]))).unwrap();
    assert!(!off.assets[0].loaded_into_dataset);
    assert_eq!(session.quad_count(), 0);
    let view=load(&mut session,manifest("view",json!([{"asset_id":"view","role":"view","data":"retained","format":" Weird Format ","graph_name":"bad graph"}]))).unwrap();
    assert_eq!(view.assets[0].format.as_deref(), Some(" Weird Format "));
    let graph=load(&mut session,manifest("named",json!([{"asset_id":"rdf","role":"view","kind":" RDF ","load_into_dataset":true,"data":RDF,"graph_name":"urn:graph"}]))).unwrap();
    assert_eq!(graph.rdf_added, 1);
    assert_eq!(
        semantica_runtime::graph::query(
            session.dataset(),
            "ASK { GRAPH <urn:graph> { <urn:item> <urn:status> \"ready\" } }"
        )
        .unwrap()["value"],
        true
    );
    for syntax in ["trig", "nquads"] {
        assert!(load(&mut session,manifest(syntax,json!([{"asset_id":"rdf","role":"rdf","data":"","format":syntax,"graph_name":"urn:graph"}]))).is_err());
    }
    for flag in [json!(false), json!(null), json!(0), json!(""), json!([])] {
        assert!(!load(&mut session,manifest("off",json!([{"asset_id":"rdf","role":"rdf","data":"invalid {","load_into_dataset":flag}]))).unwrap().assets[0].loaded_into_dataset);
    }
    assert!(
        load(
            &mut session,
            manifest(
                "string-flag",
                json!([{"asset_id":"rdf","role":"rdf","data":RDF,"load_into_dataset":"false"}])
            )
        )
        .unwrap()
        .assets[0]
            .loaded_into_dataset
    );
    assert!(
        load(
            &mut session,
            manifest(
                "badview",
                json!([{"asset_id":"a","role":"view","data":"x","load_into_dataset":true}])
            )
        )
        .is_err()
    );
}

#[test]
fn late_failure_reload_readback_and_clear_keep_atomic_state_and_original_assets() {
    let mut provider = Memory::default();
    provider.0.insert("rdf.ttl".into(), RDF.as_bytes().to_vec());
    let mut request = input(manifest(
        "p",
        json!([{"asset_id":"rdf","role":"ontology","path":"rdf.ttl"}]),
    ));
    request.base_path = Some("".into());
    let mut session = DatasetSession::new();
    let first = packages::load_package(&mut session, &provider, &request).unwrap();
    let cached = session.query("ASK { <urn:item> ?p ?o }", true).unwrap();
    let snapshot = session.snapshot().unwrap();
    let bad = manifest(
        "bad",
        json!([{"asset_id":"a","role":"rdf","data":"<urn:new> <urn:p> \"x\" ."},{"asset_id":"z","role":"rdf","data":"invalid {","format":"turtle"}]),
    );
    assert!(load(&mut session, bad).is_err());
    assert_eq!(session.snapshot().unwrap(), snapshot);
    assert_eq!(
        session
            .query("ASK { <urn:item> ?p ?o }", true)
            .unwrap()
            .result,
        cached.result
    );
    assert!(session.package_identity("bad", "1").is_err());
    assert_eq!(session.packages().len(), 1);
    provider.0.remove("rdf.ttl");
    assert!(packages::load_package(&mut session, &provider, &request).is_err());
    assert_eq!(
        session.package_asset("p", "1", "rdf").unwrap().content,
        RDF.as_bytes()
    );
    provider
        .0
        .insert("rdf.ttl".into(), b"<urn:changed> <urn:p> \"x\" .".to_vec());
    assert!(packages::load_package(&mut session, &provider, &request).is_err());
    provider.0.insert("rdf.ttl".into(), RDF.as_bytes().to_vec());
    session.clear(None).unwrap();
    let revision = session.revision();
    let again = packages::load_package(&mut session, &provider, &request).unwrap();
    assert_eq!((again.rdf_added, again.revision), (0, revision));
    assert_eq!(session.quad_count(), 0);
    assert_eq!(again.assets, first.assets);
    let other = load(
        &mut session,
        manifest("other", json!([{"asset_id":"rdf","role":"rdf","data":RDF}])),
    )
    .unwrap();
    assert_eq!(other.rdf_added, 1);
    let dup = load(
        &mut session,
        manifest(
            "duplicate",
            json!([{"asset_id":"rdf","role":"rdf","data":RDF}]),
        ),
    )
    .unwrap();
    assert_eq!(dup.rdf_added, 0);
    assert_eq!(dup.revision, other.revision + 1);
    session.update("CLEAR ALL").unwrap();
    assert_eq!(
        packages::load_package(&mut session, &provider, &request)
            .unwrap()
            .rdf_added,
        0
    );
    assert_eq!(session.quad_count(), 0);
}

#[test]
fn path_key_presence_root_containment_and_internal_symlink_follow_original() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("pkg")).unwrap();
    fs::create_dir(dir.path().join("pkg/sub")).unwrap();
    fs::write(dir.path().join("pkg/value.bin"), [0u8, 255, 10]).unwrap();
    fs::write(dir.path().join("outside.bin"), b"outside").unwrap();
    let provider = packages::FileProvider::new(dir.path()).unwrap();
    let mut session = DatasetSession::new();
    let mut good = input(manifest(
        "root",
        json!([{"asset_id":"a","role":"view","path":"sub/../value.bin"}]),
    ));
    good.base_path = Some("pkg".into());
    assert_eq!(
        packages::load_package(&mut session, &provider, &good)
            .unwrap()
            .assets[0]
            .content,
        [0, 255, 10]
    );
    for bad in [
        json!({"asset_id":"a","role":"view","data":null}),
        json!({"asset_id":"a","role":"view","data":[0,255]}),
        json!({"asset_id":"a","role":"view","data":"x","path":null}),
        json!({"asset_id":"a","role":"view","path":"../outside.bin"}),
        json!({"asset_id":"a","role":"view","path":"/absolute.bin"}),
    ] {
        let mut bad = input(manifest("bad", json!([bad])));
        bad.base_path = Some("pkg".into());
        assert!(packages::load_package(&mut session, &provider, &bad).is_err());
    }
    assert!(load(&mut session,manifest("dupe",json!([{"asset_id":"a","role":"view","data":""},{"asset_id":" a ","role":"view","data":""}]))).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("value.bin", dir.path().join("pkg/inside.link")).unwrap();
        good.manifest.as_mut().unwrap()["assets"][0]["path"] = json!("inside.link");
        assert_eq!(
            packages::load_package(&mut session, &provider, &good)
                .unwrap()
                .assets[0]
                .content,
            [0, 255, 10]
        );
        std::os::unix::fs::symlink("../outside.bin", dir.path().join("pkg/outside.link")).unwrap();
        good.manifest.as_mut().unwrap()["assets"][0]["path"] = json!("outside.link");
        assert!(packages::load_package(&mut session, &provider, &good).is_err());
    }
}
