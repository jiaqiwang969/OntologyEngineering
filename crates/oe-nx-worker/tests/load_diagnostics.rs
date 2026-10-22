use oe_nx_worker::load_diagnostics::persist_batch;
use serde_json::{Value, json};
fn bytes(q: &[Value]) -> usize {
    q.iter().map(|v| serde_json::to_vec(v).unwrap().len()).sum()
}
#[test]
fn complete_batches_exceed_legacy_total_without_growing_pending_queue() {
    let mut q = vec![];
    let mut used = 0;
    let mut received = 0;
    let mut total = 0;
    for n in 0..3027 {
        q.push(json!({"ordinal":n,"raw":"x".repeat(1024)}));
        used = bytes(&q);
        total += used;
        assert!(used < 512 * 1024);
        persist_batch(&mut q, &mut used, &mut |batch| {
            assert_eq!(batch["observations"][0]["ordinal"], received);
            assert_eq!(
                batch["observations"][0]["raw"].as_str().unwrap().len(),
                1024
            );
            received += 1;
            Ok(())
        })
        .unwrap();
        assert!(q.is_empty());
        assert_eq!(used, 0);
    }
    assert_eq!(received, 3027);
    assert!(total > 512 * 1024);
}
#[test]
fn failed_writer_retains_exact_batch_and_accounting_for_recovery() {
    let mut q = vec![
        json!({"return_code":19,"native_error":"original failure"}),
        json!({"phase":"cleanup","return_code":0}),
    ];
    let original = q.clone();
    let mut used = bytes(&q);
    let count = used;
    assert!(persist_batch(&mut q, &mut used, &mut |_| anyhow::bail!("disk_full")).is_err());
    assert_eq!(q, original);
    assert_eq!(used, count);
    persist_batch(&mut q, &mut used, &mut |b| {
        assert_eq!(b["observations"], json!(original));
        Ok(())
    })
    .unwrap();
    assert_eq!(used, 0);
    assert!(q.is_empty());
    let mut empty = vec![];
    let mut wrong = 1;
    assert!(persist_batch(&mut empty, &mut wrong, &mut |_| panic!("not called")).is_err());
}
