# Worker build planning and record packaging

`oe_windows_controller::build` is a portable library for the existing
`oe-nx-worker` and its narrow C++ shim. The current implementation creates a
finite plan and packages supplied bytes using the existing `WorkerManifest`.
It does not yet execute that plan. There is no worker-build CLI or remote build
request. NX job `Run`, job states and native results are unchanged.

## Available API

| API | What it establishes |
| --- | --- |
| `BuildSpec::validate`, `digest`, `plan` | Bounded declarations and a deterministic two-command plan |
| `BuildSpec::verify_sources` | Actual archive verification plus binding to the spec's archive/inventory/lock identities and expanded counts |
| `build_source::verify_source_archive` | Full in-memory member CRC/hash/size/Unicode-path checks; parsed root workspace and exact lockfile binding |
| `build_execution::prepare_plan` | V2 finite plan bound to an actual `VerifiedSourceArchive`; explicit network policy and portable Windows argv encoding |
| `ExecutionPlanRecord::validate_against` | Recompute an untrusted plan record against the declarations and verified source; no process authentication |
| `build_config::prepare_config` | Finite workspace-config policy and source-bound owned Cargo-home configuration bytes |
| Windows `build_windows::materialize` | New-only source tree, complete readback and retained selected input/ancestor handles; no compiler process |
| `BuildEvidence::validate_bindings` | Supplied records agree with the plan, declared files and process identities |
| `package_from_external_records` | Supplied source archive, lockfile, logs and artifacts have exactly the declared bytes; emits the existing manifest and recipe |
| `WorkerBuildPackage::verify` | Exact output member closure, hashes and original worker-package size limits |

The plan invokes the explicitly named Cargo executable for `oe-nx-worker` with
`--locked --offline`, an explicit GNU target/linker and a new target directory.
The second command invokes the explicitly named MSVC compiler for
`shim/nx_shim.cpp`, with the selected NX SDK headers and import libraries.
Arguments and a finite environment are separate typed values. No shell,
`vcvars`, caller-defined arguments or arbitrary environment map is accepted.
Drive-absolute paths may contain spaces and non-ASCII names.

The runtime declaration matches the worker's existing `Siemens NX` / `x86_64`
contract and its two pinned NX runtime libraries. Capability declarations are
limited to `create`, `inspect` and `import_step`, with
`cad-agent.nx-native-acceptance-params/v2`. These remain declarations: packaging
cannot prove that arbitrary supplied source or binaries implement an action.

## Evidence limits

Anyone can fill in an external record with exit code zero. Such a record is
never treated as an observed compiler execution. The package recipe retains:

- `record_bindings: verified_against_supplied_bytes`
- `compilation_execution: not_authenticated_external_records`
- `source_tree_verification: not_authenticated_external_records`
- `installed_toolchain_and_sdk: declared_file_identities_only_not_observed`
- Native and engineering acceptance: `not_evaluated`

The external-record packaging API retains and hash-binds archive and inventory
bytes without interpreting them. Its assessment stays unchanged even if a
caller also performs source verification. SDK identities describe selected installed files;
they are not a complete transitive SDK/toolchain closure. No licensed SDK
headers, import libraries or runtime DLLs enter the package. The included shim
is the supplied build output, not a distributed Siemens library.

Source expansion has a separate declared limit of 512 MiB / 20,000 files. The
packaged archive and worker still obey the unchanged job-payload limit of
100 MiB / 10,000 files. A large build tree does not enlarge job limits.

## Source archive verification

The separate `build_source` module accepts a ZIP, a strict `SourceManifest`
(schema `cad-agent.nx-worker-source-archive/v1`) and independently supplied raw
Cargo.lock bytes. The manifest declares a single explicit `archive_root` and
all regular member paths relative to it, with original `FileEntry` identities.
The verifier checks the raw ZIP central directory before a ZIP map can hide
exact duplicates; it rejects symlinks, reparse bits, directories, encryption,
ZIP64, extra records, overlapping data and traversal. It reuses the protocol's
Unicode and Windows compatibility keys. It then decompresses under the separate
build budget, verifies actual CRC/size/SHA and requires the complete member set.

The root must contain Cargo.toml and byte-identical Cargo.lock. Real TOML parsing
checks the virtual workspace, resolver 3, the current 12 explicit members, each
member's package name/version, and the lock's corresponding local packages.
Registry records must use the existing crates.io identity and valid checksums.
This is structural and byte verification; Cargo dependency resolution, vendor
semantic consistency, build-script behavior and compilation are not executed.

`VerifiedSourceArchive` cannot be deserialized or externally constructed. Its
assessment reports the actual counts/bytes and marks filesystem materialization,
Cargo resolution and compiler execution as not performed. Returned source bytes
have not been written or executed. `BuildSpec::verify_sources` additionally
rejects mismatched declared source identities or expanded counts. The original
external build records still cannot authenticate a build or a disk-tree scan.

## Source-bound v2 execution planning

`ExecutionSpecV2` explicitly selects `CargoOfflineOnly`. `RequireOsDenied` is
rejected because this implementation provides no OS network isolation. The
assessment states `os_network_enforcement: not_provided` and
`network_activity: not_observed`; Cargo's offline flags cannot establish either.
The v1 input declarations are reused, while the v1 plan and its historical
`network_allowed: false` requirement retain their original semantics and digest.
There is no implicit conversion from a v1 plan to a supported v2 execution plan.

