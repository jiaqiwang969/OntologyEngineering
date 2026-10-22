use oe_semantic::{
    MAX_EXPANDED_BYTES, canonical_json, digest, load_bundle, safe_relative, strict_json,
    validate_archive, validate_data_archive,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zip::write::SimpleFileOptions;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

static SERIAL: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let base = root().join("var/builds/rust-semantic-tests");
        fs::create_dir_all(&base).unwrap();
        let path = base.join(format!(
            "{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn archive(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (path, bytes) in entries {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn data_spec(entries: &[(String, Vec<u8>)], bytes: &[u8]) -> Value {
    json!({"sha256": digest(bytes), "files": entries.iter().map(|(path,data)| json!({"path":path,"sha256":digest(data)})).collect::<Vec<_>>()})
}

fn failure(result: anyhow::Result<impl std::fmt::Debug>, fragment: &str) {
    let message = format!("{:#}", result.unwrap_err());
    assert!(
        message.contains(fragment),
        "expected {fragment:?}, got {message}"
    );
}

#[derive(Clone)]
struct Synthetic {
    payload: BTreeMap<String, Vec<u8>>,
    spec: Value,
}

impl Synthetic {
    fn new() -> Self {
        let mut payload = BTreeMap::new();
        payload.insert(
            "assets/selected.json".into(),
            b"{\"example\":\"synthetic\"}\r\n".to_vec(),
        );
        payload.insert(
            "assets/scenarios.json".into(),
            serde_json::to_vec(&json!({"package_id":"synthetic.test", "scenarios":[{"id":"s1"}]}))
                .unwrap(),
        );
        payload.insert("origin.json".into(), serde_json::to_vec(&json!({"native_candidate_sha256":"a".repeat(64),"native_package_sha256":"b".repeat(64)})).unwrap());
        let assets = ["selected", "scenarios"].into_iter().map(|id| {
            let path = format!("assets/{id}.json");
            json!({"asset_id":id, "path":path,"sha256":digest(&payload[&path]),"kind":"json","role":"metadata","format":"json"})
        }).collect::<Vec<_>>();
        payload.insert(
            "manifest.json".into(),
            serde_json::to_vec(
                &json!({"package_id":"synthetic.test","version":"1","assets":assets}),
            )
            .unwrap(),
        );
        let spec = json!({"path":"runtime/vendor/test.zip", "manifest":"manifest.json", "package_id":"synthetic.test", "package_version":"1",
            "candidate_sha256":"a".repeat(64),"package_sha256":"b".repeat(64),"asset_count":2,"scenario_count":1,
            "scenario_registry":"assets/scenarios.json","runtime":{"version":"test","commit":"c".repeat(40),"wheel_sha256":"d".repeat(64)}});
        Self { payload, spec }
    }
    fn bytes(&mut self) -> Vec<u8> {
        let entries = self
            .payload
            .iter()
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect::<Vec<_>>();
        let raw = archive(&entries);
        let lock = data_spec(&entries, &raw);
        self.spec["files"] = lock["files"].clone();
        self.spec["sha256"] = lock["sha256"].clone();
        self.spec["manifest_sha256"] = digest(&self.payload["manifest.json"]).into();
        raw
    }
    fn manifest(&mut self, change: impl FnOnce(&mut Value)) {
        let mut value = strict_json(&self.payload["manifest.json"]).unwrap();
        change(&mut value);
        self.payload
            .insert("manifest.json".into(), serde_json::to_vec(&value).unwrap());
    }
    fn write(&mut self, directory: &Path, name: &str) {
        let bytes = self.bytes();
        fs::create_dir_all(directory.join("runtime/vendor")).unwrap();
        fs::write(directory.join(self.spec["path"].as_str().unwrap()), bytes).unwrap();
        fs::write(directory.join("runtime/semantic-bundles.json"), serde_json::to_vec(&json!({"schema_version":"ontology-engineering.semantica-bundle-lock/v1","bundles":{name:self.spec.clone()}})).unwrap()).unwrap();
        fs::write(directory.join("runtime/semantica-source-lock.json"), serde_json::to_vec(&json!({"source":{"version":"test","commit":"c".repeat(40)},"artifact":{"sha256":"d".repeat(64)}})).unwrap()).unwrap();
    }
}

#[test]
fn actual_locked_packages_validate_the_complete_native_inventory() {
    for (name, assets, scenarios) in [
        ("manufacturing-process-cost", 141, 68),
        ("engineering-evidence-methods", 314, 265),
        ("engineering-judgment-intake", 159, 119),
    ] {
        let bundle = load_bundle(&root(), name).unwrap();
        assert_eq!(bundle.report["assets"], assets);
        assert_eq!(bundle.report["files"], assets + 2);
        assert_eq!(
            bundle.report["scenarios"].as_array().unwrap().len(),
            scenarios
        );
        for asset in bundle.manifest["assets"].as_array().unwrap() {
            assert_eq!(
                digest(&bundle.payload[asset["path"].as_str().unwrap()]),
                asset["sha256"]
            );
        }
    }
}

#[test]
fn full_transport_survives_relocation_and_rejects_runtime_changes() {
    let scratch = Scratch::new();
    let destination = scratch.0.join("copied skill with spaces");
    let original = load_bundle(&root(), "manufacturing-process-cost").unwrap();
    for name in [
        "runtime/semantic-bundles.json",
        "runtime/semantica-source-lock.json",
        original.spec["path"].as_str().unwrap(),
    ] {
        let target = destination.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(name), target).unwrap();
    }
    assert_eq!(
        load_bundle(&destination, "manufacturing-process-cost")
            .unwrap()
            .report,
        original.report
    );
    let lock = destination.join("runtime/semantica-source-lock.json");
    let mut value = strict_json(&fs::read(&lock).unwrap()).unwrap();
    value["source"]["commit"] = "0".repeat(40).into();
    fs::write(lock, serde_json::to_vec(&value).unwrap()).unwrap();
    failure(
        load_bundle(&destination, "manufacturing-process-cost"),
        "different source-locked runtime",
    );
}

#[test]
fn traversal_and_ambiguous_names_are_rejected_without_normalization() {
    for name in [
        "",
        "/a.json",
        "../a.json",
        "a/../b.json",
        "a//b.json",
        "./a.json",
        "a/./b.json",
        "a\\b.json",
        "C:a.json",
        "a\0.json",
        "a\n.json",
        "directory/",
    ] {
        assert!(safe_relative(name).is_err(), "accepted {name:?}");
    }
    assert_eq!(safe_relative("目录/数据.json").unwrap(), "目录/数据.json");
}

#[test]
fn changed_outer_or_member_bytes_cannot_be_relocked_at_only_one_layer() {
    let mut fixture = Synthetic::new();
    let raw = fixture.bytes();
    let mut changed = raw.clone();
    changed.push(0);
    failure(
        validate_archive(&changed, &fixture.spec),
        "bundle hash mismatch",
    );
    let spec = fixture.spec.clone();
    fixture
        .payload
        .get_mut("assets/selected.json")
        .unwrap()
        .push(b' ');
    let changed = fixture.bytes();
    let mut outer_only = spec;
    outer_only["sha256"] = digest(&changed).into();
    failure(
        validate_archive(&changed, &outer_only),
        "member hash mismatch",
    );
    failure(
        validate_archive(&changed, &fixture.spec),
        "native asset hash mismatch",
    );
}

#[test]
fn transport_rejects_unlisted_missing_duplicate_locks_and_nondata_members() {
    let entries = vec![
        ("a.json".into(), b"{}".to_vec()),
        ("b.json".into(), b"[]".to_vec()),
    ];
    let bytes = archive(&entries);
    let spec = data_spec(&entries, &bytes);
    let mut changed = spec.clone();
    changed["files"].as_array_mut().unwrap().pop();
    failure(
        validate_data_archive(&bytes, &changed),
        "unlisted archive member",
    );
    changed = spec.clone();
    changed["files"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"missing.json","sha256":digest(b"{}")}));
    failure(
        validate_data_archive(&bytes, &changed),
        "incomplete bundle inventory",
    );
    changed = spec;
    let first = changed["files"][0].clone();
    changed["files"].as_array_mut().unwrap().push(first);
    failure(
        validate_data_archive(&bytes, &changed),
        "duplicate locked file",
    );
    for name in ["unsafe.py", "json", ".json", "assets/.json"] {
        let entries = vec![(name.into(), b"pass".to_vec())];
        let bytes = archive(&entries);
        failure(
            validate_data_archive(&bytes, &data_spec(&entries, &bytes)),
            "non-data asset",
        );
    }
}

