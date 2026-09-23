#!/usr/bin/env python3
"""Check source-bound CAD facts and project them without an engineering verdict.

Object kinds and relation predicates are project vocabulary, not a local TBox.
Formal semantic execution belongs to the source-locked Semantica engagement.
"""

from __future__ import annotations

import argparse
from collections import defaultdict, deque
import hashlib
import json
import os
from pathlib import Path
from urllib.parse import quote

from jsonschema import Draft202012Validator
from rdf_lines import Iri, Namespace, RdfTerms, Text, TripleLines


SCHEMA = Path(__file__).resolve().parents[1] / "contracts" / "cad-evidence.v1.schema.json"
ANALYSIS_SCHEMA = Path(__file__).resolve().parents[1] / "contracts" / "analysis-record.v1.schema.json"
CE = Namespace("urn:ontology-engineering:cad-evidence:v1:")


class CadEvidenceError(ValueError):
    pass


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _source_path(root: Path, relative: str) -> Path:
    path = Path(relative)
    if path.is_absolute() or path.as_posix() != relative or any(part in {".", ".."} for part in path.parts):
        raise CadEvidenceError(f"source path must be normalized and relative: {relative}")
    candidate = root / path
    resolved = candidate.resolve()
    if not resolved.is_relative_to(root) or not resolved.is_file() or candidate.is_symlink():
        raise CadEvidenceError(f"source is missing or leaves project root: {relative}")
    return resolved


def _pointer(document: object, pointer: str) -> object:
    if not pointer.startswith("/"):
        raise CadEvidenceError(f"invalid JSON pointer: {pointer}")
    current = document
    for escaped in pointer[1:].split("/"):
        token = escaped.replace("~1", "/").replace("~0", "~")
        try:
            current = current[int(token)] if isinstance(current, list) else current[token]  # type: ignore[index]
        except (KeyError, IndexError, TypeError, ValueError) as error:
            raise CadEvidenceError(f"JSON pointer does not resolve: {pointer}") from error
    return current


def _inventory(packet: dict) -> dict[str, tuple[str, dict]]:
    found: dict[str, tuple[str, dict]] = {}
    for kind, key in (("source", "sources"), ("object", "objects"), ("relation", "relations"),
                      ("requirement", "requirements"), ("assertion", "assertions"), ("question", "questions")):
        for entry in packet[key]:
            if entry["id"] in found:
                raise CadEvidenceError(f"duplicate ID: {entry['id']}")
            found[entry["id"]] = (kind, entry)
    return found


def _check_refs(packet: dict, found: dict[str, tuple[str, dict]]) -> None:
    def require(ids: list[str], kinds: set[str], owner: str) -> None:
        for identity in ids:
            if identity not in found or found[identity][0] not in kinds:
                raise CadEvidenceError(f"{owner} refers to absent/wrong-kind ID: {identity}")

    for source in packet["sources"]:
        require(list(source.get("embedded_source_hashes", {}).values()), {"source"}, source["id"])
    for key in ("objects", "relations"):
        for entry in packet[key]:
            require([entry["source_id"]], {"source"}, entry["id"])
            if entry["basis"] in {"visual_review", "patent_review", "analytic_derivation"} and key == "objects" and "status" not in entry:
                raise CadEvidenceError(f"{entry['id']} reconstructed object requires observed/candidate status")
            if entry["basis"] in {"engineering_inference", "analytic_derivation"} and entry.get("status") == "observed":
                raise CadEvidenceError(f"{entry['id']} inferred fact cannot be observed")
            if key == "relations":
                require([entry["subject_id"], entry["object_id"]], {"object"}, entry["id"])
    for entry in packet["requirements"]:
        require(entry["source_ids"], {"source"}, entry["id"])
        require(entry["target_ids"], {"object"}, entry["id"])
    for entry in packet["assertions"]:
        require(entry["source_ids"] + entry["challenge_source_ids"], {"source"}, entry["id"])
        require(entry["target_ids"], {"object"}, entry["id"])
        require(entry["depends_on_ids"], {"object", "relation", "requirement", "assertion"}, entry["id"])
    for entry in packet["questions"]:
        require(entry["source_ids"], {"source"}, entry["id"])
        require(entry["target_ids"], {"object", "relation", "requirement", "assertion"}, entry["id"])

    # An assertion cannot be its own proof, directly or through another assertion.
    visiting: set[str] = set()
    visited: set[str] = set()

    def walk(identity: str) -> None:
        if identity in visiting:
            raise CadEvidenceError(f"circular assertion dependency: {identity}")
        if identity in visited:
            return
        visiting.add(identity)
        for dependency in found[identity][1]["depends_on_ids"]:
            if found[dependency][0] == "assertion":
                walk(dependency)
        visiting.remove(identity)
        visited.add(identity)

    for assertion in packet["assertions"]:
        walk(assertion["id"])


