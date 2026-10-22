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
fn get(address: &str, target: &str) -> Vec<u8> {
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
    let raw = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    if let Err(e) = stream.write_all(raw.as_bytes()) {
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
fn actual_read_cli_preserves_ids_and_rejects_source_drift() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir_in(root.join("var")).unwrap();
    let graph = temp.path().join("graph.json");
    let raw=serde_json::to_vec(&json!({"nodes":[{"id":"a%2Fb","type":"Part","content":"Encoded ID"},{"id":"z","type":"Part","content":"Last"}],"edges":[]})).unwrap();
    std::fs::write(&graph, &raw).unwrap();
    let mut process = Running(
        Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(&root)
            .args(["services", "http-graph-read", "--context-graph"])
            .arg(&graph)
            .args([
                "--context-graph-sha256",
                &oe_core::sha256(&raw),
                "--allow-anonymous",
                "--resource-profile",
                "semantica.context-50k/1",
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
    assert_eq!(ready["profile"], "semantica.explorer-graph-read-http/1");
    assert_eq!(ready["engineering_adoption"], "not_established");
    let address = ready["address"].as_str().unwrap();
    let page = body(&get(address, "/api/graph/nodes?limit=1"));
    assert_eq!(page["nodes"][0]["id"], "a%2Fb");
    assert_eq!(page["total"], 2);
    assert_eq!(page["has_more"], true);
    assert_eq!(
        body(&get(address, "/api/graph/node/a%252Fb"))["content"],
        "Encoded ID"
    );
    std::fs::write(&graph, b"changed").unwrap();
    assert!(get(address, "/api/graph/nodes").is_empty());
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
