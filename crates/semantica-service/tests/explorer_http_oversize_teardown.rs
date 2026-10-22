//! Actual local Rust service controls; the original nine/accounting tests stay unchanged.
use semantica_contracts::SemanticServiceBudgetCharge;
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget,
    explorer_http::{HostAuthorization, MAX_HEADER_BYTES, serve_connection},
    load_context_graph,
};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn counts(budget: &NativeServiceBudget) -> (u64, u64, u64) {
    let r = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    (r.calls, r.input_bytes, r.output_bytes)
}
fn header() -> Vec<u8> {
    b"GET /api/graph/stats HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec()
}
fn response(raw: &[u8]) -> (u16, Value) {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("full response header");
    let head = std::str::from_utf8(&raw[..split]).unwrap();
    assert!(head.contains("Connection: close"));
    assert!(head.contains("X-Semantica-Stats-Scope: canonical-context-records"));
    assert!(head.contains("X-Semantica-Historical-Index: not-reconstructed"));
    let length: usize = head
        .lines()
        .find_map(|s| s.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        length,
        raw.len() - split - 4,
        "response prefix cannot pass as complete body"
    );
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_slice(&raw[split + 4..]).unwrap(),
    )
}

struct Exchange {
    wire: Vec<u8>,
    result: io::Result<()>,
    elapsed: Duration,
}
fn exchange(
    service: &McpService<'_>,
    raw: Vec<u8>,
    auth: &impl Fn(Option<&str>) -> HostAuthorization,
    half_close: bool,
    read_delay: Duration,
    response_write_disabled: bool,
) -> Exchange {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let reset_allowed = raw.len() > MAX_HEADER_BYTES
            || raw
                .windows(b"HTTP/1.1".len())
                .filter(|w| *w == b"HTTP/1.1")
                .count()
                > 1;
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        peer.write_all(&raw).unwrap();
        if half_close {
            peer.shutdown(Shutdown::Write).unwrap();
        }
        ready_tx.send(()).unwrap();
        std::thread::sleep(read_delay);
        let mut wire = Vec::new();
        let read = peer.read_to_end(&mut wire);
        // Never mask a timeout. Error cases can reset while aborting the single
        // connection; full-body controls still check Content-Length and JSON.
        if let Err(error) = read {
            assert!(reset_allowed, "ordinary full request must reach clean EOF");
            assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
            eprintln!(
                "actual peer terminal {:?}, {} bytes",
                error.kind(),
                wire.len()
            );
        }
        // Keep the peer alive until the actual service terminal. This makes the
        // preexisting local write-shutdown control independent of client drop.
        done_rx.recv().unwrap();
        wire
    });
    let (stream, _) = listener.accept().unwrap();
    ready_rx.recv().unwrap();
    if response_write_disabled {
        stream.shutdown(Shutdown::Write).unwrap();
    }
    let started = Instant::now();
    let result = serve_connection(Some(service), stream, auth, Duration::from_secs(2));
    let elapsed = started.elapsed();
    done_tx.send(()).unwrap();
    let wire = client.join().unwrap();
    Exchange {
        wire,
        result,
        elapsed,
    }
}

