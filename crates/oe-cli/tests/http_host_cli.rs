use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Running(Child);
#[cfg(windows)]
#[path = "support/http_windows_fixture.rs"]
mod windows_fixture;
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn get(address: &str, key: Option<&str>) -> String {
    let mut stream = match TcpStream::connect(address) {
        Ok(stream) => stream,
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            return String::new();
        }
        Err(error) => panic!("HTTP connection failed: {error}"),
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let header = key
        .map(|k| format!("X-API-Key: {k}\r\n"))
        .unwrap_or_default();
    stream
        .write_all(
            format!("GET /api/graph/stats HTTP/1.1\r\nHost: localhost\r\n{header}\r\n").as_bytes(),
        )
        .unwrap();
    let mut bytes = Vec::new();
    // A source rejection can close without a response, including TCP reset.
    let result = stream.read_to_end(&mut bytes);
    if let Err(e) = result {
        assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset);
    }
    String::from_utf8(bytes).unwrap()
}

#[cfg(any(unix, windows))]
#[test]
fn real_http_host_rotates_keys_and_rejects_changed_source() {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir_in(root.join("var")).unwrap();
    let input = temp.path().join("graph.json");
    let key = temp.path().join("key");
    let bytes = br#"{"nodes":[],"edges":[]}"#;
    std::fs::write(&input, bytes).unwrap();
    std::fs::write(&key, "synthetic-key-one").unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    #[cfg(windows)]
    windows_fixture::private(&key);
    let mut process = Running(
        Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(&root)
            .args(["services", "http-graph-stats", "--context-graph"])
            .arg(&input)
            .args([
                "--context-graph-sha256",
                &oe_core::sha256(bytes),
                "--max-requests",
                "4",
                "--duration-ms",
                "10000",
                "--api-key-file",
            ])
            .arg(&key)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut output = BufReader::new(process.0.stdout.take().unwrap());
    let mut ready = String::new();
    output.read_line(&mut ready).unwrap();
    let ready: Value = serde_json::from_str(&ready).unwrap();
    let address = ready["address"].as_str().unwrap();
    assert!(get(address, Some("synthetic-key-one")).starts_with("HTTP/1.1 200"));
    std::fs::write(&key, "synthetic-key-two").unwrap();
    assert!(get(address, Some("synthetic-key-one")).starts_with("HTTP/1.1 401"));
    assert!(get(address, Some("synthetic-key-two")).starts_with("HTTP/1.1 200"));
    // Either the preceding post-response check or next pre-response check must reject.
    std::fs::write(&input, b"changed-source").unwrap();
    assert!(get(address, Some("synthetic-key-two")).is_empty());
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
    assert!(!error.contains("synthetic-key"));
}

#[cfg(windows)]
#[test]
fn real_windows_http_host_refuses_unsafe_key_and_recovers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir_in(root.join("var")).unwrap();
    let graph = temp.path().join("graph.json");
    let key = temp.path().join("key");
    let bytes = br#"{"nodes":[],"edges":[]}"#;
    std::fs::write(&graph, bytes).unwrap();
    std::fs::write(&key, "synthetic-key-one").unwrap();
    windows_fixture::private(&key);
    oe_core::private_file::read(&key, 4096).unwrap();
    let mut process = Running(
        Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(&root)
            .args(["services", "http-graph-stats", "--context-graph"])
            .arg(&graph)
            .args([
                "--context-graph-sha256",
                &oe_core::sha256(bytes),
                "--max-requests",
                "6",
                "--duration-ms",
                "10000",
                "--api-key-file",
            ])
            .arg(&key)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut output = BufReader::new(process.0.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let ready: Value = serde_json::from_str(&line).unwrap();
    let address = ready["address"].as_str().unwrap();
    assert!(get(address, Some("synthetic-key-one")).starts_with("HTTP/1.1 200"));
    windows_fixture::grant_other(&key, false);
    assert!(get(address, Some("synthetic-key-one")).starts_with("HTTP/1.1 503"));
    windows_fixture::private(&key);
    assert!(get(address, Some("synthetic-key-one")).starts_with("HTTP/1.1 200"));
    std::fs::remove_file(&key).unwrap();
    assert!(get(address, Some("synthetic-key-one")).starts_with("HTTP/1.1 503"));
    std::fs::write(&key, "synthetic-key-two").unwrap();
    windows_fixture::private(&key);
    assert!(get(address, Some("synthetic-key-one")).starts_with("HTTP/1.1 401"));
    assert!(get(address, Some("synthetic-key-two")).starts_with("HTTP/1.1 200"));
    assert!(process.0.wait().unwrap().success());
    let mut diagnostics = String::new();
    process
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut diagnostics)
        .unwrap();
    output.read_to_string(&mut diagnostics).unwrap();
    assert!(!diagnostics.contains("synthetic-key"));
}

#[test]
fn real_http_host_is_source_bound_and_explicitly_authorized() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir_in(root.join("var")).unwrap();
    let input = temp.path().join("graph.json");
    let bytes = br#"{"nodes":[],"edges":[]}"#;
    std::fs::write(&input, bytes).unwrap();
    for anonymous in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
        command
            .arg("--root")
            .arg(&root)
            .args(["services", "http-graph-stats", "--context-graph"])
            .arg(&input)
            .args([
                "--context-graph-sha256",
                &oe_core::sha256(bytes),
                "--max-requests",
                "1",
                "--duration-ms",
                "5000",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if anonymous {
            command.arg("--allow-anonymous");
        }
        let mut process = Running(command.spawn().unwrap());
        let mut output = BufReader::new(process.0.stdout.take().unwrap());
        let mut ready = String::new();
        output.read_line(&mut ready).unwrap();
        assert!(!ready.is_empty(), "host returned no ready event");
        let ready: Value = serde_json::from_str(&ready).unwrap();
        assert_eq!(ready["source_sha256"], oe_core::sha256(bytes));
        let mut socket = TcpStream::connect(ready["address"].as_str().unwrap()).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .write_all(b"GET /api/graph/stats HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(
            response.starts_with(if anonymous {
                "HTTP/1.1 200"
            } else {
                "HTTP/1.1 503"
            }),
            "{response}"
        );
        if anonymous {
            let body: Value =
                serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["node_count"], 0);
            assert_eq!(body["edge_count"], 0);
        }
        assert!(process.0.wait().unwrap().success());
    }
    let rejected = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(&root)
        .args(["services", "http-graph-stats", "--context-graph"])
        .arg(&input)
        .args([
            "--context-graph-sha256",
            &"0".repeat(64),
            "--allow-anonymous",
        ])
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(2));
    assert!(rejected.stdout.is_empty());
    assert!(
        String::from_utf8(rejected.stderr)
            .unwrap()
            .contains("service_source_identity_mismatch")
    );
}
