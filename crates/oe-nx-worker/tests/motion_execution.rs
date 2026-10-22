use oe_nx_worker::assembly_motion::{InterferenceStatus, execution::*};
#[derive(Default)]
struct Fake {
    query_code: i32,
    raw: i32,
    cleanup: i32,
    calls: Vec<&'static str>,
}
impl InterferenceApi for Fake {
    fn query(&mut self, _: u32, _: u32) -> (i32, i32) {
        self.calls.push("query");
        (self.query_code, self.raw)
    }
    fn cleanup_update(&mut self) -> i32 {
        self.calls.push("cleanup");
        self.cleanup
    }
}
#[test]
fn issued_queries_always_cleanup_and_preserve_original_codes() {
    for (q, r, c, valid) in [
        (0, 1, 0, true),
        (0, 2, 0, true),
        (0, 3, 0, true),
        (7, 2, 0, false),
        (0, 0, 0, false),
        (0, 2, 9, false),
        (7, -1, 9, false),
    ] {
        let mut api = Fake {
            query_code: q,
            raw: r,
            cleanup: c,
            ..Default::default()
        };
        let mut events = vec![];
        let result = check_pair(&mut api, 101, 102, |e| {
            events.push(e);
            Ok(())
        });
        assert_eq!(result.is_ok(), valid);
        assert_eq!(api.calls, vec!["query", "cleanup"]);
        assert_eq!(
            events,
            vec![
                PairEvent::Pending { a: 101, b: 102 },
                PairEvent::Returned(PairReceipt {
                    a: 101,
                    b: 102,
                    query_code: q,
                    result_raw: r,
                    cleanup_code: c
                })
            ]
        );
        if r == 3 && valid {
            assert_eq!(result.unwrap().1, InterferenceStatus::Touching);
        }
    }
}
#[test]
fn durable_record_failure_stops_work_but_never_skips_owed_cleanup() {
    let mut api = Fake {
        raw: 2,
        ..Default::default()
    };
    assert!(
        check_pair(&mut api, 101, 102, |_| anyhow::bail!(
            "pending_write_failed"
        ))
        .is_err()
    );
    assert!(api.calls.is_empty());
    assert!(
        check_pair(&mut api, 101, 102, |e| match e {
            PairEvent::Pending { .. } => Ok(()),
            _ => anyhow::bail!("returned_write_failed"),
        })
        .is_err()
    );
    assert_eq!(api.calls, vec!["query", "cleanup"]);
}