fn check_oversize(half_close: bool, read_delay: Duration) {
    let mut dataset = DatasetSession::new();
    load_context_graph(&mut dataset, &json!({"nodes":[],"edges":[]})).unwrap();
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, MAX_HEADER_BYTES as u64, 100_000);
    let service = McpService::new(&mut dataset, &budget);
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let got = exchange(
        &service,
        vec![b'x'; MAX_HEADER_BYTES + 1],
        &auth,
        half_close,
        read_delay,
        false,
    );
    got.result.unwrap();
    assert!(got.elapsed < Duration::from_secs(2));
    assert_eq!(
        response(&got.wire),
        (
            431,
            json!({"detail":"HTTP headers exceed finite profile limit"})
        )
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        counts(&budget),
        (0, MAX_HEADER_BYTES as u64, got.wire.len() as u64)
    );
    // The queued extra byte has not acquired a framing allowance.
    assert!(
        budget
            .charge_claim(&SemanticServiceBudgetCharge {
                kind: "framing".into(),
                bytes: 1
            })
            .is_err()
    );
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
}
#[test]
fn cap_plus_one_closes_locally_and_delivers_full_431_with_exact_shared_ledger() {
    check_oversize(true, Duration::ZERO);
}
#[test]
fn cap_plus_one_does_not_require_a_peer_write_halfclose_or_extra_input_budget() {
    check_oversize(false, Duration::ZERO);
}
#[test]
fn delayed_peer_read_does_not_change_cap_or_absolute_connection_deadline() {
    check_oversize(true, Duration::from_millis(30));
}
#[test]
fn pipelined_excess_at_total_cap_is_never_a_second_dispatch() {
    let mut dataset = DatasetSession::new();
    load_context_graph(&mut dataset, &json!({"nodes":[],"edges":[]})).unwrap();
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, MAX_HEADER_BYTES as u64, 100_000);
    let service = McpService::new(&mut dataset, &budget);
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let mut raw = header();
    raw.extend(header());
    raw.resize(MAX_HEADER_BYTES + 1, b'x');
    let got = exchange(&service, raw, &auth, true, Duration::ZERO, false);
    got.result.unwrap();
    assert_eq!(
        response(&got.wire),
        (
            200,
            json!({"node_count":0,"edge_count":0,"node_types":{},"edge_types":{},"density":0.0})
        )
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        counts(&budget),
        (1, MAX_HEADER_BYTES as u64, got.wire.len() as u64)
    );
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
}
#[test]
fn malformed_request_is_rejected_before_auth_or_graph_session_work() {
    let mut dataset = DatasetSession::new(); // Uninitialized: malformed input must not enter it.
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, 3, 100_000);
    let service = McpService::new(&mut dataset, &budget);
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let got = exchange(
        &service,
        b"bad".to_vec(),
        &auth,
        true,
        Duration::ZERO,
        false,
    );
    got.result.unwrap();
    assert_eq!(
        response(&got.wire),
        (400, json!({"detail":"Invalid HTTP framing"}))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(counts(&budget), (0, 3, got.wire.len() as u64));
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
}
#[test]
fn response_write_failure_cannot_be_normalized_as_receive_already_closed() {
    let mut dataset = DatasetSession::new();
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, MAX_HEADER_BYTES as u64, 100_000);
    let service = McpService::new(&mut dataset, &budget);
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let got = exchange(
        &service,
        vec![b'x'; MAX_HEADER_BYTES + 1],
        &auth,
        false,
        Duration::ZERO,
        true,
    );
    let error = got.result.unwrap_err();
    eprintln!(
        "actual response write failure {:?} raw {:?}",
        error.kind(),
        error.raw_os_error()
    );
    assert!(matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected | io::ErrorKind::ConnectionReset
    ));
    assert!(got.wire.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let ledger = counts(&budget);
    assert_eq!((ledger.0, ledger.1), (0, MAX_HEADER_BYTES as u64));
    assert!(ledger.2 > 0);
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
}
#[test]
fn output_budget_failure_before_response_stays_failure() {
    let mut dataset = DatasetSession::new();
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, MAX_HEADER_BYTES as u64, 0);
    let service = McpService::new(&mut dataset, &budget);
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let got = exchange(
        &service,
        vec![b'x'; MAX_HEADER_BYTES + 1],
        &auth,
        true,
        Duration::ZERO,
        false,
    );
    assert_eq!(
        got.result.unwrap_err().to_string(),
        "http_service_output_budget"
    );
    assert!(got.wire.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(counts(&budget), (0, MAX_HEADER_BYTES as u64, 0));
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
}
#[test]
fn smaller_host_input_budget_is_not_bypassed_by_cap_teardown() {
    let mut dataset = DatasetSession::new();
    let before = dataset.snapshot().unwrap();
    let revision = dataset.revision();
    let budget = NativeServiceBudget::new(1, (MAX_HEADER_BYTES - 1) as u64, 100_000);
    let service = McpService::new(&mut dataset, &budget);
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let got = exchange(
        &service,
        vec![b'x'; MAX_HEADER_BYTES + 1],
        &auth,
        true,
        Duration::ZERO,
        false,
    );
    assert_eq!(
        got.result.unwrap_err().to_string(),
        "http_service_input_budget"
    );
    assert!(got.wire.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(counts(&budget), (0, (MAX_HEADER_BYTES - 1) as u64, 0));
    drop(service);
    assert_eq!(dataset.revision(), revision);
    assert_eq!(dataset.snapshot().unwrap(), before);
}
