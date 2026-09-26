# Scalable design brief and change-control workflow

## Purpose

Use one requirements record as the source of truth for intent, parameters,
assumptions, interfaces, validation, and release status. For P1/P2 this may be a
compact ledger beside the practice artifact. Use the full `design-brief.yaml`
for P3, high-risk engineering, multi-party handoff, or when the project contract
explicitly requires it. Do not maintain several competing records for the same
values.

## Profile scaling

| Profile | Requirements record | Change record |
|---|---|---|
| P1 CAD work | Compact objective, key values/statuses, interfaces, unknowns, and checks | One entry per meaningful stage |
| P2 tutorial/UI evidence | P1 plus material-step/UI evidence map | One entry per credited material stage |
| P3 governed release | Full validated design brief and applicable release package | Revision-controlled audit trail |

## Status vocabulary

| Status | Meaning | Allowed use |
|---|---|---|
| `given` | Supplied and confirmed by the user or an authoritative project file | May drive geometry and checks |
| `derived` | Calculated from recorded values by a reproducible method | May drive geometry when the method and inputs are recorded |
| `assumed` | Temporary engineering assumption with uncertainty | May drive exploratory geometry only; blocks production release unless closed |
| `recommended` | AI or engineer proposal awaiting acceptance | Must not drive final geometry until accepted |
| `unknown` | Not supplied or not established | Blocks the affected operation |

## Critical fields

The following fields are critical when applicable: design objective, unit system, product envelope, interface datums and dimensions, load cases, environment, material or material-selection constraints, manufacturing process, tolerances, safety or margin requirements, and acceptance criteria.

A field is not made complete by filling it with a plausible default. A critical
`unknown` blocks only the operation or claim that depends on it. An `assumed`
critical value must identify its rationale, uncertainty, and closure condition;
P3 also requires an owner.

## Formal job directory

Use the scaffold only for a formal P3 or explicitly requested job package:

```text
<job-name>/
├── design-brief.yaml
├── execution-plan.md
├── change-log.md
├── model/
├── scripts/
├── checks/
├── exports/
└── references/
```

Treat `exports/` as non-authoritative. The regenerable source is the CAD design plus its parameters and any geometry-generation scripts.

## Full-brief completion sequence

1. Normalize the product name, job ID, owner, units, and revision.
2. Record the intended function and explicitly excluded functions.
3. Define the product envelope, keep-out zones, and interface control dimensions.
4. Record load cases and environment with units and source status.
5. Record materials, manufacturing methods, tolerances, finishes, and inspection access.
6. Create the parameter dictionary and dependency order.
7. Define deterministic geometry checks, solver checks, and physical tests.
8. Mark unresolved assumptions and identify who can close each one.
9. Run `validate_design_brief.py` before P3 execution/release. For P1/P2, validate
   only the compact facts needed by the next engineering stage.

## Parameter record

Each parameter should include:

```yaml
- name: bracket_thickness
  value: 4.0
  unit: mm
  status: given
  source: user message 2026-07-14
  valid_range:
    min: 3.0
    max: 6.0
  depends_on: []
  notes: Interface-control dimension
```

Use stable machine-friendly names. Never reuse one parameter name for different physical quantities. Do not encode units in free-form value strings when a separate unit field exists.

## Change protocol

Before a coherent write stage, identify the source revision or live document,
save target, affected objects, expected checks, and recovery method. Apply a
small coherent change set. Regenerate and run the relevant checks at the stage
boundary.

For P1/P2, record meaningful stages rather than every click or API call. P3
change logs include timestamp, actor, source revision, target revision, reason,
changed parameters/features, checks run, and unresolved effects.

## Decision protocol

When requirements conflict, do not silently optimize. Present the conflict using this format:

| Requirement A | Requirement B | Conflict | Options | Decision owner |
|---|---|---|---|---|

When multiple high-impact architectures exist, compare the viable options across
performance, manufacturability, validation burden, cost, and reversibility. Do
not silently choose an irreversible, safety-significant, or production-impacting
option for the responsible engineer.

## Formal release states

| State | Meaning |
|---|---|
| `draft` | Requirements or assumptions remain open |
| `modelled` | Geometry regenerates but verification is incomplete |
| `checked` | Required deterministic checks have evidence |
| `verified-for-prototype` | Prototype-specific checks and approval are complete |
| `released` | Production release gate and responsible approval are complete |

Never skip directly from `modelled` to `released`.
