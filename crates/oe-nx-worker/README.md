# Bounded native NX worker

This workspace crate owns source/dispatch verification, the durable RUNNING
handshake, native library identity checks, operation parameters and readbacks.
It loads an explicitly pinned installed NX runtime on Windows. It does not run
Python journals or attach to an existing graphical NX session.

The current `cad-agent.nx-native-acceptance-params/v2` capabilities are limited:

- `inspect`: open declared staged parts and return native observations.
- `inspect_detailed`: query staged part geometry, full solid-edge inventory and
  annotation associations/ComputedSize with explicit raw query results.
- `maintain_drawings`: copy to task output, refresh, observe and save.
- `create_reference_dimension`: select independently declared full-circle edges
  in a copied part, create a bounded parallel modelspace dimension, read and save.
  Requires a separate inspect job for independent reopen; sheets, drawing views,
  manufacturing tolerances and layout are not produced by this action.
- `create`: the controlled millimetre block/update, two-occurrence assembly and
  AP214 export acceptance scenario.
- `import_step`: import a declared STEP input into a new task-owned native part.
- `import_parasolid`: use the installed SDK's UF import into a task-owned native
  part, then observe and save it. Curved-part dimensions use the native accurate
  bounding box with all eight corners transformed by the declared frame.

These actions do not constitute general CAD authoring. A complete acceptance
requires actual native execution, saved bytes, and independent new-process
reopen/import/reopen. A successful compilation or `COMPLETED` transport state is
not engineering approval. Other owned CAD journals still require typed Rust
implementations before their migration can be declared complete.

Build the Rust executable on Windows using the workspace lockfile:

```text
rustup run 1.95.0 cargo build --locked --offline --target x86_64-pc-windows-gnu -p oe-nx-worker --bin oe-nx-worker
rustup run 1.95.0 cargo test --locked --offline --target x86_64-pc-windows-gnu -p oe-nx-worker
```

On an x64 Windows host with an installed MSVC developer environment, the
`x86_64-pc-windows-msvc` target is also supported. Select the matching pinned Rust
toolchain and compiler environment explicitly. The worker checks the manifest
target against its compiled target; declaring another accepted protocol target
does not make it executable by this binary. Unsupported platforms fail before
native initialization. Compilation, identity checks and actual NX execution
remain separate validation activities.

The tests use synthetic geometry observations or task-owned ordinary files;
they do not call the NX SDK. Non-Windows builds can run the pure geometry tests,
but the worker executable refuses native execution there. Rust compilation does
not automatically compile or load the vendor shim.

## Explicit shim build

`shim/nx_shim.{h,cpp}` is owned, narrow C ABI glue around installed NX C++ calls.
It contains vendor object lifecycle and exception handling, with no JSON,
project decisions or business workflow. Compile it only with an installed NX
SDK and the x64 MSVC developer environment. Neither SDK headers, vendor DLLs nor
license material are included in this repository.

In an x64 MSVC Developer Command Prompt, set `SKILL_ROOT` to this received
workspace, `NX_SDK` to the selected installation's `UGOPEN` directory, and
`BUILD_DIR` to a new task-owned directory under the skill's `var/builds`.
Then run the compiler explicitly from that output directory:

```bat
cd /d "%BUILD_DIR%"
cl.exe /nologo /std:c++17 /EHsc /MD /W4 /WX /external:W0 /external:I"%NX_SDK%" /LD "%SKILL_ROOT%\crates\oe-nx-worker\shim\nx_shim.cpp" /link /LIBPATH:"%NX_SDK%" libufun.lib libnxopencpp.lib libnxopencpp_assemblies.lib libnxopencpp_annotations.lib /OUT:nx_shim.dll
```

Do not reuse a shim merely because its filename matches. Record the actual
compiler invocation, source/header/library hashes, toolchain identity, compiler
exit and resulting DLL identity. The compiled shim checks tag/logical widths,
structure layout and the selected SDK constants; runtime checks still apply.

Package the new EXE and shim with an explicit `WorkerManifest`, exact source and
build closure, and installed runtime file identities before `oev2 cad nx prepare`.
The current shim uses `Dimension::ComputedSize`, so the exact runtime closure is
`NXBIN/libufun.dll`, `NXBIN/libnxopencpp.dll`,
`NXBIN/libnxopencpp_assemblies.dll`, and `NXBIN/libnxopencpp_annotations.dll`.
All four are byte-pinned and explicitly loaded. Older three-library packages
keep their original worker identity; they do not meet this worker's closure.
Use `oev2 cad nx worker verify --worker-directory <package>` for a local read-only
check before preparing a job. It shares the preparation command's manifest,
exact file closure and byte checks, without needing a remote profile. Package
completeness does not establish compilation provenance or native acceptance.
Neither command is a shim compiler or a worker-package generator. An automated package-generation
workflow remains a separate integration step. Private acceptance packages and
host profiles are not distribution dependencies.

## Measurement boundary

For the controlled axis-aligned all-planar block, dimensions come from observed
vertex bounds. The UF bounding box is checked only for containment: the installed
NX 2412 SDK describes `UF_MODL_ask_bounding_box` as favouring speed over accuracy
and does not guarantee a minimum box. The original numerical tolerance is
retained. This does not make vertex bounds a general exact-bounds method for
arbitrary curved geometry or an independent proof of block topology.


The task-private saved-part acceptance test is `native_receipts`. It is ignored
in ordinary test runs because it needs verified native create and independently
reopened receipts. The agent supplies `OEV2_NATIVE_CREATE_JOB` and
`OEV2_NATIVE_REOPEN_JOB`, both below this workspace's `var/`, and runs:

```text
rustup run 1.95.0 cargo test --locked -p oe-nx-worker --test native_receipts -- --ignored
```

The test checks the original pre-execution parameters and staged file bytes,
uses the existing Rust block/reference-set checks, and rejects copied observations
with one wrong dimension or stale JT. Native files and original receipts are not
modified. Its current acceptance case retains the failed STEP export and the
successful saved-part reopen as separate outcomes. STEP import/reopen, arbitrary
geometry, kinematics, contact and engineering adoption remain unverified.

The earlier UF2-5 migration has its own actual import/save and independent-process
reopen evidence. The opt-in test
`imported_parasolid_survives_independent_reopen_with_original_native_bytes` uses
`OEV2_PARASOLID_IMPORT_JOB`, `OEV2_PARASOLID_REOPEN_JOB` and
`OEV2_PARASOLID_ACCEPTANCE` under this workspace's private `var/`. It checks original staged bytes, worker and
runtime identities, native dimensions and the saved-file identity used by the
second process. Its 0.01 mm comparison limit is a file/readback criterion, not
a manufacturing tolerance or spring-performance qualification. Run this test
by its name with `--ignored`; another ignored native case is not thereby passed.
