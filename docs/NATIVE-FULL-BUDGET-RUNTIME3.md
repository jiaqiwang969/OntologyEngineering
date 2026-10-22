# Full native budget runtime /3

`native_runtime3::build_full_native_budgeted(&CodeNativeBudgetedBuildRequest3)`
returns `CodeNativeBuildCapture3`. The generated
`CodeNativeBudgetedBuild3Port` calls this real implementation. The driver command
is `build-full-native-budgeted`; its stdout is the exact saved bytes of
`evidence/native-full-capture.json`. The separate `oe-native-full-linker` binary
is rustc's selected linker entry. Legacy and native /2 remain explicit modes
of the same original five-stage builder, with their existing APIs and schema
guards. Native /3 never projects its profile, side or capture into native /2.

The five actual stages remain build-normalizer, generate, build-runner, check
and identity. The normalizer stays at `target/debug/oev2-design-bootstrap`;
the runner stays at `target/<platform>/debug/oe-code-runner`. Only Cargo build
stages receive the frozen encoded Rust flags. Generation, drift checking and
compiled identity keep their original environment contract. All subprocesses
use `oe_core::process::run_command`, captured raw streams and the remaining
original body deadline. The enclosing owner's monotonic deadline and observed
process subtree remain the authority; projected child timestamps are content
checks rather than another owner or a hard real-time guarantee.

Native /3 consumes the strict /3 profile, recipe, side and two-image bundle,
the SDK /3 declared alias layout, and typed archive/response /3 input facts.
Outer Cargo explicitly removes inherited environment values and selects the
frozen Cargo/tool/context/debug values. The adapter rejects inherited native
search/configuration overrides. Its indexer and linker select PATH and LC_ALL
and retain the typed callback's empty removal list. Every actual selected
command is frozen before dispatch and checked against raw process argv, cwd,
executable payload, streams, environment key inventories and remaining budget.

For each native invocation the clang cwd is
`side.stage_root/<build-role>/call-NNNNNN`, while the pure input preparation
root is that directory's `oe-native-debug` child. The new pure /3 stage guard
accepts this exact nested relationship as well as its existing sibling case;
other parents, role names, call ordinals and debug leaves are rejected. Actual
clang argv prefixes every staged input occurrence with `oe-native-debug/`,
including each bound `-Wl` component, while preserving argument/member order,
duplicates, metadata, unused members and lazy archive boundaries. Frozen
recipe suffixes use the exact declared SDK root and native linker pins.

Preparation and callback facts retain original and staged full payload bytes,
Darwin member header attributes, member order, rebuilt index structure, raw
response bytes and expansion coordinates. Original inputs are checked around
the native call; staged snapshots and raw callback evidence are checked later
without requiring disposable original compiler inputs to remain alive.
Before/after records also bind source, tools, SDK alias layout and raw flags.
Input closure is bounded: untracked filelists, unknown libraries/frameworks,
config files, SDK search shadows and embedded Mach-O autolink/dylib channels
are rejected explicitly. Full /3 requires the original expanded argv to include
`-nodefaultlibs`; it does not append that flag or silently alter a request.
Before any archive index callback, every original selected file is checked.
All external regular archive members and their original structural symbol index
are read without changing a byte. The supported object domain is arm64
little-endian Mach-O64 MH_OBJECT with a finite known command inventory; fat,
other target/endianness/file types, bitcode and opaque metadata are refused.
An archive's structural index is parsed explicitly; a named `lib.rmeta` member
is accepted only when its payload actually satisfies the same supported object
structure. Unknown metadata never defaults to clean. These refusals do not remove any required input from
the whole-project goal. A genuinely required unsupported input needs a new
explicitly bound implementation and independent actual qualification.

Normalizer and runner each have their own frozen five-kind expectations,
actual image identity, signatures and observer directory. One shared DWARF
parser/observer/retained-evidence verifier checks both modes and both /3 roles.
It checks actual source/line/function/local/type/member data, maps every actual
OSO to the selected side's full object or archive member payload, requires every
mapped object to load with nonempty compile units and rejects observer warnings.
Signature evidence must positively report both disk validity and satisfaction
of the designated requirement. No exit-code-only or name-only success is used.
The original strict global member-type selection is retained; a whole image
with multiple matching type DIEs is refused until exact function/type selection
has its own frozen counterexample and verified fix.

`verify_full_native_capture(root, profile_ref, side_ref, artifact_ref, capture)`
checks content, five raw stages, actual invocation inventory and distinct image
observations. It does not grant owner provenance, registry state or adoption
authority. The existing owner must still check its frozen source/profile/context,
selection lock, tool and driver pins, current manifest equality and final deadline.
Native generation preparation is a separate unqualified genesis operation:
first create exact native-flags generation, then freeze that source/profile/context
and compile the selected plain driver and adapter. A formal fresh side must
reproduce that preapproved manifest; ordinary plain generation cannot stand in
for native genesis by relaxing normalizer provenance or source equality.

This source branch supplies an actual implementation and independently run
bounded tests. Fresh whole normalizer/runner pairs, complete Semantica platform
scope, original engineering criteria and adoption remain separately unqualified
until their original checks actually succeed. The source-only handoff carries
no generated outputs, executable, cache or earlier execution qualification.
