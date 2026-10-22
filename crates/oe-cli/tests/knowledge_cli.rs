use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = producer.join(format!(
            "var/tests/unified-system/cli/knowledge/fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("var/projects/demo")).unwrap();
        fs::create_dir_all(path.join("sources/books")).unwrap();
        fs::write(path.join("SKILL.md"), "private CLI fixture").unwrap();
        let result = Self(path);
        for relative in [
            "runtime/semantic-bundles.json",
            "runtime/semantica-source-lock.json",
            "references/context-routing-instructions.json",
            "references/supplier-knowledge-policy.json",
        ] {
            result.copy(producer, relative);
        }
        let lock: Value = serde_json::from_slice(
            &fs::read(producer.join("runtime/semantic-bundles.json")).unwrap(),
        )
        .unwrap();
        result.copy(
            producer,
            lock["bundles"]["engineering-judgment-intake"]["path"]
                .as_str()
                .unwrap(),
        );
        result
    }
    fn copy(&self, producer: &Path, relative: &str) {
        let destination = self.0.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(producer.join(relative), destination).unwrap();
    }
    fn save(&self, relative: &str, value: &Value) {
        fs::write(
            self.0.join(relative),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_oev2"));
        cmd.current_dir(&self.0).env_clear().env("PATH", "");
        cmd
    }
    fn run(&self, args: &[&str]) -> (i32, Value) {
        let output = self.command().args(args).output().unwrap();
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
            panic!(
                "non-JSON stdout: {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
        (output.status.code().unwrap(), value)
    }
    fn project(&self) {
        self.save("var/projects/demo/case.json",&json!({"project":{"id":"demo","revision":"1"},"object":{"id":"shaft","revision":"1","name":"shaft"},"decision_id":"decision:shaft","goal":"Understand shaft operating constraints","functions":["transmit torque"]}));
        let fields=["project/id","project/revision","object/id","object/revision","object/name","decision_id","goal","functions"].iter().map(|name|json!({"source":"var/projects/demo/case.json","pointer":format!("/{name}"),"target":format!("/{name}")})).collect::<Vec<_>>();
        self.save("var/projects/demo/projection.json",&json!({"schema":oe_knowledge::context::PROJECTION_SCHEMA,"base":{"schema":oe_knowledge::context::SCHEMA,"task_id":"task:cli"},"fields":fields}));
    }
    fn pdf(&self) {
        let raw = pdf();
        fs::write(self.0.join("sources/books/original.pdf"), &raw).unwrap();
        self.save("sources/books/manifest.json",&json!({"schema":"ontology-engineering.local-pdf-manifest/v1","sources":[{"id":"fixture","title":"Shaft fixture","edition":"one","path":"original.pdf","sha256":oe_core::sha256(raw),"chapters":[{"id":"shaft","title":"shaft mechanism","start_page":1,"end_page":2,"aliases":["shaft"]}]}]}));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn pdf() -> Vec<u8> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Count 2 /Kids [4 0 R 6 0 R] >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    for text in [
        "Shaft transmits torque under stated operating conditions. Check the supporting bearings before selecting a design.",
        "",
    ] {
        let page = objects.len() + 1;
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",page+1).into_bytes());
        let stream = format!("BT /F1 10 Tf 20 350 Td ({text}) Tj ET");
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            )
            .into_bytes(),
        );
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}
const TOOLS: [&str; 6] = [
    "--pdfinfo",
    "/opt/homebrew/bin/pdfinfo",
    "--pdftotext",
    "/opt/homebrew/bin/pdftotext",
    "--pdftoppm",
    "/opt/homebrew/bin/pdftoppm",
];
fn poppler_present() -> bool {
    TOOLS
        .iter()
        .skip(1)
        .step_by(2)
        .all(|p| Path::new(p).is_file())
}

fn delivery_fixture(f: &Fixture) -> Vec<u8> {
    let raw = pdf();
    fs::create_dir_all(f.0.join("var/downloads")).unwrap();
    fs::write(f.0.join("var/downloads/original.pdf"), &raw).unwrap();
    f.save(
        "runtime/source-delivery.json",
        &json!({
            "schema":"ontology-engineering.source-delivery/v1", "files":[{
                "relative_path":"books/original.pdf", "title":"Shaft fixture",
                "bytes":raw.len(), "sha256":oe_core::sha256(&raw),
                "drive":{"file_id":"fixture_drive_id_0001"},
                "registration":{"id":"fixture", "edition":"one", "chapters":[{
                    "id":"shaft", "title":"Shaft", "start_page":1, "end_page":2, "aliases":[]
                }]}
            }]
        }),
    );
    raw
}

#[test]
fn source_delivery_cli_root_import_register_and_relocation_use_same_originals() {
    if !poppler_present() {
        return;
    }
    let mut f = Fixture::new();
    let raw = delivery_fixture(&f);
    let (code, v) = f.run(&["knowledge", "library", "source-delivery", "verify"]);
    assert_eq!((code, v["status"].as_str()), (1, Some("not_ready")));
    // An explicit global root from a different cwd must select the installation,
    // never be shadowed by a subcommand's default source directory.
    let out = f
        .command()
        .current_dir(f.0.parent().unwrap())
        .args([
            "--root",
            f.0.to_str().unwrap(),
            "knowledge",
            "library",
            "source-delivery",
            "import",
            "--from",
            f.0.join("var/downloads").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let (code, v) = f.run(&[
        "knowledge",
        "library",
        "source-delivery",
        "import",
        "--from",
        "var/downloads",
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["files"][0]["status"], "already_verified");
    let mut args = vec!["knowledge", "library", "source-delivery", "register"];
    args.extend(TOOLS);
    let (code, v) = f.run(&args);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["index"]["counts"]["pages"], 2);
    let (code, _) = f.run(&args);
    assert_ne!(code, 0);
    args.push("--replace-index");
    assert_eq!(f.run(&args).0, 0);
    assert_eq!(
        fs::read(f.0.join("var/downloads/original.pdf")).unwrap(),
        raw
    );
    let moved = f.0.with_extension("relocated");
    fs::rename(&f.0, &moved).unwrap();
    f.0 = moved;
    let (code, v) = f.run(&[
        "knowledge",
        "library",
        "source-delivery",
        "verify",
        "--select",
        "fixture",
    ]);
    assert_eq!(code, 0, "{v}");
    let mut args = vec![
        "knowledge",
        "query",
        "--query",
        "shaft",
        "--provider",
        "local-pdf",
        "--output-dir",
        "var/projects/demo/delivery-query",
    ];
    args.extend(TOOLS);
    let (code, v) = f.run(&args);
    assert_eq!(code, 0, "{v}");
    assert!(!v["result"]["results"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read(f.0.join("sources/books/original.pdf")).unwrap(),
        raw
    );
}

#[test]
#[cfg(unix)]
fn source_delivery_explicit_gws_success_failure_timeout_and_changed_manifest() {
    use std::os::unix::fs::PermissionsExt;
    for behavior in ["ok", "wrong", "failed", "timeout", "changed"] {
        let f = Fixture::new();
        delivery_fixture(&f);
        let fake = f.0.join("var/gws-fixture");
        let body = match behavior {
            "ok" => "/bin/cp var/downloads/original.pdf \"$7\"",
            "wrong" => "printf wrong > \"$7\"",
            "failed" => "printf partial > \"$7\"; printf 'fixture-private-token' >&2; exit 9",
            "timeout" => "printf partial > \"$7\"; /bin/sleep 10",
            "changed" => {
                "/bin/cp var/downloads/original.pdf \"$7\"; printf '\\n' >> runtime/source-delivery.json"
            }
            _ => unreachable!(),
        };
        fs::write(&fake, format!("#!/bin/sh\n[ \"$1 $2 $3 $4 $6\" = 'drive files get --params --output' ] || exit 8\n[ \"$5\" = '{{\"fileId\":\"fixture_drive_id_0001\",\"alt\":\"media\"}}' ] || exit 7\n{body}\n")).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let args = [
            "knowledge",
            "library",
            "source-delivery",
            "fetch",
            "--gws",
            fake.to_str().unwrap(),
            "--timeout-seconds",
            "1",
        ];
        assert_ne!(f.run(&args).0, 0); // --select is mandatory; never dispatch.
        assert!(!f.0.join("var/cache").exists());
        let start = std::time::Instant::now();
        let (code, v) = f.run(&[args.as_slice(), &["--select", "books"]].concat());
        assert_eq!(code == 0, behavior == "ok", "{behavior}: {v}");
        assert!(!v.to_string().contains("fixture-private-token"));
        assert_eq!(
            f.0.join("sources/books/original.pdf").exists(),
            behavior == "ok"
        );
        if behavior == "timeout" {
            assert!(start.elapsed().as_secs() < 5);
        }
        let staging = f.0.join("var/cache/knowledge");
        assert_eq!(fs::read_dir(staging).unwrap().count(), 0);
    }
}

#[test]
fn no_path_receiver_status_and_argument_failures_are_structured_and_private() {
    let f = Fixture::new();
    let (code, value) = f.run(&["knowledge", "status"]);
    assert_eq!(code, 1);
    assert_eq!(value["status"], "not_ready");
    assert_eq!(value["result"]["providers"].as_array().unwrap().len(), 2);
    for args in [
        vec![
            "knowledge",
            "view",
            "--provider",
            "all",
            "--page-id",
            "PRIVATE_VALUE",
        ],
        vec![
            "knowledge",
            "query",
            "--query",
            "PRIVATE_VALUE",
            "--output-dir",
            "var/run",
            "--backend",
            "PRIVATE_BACKEND",
        ],
        vec!["knowledge", "feedback"],
    ] {
        let (code, v) = f.run(&args);
        assert_eq!(code, 2);
        assert_eq!(v["status"], "blocked");
        assert!(!v.to_string().contains("PRIVATE_"));
    }
    let output = f.command().args(["knowledge", "--help"]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("verify-packet"));
}

#[test]
fn project_context_is_exact_rooted_and_preserves_source_change_detection() {
    let f = Fixture::new();
    f.project();
    let (code, value) = f.run(&[
        "knowledge",
        "project-context",
        "--input",
        "var/projects/demo/projection.json",
        "--output-dir",
        "var/projects/demo/context-01",
    ]);
    assert_eq!(code, 0);
    assert_eq!(value["status"], "projected");
    assert_eq!(
        value["result"]["source_bindings"].as_array().unwrap().len(),
        1
    );
    assert!(
        f.0.join("var/projects/demo/context-01/context.json")
            .is_file()
    );
    let (code, v) = f.run(&[
        "knowledge",
        "project-context",
        "--input",
        "var/projects/demo/projection.json",
        "--output-dir",
        "sources/not-an-output",
    ]);
    assert_eq!(code, 2);
    assert_eq!(v["status"], "blocked");
    assert!(!f.0.join("sources/not-an-output").exists());
    f.save("var/projects/demo/context.json", &value["result"]);
    f.save("var/projects/demo/case.json", &json!({"changed":true}));
    let (code, v) = f.run(&[
        "knowledge",
        "query",
        "--query",
        "shaft",
        "--context",
        "var/projects/demo/context.json",
        "--output-dir",
        "var/projects/demo/stale",
    ]);
    assert_eq!(code, 1);
    assert_eq!(v["result"]["status"], "stale_context");
    assert_eq!(v["result"]["usage"]["network_requests"], 0);
}

#[test]
fn local_query_missing_indexes_is_not_ready_and_does_not_resolve_credentials() {
    let f = Fixture::new();
    let (code, v) = f.run(&[
        "knowledge",
        "query",
        "--query",
        "Understand a shaft",
        "--output-dir",
        "var/projects/demo/no-sources",
    ]);
    assert_eq!(code, 1);
    assert_eq!(v["result"]["status"], "not_ready");
    assert_eq!(v["result"]["mode"], "local");
    assert_eq!(v["result"]["usage"]["network_requests"], 0);
    assert!(
        f.0.join("var/projects/demo/no-sources/report.json")
            .is_file()
    );
    let (code, v) = f.run(&[
        "knowledge",
        "query",
        "--query",
        "shaft",
        "--model",
        "removed-provider",
        "--output-dir",
        "var/projects/demo/invalid-model",
    ]);
    assert_eq!(code, 2);
    assert_eq!(v["status"], "blocked");
    assert!(!f.0.join("var/projects/demo/invalid-model").exists());
}

#[test]
fn rooted_json_rejects_duplicate_keys_symlinks_and_outside_paths() {
    let f = Fixture::new();
    fs::write(
        f.0.join("var/projects/demo/duplicate.json"),
        "{\"schema\":1,\"schema\":2}",
    )
    .unwrap();
    for path in ["var/projects/demo/duplicate.json", "../SKILL.md"] {
        let (code, v) = f.run(&["knowledge", "project-context", "--input", path]);
        assert_eq!(code, 2);
        assert_eq!(v["status"], "blocked");
    }
    #[cfg(unix)]
    {
        f.project();
        std::os::unix::fs::symlink(
            f.0.join("var/projects/demo/projection.json"),
            f.0.join("var/projects/demo/link.json"),
        )
        .unwrap();
        let (code, v) = f.run(&[
            "knowledge",
            "project-context",
            "--input",
            "var/projects/demo/link.json",
        ]);
        assert_eq!(code, 2);
        assert_eq!(v["status"], "blocked");
    }
}

#[test]
fn native_registration_local_query_and_direct_blank_page_view_work_without_path() {
    if !poppler_present() {
        return;
    }
    let f = Fixture::new();
    f.pdf();
    f.project();
    let original = fs::read(f.0.join("sources/books/original.pdf")).unwrap();
    let mut args = vec![
        "knowledge",
        "register",
        "--manifest",
        "sources/books/manifest.json",
    ];
    args.extend(TOOLS);
    let (code, v) = f.run(&args);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["index"]["counts"]["pages"], 2);
    let (code, v) = f.run(&["knowledge", "status", "--provider", "local-pdf"]);
    assert_eq!(code, 0);
    assert_eq!(v["status"], "ready");
    let mut args = vec![
        "knowledge",
        "query",
        "--query",
        "shaft",
        "--project-context",
        "var/projects/demo/projection.json",
        "--provider",
        "local-pdf",
        "--candidates",
        "4",
        "--top",
        "2",
        "--full-text",
        "--output-dir",
        "var/projects/demo/query-01",
    ];
    args.extend(TOOLS);
    let (code, v) = f.run(&args);
    if v["packet"]["status"] == "not_created" {
        let result = oe_knowledge::packet::make_with_tools(
            &oe_core::SkillRoot::new(&f.0).unwrap(),
            &v["result"],
            &v["result"]["context_snapshot"],
            &oe_knowledge::poppler::PopplerTools {
                pdfinfo: TOOLS[1].into(),
                pdftotext: TOOLS[3].into(),
                pdftoppm: TOOLS[5].into(),
            },
        );
        assert!(
            result.is_ok(),
            "packet fixture failure: {}",
            result.err().unwrap()
        );
    }
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["mode"], "local");
    assert_eq!(v["result"]["usage"]["network_requests"], 0);
    assert!(!v["result"]["results"].as_array().unwrap().is_empty());
    assert_eq!(v["packet"]["status"], "created", "{v}");
    let packet_path = "var/projects/demo/query-01/packet.json";
    let packet: Value = serde_json::from_slice(&fs::read(f.0.join(packet_path)).unwrap()).unwrap();
    assert!(
        !packet["evidence_candidates"][0]["page_native_text"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    let candidate = packet["evidence_candidates"][0]["id"].as_str().unwrap();
    let mut args = vec![
        "knowledge",
        "verify-packet",
        "--packet",
        packet_path,
        "--project-context",
        "var/projects/demo/projection.json",
    ];
    args.extend(TOOLS);
    let (code, verified) = f.run(&args);
    assert_eq!(code, 0, "{verified}");
    assert_eq!(verified["result"]["status"], "ready_for_agent_review");
    assert_eq!(verified["result"]["semantic_review"], "not_run");
    let mut args = vec![
        "knowledge",
        "feedback",
        "--packet",
        packet_path,
        "--project-context",
        "var/projects/demo/projection.json",
        "--candidate",
        candidate,
        "--disposition",
        "deferred",
        "--reason",
        "Need actual bearing measurements",
        "--next-check",
        "Inspect the original constraints before any engineering decision",
        "--output",
        "var/projects/demo/feedback-01.json",
    ];
    args.extend(TOOLS);
    let (code, feedback) = f.run(&args);
    assert_eq!(code, 0, "{feedback}");
    assert_eq!(feedback["result"]["disposition"], "deferred");
    assert_eq!(feedback["result"]["engineering_verdict"], "not_run");
    let hash = oe_core::sha256(fs::read(f.0.join("var/projects/demo/feedback-01.json")).unwrap());
    f.save(
        "var/projects/demo/history.json",
        &json!([{"path":"var/projects/demo/feedback-01.json","sha256":hash}]),
    );
    let mut args = vec![
        "knowledge",
        "query",
        "--query",
        "shaft",
        "--project-context",
        "var/projects/demo/projection.json",
        "--history",
        "var/projects/demo/history.json",
        "--provider",
        "local-pdf",
        "--candidates",
        "2",
        "--top",
        "1",
        "--output-dir",
        "var/projects/demo/query-history",
    ];
    args.extend(TOOLS);
    let (code, history) = f.run(&args);
    assert_eq!(code, 0, "{history}");
    assert_eq!(
        history["result"]["explicit_history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(history["result"]["results"][0].get("native_text").is_none());
    let record: Value = serde_json::from_slice(
        &fs::read(f.0.join("var/projects/demo/query-history/report.json")).unwrap(),
    )
    .unwrap();
    assert!(record["results"][0]["native_text"].is_string());
    f.save(
        "var/projects/demo/bad-history.json",
        &json!([{"path":"var/projects/demo/feedback-01.json","sha256":"0".repeat(64)}]),
    );
    let mut args = vec![
        "knowledge",
        "query",
        "--query",
        "shaft",
        "--project-context",
        "var/projects/demo/projection.json",
        "--history",
        "var/projects/demo/bad-history.json",
        "--provider",
        "local-pdf",
        "--output-dir",
        "var/projects/demo/bad-history-run",
    ];
    args.extend(TOOLS);
    let (code, bad_history) = f.run(&args);
    assert_eq!(code, 2);
    assert_eq!(bad_history["status"], "blocked");
    assert!(!f.0.join("var/projects/demo/bad-history-run").exists());
    let mut args = vec![
        "knowledge",
        "view",
        "--provider",
        "local-pdf",
        "--page-id",
        "local_pdf:fixture:page:2",
        "--dpi",
        "72",
        "--output-dir",
        "var/projects/demo/view-blank",
    ];
    args.extend(TOOLS);
    let (code, v) = f.run(&args);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["identity"]["source_file_page"], 2);
    assert_eq!(v["result"]["identity"]["whole_page"], true);
    assert_eq!(
        fs::read(f.0.join("sources/books/original.pdf")).unwrap(),
        original
    );
    fs::write(f.0.join("sources/books/original.pdf"), "changed original").unwrap();
    let (code, v) = f.run(&args);
    assert_eq!(code, 2);
    assert_eq!(v["status"], "blocked");
    let mut args = vec![
        "knowledge",
        "verify-packet",
        "--packet",
        packet_path,
        "--project-context",
        "var/projects/demo/projection.json",
    ];
    args.extend(TOOLS);
    let (code, v) = f.run(&args);
    assert_eq!(code, 2);
    assert_eq!(v["status"], "blocked");
}

#[test]
fn query_rejects_secret_shaped_text_even_with_separate_valid_context() {
    let f = Fixture::new();
    f.project();
    let secret = format!("{}{}", "apikey_", "synthetic_denial_only");
    let (code, v) = f.run(&[
        "knowledge",
        "query",
        "--query",
        &secret,
        "--project-context",
        "var/projects/demo/projection.json",
        "--output-dir",
        "var/projects/demo/unsafe-query",
    ]);
    assert_eq!(code, 2);
    assert_eq!(v["status"], "blocked");
    assert!(!v.to_string().contains(&secret));
    assert!(!f.0.join("var/projects/demo/unsafe-query").exists());
}
