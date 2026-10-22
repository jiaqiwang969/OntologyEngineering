# Typed Darwin archive and response content

`oe-evolution::native_archive` implements the new `darwin-archive-response/1`
input algorithm. Its ten public types (101 fields) and borrowed inspection port
come from `specs/engineering.toml` and actual ggen generation. The old direct
profile, recipe, side and future-unsupported checks are unchanged.

`prepare_link_inputs(root, request, index_callback)` captures every own object,
regular archive and response occurrence. The callback receives a generated
`CodeNativeArchiveIndexRequest` and returns generated actual
`CodeNativeInputProcessEvidence`. This module does not spawn a process, set a
deadline, acquire a lease, dispatch a builder or grant adoption authority.
Execution and the remaining deadline belong to the existing process/build owner.

Own archives retain every nonstructural member in order, repeated names,
metadata, literal header attributes and unchanged full payloads. MachO member
names use member ordinal and full payload SHA; `lib.rmeta` keeps its name. The
canonical archive identity serializes exactly the original eight internal
member fields in declaration order: ordinal, stable name, date, uid, gid, mode,
SHA and payload byte count. Original rcgu names, physical paths and raw header
provenance remain separately typed facts and never enter that identity.

The callback must perform the exact pinned `llvm-ar --format=darwin sU`
request in the selected stage. Complete original, before-index and after-index
containers, ordered members and structural index entries have separate byte
references. The checker compares rebuilt payloads and attributes and verifies
the BSD symbol index against actual MachO64 little-endian external symbol
names, member offsets and same-symbol occurrence order. It also rechecks the
actual shared-process report, executable identity, argv, cwd, status, exit and
both complete retained streams. Selected nonsecret environment values are
CommandSpec facts; the shared raw report independently records key inventory,
not values or inherited environment.

Response files use a bounded UTF8 GNU token grammar: quotes, backslash escaping,
empty tokens and whitespace. Every nested or repeated occurrence retains its
own preorder identity, parent occurrence, local argument ordinal, caller cwd,
raw bytes, decoded order and final half-open expansion range. Limits are
1 MiB total response bytes, depth 8 and 10,000 arguments. Own archive snapshots
are limited to 128 MiB, individual archive members to 64 MiB and 10,000 entries.
File/tool fingerprints use a separate 512 MiB bound. Unknown MachO index source,
thin/GNU containers, hidden `-Wl,@response`, hidden output options, duplicate
`-o`, unselected external inputs, symlink paths and parent traversal reject.

`verify_prepared` and `inspect_preparation` check retained original snapshots,
all staged live files, typed facts and exact reconstruction of execution
arguments. They do not claim that rustc's temporary original objects still
exist. `verify_live_inputs` checks the original physical files at the live
owner boundary before rustc can remove them. A later missing original therefore
fails the live check while the retained content inspection remains possible.

The stage's logical object/archive paths are relative to the selected
`request.stage_root`. The full revision 3 integration must explicitly freeze
and verify any additional `oe-native-debug` prefix, the enclosing actual clang
cwd, its `-oso_prefix`, and the mapping between those two roots. This module
does not silently supply or qualify that relationship.

Current meaningful tests use fresh native C objects, repeated archive member
names, unused unresolved symbols, MachO metadata, real index callbacks,
nested/quoted/repeated/empty response files, whole-archive extraction, and
corrupt member/index/process/response facts. Successful index exit alone cannot
attest a corrupted symbol offset. A whole-archive undefined-symbol exit 1 is an
expected semantic outcome. Original failed build/ggen/test attempts and older
finite native/Cargo proofs keep their original identities.

The typed module does not yet connect full profile/recipe/side/capture revision
3, SDK aliases, the shared five-stage builder, or the unique production owner.
Fresh typed multi-crate Cargo builds, complete binary equality, native signatures
and debug observations must be run again after that connection. Previous
private probes and the whole 13,494/19/19/22 migration do not inherit qualification.

A separate hidden-input guard revision preserves the original archive/response
source and independent review identity. It rejects Darwin file-list, extra
file-reading linker options, unknown implicit libraries/frameworks/configuration
and untracked bare input files before any callback. The frozen sixteen controls
cover plain, comma-forwarded and Xlinker file lists. Known system library tokens
and any library search directories still require the later explicit SDK/native
recipe inspection; this finite refusal boundary does not qualify complete native
input closure, embedded MachO linker directives or whole Cargo library lookup.
