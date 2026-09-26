# Curriculum working-memory index

Read only the rows that match the current lesson or CAD intent. Every entry is
Candidate working memory subordinate to `AGENTS.md`, the current Fusion
document, lesson/source evidence, and deterministic engineering checks. A row
routes knowledge; it never authorizes or blocks P1/P2 execution.

| Route | Use when the task involves | Reference | State |
|---|---|---|---|
| EXP | Every new engineering task: typed process-memory preflight, exact reuse, analogy, failure/recovery, validation guidance, and end-of-practice compilation | `workflow-experience-memory.md` | Active operational route; retrieval is advisory and never execution authority |
| ALIGN | Multi-view reference registration, image/Canvas/scan/drawing-guided shape correction, one view passing while another fails, observation-map or editable-DOF isolation, profile-bound topology qualification, derivation-scope timing, or deciding whether a visible mismatch is a registration error, feature residual, or only a symptom | `../../../../docs/reference-alignment-iteration.md` and `../../../../docs/derivation-relation-lifecycle.md` | Candidate generic alignment/derivation route; each view is registered independently, validation results never carry across a subject revision, a post-acceptance upstream lifecycle change must rerun its bound downstream consumer profile on the new exact reopen, topology refinement requires current-revision insufficiency plus a bounded plan, derivation timing is contextual rather than universal, and application-specific view/feature names remain profile bindings |
| AUD | Reviewing whether materialized Patterns remain guidance-only, need Recipe/action compilation, or have a complete intent-to-tool linkage | `workflow-experience-memory.md` | Run the read-only experience-linkage audit; it reports gaps but cannot promote or execute |
| ACQ | Supplier CAD acquisition, exact purchased-part lookup, missing STEP artifacts, Fusion catalog insertion, McMaster-Carr parts, supplier-format selection, or choosing between embedded, API, local-import, and external-browser paths | `supplier-cad-acquisition.md` | Active intent route; consumes EXP memory preflight and never grants execution authority |
| NX-DRAW | NX length/width variants, centered repeated holes, paired-hole alignment, existing drawing refresh, annotation overlap, clipped sheets, or full-folder drawing review export | `nx-variant-drawing-review.md` | Canonical operational guide with bounded NX 2412 evidence and offline helpers; no new semantic runtime route, template release, or manufacturing acceptance |
| I03 | Assembly Design, relationship recovery, joints, Motion Link, or Edit In Place | `i03-assembly-mode-workflow.md` | Candidate working memory |
| I04 | Hybrid structure, body-to-component conversion, assembly modeling, or linked context | `i04-hybrid-mode-workflow.md` | Candidate working memory |
| S01 | Course orientation, roadmap extraction, or capability contact | `s01-course-roadmap.md` | Candidate working memory |
| S02 | Installation, entitlement, account, installer, or team boundaries | `s02-installation-entitlement.md` | Candidate working memory |
| S03 | Fusion interface regions, semantic UI routing, or version-local controls | `s03-interface-semantic-map.md` | Candidate working memory |
| S04 | First-part method choice, Box versus sketch-and-Extrude, or origin intent | `s04-first-part-method-selection.md` | Candidate working memory |
| S05 | Drawing interpretation, hidden lines, bores, counterbores, or hole intent | `s05-drawing-to-hole-intent.md` | Candidate working memory |
| EXPL | Dimensionless exploded or oblique instruction panels (kit or brick manuals, assembly leaflets, step screenshots): part identification from icons, same-panel pitch calibration, through-arrow placement, next-panel verification, photo side/negative evidence, perturbation order under feedback, camera matching | `exploded-instruction-reading.md` | Candidate working memory; checked against the 2026-09 brick-tutorial fixtures (3 models, 30 steps, owner-reviewed); reproduction guidance only, never fit or release evidence |
| S06 | Revolve, annular bases, repeated holes, THRU intent, or circular patterns | `s06-revolve-pattern-intent.md` | Candidate working memory |
| S07 | Master parameters, proportional scaling, midpoint placement, or regeneration | `s07-parametric-modeling-intent.md` | Candidate working memory |
| S08 | Shared sketches, overlapping profiles, staged features, holes, or late fillets | `s08-shared-sketch-feature-staging.md` | Candidate working memory |
| S09 | Sketch interaction topology and downstream profile consumption | `s09-sketch-interaction-topology.md` | Candidate working memory |
| S10 | Component interfaces, in-context geometry, fasteners, grounding, or joints | `s10-component-interface-assembly.md` | Candidate working memory |
| S11 | Mapping geometry to curved surfaces or comparing depth direction fields | `s11-curved-surface-feature-method.md` | Candidate working memory |
| S12 | Driving/reference dimensions, missing data, projection, or angled-face features | `s12-dimension-authority-projection.md` | Candidate working memory |
| S13 | Polygon symmetry, feature mirrors, extent order, protected edges, or fillets | `s13-mirror-extent-protected-edge.md` | Candidate working memory |
| S13 evidence | Auditing the frozen S13 practice candidate and its save/readback corrections | `s13-practice-candidate-20260722.md` | Candidate advisory; never routing authority alone |
| S14 | Tangency, trim topology, blind holes, component mirrors, or appearances | `s14-tangency-trim-component-mirror.md` | Candidate working memory |
| S14 evidence | Auditing S14 practice intent, identity, handedness, and save/readback findings | `s14-practice-candidate-20260722.md` | Candidate advisory; never routing authority alone |
| S15 | Driving/driven dimensions, ordered fillets, mirrored components, or Revert Position | `s15-driving-driven-fillet-order-revert.md` | Candidate working memory |
| S15 evidence | Auditing S15 dimension authority, feature order, mirror, and frame-aware readback findings | `s15-practice-candidate-20260722.md` | Candidate advisory; never routing authority alone |
| S16 | Drawing conflicts, Delete Face healing, mixed fillets, or component mirrors | `s16-drawing-authority-delete-face-component-mirror.md` | Candidate working memory |
| S16 evidence | Auditing S16 drawing authority, healed deletion, face datum, and visual evidence findings | `s16-practice-candidate-20260722.md` | Candidate advisory; never routing authority alone |
| S17 | Boundary-grounded assembly, mating faces/keypoints, Joint frames, asymmetric revolute limits, As-Built Rigid, or interference classification | `s17-boundary-frame-kinematic-assembly.md` | Candidate engineering guidance; current artifact/state must be read from Fusion |
| S17 evidence | Auditing configuration-indexed contact, motion envelopes, direction flips, and physical-stop claims | `s17-configuration-contact-motion-envelope.md` | Candidate measurement guidance; not execution or production authority |
| S18 | Configuration-bound rendering, Named Views, Scene Settings, custom HDRI, in-canvas/local render, or rendered-artifact provenance | `s18-render-activity-provenance.md` | Candidate engineering guidance; current document, scene, render state, and output must be read from Fusion |
| S19 | Motion Study persistence, selection-context restoration, native Play/Stop/Repeat, visible playback, or time-signal companions | `s19-motion-study-context-signal.md` | Candidate release-local guidance; HTML is inspection only and native P2 credit requires observed Fusion controls |
| S20 | Parametric enclosure interfaces, authentic threaded inserts, supplier provenance, rigid face-center placement, or floor/datum recovery | `s20-parametric-box-interface-provenance.md` | Candidate engineering guidance; supplier acquisition routes through ACQ and exact current evidence |
| S21 | Lid and screw interfaces, embedded supplier-session recovery, nonmanifold finishing failures, or topology-first Fillet/Chamfer recovery | `s21-lid-fastener-topology-recovery.md` | Candidate record-backed guidance; stored gates are historical and current proposals remain non-authorizing |
| S22 | Parametric tire sections, offset wall closure, revolve/mirror construction, parameter sweeps, or exact checkpoint reopen | `s22-parametric-tire-section-closure.md` | Candidate check-grounded guidance; no missing practice log or workflow record is implied |
| S23 | Raised versus recessed tread branches, Emboss/Deboss dependencies, circular tread patterns, timeline rollback, or residual-wall validation | `s23-tread-emboss-timeline-recovery.md` | Candidate branch-aware guidance; P1/P2 credit and incomplete attempt boundaries remain explicit |
| S24 | Parametric text on a curved surface, arc-with-endpoints text carriers, projection-linked sketch planes, Extrude/New-Body appearance persistence, or batch parameter application to text | `s24-parametric-text-carrier-projection.md` | Candidate check-grounded guidance; the two S24 workflow candidates remain unreviewed and are cited, not promoted |
| S25 | Canvas-first Form modeling, single-canvas scale calibration, primitive-per-feature decomposition, span-face economy, plane-dependent symmetry, or manipulator semantics | `s25-form-reference-canvas-tspline-base.md` | Candidate guidance distilled from the accepted attempt-0005 exact v5 predecessor authority for the S26-S28 Form chain |
| S26 | Form/T-Spline Crease, Mirror Duplicate or Internal Symmetry; choosing AuthoringConstraintFromSeed, PostAcceptanceDerivation, or PreserveExistingDerivation; serialized relation generators, derived symmetry closure, control-cage versus evaluated-BRep validation, or native command lifecycle recovery | `s26-native-form-relation-lifecycle.md` | Candidate relation-aware guidance; master/derived are contextual roles, seed topology is profile-bound, and exact current Fusion state plus release-local checks remain authoritative |
| S27 | Edge-loop Alt-Move coil extension, chain-distributed smoothness, per-end Fill Hole semantics, cross-body root embedding, or transparent-canvas size authority | `s27-tapered-coil-fill-hole-size-authority.md` | Candidate guidance grounded by attempt-0003 exact cloud v3: uniform 0.35 was rejected unsaved; X/Y/Z 0.50/0.56/0.45 passed native checks, save/exact reopen, and registered Right-view top/rear remeasurement; root is derived and P2/formal credit remains unclaimed |

## Maintenance rule

- Keep one primary method reference per route; use secondary evidence references only when explicitly identified.
- Register a new route after its source and reusable engineering scope are clear.
- Remove or supersede a row when its reference is retired; never leave an orphaned route.
- Do not copy detailed lesson procedures back into `SKILL.md`.
