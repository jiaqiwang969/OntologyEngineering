# Fusion failure recovery

This controlled policy applies the independently reviewed Lesson 01 delta. Its
source artifact has SHA-256
`bd77a8d4ab95463bfedecfdd301cb9be6d26368ab7a0924915a4fae6c16f4cde`.

When Fusion crashes, opens the wrong dialog, or reaches an ambiguous branch,
give the affected action no credit. Preserve the failed capture as excluded
evidence, return to the last saved cloud lineage/version, and inspect that
baseline before replay.

For the reproduced M09 recovery sequence:

1. Record the CER/wrong-dialog event with `committed_credit=false`.
2. Relaunch Fusion and reopen the same parent Assembly lineage at v1.
3. Verify zero occurrences and an unmodified baseline. Do not infer rollback
   from the document name alone.
4. Replay the exact Insert Component cloud-dialog path from a clean state.
5. Capture the selected saved Part and Initial Position before commit.
6. Save the same parent lineage as v2, close, reopen, and read back the linked
   external reference, identity transform, and Ground To Parent state.
7. Export the reopen-verified native checkpoint, run the deterministic check,
   and close to zero open documents.

Fail closed. Do not authorize blind retries, destructive cleanup, substitution
with local import/API creation, or promotion of a failed probe into a successful
lesson trace.

## S12 practice-proven general recovery rules

The following rules are Candidate guidance from S12 attempt-0002. They do not
replace the accepted M09 recovery sequence or authorize production mutation.

1. If an MCP response is truncated, freeze the attempt and read the active
   document, timeline, feature names, and local artifact state before deciding
   whether any mutation occurred. Never rerun a non-idempotent script blindly.
2. Evaluate both transport status and the JSON body. `CallToolResult.isError`
   can be false or absent while the content body contains `success=false` and a
   Fusion traceback.
3. Treat localized CAD library names as transport data. For script transport,
   prefer ASCII Unicode escapes and compare enumerated runtime names when
   `itemByName` can throw instead of returning null.
4. A missing API method is a capability-contract failure, not a geometry
   failure. Query the exact Fusion release contract and change only the failed
   operation. S12 uses writable `SketchPoint.isFixed`, not a nonexistent
   `GeometricConstraints.addFixed`.
5. For Cut, Join, Hole, and offset-plane failures, record local frame, world
   frame, plane origin, oriented normal, desired half-space, and participant
   body. A positive scalar distance does not define a semantic direction.
6. Feature health is necessary but insufficient. Compare the intended BRep
   topology, bounds, volume delta, and relation witnesses. A healthy no-op is a
   failed state transition.
7. BRep edge geometry is nullable. Exclude degenerate edges explicitly, but do
   not weaken expected semantic-selection cardinality to make a check pass.
8. When numeric and BRep checks are invariant under inversion, use a bounded
   source image and Peekaboo window capture to check semantic up/down or
   handedness before save.
9. After an authorized `saveAs`, distinguish `local_saved`, `cloud_pending`,
   and `cloud_identified`. While pending, perform read-only polling; never issue
   a second `saveAs` until exact-name search and DataFile state prove the first
   mutation did not commit.

For each recovery, retain the failed evidence, issue a new attempt or recovery
step ID, and rerun all downstream deterministic checks. A later success must not
erase the reason the new guardrail exists.

Two UI input-validation signatures:

- `wrong-active-command before typed value`: after a coordinate-driven UI
  selection, confirm the active command really is the dimension editor and the
  target is the intended dimension before typing a value (S04 Measure incident).
- `parameter expression parse readback`: after submitting a parameter
  expression, read back the parsed result before accepting; `'10in' -> '10
  min()'` is the known parse-trap class (S07).
