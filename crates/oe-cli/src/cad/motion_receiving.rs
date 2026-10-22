//! Full Project binding before and after reception is owned by the kernel.
pub(super) fn run(
    root: &oe_core::SkillRoot,
    input: &std::path::Path,
    reopen: bool,
) -> anyhow::Result<(serde_json::Value, u8)> {
    oe_kernel::motion::receive(root, input, reopen)
}
