use semantica_contracts::SemanticServiceBudgetCharge;
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget,
    explorer_http::{self, HostAuthorization},
    explorer_path_http, explorer_read_http, explorer_search_http, explorer_sparql_http,
};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    time::{Duration, Instant},
};
fn exchange(
    route: usize,
    method: &str,
    path: &str,
    cap: u64,
) -> (Vec<u8>, std::io::Result<()>, u64) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let raw = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let client = std::thread::spawn(move || {
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(raw.as_bytes()).unwrap();
        s.shutdown(Shutdown::Write).unwrap();
        let mut b = vec![];
        s.read_to_end(&mut b).unwrap();
        b
    });
    let stream = listener.accept().unwrap().0;
    let mut session = DatasetSession::new();
    let budget = NativeServiceBudget::new(0, 100000, cap);
    let service = McpService::new(&mut session, &budget);
    let auth = |_: Option<&str>| -> HostAuthorization {
        panic!("method/route rejection must precede authorization")
    };
    let d = Instant::now() + Duration::from_secs(3);
    let result = match route {
        0 => explorer_http::serve_connection(Some(&service), stream, &auth, Duration::from_secs(3)),
        1 => explorer_read_http::serve_connection(&service, stream, &auth, d, &|| {
            panic!("must not execute")
        }),
        2 => explorer_search_http::serve_connection(&service, stream, &auth, d, &|| {
            panic!("must not execute")
        }),
        3 => explorer_path_http::serve_connection(&service, stream, &auth, d, 100, |_, _| {
            panic!("must not execute")
        }),
        4 => explorer_sparql_http::serve_connection(&service, stream, &auth, d, |_, _| {
            panic!("must not execute")
        }),
        5 => explorer_sparql_http::serve_connection_v2(&service, stream, &auth, d, |_, _| {
            panic!("must not execute")
        }),
        _ => unreachable!(),
    };
    let counts = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(counts.calls, 0);
    (client.join().unwrap(), result, counts.output_bytes)
}
#[test]
fn head_wire_matches_original_status_length_and_no_body_across_all_adapters() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/explorer_head_wire.json")).unwrap();
    for (route, path) in [
        "/api/graph/stats",
        "/api/graph/nodes",
        "/api/graph/search",
        "/api/graph/path",
        "/api/sparql",
        "/api/sparql",
    ]
    .iter()
    .enumerate()
    {
        let expected = original["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["method"] == "HEAD" && r["path"] == *path)
            .unwrap();
        let (wire, result, charged) = exchange(route, "HEAD", path, 100000);
        result.unwrap();
        let split = wire.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
        let header = std::str::from_utf8(&wire[..split]).unwrap();
        assert_eq!(
            header
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            expected["status"].as_u64().unwrap()
        );
        assert!(wire[split + 4..].is_empty());
        assert_eq!(expected["body_utf8"], "");
        let original_header = expected["header_utf8"]
            .as_str()
            .unwrap()
            .to_ascii_lowercase();
        let length = original_header
            .lines()
            .find(|l| l.starts_with("content-length:"))
            .unwrap();
        assert!(header.to_ascii_lowercase().lines().any(|l| l == length));
        assert_eq!(charged, wire.len() as u64);
        let (exact, r, bytes) = exchange(route, "HEAD", path, wire.len() as u64);
        r.unwrap();
        assert_eq!(exact, wire);
        assert_eq!(bytes, wire.len() as u64);
        let (empty, r, _) = exchange(route, "HEAD", path, wire.len() as u64 - 1);
        assert!(r.is_err());
        assert!(empty.is_empty());
        let (control, r, charged) = exchange(route, "OPTIONS", path, 100000);
        r.unwrap();
        assert!(control.ends_with(b"{\"detail\":\"Method Not Allowed\"}"));
        assert_eq!(charged, control.len() as u64);
        let (missing, r, charged) = exchange(route, "HEAD", "/api/missing", 100000);
        r.unwrap();
        assert!(missing.starts_with(b"HTTP/1.1 404 "));
        assert!(missing.ends_with(b"\r\n\r\n"));
        assert_eq!(charged, missing.len() as u64);
    }
}
