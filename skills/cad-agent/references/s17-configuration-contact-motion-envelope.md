# S17 configuration, contact, and motion-envelope candidate

## Current routing note

This reference is an advisory measurement checklist. It does not certify a
historical artifact or determine the current target. Resolve the live artifact
and occurrence identities from Fusion, then bind every sample and claim to that
exact configuration and native readback.

## Trigger

A0006 proved final neutral-state relation completeness, static interference classification, and save/reopen persistence. It did not prove axis alignment, fit clearance, a collision-feasible motion path, first contact, or a directional physical stop.

A read-only audit also found that A0006 copied one API Position flip into every joint and used symmetric arm limits. Targeted video review proves that the tutorial uses distinct Motion-tab Flip semantics and asymmetric arm limits.

## Video evidence corrections

- '00:07:08-00:07:16': the leg is previewed at '-60 deg'; the instructor uses the Motion-tab Flip because the signed motion goes the wrong way. This is not evidence for Position-frame Flip.
- '00:08:55-00:09:12': the first/left arm converges to minimum '-125 deg', maximum '+135 deg'.
- '00:09:47-00:09:56': the second/right arm converges to minimum '-135 deg', maximum '+125 deg'.

## Physical evidence policy

Static exact evidence can use Fusion 'Design.analyzeInterference'. Minimum distance can use 'Application.measureManager.measureMinimumDistance'.

No dedicated continuous collision operation is currently exposed by the Fusion MCP endpoint. A sampled sweep must therefore report:

- requested and actual joint coordinate;
- complete moving occurrence identity, never body name alone;
- minimum forbidden-pair clearance;
- connected-interface and non-connected interference separately;
- sample spacing and solver version;
- last-safe and first-colliding bracket when found;
- 'continuousCollisionCertified=false' unless every interval is conservatively bounded.

For a revolute interval with moving-set radius 'R' and angular interval width 'delta', the maximum point displacement bound '2 R sin(|delta|/2)' can certify an interval only when the measured forbidden-pair clearance exceeds that bound plus the approved distance tolerance. Near-zero unresolved distance is 'UnknownNearContact', not pass.

## Lifecycle

This is a curriculum capability candidate for a new S17 attempt. It is not an independently Accepted semantic release and does not authorize a production collision claim.

## A0007 verified execution rules

The following rules were reproduced in Fusion 2704.1.23 by S17-A0007 and remain curriculum Candidate knowledge, not production release:

1. Keep source `positionFrameIsFlipped`, source `motionDirectionIsFlipped`, and `FusionJointExecutionRecord.fusionApiJointInputIsFlipped` independent. Never infer one from another without a release- and selection-order-specific mapping test.
2. For a standard Joint axial offset, write and read back `Joint.offset` (`ModelParameter.expression` and `.value`). A tool accepting `JointInput.offsetZ` did not prove that the tutorial's `-0.060 in` offset was applied.
3. When an interference response loses occurrence context, issue one request per frozen semantic role pair and inherit participant identity from the request envelope. Body names are not valid identity for mirrored or reused components.
4. Record `overlap_found`, quantitative `collisionVolume`, minimum distance, and contact classification separately. If Fusion reports overlap but volume evaluation fails, preserve the kernel error and classify the quantitative value as `UnknownKernelValidation`; never replace it with zero.
5. Configuration evidence includes the transition procedure. For Fusion sampled sweeps, use `neutralize -> immediate role-pair baseline -> apply q -> observe -> restore neutral`. A nominal q value reached through another solver history is not assumed observationally equivalent.
6. A kinematic limit is not a collision constraint. Report the video limit and the evidence-bounded collision-free interval as separate relations.
7. Persistence routing distinguishes lineage ID, version ID, and a live `DataFile` object. After save, resolve by lineage and wait for the version to advance and stabilize before closing; reopen that stable `DataFile`, then verify lineage, version, q=0, parameters, static interference, and `isModified=false`.
8. If a motion audit is interrupted, its first recovery action is deterministic neutral restoration of every joint, followed by save, stable-version reopen, and a write-once recovery report. Do not continue sampling from the interrupted solver state.
