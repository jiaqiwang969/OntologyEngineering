#!/usr/bin/env python3
"""ch11 capstone：对 manifest 登记的候选知识包执行统一检查。

核验并加载 manifest 登记的 data 与 shapes 输入，然后：
  1. 对冻结图运行已登记 SHACL 规则；
  2. 汇总每个已登记相关项的危害事件、安全目标与显式需求类型；
  3. 只读输出 Clause 8 计划态候选证据与开放主张边界；
  4. 检查已进入系统层的非 QM 安全目标是否至少有一个已分配后代需求。

这是知识模型层面的收口演示；通过不等于 ISO 26262 验证、确认评审或放行。
用法：.venv/bin/python functional-safety-book/ch11-capstone-three-items/src/capstone.py
"""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path
from typing import Iterable, Sequence

from pyshacl import validate
from rdflib import Graph
from rdflib.namespace import RDFS
import yaml

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "functional-safety-book/ch11-capstone-three-items/bundle-manifest.yaml"
BUNDLE_ID = "ch11-capstone-three-items"
SHA256_RE = re.compile(r"[0-9a-f]{64}")

TRACE_CLOSURE_QUERY = """PREFIX iso262: <https://w3id.org/iso26262#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?sg WHERE {
  ?sg a iso262:SafetyGoal ; iso262:hasASIL ?a ; iso262:hasSystemDevelopment true .
  FILTER(?a != iso262:QM)
  FILTER NOT EXISTS {
    ?req a ?reqType ; iso262:derivedFrom+ ?sg ; iso262:allocatedTo ?el .
    ?reqType rdfs:subClassOf* iso262:SafetyRequirement .
  }
}
"""

ITEM_SUMMARY_QUERY = """PREFIX iso262: <https://w3id.org/iso26262#>
SELECT ?item
       (COUNT(DISTINCT ?he) AS ?events)
       (COUNT(DISTINCT ?sg) AS ?goals)
       (COUNT(DISTINCT ?req) AS ?derivedReqs) WHERE {
  ?item a iso262:Item .
  OPTIONAL { ?he a iso262:HazardousEvent ; iso262:hazardOf ?item . }
  OPTIONAL {
    ?he2 a iso262:HazardousEvent ;
         iso262:hazardOf ?item ;
         iso262:leadsToSafetyGoal ?sg .
    OPTIONAL {
      ?req a ?reqType ; iso262:derivedFrom+ ?sg .
      VALUES ?reqType {
        iso262:FunctionalSafetyRequirement
        iso262:TechnicalSafetyRequirement
        iso262:SoftwareSafetyRequirement
        iso262:HSIRequirement
      }
    }
  }
} GROUP BY ?item ORDER BY ?item
"""

ASIL_D_CROSS_VIEW_QUERY = """PREFIX iso262: <https://w3id.org/iso26262#>
SELECT DISTINCT ?item ?sg WHERE {
  ?event a iso262:HazardousEvent ;
         iso262:hazardOf ?item ;
         iso262:leadsToSafetyGoal ?sg .
  ?sg a iso262:SafetyGoal ; iso262:hasASIL iso262:ASIL_D .
} ORDER BY ?item ?sg
"""

