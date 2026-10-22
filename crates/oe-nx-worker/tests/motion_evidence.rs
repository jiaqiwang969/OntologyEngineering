use oe_contracts::{NxMotionEvidenceBinding, NxMotionEvidenceBudget, NxMotionEvidenceEvent};
use oe_nx_worker::motion_evidence::*;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
fn binding() -> NxMotionEvidenceBinding {
    serde_json::from_value(serde_json::json!({"job_id":"motion-test","object":{"object_id":"assembly","revision":"r1","condition_scope":"fixture","source_sha256":"a".repeat(64)},"intent":{"path":"motion.json","sha256":"b".repeat(64)},"samples":2})).unwrap()
}
fn budget() -> NxMotionEvidenceBudget {
    NxMotionEvidenceBudget {
        max_events: 100,
        max_bytes: 32768,
        max_event_bytes: 1024,
        segment_bytes: 2048,
        max_segments: 16,
    }
}
#[test]
fn segments_reconstruct_exact_durable_events() {
    let dir = tempfile::tempdir().unwrap();
    let mut writer = EvidenceWriter::new(
        |i| {
            Ok(std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.path().join(segment_name(i)))?)
        },
        budget(),
        binding(),
    )
    .unwrap();
    for n in 0..18 {
        writer.append("pair_returned",serde_json::json!({"pair_index":n,"query_code":0,"result_raw":2,"cleanup_code":0,"detail":"x".repeat(180)})).unwrap();
    }
    let summary = writer.seal().unwrap();
    assert!(summary.sealed && summary.segments.len() > 1);
    assert_eq!(summary.events, 20);
    let mut all = vec![];
    let mut count = 0;
    let mut previous = "0".repeat(64);
    for (index, s) in summary.segments.iter().enumerate() {
        let bytes = std::fs::read(dir.path().join(&s.path)).unwrap();
        assert_eq!(s.index, index as u64);
        assert_eq!(s.bytes, bytes.len() as u64);
        assert_eq!(s.sha256, format!("{:x}", Sha256::digest(&bytes)));
        assert!(s.bytes <= 2048);
        assert_eq!(s.first_sequence, count + 1);
        for line in bytes.split_inclusive(|b| *b == b'\n') {
            assert_eq!(line.last(), Some(&b'\n'));
            let e: NxMotionEvidenceEvent = serde_json::from_slice(line).unwrap();
            count += 1;
            assert_eq!(e.sequence, count);
            assert_eq!(e.previous_sha256, previous);
            previous = format!("{:x}", Sha256::digest(line));
        }
        assert_eq!(s.last_sequence, count);
        all.extend_from_slice(&bytes);
    }
    assert_eq!(summary.bytes, all.len() as u64);
    assert_eq!(summary.sha256, format!("{:x}", Sha256::digest(&all)));
    assert_eq!(summary.events, count);
    assert!(writer.append("after_seal", serde_json::json!({})).is_err());
}
#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    fail_write: bool,
    fail_sync: bool,
    partial_written: bool,
    syncs: u64,
}
struct Faulty(Arc<Mutex<State>>);
impl Write for Faulty {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let mut s = self.0.lock().unwrap();
        if s.fail_write {
            if s.partial_written {
                return Err(io::Error::other("injected_partial_write"));
            }
            let n = b.len().min(7);
            s.bytes.extend_from_slice(&b[..n]);
            s.partial_written = true;
            return Ok(n);
        }
        s.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl DurableWrite for Faulty {
    fn synchronize(&mut self) -> io::Result<()> {
        let mut s = self.0.lock().unwrap();
        if s.fail_sync {
            return Err(io::Error::other("injected_sync_failure"));
        }
        s.syncs += 1;
        Ok(())
    }
}
#[test]
fn write_sync_and_budget_failures_cannot_be_sealed() {
    for fault in 0..3 {
        let state = Arc::new(Mutex::new(State::default()));
        let copy = state.clone();
        let mut writer =
            EvidenceWriter::new(move |_| Ok(Faulty(copy.clone())), budget(), binding()).unwrap();
        assert_eq!(writer.committed_events(), 1);
        assert_eq!(state.lock().unwrap().syncs, 1);
        if fault == 0 {
            state.lock().unwrap().fail_write = true;
        }
        if fault == 1 {
            state.lock().unwrap().fail_sync = true;
        }
        let value = if fault == 2 {
            serde_json::json!({"too_big":"x".repeat(2048)})
        } else {
            serde_json::json!({"query_code":0})
        };
        assert!(writer.append("pair_returned", value).is_err());
        assert!(writer.poisoned());
        assert_eq!(writer.committed_events(), 1);
        let size = state.lock().unwrap().bytes.len();
        assert!(writer.append("retry", serde_json::json!({})).is_err());
        assert!(writer.seal().is_err());
        assert_eq!(state.lock().unwrap().bytes.len(), size);
    }
}

#[test]
fn task_sink_reopens_originals_and_verification_failure_is_terminal() {
    use receiving::verify_evidence;
    use sink::{EvidenceSink, VerifiedSink, record_native};
    for corrupt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let writer = EvidenceWriter::new(
            |i| {
                Ok(std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dir.path().join(segment_name(i)))?)
            },
            budget(),
            binding(),
        )
        .unwrap();
        let calls = std::cell::Cell::new(0);
        let mut concrete =
            VerifiedSink::new(writer, |summary: &oe_contracts::NxMotionEvidenceSummary| {
                calls.set(calls.get() + 1);
                if corrupt {
                    std::fs::write(dir.path().join(segment_name(0)), b"corrupt")?;
                }
                let names = std::fs::read_dir(dir.path())?
                    .map(|e| Ok(e?.file_name().into_string().unwrap()))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                verify_evidence(summary, &budget(), &binding(), &names, |name, visit| {
                    visit(&mut std::fs::File::open(dir.path().join(name))?)
                })
            });
        let sink: &mut dyn EvidenceSink = &mut concrete;
        record_native(sink,serde_json::json!({"phase":"interference_returned","query_code":0,"result_raw":3,"cleanup_code":0})).unwrap();
        let result = sink.seal_and_verify();
        assert_eq!(result.is_err(), corrupt);
        if let Ok(summary) = result {
            assert_eq!(summary.events, 3);
        }
        assert!(sink.seal_and_verify().is_err());
        assert!(sink.append("retry", serde_json::json!({})).is_err());
        assert_eq!(calls.get(), 1);
    }
}

#[test]
fn task_sink_write_failure_cannot_reach_verifier() {
    use sink::{EvidenceSink, VerifiedSink, record_native};
    let state = Arc::new(Mutex::new(State::default()));
    let copy = state.clone();
    let writer =
        EvidenceWriter::new(move |_| Ok(Faulty(copy.clone())), budget(), binding()).unwrap();
    let calls = std::cell::Cell::new(0);
    let mut sink = VerifiedSink::new(writer, |_: &oe_contracts::NxMotionEvidenceSummary| {
        calls.set(calls.get() + 1);
        Ok(())
    });
    state.lock().unwrap().fail_sync = true;
    assert!(
        record_native(
            &mut sink,
            serde_json::json!({"phase":"interference_pending"})
        )
        .is_err()
    );
    assert!(sink.seal_and_verify().is_err());
    assert_eq!(calls.get(), 0);
}
