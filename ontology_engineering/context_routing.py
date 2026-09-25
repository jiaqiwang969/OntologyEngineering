"""Jev-assisted operational routing. Never a semantic verifier or tool executor.

The agent consumes candidate routes and checks their inputs and authority before
acting. Source files identify the available skill/workflow, not a second ontology.
"""
from __future__ import annotations

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re
import time

from ontology_engineering.jev_transport import (
    JevTransport, TransportError, pinned_model, resolve_credential_file,
    strict_json, validate_response,
)

ROOT = Path(__file__).resolve().parents[1]
INPUT_SCHEMA = "ontology-engineering.context-input/v1"
RESULT_SCHEMA = "ontology-engineering.context-routing/v1"
CHOICES = {
    "needed": "已有情景表明需要此项能力参与本轮工作，可与其他能力协同。",
    "not_needed": "已有情景足以表明本轮不需要此能力，或用户明确排除了该工作。",
    "not_established": "缺少判断所需的关键上下文，不能可靠决定是否需要此能力。",
}


def encoded(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value if isinstance(value, bytes) else encoded(value)).hexdigest()


def write_new(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n")


def _text(value, name):
    if not isinstance(value, str) or not value.strip():
        raise ValueError("invalid_" + name)
    return value


def prepare(document, *, root=ROOT):
    """Freeze current context, explicit external skills and maintained workflows."""
    if (not isinstance(document, dict) or document.get("schema") != INPUT_SCHEMA
            or set(document) - {"schema", "task_id", "context", "request", "external_skills"}):
        raise ValueError("invalid_context_input")
    _text(document.get("task_id"), "task_id")
    _text(document.get("request"), "request")
    context = document.get("context")
    if not isinstance(context, (str, dict, list)):
        raise ValueError("invalid_context")
    root = Path(root).resolve()
    config_raw = (root / "references/context-capabilities.json").read_bytes()
    config = strict_json(config_raw)
    if config.get("schema") != "ontology-engineering.context-capabilities/v1":
        raise ValueError("invalid_capability_configuration")
    pinned_model(config["model"])
    instruction_raw = (root / "references/context-routing-instructions.json").read_bytes()
    instruction = strict_json(instruction_raw)
    if (instruction.get("schema") != "ontology-engineering.context-routing-instructions/v1"
            or not isinstance(instruction.get("instructions"), dict) or not instruction["instructions"]):
        raise ValueError("invalid_routing_instructions")
    entries, sources = [], []
    for item in config["capabilities"]:
        entry = {"id": item["id"], "capability": item["capability"]}
        if item["source"] == "session skill inventory":
            source = {"id": item["id"], "path": None, "sha256": None,
                      "availability": "requires_session_discovery"}
        else:
            path = (root / item["source"]).resolve()
            if not path.is_relative_to(root):
                raise ValueError("capability_path_escapes_root")
            source = {"id": item["id"], "path": str(path),
                      "sha256": digest(path.read_bytes()) if path.is_file() else None,
                      "availability": "source_present" if path.is_file() else "source_missing"}
        entries.append(entry)
        sources.append(source)
    external = document.get("external_skills", [])
    if not isinstance(external, list) or len(external) > 12:
        raise ValueError("invalid_external_skills")
    for item in external:
        if not isinstance(item, dict) or set(item) != {"id", "skill_path", "capability"}:
            raise ValueError("invalid_external_skill")
        name = _text(item["id"], "external_id")
        if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,63}", name):
            raise ValueError("invalid_external_id")
        capability = _text(item["capability"], "external_capability")
        path = Path(item["skill_path"]).expanduser().resolve()
        if path.name != "SKILL.md" or not path.is_file():
            raise ValueError("external_skill_source_missing")
        entries.append({"id": "external_" + name, "capability": capability})
        sources.append({"id": "external_" + name, "path": str(path),
                        "sha256": digest(path.read_bytes()), "availability": "source_present"})
    ids = [e["id"] for e in entries]
    if not ids or len(ids) != len(set(ids)):
        raise ValueError("duplicate_or_empty_capabilities")
    questions = {}
    for entry in entries:
        instructions = deepcopy(instruction["instructions"])
        instructions["this_capability"] = entry
        questions[entry["id"]] = {
            "type": "choice",
            "instructions": instructions,
            "criteria": CHOICES,
        }
    payload = {"model": config["model"], "state": {"context": context, "request": document["request"]},
               "questions": questions}
    raw = encoded(payload)
    if len(raw) > 96000:
        raise ValueError("context_requires_explicit_reselection")
    if b"apikey_" in raw:
        raise ValueError("credential_material_in_context")
    code = {name: digest((root / name).read_bytes()) for name in (
        "ontology_engineering/__init__.py",
        "ontology_engineering/context_routing.py", "ontology_engineering/jev_transport.py",
        "scripts/route_engineering_task.py")}
    return {"task_id": document["task_id"], "payload": payload, "sources": sources,
            "identity": {"input_sha256": digest(document), "request_sha256": digest(payload),
                         "capability_configuration_sha256": digest(config_raw),
                         "instruction_version": instruction["version"],
                         "instruction_sha256": digest(instruction_raw), "code": code}}


