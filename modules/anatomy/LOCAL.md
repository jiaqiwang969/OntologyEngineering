# Anatomy × OntologyEngineering v2

This directory is the standalone `anatomy` professional module. The upstream
kit remains the renderer and figure-craft implementation; OntologyEngineering
owns Project identity, source custody, object revisions, conditions, evidence,
decisions and adoption.

## Routing

Use the module for a technical explanation that benefits from a crafted
isometric figure, interactive SVG, WebGL effect, 3D turntable, printable sheet
or a close visual review of a mechanism. Do not use it as a substitute for
native CAD, drawing release, BOM review, manufacturing inspection or medical
anatomy.

1. Resolve the absolute `ONTOLOGY_ENGINEERING_ROOT` and read the current
   Project workspace. Select the object revision, source and condition that
   the figure will explain; do not infer them from the current directory or a
   previous figure.
2. For a bounded offline figure, put a `PresentationScene1` source under the
   active root, then run the native sequence:

   ```sh
   "$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" presentation bind \
     --project var/projects/<project>/project.json \
     --source <source.json> --object <object-id> \
     --condition <condition> --title '<title>' \
     --output var/projects/<project>/presentation-request.json
   "$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" presentation generate \
     --project var/projects/<project>/project.json \
     --input var/projects/<project>/presentation-request.json \
     --output var/presentations/<new-delivery>
   "$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" presentation verify \
     --project var/projects/<project>/project.json \
     --directory var/presentations/<new-delivery>
   ```

   The native path produces offline HTML/SVG and verifies the source,
   Project context, contract and output bytes. It is the default because its
   boundary is explicit and reproducible.
3. For an advanced Anatomy figure, use the copied `kit/`, `examples/` and
   `scripts/` at this pinned module revision. Keep the build output in a new
   task-owned directory. Record the exact input source, Project context,
   object revision, condition, Node/Chrome identity, command, output hashes,
   capture/audit coverage and any failure or skipped check.

4. For a CAD-backed figure, project the verified v2 NX observation first. The
   projection keeps the occurrence/prototype hierarchy, source SHA and solid
   corners in `projection.json`, and writes `source.json` when the bounded
   `PresentationScene1` proxy fits the renderer budget:

   ```sh
   "$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" cad presentation project \
     --request var/projects/<project>/cad-presentation-request.json \
     --observation var/projects/<project>/cad-occurrence-binding-report.json \
     --output var/presentations/<new-cad-projection>
   ```

   The command accepts the verified occurrence report (`/assembly/assembly`)
   and also accepts a direct `NxAssemblyObservation` for bounded fixtures.
   Use the returned `source.json` as the source for the native presentation
   bind sequence above. Keep `projection.json` and `receipt.json` beside the
   figure; they are the link back to the CAD evidence. If an ordered CAD
   scene is supplied, its samples remain discrete source poses. The current
   proxy depicts the initial pose and does not claim continuous motion.
5. Return the result through the existing Project tool-result and review
   contracts. Do not edit `project.json` with a figure script and do not turn
   a visual pass into an engineering adoption decision.

## Evidence boundary

The figure can support visual explanation, presentation and a bounded
geometric or browser observation. A successful `presentation verify`, Anatomy
geometry audit, screenshot, PDF export or browser check does not establish
native CAD equivalence, collision-free continuous motion, BOM correctness,
material or manufacturing suitability, physical test results or engineering
adoption. Keep those claims under their existing owners and record unknowns.

The upstream community publisher is opt-in. Never call `scripts/publish.mjs`
unless the user explicitly requests publication and the output has already
been preserved locally with its source and Project receipt.

## Source and local delta

The copied upstream tree is fixed to
`wheresryan22/anatomy@bc943e40c59998be5da61f9d39c6a983b1116099`. The local
delta consists of this file, the OntologyEngineering section appended to
`SKILL.md`, the frontmatter normalization required by the local Skill
validator, and `SOURCES.json`; the upstream kit, examples, references and
checks remain available under their original paths. `LICENSE` and the root
third-party notice are retained. Updating the upstream tree requires a clean
checkout, a new source comparison, a new local receipt and rerunning the
module checks; do not silently follow `main`.

The registered source identity is `anatomy_projection_source` in
`specs/modules/cad-engineering-binding.toml`. That registration covers the
upstream projection and presentation method; it does not promote a figure or
the upstream browser audit into native engineering evidence.