fn central_positions(bytes: &[u8]) -> Vec<usize> {
    bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == b"PK\x01\x02").then_some(i))
        .collect()
}

#[test]
fn duplicate_raw_members_are_rejected_even_if_zip_lookup_would_coalesce_them() {
    let entries = vec![
        ("a.json".into(), b"{}".to_vec()),
        ("b.json".into(), b"{}".to_vec()),
    ];
    let mut bytes = archive(&entries);
    let positions = bytes
        .windows(6)
        .enumerate()
        .filter_map(|(i, w)| (w == b"b.json").then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 2);
    for position in positions {
        bytes[position] = b'a';
    }
    failure(
        validate_data_archive(&bytes, &data_spec(&entries, &bytes)),
        "duplicate archive member",
    );
}

#[test]
fn archive_special_files_encryption_expansion_and_local_name_conflicts_are_rejected() {
    let entries = vec![("a.json".into(), b"{}".to_vec())];
    let baseline = archive(&entries);
    let central = central_positions(&baseline)[0];
    for (field, bytes, message) in [
        (
            central + 38,
            ((0o120777u32) << 16).to_le_bytes().to_vec(),
            "regular files",
        ),
        (central + 8, 1u16.to_le_bytes().to_vec(), "encrypted"),
        (
            central + 24,
            ((MAX_EXPANDED_BYTES + 1) as u32).to_le_bytes().to_vec(),
            "expanded-size",
        ),
        (30, b"b".to_vec(), "names differ"),
    ] {
        let mut changed = baseline.clone();
        changed[field..field + bytes.len()].copy_from_slice(&bytes);
        failure(
            validate_data_archive(&changed, &data_spec(&entries, &changed)),
            message,
        );
    }
}

