//! Independent unchanged-oracle corpus validation; final public entry is oev2.
fn main() -> anyhow::Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("root required"))?;
    let round = std::env::args().nth(2).unwrap_or_else(|| "01".into());
    let skill = oe_core::SkillRoot::new(&root)?;
    for bundle in [
        "engineering-judgment-intake",
        "engineering-evidence-methods",
        "manufacturing-process-cost",
    ] {
        let report = oe_kernel::packages::regress(skill.path(), bundle)?;
        let path = skill
            .path()
            .join(format!("var/maintenance/corpus-{round}-{bundle}.json"));
        oe_core::write_new(&path, &report)?;
        println!(
            "{}",
            serde_json::json!({"bundle":bundle,"status":report["status"],"passed":report["passed"],"failed":report["failed"],"unknown":report["unknown"],"report":path})
        );
    }
    Ok(())
}
