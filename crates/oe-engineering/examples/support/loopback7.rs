// Fixture-only start notification; all HTTP authority remains in the original transport.
struct FixtureSendWindow7<'a> {
    http: &'a HttpModelTransport,
    send_started: std::sync::mpsc::Sender<std::time::Instant>,
    source_deadline: std::time::Instant,
}
impl ModelTransport for FixtureSendWindow7<'_> {
    fn identity(&self) -> String { self.http.identity() }
    fn check_service_configuration(
        &self, root: &SkillRoot, profile: &ModelServiceProfile,
    ) -> Option<Result<(), ModelTransportError>> {
        // The original run_prepared performs further current-source checks after this hook.
        // Checking settings must not consume the HTTP accept window.
        self.http.check_service_configuration(root, profile)
    }
    fn send(&self, request: &Value) -> Result<Value, ModelTransportError> {
        let started = std::time::Instant::now();
        if started >= self.source_deadline {
            return Err(ModelTransportError { code: "fixture_source_clock_expired_before_send".into(), http_status: None, dispatch_attempted: false });
        }
        self.send_started.send(started).map_err(|_| ModelTransportError {
            code: "fixture_send_stage_closed".into(), http_status: None, dispatch_attempted: false,
        })?;
        self.http.send(request)
    }
}
fn wait_for_fixture_send7(
    receiver: std::sync::mpsc::Receiver<std::time::Instant>,
    source_deadline: std::time::Instant,
) -> Result<std::time::Instant, &'static str> {
    let remaining = source_deadline.checked_duration_since(std::time::Instant::now())
        .ok_or("fixture_source_clock_expired_waiting_for_send")?;
    let started = receiver.recv_timeout(remaining).map_err(|error| match error {
        std::sync::mpsc::RecvTimeoutError::Timeout => "fixture_source_clock_expired_waiting_for_send",
        std::sync::mpsc::RecvTimeoutError::Disconnected => "fixture_send_stage_closed",
    })?;
    if std::time::Instant::now() >= source_deadline || started >= source_deadline {
        return Err("fixture_source_clock_expired_waiting_for_send");
    }
    Ok(started)
}
fn actual_loopback7(f:&Fixture,input:&Path,sources:&[&oe_knowledge::registered_ocr6::PreparedRegisteredOcrPage6],source_deadline:std::time::Instant) -> Value {
    let project = f.project_bytes();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let mut profile = load(&f.profile);
    profile["endpoint"] = json!(endpoint);
    save(&f.profile, &profile);
    let credential = f.root.path().join("var/unused-credential.txt");
    fs::write(&credential, "isolated-loopback-fixture-bearer\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let (send_started, send_receiver) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let started = wait_for_fixture_send7(send_receiver, source_deadline).expect("same caller clock awaiting real send stage");
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(v) => break v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        started.elapsed() < Duration::from_secs(30),
                        "no loopback request"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("loopback accept {e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut received = Vec::new();
        let mut chunk = [0u8; 4096];
        let end = loop {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            received.extend_from_slice(&chunk[..n]);
            assert!(received.len() < 1_100_000);
            if let Some(i) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let headers = std::str::from_utf8(&received[..end]).unwrap();
        assert!(headers.starts_with("POST /v1/responses HTTP/1.1\r\n"));
        let length: usize = headers
            .lines()
            .find_map(|s| {
                s.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse().unwrap())
            })
            .unwrap();
        while received.len() - end < length {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            received.extend_from_slice(&chunk[..n]);
        }
        assert_eq!(received.len() - end, length);
        let body = received[end..].to_vec();
        let raw = serde_json::to_vec(&response()).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",raw.len()).unwrap();
        stream.write_all(&raw).unwrap();
        stream.flush().unwrap();
        drop(stream);
        // One application request only; retries would leave an additional accepted socket.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        body
    });
    let transport =
        HttpModelTransport::new(&endpoint, &credential, Duration::from_secs(30), 1_000_000)
            .unwrap();
    let send_window = FixtureSendWindow7 { http: &transport, send_started, source_deadline };
    let t = tools();
    let executor = oe_engineering::model_registered_ocr7::RegisteredOcrModelExecutor7 {
        sources,
        root: &f.root,
        tools: &t,
        transport: Some(&send_window),
    };
    let typed=oe_contracts::ModelRegisteredOcrRunInput7{schema:"oev2.model-registered-ocr-run-input/7".into(),input:serde_json::from_value(reference(input)).unwrap(),profile:serde_json::from_value(reference(&f.profile)).unwrap(),output_dir:"var/loopback".into()};
    let receipt = oe_contracts::ModelRegisteredOcrRunPort7::run_registered_ocr(&executor,&typed).unwrap();
    drop(send_window);
    let wire = server.join().unwrap();
    let result_path = f.root.expand(Path::new(&receipt.result.path)).unwrap();
    let result = load(&result_path);
    assert_eq!(receipt.status, "completed");
    assert_eq!(result["client_dispatch_attempts"], 1);
    assert_eq!(
        receipt.result.sha256,
        sha256(fs::read(&result_path).unwrap())
    );
    assert_eq!(result["transport_configuration_status"], "matched");
    assert_eq!(
        result["plan"]["execution_port"],
        "shared_model_run_prepared/7"
    );
    let request: Value = serde_json::from_slice(&wire).unwrap();
    let payload: Value =
        serde_json::from_str(request["input"][0]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["schema"], "oev2.model-assistance-payload/7");
    assert_eq!(payload["selected_page_support_scope"], oe_engineering::model_registered_ocr7::SCOPE);
    assert!(oe_engineering::model_registered_ocr7::SCOPE.contains("not a blank physical page"));
    assert_eq!(
        payload["selected_page_support"]["pages"][0]["text"],
        include_str!("../../tests/fixtures/model-pages/vol2-page24.txt")
    );
    assert_eq!(payload["selected_page_support"]["pages"][1]["text"], "");
    assert_eq!(
        payload["selected_page_support"]["pages"][1]["content_status"],
        "navigation_only"
    );
    assert_eq!(
        payload["selected_page_support"]["pages"][1]["ocr"],
        "not_run"
    );
    assert_eq!(
        payload["selected_page_support"]["pages"][0]["document_bytes"],
        17_154_281
    );
    assert_eq!(
        payload["current_project"],
        serde_json::to_value(&f.project).unwrap()
    );
    assert_eq!(
        payload["allowed_source_refs"],
        json!(["tolerance-dimension-chain-page188", "trust-scope-page24"])
    );
    assert_eq!(request["store"], false);
    assert_eq!(request["stream"], false);
    assert_eq!(request["truncation"], "disabled");
    assert_eq!(
        fs::read(f.root.path().join("var/loopback/request.json")).unwrap(),
        wire
    );
    assert_eq!(transport.observation()["send_call_attempts"], 1);
    assert_eq!(
        transport.observation()["attempts"][0]["request_sha256"],
        sha256(&wire)
    );
    assert_eq!(transport.observation()["attempts"][0]["http_status"], 200);
    assert_eq!(result["plan"]["request_sha256"], sha256(&wire));
    assert_not_adopted(&result);
    assert_eq!(f.project_bytes(), project);
    let archived = load(
        &f.root
            .path()
            .join("var/loopback/selected-pages.original.json"),
    );
    assert_eq!(archived, payload["selected_page_support"]);
    assert_eq!(
        fs::read(
            f.root
                .path()
                .join("var/loopback/page-assistance-input.original")
        )
        .unwrap(),
        fs::read(&f.wrapper).unwrap()
    );
    assert_eq!(
        fs::read(f.root.path().join("var/loopback/profile.original")).unwrap(),
        fs::read(&f.profile).unwrap()
    );
    assert_eq!(payload["selected_registered_ocr_support"]["ocr"][0]["provenance"],serde_json::to_value(sources[0].report()).unwrap());
    assert_eq!(payload["selected_registered_ocr_support"]["native"],payload["selected_page_support"]);
    fs::write(f.root.path().join("var/actual-loopback-wire.json"),&wire).unwrap();
    save(&f.root.path().join("var/actual-loopback-transport.json"),&transport.observation());
    save(&f.root.path().join("var/actual-loopback-waits.json"),&serde_json::to_value(transport.wait_observation()).unwrap());
    result
}
#[cfg(test)]
mod send_window7_tests {
    use super::*;
    fn transport_fixture7() -> (tempfile::TempDir, SkillRoot, std::net::TcpListener, HttpModelTransport, ModelServiceProfile) {
        let dir = tempfile::tempdir().unwrap();
        let root_path = dir.path().canonicalize().unwrap();
        fs::write(root_path.join("SKILL.md"), "Isolated transport timing fixture").unwrap();
        fs::create_dir(root_path.join("var")).unwrap();
        let root = SkillRoot::new(&root_path).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let credential = root.path().join("var/unused-credential.txt");
        fs::write(&credential, "isolated-loopback-fixture-bearer\n").unwrap();
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap(); }
        let profile: ModelServiceProfile = serde_json::from_value(json!({"schema":model_assistance::PROFILE_SCHEMA,"service_id":"offline-page-fixture","api_kind":"openai_responses",
            "endpoint":endpoint,"requested_model":"offline-model","accepted_response_models":["offline-model"],
            "context_window_tokens":2000000,"maximum_request_bytes":1000000,"maximum_output_tokens":2048,"maximum_response_bytes":1000000,
            "wrapper_reserve_tokens":4096,"timeout_seconds":30,"credential_file":"var/unused-credential.txt",
            "capacity_evidence":"Synthetic timing fixture only"})).unwrap();
        let http = HttpModelTransport::new(&endpoint,&credential,Duration::from_secs(30),1_000_000).unwrap();
        (dir,root,listener,http,profile)
    }
    #[test]
    fn send_stage_preserves_identity_and_config_without_activation7() {
        let (_dir,root,_listener,http,profile)=transport_fixture7();
        let (tx,rx)=std::sync::mpsc::channel();
        let wrapper=FixtureSendWindow7{http:&http,send_started:tx,source_deadline:std::time::Instant::now()+Duration::from_secs(120)};
        assert_eq!(wrapper.identity(),http.identity());
        assert_eq!(wrapper.check_service_configuration(&root,&profile),http.check_service_configuration(&root,&profile));
        assert!(matches!(rx.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)));
        let mut mismatch=profile.clone();mismatch.timeout_seconds=31;
        assert_eq!(wrapper.check_service_configuration(&root,&mismatch),http.check_service_configuration(&root,&mismatch));
        assert!(wrapper.check_service_configuration(&root,&mismatch).unwrap().is_err());
        assert!(matches!(rx.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)));
        assert_eq!(http.observation()["send_call_attempts"],0);
    }
    #[test]
    fn no_send_wait_uses_original_absolute_deadline7() {
        let (_tx,rx)=std::sync::mpsc::channel();
        let deadline=std::time::Instant::now()+Duration::from_millis(40);
        assert_eq!(wait_for_fixture_send7(rx,deadline).unwrap_err(),"fixture_source_clock_expired_waiting_for_send");
        assert!(std::time::Instant::now()>=deadline);
    }
    #[test]
    fn expired_original_clock_refuses_before_activation_and_http7() {
        let (_dir,_root,_listener,http,_profile)=transport_fixture7();
        let (tx,rx)=std::sync::mpsc::channel();
        let wrapper=FixtureSendWindow7{http:&http,send_started:tx,source_deadline:std::time::Instant::now()};
        let e=wrapper.send(&json!({"source":"complete unchanged"})).unwrap_err();
        assert_eq!(e.code,"fixture_source_clock_expired_before_send");assert!(!e.dispatch_attempted);
        assert!(matches!(rx.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)));
        assert_eq!(http.observation()["send_call_attempts"],0);
    }
    #[test]
    fn send_signal_and_actual_http_preserve_exact_body7() {
        let (_dir,root,listener,http,profile)=transport_fixture7();
        let (tx,rx)=std::sync::mpsc::channel();
        let deadline=std::time::Instant::now()+Duration::from_secs(120);
        let wrapper=FixtureSendWindow7{http:&http,send_started:tx,source_deadline:deadline};
        assert!(wrapper.check_service_configuration(&root,&profile).unwrap().is_ok());
        assert!(matches!(rx.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)));
        let server=thread::spawn(move||{
            let started=wait_for_fixture_send7(rx,deadline).unwrap();
            listener.set_nonblocking(true).unwrap();
            let (mut stream,_)=loop{match listener.accept(){Ok(v)=>break v,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{assert!(started.elapsed()<Duration::from_secs(30));thread::sleep(Duration::from_millis(10));},Err(e)=>panic!("accept {e}")}};
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut received=Vec::new();let mut chunk=[0u8;4096];
            let end=loop{let n=stream.read(&mut chunk).unwrap();assert!(n>0);received.extend_from_slice(&chunk[..n]);if let Some(i)=received.windows(4).position(|w|w==b"\r\n\r\n"){break i+4;}};
            let headers=std::str::from_utf8(&received[..end]).unwrap();assert!(headers.starts_with("POST /v1/responses HTTP/1.1\r\n"));
            let length:usize=headers.lines().find_map(|s|s.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).map(|(_,v)|v.trim().parse().unwrap())).unwrap();
            while received.len()-end<length{let n=stream.read(&mut chunk).unwrap();assert!(n>0);received.extend_from_slice(&chunk[..n]);}
            assert_eq!(received.len()-end,length);let body=received[end..].to_vec();let raw=serde_json::to_vec(&json!({"fixture":"advisory only"})).unwrap();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",raw.len()).unwrap();stream.write_all(&raw).unwrap();stream.flush().unwrap();body
        });
        let request=json!({"model":"offline-model","input":[{"role":"user","content":[{"type":"input_text","text":"完整来源与 unknown 保留"}]}],"store":false,"stream":false,"truncation":"disabled"});
        let result=wrapper.send(&request).unwrap();let body=server.join().unwrap();
        assert_eq!(body,serde_json::to_vec(&request).unwrap());assert_eq!(result,json!({"fixture":"advisory only"}));
        assert_eq!(http.observation()["send_call_attempts"],1);assert_eq!(http.observation()["attempts"][0]["request_sha256"],sha256(&body));
        assert_eq!(http.observation()["attempts"][0]["http_status"],200);
    }
}