`prepare_plan` requires the opaque `VerifiedSourceArchive`, matching the actual
archive, inventory and lock identities and observed expanded counts. The source
directory becomes `<root>/build-<id>/source/workspace`. Cargo home, target, TEMP
and TMP also belong to that future new build tree. An installed compiler, SDK,
Windows or search-directory input may not be inside the new output subtree.
These are plan requirements; this module has not observed or created that tree.

Both versions reuse the fixed two-command argument generator. V2 clears inherited
environment values, adds only the finite named variables, and orders PATH as
MSVC, GNU linker, Rust compiler, Cargo, then Windows System32, removing duplicate
directories while preserving order. INCLUDE/LIB are explicit and limited to the
shim command. Environment and command-line UTF-16 lengths are bounded.
Windows command lines quote each argument and preserve spaces, Unicode,
percent signs and trailing backslashes without invoking a shell. Portable
literal-vector tests do not replace a future Windows fixture's actual argv readback.

`PreparedExecutionPlan` has private construction and cannot be deserialized.
`ExecutionPlanRecord` is strict but untrusted JSON: revalidation recomputes all
fields, including encoded argv, environment, source identity and assessment.
Neither a decoded plan nor an external exit-zero observation creates an
authenticated execution result. No `ExecutedBuild` or success constructor exists.

## Windows preparation without execution

`build_windows::materialize` takes an explicit `StableRoot`, execution spec,
prepared plan and verified source. It verifies the root identity and pathname,
selected tool/SDK bytes and search-directory handles before creating exactly
one `build-<id>` directory. A root containing `.nx-writer.lock` is rejected.
An existing build ID is not resumed, overwritten or deleted. The API never
uses the CAD profile's default root or starts a process.

The source is written with create-new ordinary-file operations, then completely
enumerated and hashed against the verified members. Extra files and empty
directories, missing members, changed bytes, links and reparse traversal fail.
Source inventory uses its own 512 MiB / 20,000-file budget, without changing
the job adapter's 100 MiB / 10,000-file rules. Source files and their ancestors,
generated configuration, selected compiler/SDK files and observed directories
remain pinned while `PreparedBuild` lives. It is privately constructed, not
deserializable, and represents preparation only. Failure preserves the owned
directory and attempts a bound failure receipt without losing the original error.

Workspace Cargo configuration is limited to no root config or the existing
`.cargo/config.toml` containing only `[build] target-dir = "var/builds/rust-target"`.
Its original bytes are bound; fixed `CARGO_TARGET_DIR` overrides that setting.
Legacy `.cargo/config` and other effective keys are refused. A new Cargo home
contains only generated `config.toml`, using offline mode and, when registry
packages exist, the included `source/workspace/vendor` directory. A vendor tree
must actually be present in the verified source in that case. Vendor checksum
semantics and Cargo dependency resolution remain unevaluated. Nested vendor
configs remain source bytes; they are not ancestors of the planned root Cargo
invocation. This is not a sandbox for build scripts or their subprocesses.

Both ancestor configuration names are checked from the workspace's parent to
the volume root. Existing external config is rejected; `.cargo` and all parents
must be ordinary directories without reparse traversal. Absence is an observation
at that moment, not exclusion of future additions. `revalidate_source_and_inputs`
repeats the complete source/config checks; a future runner must call it immediately
before launch. Selected files carry requested/final paths, file IDs and hashes.
Those facts do not authenticate executable roles, all search-directory contents,
or transitive compiler/SDK libraries. A shared Cargo/rustc proxy identity is refused.

Opt-in Windows filesystem fixtures require an explicitly supplied existing
task-owned `OE_OWNED_BUILD_TEST_ROOT` and `--features acceptance-fixtures --test
build_windows`. They create unique child directories, retain them for inspection,
and use synthetic compiler/SDK bytes that are never loaded or executed. Cross
Clippy checks these tests' types; it does not establish actual Windows behavior.

## Remaining build-only execution

The runner must consume the prepared, pinned tree, recheck immediately before
launch, retain source identities after execution, and create its own durable
process and stream receipts. Actual resolved executable roles and compiler-loaded
library scope still need their own observations; pinned bytes alone do not
establish these facts.

Actual execution must use typed process arguments, bounded logs/time, exact
process identity and safe process lifetime management. The plan's offline
Cargo flags do not sandbox build scripts or establish OS-level network denial.
The v1 `network_allowed: false` and both plans' `output_must_be_new` fields are
requirements, not observations made by this library. V2's Cargo-only policy
does not claim to satisfy the stronger v1 network requirement. Cargo source,
build scripts and configuration therefore need source review before execution.

A runner-authenticated result needs a distinct construction path that cannot
be minted by deserializing external `BuildEvidence`. It must remain separate
from NX job dispatch and must not load NX or run the generated worker. Build
tests, compilation, native geometry and engineering acceptance remain separate
claims. Portable fixture tests intentionally use synthetic records; they are
not compiler or NX evidence.
