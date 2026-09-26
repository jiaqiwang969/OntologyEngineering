# Source-to-Fusion assembly delivery

> Historical Fusion translation retained for provenance. Current A0–A8 method and v3 schema are in source-to-native-assembly-delivery.md; do not execute this historical software route.

Use this workflow when the requested result is a new or repaired native Fusion
assembly, not merely an analysis of a supplied STEP file. It bridges source
recovery, content completeness, Fusion authoring, native readback, and the
bundled S0-S11 assembly-analysis lane.

This workflow does **not** promise that every input can be completed
automatically. "Arbitrary input support" means deterministic intake,
classification, evidence-bounded reconstruction, and fail-closed
`PASS`/`HOLD`/`UNKNOWN` outcomes.

## Keep the two lanes distinct

```text
A0-A8 authoring lane
  proves source closure, content accounting, geometry fidelity,
  Fusion occurrence construction, persistence, and native readback

S0-S11 assembly lane
  proves identity, interfaces, mechanism semantics, sequence, direction,
  full-interval interference, tool access, animation evidence, and physical release
```

A native assembly can pass A0-A7 while its assembly process remains `HOLD` at
S3, S7, S8, or S11. Conversely, a STEP assembly can support bounded S0-S10
analysis without being a complete or editable native Fusion deliverable.

Before a Fusion call, read `references/fusion-execution.md`. For assembly
meaning and evidence gates, read `assembly/SKILL.md` and its linked input and
claim contracts.

## Contents