CLAUSE8_BOUNDARY_QUERY = """PREFIX iso262: <https://w3id.org/iso26262#>
PREFIX eps: <https://w3id.org/iso26262/eps#>
SELECT ?spec ?specReviewStatus ?specEvidenceStatus
       ?activity ?activityStatus
       ?result ?resultStatus
       ?evaluation ?evaluationStatus
       ?report ?reportReviewStatus ?reportEvidenceStatus
       ?claim ?claimStatus WHERE {
  BIND(eps:EPS_SafetyValidationSpecification_Draft AS ?spec)
  BIND(eps:EPS_VehicleSafetyValidation_Planned AS ?activity)
  BIND(eps:EPS_SafetyValidationResult_SG1_NotRun AS ?result)
  BIND(eps:EPS_SafetyValidationEvaluation_SG1_NotPerformed AS ?evaluation)
  BIND(eps:EPS_SafetyValidationReport_Template AS ?report)
  BIND(eps:Claim_SG1 AS ?claim)
  ?spec a iso262:SafetyValidationSpecification, iso262:Evidence ;
        iso262:reviewStatus ?specReviewStatus ;
        iso262:evidenceStatus ?specEvidenceStatus .
  ?activity a iso262:SafetyValidationActivity ;
            iso262:usesValidationSpecification ?spec ;
            iso262:safetyValidationExecutionStatus ?activityStatus ;
            iso262:produces ?report .
  ?report a iso262:SafetyValidationReport, iso262:Evidence ;
          iso262:hasValidationResult ?result ;
          iso262:hasValidationEvaluation ?evaluation ;
          iso262:reviewStatus ?reportReviewStatus ;
          iso262:evidenceStatus ?reportEvidenceStatus .
  ?result a iso262:SafetyValidationResult ;
          iso262:resultForCase ?case ;
          iso262:validationResultStatus ?resultStatus .
  ?evaluation a iso262:SafetyValidationEvaluation ;
              iso262:evaluatesValidationResult ?result ;
              iso262:evaluatesSafetyGoal ?goal ;
              iso262:validationEvaluationStatus ?evaluationStatus .
  ?case iso262:validatesSafetyGoal ?goal .
  ?claim a iso262:Claim ;
         iso262:addressesGoal ?goal ;
         iso262:claimStatus ?claimStatus .
  eps:Arg_SG1 a iso262:Argument ;
              iso262:supportsClaim ?claim ;
              iso262:backedByEvidence ?spec, ?report .
}
"""


class BundleManifestError(ValueError):
    """The frozen capstone bundle is malformed, incomplete, or has drifted."""


@dataclass(frozen=True)
class FrozenBundle:
    data_paths: tuple[Path, ...]
    shapes_path: Path
    bundle_sha256: str
    manifest_sha256: str
    data_payloads: tuple[bytes, ...]
    shapes_payload: bytes


def hash_file(path: Path, chunk_size: int = 1024 * 1024) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(chunk_size):
            digest.update(chunk)
    return digest.hexdigest()


def _distribution_version(distribution: str) -> str:
    try:
        return version(distribution)
    except PackageNotFoundError:
        return "not-installed"


def validator_identity() -> dict[str, str]:
    """Identify the on-disk validator and runtime used for this invocation."""
    return {
        "capstone_sha256": hash_file(Path(__file__).resolve()),
        "python": platform.python_version(),
        "rdflib": _distribution_version("rdflib"),
        "pyshacl": _distribution_version("pyshacl"),
        "pyyaml": _distribution_version("PyYAML"),
    }


def calculate_bundle_sha256(entries: Sequence[dict[str, str]]) -> str:
    canonical = sorted(
        (
            {
                "path": entry["path"],
                "role": entry["role"],
                "sha256": entry["sha256"],
            }
            for entry in entries
        ),
        key=lambda entry: (entry["path"], entry["role"]),
    )
    payload = json.dumps(
        canonical, ensure_ascii=True, sort_keys=True, separators=(",", ":")
    )
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def _relative_input(path: Path, root: Path) -> str:
    resolved = path.resolve()
    try:
        return resolved.relative_to(root.resolve()).as_posix()
    except ValueError as exc:
        raise BundleManifestError(f"bundle input escapes repository root: {path}") from exc


