//! Bounded installation smoke workload; no performance comparison is claimed.
//! Compile with the toolchain declared by the repository, then redirect stdout
//! to a private JSON file for Flameox's `summarize_benchmarks` operation.
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let n = 20_000_u64;
    let expected = n * (n - 1) / 2;
    let mut samples = Vec::new();
    for _ in 0..8 {
        let start = Instant::now();
        let mut sum = 0_u64;
        for value in 0..black_box(n) {
            sum = black_box(sum + black_box(value));
        }
        samples.push(start.elapsed().as_nanos());
        assert_eq!(black_box(sum), expected);
    }
    let values = samples.iter().map(u128::to_string).collect::<Vec<_>>().join(",");
    println!(
        "{{\"schema_version\":\"flameox.benchmark-samples.v1\",\"producer\":\"ontology-engineering-runtime-smoke\",\"producer_version\":\"1\",\"benchmarks\":[{{\"name\":\"integer_sum\",\"unit\":\"ns\",\"measurement_clock\":\"host_monotonic\",\"synchronization\":\"not_required\",\"samples\":[{values}]}}]}}"
    );
}