#[test]
fn manifest_identity_count_duplicate_ids_and_paths_remain_independent_checks() {
    for kind in [
        "identity",
        "count",
        "duplicate_id",
        "duplicate_path",
        "missing_inventory",
    ] {
        let mut fixture = Synthetic::new();
        fixture.manifest(|manifest| match kind {
            "identity" => manifest["version"] = "wrong".into(),
            "count" => {
                manifest["assets"].as_array_mut().unwrap().pop();
            }
            "duplicate_id" => {
                manifest["assets"][1]["asset_id"] = manifest["assets"][0]["asset_id"].clone()
            }
            "duplicate_path" => {
                manifest["assets"][1]["path"] = manifest["assets"][0]["path"].clone()
            }
            _ => manifest["assets"][0]["path"] = "assets/absent.json".into(),
        });
        let raw = fixture.bytes();
        let expected = match kind {
            "identity" => "package identity",
            "count" => "asset count",
            "duplicate_id" | "duplicate_path" => "duplicate native asset",
            _ => "inventories differ",
        };
        failure(validate_archive(&raw, &fixture.spec), expected);
    }
}

#[test]
fn manifest_hash_and_origin_hash_have_separate_locks() {
    let mut fixture = Synthetic::new();
    let raw = fixture.bytes();
    let mut changed = fixture.spec.clone();
    changed["manifest_sha256"] = "0".repeat(64).into();
    failure(validate_archive(&raw, &changed), "manifest hash mismatch");
    changed = fixture.spec.clone();
    changed["candidate_sha256"] = "0".repeat(64).into();
    failure(
        validate_archive(&raw, &changed),
        "native origin digest mismatch",
    );
}

