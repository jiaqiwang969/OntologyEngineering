use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn get(address: &str, path: &str) -> Vec<u8> {
    let mut stream = match TcpStream::connect(address) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => return vec![],
        Err(e) => panic!("{e}"),
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let raw = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").into_bytes();
    if let Err(e) = stream.write_all(&raw) {
        assert!(
            [
                std::io::ErrorKind::BrokenPipe,
                std::io::ErrorKind::ConnectionReset
            ]
            .contains(&e.kind())
        );
        return vec![];
    }
    let _ = stream.shutdown(Shutdown::Write);
    let mut out = vec![];
    if let Err(e) = stream.read_to_end(&mut out) {
        assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset);
    }
    out
}
fn body(wire: &[u8]) -> Value {
    assert!(wire.starts_with(b"HTTP/1.1 200"));
    let at = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    serde_json::from_slice(&wire[at + 4..]).unwrap()
}
#[test]
fn actual_decisions_cli_uses_selected_session_and_rejects_source_drift() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    for resource in ["semantica.dataset-default/1", "semantica.context-50k/1"] {
        let temp = tempfile::tempdir_in(root.join("var")).unwrap();
        let graph = temp.path().join("graph.json");
        let raw=serde_json::to_vec(&json!({"nodes":[{"id":"d1","type":"decision","content":"choice","properties":{"confidence":0.5,"timestamp":0.9999996}},{"id":"policy","type":"Policy"}],"edges":[{"id":"v","source":"d1","target":"policy","type":"violates"},{"id":"c","source":"d1","target":"policy","type":"supports","weight":0.75}]})).unwrap();
        std::fs::write(&graph, &raw).unwrap();
        let mut process = Running(
            Command::new(env!("CARGO_BIN_EXE_oev2"))
                .arg("--root")
                .arg(&root)
                .args(["services", "http-decisions", "--context-graph"])
                .arg(&graph)
                .args([
                    "--context-graph-sha256",
                    &oe_core::sha256(&raw),
                    "--allow-anonymous",
                    "--resource-profile",
                    resource,
                    "--max-requests",
                    "4",
                    "--duration-ms",
                    "10000",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut output = BufReader::new(process.0.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let ready: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(ready["profile"], "semantica.explorer-decisions-http/1");
        assert_eq!(ready["engineering_adoption"], "not_established");
        let address = ready["address"].as_str().unwrap();
        let decision = body(&get(address, "/api/decisions/d1"));
        assert_eq!(decision["decision_id"], "d1");
        assert_eq!(decision["timestamp"], "1970-01-01T00:00:01+00:00");
        let compliance = body(&get(address, "/api/decisions/d1/compliance"));
        assert_eq!(compliance["compliant"], false);
        assert_eq!(compliance["violations"][0]["policy_id"], "policy");
        let causal = body(&get(
            address,
            "/api/v2/decisions/causal-distance?source=d1&target=policy",
        ));
        assert_eq!(causal["causal_path"], json!(["d1", "policy"]));
        assert_eq!(causal["confidence_decay"], 0.75);
        std::fs::write(&graph, b"changed").unwrap();
        assert!(get(address, "/api/decisions/d1").is_empty());
        assert_eq!(process.0.wait().unwrap().code(), Some(2));
        let mut error = String::new();
        process
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error)
            .unwrap();
        assert!(error.contains("service_source_identity_mismatch"));
    }
}
