# S14 practice Candidate: intent, component identity, and assembly readback

Status: Candidate curriculum knowledge. Not an accepted semantic release.

## Generalized guidance

1. Treat explicit constraint presence and explicit constraint absence as separate design-intent evidence.
2. When a tutorial teaches Trim, preserve the pre-trim primitive and execute the actual trim dependency;
   do not replace it with directly drawn final geometry.
3. Prove handedness in a named root frame. Component-local coordinates are insufficient for assembly claims.
4. Mirror a component when the intended result is component lineage and identity; moving or copying a body
   is not procedurally equivalent.
5. Select fillet edges by geometric role, adjacency, axis and tangent status, never by transient collection index.
6. After mirror, pattern, replace or occurrence mutation, reacquire occurrences and recreate assembly-context
   proxies before root-frame checks.
7. Bind appearance semantics to a stable material/appearance ID and keep localized names as mappings.
8. Save only the successful replay, then bind local artifact hash, cloud lineage/version and post-save state.

## Candidate failure rules

- HoleFeatureInput.participantBodies on Fusion 2704.1.23 requires a native Python BRepBody list in the
  exercised execution path; generic ObjectCollection use is a tool-contract Candidate failure.
- A zero or empty assembly bound after an occurrence mutation is first a stale-handle diagnostic signal,
  not immediate proof of bad geometry.

## Governance boundary

These rules are supported by S14 attempt-0002 and may guide later curriculum diagnosis as a verified
checkpoint. Skill activation still requires frozen replay, regressions, independent review and controlled
application. They must not be treated as production-accepted ontology content.
