#!/usr/bin/env python3
"""Semantica-only MCP transport over the canonical source-locked CLI.

CAD execution uses direct NX journals and is not exposed by this transport.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess

import anyio
from jsonschema import validate
from mcp.server.lowlevel import Server
from mcp.server.stdio import stdio_server
from mcp.types import CallToolResult, TextContent, Tool, ToolAnnotations

CAD_ROOT = Path(__file__).resolve().parents[1]
OE_ROOT = CAD_ROOT.parents[1]
SEMANTIC_PYTHON = OE_ROOT / "runtime/.venv/bin/python"
SEMANTIC_CLI = OE_ROOT / "scripts/semantic_engagement.py"
SEMANTIC_SCHEMA = "ontology-engineering.semantic-engagement-response/v1"


def _semantic_tool(name: str, description: str, fields: tuple[str, ...],
                   required: tuple[str, ...] = ()) -> Tool:
    return Tool(
        name=name, description=description,
        inputSchema={
            "type": "object",
            "properties": {key: {"type": "string", "minLength": 1} for key in fields},
            "required": list(required), "additionalProperties": False,
        },
        annotations=ToolAnnotations(readOnlyHint=True, destructiveHint=False,
                                    idempotentHint=True, openWorldHint=False),
    )


SEMANTIC_TOOLS = (
    _semantic_tool("semantic_doctor", "Verify the canonical Semantica source lock and runtime; no ontology execution.",
                   ("binding",)),
    _semantic_tool("semantic_discover", "Discover source-locked packages and capabilities; no promotion or release.",
                   ("binding", "workspace")),
    _semantic_tool("semantic_run", "Run an exact bound package/scenario through Semantica. Requires a current task; inspect native verdicts and receipts.",
                   ("binding", "task", "workspace", "scenario", "recorded_at"),
                   ("binding", "task")),
    _semantic_tool("semantic_review", "Review project evidence against an adopted promoted package through Semantica; never physical or manufacturing acceptance.",
                   ("binding", "task", "workspace", "evidence_file", "source_id", "format",
                    "scope", "focus", "focus_type", "query_asset", "shape_asset", "recorded_at"),
                   ("binding", "task", "workspace", "evidence_file", "source_id", "format",
                    "scope", "focus", "focus_type", "query_asset", "shape_asset")),
)


def tool_catalog(mode: str) -> list[Tool]:
    if mode == "semantica":
        return [tool.model_copy(deep=True) for tool in SEMANTIC_TOOLS]
    raise ValueError("Only Semantica is available in this MCP transport; CAD uses NXDirect")


def build_command(mode: str, name: str, arguments: dict) -> list[str]:
    catalog = {tool.name: tool for tool in tool_catalog(mode)}
    if name not in catalog:
        raise ValueError("unknown tool for this transport")
    validate(arguments, catalog[name].inputSchema)
    # Fixed tool names select fixed CLI subcommands. A value beginning with --
    # stays a value, never a new option; no shell and no arbitrary argv surface.
    command = name.removeprefix("semantic_")
    return [str(SEMANTIC_PYTHON), "-B", str(SEMANTIC_CLI), command] + [
        "--" + key.replace("_", "-") + "=" + value
        for key, value in arguments.items()
    ]


def result_for(mode: str, payload: dict, returncode: int) -> CallToolResult:
    if mode != "semantica":
        raise ValueError("Unsupported transport mode")
    ok = (returncode == 0 and payload.get("$schema") == SEMANTIC_SCHEMA
          and payload.get("command_verdict") == "passed")
    # Do not replace the canonical taxonomy, native receipt or blocked verdict.
    return CallToolResult(
        content=[TextContent(type="text", text=json.dumps(payload, ensure_ascii=False))],
        structuredContent=payload, isError=not ok,
    )


def invoke(mode: str, name: str, arguments: dict) -> CallToolResult:
    argv = build_command(mode, name, arguments)
    env = dict(os.environ)
    for key in ("CAD_AGENT_ROOT", "CAD_AGENT_SKILL_DIR", "PYTHONPATH", "PYTHONHOME"):
        env.pop(key, None)
    env["PYTHONUNBUFFERED"] = "1"
    proc = subprocess.run(argv, cwd=OE_ROOT, env=env, text=True, capture_output=True)
    try:
        payload = json.loads(proc.stdout)
        if not isinstance(payload, dict):
            raise ValueError("canonical entrypoint returned a non-object")
    except (ValueError, TypeError) as exc:
        payload = {"transport_error": str(exc), "exit_code": proc.returncode,
                   "stderr_tail": proc.stderr[-4000:]}
    return result_for(mode, payload, proc.returncode)


def create_server(mode: str) -> Server:
    tool_catalog(mode)  # Reject unsupported modes before starting a server.
    server = Server(
        "ontology-engineering",
        version="1.1.0",
        instructions=(
            "Semantica-only transport. CAD uses direct NX journals. Semantics run exclusively "
            "through the ontology-engineering source-locked Semantica CLI; "
            "native semantic verdicts do not authorize CAD or product release."
        ),
    )
    lock = anyio.Lock()

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return tool_catalog(mode)

    @server.call_tool(validate_input=True)
    async def call_tool(name: str, arguments: dict) -> CallToolResult:
        async with lock:
            # Preserve the canonical CLI's result and cleanup on cancellation.
            with anyio.CancelScope(shield=True):
                return await anyio.to_thread.run_sync(invoke, mode, name, arguments)

    return server


async def serve(mode: str) -> None:
    server = create_server(mode)
    async with stdio_server() as (reader, writer):
        await server.run(reader, writer, server.create_initialization_options())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("semantica",))
    args = parser.parse_args()
    anyio.run(serve, args.mode)


if __name__ == "__main__":
    main()