def verify(packet_path: Path, project_root: Path) -> tuple[dict, dict]:
    packet_path = packet_path.resolve()
    root = project_root.resolve()
    packet = json.loads(packet_path.read_text(encoding="utf-8"))
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    errors = sorted(Draft202012Validator(schema).iter_errors(packet), key=lambda error: list(map(str, error.path)))
    if errors:
        first = errors[0]
        raise CadEvidenceError(f"schema: /{'/'.join(map(str, first.path))}: {first.message}")
    found = _inventory(packet)
    _check_refs(packet, found)
    paths: dict[str, Path] = {}
    sources = {entry["id"]: entry for entry in packet["sources"]}
    for source in packet["sources"]:
        path = _source_path(root, source["path"])
        if _sha256(path) != source["sha256"]:
            raise CadEvidenceError(f"source SHA-256 mismatch: {source['id']}")
        paths[source["id"]] = path
    cache: dict[str, object] = {}

    def source_json(source_id: str) -> object:
        if sources[source_id]["media_type"] != "application/json":
            raise CadEvidenceError(f"JSON pointer requires JSON source: {source_id}")
        if source_id not in cache:
            cache[source_id] = json.loads(paths[source_id].read_text(encoding="utf-8"))
        return cache[source_id]

    for source in packet["sources"]:
        for pointer, target in source.get("embedded_source_hashes", {}).items():
            if _pointer(source_json(source["id"]), pointer) != sources[target]["sha256"]:
                raise CadEvidenceError(f"embedded source hash mismatch: {source['id']} {pointer}")
        if source["role"] == "analysis_record":
            document = source_json(source["id"])
            analysis_schema = json.loads(ANALYSIS_SCHEMA.read_text(encoding="utf-8"))
            problems = sorted(Draft202012Validator(analysis_schema).iter_errors(document),
                              key=lambda error: list(map(str, error.path)))
            if problems:
                first = problems[0]
                raise CadEvidenceError(f"analysis record schema: {source['id']} /{'/'.join(map(str, first.path))}: {first.message}")
            for referenced_id, digest in document["source_hashes"].items():
                if referenced_id == source["id"] or referenced_id not in sources or digest != sources[referenced_id]["sha256"]:
                    raise CadEvidenceError(f"analysis source hash mismatch: {source['id']} {referenced_id}")
            for item in document["inputs"]:
                if item["source_id"] not in document["source_hashes"]:
                    raise CadEvidenceError(f"analysis input source not bound: {source['id']} {item['name']}")
                if item["provenance"] == "unknown" and item["value"] is not None:
                    raise CadEvidenceError(f"unknown analysis input has a value: {source['id']} {item['name']}")
                if item["provenance"] == "measured" and sources[item["source_id"]]["role"] != "measurement":
                    raise CadEvidenceError(f"measured analysis input lacks measurement source: {source['id']} {item['name']}")
            implementation = document["calculation"].get("implementation_source_id")
            if document["calculation"]["method"] == "numerical" and not implementation:
                raise CadEvidenceError(f"numerical analysis implementation not bound: {source['id']}")
            if implementation and implementation not in document["source_hashes"]:
                raise CadEvidenceError(f"analysis implementation source not bound: {source['id']} {implementation}")
    expected_roles = {"native_readback": "native_readback", "drawing_review": "drawing",
                      "bom_review": "bom", "simulation_result": "simulation_result", "measurement": "measurement",
                      "visual_review": "reference_media", "patent_review": "patent_document",
                      "analytic_derivation": "analysis_record"}
    for entry in packet["objects"] + packet["relations"]:
        basis = entry["basis"]
        role = sources[entry["source_id"]]["role"]
        if basis in expected_roles and role != expected_roles[basis]:
            raise CadEvidenceError(f"{entry['id']} claims {basis} from {role}")
        if basis == "visual_review" and not sources[entry["source_id"]]["media_type"].startswith(("image/", "video/")):
            raise CadEvidenceError(f"{entry['id']} visual review requires original image/video media")
        if basis == "native_readback":
            observed = _pointer(source_json(entry["source_id"]), entry["source_locator"])
            if "expected" in entry:
                if not isinstance(observed, dict) or any(observed.get(key) != value for key, value in entry["expected"].items()):
                    raise CadEvidenceError(f"native readback value mismatch: {entry['id']}")
        if basis == "analytic_derivation":
            if not entry.get("expected"):
                raise CadEvidenceError(f"{entry['id']} analytic derivation requires expected result binding")
            observed = _pointer(source_json(entry["source_id"]), entry["source_locator"])
            if not isinstance(observed, dict) or any(observed.get(key) != value for key, value in entry["expected"].items()):
                raise CadEvidenceError(f"analytic derivation result mismatch: {entry['id']}")
    audit = {
        "record_type": "ontology-engineering.cad-evidence-integrity/v1",
        "packet_id": packet["packet_id"],
        "packet_sha256": _sha256(packet_path),
        "schema_sha256": _sha256(SCHEMA),
        "analysis_schema_sha256": _sha256(ANALYSIS_SCHEMA),
        "adapter_sha256": _sha256(Path(__file__)),
        "source_integrity": "verified",
        "source_results": [{"id": entry["id"], "path": entry["path"], "sha256": entry["sha256"]} for entry in packet["sources"]],
        "entity_count": len(found),
        "semantic_execution": "not_run",
        "engineering_verdict": "not_assessed",
    }
    return packet, audit


