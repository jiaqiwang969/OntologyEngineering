#[path = "support/incad_cad_fixture.rs"]
mod fixture;
use fixture::*;
use oe_knowledge::incad::{cad, verify};
use serde_json::{Value, json};

#[test]
fn original_archives_and_members_are_rechecked_without_native_adoption() {
    let t = tempfile::tempdir().unwrap();
    let f = CadFixture::new(t.path());
    let input = verify(&f.source.request).unwrap();
    let selected = cad::verify(&input, &["ap203".into()]).unwrap();
    assert_eq!(selected.files().len(), 3);
    for original in selected.files() {
        assert_eq!(
            std::fs::read(f.folder().join(&original.relative_path)).unwrap(),
            original.bytes
        );
    }
    assert_eq!(
        selected.audit()["selections"][0]["supplier_label"],
        "STEP(AP214)"
    );
    assert_eq!(selected.audit()["reference_exposure"], "development_replay");
    assert_eq!(selected.audit()["native_execution"], "not_run");
    assert_eq!(selected.audit()["adopted"], false);
    std::fs::write(
        f.folder().join("cad/ap203/files/model/part.step"),
        b"changed",
    )
    .unwrap();
    assert!(selected.ensure_current().is_err());
    assert!(cad::verify(&input, &["ap203".into()]).is_err());
}
#[test]
fn matching_rebound_archive_hash_cannot_bypass_crc_or_full_member_inventory() {
    for defect in ["crc", "missing_record", "extra_record"] {
        let t = tempfile::tempdir().unwrap();
        let mut f = CadFixture::new(t.path());
        let p = f.folder().join("case.json");
        let mut case: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        match defect {
            "crc" => {
                let zp = f.folder().join("cad/ap203/original.zip");
                let mut bytes = std::fs::read(&zp).unwrap();
                let position = bytes.windows(9).position(|b| b == b"SYNTHETIC").unwrap();
                bytes[position] = b'X';
                std::fs::write(&zp, &bytes).unwrap();
                case["cad_downloads"][0]["original_sha256"] = json!(oe_core::sha256(&bytes));
            }
            "missing_record" => {
                case["cad_downloads"][0]["members"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            "extra_record" => {
                let mut r = case["cad_downloads"][0]["members"][0].clone();
                r["archive_name"] = json!("absent.step");
                r["path"] = json!("cad/ap203/files/absent.step");
                case["cad_downloads"][0]["members"]
                    .as_array_mut()
                    .unwrap()
                    .push(r);
            }
            _ => unreachable!(),
        }
        save(&p, &case);
        f.rebind_sources();
        let input = verify(&f.source.request).unwrap();
        assert!(cad::verify(&input, &["ap203".into()]).is_err(), "{defect}");
    }
}
#[test]
fn format_duplicates_paths_and_missing_originals_are_refused() {
    let t = tempfile::tempdir().unwrap();
    let f = CadFixture::new(t.path());
    let input = verify(&f.source.request).unwrap();
    for formats in [
        vec![],
        vec!["ap203".into(), "ap203".into()],
        vec!["../ap203".into()],
        vec!["unknown".into()],
    ] {
        assert!(cad::verify(&input, &formats).is_err());
    }
    std::fs::remove_file(f.folder().join("cad/ap203/original.zip")).unwrap();
    assert!(cad::verify(&input, &["ap203".into()]).is_err());
    let t = tempfile::tempdir().unwrap();
    let mut f = CadFixture::new(t.path());
    let p = f.folder().join("case.json");
    let mut c: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    c["cad_downloads"][0]["members"][0]["archive_name"] = json!("../outside");
    save(&p, &c);
    f.rebind_sources();
    let input = verify(&f.source.request).unwrap();
    assert!(cad::verify(&input, &["ap203".into()]).is_err());
}
