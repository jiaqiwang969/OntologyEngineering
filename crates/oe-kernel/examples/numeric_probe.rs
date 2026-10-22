//! Independent arithmetic probe. Expectations are ordinary balance arithmetic.
fn main() -> anyhow::Result<()> {
    let store = oe_kernel::graph::load(b"")?;
    for query in [
        "SELECT (120.0 - 100.0 - 15.0 - 5.0 AS ?balance) WHERE {}",
        "SELECT (ABS(120.0 - 100.0 - 15.0 - 5.0) AS ?balance) WHERE {}",
        "SELECT (5.0 + 3.0 - 6.0 - 2.0 AS ?balance) WHERE {}",
        "SELECT (?gross - ?effective - ?retained - ?other AS ?balance) WHERE { VALUES (?gross ?effective ?retained ?other) {(120.0 100.0 15.0 5.0)} }",
        "SELECT (ABS(?gross - ?effective - ?retained - ?other) > ?tolerance AS ?violated) WHERE { VALUES (?gross ?effective ?retained ?other ?tolerance) {(120.0 100.0 15.0 5.0 0.5)} }",
    ] {
        println!(
            "{}",
            serde_json::json!({"query":query,"result":oe_kernel::graph::query(&store,query)?})
        );
    }
    Ok(())
}
