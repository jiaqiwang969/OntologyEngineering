#!/usr/bin/env python3
"""Tested calling layer for guarded one-shot Fusion MCP work.

Every driver written between 2026-09-05 and 2026-09-06 re-implemented the same
five things by hand: guard-transient retry, ``message`` -> last-line-JSON
parsing, document name/lineage identity checks, "the script produced no output"
detection, and ASCII escaping of non-ASCII literals.  Four independent copies
means four independent bugs.  This module is the single tested implementation.

It changes no safety rule:

* one call = one cell (every call spawns ``fusion_call.py`` afresh);
* a retry is only ever a **new** one-shot cell, and only for a rejection that
  provably happened before any ``tools/call`` reached Fusion;
* after an upstream timeout or any ambiguous result the policy refuses to
  retry, refuses to probe, and returns the ambiguity to the caller.

The module is import-safe and its whole classification/encoding core is pure:
``fusion_call.py`` imports :func:`classify_report` from here so the caller and
the drivers can never disagree about what a result means.

CLI::

    fusion_ops.py escape          <script.py>            # ASCII-safe rewrite
    fusion_ops.py render          <template.py> --subs subs.json
    fusion_ops.py classify        <acceptance-report.json> [--tool T]
    fusion_ops.py read            '<json arguments>'   [--receipt DIR/STEM]
    fusion_ops.py script          <script.py> [--role R] [--read-only]
    fusion_ops.py search-exact    <DOCUMENT NAME>
    fusion_ops.py assert-active   --name N --lineage L
    fusion_ops.py activate        --name N --lineage L
"""

from __future__ import annotations

import argparse
import ast
import io
import json
import os
import re
import subprocess
import sys
import time
import tokenize
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Iterable, Mapping, Sequence

__all__ = [
    "TAXONOMY",
    "AsciiScriptError",
    "CallOutcome",
    "FusionOps",
    "FusionOpsError",
    "Receipt",
    "RetryPolicy",
    "ScriptShapeError",
    "ascii_safe_script",
    "classify_report",
    "exact_matches",
    "longest_line",
    "python_literal",
    "render_script",
]

# --------------------------------------------------------------------------
# Constants
# --------------------------------------------------------------------------

DEFAULT_SKILL_DIR = Path(
    os.environ.get("CAD_AGENT_SKILL_DIR", str(Path(__file__).resolve().parents[1]))
)
DEFAULT_FUSION_CALL = DEFAULT_SKILL_DIR / "scripts" / "fusion_call.py"

#: Longest source line we are willing to put on the wire.  Fusion's built-in
#: ``mcp_execute_script`` has been observed to accept a 10 KB script whose
#: single longest line was ~8 KB, report ``success=true`` in 29 ms and execute
#: nothing at all (v018 A8_parameters, 2026-09-06 18:21).  The identical script
#: reformatted to a 253-character longest line ran normally.  We do not control
#: Fusion's parser, so we keep our own lines short and fail closed instead.
MAX_SCRIPT_LINE = 2000

#: The full stable result taxonomy printed by ``fusion_call.py``.
TAXONOMY = (
    "delivered_ok",
    "inner_business_error",
    "inner_script_error",
    "no_output",
    "response_truncated",
    "guard_transient",
    "guard_rejected",
    "upstream_serialization_error",
    "upstream_timeout",
    "transport_failure",
)

#: Exit code for each classification.  0 still means "delivered, read the
#: payload"; 4 means "the transport said yes but nothing usable came back".
EXIT_CODES = {
    "delivered_ok": 0,
    "inner_business_error": 0,
    "inner_script_error": 0,
    "guard_transient": 0,
    "guard_rejected": 0,
    "no_output": 4,
    "response_truncated": 4,
    "upstream_serialization_error": 4,
    "upstream_timeout": 2,
    "transport_failure": 2,
}

#: Guard rejections raised by ``FusionRuntimeGuard`` *before* any upstream
#: session is opened or any ``tools/call`` is forwarded.  Only these may be
#: retried, and only by launching a brand-new one-shot cell.
GUARD_TRANSIENT_PATTERNS = (
    "module-loading auxiliary is not uniquely stable",
    "module-loading auxiliary does not have its known size",
    "window census",
    "has no verified top-level windows",
    "application census is invalid",
    "Peekaboo permissions payload is invalid",
)

#: A guard rejection that arrives later than this is not treated as a
#: pre-upstream transient, because the upstream session may already exist.
GUARD_TRANSIENT_MAX_MS = 60_000

#: Fusion's own response serializer (nlohmann::json) failing to encode a result
#: it already computed.  Observed 2026-09-07 00:3x for
#: ``fusion_mcp_read {"queryType":"activeCommand"}``:
#: ``{"error":"[json.exception.type_error.316] invalid UTF-8 byte at index 158: 0x20"}``
#: with ``isError=true`` after 3.8 s.  The guard PASSED and the request reached
#: Fusion; only the reply could not be encoded.  Recorded in
#: references/runbooks/fusion-runtime-safety-incidents.md as
#: ``McpActiveCommandResponseSerializationError``: not a crash, not a guard
#: rejection, and never to be retried with the same query surface.
UPSTREAM_SERIALIZATION_PATTERNS = (
    "json.exception.type_error.316",
    "invalid UTF-8 byte at index",
    "[json.exception.",
)

# "timeout_seconds must be finite ..." is an argv validation error, not a
# timeout: an over-broad pattern here would wrongly declare the Fusion PID
# ambiguous and forbid all further work.
_TIMEOUT_PATTERN = re.compile(r"\btimed\s*out\b|\btimeout(?!_)\b", re.IGNORECASE)
_PLACEHOLDER = re.compile(r"\{\{([A-Za-z_][A-Za-z0-9_]*)\}\}")


