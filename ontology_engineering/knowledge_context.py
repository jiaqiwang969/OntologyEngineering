"""Deterministic project-record projections; no inference or project state store."""
from __future__ import annotations

from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat


SCHEMA = "ontology-engineering.project-context-projection/v1"
CONTEXT_SCHEMA = "ontology-engineering.supplier-knowledge-context/v1"
MAX_SOURCES = 16
MAX_MAPPINGS = 64
MAX_BINDING_BYTES = 24000
MAX_SOURCE_BYTES = 8 * 1024 * 1024
_SHA = re.compile(r"[0-9a-f]{64}\Z")
_ROOT_TARGETS = {
    "goal", "stage", "background", "decision_id", "functions", "statements",
    "unknowns", "changes", "alternatives", "prior_decisions", "record_refs",
    "decision_criteria", "tradeoffs",
}
_LEAF_TARGETS = {
    "project": {"id", "revision"},
    "object": {"id", "revision", "name"},
    "iteration": {"id", "focus", "stop_condition"},
}


def _error(code):
    return ValueError("knowledge_context_" + code)


def _encoded(value):
    try:
        return json.dumps(value, ensure_ascii=False, sort_keys=True,
                          separators=(",", ":"), allow_nan=False).encode("utf-8")
    except (TypeError, ValueError, UnicodeError):
        raise _error("non_json_value") from None


def _digest(value):
    return hashlib.sha256(_encoded(value)).hexdigest()


def _keys(value, required, code):
    if not isinstance(value, dict) or set(value) != set(required):
        raise _error(code)


def _path(value):
    if (not isinstance(value, str) or not value or len(value) > 600
            or "\\" in value or "\x00" in value):
        raise _error("source_path_invalid")
    path = PurePosixPath(value)
    if (path.is_absolute() or path.as_posix() != value or ".." in path.parts
            or path.parts[:2] != ("var", "projects") or len(path.parts) < 3):
        raise _error("source_path_invalid")
    return value


def _pointer(value):
    if (not isinstance(value, str) or len(value) > 1000 or "\x00" in value
            or (value and not value.startswith("/"))
            or re.search(r"~(?![01])", value)):
        raise _error("pointer_invalid")
    return [] if not value else [part.replace("~1", "/").replace("~0", "~")
                               for part in value[1:].split("/")]


def _target(value):
    parts = _pointer(value)
    if not ((len(parts) == 1 and parts[0] in _ROOT_TARGETS)
            or (len(parts) == 2 and parts[1] in _LEAF_TARGETS.get(parts[0], set()))):
        raise _error("target_invalid")
    return parts


def _get(document, pointer):
    value = document
    for part in _pointer(pointer):
        if isinstance(value, dict) and part in value:
            value = value[part]
        elif isinstance(value, list) and re.fullmatch(r"0|[1-9][0-9]*", part):
            index = int(part)
            if index >= len(value):
                raise _error("pointer_missing")
            value = value[index]
        else:
            raise _error("pointer_missing")
    return value


def _read(root, relative):
    """Open each component beneath the installation without following symlinks."""
    parts = PurePosixPath(_path(relative)).parts
    descriptor = None
    try:
        descriptor = os.open(Path(root).resolve(), os.O_RDONLY | os.O_DIRECTORY)
        for part in parts[:-1]:
            following = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                dir_fd=descriptor)
            os.close(descriptor)
            descriptor = following
        # NONBLOCK prevents an unexpected FIFO from hanging before fstat.
        leaf = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                       dir_fd=descriptor)
        with os.fdopen(leaf, "rb") as stream:
            info = os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode):
                raise _error("source_not_regular")
            if info.st_size > MAX_SOURCE_BYTES:
                raise _error("source_too_large")
            raw = stream.read(MAX_SOURCE_BYTES + 1)
        if len(raw) > MAX_SOURCE_BYTES:
            raise _error("source_too_large")
        return raw
    except FileNotFoundError:
        raise _error("source_missing") from None
    except OSError:
        raise _error("source_unreadable_or_symlink") from None
    finally:
        if descriptor is not None:
            os.close(descriptor)


def _json(raw):
    def pairs(items):
        value = {}
        for key, item in items:
            if key in value:
                raise _error("source_json_invalid")
            value[key] = item
        return value

    def constant(_):
        raise _error("source_json_invalid")

    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=pairs,
                          parse_constant=constant)
    except (ValueError, UnicodeError, RecursionError):
        raise _error("source_json_invalid") from None