def project_abox(packet: dict, packet_sha256: str | None = None) -> bytes:
    project = packet["project"]
    identity = ":".join(quote(project[key], safe="") for key in
                        ("project_id", "configuration_id", "model_revision", "product_state"))
    base = f"urn:ontology-engineering:project:{identity}:"

    def uri(kind: str, item_id: str) -> Iri:
        return Iri(base + kind + ":" + quote(item_id, safe=""))

    graph = TripleLines()
    identity_kind = {}
    for collection_name, kind in (("objects", "cad_object"), ("relations", "cad_relation"),
                                  ("requirements", "cad_requirement"), ("assertions", "cad_assertion")):
        identity_kind.update((entry["id"], kind) for entry in packet[collection_name])
    snapshot = uri("cad_snapshot", packet["packet_id"])
    graph.add((snapshot, RdfTerms.type, CE.CadEvidenceSnapshot))
    if packet_sha256:
        graph.add((snapshot, CE.packet_sha256, Text(packet_sha256)))
    for key, rdf_class, collection, predicate in (
        ("sources", CE.EvidenceSource, "source", CE.hasSource),
        ("objects", CE.CadObject, "cad_object", CE.hasObject),
        ("relations", CE.CadRelation, "cad_relation", CE.hasRelation),
        ("requirements", CE.DesignRequirement, "cad_requirement", CE.hasRequirement),
        ("assertions", CE.CadAssertion, "cad_assertion", CE.hasAssertion),
        ("questions", CE.EngineeringQuestion, "cad_question", CE.hasQuestion),
    ):
        for entry in packet[key]:
            subject = uri(collection, entry["id"])
            graph.add((snapshot, predicate, subject))
            graph.add((subject, RdfTerms.type, rdf_class))
            for field, value in entry.items():
                if field in {"expected", "embedded_source_hashes"}:
                    graph.add((subject, CE[field + "Json"], Text(json.dumps(value, ensure_ascii=False, sort_keys=True))))
                elif isinstance(value, str):
                    graph.add((subject, CE[field], Text(value)))
            if collection in {"cad_object", "cad_relation"}:
                graph.add((subject, CE.wasDerivedFrom, uri("source", entry["source_id"])))
            if collection == "cad_relation":
                graph.add((subject, CE.subject, uri("cad_object", entry["subject_id"])))
                graph.add((subject, CE.object, uri("cad_object", entry["object_id"])))
            for field, target_kind in (("target_ids", None), ("source_ids", "source"),
                                       ("challenge_source_ids", "source")):
                for identity in entry.get(field, []):
                    graph.add((subject, CE[field], uri(target_kind or identity_kind[identity], identity)))
            for dependency in entry.get("depends_on_ids", []):
                graph.add((subject, CE.dependsOn, uri(identity_kind[dependency], dependency)))
            for requested in entry.get("requested_evidence", []):
                graph.add((subject, CE.requestsEvidence, Text(requested)))
    lines = sorted(f"{s.n3()} {p.n3()} {o.n3()} .\n" for s, p, o in graph)
    return "".join(lines).encode("utf-8")