#[test]
fn scenario_registry_identity_uniqueness_and_count_are_checked() {
    for kind in ["identity", "duplicate", "count"] {
        let mut fixture = Synthetic::new();
        let scenarios = if kind == "duplicate" {
            json!([{"id":"s1"},{"id":"s1"}])
        } else {
            json!([{"id":"s1"}])
        };
        let bytes=serde_json::to_vec(&json!({"package_id":if kind=="identity" {"other"}else{"synthetic.test"},"scenarios":scenarios})).unwrap();
        let sha = digest(&bytes);
        fixture
            .payload
            .insert("assets/scenarios.json".into(), bytes);
        fixture.manifest(|manifest| manifest["assets"][1]["sha256"] = sha.into());
        if kind == "count" {
            fixture.spec["scenario_count"] = 2.into();
        }
        let raw = fixture.bytes();
        failure(
            validate_archive(&raw, &fixture.spec),
            if kind == "identity" {
                "registry identity"
            } else {
                "scenario inventory"
            },
        );
    }
}

#[cfg(unix)]
#[test]
fn filesystem_symlinks_at_root_parent_lock_and_archive_are_rejected() {
    use std::os::unix::fs::symlink;
    for location in ["root", "parent", "lock", "archive"] {
        let scratch = Scratch::new();
        let directory = scratch.0.join("skill");
        Synthetic::new().write(&directory, "test");
        let mut requested = directory.clone();
        match location {
            "root" => {
                requested = scratch.0.join("alias");
                symlink(&directory, &requested).unwrap();
            }
            "parent" => {
                let moved = directory.join("real-vendor");
                fs::rename(directory.join("runtime/vendor"), &moved).unwrap();
                symlink(moved, directory.join("runtime/vendor")).unwrap();
            }
            other => {
                let path = directory.join(if other == "lock" {
                    "runtime/semantic-bundles.json"
                } else {
                    "runtime/vendor/test.zip"
                });
                let moved = directory.join("original");
                fs::rename(&path, &moved).unwrap();
                symlink(moved, path).unwrap();
            }
        }
        failure(load_bundle(&requested, "test"), "symbolic link");
    }
}

#[test]
fn locks_reject_unknown_names_schemas_duplicate_json_and_traversal() {
    for kind in ["name", "schema", "duplicate", "traversal"] {
        let scratch = Scratch::new();
        let mut fixture = Synthetic::new();
        fixture.write(&scratch.0, "test");
        let lock = scratch.0.join("runtime/semantic-bundles.json");
        let mut value = strict_json(&fs::read(&lock).unwrap()).unwrap();
        match kind {
            "schema" => value["schema_version"] = "other".into(),
            "traversal" => {
                value["bundles"]["test"]["path"] = "runtime/../runtime/vendor/test.zip".into()
            }
            _ => {}
        }
        let mut bytes = serde_json::to_vec(&value).unwrap();
        if kind == "duplicate" {
            bytes.splice(1..1, b"\"bundles\":{},".iter().copied());
        }
        fs::write(lock, bytes).unwrap();
        failure(
            load_bundle(&scratch.0, if kind == "name" { "unknown" } else { "test" }),
            match kind {
                "name" => "not declared",
                "schema" => "unknown bundle lock schema",
                "duplicate" => "duplicate_json_key",
                _ => "inside its package",
            },
        );
    }
}

#[test]
fn strict_json_preserves_large_integers_and_real_objects_using_serde_internal_names() {
    let raw=br#"{"large":1844674407370955161600000000000001,"negative":-1844674407370955161600000000000001,"object":{"$serde_json::private::Number":"1.25"}}"#;
    let value = strict_json(raw).unwrap();
    assert_eq!(
        value["large"].to_string(),
        "1844674407370955161600000000000001"
    );
    assert_eq!(
        value["negative"].to_string(),
        "-1844674407370955161600000000000001"
    );
    assert!(value["object"].is_object());
    assert_eq!(
        strict_json(&canonical_json(&value).unwrap()).unwrap(),
        value
    );
}

