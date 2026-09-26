# Claim and status boundaries

## Status vocabulary

- `PASS`: the exact declared claim, scope and coverage passed all applicable gates with independent evidence.
- `PASS_WITH_HOLDS`: a bounded subset passed while named, non-overlapping claims remain held. Never present it as total success.
- `HOLD`: required input, disambiguation, validation or authority is missing; further execution would create unsupported facts.
- `UNKNOWN`: the method or available evidence cannot determine the claim. Unknown is a valid result, not an invitation to guess.
- `BLOCKED`: a known prerequisite or safety boundary prevents execution.
- `FAIL`: an executed check falsified the declared claim.

Every status must name the claim, evidence, geometry class, assembly state, coverage interval and excluded scope.

## Distinctions that must survive every handoff

- **Contact is not penetration.** Intended baseline contact may be legal; new overlap, collision re-entry or topology crossing is not excused by a part-level whitelist.
- **Broad phase is not exact collision.** AABB, bounding sphere and voxel screens only nominate pairs. They do not certify clearance.
- **Sampled is not continuous.** State that a sweep used samples and give spacing/count. Do not use “continuous” unless the solver actually proves the interval under declared assumptions.
- **Proxy is not exact.** Mesh, convex hull, simplified envelope and surface shell results retain their proxy class. A visually clean animation is not collision evidence.
- **Geometric feasibility is not process truth.** STEP cannot supply actual order, torque, adhesive, tool grip, support or acceptance unless those facts are in controlled records.
- **Simulation ranking is not physical release.** A directional probe can reject or rank candidates. Fixture calibration and physical acceptance remain S11.
- **A short direction probe is not full contact validation.** S9 compares local prefixes under one fair basis. Flexible/contact-driven passage, force transfer, press, snap and cable claims require the separate P-level branch.
- **Planner success is not path certification.** A paper planner supplies a candidate state sequence. S7 must independently bind and check `q0`, mover/fixed sets, every admitted state and every edge.
- **State reversal is not transform negation.** Assembly from a disassembly result reverses the exact ordered absolute states; it does not negate translations, Euler angles or matrices.
- **Angle servo is not motor capacity.** A prescribed motion with finite tracking strength can expose contact response. Capacity requires a torque-limited motor/controller model and convergence evidence.
- **Geometric reclosure is not mechanical relock.** Visual overlap or endpoint coincidence does not prove signed feature capture, retention or pull-out resistance.
- **Delivery verification is not engineering verification.** A Blender scene, MP4 or saved-file readback can prove that evidence was expressed faithfully, but it cannot create geometry, contact, force or physical-release evidence.
- **Definition is not occurrence.** Reused geometry definitions may appear in many world poses. Claims attach to occurrences or solver units, not names alone.
- **Final-state clearance is not a path.** Check the full approach/withdrawal interval and the correct sequence-aware static set.

## Forbidden upgrades

Do not claim any of the following from weaker evidence:

- “arbitrary STEP supported” from one successful schema or one assembly;
- “all parts identified” from matching counts, names or pictures;
- “interference-free” from endpoint checks, broad-phase screens or rendered frames;
- “assembly order recovered” from CAD hierarchy alone;
- “assembly path certified” from a planner `Success` flag or a rendered reverse-disassembly movie;
- “tool reachable” from a coaxial ray alone;
- “motor sized” from servo strength or recovered contact torque without a torque-speed/current model;
- “mechanically relocked” from geometric endpoint overlap alone;
- “thread engagement validated” from a coaxial rigid path or generic contact probe;
- “physical process validated” from geometry, animation or simulation alone;
- “ontology released” from a local skill or JSON snapshot.

When the target requires a stronger claim than the evidence supports, return the narrower result and the exact next piece of evidence needed.