def build_bundle_manifest_document(
    data_paths: Iterable[Path],
    shapes_path: Path,
    *,
    root: Path = ROOT,
    generated_at_utc: str | None = None,
) -> dict[str, object]:
    """Build a hash-bound manifest document; writing it is an explicit freeze step."""
    root = root.resolve()
    entries = [
        {
            "path": _relative_input(path, root),
            "role": "data",
            "sha256": hash_file(path),
        }
        for path in data_paths
    ]
    entries.append(
        {
            "path": _relative_input(shapes_path, root),
            "role": "shapes",
            "sha256": hash_file(shapes_path),
        }
    )
    entries.sort(key=lambda entry: (entry["path"], entry["role"]))
    paths = [entry["path"] for entry in entries]
    if len(paths) != len(set(paths)):
        raise BundleManifestError("bundle manifest cannot contain duplicate paths")
    if not any(entry["role"] == "data" for entry in entries):
        raise BundleManifestError("bundle manifest requires at least one data input")

    return {
        "schema_version": 1,
        "bundle_id": BUNDLE_ID,
        "generated_at_utc": generated_at_utc
        or datetime.now(timezone.utc).isoformat(),
        "bundle_sha256": calculate_bundle_sha256(entries),
        "inputs": entries,
    }


def load_bundle_manifest(
    manifest_path: Path = MANIFEST, *, root: Path = ROOT
) -> FrozenBundle:
    root = root.resolve()
    if not manifest_path.is_file():
        raise BundleManifestError(f"bundle manifest is missing: {manifest_path}")
    try:
        raw_manifest = manifest_path.read_bytes()
        document = yaml.safe_load(raw_manifest.decode("utf-8"))
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        raise BundleManifestError(f"cannot read bundle manifest: {exc}") from exc
    if not isinstance(document, dict):
        raise BundleManifestError("bundle manifest root must be a mapping")
    if document.get("schema_version") != 1:
        raise BundleManifestError("bundle manifest schema_version must be 1")
    if document.get("bundle_id") != BUNDLE_ID:
        raise BundleManifestError(f"bundle_id must be {BUNDLE_ID!r}")

    entries = document.get("inputs")
    if not isinstance(entries, list) or not entries:
        raise BundleManifestError("bundle manifest inputs must be a non-empty list")

    normalized: list[dict[str, str]] = []
    data_paths: list[Path] = []
    data_payloads: list[bytes] = []
    shapes_paths: list[Path] = []
    shapes_payloads: list[bytes] = []
    seen_paths: set[str] = set()
    seen_resolved_paths: set[Path] = set()
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            raise BundleManifestError(f"inputs[{index}] must be a mapping")
        relative = entry.get("path")
        role = entry.get("role")
        expected_hash = entry.get("sha256")
        if not isinstance(relative, str) or not relative:
            raise BundleManifestError(f"inputs[{index}].path must be a non-empty string")
        relative_path = Path(relative)
        if relative_path.is_absolute():
            raise BundleManifestError(
                f"bundle path must be repository-relative: {relative}"
            )
        if ".." in relative_path.parts:
            raise BundleManifestError(
                f"bundle path escapes or is not normalized: {relative}"
            )
        if relative_path.as_posix() != relative:
            raise BundleManifestError(
                f"bundle path must be a normalized repository-relative path: {relative}"
            )
        if role not in {"data", "shapes"}:
            raise BundleManifestError(f"inputs[{index}].role must be data or shapes")
        if not isinstance(expected_hash, str) or not SHA256_RE.fullmatch(expected_hash):
            raise BundleManifestError(f"inputs[{index}].sha256 must be lowercase SHA-256")
        if relative in seen_paths:
            raise BundleManifestError(f"duplicate bundle path: {relative}")
        seen_paths.add(relative)

        path = (root / relative_path).resolve()
        try:
            canonical_relative = path.relative_to(root).as_posix()
        except ValueError as exc:
            raise BundleManifestError(f"bundle path escapes repository root: {relative}") from exc
        if canonical_relative != relative:
            raise BundleManifestError(
                f"bundle path must identify its canonical repository location: {relative}"
            )
        if path in seen_resolved_paths:
            raise BundleManifestError(f"duplicate resolved bundle path: {relative}")
        seen_resolved_paths.add(path)
        if not path.is_file():
            raise BundleManifestError(f"bundle input is missing: {relative}")
        try:
            payload = path.read_bytes()
        except OSError as exc:
            raise BundleManifestError(f"cannot read bundle input {relative}: {exc}") from exc
        actual_hash = hashlib.sha256(payload).hexdigest()
        if actual_hash != expected_hash:
            raise BundleManifestError(
                f"bundle input hash drift: {relative} expected={expected_hash} actual={actual_hash}"
            )

        normalized.append({"path": relative, "role": role, "sha256": expected_hash})
        if role == "data":
            data_paths.append(path)
            data_payloads.append(payload)
        else:
            shapes_paths.append(path)
            shapes_payloads.append(payload)

    if not data_paths:
        raise BundleManifestError("bundle manifest requires at least one data input")
    if len(shapes_paths) != 1:
        raise BundleManifestError("bundle manifest requires exactly one shapes input")

    calculated_id = calculate_bundle_sha256(normalized)
    declared_id = document.get("bundle_sha256")
    if declared_id != calculated_id:
        raise BundleManifestError(
            f"bundle_sha256 mismatch: expected={declared_id} actual={calculated_id}"
        )
    manifest_hash = hashlib.sha256(raw_manifest).hexdigest()
    return FrozenBundle(
        tuple(data_paths),
        shapes_paths[0],
        calculated_id,
        manifest_hash,
        tuple(data_payloads),
        shapes_payloads[0],
    )