class FusionOpsError(RuntimeError):
    """A calling-layer failure that the driver must not paper over."""


class AsciiScriptError(FusionOpsError):
    """The script cannot be made ASCII-safe without changing its meaning."""


class ScriptShapeError(FusionOpsError):
    """The script violates a transport shape rule (for example line length)."""


# --------------------------------------------------------------------------
# (a) Transport-safe script encoding
# --------------------------------------------------------------------------
#
# Byte-level trace of a script, measured 2026-09-06 with the installed
# packages (see ANALYSIS.md):
#
#   fusion_call.py  json.dumps(plan)            -> ensure_ascii=True, pure ASCII
#   acceptance_client -> cell  (mcp stdio)      -> model_dump_json(), RAW UTF-8
#   cell -> Fusion  (streamable HTTP, httpx)    -> encode_json(ensure_ascii=
#                                                  False), RAW UTF-8 body with
#                                                  Content-Type: application/
#                                                  json and NO charset
#
# Every stage we own is byte-lossless.  The first stage that is not ours is
# Fusion's decode of that raw UTF-8 body; a non-UTF-8 decode there turns
# "153-天花板小车" into "153-å¤©è\x8a±æ\x9d¿å°\x8fè½¦" and
# ``createSTEPImportOptions`` then raises "3 : The selected file does not
# exist."  We cannot patch Fusion, so we make the payload pure ASCII: a
# ``\uXXXX`` escape survives every decoding a JSON reader could plausibly use.


def _escape_text(text: str) -> str:
    """Escape every non-ASCII character as a Python ``\\uXXXX`` escape."""
    out: list[str] = []
    for char in text:
        code = ord(char)
        if code < 0x80:
            out.append(char)
        elif code <= 0xFFFF:
            out.append(f"\\u{code:04x}")
        else:
            out.append(f"\\U{code:08x}")
    return "".join(out)


def _ascii_literal(value: object) -> str:
    """One ASCII-only Python literal with exactly ``value``'s value."""
    literal = ascii(value)
    if not literal.isascii():  # pragma: no cover - ascii() guarantees this
        raise AsciiScriptError("ascii() produced a non-ASCII literal")
    return literal


def ascii_safe_script(script: str) -> str:
    """Return ``script`` with every non-ASCII character escaped.

    The rewrite is surgical: only STRING / FSTRING_MIDDLE / COMMENT tokens are
    touched, and the result is verified to have an identical AST before it is
    returned.  Anything that cannot be proven equivalent raises
    :class:`AsciiScriptError` rather than being silently mangled.

    It is idempotent, and a script that is already ASCII is returned unchanged.
    """
    if script.isascii():
        return script
    try:
        original_ast = ast.dump(ast.parse(script))
    except SyntaxError as exc:
        raise AsciiScriptError(f"script is not valid Python: {exc}") from exc

    lines = script.splitlines(keepends=True)
    edits: list[tuple[int, int, int, int, str]] = []
    fstring_raw_depth: list[bool] = []

    try:
        tokens = list(tokenize.generate_tokens(io.StringIO(script).readline))
    except tokenize.TokenError as exc:  # pragma: no cover - ast.parse catches first
        raise AsciiScriptError(f"script cannot be tokenized: {exc}") from exc

    fstring_start = getattr(tokenize, "FSTRING_START", None)
    fstring_middle = getattr(tokenize, "FSTRING_MIDDLE", None)
    fstring_end = getattr(tokenize, "FSTRING_END", None)

    for token in tokens:
        text = token.string
        if token.type == fstring_start and fstring_start is not None:
            fstring_raw_depth.append("r" in text.split('"')[0].split("'")[0].lower())
            continue
        if token.type == fstring_end and fstring_end is not None:
            if fstring_raw_depth:
                fstring_raw_depth.pop()
            continue
        if text.isascii():
            continue

        if token.type == tokenize.STRING:
            prefix = text[: len(text) - len(text.lstrip("bBfFrRuU"))].lower()
            if "f" in prefix:
                if "r" in prefix:
                    raise AsciiScriptError(
                        "raw f-string with non-ASCII text cannot be escaped safely"
                    )
                replacement = _escape_text(text)
            else:
                try:
                    replacement = _ascii_literal(ast.literal_eval(text))
                except (ValueError, SyntaxError) as exc:
                    raise AsciiScriptError(
                        f"cannot re-render string literal at line {token.start[0]}: {exc}"
                    ) from exc
        elif fstring_middle is not None and token.type == fstring_middle:
            if fstring_raw_depth and fstring_raw_depth[-1]:
                raise AsciiScriptError(
                    "raw f-string with non-ASCII text cannot be escaped safely"
                )
            replacement = _escape_text(text)
        elif token.type == tokenize.COMMENT:
            replacement = _escape_text(text)
        else:
            raise AsciiScriptError(
                "non-ASCII outside string literals and comments at line "
                f"{token.start[0]} ({tokenize.tok_name.get(token.type, token.type)}); "
                "rename the identifier or move the text into a string literal"
            )
        edits.append((token.start[0], token.start[1], token.end[0], token.end[1], replacement))

    for start_row, start_col, end_row, end_col, replacement in reversed(edits):
        if start_row == end_row:
            line = lines[start_row - 1]
            lines[start_row - 1] = line[:start_col] + replacement + line[end_col:]
        else:
            head = lines[start_row - 1][:start_col]
            tail = lines[end_row - 1][end_col:]
            lines[start_row - 1 : end_row] = [head + replacement + tail]

    escaped = "".join(lines)
    if not escaped.isascii():
        raise AsciiScriptError("escaping left non-ASCII characters in the script")
    try:
        if ast.dump(ast.parse(escaped)) != original_ast:
            raise AsciiScriptError("ASCII escaping changed the script's meaning")
    except SyntaxError as exc:
        raise AsciiScriptError(f"ASCII escaping produced invalid Python: {exc}") from exc
    return escaped