#[test]
fn duplicate_and_nonfinite_json_are_rejected_at_every_depth() {
    for raw in [
        r#"{"a":1,"a":2}"#,
        r#"{"x":[{"a":1,"\u0061":2}]}"#,
        r#"{"n":NaN}"#,
        r#"{"n":Infinity}"#,
        r#"{"n":-Infinity}"#,
        r#"{"n":1e400}"#,
        r#"{"n":-1e400}"#,
        r#"{} trailing"#,
    ] {
        assert!(strict_json(raw.as_bytes()).is_err(), "accepted {raw}");
    }
    let too_deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
    assert!(strict_json(too_deep.as_bytes()).is_err());
}

#[test]
fn canonical_numbers_match_python_v1_float_and_integer_spelling() {
    for (raw, expected) in [
        ("1e-7", "1e-07"),
        ("1e-6", "1e-06"),
        ("1e-5", "1e-05"),
        ("1e-4", "0.0001"),
        ("1e15", "1000000000000000.0"),
        ("1e16", "1e+16"),
        ("1e20", "1e+20"),
        ("1.23000", "1.23"),
        ("-0", "0"),
        ("-0.0", "-0.0"),
        ("1e-400", "0.0"),
        ("-1e-400", "-0.0"),
        ("5e-324", "5e-324"),
        ("1.7976931348623157e308", "1.7976931348623157e+308"),
        ("0.10000000000000001", "0.1"),
        (
            "123456789012345678901234567890",
            "123456789012345678901234567890",
        ),
    ] {
        let value = strict_json(raw.as_bytes()).unwrap();
        assert_eq!(
            String::from_utf8(canonical_json(&value).unwrap()).unwrap(),
            expected,
            "input {raw}"
        );
    }
    let nonfinite: Value = serde_json::from_str("1e400").unwrap();
    assert!(canonical_json(&nonfinite).is_err());
    let unordered = json!({"中文":"原页\r\n\t", "a":{"z":2,"a":1}});
    assert_eq!(
        String::from_utf8(canonical_json(&unordered).unwrap()).unwrap(),
        "{\"a\":{\"a\":1,\"z\":2},\"中文\":\"原页\\r\\n\\t\"}"
    );
}

fn selection(bundle: &str, ids: Value) -> Value {
    json!({"bundle":bundle,"asset_ids":ids,"purpose":"check the current decision","applicability":{"object_id":"synthetic","revision":"R2"}})
}

#[test]
fn no_selection_does_not_read_a_runtime_or_claim_a_semantic_review() {
    let report = oe_semantic::build_premises(Path::new("/does/not/exist"), &json!([])).unwrap();
    assert_eq!(report["status"], "not_provided");
    assert_eq!(report["selected_bytes"], 2);
    assert_eq!(
        report["snapshot_sha256"],
        "843df8215507c59607d5a35eb9d04567a7d7968a7924c30603ee2dcc983ed9eb"
    );
    assert_eq!(report["semantic_execution"], "not_run");
    assert_eq!(report["fact_admitted"], false);
}

