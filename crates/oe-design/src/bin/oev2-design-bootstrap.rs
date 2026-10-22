//! Internal bootstrap before generated contracts exist; final user CLI is oev2.
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() >= 3,
        "usage: bootstrap normalize|generate|check root [ggen]"
    );
    let root = oe_core::SkillRoot::new(&args[2])?;
    match args[1].as_str() {
        "normalize" => {
            let spec = root.path().join("specs/engineering.toml");
            let d = oe_design::load_current(&root, &spec)?;
            let bytes = oe_core::read_regular_bytes(&spec, 4 * 1024 * 1024)?;
            let composition = if oe_design::composition::transport::schema(&bytes)
                .ok()
                .as_deref()
                .is_some_and(oe_design::composition::transport::is_composed)
            {
                let loaded = oe_design::composition::load(root.path(), &spec)?;
                oe_design::composition::require_current(&root, &loaded)?;
                Some(loaded)
            } else {
                None
            };
            let destination = if args.get(3).map(String::as_str) == Some("--output") {
                args.get(4)
                    .ok_or_else(|| {
                        anyhow::anyhow!("normalize --output requires a private directory")
                    })?
                    .as_str()
            } else {
                "var/maintenance/normalized-baseline"
            };
            let out = root.new_private_dir(std::path::Path::new(destination))?;
            oe_core::write_new_bytes(
                &out.join("model.ttl"),
                oe_design::model(root.path(), &d)?.0.as_bytes(),
            )?;
            let normalized = match &composition {
                Some(loaded) => loaded.normalized()?,
                None => d.canonical()?,
            };
            if composition.is_some() {
                anyhow::ensure!(
                    normalized.len() as u64 <= oe_core::source_limits::PUBLIC_FILE_BYTES,
                    "normalized_composition_artifact_budget"
                );
            }
            oe_core::write_new_bytes(&out.join("normalized-design.json"), &normalized)?;
            if let Some(loaded) = &composition {
                loaded.verify(root.path())?;
                oe_core::write_new_bytes(
                    &out.join("design-composition-source.json"),
                    &loaded.source_bytes()?,
                )?;
            }
            oe_core::write_new(
                &out.join("execution-binding.json"),
                &oe_design::provenance::execution(&root, "bootstrap_normalization")?,
            )?;
        }
        "generate" => {
            let tool = std::path::Path::new(
                args.get(3)
                    .ok_or_else(|| anyhow::anyhow!("ggen required"))?,
            );
            let (out, v) = oe_design::generation::run_spec(
                &root,
                &root.path().join("specs/engineering.toml"),
                tool,
            )?;
            let record = oe_design::provenance::execution(&root, "generation_publication")?;
            let receipt_root = root.unique_private_dir(
                std::path::Path::new("var/maintenance"),
                "compiler-execution",
            )?;
            oe_core::write_new(&receipt_root.join("binding.json"), &record)?;
            oe_design::generation::publish(&root, &out)?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        "check" => {
            let tool = std::path::Path::new(
                args.get(3)
                    .ok_or_else(|| anyhow::anyhow!("ggen required"))?,
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&oe_design::generation::check_spec(
                    &root,
                    &root.path().join("specs/engineering.toml"),
                    tool
                )?)?
            );
        }
        _ => anyhow::bail!("unsupported bootstrap command"),
    }
    Ok(())
}