def longest_line(script: str) -> int:
    """Length of the longest source line, in characters."""
    return max((len(line) for line in script.splitlines()), default=0)


def check_script_shape(script: str, *, max_line: int = MAX_SCRIPT_LINE) -> None:
    """Fail closed on a script shape that Fusion has silently dropped before."""
    width = longest_line(script)
    if width > max_line:
        raise ScriptShapeError(
            f"longest source line is {width} characters (limit {max_line}). "
            "Fusion's mcp_execute_script has returned success=true with empty "
            "output for such a script without executing it. Re-render the "
            "literal across multiple lines (fusion_ops.render_script does "
            "this automatically)."
        )


def prepare_script(
    script: str,
    *,
    max_line: int = MAX_SCRIPT_LINE,
    escape: bool = True,
) -> str:
    """ASCII-escape and shape-check one script before it goes on the wire."""
    prepared = ascii_safe_script(script) if escape else script
    check_script_shape(prepared, max_line=max_line)
    return prepared


# --------------------------------------------------------------------------
# Multi-line ASCII Python literals for generated scripts
# --------------------------------------------------------------------------


def python_literal(value: object, *, indent: int = 0, width: int = 88) -> str:
    """Render ``value`` as an ASCII-only Python literal with bounded lines.

    Containers whose single-line form would exceed ``width`` are broken one
    element per line.  The result is always ASCII and always re-parses to an
    equal value.
    """
    pad = " " * indent
    if isinstance(value, (str, bytes, int, float, bool)) or value is None:
        return _ascii_literal(value)
    if isinstance(value, Mapping):
        if not value:
            return "{}"
        flat = "{" + ", ".join(
            f"{_ascii_literal(k)}: {python_literal(v)}" for k, v in value.items()
        ) + "}"
        if len(flat) + indent <= width:
            return flat
        inner = ",\n".join(
            f"{pad}    {_ascii_literal(k)}: {python_literal(v, indent=indent + 4, width=width)}"
            for k, v in value.items()
        )
        return "{\n" + inner + ",\n" + pad + "}"
    if isinstance(value, (list, tuple, set, frozenset)):
        items = list(value)
        open_c, close_c = ("[", "]") if isinstance(value, list) else ("(", ")")
        if isinstance(value, (set, frozenset)):
            open_c, close_c = "{", "}"
            items = sorted(items, key=repr)
        if not items:
            return "[]" if isinstance(value, list) else ("()" if isinstance(value, tuple) else "set()")
        flat = open_c + ", ".join(python_literal(item) for item in items)
        if isinstance(value, tuple) and len(items) == 1:
            flat += ","
        flat += close_c
        if len(flat) + indent <= width:
            return flat
        inner = ",\n".join(
            f"{pad}    {python_literal(item, indent=indent + 4, width=width)}"
            for item in items
        )
        return open_c + "\n" + inner + ",\n" + pad + close_c
    raise AsciiScriptError(f"cannot render {type(value).__name__} as a Python literal")


def render_script(template: str, /, **subs: object) -> str:
    """Substitute ``{{NAME}}`` placeholders and return a transport-safe script.

    Every substituted value becomes an ASCII-only Python literal; containers
    are broken across lines so no single line can grow past
    :data:`MAX_SCRIPT_LINE`.  The finished script is ASCII-escaped and
    shape-checked, so a driver can never re-introduce either 2026-09-06 defect
    by hand.
    """
    names = set(_PLACEHOLDER.findall(template))
    missing = sorted(names - set(subs))
    if missing:
        raise FusionOpsError(f"template placeholders without a value: {', '.join(missing)}")
    unused = sorted(set(subs) - names)
    if unused:
        raise FusionOpsError(f"values without a template placeholder: {', '.join(unused)}")

    def replace(match: re.Match[str]) -> str:
        line_start = template.rfind("\n", 0, match.start()) + 1
        prefix = template[line_start : match.start()]
        indent = len(prefix) if not prefix.strip() else 0
        return python_literal(subs[match.group(1)], indent=indent)

    rendered = _PLACEHOLDER.sub(replace, template)
    return prepare_script(rendered)


# --------------------------------------------------------------------------
# (b) Result taxonomy
# --------------------------------------------------------------------------


@dataclass
class CallOutcome:
    """One classified result, with every raw byte we were given preserved."""

    classification: str
    exit_code: int
    message: str
    payload: dict[str, Any] | None = None
    raw_text: str = ""
    inner: dict[str, Any] | None = None
    duration_ms: float | None = None
    delivery_ack_confirmed: bool | None = None
    cell_stderr_tail: str = ""
    stop_reason: str | None = None
    runtime_error: dict[str, Any] | None = None
    detail: dict[str, Any] = field(default_factory=dict)
    attempts: int = 1

    @property
    def ok(self) -> bool:
        return self.classification == "delivered_ok"

    @property
    def retryable(self) -> bool:
        return self.classification == "guard_transient"

    def to_json(self) -> dict[str, Any]:
        body: dict[str, Any] = {
            # ``status`` is kept for callers written against the old caller.
            "status": "delivered" if self.exit_code == 0 else "failed",
            "classification": self.classification,
            "message": self.message,
            "duration_ms": self.duration_ms,
            "delivery_ack_confirmed": self.delivery_ack_confirmed,
            "attempts": self.attempts,
            "payload": self.payload if self.payload is not None else {},
            "raw_text": self.raw_text,
            "inner": self.inner,
        }
        if self.stop_reason:
            body["stop_reason"] = self.stop_reason
        if self.runtime_error:
            body["runtime_error"] = self.runtime_error
        if self.cell_stderr_tail:
            body["cell_stderr_tail"] = self.cell_stderr_tail
        if self.detail:
            body["detail"] = self.detail
        return body


