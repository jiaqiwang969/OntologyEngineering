# Direct native builder and content verification

`build-native-budgeted` dispatches `CodeNativeBudgetedBuildRequest/2` through
`NativeRuntimeBuilder`. It returns `CodeNativeBuildCapture/2` and writes the
same JSON to the fresh authorized evidence directory's `native-capture.json`.
The original source artifact remains separately bound by `capture.artifact`.

One shared implementation owns the five original stages: own normalizer build,
actual locked ggen generation, runner build, actual generation drift check and
runner identity. The private legacy wrapper and original profile/1 public API
retain their original arguments and environment removals. Explicit native mode
adds the exact frozen encoded flags only to the two Cargo stages. It preserves
manifest, compiled normalizer, generation and tool guards. No registry or owner
authority is created by the builder or content verifier.

The separate `oe-native-linker` binary accepts rustc's native argument vector;
the fixed driver also exposes an explicit `native-link` branch. Each side owns
fresh `target-name-native-stage/build-{normalizer,runner}/call-NNNNNN` evidence
and stable `oe-native-debug/objects` paths whose names bind whole original payload SHA and original
argument ordinal. Repeated input arguments remain repeated. The direct recipe
refuses own archives, response files, unknown flags, cross-side inputs and
parent traversal; capture verification repeats the same argument parser.
External archives are unchanged, exact selected toolchain file pins.

Each native link and debug observer uses `oe_core::process::run_command` with
remaining body deadline and output cap. Exact argv, cwd, environment values,
removals, tool bytes and projected deadline are frozen before dispatch beside
the raw process folder. Raw terminal status, two complete streams and local PID
subtree cleanup are rechecked. The parent Cargo owner's monotonic deadline and
PID/birth subtree remain necessary: the child's wall-clock projection neither
creates another owner nor certifies hostile containment or hard real time.

Debug expectations are frozen independently. A qualified function name and
frozen source anchor every check. `type_member.name` is `Structure.member`
(optionally a fully qualified structure); it must reference a real parameter or
local type in that function. The member DIE's type offset is independently read
and checked, including known integer size and encoding. Variables require real
location expressions and correct lexical scope; source lines require the exact
CU table/file and a live address inside the function. Location lists, escaped
DWARF paths and unsupported type forms are refused rather than inferred.

Independent signature, nm, dsymutil and dwarfdump observations must account for
all actual N_OSO entries, loaded objects and archive members without warnings.
Required objects must have nonempty compile units; copied observer leaves bind
whole object/member payload. Neither UUID, exit zero nor a matching name is a
sufficient result. `verify_native_capture` replays raw observations and content
checks only; the existing engineering owner must still verify its registry,
prior context, builder pin, selected source, budget and final deadline.

Current recipe/2 requires regular, non-symlink SDK bindings. Real Apple SDK
aliases remain unsupported, as do own archives and response inputs. Thus a
successful unit test or generation check does not establish whole native
normalizer/runner execution, binary equality, owner-observed provenance or full
Semantica adoption. Explicit future SDK/archive schemas are separate work. A
native preparation must first produce the exact frozen generated source; the
owner's artifact-manifest equality must not be weakened to admit changes caused
by normalizer flags/provenance. Rustc may delete original temporary inputs after
link completion: retained whole-payload staging plus original before/after
observations do not independently recreate the deleted producer's provenance.
