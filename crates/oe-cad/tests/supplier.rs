use oe_cad::supplier::{archive, artifacts, session};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let skill = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        let p = skill
            .join("var/tests/unified-system/rust-rewrite-20260927/phase11-supplier-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        oe_core::create_new_private_dir(&p).unwrap();
        oe_core::write_new_bytes(
            &p.join("SKILL.md"),
            b"# isolated synthetic supplier fixture\n",
        )
        .unwrap();
        for (name, bytes) in oe_browser::source::SOURCE_FILES
            .iter()
            .chain(oe_cad::supplier::source::SOURCE_FILES)
        {
            let path = p.join(name);
            oe_core::ensure_directory(path.parent().unwrap()).unwrap();
            if path.exists() {
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    *bytes,
                    "shared source differs"
                );
            } else {
                oe_core::write_new_bytes(&path, bytes).unwrap();
            }
        }
        Self(p)
    }
    fn root(&self) -> SkillRoot {
        SkillRoot::new(&self.0).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn step(part: &str, schema: &str) -> Vec<u8> {
    format!("ISO-10303-21;\nHEADER;FILE_SCHEMA(('{schema}'));ENDSEC;DATA;#1=PRODUCT('{part}','','',());#2=SI_UNIT(.MILLI.,.METRE.);ENDSEC;END-ISO-10303-21;\n").into_bytes()
}
fn zip(rows: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in rows {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
#[test]
fn step_ap214_and_suffix_have_evidence_without_native_units_verdict() {
    let b = step("P1_1", "AUTOMOTIVE_DESIGN");
    let r = artifacts::inspect_download(
        "P1_20261001.zip",
        zip(&[("P1.stp", &b)]),
        "P1",
        "STEP AP214",
    )
    .unwrap();
    assert_eq!(r.receipt["files"][0]["product"], "P1_1");
    assert_eq!(r.receipt["files"][0]["units"], "unknown");
    assert_eq!(
        r.receipt["files"][0]["millimetre_unit_definition_present"],
        true
    );
    assert_eq!(r.receipt["engineering_acceptance"], "not_evaluated");
}

#[test]
fn step_comments_and_strings_do_not_forge_product_or_unit_evidence() {
    let text=b"ISO-10303-21;FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));/* PRODUCT('P1','','',()); */#1=PRODUCT('wrong','','',());#2=DESCRIPTION('PRODUCT(''P1'') SI_UNIT(.MILLI.,.METRE.)');END-ISO-10303-21;";
    assert!(artifacts::inspect_file("P1.stp", text, "P1", "STEP").is_err());
    let text=b"ISO-10303-21;FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));#1=PRODUCT('P1','','',());#2=DESCRIPTION('SI_UNIT(.MILLI.,.METRE.)');END-ISO-10303-21;";
    let r = artifacts::inspect_file("P1.stp", text, "P1", "STEP").unwrap();
    assert_eq!(r["millimetre_unit_definition_present"], false);
}

#[test]
fn multiple_real_step_products_are_rejected() {
    let text=b"ISO-10303-21;FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));#1=PRODUCT('P1','','',());#2=PRODUCT('other','','',());END-ISO-10303-21;";
    assert!(artifacts::inspect_file("P1.stp", text, "P1", "STEP").is_err());
}

#[test]
fn dxf_dwg_pdf_iges_and_native_families_keep_distinct_checks() {
    let dxf = b"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nEOF\n";
    assert_eq!(
        artifacts::inspect_file("P1.dxf", dxf, "P1", "DXF").unwrap()["units"],
        "mm"
    );
    assert!(artifacts::inspect_file("P1.dwg", b"AC1032bytes", "P1", "DWG").is_ok());
    assert!(artifacts::inspect_file("P1.pdf", b"%PDF-1.7\n%%EOF", "P1", "PDF").is_ok());
    let iges = b"SGDPT"
        .iter()
        .map(|c| format!("{}{}1\n", " ".repeat(72), *c as char))
        .collect::<String>();
    assert!(artifacts::inspect_file("P1.igs", iges.as_bytes(), "P1", "IGES").is_ok());
    for (name, label) in [
        ("P1.x_t", "PARASOLID"),
        ("P1.sat", "ACIS"),
        ("P1.prt", "NX 2306"),
    ] {
        let r = artifacts::inspect_file(name, b"synthetic native bytes", "P1", label).unwrap();
        assert_eq!(r["format_check"], "native_reader_required");
        assert_eq!(r["units"], "unknown");
    }
}

#[test]
fn archive_identity_paths_duplicates_html_and_wrong_format_fail() {
    let b = step("P1", "CONFIG_CONTROL_DESIGN");
    for name in [
        "../P1.stp",
        "dir/P1.stp",
        "P1\\evil.stp",
        "OTHER.stp",
        "P1:stream.stp",
    ] {
        assert!(
            artifacts::inspect_download("P1.zip", zip(&[(name, &b)]), "P1", "STEP").is_err(),
            "{name}"
        );
    }
    assert!(
        artifacts::inspect_download(
            "P1.zip",
            zip(&[("P1.stp", &b), ("p1.STP", &b)]),
            "P1",
            "STEP"
        )
        .is_err()
    );
    assert!(
        artifacts::inspect_download("P1.pdf", b"<html>sign in</html>".to_vec(), "P1", "PDF")
            .is_err()
    );
    assert!(artifacts::inspect_download("P1.prt", b, "P1", "STEP").is_err());
    assert!(artifacts::inspect_download("OTHER.prt", b"native".to_vec(), "P1", "NX").is_err());
}

#[test]
fn ap203_comparison_is_explicit_and_does_not_reintroduce_unit_claim() {
    let b = step("P1", "CONFIG_CONTROL_DESIGN");
    let r = artifacts::inspect_ap203_archive(zip(&[("P1.stp", &b)]), "P1").unwrap();
    assert_eq!(r.receipt["files"][0]["units"], "unknown");
    assert!(
        artifacts::inspect_ap203_archive(
            zip(&[("P1.stp", &step("P1", "AUTOMOTIVE_DESIGN"))]),
            "P1"
        )
        .is_err()
    );
    assert!(artifacts::inspect_ap203_archive(zip(&[("P1_1.stp", &b)]), "P1").is_err());
    assert!(
        artifacts::inspect_ap203_archive(zip(&[("P1.stp", &b), ("empty.txt", b"")]), "P1").is_err()
    );
}

#[test]
fn stable_new_file_is_required_and_original_download_is_unchanged() {
    let f = Fixture::new();
    let downloads = f.0.join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    let old = downloads.join("P1.prt");
    std::fs::write(&old, b"old bytes").unwrap();
    let mut watch = artifacts::FreshDownload::new(&downloads, "P1", "NX").unwrap();
    assert!(watch.poll().unwrap().is_none());
    let path = downloads.join("P1 (1).prt");
    std::fs::write(&path, b"new native bytes").unwrap();
    assert!(watch.poll().unwrap().is_none());
    let r = watch.poll().unwrap().unwrap();
    assert_eq!(r.files["P1 (1).prt"], b"new native bytes");
    assert_eq!(std::fs::read(old).unwrap(), b"old bytes");
    assert_eq!(std::fs::read(path).unwrap(), b"new native bytes");
}

#[test]
fn partial_and_multiple_downloads_cannot_satisfy_acquisition() {
    let f = Fixture::new();
    let d = f.0.join("downloads");
    std::fs::create_dir(&d).unwrap();
    let mut w = artifacts::FreshDownload::new(&d, "P1", "NX").unwrap();
    std::fs::write(d.join("P1.prt"), b"part").unwrap();
    std::fs::write(d.join("P1.prt.crdownload"), b"incomplete").unwrap();
    assert!(w.poll().unwrap().is_none());
    assert!(w.poll().unwrap().is_none());
    std::fs::remove_file(d.join("P1.prt.crdownload")).unwrap();
    std::fs::write(d.join("P1 (1).prt"), b"second").unwrap();
    assert!(w.poll().is_err());
}

#[cfg(unix)]
#[test]
fn symlink_download_cannot_be_reinterpreted_as_new_file() {
    let f = Fixture::new();
    let d = f.0.join("downloads");
    std::fs::create_dir(&d).unwrap();
    let mut w = artifacts::FreshDownload::new(&d, "P1", "NX").unwrap();
    std::fs::write(f.0.join("native.prt"), b"native").unwrap();
    std::os::unix::fs::symlink(f.0.join("native.prt"), d.join("P1.prt")).unwrap();
    assert!(w.poll().is_err());
}

fn identity(email: &str) -> Value {
    json!({"schema":session::IDENTITY_SCHEMA,"match_fields":{"email":email}})
}
fn account(email: &str) -> Value {
    json!({"url":session::ACCOUNT_URL,"title":"个人信息管理","account_fields":{"email":email,"username":"synthetic"},"login_form_visible":false})
}

#[test]
fn account_exact_fields_and_unicode_casefold_are_required_without_private_values_in_result() {
    let email = ["strasse", "example.invalid"].join("@");
    let other = ["STRAẞE", "EXAMPLE.INVALID"].join("@");
    let r = session::classify(&account(&other), &identity(&email)).unwrap();
    assert_eq!(r["account_verified"], true);
    assert_eq!(r["password_reauthentication_verified"], false);
    assert!(!r.to_string().contains(&email));
    assert!(!r.to_string().contains(&other));
    let wrong = ["other", "example.invalid"].join("@");
    assert_eq!(
        session::classify(&account(&wrong), &identity(&email)).unwrap()["status"],
        "different_account"
    );
}

#[test]
fn same_text_on_wrong_origin_or_other_page_never_authenticates() {
    let email = ["person", "example.invalid"].join("@");
    let mut ui = account(&email);
    ui["url"] = "https://www.misumi.com.cn/".into();
    assert_eq!(
        session::classify(&ui, &identity(&email)).unwrap()["account_verified"],
        false
    );
    ui["url"] = "https://untrusted.invalid/my/user/manage/".into();
    assert_eq!(
        session::classify(&ui, &identity(&email)).unwrap()["status"],
        "unexpected_origin"
    );
    ui = account(&email);
    ui["account_fields"]["email"] = Value::Null;
    assert_eq!(
        session::classify(&ui, &identity(&email)).unwrap()["status"],
        "account_state_unresolved"
    );
}

#[test]
fn login_challenge_and_access_denial_preserve_distinct_recovery() {
    let email = ["person", "example.invalid"].join("@");
    let mut ui = account(&email);
    ui["url"] = "https://www.misumi.com.cn/mydesk2/s/login/".into();
    ui["login_form_visible"] = true.into();
    ui["autofill_ready"] = true.into();
    assert_eq!(
        session::classify(&ui, &identity(&email)).unwrap()["status"],
        "login_required"
    );
    ui["challenge_visible"] = true.into();
    assert_eq!(
        session::classify(&ui, &identity(&email)).unwrap()["status"],
        "verification_challenge"
    );
    ui["access_denied"] = true.into();
    assert_eq!(
        session::classify(&ui, &identity(&email)).unwrap()["status"],
        "site_access_denied"
    );
}

#[cfg(unix)]
#[test]
fn identity_input_must_be_private_and_regular() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let path = f.0.join("identity.json");
    let email = ["person", "example.invalid"].join("@");
    oe_core::write_new(&path, &identity(&email)).unwrap();
    let before = std::fs::read(&path).unwrap();
    let (_, r) = session::read_identity(&path).unwrap();
    assert_eq!(r.sha256, oe_core::sha256(&before));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(session::read_identity(&path).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn observed_public_urls_reject_credentials_encoded_traversal_and_other_hosts() {
    assert!(
        archive::validate_static_url("https://www.misumi.com.cn/linked/material/drawing.zip")
            .is_ok()
    );
    for s in [
        "https://wrong.invalid/linked/material/drawing.zip",
        "http://www.misumi.com.cn/linked/material/drawing.zip",
        "https://user@www.misumi.com.cn/linked/material/drawing.zip",
        "https://www.misumi.com.cn/linked/material/%2e%2e/drawing.zip",
        "https://www.misumi.com.cn/linked/material/drawing.zip?x=1",
    ] {
        assert!(archive::validate_static_url(s).is_err(), "{s}");
    }
    assert!(
        archive::validate_ap203_url(
            "https://rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/observed/STEP_AP203/P1_STEP.zip",
            "P1"
        )
        .is_ok()
    );
    assert!(
        archive::validate_ap203_url(
            "https://rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/observed/STEP_AP214/P1_STEP.zip",
            "P1"
        )
        .is_err()
    );
}

#[test]
fn static_archive_checks_all_members_and_windows_collisions() {
    let dxf = b"SECTION HEADER EOF";
    let r = archive::inspect_static_archive(&zip(&[
        ("folder/P1.dxf", dxf),
        ("README.txt", b"synthetic"),
    ]))
    .unwrap();
    assert_eq!(r.len(), 2);
    for rows in [
        vec![("../P1.dxf", dxf.as_slice())],
        vec![("P1.dxf", dxf.as_slice()), ("p1.DXF", dxf.as_slice())],
        vec![("P1.stp", dxf.as_slice())],
        vec![("P1.pdf", b"<html>login</html>".as_slice())],
    ] {
        assert!(archive::inspect_static_archive(&zip(&rows)).is_err());
    }
}