def verify_bundle_stability(
    initial: FrozenBundle, manifest_path: Path = MANIFEST, *, root: Path = ROOT
) -> None:
    """Fail if the final manifest/input snapshot differs from the initial one."""
    current = load_bundle_manifest(manifest_path, root=root)
    if current.bundle_sha256 != initial.bundle_sha256:
        raise BundleManifestError(
            "bundle identity changed during validation: "
            f"before={initial.bundle_sha256} after={current.bundle_sha256}"
        )
    if current.manifest_sha256 != initial.manifest_sha256:
        raise BundleManifestError(
            "bundle manifest changed during validation: "
            f"before={initial.manifest_sha256} after={current.manifest_sha256}"
        )


def load(inputs: Iterable[tuple[Path, bytes]]) -> Graph:
    g = Graph()
    for path, payload in inputs:
        g.parse(data=payload, format="turtle", publicID=path.as_uri())
    return g


def shacl_gate(g: Graph, shapes_path: Path, shapes_payload: bytes) -> bool:
    shapes = Graph().parse(
        data=shapes_payload, format="turtle", publicID=shapes_path.as_uri()
    )
    conforms, _, _ = validate(g, shacl_graph=shapes, inference="rdfs", advanced=True)
    print(f"[SHACL] conforms={conforms}")
    return bool(conforms)


def display_term(g: Graph, term: object) -> str:
    """Use one deterministic label for display without changing query cardinality."""
    labels = sorted(str(label) for label in g.objects(term, RDFS.label))
    return labels[0] if labels else str(term)


def item_summary(g: Graph) -> None:
    print("\n== 三案例对照（一套 TBox，三个 ABox）==")
    for r in g.query(ITEM_SUMMARY_QUERY):
        print(
            f"  {display_term(g, r.item)}: 危害事件={r.events} 安全目标={r.goals} "
            f"派生安全需求(已登记类型)={r.derivedReqs}"
        )


def trace_gaps(g: Graph) -> list[object]:
    """Return system-development ASIL goals without an allocated descendant."""
    return [row.sg for row in g.query(TRACE_CLOSURE_QUERY)]


def trace_check(g: Graph) -> bool:
    """Check each in-scope non-QM goal has an allocated descendant requirement."""
    missing = trace_gaps(g)
    if missing:
        print("\n[TRACE] 缺少已分配安全需求后代的系统层安全目标：")
        for safety_goal in missing:
            print("  ", safety_goal)
        return False
    print("\n[TRACE] 每个已进入系统层的非 QM 安全目标至少有一个已分配安全需求后代。")
    return True


