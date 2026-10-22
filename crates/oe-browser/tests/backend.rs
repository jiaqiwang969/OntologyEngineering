//! Current live admission; transport primitives are tested privately with synthetic fixtures.
use oe_browser::{
    BackendKind, BrowserTask, SnapshotSource,
    backend::{self, BackendConfig},
};
use oe_core::SkillRoot;
use serde_json::json;
#[test]
fn current_default_is_peekaboo_and_old_live_owners_reject_without_transport() {
    let root = SkillRoot::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap(),
    )
    .unwrap();
    let task=BrowserTask::parse(json!({"schema":oe_browser::task::TASK_SCHEMA,"goal":"Readonly","url":"https://www.misumi.com.cn/"})).unwrap();
    assert_eq!(task.backend, BackendKind::PeekabooBackground);
    let source = SnapshotSource {
        javascript: "1".into(),
        sha256: oe_core::sha256(b"1"),
    };
    for kind in [BackendKind::ChromeAppleEvents, BackendKind::BrowserHarness] {
        let mut t = task.clone();
        t.backend = kind;
        if kind == BackendKind::ChromeAppleEvents {
            t.tab_id = Some(71);
            t.keep_open = true;
            t.allowed_actions = vec![oe_browser::AllowedAction {
                kind: oe_browser::ActionKind::Click,
                label: "Read".into(),
            }];
        }
        assert!(
            backend::connect(
                &root,
                &t,
                &source,
                &BackendConfig::new("var/old-live-must-not-exist".into())
            )
            .is_err()
        );
    }
    assert!(
        backend::connect(
            &root,
            &task,
            &source,
            &BackendConfig::new("var/no-cdp-must-not-exist".into())
        )
        .is_err()
    );
    assert!(!root.path().join("var/no-cdp-must-not-exist").exists());
}

#[test]
fn explicit_cdp_endpoint_and_ax_window_scope_are_checked_without_live_probes() {
    for bad in [
        "https://127.0.0.1:9222/",
        "http://localhost:9222/",
        "http://0.0.0.0:9222/",
        "http://user:secret@127.0.0.1:9222/",
        "http://127.0.0.1:9222/profile",
        "http://127.0.0.1:9222/?token=x",
        "http://127.0.0.1:9222/#x",
    ] {
        assert!(backend::parse_existing_cdp_endpoint(bad).is_err());
    }
    assert!(backend::parse_existing_cdp_endpoint("http://127.0.0.1:9222").is_ok());
    let inventory = json!({"success":true,"data":{"targetApplication":{"bundleIdentifier":"com.google.Chrome"},"windows":[{"window_id":7,"layer":0,"title":"Actual normal browser window","isMinimized":false}]}});
    assert!(backend::select_ax_window(&inventory, 7).is_ok());
    for patch in [
        json!({"title":""}),
        json!({"layer":103}),
        json!({"isMinimized":true}),
        json!({"window_id":8}),
    ] {
        let mut v = inventory.clone();
        v["data"]["windows"][0]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(backend::select_ax_window(&v, 7).is_err());
    }
}