def _is_script_call(tool: str | None, arguments: Mapping[str, Any] | None) -> bool:
    return (
        tool == "fusion_mcp_execute"
        and isinstance(arguments, Mapping)
        and arguments.get("featureType") == "script"
    )


def _first_text(payload: Mapping[str, Any]) -> str:
    content = payload.get("content")
    if not isinstance(content, Sequence):
        return ""
    for block in content:
        if isinstance(block, Mapping) and isinstance(block.get("text"), str):
            return block["text"]
    return ""


def _looks_like_upstream_serialization(text: str) -> bool:
    """True for a Fusion-side reply-encoding failure (guard already passed)."""
    stripped = text.strip()
    if not stripped:
        return False
    return any(pattern in stripped for pattern in UPSTREAM_SERIALIZATION_PATTERNS)


def _looks_transient(text: str) -> bool:
    single_line = text.strip()
    if "\n" in single_line:
        return False
    return any(pattern in single_line for pattern in GUARD_TRANSIENT_PATTERNS)


def last_json_line(text: str) -> dict[str, Any] | None:
    """Parse the last non-empty line of ``text`` as a JSON object."""
    for line in reversed(text.strip().splitlines()):
        stripped = line.strip()
        if not stripped:
            continue
        if not stripped.startswith("{"):
            return None
        try:
            value = json.loads(stripped)
        except json.JSONDecodeError:
            return None
        return value if isinstance(value, dict) else None
    return None


def classify_report(
    report: Mapping[str, Any],
    *,
    tool: str | None = None,
    arguments: Mapping[str, Any] | None = None,
    guard_transient_max_ms: float = GUARD_TRANSIENT_MAX_MS,
) -> CallOutcome:
    """Classify one ``fusion-mcp.stdio-acceptance-report.v2`` into the taxonomy.

    Pure function: this is the single definition shared by ``fusion_call.py``
    and every driver, and it is what the taxonomy tests exercise.
    """
    calls = report.get("calls") or []
    call = calls[0] if isinstance(calls, list) and calls else {}
    stderr_tail = str((report.get("child_stderr") or {}).get("tail") or "")
    runtime_error = report.get("runtime_error") if isinstance(report.get("runtime_error"), dict) else None
    stop_reason = report.get("stop_reason")
    detail: dict[str, Any] = {
        "initialize_status": (report.get("initialize") or {}).get("status"),
        "tools_list_status": (report.get("tools_list") or {}).get("status"),
        "report_status": report.get("status"),
        "child_stderr_bytes": (report.get("child_stderr") or {}).get("bytes"),
    }
    transport = (call.get("layers") or {}).get("transport_protocol") or {}
    duration_ms = call.get("duration_ms")
    ack = (call.get("delivery_ack") or {}).get("confirmed")

    def build(classification: str, message: str, **extra: Any) -> CallOutcome:
        return CallOutcome(
            classification=classification,
            exit_code=EXIT_CODES[classification],
            message=message,
            duration_ms=duration_ms,
            delivery_ack_confirmed=ack,
            cell_stderr_tail=stderr_tail,
            stop_reason=stop_reason if isinstance(stop_reason, str) else None,
            runtime_error=runtime_error,
            detail=detail,
            **extra,
        )

    if transport.get("status") != "success":
        # Only a request that was actually issued can have timed out; a cell
        # that died before tools/call is a plain transport failure, and the
        # cell's own stderr is deliberately NOT scanned for the word "timeout".
        blob = json.dumps(
            {
                "stop_reason": stop_reason,
                "runtime_error": runtime_error,
                "transport": transport,
            },
            ensure_ascii=False,
        )
        timed_out = bool(calls) and bool(_TIMEOUT_PATTERN.search(blob))
        classification = "upstream_timeout" if timed_out else "transport_failure"
        reason = (
            (runtime_error or {}).get("message")
            or (transport.get("error") or {}).get("message")
            or (stop_reason if isinstance(stop_reason, str) else None)
            or "the cell never returned a response"
        )
        if not calls:
            reason = (
                f"the cell produced no call at all ({reason}). "
                "Cause is in cell_stderr_tail."
            )
        if timed_out:
            reason += (
                " AMBIGUOUS: the upstream request may have committed inside "
                "Fusion. Do not retry, do not probe this Fusion PID."
            )
        return build(classification, reason, raw_text=stderr_tail)

    response = call.get("response") or {}
    payload = response.get("payload")
    if payload is None:
        detail["response_bytes"] = response.get("bytes")
        detail["omission_reason"] = response.get("omission_reason")
        return build(
            "response_truncated",
            "the response was delivered but not captured: "
            f"{response.get('omission_reason') or 'unknown reason'} "
            f"({response.get('bytes')} bytes). Raise --max-response-bytes.",
            raw_text="",
        )

    text = _first_text(payload)
    if payload.get("isError"):
        if _looks_transient(text):
            if duration_ms is not None and duration_ms > guard_transient_max_ms:
                return build(
                    "guard_rejected",
                    "guard rejection matched a transient pattern but arrived after "
                    f"{duration_ms:.0f} ms; treated as non-retryable: {text.strip()}",
                    payload=dict(payload),
                    raw_text=text,
                )
            return build(
                "guard_transient",
                f"pre-upstream guard rejection (no Fusion request was sent): {text.strip()}",
                payload=dict(payload),
                raw_text=text,
            )
        if _looks_like_upstream_serialization(text):
            return build(
                "upstream_serialization_error",
                "Fusion could not serialize its own reply "
                "(McpActiveCommandResponseSerializationError): "
                f"{text.strip()[:300]}. The guard PASSED and the request DID "
                "reach Fusion, so this is neither a crash nor a guard "
                "rejection. Do NOT retry the same query; use a different "
                "query surface or external window evidence. If the call was a "
                "mutation, its effect may have committed - read back before "
                "assuming nothing happened.",
                payload=dict(payload),
                raw_text=text,
                inner=last_json_line(text),
            )
        return build(
            "guard_rejected",
            f"the call was rejected before it changed anything: {text.strip() or '(empty)'}",
            payload=dict(payload),
            raw_text=text,
        )

    outer = last_json_line(text) if text.strip().startswith("{") else None
    if outer is None:
        try:
            candidate = json.loads(text)
            outer = candidate if isinstance(candidate, dict) else None
        except (json.JSONDecodeError, TypeError):
            outer = None

    if outer is None:
        return build(
            "no_output",
            "the tool reported no error but returned no JSON body",
            payload=dict(payload),
            raw_text=text,
        )

    if _is_script_call(tool, arguments):
        stdout = outer.get("message")
        if outer.get("success") is False:
            return build(
                "inner_script_error",
                f"Fusion reported the script failed: {json.dumps(outer, ensure_ascii=False)[:400]}",
                payload=dict(payload),
                raw_text=text,
                inner=outer,
            )
        if not isinstance(stdout, str) or not stdout.strip():
            return build(
                "no_output",
                "the script produced NO output while Fusion reported success=true: "
                "it did not run to completion (Fusion has silently dropped scripts "
                "with very long source lines). Treat the document as UNCHANGED only "
                "after an independent read-back.",
                payload=dict(payload),
                raw_text=text,
                inner=outer,
            )
        inner = last_json_line(stdout)
        if inner is None:
            return build(
                "no_output",
                "the script printed output that is not a JSON object on its last "
                f"line: {stdout.strip()[:300]!r}",
                payload=dict(payload),
                raw_text=text,
                inner=None,
            )
        if inner.get("success") is True:
            return build(
                "delivered_ok",
                "delivered",
                payload=dict(payload),
                raw_text=text,
                inner=inner,
            )
        if "trace" in inner or "traceback" in inner:
            return build(
                "inner_script_error",
                f"the script raised: {str(inner.get('error'))[:300]}",
                payload=dict(payload),
                raw_text=text,
                inner=inner,
            )
        return build(
            "inner_business_error",
            f"the script ran and reported a business failure: "
            f"{json.dumps(inner, ensure_ascii=False)[:300]}",
            payload=dict(payload),
            raw_text=text,
            inner=inner,
        )

    if outer.get("success") is True:
        return build("delivered_ok", "delivered", payload=dict(payload), raw_text=text, inner=outer)
    if outer.get("success") is False:
        return build(
            "inner_business_error",
            f"the tool reported a business failure: "
            f"{json.dumps(outer, ensure_ascii=False)[:300]}",
            payload=dict(payload),
            raw_text=text,
            inner=outer,
        )
    return build(
        "delivered_ok",
        "delivered (the tool reported no explicit success flag)",
        payload=dict(payload),
        raw_text=text,
        inner=outer,
    )


