//! Independent checks of a drawing-understanding result. Nothing here calls the classifier: the graph check
//! re-derives every expectation from the bytes of the source DXF with its own reader, and the page check
//! computes its expectations in the browser from the graph embedded in the page. The drafting-rule check
//! evaluates the shapes of the drafting ontology over facts exported from the bytes and the graph. Passing says
//! the source is conserved and the outputs keep their contracts; it is not engineering correctness.
pub mod browser;
pub mod drafting;
mod dxf;
pub mod graph;
mod svg;

/// One file by path and digest; the path is relative to `base` when the file lies inside it.
pub fn file(
    base: Option<&std::path::Path>,
    path: &std::path::Path,
) -> anyhow::Result<oe_contracts::MdDrawingRouteFile> {
    let bytes = oe_core::read_regular_bytes(path, 512 * 1024 * 1024)?;
    let shown = match base.and_then(|b| path.strip_prefix(b).ok()) {
        Some(relative) if !relative.as_os_str().is_empty() => {
            relative.to_string_lossy().into_owned()
        }
        _ => path.to_string_lossy().into_owned(),
    };
    Ok(oe_contracts::MdDrawingRouteFile {
        path: shown,
        sha256: oe_core::sha256(&bytes),
        bytes: bytes.len() as u64,
    })
}