#[test]
fn selected_native_assets_keep_exact_bytes_and_the_preexisting_snapshot_hashes() {
    // These expected hashes were produced by the existing Python adapter in
    // already archived synthetic development runs. No Python is executed here.
    let cases = [
        (
            json!([{"bundle":"engineering-judgment-intake","asset_ids":["pattern-catalog"],"purpose":"使用身份、主张、适用范围等既有结构理解当前问题","applicability":{"scope":"问题形成，未作物理结论"}}]),
            "809b325d36881cbfc4d366b8257bdf760775574cf12b54798f6c0960158638e0",
            5985,
        ),
        (
            json!([{"bundle":"engineering-judgment-intake","asset_ids":["shape-pattern-identity","findings-pattern-identity"],"purpose":"读取原生身份和版本审查定义以解释当前反馈","applicability":{"declared_revision":"R2","declared_subject":"assembly-A","status":"caller_assertion_only"}}]),
            "afeab668eb8118334696fd671f8edba47436bff135654b2eb77f2c4c6b97349f",
            3816,
        ),
    ];
    let bundle = load_bundle(&root(), "engineering-judgment-intake").unwrap();
    for (input, expected, bytes) in cases {
        let report = oe_semantic::build_premises(&root(), &input).unwrap();
        assert_eq!(report["snapshot_sha256"], expected);
        assert_eq!(report["selected_bytes"], bytes);
        assert_eq!(report["project_promotion"], "not_established");
        assert_eq!(
            report["selections"][0]["applicability_status"],
            "caller_assertion_not_established"
        );
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains(root().to_str().unwrap())
        );
        for asset in report["selections"][0]["assets"].as_array().unwrap() {
            let native = bundle.manifest["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["asset_id"] == asset["asset_id"])
                .unwrap();
            assert_eq!(
                asset["content"].as_str().unwrap().as_bytes(),
                bundle.payload[native["path"].as_str().unwrap()]
            );
            assert_eq!(
                digest(asset["content"].as_str().unwrap().as_bytes()),
                asset["sha256"]
            );
        }
    }
}

#[test]
fn premise_selection_is_bounded_strict_and_never_truncated() {
    let good = selection("engineering-judgment-intake", json!(["pattern-catalog"]));
    for kind in [
        "not_array",
        "too_many",
        "unknown",
        "duplicate",
        "asset_budget",
        "byte_budget",
        "extra",
        "missing",
        "bad_applicability",
        "control",
        "empty",
    ] {
        let mut item = good.clone();
        match kind {
            "unknown" => item["asset_ids"] = json!(["absent"]),
            "duplicate" => item["asset_ids"] = json!(["pattern-catalog", "pattern-catalog"]),
            "asset_budget" => {
                item["asset_ids"] = json!((0..33).map(|i| format!("id-{i}")).collect::<Vec<_>>())
            }
            "byte_budget" => item["purpose"] = "x".repeat(64 * 1024).into(),
            "extra" => item["status"] = "passed".into(),
            "missing" => {
                item.as_object_mut().unwrap().remove("purpose");
            }
            "bad_applicability" => item["applicability"] = json!([]),
            "control" => item["purpose"] = "bad\ntext".into(),
            "empty" => item["asset_ids"] = json!([]),
            _ => {}
        }
        let input = match kind {
            "not_array" => item,
            "too_many" => Value::Array(vec![item; 9]),
            _ => json!([item]),
        };
        assert!(
            oe_semantic::build_premises(&root(), &input).is_err(),
            "accepted {kind}"
        );
    }
}

#[test]
fn multiple_packages_and_repeated_selections_preserve_intent_and_order() {
    let input = json!([
        selection("engineering-judgment-intake", json!(["input-profiles"])),
        selection("engineering-evidence-methods", json!(["input-profiles"])),
        selection("engineering-judgment-intake", json!(["pattern-catalog"]))
    ]);
    let report = oe_semantic::build_premises(&root(), &input).unwrap();
    assert_eq!(report["asset_count"], 3);
    assert_eq!(
        report["selections"][0]["bundle"],
        "engineering-judgment-intake"
    );
    assert_eq!(
        report["selections"][1]["bundle"],
        "engineering-evidence-methods"
    );
    assert_eq!(
        report["selections"][2]["assets"][0]["asset_id"],
        "pattern-catalog"
    );
}