def validate_bindings(bindings):
    """Validate transport syntax only; source and context checks are separate."""
    if not isinstance(bindings, list) or len(bindings) > MAX_SOURCES:
        raise _error("bindings_invalid")
    if len(_encoded(bindings)) > MAX_BINDING_BYTES:
        raise _error("bindings_too_large")
    paths, targets, count = set(), set(), 0
    for source in bindings:
        _keys(source, {"path", "sha256", "mappings"}, "binding_invalid")
        path = _path(source["path"])
        if path in paths:
            raise _error("binding_duplicate_source")
        paths.add(path)
        if not isinstance(source["sha256"], str) or not _SHA.fullmatch(source["sha256"]):
            raise _error("binding_hash_invalid")
        if not isinstance(source["mappings"], list) or not source["mappings"]:
            raise _error("binding_mappings_invalid")
        for mapping in source["mappings"]:
            count += 1
            if count > MAX_MAPPINGS:
                raise _error("binding_mapping_limit")
            _keys(mapping, {"pointer", "target", "value_sha256"}, "binding_mapping_invalid")
            _pointer(mapping["pointer"])
            _target(mapping["target"])
            if mapping["target"] in targets:
                raise _error("binding_duplicate_target")
            targets.add(mapping["target"])
            if (not isinstance(mapping["value_sha256"], str)
                    or not _SHA.fullmatch(mapping["value_sha256"])):
                raise _error("binding_hash_invalid")
    return deepcopy(bindings)


def project_context(spec, skill_root):
    """Copy explicitly selected source values into the existing context contract."""
    _keys(spec, {"schema", "base", "fields"}, "projection_spec_invalid")
    if spec["schema"] != SCHEMA:
        raise _error("projection_schema_invalid")
    _keys(spec["base"], {"schema", "task_id"}, "projection_base_invalid")
    if (spec["base"]["schema"] != CONTEXT_SCHEMA
            or not isinstance(spec["base"]["task_id"], str)
            or not spec["base"]["task_id"].strip()):
        raise _error("projection_base_invalid")
    fields = spec["fields"]
    if not isinstance(fields, list) or not 1 <= len(fields) <= MAX_MAPPINGS:
        raise _error("projection_fields_invalid")
    context, sources, targets = deepcopy(spec["base"]), {}, set()
    for field in fields:
        _keys(field, {"source", "pointer", "target"}, "projection_field_invalid")
        path = _path(field["source"])
        parts = _target(field["target"])
        if field["target"] in targets:
            raise _error("binding_duplicate_target")
        targets.add(field["target"])
        if path not in sources:
            if len(sources) >= MAX_SOURCES:
                raise _error("binding_source_limit")
            raw = _read(skill_root, path)
            sources[path] = (_json(raw), {"path": path,
                "sha256": hashlib.sha256(raw).hexdigest(), "mappings": []})
        document, binding = sources[path]
        value = _get(document, field["pointer"])
        destination = context if len(parts) == 1 else context.setdefault(parts[0], {})
        destination[parts[-1]] = deepcopy(value)
        binding["mappings"].append({"pointer": field["pointer"], "target": field["target"],
                                    "value_sha256": _digest(value)})
    # Reuse the established operational schema. Lazy import avoids a second
    # schema owner and permits supplier_knowledge to use validate_bindings.
    from .supplier_knowledge import validate_context
    context = validate_context(context)
    bindings = validate_bindings([entry[1] for entry in sources.values()])
    for source in bindings:
        for mapping in source["mappings"]:
            if _digest(_get(context, mapping["target"])) != mapping["value_sha256"]:
                raise _error("projection_normalization_changed_source_value")
    context["source_bindings"] = bindings
    return validate_context(context)


def verify_context_sources(context, skill_root):
    """Check current source bytes and exact mapped values, never engineering truth."""
    if not isinstance(context, dict):
        raise _error("invalid_context")
    bindings = validate_bindings(context.get("source_bindings", []))
    results = []
    targets = {mapping["target"] for source in bindings for mapping in source["mappings"]}
    unmapped = []
    if bindings:
        for key, value in context.items():
            if key in {"schema", "task_id", "source_bindings", "record_refs"}:
                continue
            values = [("/" + key + "/" + leaf, item) for leaf, item in value.items()] if (
                key in _LEAF_TARGETS and isinstance(value, dict)) else [("/" + key, value)]
            for target, item in values:
                empty_default = item == "" or isinstance(item, (list, dict)) and not item
                if target not in targets and not empty_default:
                    unmapped.append(target)
    for binding in bindings:
        reasons = []
        for mapping in binding["mappings"]:
            try:
                matches = _digest(_get(context, mapping["target"])) == mapping["value_sha256"]
            except ValueError:
                matches = False
            if not matches:
                reasons.append("context_value_changed")
        try:
            raw = _read(skill_root, binding["path"])
            if hashlib.sha256(raw).hexdigest() != binding["sha256"]:
                reasons.append("source_changed")
            else:
                document = _json(raw)
                for mapping in binding["mappings"]:
                    try:
                        matches = _digest(_get(document, mapping["pointer"])) == mapping["value_sha256"]
                    except ValueError:
                        matches = False
                    if not matches:
                        reasons.append("source_value_changed")
        except ValueError as exc:
            reasons.append(str(exc).removeprefix("knowledge_context_"))
        results.append({"path": binding["path"], "status": "stale" if reasons else "current",
                        "reasons": sorted(set(reasons))})
    return {"status": ("not_bound" if not bindings else
                       "stale" if unmapped or any(r["status"] == "stale" for r in results) else "current"),
            "sources": results,
            "reasons": ["unmapped_context_field"] if unmapped else [],
            "unmapped_context_fields": sorted(unmapped),
            "meaning": "Exact selected project-record bytes and mapped values only; no fact acceptance or project identity inference."}
