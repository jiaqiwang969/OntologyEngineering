# Optional Codex MCP registration

Use this only when the user requests registered MCP tools. Normal skill calls
continue to use `fusion_call.py` / `fusion_ops.py` directly.

`scripts/mcp_stdio.py` is a transport adapter, launched with this module's
`.venv/bin/python`. It supports two modes:

- `fusion`: the four static Fusion tool schemas; each call delegates once to
  `scripts/fusion_call.py`. No Fusion session is retained by the adapter.
- `semantica`: `semantic_doctor`, `semantic_discover`, `semantic_run` and
  `semantic_review`, delegated to the parent skill's own runtime and
  `scripts/semantic_engagement.py`. No legacy semantic engine is loaded.

For Codex, configure stdio `command` as the absolute CAD private Python and
`args` as `["/absolute/canonical/cad-agent/scripts/mcp_stdio.py", "fusion"]`
under `mcp_servers.fusion-mcp`. Use the same launcher with mode `semantica`
under `mcp_servers.ontology-engineering`. Suggested startup timeout: 30 s;
tool timeout: 900 s. The adapter shields the canonical call from cancellation;
the canonical cell remains responsible for deadlines and PID retirement.

When migrating an old installation, back up the config with mode 0600 and
disable the obsolete `cad-agent-semantic` registration. Preserve its files
and already-running sessions; do not reinstall its wheel in the new module.
The four Semantica tools replace the routing, not the old seven tool contracts.
MCP clients may require reconnect/reload to discard an already-closed transport.

Verify initialize and tools/list for both registrations. Then run
`semantic_doctor` and `semantic_discover`; verify the source identity and native
verdict. For Fusion, run external preflight before one `projects` read. Listing
four tools is not proof that the installed Fusion release supports execution.
Unsupported release, protected document and retired-PID checks stay enforced.

The bounded `2705.1.15` runtime update and its verified read-only call shapes
are documented in [release compatibility](fusion-release-compatibility.md).
