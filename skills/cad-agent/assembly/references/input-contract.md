# Input contract

Use this contract before interpreting a new `.step`, `.stp`, or `.p21` file. A readable STEP file is an input, not proof that the represented product can be assembled.

## Evidence lanes

Choose the narrowest honest lane.

- **Geometry lane — STEP only.** May establish file hash, schema signals, occurrence candidates, transforms, topology, exact or proxy geometry, interface candidates and computed path evidence after the appropriate parsers run. It cannot establish the actual shop-floor order, tool, torque, adhesive, acceptance instruction, support condition, handed process meaning or physical release.
- **Identity lane — STEP plus controlled BOM.** Adds named part/occurrence binding when every binding is unique and independently checked. BOM presence alone does not establish sequence.
- **Process lane — STEP plus controlled BOM and SOP/process record.** Can bind operation roles, order, tool, parameters, acceptance, preconditions and postconditions. Missing fields remain `UNKNOWN`; never fill them from habit.
- **Physical-release lane.** Requires the process lane plus fixture calibration, machine-readable acceptance and a recorded minimal physical falsification. Simulation may rank candidates but does not substitute for S11.

If only a single solid part is present, classify the input as `PART_NOT_ASSEMBLY` and route part work to the CAD skill. If assembly structure is absent but multiple bodies exist, report `STRUCTURE_AMBIGUOUS` and request or derive a controlled occurrence map without inventing one.

## Minimum project manifest

Copy `assets/project-input.template.json` into the project evidence directory and fill it without changing source files. At minimum record:

- stable `project_id` and output/evidence root;
- source file path, SHA-256, size, observed FILE_SCHEMA and any external references;
- declared unit, world frame, up axis and transform convention;
- raw-name bytes, decoded names and the chosen encoding rule;
- BOM and SOP records with hashes, revisions and authority owners;
- target claims and requested release intent;
- allowed mutations and tools;
- known unknowns, exclusions and safety boundaries.

Do not use a project name, folder name or model appearance as identity evidence.

## STEP adapter declaration

Before S1, identify the actual schema and adapter behavior. AP203, AP214 and AP242 may encode product structure and placement differently. Record which relationship direction and transform composition convention were used. If the file contains external references, hash and freeze every resolved dependency; otherwise hold the unresolved branch.

Run two independent transform interpretations whenever practical. Equality of leaf counts, a traversable tree or a plausible picture is not an independent transform check.

Declare these Part 21 text checks in every adapter:

- apostrophe escaping `''` inside names: a `'([^']*)'` pattern truncates `'0,250'' NPT TEE'` and merges two occurrences into one path;
- 72-column line wrapping of point, direction and name statements: accumulate per statement, never per line, and delete the wrap line feeds inside names instead of replacing them with spaces;
- `PRODUCT.id` versus `PRODUCT.name`: the id is the unique definition key while names may be shared by several definitions, so shape lookup by name silently loses definitions.

## Geometry intake classes

Classify every leaf before collision or mass-property work:

- `SOLID_LEAF`: one or more valid solids; exact B-Rep operations are eligible.
- `SURFACE_ONLY_LEAF`: faces/open shells but no valid solid; do not call volume, penetration depth or inside/outside facts exact.
- `EMPTY_DEFINITION`: occurrence resolves to no usable geometry; hold geometry claims and retain identity evidence.
- `FLEXIBLE_OR_DEFORMABLE`: cable, harness, spring, seal or process-deformed part; rigid STEP is only a reference pose.
- `PURCHASED_MODULE`: several CAD leaves that must move as one solver unit unless process evidence says otherwise.
- `ASSEMBLY_CONTAINER`: structural node, not a moving leaf.

Mesh a leaf only after recording tessellation tolerance and a watertightness/self-check result. A mesh proxy must stay labelled `proxy` in every downstream artifact.

## Intake decision

Run:

```bash
python3 scripts/preflight_step.py /path/to/model.step --json
```

Add repeatable `--bom` and `--sop` arguments when controlled records exist. The preflight is deliberately lexical: it flags missing standard container markers and exposes schema/assembly/encoding signals, but does not prove complete Part 21 syntax and makes no B-Rep, transform, interface, motion or release claim.