#[test]
fn unknown_unselected_bundle_is_not_loaded_and_package_conflicts_are_rejected() {
    let scratch = Scratch::new();
    let mut fixture = Synthetic::new();
    fixture.write(&scratch.0, "first");
    let lock_path = scratch.0.join("runtime/semantic-bundles.json");
    let mut lock = strict_json(&fs::read(&lock_path).unwrap()).unwrap();
    let mut second = fixture.clone();
    second.spec["package_version"] = "2".into();
    second.spec["path"] = "runtime/vendor/second.zip".into();
    second.manifest(|manifest| manifest["version"] = "2".into());
    let raw = second.bytes();
    fs::write(scratch.0.join("runtime/vendor/second.zip"), raw).unwrap();
    lock["bundles"]["second"] = second.spec;
    lock["bundles"]["never_requested"] = json!({"path":"missing.zip"});
    fs::write(&lock_path, serde_json::to_vec(&lock).unwrap()).unwrap();
    let first = selection("first", json!(["selected"]));
    assert!(oe_semantic::build_premises(&scratch.0, &json!([first.clone()])).is_ok());
    failure(
        oe_semantic::build_premises(
            &scratch.0,
            &json!([first, selection("second", json!(["selected"]))]),
        ),
        "conflicting versions or identities",
    );
}

#[test]
fn selected_text_is_exact_utf8_and_invalid_utf8_is_not_lossily_replaced() {
    for valid in [true, false] {
        let scratch = Scratch::new();
        let mut fixture = Synthetic::new();
        let raw = if valid {
            "  原文\r\n\t保留空白\n".as_bytes().to_vec()
        } else {
            vec![0xff, 0xfe]
        };
        let sha = digest(&raw);
        fixture
            .payload
            .insert("assets/selected.json".into(), raw.clone());
        fixture.manifest(|manifest| manifest["assets"][0]["sha256"] = sha.into());
        fixture.write(&scratch.0, "test");
        let report = oe_semantic::build_premises(
            &scratch.0,
            &json!([selection("test", json!(["selected"]))]),
        );
        if valid {
            assert_eq!(
                report.unwrap()["selections"][0]["assets"][0]["content"]
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                raw
            );
        } else {
            failure(report, "not valid UTF-8");
        }
    }
}

#[test]
fn applicability_storage_order_matches_python_canonical_roundtrip() {
    let mut input = selection("engineering-judgment-intake", json!(["pattern-catalog"]));
    input["applicability"] = strict_json(
        br#"{"subject":{"revision":"R2","object_id":"assembly-A"},"status":"caller_assertion_only"}"#,
    )
    .unwrap();
    let reversed = oe_semantic::build_premises(&root(), &json!([input.clone()])).unwrap();
    input["applicability"] = strict_json(
        br#"{"status":"caller_assertion_only","subject":{"object_id":"assembly-A","revision":"R2"}}"#,
    )
    .unwrap();
    let sorted = oe_semantic::build_premises(&root(), &json!([input])).unwrap();
    assert_eq!(
        serde_json::to_string(&reversed["selections"][0]["applicability"]).unwrap(),
        r#"{"status":"caller_assertion_only","subject":{"object_id":"assembly-A","revision":"R2"}}"#,
    );
    // Request identities use ordered serialization, not Value equality alone.
    assert_eq!(
        serde_json::to_vec(&reversed).unwrap(),
        serde_json::to_vec(&sorted).unwrap(),
    );
}

#[test]
fn applicability_key_order_does_not_change_snapshot_but_conditions_and_asset_order_do() {
    let mut one = selection(
        "engineering-judgment-intake",
        json!(["shape-pattern-identity", "findings-pattern-identity"]),
    );
    one["applicability"] = strict_json(br#"{"z":1e-7,"a":1844674407370955161600001}"#).unwrap();
    let mut two = one.clone();
    two["applicability"] =
        strict_json(br#"{"a":1844674407370955161600001,"z":0.0000001}"#).unwrap();
    let hash = |value: Value| {
        oe_semantic::build_premises(&root(), &json!([value])).unwrap()["snapshot_sha256"].clone()
    };
    assert_eq!(hash(one.clone()), hash(two.clone()));
    two["applicability"]["z"] = json!(0.1);
    assert_ne!(hash(one.clone()), hash(two));
    let mut reversed = one.clone();
    reversed["asset_ids"].as_array_mut().unwrap().reverse();
    assert_ne!(hash(one), hash(reversed));
}
