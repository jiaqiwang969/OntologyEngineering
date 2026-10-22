//! Current Peekaboo protocol tested with synthetic owned Unix servers. No real browser.
#![cfg(target_os = "macos")]
use crate::{Action, BrowserTask, DispatchState, SnapshotSource, backend::BackendConfig};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{PermissionsExt, symlink},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case {
    path: PathBuf,
}
impl Case {
    fn new() -> Self {
        let path = Path::new("/tmp").canonicalize().unwrap().join(format!(
            "oe-pb-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "Synthetic transport only\n").unwrap();
        for (relative, bytes) in crate::source::SOURCE_FILES
            .iter()
            .filter(|(p, _)| p.starts_with("runtime/peekaboo/"))
        {
            let p = path.join(relative);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, bytes).unwrap();
        }
        Self { path }
    }
    fn root(&self) -> SkillRoot {
        SkillRoot::new(&self.path).unwrap()
    }
    fn config(&self, name: &str) -> BackendConfig {
        let mut c = BackendConfig::new(PathBuf::from(format!("var/{name}")));
        c.peekaboo_socket = Some(self.path.join("s"));
        c.cdp_endpoint = Some("http://127.0.0.1:9222".into());
        c.timeout = Duration::from_millis(700);
        c
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !thread::panicking() {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}
fn task() -> BrowserTask {
    BrowserTask::parse(json!({"schema":crate::task::TASK_SCHEMA,"goal":"Inspect fixture","url":"https://www.misumi.com.cn/example?HissuCode=PART-A","text_values":{"Search":"PART-A"}})).unwrap()
}
fn source() -> SnapshotSource {
    let javascript = "(()=>{return {snapshot_fixture:true}})()".to_owned();
    SnapshotSource {
        sha256: oe_core::sha256(javascript.as_bytes()),
        javascript,
    }
}
fn page() -> Value {
    json!({"url":task().url,"title":"Fixture","text":"PART-A fixture","page_key":[1,task().url],"marker":[1,task().url,"fixture"],"guards":{"1":[1,"Read data"],"2":[2,"Search"],"3":[3,"Format"]},"actions":[{"id":"e1","node":1,"kind":"click","label":"Read data","rect":{"x":1,"y":1,"w":10,"h":10}},{"id":"e2","node":2,"kind":"fill","label":"Search"},{"id":"e3","node":3,"kind":"select","label":"Format → STEP","value":"STEP"},{"id":"wait","kind":"wait","label":"Wait"}]})
}
fn packet(result: Value) -> Value {
    let lock: Value = serde_json::from_slice(include_bytes!(
        "../../../../runtime/peekaboo/provider-lock.json"
    ))
    .unwrap();
    json!({"schema":"oev2.peekaboo-bridge/1","success":true,"status":"ready","worker_session":"fixture-worker","worker_pid":std::process::id(),"chrome_pid":std::process::id(),"chrome_pid_before":std::process::id(),"chrome_pid_after":std::process::id(),
 "bridge_sha256":oe_core::sha256(include_bytes!("../../../../runtime/peekaboo/bridge.mjs")),"focus_hook_sha256":oe_core::sha256(include_bytes!("../../../../runtime/peekaboo/focus-policy.mjs")),
 "peekaboo_binary_sha256":lock["peekaboo_binary_sha256"],"chrome_devtools_source_sha256":lock["chrome_devtools_source_sha256"],"provider_pin_applied":true,"cdp_endpoint_sha256":oe_core::sha256(b"http://127.0.0.1:9222"),
 "chrome_launches":0,"settings_changes":0,"focus_actions":0,"storage_reads":0,"request_retries":0,"focus_before":{"bundle":"fixture.application","pid":123},"focus_after":{"bundle":"fixture.application","pid":123},"active_application_unchanged":true,"target_after":{"url":task().url,"origin":"https://www.misumi.com.cn","page_has_focus":false},"result":result})
}
fn js(value: Value) -> Value {
    packet(json!({"value":serde_json::to_string(&value).unwrap()}))
}
fn reply(r: &Value) -> Value {
    match r["action"].as_str().unwrap_or("") {
        "check" => packet(json!({"connection_verified":true,"real_operation":"list_pages"})),
        "open" => packet(
            json!({"page_id":7,"owns_target":true,"prior_page_ids":[],"selected_created_page_id":7}),
        ),
        "bind" => packet(json!({"page_id":r["page_id"],"owns_target":false})),
        "close" => packet(json!({"closed":true})),
        "screenshot" => packet(json!({"data":"Zml4dHVyZQ=="})),
        "navigate" => packet(json!({"navigated":true})),
        "target_state" => packet(
            json!({"url":task().url,"origin":"https://www.misumi.com.cn","page_has_focus":false}),
        ),
        "evaluate" => {
            let e = r["expression"].as_str().unwrap();
            if e.contains("return c?[c.pageKey()") {
                let n = if e.contains("nodes.get(2)") {
                    "2"
                } else if e.contains("nodes.get(3)") {
                    "3"
                } else {
                    "1"
                };
                js(json!([page()["page_key"], page()["guards"][n]]))
            } else if e.contains("p?.marker") {
                js(page()["marker"].clone())
            } else if e.contains("e.value=a.value") {
                // DOM program contains every branch; determine this request's selected action from its literal.
                let id = if e.contains("\"id\":\"e1\"") {
                    "e1"
                } else if e.contains("\"id\":\"e2\"") {
                    "e2"
                } else {
                    "e3"
                };
                js(
                    json!({"executed":id,"readback":if id=="e2"{json!("PART-A")}else{json!("STEP")}}),
                )
            } else if e.contains("snapshot_fixture") {
                js(page())
            } else if e.contains("account_fields") {
                js(
                    json!({"account_fields":{"username":"Fixture Account Alpha","email":"synthetic"}}),
                )
            } else {
                js(json!("https://www.misumi.com.cn"))
            }
        }
        _ => panic!("unrecognized transport operation"),
    }
}
fn frame(v: Value) -> Vec<u8> {
    let mut b = serde_json::to_vec(&v).unwrap();
    b.push(b'\n');
    b
}
struct Server {
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<Value>>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(c: &Case, mut handler: impl FnMut(&Value) -> Vec<u8> + Send + 'static) -> Self {
        let l = UnixListener::bind(c.path.join("s")).unwrap();
        fs::set_permissions(c.path.join("s"), fs::Permissions::from_mode(0o600)).unwrap();
        l.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(vec![]));
        let done = stop.clone();
        let log = requests.clone();
        let t = thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                match l.accept() {
                    Ok((mut s, _)) => {
                        s.set_nonblocking(false).unwrap();
                        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let mut line = String::new();
                        if BufReader::new(&mut s).read_line(&mut line).is_err() || line.is_empty() {
                            continue;
                        }
                        let r: Value = serde_json::from_str(&line).unwrap();
                        log.lock().unwrap().push(r.clone());
                        let b = handler(&r);
                        let _ = s.write_all(&b);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            stop,
            requests,
            thread: Some(t),
        }
    }
    fn normal(c: &Case) -> Self {
        Self::new(c, |r| frame(reply(r)))
    }
    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
    fn inputs(&self) -> Vec<Value> {
        self.requests()
            .into_iter()
            .filter(|r| {
                r["action"] == "evaluate"
                    && r["expression"]
                        .as_str()
                        .is_some_and(|e| e.contains("e.value=a.value"))
            })
            .collect()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}
#[test]
fn stale_guards_and_forged_actions_fail_before_dispatch() {
    let c = Case::new();
    let s = Server::normal(&c);
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    let p = b.observe(false).unwrap();
    let mut a = Action::from_observation(&p, "e1").unwrap();
    a.label = "forged".into();
    assert!(b.act(&p, &a, None).is_err());
    let mut forged = p.clone();
    forged.page["guards"]["1"] = json!(["forged"]);
    let forged = crate::Observation::new(forged.session, forged.page).unwrap();
    let a = Action::from_observation(&forged, "e1").unwrap();
    assert!(b.act(&forged, &a, None).is_err());
    assert!(s.inputs().is_empty());
    b.close().unwrap();
}
#[test]
fn dom_click_fill_select_use_no_global_input_and_observation_is_consumed() {
    let c = Case::new();
    let s = Server::normal(&c);
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    for (id, text) in [("e1", None), ("e2", Some("PART-A")), ("e3", None)] {
        let p = b.observe(false).unwrap();
        let a = Action::from_observation(&p, id).unwrap();
        assert_eq!(b.act(&p, &a, text).unwrap().state, DispatchState::Confirmed);
        assert!(b.act(&p, &a, text).is_err());
    }
    assert_eq!(s.inputs().len(), 3);
    assert!(s.requests().iter().all(|r| r.get("method").is_none()));
    b.close().unwrap();
}
#[test]
fn changed_page_guard_refuses_before_dom_mutation() {
    let c = Case::new();
    let s = Server::new(&c, |r| {
        if r["expression"]
            .as_str()
            .is_some_and(|e| e.contains("return c?[c.pageKey()"))
        {
            frame(js(json!(["different document"])))
        } else {
            frame(reply(r))
        }
    });
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    let p = b.observe(false).unwrap();
    let a = Action::from_observation(&p, "e1").unwrap();
    assert!(b.act(&p, &a, None).is_err());
    assert!(s.inputs().is_empty());
    b.close().unwrap();
}
#[test]
fn input_failure_is_outcome_unknown_and_is_never_replayed() {
    let c = Case::new();
    let s = Server::new(&c, |r| {
        if r["expression"]
            .as_str()
            .is_some_and(|e| e.contains("e.value=a.value"))
        {
            let mut p = reply(r);
            p["success"] = false.into();
            frame(p)
        } else {
            frame(reply(r))
        }
    });
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    let p = b.observe(false).unwrap();
    let a = Action::from_observation(&p, "e1").unwrap();
    assert_eq!(
        b.act(&p, &a, None).unwrap().state,
        DispatchState::OutcomeUnknown
    );
    assert!(b.act(&p, &a, None).is_err());
    assert_eq!(s.inputs().len(), 1);
    b.close().unwrap();
}
#[test]
fn atomic_dom_stale_unsupported_and_unknown_results_are_distinct() {
    for (i, outcome, state) in [
        (0, json!({"stale":true}), DispatchState::NotDispatched),
        (1, json!({"unsupported":true}), DispatchState::NotDispatched),
        (
            2,
            json!({"outcome_unknown":true}),
            DispatchState::OutcomeUnknown,
        ),
    ] {
        let c = Case::new();
        let s = Server::new(&c, move |r| {
            if r["expression"]
                .as_str()
                .is_some_and(|e| e.contains("e.value=a.value"))
            {
                frame(js(outcome.clone()))
            } else {
                frame(reply(r))
            }
        });
        let mut b =
            super::connect_protocol(&c.root(), &task(), &source(), &c.config(&format!("out{i}")))
                .unwrap();
        let p = b.observe(false).unwrap();
        let a = Action::from_observation(&p, "e3").unwrap();
        assert_eq!(b.act(&p, &a, None).unwrap().state, state);
        assert_eq!(s.inputs().len(), 1);
        b.close().unwrap();
    }
}
#[test]
fn malformed_duplicate_truncated_oversized_timeout_and_worker_start_are_not_ready() {
    for (_i, raw, delay) in [
        (0, b"{\"success\":true,\"success\":false}\n".to_vec(), 0),
        (1, b"{}".to_vec(), 0),
        (2, vec![b'x'; 2048], 0),
        (3, b"{}\n{}\n".to_vec(), 0),
        (4, frame(packet(json!({}))), 150),
        (
            5,
            frame(
                json!({"schema":"oev2.peekaboo-bridge/1","success":true,"status":"worker_started"}),
            ),
            0,
        ),
    ] {
        let c = Case::new();
        let s = Server::new(&c, move |_| {
            thread::sleep(Duration::from_millis(delay));
            raw.clone()
        });
        let mut cfg = c.config("out");
        cfg.timeout = Duration::from_millis(50);
        cfg.response_limit = 1024;
        let started = Instant::now();
        assert!(super::connect_protocol(&c.root(), &task(), &source(), &cfg).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(s.requests().len(), 1);
    }
}
#[test]
fn source_socket_permissions_symlink_and_absence_reject_without_browser_operations() {
    let c = Case::new();
    let mut broken = source();
    broken.javascript.push(' ');
    assert!(super::connect_protocol(&c.root(), &task(), &broken, &c.config("source")).is_err());
    assert!(!c.path.join("var/source").exists());
    assert!(super::connect_protocol(&c.root(), &task(), &source(), &c.config("missing")).is_err());
    let s = Server::normal(&c);
    fs::set_permissions(c.path.join("s"), fs::Permissions::from_mode(0o666)).unwrap();
    assert!(super::connect_protocol(&c.root(), &task(), &source(), &c.config("mode")).is_err());
    fs::set_permissions(c.path.join("s"), fs::Permissions::from_mode(0o600)).unwrap();
    symlink(c.path.join("s"), c.path.join("link")).unwrap();
    let mut cfg = c.config("link");
    cfg.peekaboo_socket = Some(c.path.join("link"));
    assert!(super::connect_protocol(&c.root(), &task(), &source(), &cfg).is_err());
    assert!(s.requests().is_empty());
}
#[test]
fn provider_worker_focus_and_chrome_source_changes_refuse_before_further_input() {
    for (i, field, value) in [
        (0, "bridge_sha256", json!("0".repeat(64))),
        (1, "worker_session", json!("changed")),
        (2, "focus_after", json!({"bundle":"other","pid":123})),
        (3, "focus_before", json!({"bundle":"other","pid":123})),
    ] {
        let c = Case::new();
        let s = Server::new(&c, move |r| {
            let mut p = reply(r);
            if r["action"] == "evaluate" {
                p[field] = value.clone();
                if field == "focus_before" {
                    p["focus_after"] = value.clone();
                }
            }
            frame(p)
        });
        let mut b =
            super::connect_protocol(&c.root(), &task(), &source(), &c.config(&format!("out{i}")))
                .unwrap();
        assert!(b.observe(false).is_err());
        assert!(s.inputs().is_empty());
    }
}
#[test]
fn screenshot_and_snapshot_must_share_marker() {
    let c = Case::new();
    let s = Server::new(&c, |r| {
        if r["expression"]
            .as_str()
            .is_some_and(|e| e.contains("p?.marker"))
        {
            frame(js(json!(["different"])))
        } else {
            frame(reply(r))
        }
    });
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    assert!(b.observe(true).is_err());
    assert!(s.inputs().is_empty());
    b.close().unwrap();
}
#[test]
fn explicit_existing_page_filters_actions_and_never_closes_user_target() {
    let c = Case::new();
    let s = Server::normal(&c);
    let mut t = task();
    t.keep_open = true;
    t.tab_id = Some(47);
    t.allowed_actions = vec![crate::AllowedAction {
        kind: crate::ActionKind::Click,
        label: "Read data".into(),
    }];
    let mut b = super::connect_protocol(&c.root(), &t, &source(), &c.config("out")).unwrap();
    assert_eq!(b.identity().target, "47");
    assert_eq!(b.observe(false).unwrap().actions().unwrap().len(), 2);
    assert_eq!(b.close().unwrap()["status"], "retained");
    assert!(!s.requests().iter().any(|r| r["action"] == "close"));
}
fn tree_contains(root: &Path, needle: &[u8]) -> bool {
    fs::read_dir(root).unwrap().flatten().any(|e| {
        let p = e.path();
        if p.is_dir() {
            tree_contains(&p, needle)
        } else {
            fs::read(p)
                .unwrap()
                .windows(needle.len())
                .any(|w| w == needle)
        }
    })
}
#[test]
fn account_values_remain_in_memory_and_navigation_uses_only_bound_fixed_url() {
    let c = Case::new();
    let s = Server::normal(&c);
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    assert_eq!(
        b.account_observation().unwrap()["account_fields"]["username"],
        "Fixture Account Alpha"
    );
    assert!(!tree_contains(
        &c.path.join("var/out"),
        b"Fixture Account Alpha"
    ));
    assert_eq!(
        b.navigate_account().unwrap().state,
        DispatchState::Confirmed
    );
    assert!(s.inputs().is_empty());
    b.close().unwrap();
}
#[test]
fn changed_socket_identity_after_input_cannot_confirm_result() {
    let c = Case::new();
    let path = c.path.join("s");
    let hold = Arc::new(Mutex::new(None));
    let held = hold.clone();
    let s = Server::new(&c, move |r| {
        if r["expression"]
            .as_str()
            .is_some_and(|e| e.contains("e.value=a.value"))
        {
            fs::remove_file(&path).unwrap();
            let replacement = UnixListener::bind(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            *held.lock().unwrap() = Some(replacement);
        }
        frame(reply(r))
    });
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    let p = b.observe(false).unwrap();
    let a = Action::from_observation(&p, "e1").unwrap();
    assert_eq!(
        b.act(&p, &a, None).unwrap().state,
        DispatchState::OutcomeUnknown
    );
    assert_eq!(s.inputs().len(), 1);
    assert!(b.close().is_err());
}
#[test]
fn excessive_controls_are_explicitly_unusable() {
    let c = Case::new();
    let s = Server::new(&c, |r| {
        if r["expression"]
            .as_str()
            .is_some_and(|e| e.contains("snapshot_fixture"))
        {
            let mut p = page();
            p["actions"] = json!(
                (0..254)
                    .map(|i| json!({"id":format!("e{i}"),"kind":"click","node":1,"label":"Read"}))
                    .collect::<Vec<_>>()
            );
            frame(js(p))
        } else {
            frame(reply(r))
        }
    });
    let mut b = super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
    assert!(b.observe(false).is_err());
    assert!(s.inputs().is_empty());
    b.close().unwrap();
}

#[test]
fn new_target_identity_cannot_adopt_a_preexisting_same_url_page() {
    let c = Case::new();
    let server = Server::new(&c, |r| {
        let mut p = reply(r);
        if r["action"] == "open" {
            p["result"]["prior_page_ids"] = json!([7]);
        }
        frame(p)
    });
    assert!(super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).is_err());
    assert!(!server.requests().iter().any(|r| r["action"] == "close"));
    assert!(server.inputs().is_empty());
}

#[test]
fn actual_target_focus_or_origin_change_refuses_before_dom_mutation() {
    for (field, value) in [
        ("page_has_focus", json!(true)),
        ("origin", json!("https://other.invalid")),
        ("url", json!("https://other.invalid/same")),
    ] {
        let c = Case::new();
        let server = Server::new(&c, move |r| {
            let mut p = reply(r);
            if r["action"] == "target_state" {
                p["result"][field] = value.clone();
                p["target_after"][field] = value.clone();
            }
            frame(p)
        });
        let mut browser =
            super::connect_protocol(&c.root(), &task(), &source(), &c.config("out")).unwrap();
        assert!(browser.observe(false).is_err());
        assert!(server.inputs().is_empty());
    }
}
