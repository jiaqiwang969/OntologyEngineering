//! Synthetic archive-only source graphs. No browser, Project adoption or native CAD.
#[path = "support/incad_cad_fixture.rs"]
mod fixture;
use fixture::{CadFixture, save};
use oe_core::{SkillRoot, sha256};
use oe_knowledge::incad::library;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const ARCHIVE: &str = "sources/misumi/incad/fixture-v1";
const INDEX: &str = "sources/.indexes/incad/fixture-v1";

struct LibraryFixture {
    _temp: tempfile::TempDir,
    root: SkillRoot,
    case: CadFixture,
}
impl LibraryFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        fs::write(
            path.join("SKILL.md"),
            "Synthetic archive-only library fixture\n",
        )
        .unwrap();
        let root = SkillRoot::new(&path).unwrap();
        let case = CadFixture::new(&path.join(ARCHIVE));
        let mut result = Self {
            _temp: temp,
            root,
            case,
        };
        result.normalize_records();
        result.discovery();
        result
    }
    fn normalize_records(&mut self) {
        let case_path = self.case.folder().join("case.json");
        let mut record = read(&case_path);
        record["official_bom"] = json!({"headers":["案例No.","零件No.","零件名称","制造厂名称","型式","数量"],
            "rows":[["000019","1","Synthetic coupling","MISUMI","MODEL-TEST-19","2"]],"links":[],"rows_count":1});
        record["visible_bom"] = record["official_bom"].clone();
        save(&case_path, &record);
        let reference_path = self.case.source.projection.join("reference.json");
        let mut reference = read(&reference_path);
        reference["bom"] = record["official_bom"].clone();
        reference["cad"] = record["cad_downloads"].clone();
        save(&reference_path, &reference);
        self.case.rebind_sources();
    }
    fn discovery(&self) {
        let case_sha = sha256(fs::read(self.case.folder().join("manifest.json")).unwrap());
        save(
            &self
                .root
                .path()
                .join(ARCHIVE)
                .join("design-inputs/index.json"),
            &json!({
                "schema":"incad.design-input-index/1","cases":[{"number":"000019",
                    "folder":"design-inputs/cn/000019/fixed","source_manifest_sha256":case_sha,
                    "input":{"title":"UNTRUSTED DISCOVERY TEXT MUST NOT BECOME SOURCE"}}]
            }),
        );
        save(
            &self
                .root
                .path()
                .join(ARCHIVE)
                .join("cases/cn/000019/current.json"),
            &json!({
                "site":"cn","number":"000019","folder":"cases/cn/000019/attempts/fixed","manifest_sha256":case_sha,
                "complete_for_requested_scope":false
            }),
        );
    }
    fn build(&self) -> Value {
        library::build(&self.root, Path::new(ARCHIVE), Path::new(INDEX)).unwrap()
    }
    fn graph(&self) -> Value {
        library::view(&self.root, Path::new(INDEX), "000019").unwrap()["graph"].clone()
    }
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        let dest = target.join(item.file_name());
        let kind = item.file_type().unwrap();
        assert!(!kind.is_symlink());
        if kind.is_dir() {
            copy_tree(&item.path(), &dest);
        } else {
            fs::copy(item.path(), dest).unwrap();
        }
    }
}
fn inventory(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    for item in fs::read_dir(root).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            out.extend(inventory(&item.path()));
        } else {
            out.push((item.path(), fs::read(item.path()).unwrap()));
        }
    }
    out
}

