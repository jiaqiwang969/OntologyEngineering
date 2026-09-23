"""Source-bound fact projection for the Semantica engineering-method package.

This module checks transport, structure and source pointers only. It neither
evaluates evidence applicability nor implements semantic rules.
"""
from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
from urllib.parse import quote

from scripts.semantic_bundle_transport import load_bundle

SCHEMA = "ontology-engineering.method-evidence/v1"


def _json(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate JSON key: " + key)
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique, parse_constant=lambda value: (_ for _ in ()).throw(ValueError("non-finite JSON number")))


def _keys(value, expected, label):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError(label + " has missing or unknown fields")


def _text(value, label):
    if not isinstance(value, str) or not value.strip():
        raise ValueError(label + " must be nonempty text")
    return value


def _inside(root, relative):
    _text(relative, "source path")
    parts = relative.split("/")
    if PurePosixPath(relative).is_absolute() or "\\" in relative or any(x in {"", ".", ".."} for x in parts):
        raise ValueError("source must be a relative file inside evidence root")
    current = root
    for part in parts:
        current = current / part
        if current.is_symlink():
            raise ValueError("source path contains symlink")
    if not current.resolve().is_relative_to(root.resolve()) or not current.is_file():
        raise ValueError("source is missing or escapes evidence root")
    return current


def pointer_value(document, pointer):
    if not isinstance(pointer, str) or (pointer and not pointer.startswith("/")):
        raise ValueError("invalid JSON pointer")
    value = document
    for raw in pointer.split("/")[1:]:
        if re.search(r"~(?![01])", raw):
            raise ValueError("invalid JSON pointer escape")
        part = raw.replace("~1", "/").replace("~0", "~")
        if isinstance(value, list):
            if not re.fullmatch(r"0|[1-9][0-9]*", part) or int(part) >= len(value):
                raise ValueError("invalid array pointer")
            value = value[int(part)]
        elif isinstance(value, dict) and part in value:
            value = value[part]
        else:
            raise ValueError("source pointer does not resolve")
    return value


def profiles(root=None, *, bundle_name="engineering-evidence-methods"):
    args = {} if root is None else {"root": root}
    spec, payload, _ = load_bundle(bundle_name, **args)
    manifest = _json(payload[spec["manifest"]])
    asset = next(x for x in manifest["assets"] if x["asset_id"] == "input-profiles")
    return spec, _json(payload[asset["path"]])


def project_record(record_path, evidence_root, *, skill_root=None, bundle_name="engineering-evidence-methods"):
    record_path, evidence_root = Path(record_path), Path(evidence_root).resolve()
    raw = record_path.read_bytes()
    record = _json(raw)
    _keys(record, {"schema", "record_id", "claim", "method", "sources", "facts"}, "record")
    if record["schema"] != SCHEMA:
        raise ValueError("unsupported method evidence schema")
    _text(record["record_id"], "record_id")
    _keys(record["claim"], {"id", "statement", "subject_revision", "scope"}, "claim")
    for key, value in record["claim"].items():
        _text(value, "claim." + key)
    spec, registry = profiles(skill_root, bundle_name=bundle_name)
    if record["method"] not in registry["profiles"]:
        raise ValueError("method is not in the locked Semantica package")
    profile = registry["profiles"][record["method"]]
    if not isinstance(record["sources"], list) or not record["sources"]:
        raise ValueError("at least one source snapshot is required")
    sources, source_audit = {}, {}
    for source in record["sources"]:
        _keys(source, {"id", "path", "sha256", "media_type"}, "source")
        sid = _text(source["id"], "source.id")
        if sid in sources:
            raise ValueError("duplicate source ID")
        if source["media_type"] != "application/json":
            raise ValueError("v1 requires a controlled JSON fact snapshot")
        path = _inside(evidence_root, source["path"])
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest != source["sha256"]:
            raise ValueError("source hash mismatch: " + sid)
        sources[sid] = _json(data)
        source_audit[sid] = digest
    if not isinstance(record["facts"], dict) or set(record["facts"]) - set(profile["fields"]):
        raise ValueError("facts contain an unknown field")
    facts, lineage = {}, {}
    for field, ref in record["facts"].items():
        _keys(ref, {"source_id", "pointer"}, "fact reference")
        sid = ref["source_id"]
        if sid not in sources:
            raise ValueError("unknown fact source")
        value = pointer_value(sources[sid], ref["pointer"])
        kind = profile["fields"][field]
        matches = {"string": lambda: isinstance(value, str) and bool(value.strip()),
                   "boolean": lambda: type(value) is bool,
                   "number": lambda: type(value) in {int, float} and math.isfinite(value),
                   "strings": lambda: isinstance(value, list) and all(isinstance(x, str) and x.strip() for x in value) and len(set(value)) == len(value)}
        if not matches[kind]():
            raise ValueError("fact type mismatch: " + field)
        facts[field] = value
        lineage[field] = {**ref, "source_sha256": source_audit[sid]}
    ns = registry["namespace"]
    focus = ns + "claim/" + quote(record["claim"]["id"], safe="")
    def lit(value):
        return json.dumps(value, ensure_ascii=False, allow_nan=False)
    rows = [f"<{focus}> a <{registry['focus_type']}> .",
            f"<{focus}> <{ns}method> {lit(record['method'])} .",
            f"<{focus}> <{ns}subjectRevision> {lit(record['claim']['subject_revision'])} .",
            f"<{focus}> <{ns}scope> {lit(record['claim']['scope'])} .",
            f"<{focus}> <https://product-trustworthiness.local/claim#claimStatement> {lit(record['claim']['statement'])} ."]
    for field, value in sorted(facts.items()):
        values = value if isinstance(value, list) else [value]
        for item in values:
            rows.append(f"<{focus}> <{ns}{field}> {lit(item)} .")
        if isinstance(value, list):
            rows.append(f"<{focus}> <{ns}{field}Complete> true .")
        eid = ns + "source/" + source_audit[lineage[field]["source_id"]] + "/" + quote(field, safe="")
        rows.extend([f"<{focus}> <{ns}fieldEvidence> <{eid}> .",
                     f"<{eid}> a <{ns}MethodEvidence> ; <{ns}fieldName> {lit(field)} ; <{ns}sourceSha256> {lit(lineage[field]['source_sha256'])} ; <{ns}sourcePointer> {lit(lineage[field]['pointer'])} ."])
    rdf = ("\n".join(rows) + "\n").encode("utf-8")
    audit = {"schema": "ontology-engineering.method-projection/v1", "record_sha256": hashlib.sha256(raw).hexdigest(),
             "projection_sha256": hashlib.sha256(rdf).hexdigest(), "source_integrity": "verified", "field_lineage": lineage,
             "focus": focus, "focus_type": registry["focus_type"], "scope": record["claim"]["scope"],
             "method": record["method"], "query_asset": "findings-" + record["method"], "shape_asset": "shape-" + record["method"],
             "package": {key: spec[key] for key in ("package_id", "package_version", "package_sha256", "sha256")},
             "semantic_execution": "not_run", "engineering_verdict": "not_assessed",
             "meaning": "Source bytes and pointers match. Field meaning and truth remain evidence assertions for Semantica and engineering review."}
    return rdf, audit
