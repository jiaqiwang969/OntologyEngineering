# S13 practice Candidate: mirror, extent order and protected edges

Status: Candidate advisory only. Not Accepted, not active for routing, and not production-ready.

## Proven observations

- The two octagonal posts use eight sketch-level symmetry dependencies.
- The left axle uses one feature-level mirror of the right axle with IdenticalPatternCompute.
- The lug uses a symmetric through-all cut before a symmetric whole-length join.
- The axle datum is derived from AxleHalfLug=LugThickness/2, not from generated topology.
- Exactly two axle-root circular edges remain sharp and are excluded from the typical fillet set.
- Aggregate sketch isFullyConstrained can be false while all 16 polygon lines, the construction circle and eight symmetry relations satisfy the declared constraint contract.
- Fusion 2704.1.23 exposes addDistanceDimension/addDiameterDimension and treats Occurrence.name as read-only.
- A successful unsaved-document saveAs can first expose local cache identity; poll lineage/version and never repeat saveAs merely because identity propagation is pending.

## Governance correction

Any older S13 note that leaves the fresh curriculum document unsaved is superseded by the repository save barrier. Attempt-0002 proved local F3D export, one cloud saveAs, stable lineage/version readback and post-save visual evidence.

## Required use

Use this reference only as an advisory Candidate for S14 and later retained-task checks. The accepted ontology remains the controlling input until independent review, Git binding, controlled application and regression complete.

Evidence root: curriculum-learning/S13/attempt-0002