def run(prepared, output, *, transport=None, credential_file=None, max_attempts=2, sleeper=time.sleep):
    """Call Jev, preserve partial answers and return routes for the agent to use.

No dispatch, project binding changes, ontology writes or fact admission occur.
Missing service credentials still produce an explicit unavailable route report.
"""
    if type(max_attempts) is not int or not 1 <= max_attempts <= 3:
        raise ValueError("invalid_attempt_limit")
    output = Path(output).expanduser().resolve()
    if output.is_relative_to(ROOT):
        raise ValueError("routing_output_must_be_outside_skill")
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    write_new(output / "input.json", prepared)
    valid, errors, attempts, transport_errors = {}, {}, [], []
    if transport is None:
        try:
            transport = JevTransport(resolve_credential_file(credential_file))
        except (ValueError, OSError):
            transport_errors.append("credential_unavailable")
    kind = getattr(transport, "kind", "unavailable")
    for index in range(max_attempts if transport is not None else 0):
        payload = dict(prepared["payload"])
        payload["questions"] = {q: value for q, value in payload["questions"].items() if q not in valid}
        if not payload["questions"]:
            break
        write_new(output / f"request-{index + 1}.json", payload)
        started = time.monotonic()
        record = {"request_sha256": digest(payload), "response": None,
                  "transport_error": None, "question_errors": {}, "execution_kind": kind}
        retry, delay = False, 0
        try:
            response = transport(payload)
            accepted, current_errors = validate_response(response, payload)
            record.update(response=response, question_errors=current_errors)
            valid.update(accepted)
            errors.update(current_errors)
            retry = bool(current_errors)
        except TransportError as error:
            record["transport_error"] = error.code
            transport_errors.append(error.code)
            retry = error.retryable
            delay = min(5, max(0, error.retry_after or 0.5 * (2 ** index)))
        except Exception:
            # Arbitrary transport exception text may contain credentials or data.
            record["transport_error"] = "unexpected_transport_error"
            transport_errors.append("unexpected_transport_error")
        record["elapsed_s"] = time.monotonic() - started
        write_new(output / f"response-{index + 1}.json", record)
        attempts.append(record)
        if not retry or index + 1 == max_attempts:
            break
        sleeper(delay)
    sources = {entry["id"]: entry for entry in prepared["sources"]}
    routes = []
    for q in prepared["payload"]["questions"]:
        answer = valid.get(q)
        routes.append({"capability_id": q, "source": sources[q], "answer": answer,
                       "need": answer["choice"] if answer else "not_established",
                       "model_status": "answered" if answer else "not_answered",
                       "error": errors.get(q) if not answer else None,
                       "execution": "not_started"})
    usage = {"input_tokens": 0, "output_tokens": 0, "unreported_attempts": 0}
    for attempt in attempts:
        reported = (attempt["response"] or {}).get("usage", {})
        if any(type(reported.get(k)) is not int for k in ("input_tokens", "output_tokens")):
            usage["unreported_attempts"] += 1
        for k in ("input_tokens", "output_tokens"):
            if type(reported.get(k)) is int:
                usage[k] += reported[k]
    result = {
        "schema": RESULT_SCHEMA, "task_id": prepared["task_id"], "identity": prepared["identity"],
        "model": prepared["payload"]["model"], "execution_kind": kind,
        "status": "completed" if len(valid) == len(routes) else "partial" if valid else "unavailable",
        "routes": routes, "candidate_routes": [r["capability_id"] for r in routes if r["need"] == "needed"],
        "uncertain_routes": [r["capability_id"] for r in routes if r["need"] == "not_established"],
        "attempts": len(attempts), "usage": usage, "transport_errors": transport_errors,
        "consumer": "agent_checks_context_sources_dependencies_and_authority_then_acts",
        "automatic_tool_dispatch": False, "semantic_review": "not_run", "fact_admission": False,
    }
    write_new(output / "routing.json", result)
    return result
