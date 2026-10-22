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
fn post(address: &str, query: &Value) -> Vec<u8> {
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
    let body = serde_json::to_vec(query).unwrap();
    let mut raw=format!("POST /api/graph/search HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()).into_bytes();
    raw.extend(body);
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
fn actual_search_cli_uses_selected_session_and_rejects_source_drift() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    for resource in ["semantica.dataset-default/1", "semantica.context-50k/1"] {
        let temp = tempfile::tempdir_in(root.join("var")).unwrap();
        let graph = temp.path().join("graph.json");
        let raw=serde_json::to_vec(&json!({"nodes":[{"id":"a","type":"Part","content":"Bearing A"},{"id":"b","type":"Part","content":"Bearing B"},{"id":"c","content":"frame"}],"edges":[{"id":"cb","source":"c","target":"b","weight":1.0},{"id":"ba","source":"b","target":"a","weight":1.0}]})).unwrap();
        std::fs::write(&graph, &raw).unwrap();
        let mut process = Running(
            Command::new(env!("CARGO_BIN_EXE_oev2"))
                .arg("--root")
                .arg(&root)
                .args(["services", "http-search", "--context-graph"])
                .arg(&graph)
                .args([
                    "--context-graph-sha256",
                    &oe_core::sha256(&raw),
                    "--allow-anonymous",
                    "--resource-profile",
                    resource,
                    "--max-requests",
                    "3",
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
        assert_eq!(ready["profile"], "semantica.explorer-search-http/1");
        assert_eq!(ready["engineering_adoption"], "not_established");
        let address = ready["address"].as_str().unwrap();
        let filtered = body(&post(
            address,
            &json!({"query":"bearing","anchor_node":"c","max_hops":1}),
        ));
        assert_eq!(filtered["total"], 1);
        assert_eq!(filtered["results"][0]["node"]["id"], "b");
        assert_eq!(filtered["results"][0]["hop_distance"], 1);
        let all = body(&post(address, &json!({"query":"bearing"})));
        assert_eq!(all["total"], 2);
        assert_eq!(all["results"][0]["node"]["id"], "b");
        assert_eq!(all["results"][1]["node"]["id"], "a");
        std::fs::write(&graph, b"changed").unwrap();
        assert!(post(address, &json!({"query":"bearing"})).is_empty());
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
