//! Acceptance fixture only: proves unchanged binary stdio through exact launcher.
fn main() -> anyhow::Result<()> {
    let frame = oe_nx_protocol::read_frame(&mut std::io::stdin().lock())?;
    oe_nx_protocol::write_frame(&mut std::io::stdout().lock(), &frame)?;
    eprintln!("owned binary framing fixture completed");
    Ok(())
}
