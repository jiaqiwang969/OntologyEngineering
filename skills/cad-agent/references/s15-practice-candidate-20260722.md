# S15 practice Candidate: dimension authority, ordered features, and frame-aware mirror readback

Status: Candidate curriculum knowledge. Not an accepted semantic release.

## Generalized guidance

1. Classify every sketch dimension as driving or driven/reference; do not use a reference closure value to
   over-constrain an already determined profile.
2. Preserve pedagogical warning transitions. The recovery action is part of the learned design intent.
3. Treat feature order as a dependency: a downstream fillet consumes topology produced by upstream fillets.
4. Select edges by geometric role, adjacency, tangent status, axis, span, and named feature context, never by index.
5. Use a true component mirror when the lesson requires component identity and lineage.
6. Distinguish component-native geometry, occurrence transforms, and assembly-context proxies before making
   bounds, overlap, handedness, or interference claims.
7. A part-definition overlay may intentionally place left/right definitions in the same assembly location;
   final interference acceptance belongs to the declared final-assembly lesson.
8. Test Move/Revert Position with matrix and pending-snapshot witnesses, not visual position alone.
9. A solver-history tolerance must remain narrow, version-bound, explicitly evidenced, and non-promotable;
   topology, bounds, settings, and provenance checks remain strict.
10. Save through Fusion MCP/API, then prove unique cloud lineage, version, and unmodified state.

## Candidate failure rules

- If a closure dimension triggers over-constraint, inspect dimension authority before deleting constraints.
- If fillet topology changes, inspect source-edge role and tangent-chain state before changing radius.
- If native and root bounds disagree, reacquire occurrence proxies before diagnosing geometry.
- If exact volume differs with identical topology and bounds, freeze evidence and classify the kernel-history
  delta; do not silently relax the gate.

## Governance boundary

These rules are supported by S15 attempt-0002 and may guide later curriculum diagnosis. Skill activation
still requires frozen replay, regressions, independent review, and controlled application. They must not be
treated as production-accepted ontology content.
