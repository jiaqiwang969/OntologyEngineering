# S17 boundary-frame kinematic assembly

## Current routing note

This file preserves Candidate lessons from the A0003–A0007 evidence chain. It
does not authorize recreating a historical target, replaying prior transforms,
or treating the ledger below as current spatial truth. Read the live artifact,
occurrences, relations, frames, motion state, and persistence directly from
Fusion. Historical attempts remain evidence, not current routing.

## Authority and lifecycle

Use this reference only with the generic I03 assembly workflow for an isolated
S17 retry. It is Candidate working memory, not current execution authority,
Accepted knowledge, production authority, or a semantic release.

Frozen sources:

- Video SHA-256: `59c93b055845d1432996b93fc6cab3f9ece9ca782605d016bfe1a075d90ea734`
- Subtitle SHA-256: `86b609888d8713fcc85a24b958d01ea65a54fe835c709d2e7f41dff26e10a8d7`
- Aameri/Cheong/Beck paper SHA-256: `0072aff286144700eb7c9587ae50e99813184217afc1b0175cc49abf64a77c1c`
- Video Vision session: `adf34102f2bb`
- Fusion release: `2704.1.23`

The paper supplies the weak static foundation: assembly proper parts, boundary entities, incidence,
mating features, connection, relative configuration and rejection of unexplained co-occupation. It
does not supply Fusion Joint frames, relative orientation, revolute state, limits or execution paths;
those remain explicit CAD Agent extensions.

## Source-derived relation ledger

| Relation | Moving interface -> fixed interface | Construction and alignment | Kinematics |
|---|---|---|---|
| `Head to Torso` | Head bottom annular planar shoulder, center -> Torso neck-stud top planar face, center | Standard; source angle `0 deg`; XYZ `0` | Revolute Z; `-120/+120 deg`; neutral `q=0` |
| `Hips to Torso` | No replacement mating selection; components already share the intended pose | As-Built; preserve existing transform | Rigid; 0 DOF |
| `R Leg to Hips` | Right-leg broad planar pivot face/hole center -> right hip axle/disc end face center | Standard; source angle `180 deg`; XYZ `0` | Revolute Z; `-60/+90 deg`; neutral `q=0` |
| `L Leg to Hips` | Left-leg broad planar pivot face/hole center -> left hip axle/disc end face center | Standard; source angle `0 deg`; XYZ `0` | Revolute Z; `-90/+60 deg`; neutral `q=0` |
| `L Arm to Torso` | `Right Arm(Mirror)` shoulder planar face center -> Torso semantic-left shoulder face center | Standard; source angle `180 deg`; XYZ `0` | Revolute Z; `-125/+135 deg`; neutral `q=0` |
| `R Arm to Torso` | `Right Arm` shoulder planar face center -> Torso semantic-right shoulder face center | Standard; source angle `180 deg`; XYZ `0` | Revolute Z; `-135/+125 deg`; neutral `q=0` |
| `L Hand to L Arm` | `Left Hand` wrist-peg planar end center -> mirrored-arm wrist annular end center | Standard; source display `360 deg`, normalized `0`; local Z `-0.060 in` | Revolute; no limits; neutral `q=0` |
| `R Hand to R Arm` | `Left Hand(Mirror)` wrist-peg planar end center -> right-arm wrist annular end center | Standard; source angle `0 deg`; local Z `-0.060 in` | Revolute; no limits; neutral `q=0` |

Position-frame Flip and Motion-direction Flip are separate evidence fields. Do not guess either from a prior artifact. Derive the
complete frame from each selected boundary chain, compare the resulting pose and axes with the frozen
frames, then record the actual Fusion `isFlipped` readback as realization evidence.

For S17, the fresh video review shows no Position-tab frame Flip. It does show a left-leg Motion-tab
direction Flip. Record these as `positionFrameIsFlipped=false` and
`motionDirectionIsFlipped=true`; do not collapse them into one UI-free boolean.

The audio phrase `negative sixty thou` means `-0.060 in`; ASR output resembling `negative 60,000` is
invalid. Audio can establish intent, sequence and unit context, but synchronized frames govern boundary,
axis, sign and UI state.

## Negative precedent and clean-retry rule

`S17_Minifigure_Assembly_A0003` is a failed predecessor even though an earlier record claimed PASS. Its
activation was later revoked, and its currently unsaved right/left leg Joint alignment angles are both
`90 deg`. Those values conflate frame alignment with motion state and must not be saved, copied or used as
a successful template.

The historical recovery created a clean lineage rather than modifying A0003.
Any current repair should likewise use the user-designated practice artifact or
copy, re-derive interfaces from its exact geometry, and avoid copying A0003
Joint data, compensating transforms, or alignment angles.

## S17 engineering checks

- one grounded Torso and nine semantic leaf occurrences;
- seven standard Revolute relations and one As-Built Rigid relation with exact role pairs;
- every standard relation records boundary, key point, both local frames and resulting frame;
- alignment values, current `q`, neutral `q` and limits remain separate and match the ledger;
- source/mirror names remain provenance while anatomical side is a semantic role;
- zero unexpected overlap at neutral pose and declared samples across each bounded motion envelope;
- intended peg/socket occupancy is confined to the named mating-interface envelope;
- front and oblique views agree with the source without overriding deterministic failures;
- save, close/reopen by lineage, and readback preserve the complete relation ledger.

## A0006 reproduced checkpoint (attempt-0005)

The complete S17 reconstruction was repeated from an empty assembly using the ordered video material steps and explicit interface evidence. The saved artifact is 'S17_Minifigure_Assembly_A0006', lineage 'urn:adsk.wipprod:dm.lineage:0d71SLa7RwuxfWXBskI9Fw', version 'urn:adsk.wipprod:fs.file:vf.0d71SLa7RwuxfWXBskI9Fw?version=1'.

The decisive correction is that a Fusion joint location is not merely a point. It is a boundary selection plus a key-point construction plus a local frame:

- head/torso uses planar-face center-of-area construction;
- leg/hip, arm/torso, and hand/arm use the selected planar face plus the intended circular-edge center;
- alignment angle establishes the zero frame;
- current revolute value is the present configuration;
- limits constrain future motion and cannot compensate for a wrong zero frame.

The reopened artifact retained six root occurrences, fifteen total occurrences, seven standard revolute joints, one as-built rigid joint, all occurrence transforms, all joint states, and cloud lineage. The maximum pre/post transform delta was '0.0'. Non-connected interference was zero. Small overlaps were limited to declared connected interfaces and are retained as measured candidate evidence rather than hidden by a blanket no-interference claim.

Visual comparison uses the tutorial's front-right oblique camera for source matching. Orthogonal front and isometric views remain diagnostic sidecars. A face seen from an arbitrary camera is not sufficient evidence for changing a correctly reproduced joint state.

This checkpoint is a reproduced curriculum candidate. It is not an independently accepted semantic release or production activation.