#[test]
fn incad_library_builds_source_bound_graph_without_project_adoption() {
    let f = LibraryFixture::new();
    let originals = inventory(&f.root.path().join(ARCHIVE));
    let built = f.build();
    assert_eq!(built["status"], "built");
    assert_eq!(built["case_count"], 1);
    assert_eq!(
        library::verify(&f.root, Path::new(INDEX)).unwrap()["status"],
        "verified"
    );
    let graph = f.graph();
    assert_eq!(graph["schema"], "oev2.incad-source-graph/1");
    assert_eq!(graph["case_number"], "000019");
    assert_eq!(graph["native_validation"], "not_run");
    assert_eq!(graph["engineering_adoption"], "not_assessed");
    assert_eq!(
        graph["source_revision"],
        sha256(fs::read(f.case.folder().join("manifest.json")).unwrap())
    );
    let kinds = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["kind"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    for kind in [
        "ConditionStatement",
        "BomRow",
        "ModelDesignation",
        "CadArchive",
        "CadMember",
        "SourceFile",
    ] {
        assert!(kinds.contains(kind), "{kind}: {graph:#}");
    }
    assert!(!graph.to_string().contains("UNTRUSTED DISCOVERY TEXT"));
    assert!(graph.to_string().contains("STEP(AP214)"));
    assert!(graph.to_string().contains("image_not_archived"));
    let search = library::search(&f.root, Path::new(INDEX), "MODEL-TEST-19", 10).unwrap();
    assert_eq!(search["status"], "candidates");
    assert_eq!(search["results"].as_array().unwrap().len(), 1);
    assert_eq!(search["results"][0]["graph"], graph);
    assert!(
        library::search(&f.root, Path::new(INDEX), "missing-keyword", 10).unwrap()["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(library::build(&f.root, Path::new(ARCHIVE), Path::new(INDEX)).is_err());
    assert!(!f.root.path().join("var/projects").exists());
    for (path, bytes) in originals {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn incad_library_rejects_changed_sources_and_rebound_graph() {
    for fault in ["source", "graph"] {
        let f = LibraryFixture::new();
        f.build();
        if fault == "source" {
            let path = f.case.folder().join("cad/ap203/files/model/part.step");
            let mut raw = fs::read(&path).unwrap();
            raw[0] ^= 1;
            fs::write(path, raw).unwrap();
        } else {
            let graph_path = f.root.path().join(INDEX).join("cases/000019.json");
            let mut graph = read(&graph_path);
            graph["title"] = json!("Forged source title with a rebound outer digest");
            let graph_sha = save(&graph_path, &graph);
            let path = f.root.path().join(INDEX).join("library.json");
            let mut manifest = read(&path);
            manifest["cases"][0]["graph"]["sha256"] = json!(graph_sha);
            manifest["cases"][0]["title"] = graph["title"].clone();
            save(&path, &manifest);
        }
        let before = inventory(&f.root.path().join(INDEX));
        assert!(
            library::verify(&f.root, Path::new(INDEX)).is_err(),
            "{fault}"
        );
        assert!(
            library::search(&f.root, Path::new(INDEX), "MODEL-TEST-19", 10).is_err(),
            "{fault}"
        );
        assert!(
            library::view(&f.root, Path::new(INDEX), "000019").is_err(),
            "{fault}"
        );
        for (path, bytes) in before {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
}

#[test]
fn incad_library_rejects_unsafe_or_incomplete_source_closure() {
    for fault in [
        "traversal",
        "duplicate",
        "missing_member",
        "bom_disagrees",
        "symlink",
    ] {
        let mut f = LibraryFixture::new();
        match fault {
            "traversal" | "duplicate" => {
                let path = f.root.path().join(ARCHIVE).join("design-inputs/index.json");
                let mut index = read(&path);
                if fault == "traversal" {
                    index["cases"][0]["folder"] = json!("../../outside");
                } else {
                    let duplicate = index["cases"][0].clone();
                    index["cases"].as_array_mut().unwrap().push(duplicate);
                }
                save(&path, &index);
            }
            "missing_member" => {
                fs::remove_file(f.case.folder().join("cad/ap203/files/model/part.step")).unwrap();
            }
            "bom_disagrees" => {
                let path = f.case.source.projection.join("reference.json");
                let mut reference = read(&path);
                reference["bom"]["rows"][0][4] = json!("UNSUPPORTED-MODEL");
                save(&path, &reference);
                f.case.rebind_sources();
                f.discovery();
            }
            _ => {
                #[cfg(unix)]
                {
                    let path = f.case.folder().join("source.html");
                    let held = f.root.path().join("held-source.html");
                    fs::rename(&path, &held).unwrap();
                    std::os::unix::fs::symlink(held, path).unwrap();
                }
                #[cfg(not(unix))]
                {
                    continue;
                }
            }
        }
        assert!(
            library::build(&f.root, Path::new(ARCHIVE), Path::new(INDEX)).is_err(),
            "{fault}"
        );
        assert!(!f.root.path().join(INDEX).exists(), "{fault}");
    }
    let f = LibraryFixture::new();
    for index in [
        "var/not-source-index",
        "sources/misumi/overwrite-source",
        "sources/.indexes/../escape",
    ] {
        assert!(
            library::build(&f.root, Path::new(ARCHIVE), Path::new(index)).is_err(),
            "{index}"
        );
        assert!(!f.root.path().join(index).exists());
    }
}

#[test]
fn incad_library_copies_portably_with_relative_source_bindings() {
    let f = LibraryFixture::new();
    f.build();
    let original = f.graph();
    let second = tempfile::tempdir().unwrap();
    let path = second.path().canonicalize().unwrap();
    fs::write(path.join("SKILL.md"), "Independent restored root\n").unwrap();
    copy_tree(&f.root.path().join(ARCHIVE), &path.join(ARCHIVE));
    copy_tree(&f.root.path().join(INDEX), &path.join(INDEX));
    let root = SkillRoot::new(&path).unwrap();
    fs::rename(
        f.root.path().join(ARCHIVE),
        f.root.path().join("held-original-archive"),
    )
    .unwrap();
    let manifest = fs::read_to_string(path.join(INDEX).join("library.json")).unwrap();
    assert!(!manifest.contains(f.root.path().to_str().unwrap()));
    assert_eq!(
        library::verify(&root, Path::new(INDEX)).unwrap()["status"],
        "verified"
    );
    assert_eq!(
        library::view(&root, Path::new(INDEX), "000019").unwrap()["graph"],
        original
    );
    assert_eq!(
        library::search(&root, Path::new(INDEX), "MODEL-TEST-19", 1).unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn incad_library_reads_large_cad_members_beyond_intake_limit() {
    let mut f = LibraryFixture::new();
    let bytes = vec![b'x'; 17 * 1024 * 1024];
    f.case.files.insert("model/part.step".into(), bytes.clone());
    f.case.publish();
    f.normalize_records();
    f.discovery();
    f.build();
    assert_eq!(
        library::verify(&f.root, Path::new(INDEX)).unwrap()["status"],
        "verified"
    );
    let graph = f.graph();
    assert_eq!(graph["native_validation"], "not_run");
    assert_eq!(
        fs::read(f.case.folder().join("cad/ap203/files/model/part.step")).unwrap(),
        bytes
    );
    assert!(
        graph["source_files"]
            .to_string()
            .contains(&(17 * 1024 * 1024).to_string())
    );
}

#[test]
fn incad_library_search_only_rehashes_returned_case_payloads() {
    let f = LibraryFixture::new();
    let archive = f.root.path().join(ARCHIVE);
    let folder = archive.join("cases/cn/000020/attempts/fixed");
    let projection = archive.join("design-inputs/cn/000020/fixed");
    copy_tree(&f.case.folder(), &folder);
    copy_tree(&f.case.source.projection, &projection);
    let renumber = |value: Value| {
        serde_json::from_str::<Value>(&value.to_string().replace("000019", "000020")).unwrap()
    };
    let record = renumber(read(&folder.join("case.json")));
    save(&folder.join("case.json"), &record);
    let mut manifest = renumber(read(&folder.join("manifest.json")));
    for file in manifest["files"].as_array_mut().unwrap() {
        *file = fixture::entry(&folder, file["path"].as_str().unwrap());
    }
    let case_sha = save(&folder.join("manifest.json"), &manifest);
    let mut projected = renumber(read(&projection.join("manifest.json")));
    let mut source = projected["source"].clone();
    source["manifest_sha256"] = json!(case_sha);
    source["case_record_sha256"] = fixture::entry(&folder, "case.json")["sha256"].clone();
    for name in [
        "input.json",
        "reference.json",
        "study.json",
        "original.json",
    ] {
        let path = projection.join(name);
        let mut value = renumber(read(&path));
        value["source"] = source.clone();
        save(&path, &value);
    }
    projected["source"] = source;
    for file in projected["files"].as_array_mut().unwrap() {
        *file = fixture::entry(&projection, file["path"].as_str().unwrap());
    }
    save(&projection.join("manifest.json"), &projected);
    let discovery_path = archive.join("design-inputs/index.json");
    let mut discovery = read(&discovery_path);
    discovery["cases"].as_array_mut().unwrap().push(json!({
        "number":"000020","folder":"design-inputs/cn/000020/fixed","source_manifest_sha256":case_sha
    }));
    save(&discovery_path, &discovery);
    assert_eq!(f.build()["case_count"], 2);
    let bad_path = f.case.folder().join("cad/ap203/files/model/part.step");
    let mut raw = fs::read(&bad_path).unwrap();
    raw[0] ^= 1;
    fs::write(&bad_path, &raw).unwrap();
    let healthy = library::search(&f.root, Path::new(INDEX), "000020", 1).unwrap();
    assert_eq!(healthy["status"], "candidates");
    assert_eq!(healthy["results"].as_array().unwrap().len(), 1);
    assert_eq!(healthy["results"][0]["case_number"], "000020");
    assert_eq!(healthy["coverage"]["declarations_rebuilt"], 2);
    assert_eq!(healthy["coverage"]["returned_source_closures_verified"], 1);
    assert_eq!(
        healthy["coverage"]["unreturned_cad_byte_revalidation"],
        "not_run"
    );
    assert_eq!(healthy["results"][0]["cad_byte_revalidation"], "verified");
    assert!(library::verify(&f.root, Path::new(INDEX)).is_err());
    assert!(library::search(&f.root, Path::new(INDEX), "000019", 1).is_err());
    assert_eq!(fs::read(bad_path).unwrap(), raw);
}

#[test]
fn incad_library_preserves_missing_purpose_without_inventing_it() {
    let mut f = LibraryFixture::new();
    let input_path = f.case.source.projection.join("input.json");
    let mut input = read(&input_path);
    input["purpose"] = json!("");
    save(&input_path, &input);
    f.case.rebind_sources();
    f.discovery();
    let index = Path::new("sources/.indexes/incad");
    library::build(&f.root, Path::new(ARCHIVE), index).unwrap();
    assert_eq!(
        library::verify(&f.root, index).unwrap()["status"],
        "verified"
    );
    let graph = library::view(&f.root, index, "000019").unwrap()["graph"].clone();
    assert_eq!(graph["purpose"], "");
    assert_eq!(graph["coverage"]["purpose_status"], "missing");
    assert_eq!(graph["engineering_adoption"], "not_assessed");
}
