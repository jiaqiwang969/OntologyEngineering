use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
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

#[test]
fn all_http_hosts_initialize_before_announcing_ready() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let corpus: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/explorer_initialization.json"
    ))
    .unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 42);
    for command in [
        "http-decisions",
        "http-search",
        "http-graph-read",
        "http-graph-stats",
        "http-path",
        "http-sparql",
        "http-sparql-v2",
    ] {
        for row in rows {
            let temp = tempfile::tempdir_in(root.join("var")).unwrap();
            let graph = temp.path().join("graph.json");
            let bytes = serde_json::to_vec(&row["graph"]).unwrap();
            std::fs::write(&graph, &bytes).unwrap();
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_oev2"));
            cmd.arg("--root")
                .arg(&root)
                .args(["services", command, "--context-graph"])
                .arg(&graph)
                .args([
                    "--context-graph-sha256",
                    &oe_core::sha256(&bytes),
                    "--allow-anonymous",
                    "--max-requests",
                    "1",
                    "--duration-ms",
                    "10000",
                ]);
            if ["http-path", "http-sparql", "http-sparql-v2"].contains(&command) {
                cmd.arg("--evidence").arg(temp.path().join("evidence"));
            }
            if ["http-path", "http-sparql-v2"].contains(&command) {
                cmd.args(["--resource-profile", "semantica.dataset-default/1"]);
            }
            let mut child = Running(
                cmd.stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let mut output = BufReader::new(child.0.stdout.take().unwrap());
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            if row["result"]["admitted"] == true {
                let ready: Value = serde_json::from_str(&line)
                    .unwrap_or_else(|e| panic!("{command} {}: {e}; {line}", row["id"]));
                assert_eq!(ready["schema"], "semantica.http-host-ready/1");
                let mut stream = TcpStream::connect(ready["address"].as_str().unwrap()).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                stream.write_all(b"GET /api/missing HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                assert!(response.starts_with("HTTP/1.1 404"));
                assert!(child.0.wait().unwrap().success(), "{command} {}", row["id"]);
            } else {
                assert!(
                    line.is_empty(),
                    "{command} {} announced ready: {line}",
                    row["id"]
                );
                assert_eq!(child.0.wait().unwrap().code(), Some(2));
                let mut error = String::new();
                child
                    .0
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut error)
                    .unwrap();
                let expected = if row["result"]["error_type"] == "OverflowError" {
                    "explorer_search_integer_float_overflow"
                } else {
                    "explorer_search_tags_not_iterable"
                };
                assert!(error.contains(expected), "{command} {}: {error}", row["id"]);
            }
            assert_eq!(std::fs::read(&graph).unwrap(), bytes);
        }
    }
}