- [A0 - Freeze intent, authority, and claim scope](#a0---freeze-intent-authority-and-claim-scope)
- [A1 - Close the source and dependency graph](#a1---close-the-source-and-dependency-graph)
- [A2 - Reconstruct identity and the occurrence tree](#a2---reconstruct-identity-and-the-occurrence-tree)
- [A3 - Close the content-completeness universe](#a3---close-the-content-completeness-universe)
- [A4 - Enforce geometry fidelity and provenance](#a4---enforce-geometry-fidelity-and-provenance)
- [A5 - Freeze placement and interface contracts](#a5---freeze-placement-and-interface-contracts)
- [A6 - Author in Fusion as a resumable transaction](#a6---author-in-fusion-as-a-resumable-transaction)
- [A7 - Perform native acceptance and persistence readback](#a7---perform-native-acceptance-and-persistence-readback)
- [A8 - Hand off to S0-S11 and video delivery](#a8---hand-off-to-s0-s11-and-video-delivery)
- [Case-derived transfer boundaries](#case-derived-transfer-boundaries)

## A0 - Freeze intent, authority, and claim scope

- Name the target artifact, destination project/folder, configuration, units,
  world frame, and whether the work is analysis, a display assembly, an
  editable engineering assembly, or manufacturing handoff.
- Freeze every supplied STEP, native CAD root, dependency, BOM, SOP, supplier
  record, image/video record, and rights statement with path, byte size, hash,
  revision, and authority.
- Register every cited record once in `evidence_registry` with a stable logical
  ID, path, SHA-256, media type, evidence kind, and timezone-aware capture time.
  Every `*_evidence_ids` value must resolve to this registry; a fluent but
  unregistered ID is invalid evidence.
- Declare allowed mutations. Never overwrite a trusted source, reuse an
  ambiguous untitled document, publish, or release for manufacture by default.
- Use the manifest-v2 `release_intent` enum. Use only
  `GRANTED_PUBLIC_REDISTRIBUTION`, `GRANTED_PRIVATE_USE`, `RESTRICTED`,
  `DENIED`, or `UNKNOWN` for source rights, with evidence. `DENIED` or any
  non-public grant blocks `PUBLIC_REDISTRIBUTION`; unknown rights remain a hold.
- Select only claims that the available evidence can support. Source geometry
  alone cannot establish installed hardware count, process order, torque,
  harness routing, tool access, or physical release.

Start from `assembly/assets/assembly-authoring-manifest.template.json`. Keep the
filled manifest and evidence ledger in the target project, not in this skill.

## A1 - Close the source and dependency graph

Choose the authoritative root from provenance and configuration evidence, not
from the most attractive render. Recursively inventory:

- assembly and part files, external references, configurations, suppression,
  lightweight or envelope state, and virtual/internal definitions;
- reused definitions versus saved references versus expanded physical
  occurrences;
- exact missing dependencies, unresolved aliases, and possible substitutes.

Record every root, external, and virtual/internal file in
`source_closure.file_records`, and every dependency edge in
`source_closure.reference_edges`. Bind resolved disk files to controlled
`sources[]` records, require evidence on every file and edge, and make every
non-root record reachable from the selected root. Derive expected, resolved,
and unresolved external-file totals from that ledger. A claimed 210-file
closure backed by one enumerated file is a validation failure, even when its
three self-reported totals are arithmetically consistent.

Enumerate each native/source definition in
`source_closure.definition_records`, bind it to its containing resolved file,
and derive `definition_count` from those records. A geometry record with
source-bound provenance must resolve its `source_definition_id` to this ledger;
an arbitrary fluent identifier is not provenance.

Do not replace a missing native part merely because a STEP file has the same
name. Record it as a candidate until geometry, revision, role, and placement
equivalence are proven. A closed file-count manifest is not yet a closed
occurrence tree.

## A2 - Reconstruct identity and the occurrence tree

Keep these identities separate:

```text
source artifact -> reusable definition -> occurrence path -> world transform
                -> BOM/process role -> interface/placement contract
                -> Fusion definition -> Fusion occurrence -> persisted data identity
```

- Preserve raw names and stable native IDs where available.
- Expand every occurrence path; do not collapse repeats into a unique-name or
  unique-geometry count.
- Compute world transforms using a declared convention and independently
  cross-check direction/composition.
- Treat virtual definitions, suppressed items, purchased modules, and harness
  subgraphs explicitly. Do not silently turn metadata-only recovery into exact
  standalone B-Rep.
- Reconcile source and Fusion trees at occurrence identity level. A count
  subtraction is a discrepancy, not an explanation.

Use one `occurrence_reconciliation.rows[]` record for every source and Fusion
occurrence. Every side also carries its definition identity:
`source_occurrence_id` binds `source_definition_id`, and
`fusion_occurrence_id` binds `fusion_definition_id`. `MATCHED` requires all
four identities, the Fusion definition's provenance must bind that same source
definition, and the completeness row must name that exact Fusion definition.
Source-only exclusions and Fusion-only additions use a null identity and null
definition on the absent side, plus a non-placeholder rationale and registered
evidence; unresolved differences remain a hold. The row-derived source and
Fusion sets must equal their native readback totals. A mismatch may pass only
when every difference has such a disposition; a zero source or Fusion
occurrence count cannot pass.

## A3 - Close the content-completeness universe

Build a complete installed-content ledger before claiming "the whole
assembly". At minimum account for:

- structural and enclosure parts;
- screws/bolts, nuts, washers, pins, keys, rings, bearings, and spacers;
- actuators, electronics, sensors, purchased modules, and optional variants;
- harnesses, cables, hoses, seals, springs, and other flexible/process-shaped
  items.

Create `completeness.requirements[]` before category summaries. Each requirement
binds an item identity, category, evidence, quantity semantics, selected target
count where knowable, and one row per physical occurrence. Use only:

| Quantity semantics | Meaning in the selected configuration |
|---|---|
| `EXACT` | equal positive lower, upper, and selected installed counts |
| `MINIMUM` | positive lower bound; a separately evidenced selected count is required for total closure |
| `RANGE` | lower/upper interval; the selected count must lie inside it |
| `OPTIONAL` | lower zero, positive upper, and an evidenced selected count, including zero |
| `PROCUREMENT_PACK` | purchasing quantity only; never creates installed occurrences |
| `UNKNOWN_UPPER` | known lower bound with unknown upper; blocks total completeness |

Derive each category's expected, exact-present, proxy-present, missing, and
unknown counts from these occurrence rows. Do not accept category arithmetic as
its own evidence. A zero-count `NOT_APPLICABLE` category requires both a
specific rationale and registered evidence. A procurement pack quantity is
never an installed occurrence count. A definition appearing once does not
satisfy several required occurrences. Lower bounds remain lower bounds;
unknown upper bounds remain unknown.

Every missing occurrence needs a disposition:

```text
exact source recovery | exact supplier acquisition | evidence-grounded native model
display-only analytic proxy | optional/not-installed | duplicate already present
HOLD: identity | HOLD: geometry | HOLD: placement | HOLD: authority
```

No unresolved item may disappear from the final claim ledger. A partial exact
subset can be `PASS_WITH_HOLDS`; it cannot become total completeness `PASS`.

## A4 - Enforce geometry fidelity and provenance

Classify every definition and retain the class through delivery:

- `NATIVE_PARAMETRIC`: editable native features with verified parameters;
- `IMPORTED_BREP`: exact solid/surface carrier imported from a controlled source;
- `SUPPLIER_EXACT_BREP`: exact purchased-part geometry with supplier provenance;
- `ANALYTIC_DISPLAY_PROXY`: evidence-grounded simplified B-Rep, display only;
- `MESH_PROXY`: tessellated display/collision proxy, never native CAD truth;
- `FACETED_BREP_PROXY`: a tessellation converted to faceted B-Rep, still proxy;
- `SURFACE_ONLY` or `EMPTY`: not a valid solid-volume authority;
- `FLEXIBLE_REFERENCE_POSE`: one reference state, not a routed/deformed solution.

Create one `geometry_definitions[]` record per delivered Fusion definition,
including the strict class, provenance kind, source IDs/source definition ID,
faceted-or-tessellated flag, native body counts, and registered evidence. The
only classes allowed in `accepted_native_classes` are `NATIVE_PARAMETRIC`,
`IMPORTED_BREP`, and `SUPPLIER_EXACT_BREP`. `TRIANGULATED_BREP` is not a native
class. A record labeled `IMPORTED_BREP` still fails if its provenance says
tessellated conversion or its faceted flag is true. Source-native, controlled
B-Rep, supplier-BRep, and flexible-reference provenance must resolve to the
source-definition ledger. Evidence-grounded newly authored geometry may use a
null source-definition ID, but still requires controlled source IDs and
registered evidence.

For `MANUFACTURING_HANDOFF`, every source definition classified as
`PURCHASED_MODULE` or `PURCHASED_HARDWARE` must remain
`SUPPLIER_EXACT_BREP` with `SUPPLIER_BREP` provenance. A generic imported solid
or authored look-alike is a hold, even when its body and occurrence counts
match.

Prefer controlled native CAD or exact B-Rep. A coarse triangle body must not be
used merely because it is easy to import. If mesh is needed for visualization
or collision, preserve the B-Rep authority, record linear/angular tessellation
tolerances and triangle count, and self-check mesh-derived volume or moments
against exact geometry. Refining a mesh does not promote it to native CAD.

Generic screws, nuts, or simplified purchased modules may be created only with
an explicit claim limit. A display proxy does not support supplier-exact,
thread, tolerance, tool-access, strength, or manufacturing claims.

## A5 - Freeze placement and interface contracts

Do not infer placement from appearance. Each created occurrence must bind:

```text
requirement/role + source occurrence or missing-item slot
  -> host occurrence(s) + exact face/port/landmark evidence
  -> origin + complete local basis + world transform
  -> insertion sign + seat/depth + clocking when observable
  -> definition provenance + allowed claim class
```

Repeated occurrences may share one definition but retain distinct occurrence
IDs and transforms. If only the axis or transverse center is known, retain the
unresolved seat/clocking field and hold creation rather than inventing a pose.

When the only placement source is an instruction panel or a photo, follow
`exploded-instruction-reading.md`: rank the source evidence, measure cells with
a same-panel ruler, resolve the through-arrow to one port pair, and verify the
result against the next panel's pre-existing state. An empty region in a photo
excludes candidates on that side but never fixes a pose by itself. When a
candidate set cannot be reduced to one, enumerate it in perturbation order
(same-orientation unit translations, then orientation, then identity) and
report `UNKNOWN` with the discriminating observation rather than the most
plausible candidate.
Do not mirror a placement unless the mirrored evidence and handedness are
independently valid.

Bind every present requirement occurrence to exactly one
`placement_contracts[]` record. Record its Fusion and source-lineage identity,
host occurrence IDs, a finite 4x4 `ROW_MAJOR_PARENT_TO_WORLD` transform,
placement/transform/lineage statuses, optional joint identity, and evidence.
The matrix must be an affine, orthonormal, right-handed rigid transform; 16
finite numbers or an all-zero matrix are not sufficient. Missing contracts,
duplicate occurrence identities, unknown hosts, or any non-PASS
placement/transform/lineage status block a persisted-assembly claim.

Declare one evidence-backed `placement_root` anchor for the Fusion root
component or world frame. Every top-level occurrence hosts that anchor;
multiple top-level siblings are valid. Every other host edge must lead back to
the anchor, and disconnected or cyclic host graphs fail.

Generate a product-specific acceptance profile from the closed occurrence and
interface inventories: role cardinalities, required interface pairs, expected
frame bases, fits, permitted localized overlaps, joints, limits, and default
pairwise material exclusion. Product names do not replace these invariants.

## A6 - Author in Fusion as a resumable transaction

- Use the parent `cad-agent` guarded one-shot Fusion entrypoint and exactly one
  writer. Never connect directly to the loopback Fusion MCP endpoint.
- Inspect native document state first. Resolve the exact named destination and
  no-overwrite policy before the first mutation.
- Create/reuse definitions deterministically, then create occurrences with one
  verified transform application. Preserve source-to-Fusion lineage and avoid
  duplicate insertion on resume.
- Name components and occurrences with stable semantic IDs; display labels are
  secondary and may remain human-friendly.
- Add rigid groups, Joints, or As-Built Joints only from the interface and DOF
  contract. A successful command or coincident center is not relation proof.
- Work in bounded batches with named checkpoints. After any timeout or
  ambiguous write, follow the Fusion ambiguity runbook; never blind-retry.

## A7 - Perform native acceptance and persistence readback

Before reporting a persisted assembly, read back and bind evidence for:

- saved document name, exact document-creation identity, clean modified state,
  stable data-file identity, and close/reopen receipt;
- definition, occurrence, B-Rep body, mesh body, surface-only, empty, and
  unresolved-reference counts;
- source-to-Fusion occurrence reconciliation and transform/lineage coverage;
- completeness-ledger totals, including every fastener and flexible/purchased
  category;
- duplicate or orphan occurrences and unexpected proxy geometry;
- relation identities, DOF, limits, health where natively available, and
  unaffected accepted relations;
- close/reopen persistence and a second readback of the same identities.

The v2 native readback also records duplicate and orphan occurrence counts and
a nested second readback with the same document-creation ID, data-file identity,
and occurrence count. Joint records are enumerated and reconciled to a
`joint_validation` summary; a truly static placement-only assembly may use
evidence-backed `NOT_APPLICABLE`, but an empty array cannot silently mean that
joints were checked. Component-definition and body totals must reconcile to the
per-definition geometry ledger. All required native counts are positive where
applicable; zero occurrences never satisfy persisted assembly.

For a `PERSISTED_FUSION_ASSEMBLY` claim, an unsaved document, unresolved data
identity, missing readback, source-tree discrepancy, unaccounted required
occurrence, or triangle mesh in a native-only scope is a hold or failure. A
screenshot and a visually plausible whole robot are not acceptance evidence.

Run the local manifest gate:

```bash
python3 assembly/scripts/validate_authoring_manifest.py \
  /path/to/assembly-authoring-manifest.json --check-files
```

`--check-files` is mandatory for any engineering `PASS`. Omitting it adds an
explicit file-verification hold; the validator will not emit plain `PASS` for a
non-empty claim ladder. This checks the manifest-v2 contract, evidence
referential integrity, exact source/evidence file identities and hashes,
ledger-derived closure and count arithmetic, claim
dependencies, rights/release compatibility, native/proxy provenance,
occurrence/placement/readback reconciliation, and overclaiming. It does not
inspect Fusion or interpret the content or engineering relevance of referenced
evidence. A hash proves which bytes were checked, not that those bytes support
the claim; source-native readback and evidence review remain the fact authority.

## A8 - Hand off to S0-S11 and video delivery

Feed the frozen A-lane identities and exact final transforms into S0-S11. Do
not invent assembly motion from the authoring timeline.

A `PROCESS_AND_MOTION` claim requires at least one certified motion, zero
uncertified motions, successful scene readback and media decode, and a media
SHA-256 that exactly matches a cited, hash-checked `PROCESS_RECORD` with a
`video/*` media type.

- S7 must certify the complete playable interval against the true
  sequence-aware remaining set and preserve baseline-contact semantics.
- S8 separately certifies real tool engagement, approach, stroke, and swept
  grip envelope.
- S10 animation consumes only certified trajectories. Uncertified, flexible,
  missing, or process-mediated items remain static, annotated, or held.
- The certification state chain and the rendered scene must use the same
  occurrence presence/visibility timing. Fasteners that clamp a moving part
  cannot appear as pre-existing obstacles when the certified sequence installs
  them afterward.
- Save and reopen the scene source; verify occurrence transforms and visibility
  states, then record frame count, duration, decode result, output hash, and a
  human near-field review. An MP4 alone is not the engineering evidence.

Only S11 can support fixture-calibrated physical release. A complete native
Fusion model plus a clean animation is still not manufacturing or physical
assembly approval.

## Case-derived transfer boundaries

- Lingxi X1 validates the S0-S10 analysis/animation lane, independent transform
  checks, scale-sensitive meshing, sequence-aware collision, connection-graph
  timing, and fail-closed visual review.
- Poppy demonstrates why visible main structure is not completeness: native
  trees may omit hundreds of installed fasteners and all harnesses. It also
  demonstrates exact-placement versus display-proxy claim separation.
- Fourier N1 demonstrates source-native file closure, saved versus expanded
  occurrence identity, virtual metadata subgraphs, missing-part substitution
  holds, and occurrence-level reconciliation with a partial Fusion translation.

These are method evidence, not reusable counts or PASS verdicts. Read
`assembly/references/casebook.md` for the bounded case summaries.
