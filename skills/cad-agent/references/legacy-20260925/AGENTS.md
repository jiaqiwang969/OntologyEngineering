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
  module. The bundled runtime wheel contains Fusion execution only; do not
  reinstall the legacy `cad-agent-mcp` wheel into this module's `.venv`.
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
- Before any live Fusion call, read `references/fusion-execution.md`. For a
  timeout, ambiguity, modal, crash, or recovery boundary, also read the bundled
  runbooks it names. Never connect directly to Fusion's loopback MCP endpoint.
- Treat every untitled or never-saved Fusion document as protected. First save,
  naming, project/folder resolution, and readback use guarded MCP/API operations,
  never GUI shortcuts. A timeout retires that PID from further MCP requests.
- No live Fusion write, close, restart, publish, deletion, or release is implied
  by editing this skill.

## Assembly and mechanism boundaries

- Source-to-Fusion authoring uses A0-A8 in
  `references/source-to-fusion-assembly-delivery.md`; supplied-assembly process
  analysis uses S0-S11 in `assembly/SKILL.md`.
- Complete assembly claims require requirement/occurrence accounting, native
  geometry evidence, placement and source-to-Fusion reconciliation, persisted
  readback, and the applicable S0-S11 checks. Missing screws, nuts, washers,
  pins, purchased modules, harnesses, or unexplained count deltas remain HOLD.
- A manifest validator PASS is never a substitute for inspecting Fusion or for
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
   migration source provenance and the locked Fusion-only wheel identity.

Formal Semantica regression, release, promotion, manufacturing release, or
public redistribution requires its own explicit evidence and authorization.
Routine practice must not invent those gates, and a plausible model, image,
animation, or self-reported manifest must never upgrade unresolved evidence.
