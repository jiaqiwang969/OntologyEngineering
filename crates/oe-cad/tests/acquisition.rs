//! Independent private account and source-binding counterexamples.
use anyhow::Result;
use oe_browser::{
    Action, BackendKind, Browser, DispatchState, ExecutionReceipt, Observation, SessionIdentity,
};
use oe_cad::supplier::{session, source};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: SkillRoot,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let skill = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        let path = skill
            .join("var/tests/unified-system/rust-rewrite-20260927/phase11-acquisition-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::SeqCst)
            ));
        oe_core::create_new_private_dir(&path).unwrap();
        oe_core::write_new_bytes(
            &path.join("SKILL.md"),
            b"# isolated supplier workflow fixture\n",
        )
        .unwrap();
        for (name, bytes) in oe_browser::source::SOURCE_FILES
            .iter()
            .chain(source::SOURCE_FILES)
        {
            let p = path.join(name);
            oe_core::ensure_directory(p.parent().unwrap()).unwrap();
            if p.exists() {
                assert_eq!(std::fs::read(&p).unwrap(), *bytes, "shared source differs");
            } else {
                oe_core::write_new_bytes(&p, bytes).unwrap();
            }
        }
        oe_core::create_new_private_dir(&path.join("downloads")).unwrap();
        Self {
            root: SkillRoot::new(&path).unwrap(),
            path,
        }
    }

    fn preflight(&self, browser: &mut FixtureBrowser) -> Value {
        let path = self.path.join("identity.json");
        oe_core::write_new(&path,&json!({"schema":session::IDENTITY_SCHEMA,"match_fields":{"username":"synthetic-test-account"}})).unwrap();
        session::check(
            &self.root,
            browser,
            &path,
            PathBuf::from("var/account").as_path(),
            &session::SessionOptions::default(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.path).unwrap();
        }
    }
}
fn part_url() -> String {
    "https://www.misumi.com.cn/vona2/detail/fixture/?HissuCode=P1".into()
}
struct FixtureBrowser {
    session: SessionIdentity,
    download: PathBuf,
    selected: bool,
    generations: u64,
    actions: u64,
    unknown: bool,
    fail_post_generation: bool,
    document: bool,
    navigations: u64,
    prior_link: bool,
    historical: bool,
    popup: bool,
    no_cad: bool,
    expose_identity: Option<PathBuf>,
}
impl FixtureBrowser {
    fn new(f: &Fixture) -> Self {
        Self {
            session: SessionIdentity {
                backend: BackendKind::PeekabooBackground,
                target: "71".into(),
                session: "isolated-fixture-process-1".into(),
                origin: session::ORIGIN.into(),
                owns_target: false,
            },
            download: f.path.join("downloads/P1.dxf"),
            selected: false,
            generations: 0,
            actions: 0,
            unknown: false,
            fail_post_generation: false,
            document: false,
            navigations: 0,
            prior_link: false,
            historical: false,
            popup: false,
            no_cad: false,
            expose_identity: None,
        }
    }
    fn ui(&self) -> Value {
        let mut value = json!({"source_url":part_url(),"part":"P1","configured":false,"displayed_part":"P1","configuration_marker_present":false,"exact_part_token_present":false,
        "format_options":[{"label":"DXF","value":"dxf-observed","disabled":false}],"format_value":if self.selected {"dxf-observed"} else {"other"},"dialog_text":"生成数据","documents":if self.document {json!([{"label":"安装说明","url":"https://www.misumi.com.cn/linked/material/observed.pdf"}])} else {json!([])},"specification_excerpt":null,"observed_download_url":if self.prior_link {json!("https://rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/old/DXF/P1_DXF.zip")}else{Value::Null}});
        if self.historical {
            value["popup_visible"] = self.popup.into();
            value["format_value"] = if !self.popup {
                Value::Null
            } else if self.selected {
                json!("STEP_AP203")
            } else {
                json!("STEP_AP214")
            };
            value["format_options"] =
                json!([{"label":"STEP AP203","value":"STEP_AP203","disabled":false}]);
            if self.no_cad {
                value["dialog_text"] = "没有您指定条件的CAD".into();
            }
        }
        value
    }
}
impl Browser for FixtureBrowser {
    fn identity(&self) -> &SessionIdentity {
        &self.session
    }
    fn observe(&mut self, _: bool) -> Result<Observation> {
        let action = if self.historical && !self.popup {
            json!({"id":"open-cad","kind":"click","label":"CAD下载","node":3})
        } else if self.historical && !self.selected {
            json!({"id":"select-ap203","kind":"select","label":"格式 → STEP AP203","value":"STEP_AP203","node":1})
        } else if self.selected {
            json!({"id":"generate","kind":"click","label":"数据生成","node":2})
        } else {
            json!({"id":"select-dxf","kind":"select","label":"格式 → DXF","value":"dxf-observed","node":1})
        };
        Observation::new(
            self.session.clone(),
            json!({"url":part_url(),"title":"P1 product","text":"P1 configured product","marker":[self.popup,self.selected],"page_key":["P1"],"guards":{"1":["format"],"2":["generate"],"3":["open"]},"actions":[action,{"id":"wait","kind":"wait","label":"Wait"}]}),
        )
    }
    fn fresh(&mut self, page: &Observation, action: Option<&Action>) -> Result<bool> {
        page.validate()?;
        if let Some(a) = action {
            a.validate_against(page)?;
        }
        Ok(page.session == self.session)
    }
    fn act(
        &mut self,
        page: &Observation,
        action: &Action,
        _: Option<&str>,
    ) -> Result<ExecutionReceipt> {
        action.validate_against(page)?;
        self.actions += 1;
        let state = if action.label == "CAD下载" {
            self.popup = true;
            DispatchState::Confirmed
        } else if action.kind == oe_browser::ActionKind::Select {
            self.selected = true;
            DispatchState::Confirmed
        } else if action.kind == oe_browser::ActionKind::Click {
            self.generations += 1;
            if self.unknown {
                DispatchState::OutcomeUnknown
            } else {
                if self.historical {
                    use std::io::Write;
                    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
                    archive.start_file("P1.stp", zip::write::SimpleFileOptions::default())?;
                    archive.write_all(b"ISO-10303-21;HEADER;FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));ENDSEC;DATA;#1=PRODUCT('P1','','',());#2=SI_UNIT(.MILLI.,.METRE.);ENDSEC;END-ISO-10303-21;")?;
                    std::fs::write(
                        self.download.parent().unwrap().join("P1_STEP.zip"),
                        archive.finish()?.into_inner(),
                    )?;
                } else {
                    std::fs::write(
                        &self.download,
                        b"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nEOF\n",
                    )?;
                }
                DispatchState::Confirmed
            }
        } else {
            DispatchState::Confirmed
        };
        Ok(ExecutionReceipt {
            action_id: action.id.clone(),
            state,
            execution_kind: "synthetic_fixture".into(),
            readback: None,
            diagnostic_code: None,
        })
    }
    fn supplier_observation(&mut self) -> Result<Value> {
        if self.fail_post_generation && self.generations > 0 {
            anyhow::bail!("synthetic_post_generation_readback_failed");
        }
        Ok(self.ui())
    }
    fn account_observation(&mut self) -> Result<Value> {
        if let Some(path) = self.expose_identity.take() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))?;
            }
            #[cfg(windows)]
            {
                let result = std::process::Command::new("icacls.exe")
                    .arg(path)
                    .args(["/grant", "*S-1-1-0:R"])
                    .output()?;
                anyhow::ensure!(result.status.success(), "synthetic_acl_mutation_failed");
            }
        }
        Ok(
            json!({"url":session::ACCOUNT_URL,"title":"个人信息管理","account_fields":{"username":"synthetic-test-account"}}),
        )
    }
    fn navigate_account(&mut self) -> Result<ExecutionReceipt> {
        self.navigations += 1;
        Ok(ExecutionReceipt {
            action_id: "navigate_account".into(),
            state: DispatchState::Confirmed,
            execution_kind: "fixture_navigation".into(),
            readback: None,
            diagnostic_code: None,
        })
    }
    fn close(&mut self) -> Result<Value> {
        Ok(json!({"status":"not_closed","owns_target":false}))
    }
}
#[test]
fn account_receipt_rejects_another_tab_process_expiry_or_changed_source() {
    let f = Fixture::new();
    let mut b = FixtureBrowser::new(&f);
    f.preflight(&mut b);
    let path = PathBuf::from("var/account/result.json");
    assert!(session::validate_receipt(&f.root, &path, &b.session, 600).is_ok());
    let mut other = b.session.clone();
    other.target = "72".into();
    assert!(session::validate_receipt(&f.root, &path, &other, 600).is_err());
    other = b.session.clone();
    other.session = "restarted-browser".into();
    assert!(session::validate_receipt(&f.root, &path, &other, 600).is_err());
    let file = f.path.join(&path);
    let mut value = oe_core::json::strict_json(&std::fs::read(&file).unwrap()).unwrap();
    value["checked_at_unix_ms"] = 1.into();
    std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(session::validate_receipt(&f.root, &path, &b.session, 600).is_err());
    value["checked_at_unix_ms"] = session::unix_ms().unwrap().into();
    std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
    std::fs::write(
        f.path.join("crates/oe-cad/src/supplier/session.rs"),
        b"changed",
    )
    .unwrap();
    assert!(session::validate_receipt(&f.root, &path, &b.session, 600).is_err());
}
#[test]
fn account_navigation_is_once_and_persisted_fields_are_private_hashes() {
    let f = Fixture::new();
    let mut b = FixtureBrowser::new(&f);
    let path = f.path.join("identity.json");
    oe_core::write_new(&path,&json!({"schema":session::IDENTITY_SCHEMA,"match_fields":{"username":"synthetic-test-account"}})).unwrap();
    let r = session::check(
        &f.root,
        &mut b,
        &path,
        PathBuf::from("var/account").as_path(),
        &session::SessionOptions {
            navigate_account: true,
            max_seconds: 1,
        },
    )
    .unwrap();
    assert_eq!(b.navigations, 1);
    assert_eq!(r["account_verified"], true);
    assert!(!r.to_string().contains("synthetic-test-account"));
    let raw = std::fs::read_to_string(f.path.join("var/account/observation-0001.json")).unwrap();
    assert!(!raw.contains("synthetic-test-account"));
    assert!(raw.contains("observation_sha256"));
}
#[test]
fn session_rejects_permission_change_during_matching_account_observation() {
    let f = Fixture::new();
    let mut browser = FixtureBrowser::new(&f);
    browser.expose_identity = Some(f.path.join("identity.json"));
    let result = f.preflight(&mut browser);
    assert_eq!(result["account_verified"], false);
    assert_eq!(result["status"], "account_check_unconfirmed");
    assert_eq!(
        result["error_code"],
        "private_identity_file_permissions_required"
    );
    assert_eq!(result["credential_values_recorded"], false);
}
#[test]
fn receipt_rejects_public_permissions_despite_valid_account_binding() {
    let f = Fixture::new();
    let mut browser = FixtureBrowser::new(&f);
    assert_eq!(f.preflight(&mut browser)["account_verified"], true);
    let path = f.path.join("var/account/result.json");
    session::validate_receipt(&f.root, &path, &browser.session, 600).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    #[cfg(windows)]
    {
        let result = std::process::Command::new("icacls.exe")
            .arg(&path)
            .args(["/grant", "*S-1-1-0:R"])
            .output()
            .unwrap();
        assert!(result.status.success());
    }
    assert!(session::validate_receipt(&f.root, &path, &browser.session, 600).is_err());
}
