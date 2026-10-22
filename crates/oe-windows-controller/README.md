# Windows NX controller

This crate is part of the repository's Cargo workspace. `oe-nx-controller`
accepts one bounded protocol request, or supervises one already-bound worker.
It uses Windows file handles, directory identities, precise process creation
times and COM process creation. It does not interpret CAD geometry or grant
engineering acceptance.

Build on a Windows x64 host with the installed Rust GNU toolchain:

```text
cargo build --locked --offline --target x86_64-pc-windows-gnu -p oe-windows-controller --bin oe-nx-controller
cargo test --locked --offline --target x86_64-pc-windows-gnu -p oe-windows-controller
```

`--offline` requires dependencies already present or a separately verified Cargo
vendor directory. Build from the received workspace and its root lockfile;
there is no separate controller workspace or private maintenance-path dependency.
The existing local `oe cad nx` SSH client remains a Unix-side component. Building
this crate does not establish Windows support for every other workspace crate.

The controller requires explicit root and NX installation paths. Optional root
identity and review pins are supplied by the control plane, never by the worker
payload. Both `--legacy-review` / `--legacy-review-sha256` and
`--dispatch-writer-review` / `--dispatch-writer-review-sha256` must be complete
pairs. A reviewed writer release can permit an independent new job after fresh
OS observations; it never changes the original `UNKNOWN` state, authorizes
repeating that dispatch, or creates a native result.

`controller-primitives-probe`, `frame-echo-fixture` and
`synthetic-worker-fixture` are explicit examples behind `acceptance-fixtures`.
They are absent from default builds and installed product binaries. For an
authorized test using a new task-owned directory:

```text
cargo build --locked --offline -p oe-windows-controller --features acceptance-fixtures --examples
```

These examples test platform or protocol behavior and do not load NX. Synthetic
results cannot be used as native CAD evidence. Do not point them at an existing
job root. Windows tests, source-only receiver builds, native NX execution and
engineering acceptance are separate verification steps; none inherits approval
from an older binary with a different source identity.


The portable `build` library provides fixed worker-build plans and packaging of
explicitly **unauthenticated external build records**. It does not execute a
compiler, implement a build command or submit an NX job. The separate
`build_source` module verifies source archives in memory, including their root
workspace and exact Cargo.lock. `build_execution` prepares a source-bound v2
plan with an explicit Cargo-offline policy and portable Windows argv encoding;
it does not execute processes. `build_config` limits effective Cargo settings;
the Windows-only `build_windows` library materializes verified source into a
new build tree and retains file/ancestor handles, without starting a compiler.
See [BUILD.md](BUILD.md) for the API, evidence limits and remaining runner work.
