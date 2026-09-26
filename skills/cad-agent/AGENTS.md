# Engineering-ontology CAD module working rules

These rules apply to every file below `ontology-engineering/skills/cad-agent`.

## Authority

- This nested directory is the real canonical CAD module. A former top-level
  `~/.codex/skills/cad-agent` copy, if still present, is legacy compatibility
  content; it may be archived after caller migration and is not authoritative.
- `~/120-agent-cad/.../.agents/skills/cad-agent` is a legacy mirror/external
  upstream, not the source of truth. Never rebuild or `rsync --delete` from it
  into this directory.
- The parent ontology-engineering skill is the semantic control plane;
  source-locked Semantica is the only formal CQ/SHACL/rule and review executor.
  The former CAD ontology and local validators are archived outside this
  module. Fusion execution has been removed; do not reinstall its archived runtime
  or the legacy `cad-agent-mcp` wheel into this module's `.venv`.
- Make reviewed operational contributions directly here. Before editing, read
  `SKILL.md`, `CONTRIBUTING.md`, and only the module references relevant to the
  task.
- Preserve unrelated canonical content. Do not import a whole repository tree
  when a bounded file-level delta is sufficient.

## Routine execution

- Routine CAD work uses this module's private `.venv`, scripts, references and
  native CAD tools. It does not require repository `docs/`, curriculum records,
  or a `CAD_AGENT_ROOT` checkout. CAD/process handoff projects evidence into a
  project ABox; an integrity result is not a Semantica or physical verdict.
- A stale `CAD_AGENT_ROOT` must not redirect canonical wrappers. Repository-
  bound formal lifecycle work is a separately declared mode and uses that
  repository's own entry points.
- Default CAD is direct NXOpen/Journal; read `references/nx-execution.md`. No CAD MCP.
- Use one writer, a stable host job root, fresh task-owned files and fresh-process
  readback. A timeout triggers inspection of the same job, never blind rerun.
- Fusion is outside the tool scope and has no opt-in execution route.
  Historical lessons may supply mechanical methods, never software activation.
  Skill maintenance does not authorize
  closing user documents, restarting services or modifying customer originals.

## Assembly and mechanism boundaries

- Source-to-native authoring uses A0-A8 in
  `references/source-to-native-assembly-delivery.md`; supplied-assembly process
  analysis uses S0-S11 in `assembly/SKILL.md`.
- Complete assembly claims require requirement/occurrence accounting, native
  geometry evidence, placement and source-to-native reconciliation, persisted
  readback, and the applicable S0-S11 checks. Missing screws, nuts, washers,
  pins, purchased modules, harnesses, or unexplained count deltas remain HOLD.
- A manifest validator PASS is never a substitute for inspecting native CAD or for
  reviewing whether cited evidence semantically proves its claim.
- Historical mechanism ontology builders/validators that require unshipped
  source evidence are provenance references, not standalone executable gates.

## Contribution checks

After a change, run checks proportional to scope:

1. skill-creator `quick_validate.py` on this root and any changed nested skill;
2. the affected bundled validator/self-test and static syntax/lint checks;
3. `doctor.sh` without `--smoke` unless a live guarded read is required;
4. adversarial negative tests for any changed PASS/HOLD/FAIL gate;
5. update the canonical content stamp in `BUILD_INFO` while preserving the
   migration source provenance and the historical Fusion-only wheel provenance.

Formal Semantica regression, release, promotion, manufacturing release, or
public redistribution requires its own explicit evidence and authorization.
Routine practice must not invent those gates, and a plausible model, image,
animation, or self-reported manifest must never upgrade unresolved evidence.
