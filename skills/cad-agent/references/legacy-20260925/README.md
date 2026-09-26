# cad-agent

Canonical, self-contained **cad-agent** skill for Codex and Claude Code: an ontology-guided CAD agent
with guarded, one-shot execution against real CAD applications. It is the real directory that lives at
`~/.codex/skills/cad-agent`; every Claude profile links to it. See `ARCHIVED_ENTRYPOINT.md` for the agent-facing
instructions and `AGENTS.md` / `CONTRIBUTING.md` for the working rules and contribution policy.

## What it drives

| Executor | Where it runs | Entry point | Read first |
|---|---|---|---|
| Autodesk Fusion (built-in MCP endpoint) | this Mac, `127.0.0.1:27182` | `scripts/fusion_call.py <tool> '<json>'` | `references/fusion-execution.md` |
| Siemens NX 2412 (DreamEnding/NX_MCP bridge inside NX) | Windows workstation `dell-nb`, headless or GUI NX | `scripts/nx_call.py <tool> '<json>'`, `scripts/nx_bridge.sh start|stop|status` | `references/nx-execution.md` |
| AutoCAD 2024 (COM through an interactive-session relay) | Windows workstation `dell-nb`, visible AutoCAD | `scripts/autocad_call.py <tool> '<json>'`, `scripts/autocad_service.sh start|stop|status` | `references/autocad-execution.md` |

All three follow the same pattern: no resident MCP server is registered with the agent; every call
spawns a short-lived process, performs exactly one guarded tool call, prints JSON and exits. Nothing
to restart, nothing to leak.

## Layout

| Path | Content |
|---|---|
| `ARCHIVED_ENTRYPOINT.md`, `AGENTS.md`, `CONTRIBUTING.md` | agent instructions, working rules, contribution policy |
| `scripts/` | one-shot callers (`fusion_call.py`, `nx_call.py`, `autocad_call.py`), service drivers, `dell_ssh.sh`, semantic queries, job templates |
| `references/` | execution guides, runbooks, the S01-S27 lesson references, assembly-delivery and release workflows |
| `assembly/` | assembly ontology module (S0-S11 process gate, film/animation tool chain, validators) |
| `mechanism/` | kinematics-first mechanism module (wiper ontology + kinematics exemplar) |
| `ontology/`, `shapes/`, `sparql/`, `data/`, `contracts/` | CAD ontology, SHACL shapes, competency questions, semantic bundle, tool contracts and registry |
| `assets/` | templates plus the Dell-side sources needed to rebuild the NX and AutoCAD services (`assets/nx-mcp/`, `assets/autocad-mcp/`) |
| `dist/` | bundled `cad_agent` wheel installed into the private `.venv` by `setup.sh` |
| `lessons-inbox/` | unreviewed lessons captured while using the skill |

## Quick start

```bash
./setup.sh                 # creates .venv from the bundled wheel (python >= 3.11)
./doctor.sh                # runtime, Fusion endpoint and dell-nb executor checks
./doctor.sh --smoke        # plus one live read-only Fusion call
./doctor.sh --smoke-dell   # plus one status call to NX and AutoCAD on dell-nb

python3 scripts/fusion_call.py fusion_mcp_read '{"queryType": "projects"}'
scripts/nx_bridge.sh start && scripts/nx_call.py --status
scripts/autocad_call.py --status
```

Machine-specific values (Fusion endpoint, Peekaboo path, the Dell's address and interpreters) are
overridable through environment variables documented in the scripts and references; the Dell-side
paths default to the studio's `dell-nb` deployment.

## Using it as a skill

- Codex: keep the directory at `~/.codex/skills/cad-agent`.
- Claude Code: symlink it into `~/.claude/skills/` or `~/.claude-profiles/<profile>/skills/`
  (the studio uses `~/.agents/bin/sync-codex-skills-to-claude.sh` to link every codex skill into every profile).

`BUILD_INFO` carries the canonical content digest; recompute it with
`python3 scripts/canonical_content_digest.py` after a change, as described in `CONTRIBUTING.md`.

## Third-party components

- `assets/nx-mcp/nx_mcp_local_patches.diff` patches [DreamEnding/NX_MCP](https://github.com/DreamEnding/NX_MCP) (MIT) for NX 2412's Python binding.
- `assets/autocad-mcp/dell/acad_mcp_server.py` builds on [daobataotie/CAD-MCP](https://github.com/daobataotie/CAD-MCP) (MIT) for the drawing primitives.
