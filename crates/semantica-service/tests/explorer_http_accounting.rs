//! Integration expectations for the two independently reproduced HTTP defects.
//! These tests use the real shared service budget and Dataset, with no model calls.
use semantica_contracts::SemanticServiceBudgetCharge;
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget,
    explorer_http::{HostAuthorization, MAX_HEADER_BYTES, serve_connection},
    load_context_graph,
};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

#[test]
fn partial_header_timeout_retains_actual_shared_budget_charge() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let mut socket = TcpStream::connect(address).unwrap();
        socket.write_all(b"GET /").unwrap();
        ready_tx.send(()).unwrap();
        done_rx.recv().unwrap();
    });
    let (socket, _) = listener.accept().unwrap();
    ready_rx.recv().unwrap();
    let mut dataset = DatasetSession::new();
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, 5, 1_000);
    let service = McpService::new(&mut dataset, &budget);
    let auth_calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        auth_calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let outcome = serve_connection(Some(&service), socket, &auth, Duration::from_millis(50));
    assert!(outcome.is_err());
    assert_eq!(auth_calls.load(Ordering::SeqCst), 0);
    let receipt = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(receipt.input_bytes, 5);
    assert_eq!(receipt.calls, 0);
    assert_eq!(receipt.output_bytes, 0);
    assert!(
        budget
            .charge_claim(&SemanticServiceBudgetCharge {
                kind: "framing".into(),
                bytes: 1,
            })
            .is_err()
    );
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
    done_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn valid_exact_cap_header_with_actual_peer_eof_delivers_complete_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let mut request = b"GET /api/graph/stats HTTP/1.1\r\nHost: localhost\r\nX-Pad: ".to_vec();
        request.resize(MAX_HEADER_BYTES - 4, b'a');
        request.extend_from_slice(b"\r\n\r\n");
        assert_eq!(request.len(), MAX_HEADER_BYTES);
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket.write_all(&request).unwrap();
        socket.shutdown(Shutdown::Write).unwrap();
        ready_tx.send(()).unwrap();
        let mut wire = Vec::new();
        socket.read_to_end(&mut wire).unwrap();
        wire
    });
    let (socket, _) = listener.accept().unwrap();
    ready_rx.recv().unwrap();
    let mut dataset = DatasetSession::new();
    load_context_graph(&mut dataset, &json!({"nodes": [], "edges": []})).unwrap();
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, MAX_HEADER_BYTES as u64, 100_000);
    let service = McpService::new(&mut dataset, &budget);
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    serve_connection(Some(&service), socket, &auth, Duration::from_secs(2)).unwrap();
    let wire = client.join().unwrap();
    let split = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = std::str::from_utf8(&wire[..split]).unwrap();
    assert!(head.starts_with("HTTP/1.1 200 "));
    assert!(head.contains("X-Semantica-Stats-Scope: canonical-context-records"));
    assert!(head.contains("X-Semantica-Historical-Index: not-reconstructed"));
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(length, wire.len() - split - 4);
    let body: serde_json::Value = serde_json::from_slice(&wire[split + 4..]).unwrap();
    assert_eq!(
        body,
        json!({"node_count": 0, "edge_count": 0, "node_types": {}, "edge_types": {}, "density": 0.0})
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let receipt = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(receipt.input_bytes, MAX_HEADER_BYTES as u64);
    assert_eq!(receipt.calls, 1);
    assert_eq!(receipt.output_bytes, wire.len() as u64);
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
}
