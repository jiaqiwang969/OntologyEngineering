// Exactly one known-page source factory followed by finite localhost controls.
include!("support/native_fixture7.rs");
include!("support/loopback7.rs");
use anyhow::{Result, ensure};
use oe_engineering::model_registered_ocr7 as channel;
use oe_knowledge::registered_ocr6;
use std::time::Instant;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage request6 independent-gold fresh-leaf"
    );
    let started = Instant::now();
    let deadline = started + Duration::from_secs(120);
    let f = Fixture::new();
    println!(
        "{}",
        json!({"persistent_fixture":f.root.path(),"source_clock_seconds":120,"actual_factory_calls_before":0})
    );
    let input6: Value = oe_core::json::strict_json(&oe_core::read_regular_bytes(
        Path::new(&args[1]),
        1024 * 1024,
    )?)?;
    let request6 = registered_ocr6::parse_request6(&input6)?;
    let gold: Value = oe_core::json::strict_json(&oe_core::read_regular_bytes(
        Path::new(&args[2]),
        1024 * 1024,
    )?)?;
    ensure!(
        request6.document_id == "vol2"
            && request6.page.physical_page == 24
            && gold["development_seen"] == true
            && gold["unseen"] == false
            && gold["ocr_accuracy_gold"] == "none",
        "frozen_known_page"
    );
    let input = f.root.path().join("var/ocr7-input.json");
    let v = json!({"schema":channel::INPUT_SCHEMA,"native_assistance":reference(&f.wrapper),"ocr_pages":[{"source_ref":"trust-scope-page24","request":input6,"expected_selected_row_sha256":null,"expected_raw_tsv_sha256":null}]});
    save(&input, &v);
    let project_before = f.project_bytes();
    let source = registered_ocr6::prepare_registered_page_ocr6(
        &f.root,
        &request6,
        &tools(),
        deadline,
        &args[3],
    )?;
    let report = serde_json::to_value(source.report())?;
    ensure!(
        report["page"]["render"]["png"]["sha256"] == gold["png_sha256"]
            && report["page"]["image_content"]["decoded_sha256"] == gold["pixels_sha256"]
            && report["page"]["native_text_chars"] == gold["native_text_chars"],
        "pre-output original image and native source"
    );
    let sources = [&source];
    let mut controls = Vec::new();
    ensure!(
        channel::prepare_with_registered_ocr7(&f.root, &input, &f.profile, &tools(), &[]).is_err(),
        "JSON authority"
    );
    controls.push(json!({"case":"JSON_without_capability","refused":true,"dispatch":0}));
    for (key, value) in [
        ("expected_raw_tsv_sha256", json!("b".repeat(64))),
        ("expected_selected_row_sha256", json!("b".repeat(64))),
        ("source_ref", json!("invented-source")),
    ] {
        let mut changed = v.clone();
        changed["ocr_pages"][0][key] = value;
        let path = f.root.path().join(format!("var/refused-{key}.json"));
        save(&path, &changed);
        ensure!(
            channel::prepare_with_registered_ocr7(&f.root, &path, &f.profile, &tools(), &sources)
                .is_err(),
            "accepted {key}"
        );
        controls.push(json!({"case":key,"refused":true,"dispatch":0}));
    }
    let foreign = tempfile::tempdir()?;
    fs::write(foreign.path().join("SKILL.md"), "foreign root")?;
    let foreign = SkillRoot::new(foreign.path())?;
    ensure!(
        registered_ocr6::verify_in_root6(&foreign, &source).is_err(),
        "foreign root authority"
    );
    controls.push(json!({"case":"foreign_root","refused":true,"dispatch":0}));
    let loopback = actual_loopback7(&f, &input, &sources, deadline);
    controls.push(json!({"case":"real_loopback_generated_port7","status":loopback["status"],"dispatch":1,"full_raw_source_rows_and_native_separate":true}));
    let raw_path = f
        .root
        .expand(Path::new(source.report().raw_tsv.path.as_str()))?;
    let original = fs::read(&raw_path)?;
    let changed = changed(&raw_path);
    for name in [
        "before-raw-drift",
        "after-raw-drift",
        "invented-response-ref",
    ] {
        let mut transport = f.injected();
        if name == "before-raw-drift" {
            transport.before = Some((raw_path.clone(), changed.clone()));
        }
        if name == "after-raw-drift" {
            transport.after = Some((raw_path.clone(), changed.clone()));
        }
        if name == "invented-response-ref" {
            let mut a = advice();
            a["answers"][0]["source_refs"] = json!(["invented-source"]);
            transport.response["output"][0]["content"][0]["text"] =
                json!(serde_json::to_string(&a)?);
        }
        let t = tools();
        let prepared =
            channel::prepare_with_registered_ocr7(&f.root, &input, &f.profile, &t, &sources)?;
        let result = model_assistance::run_registered_ocr7(
            &f.root,
            prepared,
            Path::new(&format!("var/{name}")),
            &t,
            Some(&transport),
        );
        fs::write(&raw_path, &original)?;
        let result = result?;
        assert_not_adopted(&result);
        if name == "before-raw-drift" {
            ensure!(
                result["status"] == "stale" && transport.requests.borrow().is_empty(),
                "source drift sent"
            );
        }
        if name == "after-raw-drift" {
            ensure!(
                result["status"] == "stale"
                    && result["current_support"] == "not_established"
                    && result["advisory"].is_object()
                    && transport.requests.borrow().len() == 1
                    && f.root
                        .path()
                        .join(format!("var/{name}/response.parsed.json"))
                        .exists(),
                "drift lost response or adopted"
            );
        }
        if name == "invented-response-ref" {
            ensure!(
                result["status"] == "failed"
                    && result["advisory"].is_null()
                    && transport.requests.borrow().len() == 1,
                "invented source accepted"
            );
        }
        controls.push(json!({"case":name,"status":result["status"],"dispatch":transport.requests.borrow().len(),"current_support":result["current_support"]}));
    }
    let profile_raw = fs::read(&f.profile)?;
    let profile_value = load(&f.profile);
    for (name, key) in [
        ("byte-budget", "maximum_request_bytes"),
        ("context-budget", "context_window_tokens"),
    ] {
        let mut v = profile_value.clone();
        v[key] = json!(1);
        save(&f.profile, &v);
        let t = tools();
        let prepared =
            channel::prepare_with_registered_ocr7(&f.root, &input, &f.profile, &t, &sources);
        let result = match prepared {
            Ok(p) => {
                ensure!(
                    !p.admitted() && serde_json::to_value(&p.packet().ocr[0].provenance)? == report,
                    "budget trimmed source"
                );
                model_assistance::run_registered_ocr7(
                    &f.root,
                    p,
                    Path::new(&format!("var/{name}")),
                    &t,
                    None,
                )
            }
            Err(e) => Err(e),
        };
        fs::write(&f.profile, &profile_raw)?;
        let result = result?;
        ensure!(
            result["status"] == "blocked" && result["client_dispatch_attempts"] == 0,
            "budget bypass"
        );
        controls
            .push(json!({"case":name,"status":"blocked","dispatch":0,"full_source_retained":true}));
    }
    let t = tools();
    let prepared =
        channel::prepare_with_registered_ocr7(&f.root, &input, &f.profile, &t, &sources)?;
    let r = model_assistance::run_registered_ocr7(
        &f.root,
        prepared,
        Path::new("var/offline"),
        &t,
        None,
    )?;
    ensure!(
        r["status"] == "not_run" && r["client_dispatch_attempts"] == 0,
        "offline sent"
    );
    controls.push(json!({"case":"offline","status":"not_run","dispatch":0}));
    let prepared =
        channel::prepare_with_registered_ocr7(&f.root, &input, &f.profile, &t, &sources)?;
    ensure!(
        model_assistance::run_registered_ocr7(
            &f.root,
            prepared,
            Path::new("var/loopback"),
            &t,
            None
        )
        .is_err(),
        "overwrote old output"
    );
    controls.push(json!({"case":"fresh_output_collision","refused":true,"dispatch":0}));
    registered_ocr6::verify_in_root6(&f.root, &source)?;
    ensure!(f.project_bytes() == project_before, "Project changed");
    ensure!(Instant::now() < deadline, "same source clock expired");
    let terminal = json!({"schema":"private.same-Project-OCR7-finite-actual/1","actual_ocr_factory_calls":1,"actual_local_HTTP_POST":1,"injected_dispatches":2,"development_seen":true,"unseen":false,"source6":report,"controls":controls,"source_clock_seconds":120,"elapsed_millis":started.elapsed().as_millis(),"service_profile_timeout_seconds":30,"real_credentials":"not_read","real_external_model":"not_run","Project":"synthetic fixture exact unchanged, no engineering adoption","whole_scope":"unknown"});
    let path = f.root.path().join("var/actual-terminal7.json");
    oe_core::write_new(&path, &terminal)?;
    println!(
        "{}",
        json!({"terminal":path,"elapsed_millis":started.elapsed().as_millis(),"actual_ocr_factory_calls":1,"actual_local_HTTP_POST":1,"real_external_model":"not_run"})
    );
    Ok(())
}
