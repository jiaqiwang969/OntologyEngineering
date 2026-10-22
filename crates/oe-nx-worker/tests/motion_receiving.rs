use oe_contracts::{NxMotionEvidenceBinding, NxMotionEvidenceBudget, NxMotionEvidenceSummary};
use oe_nx_worker::motion_evidence::{receiving::verify_evidence, *};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
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
fn fixture() -> (NxMotionEvidenceSummary, BTreeMap<String, Vec<u8>>) {
    let dir = tempfile::tempdir().unwrap();
    let mut w = EvidenceWriter::new(
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
        w.append(
            "pair_returned",
            serde_json::json!({"index":n,"result_raw":2,"detail":"x".repeat(180)}),
        )
        .unwrap();
    }
    let summary = w.seal().unwrap();
    let files = summary
        .segments
        .iter()
        .map(|s| {
            (
                s.path.clone(),
                std::fs::read(dir.path().join(&s.path)).unwrap(),
            )
        })
        .collect();
    (summary, files)
}
fn read(
    summary: &NxMotionEvidenceSummary,
    files: &BTreeMap<String, Vec<u8>>,
    names: &[String],
    b: &NxMotionEvidenceBinding,
) -> anyhow::Result<()> {
    verify_evidence(summary, &budget(), b, names, |name, visit| {
        visit(&mut Cursor::new(&files[name]))
    })
}
#[test]
fn original_stream_requires_exact_inventory_binding_chain_and_seal() {
    let (summary, files) = fixture();
    let names = files.keys().cloned().collect::<Vec<_>>();
    assert!(summary.segments.len() > 1);
    read(&summary, &files, &names, &binding()).unwrap();
    for fault in 0..9 {
        let mut s = summary.clone();
        let mut f = files.clone();
        let mut n = names.clone();
        let mut b = binding();
        match fault {
            0 => {
                n.push("motion-events-999999.jsonl".into());
            }
            1 => {
                n.pop();
            }
            2 => s.segments.swap(0, 1),
            3 => {
                f.get_mut(&names[0]).unwrap()[2] ^= 1;
            }
            4 => {
                f.get_mut(names.last().unwrap()).unwrap().pop();
            }
            5 => b.object.revision = "r2".into(),
            6 => s.sealed = false,
            7 => s.events -= 1,
            8 => {
                n[1] = n[0].clone();
            }
            _ => unreachable!(),
        }
        assert!(read(&s, &f, &n, &b).is_err(), "{fault}");
    }
}
#[test]
fn source_callback_and_post_read_failures_never_qualify() {
    let (s, f) = fixture();
    let names = f.keys().cloned().collect::<Vec<_>>();
    assert!(verify_evidence(&s, &budget(), &binding(), &names, |_, _| Ok(())).is_err());
    assert!(
        verify_evidence(&s, &budget(), &binding(), &names, |n, v| {
            v(&mut Cursor::new(&f[n]))?;
            v(&mut Cursor::new(&f[n]))
        })
        .is_err()
    );
    assert!(
        verify_evidence(&s, &budget(), &binding(), &names, |n, v| {
            v(&mut Cursor::new(&f[n]))?;
            anyhow::bail!("pinned_identity_changed")
        })
        .is_err()
    );
    // Even a faulty adapter swallowing visitor failures must not qualify.
    assert!(
        verify_evidence(&s, &budget(), &binding(), &names, |n, v| {
            let _ = v(&mut Cursor::new(&f[n]));
            let _ = v(&mut Cursor::new(&f[n]));
            Ok(())
        })
        .is_err()
    );
    struct MissingEof {
        data: Cursor<Vec<u8>>,
    }
    impl Read for MissingEof {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            let n = self.data.read(b)?;
            if n == 0 {
                Err(std::io::Error::other("missing_verified_eof"))
            } else {
                Ok(n)
            }
        }
    }
    assert!(
        verify_evidence(&s, &budget(), &binding(), &names, |n, v| {
            let _ = v(&mut MissingEof {
                data: Cursor::new(f[n].clone()),
            });
            Ok(())
        })
        .is_err()
    );
    // Early I/O error is distinct from a legitimate EOF.
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("read_failed"))
        }
    }
    assert!(verify_evidence(&s, &budget(), &binding(), &names, |_, v| v(&mut Broken)).is_err());
}

#[test]
fn recomputed_hashes_cannot_hide_missing_seal_wrong_binding_or_bad_sequence() {
    use oe_contracts::{NxMotionEvidenceEvent, NxMotionEvidenceSegment};
    use sha2::{Digest, Sha256};
    for fault in 0..7 {
        let mut phases = vec![
            ("stream_open", serde_json::to_value(binding()).unwrap()),
            (
                "stream_seal",
                serde_json::json!({"committed_before_seal":1}),
            ),
        ];
        match fault {
            0 => {}
            1 => phases[1] = ("pair_returned", serde_json::json!({"query_code":0})),
            2 => phases.push(("extra_after_seal", serde_json::json!({}))),
            3 => phases[0].1["object"]["revision"] = serde_json::json!("wrong"),
            4 => phases[1].1 = serde_json::json!({"committed_before_seal":9}),
            5 => {}
            6 => phases[1] = ("stream_open", serde_json::to_value(binding()).unwrap()),
            _ => unreachable!(),
        }
        let events = phases.len() as u64;
        let mut previous = "0".repeat(64);
        let mut raw = vec![];
        for (i, (phase, observation)) in phases.into_iter().enumerate() {
            let event = NxMotionEvidenceEvent {
                schema: "oev2.nx-motion-evidence-event/1".into(),
                sequence: if fault == 5 && i == 1 {
                    1
                } else {
                    i as u64 + 1
                },
                previous_sha256: previous,
                phase: phase.into(),
                observation,
            };
            let mut line = serde_json::to_vec(&event).unwrap();
            line.push(b'\n');
            previous = format!("{:x}", Sha256::digest(&line));
            raw.extend(line);
        }
        let sha = format!("{:x}", Sha256::digest(&raw));
        let size = raw.len() as u64;
        let name = segment_name(0);
        let summary = NxMotionEvidenceSummary {
            schema: "oev2.nx-motion-evidence-summary/1".into(),
            events,
            bytes: size,
            sha256: sha.clone(),
            sealed: true,
            segments: vec![NxMotionEvidenceSegment {
                index: 0,
                path: name.clone(),
                first_sequence: 1,
                last_sequence: events,
                bytes: size,
                sha256: sha,
            }],
        };
        let files = BTreeMap::from([(name.clone(), raw)]);
        assert_eq!(
            read(&summary, &files, &[name], &binding()).is_ok(),
            fault == 0,
            "{fault}"
        );
    }
}
