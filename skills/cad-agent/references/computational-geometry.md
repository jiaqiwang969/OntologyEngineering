# Hybrid computational geometry routing

## Principle

Do not force every shape into a conventional feature tree, and do not force every mechanical interface into code-generated implicit geometry. Divide the product into a **performance core** and an **integration shell** when that improves traceability.

The performance core contains geometry primarily driven by equations, scalar/vector fields, topology, flow, heat transfer, lattice rules, or continuous variation. The integration shell contains conventional mechanical interfaces, datums, joints, ports, fasteners, tubes, brackets, housings, drawings, and manufacturing annotations.

## Tool-selection matrix

| Geometry characteristic | Preferred route | Why |
|---|---|---|
| Prismatic part with sketches, holes, fillets, and standard interfaces | NX parametric features | Clear dimensions, feature history, drawings, and assembly semantics |
| Tubing, external routing, supports, and service clearances | NX assembly/context modelling | Interfaces and keep-out zones are easier to inspect and modify |
| Lattice, graded porosity, conformal channel, or field-driven wall | PicoGK or another computational-geometry kernel | Geometry is generated more reliably from rules than by manual feature authoring |
| Repetitive configurable family | Parametric CAD or code-generated CAD | Choose the route that preserves named parameters and regeneration |
| Optimization result requiring cleanup and interfaces | Computational core plus NX integration shell | Preserve performance geometry while adding robust mechanical references |
| Simple concept shape without engineering requirements | Conventional CAD | Avoid unnecessary computational complexity |

## Architecture contract

Before creating an interface-breaking or reusable geometry exchange, define:

| Contract item | Required decision |
|---|---|
| Master units | One explicit length unit and conversion rule |
| Coordinate frame | Origin, axis orientation, handedness, and transform ownership |
| Interface geometry | Datum planes/axes, port centers, mating surfaces, bolt patterns, and keep-outs |
| Exchange format | STEP for B-rep when possible; mesh only when topology or downstream needs justify it |
| Regeneration owner | Which script or CAD document is authoritative for each body |
| Persistent identity | Stable names or IDs for interfaces that survive regeneration |
| Tolerance policy | Geometry approximation, tessellation, sewing, and manufacturing tolerances |
| Validation owner | Checks performed before import and after assembly integration |

## Computational-core workflow

If the generating rules or independent parameters are still unknown, use
[inverse shape parameter discovery](inverse-shape-parameter-discovery.md)
before choosing a formula or fitting a dense mesh. Return here once the
controls, constraints, and remaining ambiguities are explicit.

1. Express the design intent as engineering parameters and invariants, not as a sequence of UI clicks.
2. Record the equations, source data, field definitions, topology rules, and valid parameter ranges.
3. Generate geometry deterministically from a versioned script.
4. Validate topology, watertightness, minimum feature size, curvature or gradient limits, and expected volume before export.
5. Export with an explicit coordinate system, units, tolerance, revision, and parameter manifest.
6. Import into NX as a named component. Add mechanical interfaces in a way that does not obscure the computational source.
7. Re-run interface, envelope, mass, and interference checks after every regenerated import.

## Conventional-CAD workflow

Use NX for reference geometry, named user parameters, sketches with controlled constraints, standard features, components, joints, drawings, and manufacturing setup. Avoid fragile references to transient faces or edges when a stable datum, parameter, sketch point, axis, or interface body can be used.

When a NX feature becomes an unreadable collection of hundreds of repeated operations or depends on continuously changing fields, move that region to a computational-geometry source instead of adding more UI features.

## Exchange and versioning

For reusable/P3 generated bodies, store the applicable source package:

```text
source-code/
parameter-manifest.yaml
coordinate-system.md
interface-control.json
exported-geometry/
generation-log.txt
checks/
```

A mesh, STEP file, or native CAD file without its source and parameter manifest is an output artifact, not the complete design source.

Use semantic revisions for interfaces. A change to a port location, mating surface, bolt pattern, unit system, or coordinate transform is an interface-breaking change and requires downstream revalidation.

## Validation boundary

Computational generation can prove that a model follows encoded rules; it cannot prove that the rules are complete or physically valid. NX can prove that an exact CAD operation succeeded; it cannot prove material behavior, fatigue life, thermal margin, rotor safety, or manufacturability without the corresponding analysis and test evidence.

Always keep three evidence layers separate: geometric validity, simulated behavior, and physical validation.

## Example decomposition

For an engine-like machine, a practical split is:

| Region | Likely owner |
|---|---|
| Flow passages, continuously varying blades, cooling channels, lattice or heat-exchange core | Computational geometry plus physics solver |
| Shaft interfaces, bearing seats, flanges, fastener patterns, covers, brackets, external tubing, sensors, and assembly envelope | NX |
| Loads, temperatures, pressure fields, structural/thermal/flow response | Appropriate solvers |
| Material qualification, overspeed, containment, leakage, endurance, and maintainability | Physical test and responsible engineering review |

This split is a starting hypothesis, not a fixed rule. Choose the boundary that preserves regeneration, interface stability, and independent verification.
