#!/usr/bin/env python3
"""Project a verified CAD/process handoff into the proposed MFG q21 input view.

This adapter only writes RDF. It does not execute the proposed package, infer
functional coverage, decide a manufacturing route, or issue a product verdict.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
from urllib.parse import quote

CAD_MODULE = Path(__file__).resolve().parents[2] / "cad-agent"
sys.path.insert(0, str(CAD_MODULE / "scripts"))
from cad_process_handoff import HandoffError, verify  # noqa: E402
from rdf_lines import Iri, Namespace, RdfTerms, Text, TripleLines  # noqa: E402


P = Namespace("urn:mfg:loop:")
PACKAGE_ID = "semantica.manufacturing.process-cost-loop"
PROPOSED_VERSION = "0.1.3"
PROPOSED_DELTA_SHA256 = "73bdf2e23f73d64b98aa5fc0223ee1d842fccfdbf0d2ae8ccc2c031e6fbd4dd8"


def project_q21(packet: dict, claim_id: str, packet_sha256: str) -> tuple[bytes, str]:
    claim = next((item for item in packet["claims"] if item["id"] == claim_id), None)
    if claim is None or claim["maturity"] != "batch_delivery":
        raise HandoffError("q21 view requires one batch-delivery decision claim")
    if not claim.get("baseline_route_id") or not claim.get("target_route_id"):
        raise HandoffError("q21 view requires explicit baseline and target process routes")
    routes = {route["id"]: route for route in packet["routes"]}
    functions = {function["id"]: function for function in packet["functions"]}
    if claim["target_route_id"] not in claim["depends_on_route_ids"]:
        raise HandoffError("target route is not a dependency of the decision claim")
    if claim["baseline_route_id"] not in routes:
        raise HandoffError("baseline route is absent")

    project = packet["project"]
    prefix = ".".join(quote(project[key], safe="") for key in ("project_id", "configuration_id", "model_revision"))

    def item_uri(kind: str, identity: str) -> Iri:
        return Iri(str(P) + prefix + "." + kind + "." + quote(identity, safe=""))

    graph = TripleLines()
    focus = item_uri("claim", claim_id)
    snapshot = item_uri("snapshot", packet["handoff_id"])
    graph.add((focus, RdfTerms.type, P.ProcessReplacementClaim))
    graph.add((focus, P.baselineRoute, item_uri("route", claim["baseline_route_id"])))
    graph.add((focus, P.targetRoute, item_uri("route", claim["target_route_id"])))
    graph.add((focus, P.decisionLevel, Text("production")))
    graph.add((focus, P.sourceSnapshot, snapshot))
    graph.add((focus, P.recordKind, Text("project_input_projection_not_release")))
    graph.add((snapshot, P.digest, Text(packet_sha256)))
    graph.add((snapshot, P.productVersion, Text(project["model_revision"])))
    graph.add((snapshot, P.testedState, Text(project["product_state"])))
    for source_id in claim.get("function_list_completeness_source_ids", []):
        graph.add((focus, P.completenessEvidence, item_uri("source", source_id)))
    if "function_list_completeness_scope" in claim:
        graph.add((focus, P.scope, Text(claim["function_list_completeness_scope"])))
    for function_id in claim["depends_on_function_ids"]:
        function = functions[function_id]
        if function["status"] != "required":
            raise HandoffError(f"q21 required function is only a candidate: {function_id}")
        function_uri = item_uri("function", function_id)
        graph.add((focus, P.requiredFunction, function_uri))
        graph.add((function_uri, RdfTerms.type, P.RequiredFunction))
        graph.add((function_uri, P.scope, Text(function["basis"])))
        for source_id in function["source_ids"]:
            graph.add((function_uri, P.source, item_uri("source", source_id)))
    for coverage in packet["coverage_assertions"]:
        if coverage["claim_id"] != claim_id:
            continue
        coverage_uri = item_uri("coverage", coverage["id"])
        graph.add((coverage_uri, RdfTerms.type, P.FunctionCoverage))
        graph.add((coverage_uri, P.forClaim, focus))
        graph.add((coverage_uri, P.forFunction, item_uri("function", coverage["function_id"])))
        graph.add((coverage_uri, P.mechanism, item_uri("operation", coverage["operation_id"])))
        graph.add((coverage_uri, P.status, Text(coverage["status"])))
        for source_id in coverage["validation_source_ids"]:
            graph.add((coverage_uri, P.validatedBy, item_uri("source", source_id)))
        for source_id in coverage["scope_checked_source_ids"]:
            graph.add((coverage_uri, P.scopeChecked, item_uri("source", source_id)))
    sources = {source["id"]: source for source in packet["sources"]}
    for source_id, source in sources.items():
        subject = item_uri("source", source_id)
        graph.add((subject, P.digest, Text(source["sha256"])))
        graph.add((subject, P.recordKind, Text(source["role"])))
    lines = sorted(f"{s.n3()} {p.n3()} {o.n3()} .\n" for s, p, o in graph)
    return "".join(lines).encode("utf-8"), str(focus)


def _write_once(path: Path, content: bytes) -> None:
    if path.exists():
        if path.read_bytes() != content:
            raise HandoffError(f"immutable output collision: {path}")
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
    parser.add_argument("--project-root", type=Path, required=True)
    parser.add_argument("--claim-id", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        packet, source_audit = verify(args.packet, args.project_root)
        rdf, focus = project_q21(packet, args.claim_id, source_audit["packet_sha256"])
        digest = "-".join((source_audit["packet_sha256"][:12], source_audit["schema_sha256"][:8], hashlib.sha256(Path(__file__).read_bytes()).hexdigest()[:8]))
        output_dir = args.output_dir.resolve()
        output_dir.mkdir(parents=True, exist_ok=True)
        view_path = output_dir / f"q21-authoring-view-{digest}.ttl"
        audit_path = output_dir / f"q21-authoring-view-{digest}.json"
        audit = {
            "record_type": "ontology-engineering.mfg-q21-authoring-projection/v1",
            "packet_sha256": source_audit["packet_sha256"],
            "source_integrity": source_audit["source_integrity"],
            "project_abox_focus": focus,
            "focus_type": str(P.ProcessReplacementClaim),
            "package_id": PACKAGE_ID,
            "proposed_version": PROPOSED_VERSION,
            "proposed_delta_sha256": PROPOSED_DELTA_SHA256,
            "query_asset_id": "q21",
            "shape_asset_id": "causal-decision-shape",
            "view_sha256": hashlib.sha256(rdf).hexdigest(),
            "view_path": str(view_path),
            "package_state": "proposed_not_promoted",
            "semantic_execution": "not_run",
            "engineering_verdict": "not_assessed",
        }
        _write_once(view_path, rdf)
        _write_once(audit_path, (json.dumps(audit, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8"))
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(json.dumps({"status": "blocked", "error": str(exc)}, ensure_ascii=False))
        return 2
    print(json.dumps({"status": "projected_for_authoring_only", "view_path": str(view_path), "audit_path": str(audit_path), "semantic_execution": "not_run"}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