def compare_packets(previous: dict, current: dict) -> dict:
    def entities(packet: dict) -> dict[str, tuple[str, dict, set[str]]]:
        result: dict[str, tuple[str, dict, set[str]]] = {}
        for key, kind in (("sources", "source"), ("objects", "object"), ("relations", "relation"),
                          ("requirements", "requirement"), ("assertions", "assertion"), ("questions", "question")):
            for entry in packet[key]:
                deps = set(entry.get("source_ids", []) + entry.get("challenge_source_ids", []) +
                           entry.get("target_ids", []) + entry.get("depends_on_ids", []))
                if "source_id" in entry:
                    deps.add(entry["source_id"])
                if kind == "source":
                    deps.update(entry.get("embedded_source_hashes", {}).values())
                if kind == "relation":
                    deps.update([entry["subject_id"], entry["object_id"]])
                result[entry["id"]] = kind, entry, deps
        return result

    old, new = entities(previous), entities(current)
    changed = {identity for identity in old.keys() | new.keys() if old.get(identity) != new.get(identity)}
    identity_scope_changed = previous["project"] != current["project"]
    if identity_scope_changed:
        changed.update(new)
    reverse: dict[str, set[str]] = defaultdict(set)
    for inventory in (old, new):
        for identity, (_, _, dependencies) in inventory.items():
            for dependency in dependencies:
                reverse[dependency].add(identity)
    affected = set(changed)
    queue = deque(changed)
    while queue:
        for dependent in reverse[queue.popleft()] - affected:
            affected.add(dependent)
            queue.append(dependent)
    return {
        "record_type": "ontology-engineering.cad-evidence-impact/v1",
        "identity_scope_changed": identity_scope_changed,
        "changed_ids": sorted(changed),
        "affected_ids": sorted(affected),
        "affected_assertion_ids": sorted(identity for identity in affected if new.get(identity, old.get(identity))[0] == "assertion"),
        "affected_question_ids": sorted(identity for identity in affected if new.get(identity, old.get(identity))[0] == "question"),
        "semantic_execution": "not_run",
        "engineering_verdict": "re_review_required_not_assessed",
    }


def _write_once(path: Path, content: bytes) -> None:
    if path.exists():
        if path.read_bytes() != content:
            raise CadEvidenceError(f"immutable output collision: {path}")
        return
    temporary = path.with_name(path.name + f".tmp.{os.getpid()}")
    try:
        temporary.write_bytes(content)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packet", type=Path, required=True)
    parser.add_argument("--previous-packet", type=Path)
    parser.add_argument("--project-root", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        packet, audit = verify(args.packet, args.project_root)
        previous = verify(args.previous_packet, args.project_root)[0] if args.previous_packet else None
        abox = project_abox(packet, audit["packet_sha256"])
        digest = f"{audit['packet_sha256'][:12]}-{audit['schema_sha256'][:8]}-{audit['analysis_schema_sha256'][:8]}-{audit['adapter_sha256'][:8]}"
        output = args.output_dir.resolve()
        output.mkdir(parents=True, exist_ok=True)
        abox_path = output / f"cad-evidence-{digest}.nt"
        audit_path = output / f"cad-evidence-{digest}.integrity.json"
        audit["abox_sha256"] = hashlib.sha256(abox).hexdigest()
        audit["abox_path"] = str(abox_path)
        if previous is not None:
            impact_path = output / f"cad-impact-{digest}.json"
            _write_once(impact_path, (json.dumps(compare_packets(previous, packet), ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode())
            audit["impact_path"] = str(impact_path)
        _write_once(abox_path, abox)
        _write_once(audit_path, (json.dumps(audit, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(json.dumps({"status": "blocked", "error": str(error)}, ensure_ascii=False))
        return 2
    print(json.dumps({"status": "projected", "packet_sha256": audit["packet_sha256"], "abox_path": str(abox_path), "audit_path": str(audit_path), "semantic_execution": "not_run"}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
