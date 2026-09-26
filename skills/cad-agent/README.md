# CAD Agent

Canonical CAD module inside `ontology-engineering/skills/cad-agent`.
Default: direct Siemens NXOpen/Journal, no CAD MCP; purchased parts: MISUMI China.
Mechanical methods, A0–A8 authoring, S0–S11 assembly, kinematics, physics/evidence
boundaries and manufacturing handoff remain. Formal semantics use parent Semantica.

Read [SKILL.md](SKILL.md), [NX execution](references/nx-execution.md),
[MISUMI guide](references/misumi-cn-guide.md) and
[method migration](references/nx-method-migration.md) as relevant.

```bash
bash setup.sh
bash doctor.sh --json
python3 scripts/semantic_query.py capabilities
python3 scripts/nx_direct.py --help
```

Setup installs shared engineering helpers, not commercial CAD or NXOpen. Configure
an SSH profile and licensed NX host separately. Doctor is local by default and does
not certify native modeling or license availability. Legacy Fusion/MCP/McMaster
files and the prior entrypoint are preserved as historical sources; they are not
active installation or routing defaults. Explicit legacy CLI use is guarded.

The canonical digest in BUILD_INFO covers this module; its historical wheel
identity does not describe the new direct executor. See CONTRIBUTING.md for checks.
No public package publication, production release or purchasing is implied.