# --------------------------------------------------------------------------
# (d) Retry policy, receipts and the driver-facing helpers
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class RetryPolicy:
    """Bounded retry for pre-upstream guard rejections only.

    ``attempts`` counts total launches, so the default is one call plus five
    retries with 45 s between them.  Nothing else is ever retried: an upstream
    timeout, an ambiguous delivery and any inner result are returned as-is,
    because the Fusion-side effect may already have committed.
    """

    attempts: int = 6
    delay_seconds: float = 45.0
    classes: tuple[str, ...] = ("guard_transient",)

    def __post_init__(self) -> None:
        if self.attempts < 1:
            raise ValueError("attempts must be >= 1")
        if self.delay_seconds < 0:
            raise ValueError("delay_seconds must be >= 0")
        forbidden = set(self.classes) - {"guard_transient"}
        if forbidden:
            raise ValueError(
                "only pre-upstream guard rejections may be retried; refused: "
                + ", ".join(sorted(forbidden))
            )

    def should_retry(self, outcome: CallOutcome, attempt: int) -> bool:
        """True when launching one more *fresh* cell is safe and allowed."""
        if attempt >= self.attempts:
            return False
        return outcome.classification in self.classes


@dataclass
class Receipt:
    """The four-file evidence set every driver was writing by hand."""

    directory: Path
    stem: str

    def path(self, suffix: str) -> Path:
        return self.directory / f"{self.stem}{suffix}"

    def write(
        self,
        *,
        tool: str,
        arguments: Mapping[str, Any],
        outcome: CallOutcome,
        script: str | None = None,
        stderr: str = "",
        elapsed_seconds: float | None = None,
    ) -> None:
        self.directory.mkdir(parents=True, exist_ok=True)
        self.path(".args.json").write_text(
            json.dumps(arguments, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
        )
        if script is not None:
            self.path(".py").write_text(script, encoding="utf-8")
        self.path(".result.json").write_text(
            json.dumps(outcome.to_json(), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        self.path(".stderr").write_text(stderr, encoding="utf-8")
        self.path(".exit").write_text(
            "EXIT:%d classification:%s attempts:%d elapsed:%s finished:%s\n"
            % (
                outcome.exit_code,
                outcome.classification,
                outcome.attempts,
                "-" if elapsed_seconds is None else f"{elapsed_seconds:.0f}s",
                time.strftime("%Y-%m-%d %H:%M:%S"),
            ),
            encoding="utf-8",
        )


CallerRunner = Callable[[Sequence[str], Mapping[str, str], float], "CallerResult"]


@dataclass
class CallerResult:
    """What one ``fusion_call.py`` subprocess returned."""

    returncode: int
    stdout: str
    stderr: str


def _default_runner(argv: Sequence[str], env: Mapping[str, str], timeout: float) -> CallerResult:
    proc = subprocess.run(
        list(argv),
        capture_output=True,
        text=True,
        env=dict(env),
        timeout=timeout,
    )
    return CallerResult(proc.returncode, proc.stdout, proc.stderr)


def outcome_from_caller(result: CallerResult) -> CallOutcome:
    """Turn one ``fusion_call.py`` invocation into a :class:`CallOutcome`.

    Understands the patched caller (which prints ``classification``) and the
    unpatched one (which prints ``status``/``payload`` only), so a driver keeps
    working while the caller patch is rolled out.
    """
    text = result.stdout.strip()
    body: Any = None
    if text:
        try:
            body = json.loads(text)
        except json.JSONDecodeError:
            body = None
    if not isinstance(body, dict):
        return CallOutcome(
            classification="transport_failure",
            exit_code=2,
            message=(
                f"fusion_call.py exited {result.returncode} without a JSON report; "
                "the cause is in the preserved stderr"
            ),
            raw_text=result.stdout,
            cell_stderr_tail=result.stderr[-4000:],
        )
    classification = body.get("classification")
    if classification in EXIT_CODES:
        return CallOutcome(
            classification=classification,
            exit_code=result.returncode,
            message=str(body.get("message") or ""),
            payload=body.get("payload") if isinstance(body.get("payload"), dict) else None,
            raw_text=str(body.get("raw_text") or ""),
            inner=body.get("inner") if isinstance(body.get("inner"), dict) else None,
            duration_ms=body.get("duration_ms"),
            delivery_ack_confirmed=body.get("delivery_ack_confirmed"),
            cell_stderr_tail=str(body.get("cell_stderr_tail") or result.stderr[-4000:]),
            stop_reason=body.get("stop_reason"),
            runtime_error=body.get("runtime_error"),
            detail=body.get("detail") or {},
            attempts=int(body.get("attempts") or 1),
        )
    # Legacy caller: rebuild the taxonomy from the delivered payload alone.
    payload = body.get("payload") if isinstance(body.get("payload"), dict) else {}
    synthetic = {
        "calls": [
            {
                "layers": {"transport_protocol": {"status": "success"}},
                "duration_ms": body.get("duration_ms"),
                "delivery_ack": {"confirmed": body.get("delivery_ack_confirmed")},
                "response": {"payload": payload},
            }
        ]
    }
    outcome = classify_report(synthetic, tool="fusion_mcp_execute", arguments={"featureType": "script"})
    outcome.cell_stderr_tail = result.stderr[-4000:]
    return outcome


class FusionOps:
    """Driver-facing helpers.  One call = one cell, always."""

    def __init__(
        self,
        *,
        fusion_call: Path | str = DEFAULT_FUSION_CALL,
        python: str = "python3",
        timeout: float = 300.0,
        env: Mapping[str, str] | None = None,
        retry: RetryPolicy | None = None,
        max_script_line: int = MAX_SCRIPT_LINE,
        receipts_dir: Path | None = None,
        runner: CallerRunner | None = None,
        sleep: Callable[[float], None] = time.sleep,
        log: Callable[[str], None] | None = None,
    ) -> None:
        self.fusion_call = Path(fusion_call)
        self.python = python
        self.timeout = timeout
        self.env = dict(env if env is not None else os.environ)
        self.env.setdefault("FUSION_CAD_TIMEOUT_SECONDS", "300")
        self.env.setdefault("FUSION_CAD_STARTUP_TIMEOUT_SECONDS", "300")
        self.retry = retry or RetryPolicy()
        self.max_script_line = max_script_line
        self.receipts_dir = receipts_dir
        self.runner = runner or _default_runner
        self.sleep = sleep
        self.log = log or (lambda message: print(message, flush=True))

    # -- core ----------------------------------------------------------------

    def call(
        self,
        tool: str,
        arguments: Mapping[str, Any],
        *,
        stem: str | None = None,
        retry: RetryPolicy | None = None,
        script: str | None = None,
        dirty_recovery_receipt: Path | None = None,
    ) -> CallOutcome:
        """One guarded call, with bounded pre-upstream-rejection retry."""
        policy = retry if retry is not None else self.retry
        argv = [
            self.python,
            "-B",
            str(self.fusion_call),
            tool,
            json.dumps(arguments, ensure_ascii=False),
            "--timeout",
            str(self.timeout),
        ]
        if dirty_recovery_receipt is not None:
            argv += ["--dirty-recovery-receipt", str(dirty_recovery_receipt)]

        attempt = 0
        started = time.time()
        while True:
            attempt += 1
            self.log(f"{stem or tool}: launching one-shot cell (attempt {attempt})")
            try:
                result = self.runner(argv, self.env, self.timeout + 90)
            except subprocess.TimeoutExpired as exc:
                outcome = CallOutcome(
                    classification="upstream_timeout",
                    exit_code=2,
                    message=(
                        f"fusion_call.py did not return within {exc.timeout:.0f} s. "
                        "AMBIGUOUS: do not retry and do not probe this Fusion PID."
                    ),
                    cell_stderr_tail="",
                    attempts=attempt,
                )
                break
            outcome = outcome_from_caller(result)
            outcome.attempts = attempt
            if policy.should_retry(outcome, attempt):
                self.log(
                    f"{stem or tool}: {outcome.classification} -> "
                    f"wait {policy.delay_seconds:.0f} s and launch a new cell"
                )
                self.sleep(policy.delay_seconds)
                continue
            break

        if stem is not None and self.receipts_dir is not None:
            Receipt(self.receipts_dir, stem).write(
                tool=tool,
                arguments=arguments,
                outcome=outcome,
                script=script,
                stderr=outcome.cell_stderr_tail,
                elapsed_seconds=time.time() - started,
            )
        return outcome

    def call_read(self, arguments: Mapping[str, Any], **kwargs: Any) -> CallOutcome:
        """One ``fusion_mcp_read`` call."""
        return self.call("fusion_mcp_read", arguments, **kwargs)

    def call_execute_script(
        self,
        path_or_text: Path | str,
        *,
        role: str | None = None,
        read_only: bool = False,
        escape: bool = True,
        **kwargs: Any,
    ) -> CallOutcome:
        """One ``fusion_mcp_execute`` script call, ASCII-safe and shape-checked.

        ``role`` declares ``FUSION_MCP_UNTITLED_PRESERVATION_ROLE`` when the
        script does not already declare it; the constant is written as a plain
        ASCII literal so the cell's static allowlist still sees it.
        """
        text = _read_script(path_or_text)
        if role is not None:
            text = _ensure_preservation_role(text, role)
        prepared = prepare_script(text, max_line=self.max_script_line, escape=escape)
        arguments: dict[str, Any] = {
            "featureType": "script",
            "object": {"script": prepared},
        }
        if read_only:
            arguments["object"]["readOnly"] = True
        return self.call("fusion_mcp_execute", arguments, script=prepared, **kwargs)

    # -- identity ------------------------------------------------------------

    def exact_document_search(
        self, name: str, *, stem: str | None = None
    ) -> list[dict[str, Any]]:
        """``document/search`` filtered to EXACT name matches.

        Fusion's search is fuzzy: on 2026-09-06 19:31 a search for
        ``FA04_V019_TROLLEY_6P84P_W850_REBUILD_V0_1`` returned exactly one
        result, and that result was the V018 document.  A driver that trusts
        the hit count creates its work in the wrong document.
        """
        outcome = self.call_read(
            {"queryType": "document", "operation": "search", "name": name},
            stem=stem,
        )
        if outcome.classification != "delivered_ok":
            raise FusionOpsError(
                f"document search failed ({outcome.classification}): {outcome.message}"
            )
        results = (outcome.inner or {}).get("results") or []
        return exact_matches(results, name)

    def assert_active_document(
        self, name: str, lineage: str, *, stem: str | None = None
    ) -> dict[str, Any]:
        """Prove the active document is exactly ``name`` @ ``lineage``."""
        outcome = self.call_execute_script(
            ACTIVE_IDENTITY_TEMPLATE_RENDERED(name, lineage),
            read_only=True,
            stem=stem,
        )
        if outcome.classification != "delivered_ok":
            raise FusionOpsError(
                f"active-document identity is not proven ({outcome.classification}): "
                f"{outcome.message}"
            )
        inner = outcome.inner or {}
        if inner.get("name") != name or inner.get("lineage") != lineage:
            raise FusionOpsError(
                f"active document is {inner.get('name')!r} @ {inner.get('lineage')!r}, "
                f"expected {name!r} @ {lineage!r}"
            )
        return inner

    def activate_document(
        self,
        name: str,
        lineage: str,
        *,
        settle_seconds: float = 8.0,
        stem: str | None = None,
    ) -> dict[str, Any]:
        """Activate an already-open document, settle outside MCP, then verify.

        The activation and the read-back are two separate one-shot cells with a
        settle window between them, which is the only sequence the runtime
        safety rules allow.
        """
        outcome = self.call_execute_script(
            ACTIVATE_TEMPLATE_RENDERED(name, lineage),
            stem=None if stem is None else f"{stem}_activate",
        )
        if outcome.classification != "delivered_ok":
            raise FusionOpsError(
                f"activation failed ({outcome.classification}): {outcome.message}"
            )
        self.sleep(settle_seconds)
        return self.assert_active_document(
            name, lineage, stem=None if stem is None else f"{stem}_verify"
        )


def exact_matches(results: Iterable[Mapping[str, Any]], name: str) -> list[dict[str, Any]]:
    """Keep only results whose ``name`` is byte-exactly ``name``."""
    return [dict(row) for row in results if isinstance(row, Mapping) and row.get("name") == name]


def _read_script(path_or_text: Path | str) -> str:
    if isinstance(path_or_text, Path):
        return path_or_text.read_text(encoding="utf-8")
    if "\n" not in path_or_text and path_or_text.endswith(".py"):
        candidate = Path(path_or_text)
        if candidate.exists():
            return candidate.read_text(encoding="utf-8")
    return path_or_text


def _ensure_preservation_role(script: str, role: str) -> str:
    if "FUSION_MCP_UNTITLED_PRESERVATION_ROLE" in script:
        return script
    return f'FUSION_MCP_UNTITLED_PRESERVATION_ROLE = {role!r}\n{script}'


# Identity/activation scripts.  They are rendered through ``render_script`` so
# a non-ASCII document name is escaped exactly like any other literal.

_ACTIVE_IDENTITY_TEMPLATE = '''\
import adsk.core
import json

EXPECTED_NAME = {{NAME}}
EXPECTED_LINEAGE = {{LINEAGE}}

result = {"success": False, "stage": "identity"}
try:
    app = adsk.core.Application.get()
    document = app.activeDocument
    data_file = document.dataFile if document is not None else None
    result.update(
        {
            "success": True,
            "name": str(document.name) if document is not None else None,
            "lineage": str(data_file.id) if data_file is not None else None,
            "version": int(data_file.versionNumber) if data_file is not None else None,
            "is_saved": bool(document.isSaved) if document is not None else None,
            "is_modified": bool(document.isModified) if document is not None else None,
            "matches": bool(
                document is not None
                and str(document.name) == EXPECTED_NAME
                and data_file is not None
                and str(data_file.id) == EXPECTED_LINEAGE
            ),
        }
    )
except Exception as exc:
    result.update({"success": False, "error": str(exc)})
print(json.dumps(result))
'''

_ACTIVATE_TEMPLATE = '''\
import adsk.core
import json

TARGET_NAME = {{NAME}}
TARGET_LINEAGE = {{LINEAGE}}

result = {"success": False, "stage": "activate"}
try:
    app = adsk.core.Application.get()
    documents = [app.documents.item(i) for i in range(app.documents.count)]
    matches = [
        d
        for d in documents
        if str(d.name) == TARGET_NAME
        and d.dataFile is not None
        and str(d.dataFile.id) == TARGET_LINEAGE
    ]
    if len(matches) != 1:
        raise RuntimeError(
            "expected exactly one open document with that name and lineage, found "
            + str(len(matches))
        )
    target = matches[0]
    if any(d.isModified for d in documents):
        raise RuntimeError("refusing to activate while an open document is modified")
    target.activate()
    result.update({"success": True, "activated": str(target.name)})
except Exception as exc:
    result.update({"success": False, "error": str(exc)})
print(json.dumps(result))
'''


def ACTIVE_IDENTITY_TEMPLATE_RENDERED(name: str, lineage: str) -> str:
    return render_script(_ACTIVE_IDENTITY_TEMPLATE, NAME=name, LINEAGE=lineage)


def ACTIVATE_TEMPLATE_RENDERED(name: str, lineage: str) -> str:
    return render_script(_ACTIVATE_TEMPLATE, NAME=name, LINEAGE=lineage)


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    escape = sub.add_parser("escape", help="print the ASCII-safe form of a script")
    escape.add_argument("path", type=Path)

    render = sub.add_parser("render", help="render a {{PLACEHOLDER}} template")
    render.add_argument("template", type=Path)
    render.add_argument("--subs", type=Path, required=True, help="JSON object of substitutions")
    render.add_argument("--out", type=Path)

    classify = sub.add_parser("classify", help="classify a saved acceptance report (offline)")
    classify.add_argument("report", type=Path)
    classify.add_argument("--tool")
    classify.add_argument("--arguments", help="JSON object, used to detect a script call")

    read = sub.add_parser("read", help="one fusion_mcp_read call")
    read.add_argument("arguments")

    script = sub.add_parser("script", help="one fusion_mcp_execute script call")
    script.add_argument("path", type=Path)
    script.add_argument("--role")
    script.add_argument("--read-only", action="store_true")

    search = sub.add_parser("search-exact", help="exact-name document search")
    search.add_argument("name")

    for name in ("assert-active", "activate"):
        node = sub.add_parser(name)
        node.add_argument("--name", required=True)
        node.add_argument("--lineage", required=True)

    for node in (read, script, search, sub.choices["assert-active"], sub.choices["activate"]):
        node.add_argument("--receipt-dir", type=Path)
        node.add_argument("--stem")
        node.add_argument("--timeout", type=float, default=300.0)
        node.add_argument("--retry-attempts", type=int, default=RetryPolicy.attempts)
        node.add_argument("--retry-delay", type=float, default=RetryPolicy.delay_seconds)
    return parser


def _ops(args: argparse.Namespace) -> FusionOps:
    return FusionOps(
        timeout=getattr(args, "timeout", 300.0),
        retry=RetryPolicy(
            attempts=getattr(args, "retry_attempts", 6),
            delay_seconds=getattr(args, "retry_delay", 45.0),
        ),
        receipts_dir=getattr(args, "receipt_dir", None),
    )


def main(argv: Sequence[str] | None = None) -> int:
    args = _build_parser().parse_args(argv)

    if args.command == "escape":
        sys.stdout.write(prepare_script(args.path.read_text(encoding="utf-8")))
        return 0
    if args.command == "render":
        subs = json.loads(args.subs.read_text(encoding="utf-8"))
        rendered = render_script(args.template.read_text(encoding="utf-8"), **subs)
        if args.out is not None:
            args.out.write_text(rendered, encoding="utf-8")
        else:
            sys.stdout.write(rendered)
        return 0
    if args.command == "classify":
        report = json.loads(args.report.read_text(encoding="utf-8"))
        arguments = json.loads(args.arguments) if args.arguments else None
        outcome = classify_report(report, tool=args.tool, arguments=arguments)
        json.dump(outcome.to_json(), sys.stdout, ensure_ascii=False, indent=2)
        sys.stdout.write("\n")
        return outcome.exit_code

    ops = _ops(args)
    if args.command == "read":
        outcome = ops.call_read(json.loads(args.arguments), stem=args.stem)
    elif args.command == "script":
        outcome = ops.call_execute_script(
            args.path, role=args.role, read_only=args.read_only, stem=args.stem
        )
    elif args.command == "search-exact":
        rows = ops.exact_document_search(args.name, stem=args.stem)
        json.dump({"exact_matches": rows, "count": len(rows)}, sys.stdout, ensure_ascii=False, indent=2)
        sys.stdout.write("\n")
        return 0 if len(rows) == 1 else 4
    elif args.command == "assert-active":
        json.dump(ops.assert_active_document(args.name, args.lineage, stem=args.stem),
                  sys.stdout, ensure_ascii=False, indent=2)
        sys.stdout.write("\n")
        return 0
    elif args.command == "activate":
        json.dump(ops.activate_document(args.name, args.lineage, stem=args.stem),
                  sys.stdout, ensure_ascii=False, indent=2)
        sys.stdout.write("\n")
        return 0
    else:  # pragma: no cover - argparse enforces the choices
        raise SystemExit(3)

    json.dump(outcome.to_json(), sys.stdout, ensure_ascii=False, indent=2)
    sys.stdout.write("\n")
    return outcome.exit_code


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except FusionOpsError as exc:
        sys.stderr.write(f"{type(exc).__name__}: {exc}\n")
        raise SystemExit(4) from None
