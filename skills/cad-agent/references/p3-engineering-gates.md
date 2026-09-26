# Engineering verification and release gates

This reference governs P3 release and risk-scaled engineering validation. It
does not require ordinary P1/P2 practice to create a full release packet. Select
only the checks needed for the claim during practice; require the full
applicable chain before prototype, manufacturing, production, or certification
claims.

## Evidence hierarchy

Keep the following claims separate. Passing an earlier layer never implies passing a later one.

| Layer | Typical evidence | Permitted claim |
|---|---|---|
| Intent traceability | Approved design brief and parameter mapping | The model represents recorded requirements and assumptions |
| Geometric validity | Regeneration, topology, dimensions, mass properties, interference | The CAD geometry is internally valid under the checked conditions |
| Manufacturability review | Process-specific rules, tool access, minimum feature, drawing review | The design is a candidate for the named manufacturing process |
| Simulation | Defined model, boundary conditions, convergence, sensitivity, margins | The mathematical model predicts the recorded response under stated assumptions |
| Prototype test | Inspection and controlled physical test data | The tested article met recorded prototype criteria |
| Production release | Applicable standards, quality plan, responsible approvals, controlled revision | The recorded revision is released for the defined production scope |
| Certification | Evidence accepted by the responsible authority | Only the authority-approved scope is certified |

## Minimum geometry checks

Select applicable checks and record numeric results, tool/version, input revision, pass criterion, and evidence path.

| Check | When required | Example evidence |
|---|---|---|
| Regeneration/build health | Whenever a regenerable model result is claimed | No failed timeline features; script exit status and log |
| Body validity | When solid validity is part of the result or downstream use | Valid B-rep/solid status |
| Watertightness/manifoldness | Closed fluid or manufactured body | Closed-volume or manifold report |
| Bounding envelope | Packaging-controlled designs | Min/max coordinates compared with keep-out |
| Interface dimensions | Every mating interface | Datum-based measurements and tolerance |
| Minimum wall/feature | Casting, machining, additive, pressure boundary | Minimum value and location |
| Clearance/interference | Assemblies and moving/service regions | Interference report and intentional-contact disposition |
| Mass properties | Weight, balance, inertia, or load-sensitive designs | Volume, mass, center of mass, inertia tensor |
| Draft/tool access | Moulding, casting, machining | Draft analysis or accessibility result |
| Curvature/continuity | Sealing, flow, styling, or manufacturing-sensitive surfaces | Curvature/continuity report |
| Mesh/tessellation quality | Mesh exchange or additive workflow | Chord tolerance, angle tolerance, manifold check |

A screenshot may accompany a check but must not be the only evidence for a numeric or topological claim.

## Simulation gate

Before interpreting a solver result, record geometry revision, solver and version, material model, load cases, boundary conditions, contacts, mesh strategy, convergence measure, solver tolerances, simplifications, and acceptance criterion.

For each critical output, run an appropriate convergence or sensitivity study. Separate numerical uncertainty, model-form uncertainty, input uncertainty, and manufacturing variability. Do not report excessive precision.

A simulation report must state what failure modes are outside the model. For high-consequence rotating, pressure, thermal, fatigue, or containment functions, define a physical correlation or qualification test.

## Prototype gate

A prototype release must include controlled drawings or build data, material and process identification, inspection plan, instrumentation, safe test envelope, abort criteria, responsible test owner, and post-test inspection.

Do not infer production readiness from one successful demonstration. Record article serial/revision, actual dimensions, material certificates when applicable, test conditions, anomalies, repairs, and deviations.

## Manufacturing handoff gate

Before generating or releasing final STEP, drawing, STL/3MF, NC, or additive build data, verify:

| Area | Release requirement |
|---|---|
| Identity | Part number, name, revision, configuration, and source document |
| Units and datums | Explicit units, coordinate system, datum scheme, and orientation |
| Definition | Dimensions, tolerances, materials, finishes, notes, and acceptance criteria |
| Process | Named manufacturing process, machine/build constraints, post-processing, and inspection access |
| Interfaces | Controlled mating dimensions, keep-outs, sealing surfaces, and fastener specifications |
| Evidence | Required geometry checks passed; simulation/test evidence linked where required |
| Exceptions | Open assumptions, deviations, waivers, and owners visible |
| Approval | Explicit responsible-person approval recorded with scope and date |

The AI may prepare export settings and a release candidate. It must not perform the final production-impacting export or submission without explicit approval.

## Risk-scaled depth

| Consequence of failure | Minimum process |
|---|---|
| Low: visual mock-up or nonfunctional fit study | Brief, units, envelope, basic geometry checks, versioned export |
| Moderate: fixture, enclosure, noncritical mechanism | Interface control, material/process review, interference, tolerance and inspection plan |
| High: pressure, high speed, high temperature, structural safety, human proximity | Responsible engineer, applicable standards, independent review, solver verification, controlled prototype testing, quality and release plan |

When uncertain, choose the higher level and ask the user to identify the responsible engineer and applicable standard.

## Verification report status

Use only `pass`, `fail`, `not-run`, or `not-applicable`. A warning is not a pass. A failed check requires an approved disposition; an unknown critical check blocks release.

The final report must summarize requirement coverage, model revision, checks, evidence locations, failed or missing evidence, residual risk, unresolved assumptions, and approval state.