def asil_d_cross_view(g: Graph) -> None:
    print("\n== 当前冻结图中的 ASIL D 安全目标 ==")
    for r in g.query(ASIL_D_CROSS_VIEW_QUERY):
        print(f"  {display_term(g, r.item)} -> {display_term(g, r.sg)}")


def clause8_boundary(g: Graph) -> None:
    """Print the Clause 8 candidate-evidence boundary without strengthening it."""
    print("\n== EPS Clause 8 候选证据边界 ==")
    rows = list(g.query(CLAUSE8_BOUNDARY_QUERY))
    if not rows:
        print("  [BOUNDARY] 当前冻结图未命中 CQ-CH11-03 状态链。")
        return
    for r in rows:
        print(
            f"  spec={display_term(g, r.spec)} "
            f"review={display_term(g, r.specReviewStatus)} "
            f"evidence={display_term(g, r.specEvidenceStatus)}"
        )
        print(
            f"  activity={display_term(g, r.activity)} "
            f"execution={display_term(g, r.activityStatus)}"
        )
        print(
            f"  result={display_term(g, r.result)} "
            f"status={display_term(g, r.resultStatus)}"
        )
        print(
            f"  evaluation={display_term(g, r.evaluation)} "
            f"status={display_term(g, r.evaluationStatus)}"
        )
        print(
            f"  report={display_term(g, r.report)} "
            f"review={display_term(g, r.reportReviewStatus)} "
            f"evidence={display_term(g, r.reportEvidenceStatus)}"
        )
        print(
            f"  claim={display_term(g, r.claim)} "
            f"status={display_term(g, r.claimStatus)}"
        )
    print(
        "  [BOUNDARY] Draft/Planned/NotRun/NotPerformed/Candidate "
        "只表示已登记候选边界，不证明安全目标已达成。"
    )


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=MANIFEST,
        help="hash-bound bundle manifest (defaults to the chapter manifest)",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    manifest_path = args.manifest if args.manifest.is_absolute() else ROOT / args.manifest
    identity = validator_identity()
    print(f"[VALIDATOR] capstone_sha256={identity['capstone_sha256']}")
    print(
        f"[RUNTIME] python={identity['python']} rdflib={identity['rdflib']} "
        f"pyshacl={identity['pyshacl']} pyyaml={identity['pyyaml']}"
    )
    try:
        bundle = load_bundle_manifest(manifest_path)
    except BundleManifestError as exc:
        print(f"[BUNDLE] FAIL: {exc}", file=sys.stderr)
        return 2
    try:
        shown_manifest = manifest_path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        shown_manifest = str(manifest_path.resolve())
    print(
        f"[BUNDLE] manifest={shown_manifest} inputs={len(bundle.data_paths) + 1} "
        f"bundle_sha256={bundle.bundle_sha256} "
        f"manifest_sha256={bundle.manifest_sha256}"
    )
    g = load(zip(bundle.data_paths, bundle.data_payloads, strict=True))
    print(f"[LOAD] triples={len(g)}")
    ok = shacl_gate(g, bundle.shapes_path, bundle.shapes_payload)
    item_summary(g)
    asil_d_cross_view(g)
    clause8_boundary(g)
    ok = trace_check(g) and ok
    try:
        verify_bundle_stability(bundle, manifest_path)
    except BundleManifestError as exc:
        print(f"[BUNDLE] FAIL after validation: {exc}", file=sys.stderr)
        return 2
    print(f"[BUNDLE] final_recheck=match manifest_sha256={bundle.manifest_sha256}")
    print("\ncapstone 知识模型收口：", "通过" if ok else "未通过")
    print("（案例为合成教学数据；通过≠ISO 验证、确认评审、工程充分性或放行。）")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
