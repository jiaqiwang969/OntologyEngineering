#!/usr/bin/env python3
"""Verify a CAD/process evidence handoff and project it to a bounded RDF ABox.

This is an evidence adapter. It does not run CQ, SHACL, rules, decide whether a
joint seals, or release a manufacturing route. Those decisions remain with the
source-locked Semantica engagement and the relevant engineering authorities.
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


ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "contracts" / "cad-process-handoff.v1.schema.json"
CP = Namespace("urn:ontology-engineering:cad-process:v1:")


class HandoffError(ValueError):
    pass


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _check_source_path(project_root: Path, relative: str) -> Path:
    candidate = Path(relative)
    if candidate.is_absolute() or ".." in candidate.parts or "." in candidate.parts:
        raise HandoffError(f"source path must be normalized and relative: {relative}")
    resolved = (project_root / candidate).resolve()
    if not resolved.is_relative_to(project_root) or not resolved.is_file():
        raise HandoffError(f"source is missing or leaves project root: {relative}")
    return resolved


def _json_pointer(document: object, pointer: str) -> object:
    if not pointer.startswith("/"):
        raise HandoffError(f"invalid JSON pointer: {pointer}")
    current = document
    for escaped in pointer[1:].split("/"):
        token = escaped.replace("~1", "/").replace("~0", "~")
        try:
            current = current[int(token)] if isinstance(current, list) else current[token]  # type: ignore[index]
        except (KeyError, IndexError, TypeError, ValueError) as exc:
            raise HandoffError(f"JSON pointer does not resolve: {pointer}") from exc
    return current


def _check_refs(packet: dict) -> dict[str, dict]:
    collections = {
        "source": packet["sources"],
        "feature": packet["features"],
        "function": packet["functions"],
        "route": packet["routes"],
        "coverage": packet["coverage_assertions"],
        "claim": packet["claims"],
        "question": packet["questions"],
        "assumption": packet["assumptions"],
        "downstream_output": packet["downstream_outputs"],
    }
    ids: dict[str, dict] = {}
    for kind, entries in collections.items():
        for entry in entries:
            identity = entry["id"]
            if identity in ids:
                raise HandoffError(f"duplicate entity ID: {identity}")
            ids[identity] = {"kind": kind, "entry": entry}

    def expect(values: list[str], allowed: set[str], owner: str) -> None:
        for value in values:
            found = ids.get(value)
            if found is None or found["kind"] not in allowed:
                raise HandoffError(f"{owner} refers to absent/wrong-kind ID: {value}")

    for feature in packet["features"]:
        expect([feature["source_id"]], {"source"}, feature["id"])
        source_role = ids[feature["source_id"]]["entry"]["role"]
        if feature["basis"] == "native_geometry_readback" and source_role != "native_readback":
            raise HandoffError(f"{feature['id']} claims native readback from {source_role}")
        if feature["basis"] == "drawing_review" and source_role != "drawing":
            raise HandoffError(f"{feature['id']} claims drawing review from {source_role}")
    for source in packet["sources"]:
        for target_id in source.get("embedded_source_hashes", {}).values():
            expect([target_id], {"source"}, source["id"])
    for function in packet["functions"]:
        expect(function["source_ids"], {"source"}, function["id"])
        expect(function["feature_ids"], {"feature"}, function["id"])
    operation_routes = {}
    for route in packet["routes"]:
        expect(route["source_ids"], {"source"}, route["id"])
        for operation in route["operations"]:
            if operation["id"] in ids:
                raise HandoffError(f"duplicate entity ID: {operation['id']}")
            ids[operation["id"]] = {"kind": "operation", "entry": operation}
            operation_routes[operation["id"]] = route["id"]
            expect(operation["target_feature_ids"], {"feature"}, operation["id"])
            expect(operation["intended_function_ids"], {"function"}, operation["id"])
            expect(operation["source_ids"], {"source"}, operation["id"])
            expect(operation["validation_source_ids"], {"source"}, operation["id"])
    for claim in packet["claims"]:
        expect(claim["source_ids"], {"source"}, claim["id"])
        expect(claim["depends_on_function_ids"], {"function"}, claim["id"])
        expect(claim["depends_on_route_ids"], {"route"}, claim["id"])
        for route_key in ("baseline_route_id", "target_route_id"):
            if route_key in claim:
                expect([claim[route_key]], {"route"}, claim["id"])
        completeness = claim.get("function_list_completeness_source_ids", [])
        expect(completeness, {"source"}, claim["id"])
        if bool(completeness) != bool(claim.get("function_list_completeness_scope")):
            raise HandoffError(f"{claim['id']} completeness evidence and scope must occur together")
    for coverage in packet["coverage_assertions"]:
        expect([coverage["claim_id"]], {"claim"}, coverage["id"])
        expect([coverage["function_id"]], {"function"}, coverage["id"])
        expect([coverage["operation_id"]], {"operation"}, coverage["id"])
        expect(coverage["source_ids"], {"source"}, coverage["id"])
        expect(coverage["validation_source_ids"], {"source"}, coverage["id"])
        expect(coverage["scope_checked_source_ids"], {"source"}, coverage["id"])
        claim = ids[coverage["claim_id"]]["entry"]
        if coverage["function_id"] not in claim["depends_on_function_ids"] or operation_routes[coverage["operation_id"]] not in claim["depends_on_route_ids"]:
            raise HandoffError(f"{coverage['id']} is outside its claim's function/route scope")
        if coverage["status"] == "verified":
            if not coverage["scope_checked_source_ids"] or not any(ids[source_id]["entry"]["role"] == "test_result" for source_id in coverage["validation_source_ids"]):
                raise HandoffError(f"{coverage['id']} verified coverage requires scoped physical test evidence")
    for question in packet["questions"]:
        expect(question["source_ids"], {"source"}, question["id"])
        expect(question["target_ids"], {"feature", "function", "route", "claim", "operation", "coverage"}, question["id"])
    for assumption in packet["assumptions"]:
        expect(assumption["source_ids"], {"source"}, assumption["id"])
        expect(assumption["applies_to_claim_ids"], {"claim"}, assumption["id"])
    for output in packet["downstream_outputs"]:
        expect(output["source_ids"], {"source"}, output["id"])
        expect(output["depends_on_claim_ids"], {"claim"}, output["id"])
    return ids


def verify(packet_path: Path, project_root: Path) -> tuple[dict, dict]:
    packet_path = packet_path.resolve()
    project_root = project_root.resolve()
    packet = json.loads(packet_path.read_text(encoding="utf-8"))
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    errors = sorted(Draft202012Validator(schema).iter_errors(packet), key=lambda error: list(map(str, error.path)))
    if errors:
        first = errors[0]
        raise HandoffError(f"schema: /{'/'.join(map(str, first.path))}: {first.message}")
    ids = _check_refs(packet)
    source_results = []
    source_paths = {}
    for source in packet["sources"]:
        path = _check_source_path(project_root, source["path"])
        actual_sha = _sha256(path)
        if actual_sha != source["sha256"]:
            raise HandoffError(f"source SHA-256 mismatch: {source['id']}: {source['path']}")
        source_results.append({"id": source["id"], "path": source["path"], "sha256": actual_sha})
        source_paths[source["id"]] = path
    source_by_id = {source["id"]: source for source in packet["sources"]}
    json_cache = {}

    def source_json(source_id: str) -> object:
        source = source_by_id[source_id]
        if source["media_type"] != "application/json":
            raise HandoffError(f"JSON pointer requires application/json source: {source_id}")
        if source_id not in json_cache:
            json_cache[source_id] = json.loads(source_paths[source_id].read_text(encoding="utf-8"))
        return json_cache[source_id]

    for source in packet["sources"]:
        for pointer, target_id in source.get("embedded_source_hashes", {}).items():
            embedded = _json_pointer(source_json(source["id"]), pointer)
            if embedded != source_by_id[target_id]["sha256"]:
                raise HandoffError(f"embedded source hash mismatch: {source['id']} {pointer} -> {target_id}")
    for feature in packet["features"]:
        if feature["basis"] != "native_geometry_readback":
            continue
        observed = _json_pointer(source_json(feature["source_id"]), feature["source_locator"])
        if not isinstance(observed, dict):
            raise HandoffError(f"native readback locator is not an object: {feature['id']}")
        for key, value in feature.get("attributes", {}).items():
            if observed.get(key) != value:
                raise HandoffError(f"native readback attribute mismatch: {feature['id']}.{key}")
    audit = {
        "record_type": "ontology-engineering.cad-process-handoff-integrity/v1",
        "handoff_id": packet["handoff_id"],
        "packet_sha256": _sha256(packet_path),
        "schema_sha256": _sha256(SCHEMA),
        "adapter_sha256": _sha256(Path(__file__)),
        "source_integrity": "verified",
        "source_results": source_results,
        "entity_count": len(ids),
        "semantic_execution": "not_run",
        "engineering_verdict": "not_assessed",
    }
    return packet, audit


def project_abox(packet: dict, packet_sha256: str | None = None) -> bytes:
    project = packet["project"]
    identity = ":".join(quote(project[key], safe="") for key in ("project_id", "configuration_id", "model_revision", "product_state"))
    base = f"urn:ontology-engineering:project:{identity}:"

    def uri(kind: str, item_id: str) -> Iri:
        return Iri(base + kind + ":" + quote(item_id, safe=""))

    def lit(value: object) -> Text:
        return Text(value)

    graph = TripleLines()
    snapshot = uri("snapshot", packet["handoff_id"])
    graph.add((snapshot, RdfTerms.type, CP.ProjectSnapshot))
    if packet_sha256 is not None:
        graph.add((snapshot, CP.packet_sha256, lit(packet_sha256)))
    for key in ("project_id", "configuration_id", "model_revision", "product_state"):
        graph.add((snapshot, CP[key], lit(project[key])))
    for source in packet["sources"]:
        subject = uri("source", source["id"])
        graph.add((snapshot, CP.hasSource, subject))
        graph.add((subject, RdfTerms.type, CP.EvidenceSource))
        for key in ("id", "path", "sha256", "media_type", "role"):
            graph.add((subject, CP[key], lit(source[key])))
    for feature in packet["features"]:
        subject = uri("feature", feature["id"])
        graph.add((snapshot, CP.hasFeature, subject))
        graph.add((subject, RdfTerms.type, CP.CadFeature))
        for key in ("id", "part_id", "kind", "statement", "geometry_state", "basis", "source_locator"):
            graph.add((subject, CP[key], lit(feature[key])))
        graph.add((subject, CP.wasDerivedFrom, uri("source", feature["source_id"])))
        if feature.get("attributes"):
            graph.add((subject, CP.attributesJson, lit(json.dumps(feature["attributes"], ensure_ascii=False, sort_keys=True))))
    for function in packet["functions"]:
        subject = uri("function", function["id"])
        graph.add((snapshot, CP.hasFunction, subject))
        graph.add((subject, RdfTerms.type, CP.FunctionRequirement))
        for key in ("id", "name", "status", "basis"):
            graph.add((subject, CP[key], lit(function[key])))
        for source_id in function["source_ids"]:
            graph.add((subject, CP.wasDerivedFrom, uri("source", source_id)))
        for feature_id in function["feature_ids"]:
            graph.add((subject, CP.appliesToFeature, uri("feature", feature_id)))
    for route in packet["routes"]:
        subject = uri("route", route["id"])
        graph.add((snapshot, CP.hasRoute, subject))
        graph.add((subject, RdfTerms.type, CP.ProcessRoute))
        for key in ("id", "name", "status"):
            graph.add((subject, CP[key], lit(route[key])))
        for source_id in route["source_ids"]:
            graph.add((subject, CP.wasDerivedFrom, uri("source", source_id)))
        for operation in route["operations"]:
            op_uri = uri("operation", operation["id"])
            graph.add((subject, CP.hasOperation, op_uri))
            graph.add((op_uri, RdfTerms.type, CP.ProcessOperation))
            for key in ("id", "process_type", "mechanism_statement"):
                graph.add((op_uri, CP[key], lit(operation[key])))
            for feature_id in operation["target_feature_ids"]:
                graph.add((op_uri, CP.targetsFeature, uri("feature", feature_id)))
            for function_id in operation["intended_function_ids"]:
                graph.add((op_uri, CP.intendsFunction, uri("function", function_id)))
            for source_id in operation["source_ids"]:
                graph.add((op_uri, CP.wasDerivedFrom, uri("source", source_id)))
            for source_id in operation["validation_source_ids"]:
                graph.add((op_uri, CP.hasValidationSource, uri("source", source_id)))
    for coverage in packet["coverage_assertions"]:
        subject = uri("coverage", coverage["id"])
        graph.add((snapshot, CP.hasCoverageAssertion, subject))
        graph.add((subject, RdfTerms.type, CP.FunctionCoverageAssertion))
        graph.add((subject, CP.forClaim, uri("claim", coverage["claim_id"])))
        graph.add((subject, CP.forFunction, uri("function", coverage["function_id"])))
        graph.add((subject, CP.formedByOperation, uri("operation", coverage["operation_id"])))
        for key in ("id", "status", "mechanism_statement"):
            graph.add((subject, CP[key], lit(coverage[key])))
        for field, predicate in (("source_ids", CP.wasDerivedFrom), ("validation_source_ids", CP.hasValidationSource), ("scope_checked_source_ids", CP.hasScopeCheckSource)):
            for source_id in coverage[field]:
                graph.add((subject, predicate, uri("source", source_id)))
    for claim in packet["claims"]:
        subject = uri("claim", claim["id"])
        graph.add((snapshot, CP.hasClaim, subject))
        graph.add((subject, RdfTerms.type, CP.DecisionClaim))
        for key in ("id", "statement", "maturity"):
            graph.add((subject, CP[key], lit(claim[key])))
        for source_id in claim["source_ids"]:
            graph.add((subject, CP.wasDerivedFrom, uri("source", source_id)))
        for function_id in claim["depends_on_function_ids"]:
            graph.add((subject, CP.dependsOnFunction, uri("function", function_id)))
        for route_id in claim["depends_on_route_ids"]:
            graph.add((subject, CP.dependsOnRoute, uri("route", route_id)))
        for key, predicate in (("baseline_route_id", CP.baselineRoute), ("target_route_id", CP.targetRoute)):
            if key in claim:
                graph.add((subject, predicate, uri("route", claim[key])))
        for source_id in claim.get("function_list_completeness_source_ids", []):
            graph.add((subject, CP.functionListCompletenessEvidence, uri("source", source_id)))
        if "function_list_completeness_scope" in claim:
            graph.add((subject, CP.functionListCompletenessScope, lit(claim["function_list_completeness_scope"])))
    for question in packet["questions"]:
        subject = uri("question", question["id"])
        graph.add((snapshot, CP.hasQuestion, subject))
        graph.add((subject, RdfTerms.type, CP.EngineeringQuestion))
        for key in ("id", "direction", "text", "status"):
            graph.add((subject, CP[key], lit(question[key])))
        for target_id in question["target_ids"]:
            target_kind = next(kind for kind, entries in (("feature", packet["features"]), ("function", packet["functions"]), ("route", packet["routes"]), ("coverage", packet["coverage_assertions"]), ("claim", packet["claims"]), ("operation", [op for route in packet["routes"] for op in route["operations"]])) if any(entry["id"] == target_id for entry in entries))
            graph.add((subject, CP.targets, uri(target_kind, target_id)))
        for source_id in question["source_ids"]:
            graph.add((subject, CP.wasDerivedFrom, uri("source", source_id)))
        for requested in question["requested_evidence"]:
            graph.add((subject, CP.requestsEvidence, lit(requested)))
    for assumption in packet["assumptions"]:
        subject = uri("assumption", assumption["id"])
        graph.add((snapshot, CP.hasAssumption, subject))
        graph.add((subject, RdfTerms.type, CP.ModelAssumption))
        for key in ("id", "statement", "scope"):
            graph.add((subject, CP[key], lit(assumption[key])))
        for source_id in assumption["source_ids"]:
            graph.add((subject, CP.wasDerivedFrom, uri("source", source_id)))
        for claim_id in assumption["applies_to_claim_ids"]:
            graph.add((subject, CP.limitsClaimEvidence, uri("claim", claim_id)))
    for output in packet["downstream_outputs"]:
        subject = uri("downstream_output", output["id"])
        graph.add((snapshot, CP.hasDownstreamOutput, subject))
        graph.add((subject, RdfTerms.type, CP.DownstreamOutput))
        for key in ("id", "kind", "statement", "state"):
            graph.add((subject, CP[key], lit(output[key])))
        for source_id in output["source_ids"]:
            graph.add((subject, CP.wasDerivedFrom, uri("source", source_id)))
        for claim_id in output["depends_on_claim_ids"]:
            graph.add((subject, CP.dependsOnClaim, uri("claim", claim_id)))
    lines = sorted(f"{s.n3()} {p.n3()} {o.n3()} .\n" for s, p, o in graph)
    return "".join(lines).encode("utf-8")


def compare_packets(previous: dict, current: dict) -> dict:
    """Return explicit dependency impact, without judging engineering validity."""

    def entities(packet: dict) -> dict[str, tuple[str, dict, set[str]]]:
        result: dict[str, tuple[str, dict, set[str]]] = {}
        for source in packet["sources"]:
            result[source["id"]] = ("source", source, set(source.get("embedded_source_hashes", {}).values()))
        for feature in packet["features"]:
            result[feature["id"]] = ("feature", feature, {feature["source_id"]})
        for function in packet["functions"]:
            result[function["id"]] = ("function", function, set(function["source_ids"] + function["feature_ids"]))
        for route in packet["routes"]:
            operations = route["operations"]
            result[route["id"]] = ("route", route, set(route["source_ids"] + [operation["id"] for operation in operations]))
            for operation in operations:
                dependencies = operation["source_ids"] + operation["validation_source_ids"] + operation["target_feature_ids"] + operation["intended_function_ids"]
                result[operation["id"]] = ("operation", operation, set(dependencies))
        for coverage in packet["coverage_assertions"]:
            dependencies = [coverage["claim_id"], coverage["function_id"], coverage["operation_id"]] + coverage["source_ids"] + coverage["validation_source_ids"] + coverage["scope_checked_source_ids"]
            result[coverage["id"]] = ("coverage", coverage, set(dependencies))
        for claim in packet["claims"]:
            dependencies = claim["source_ids"] + claim["depends_on_function_ids"] + claim["depends_on_route_ids"] + claim.get("function_list_completeness_source_ids", [])
            dependencies += [coverage["id"] for coverage in packet["coverage_assertions"] if coverage["claim_id"] == claim["id"]]
            dependencies += [claim[key] for key in ("baseline_route_id", "target_route_id") if key in claim]
            result[claim["id"]] = ("claim", claim, set(dependencies))
        for question in packet["questions"]:
            result[question["id"]] = ("question", question, set(question["source_ids"] + question["target_ids"]))
        for assumption in packet["assumptions"]:
            result[assumption["id"]] = ("assumption", assumption, set(assumption["source_ids"]))
            for claim_id in assumption["applies_to_claim_ids"]:
                kind, entry, dependencies = result[claim_id]
                result[claim_id] = (kind, entry, dependencies | {assumption["id"]})
        for output in packet["downstream_outputs"]:
            result[output["id"]] = ("downstream_output", output, set(output["source_ids"] + output["depends_on_claim_ids"]))
        return result

    old = entities(previous)
    new = entities(current)
    changed = {identity for identity in old.keys() | new.keys() if old.get(identity) != new.get(identity)}
    identity_scope_changed = previous["project"] != current["project"]
    if identity_scope_changed:
        changed.update(identity for identity, (kind, _, _) in new.items() if kind in {"feature", "function", "route", "claim"})
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
        "record_type": "ontology-engineering.cad-process-dependency-impact/v1",
        "identity_scope_changed": identity_scope_changed,
        "changed_ids": sorted(changed),
        "affected_ids": sorted(affected),
        "affected_claim_ids": sorted(identity for identity in affected if new.get(identity, old.get(identity))[0] == "claim"),
        "affected_question_ids": sorted(identity for identity in affected if new.get(identity, old.get(identity))[0] == "question"),
        "affected_output_ids": sorted(identity for identity in affected if new.get(identity, old.get(identity))[0] == "downstream_output"),
        "semantic_execution": "not_run",
        "engineering_verdict": "re_review_required_not_assessed",
    }


def _write_once(path: Path, content: bytes) -> None:
    if path.exists():
        if path.read_bytes() != content:
            raise HandoffError(f"immutable output collision: {path}")
        return
    tmp = path.with_name(path.name + f".tmp.{os.getpid()}")
    try:
        tmp.write_bytes(content)
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packet", type=Path, required=True)
    parser.add_argument("--previous-packet", type=Path)
    parser.add_argument("--project-root", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        packet, audit = verify(args.packet.resolve(), args.project_root.resolve())
        previous = None
        previous_audit = None
        if args.previous_packet is not None:
            previous, previous_audit = verify(args.previous_packet.resolve(), args.project_root.resolve())
        abox = project_abox(packet, audit["packet_sha256"])
        digest = f"{audit['packet_sha256'][:12]}-{audit['schema_sha256'][:8]}-{audit['adapter_sha256'][:8]}"
        output_dir = args.output_dir.resolve()
        output_dir.mkdir(parents=True, exist_ok=True)
        abox_path = output_dir / f"cad-process-{digest}.ttl"
        audit_path = output_dir / f"cad-process-{digest}.integrity.json"
        cad_questions_path = output_dir / f"cad-requests-{digest}.json"
        process_questions_path = output_dir / f"process-requests-{digest}.json"
        audit["abox_sha256"] = hashlib.sha256(abox).hexdigest()
        audit["abox_path"] = str(abox_path)
        audit["cad_requests_path"] = str(cad_questions_path)
        audit["process_requests_path"] = str(process_questions_path)
        if previous is not None and previous_audit is not None:
            impact = compare_packets(previous, packet)
            impact["previous_packet_sha256"] = previous_audit["packet_sha256"]
            impact["current_packet_sha256"] = audit["packet_sha256"]
            impact_path = output_dir / f"impact-{previous_audit['packet_sha256'][:12]}-to-{digest}.json"
            _write_once(impact_path, (json.dumps(impact, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8"))
            audit["impact_path"] = str(impact_path)
        for direction, path in (("process_to_cad", cad_questions_path), ("cad_to_process", process_questions_path)):
            requests = {
                "record_type": "ontology-engineering.cad-process-requests/v1",
                "handoff_id": packet["handoff_id"],
                "packet_sha256": audit["packet_sha256"],
                "direction": direction,
                "questions": [question for question in packet["questions"] if question["direction"] == direction],
                "engineering_verdict": "not_assessed",
            }
            _write_once(path, (json.dumps(requests, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8"))
        _write_once(abox_path, abox)
        _write_once(audit_path, (json.dumps(audit, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8"))
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(json.dumps({"status": "blocked", "error": str(exc)}, ensure_ascii=False))
        return 2
    print(json.dumps({"status": "projected", "packet_sha256": audit["packet_sha256"], "abox_path": str(abox_path), "audit_path": str(audit_path), "semantic_execution": "not_run"}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
