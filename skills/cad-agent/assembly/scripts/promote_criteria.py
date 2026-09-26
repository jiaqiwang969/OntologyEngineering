#!/usr/bin/env python3
"""把一组判据的 validation.status 改为指定状态并写证据，重算 snapshot-manifest 的计数与哈希（O-23 转正用）。
用法：promote_criteria.py --ids K-IF-09,K-GOV-15 --status VALIDATED --evidence "…" --snapshot-id <id>
之后必须运行 scripts/validate_bundle.py 与 canonical_content_digest.py 并更新 BUILD_INFO。"""
import argparse, collections, datetime, hashlib, json
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1] / "references" / "ontology"
ap = argparse.ArgumentParser(); ap.add_argument("--ids", required=True); ap.add_argument("--status", required=True)
ap.add_argument("--evidence", required=True); ap.add_argument("--snapshot-id", required=True); ap.add_argument("--derivation", default="")
a = ap.parse_args(); ids = set(a.ids.split(","))
def load(f): return json.loads((ROOT / f).read_text(encoding="utf-8"))
def dump(f, d): (ROOT / f).write_text(json.dumps(d, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
C = load("criteria.v1.json"); n = 0
for k in C["criteria"]:
    if k["id"] in ids:
        k["validation"] = {"status": a.status, "evidence": a.evidence}; n += 1
dump("criteria.v1.json", C)
M = load("snapshot-manifest.v1.json"); F = load("failures.v1.json"); P = load("pipeline.v1.json"); T = load("tool-capabilities.v1.json"); O = load("open-items.v1.json"); pol = load("algorithm-physics-routing-policy.v1.json")
M["predecessor_snapshot_id"] = M["snapshot_id"]; M["snapshot_id"] = a.snapshot_id
M["created_at"] = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
if a.derivation: M.setdefault("source", {})["derivation"] = a.derivation
M["counts"] = {"criteria": len(C["criteria"]), "failures": len(F["failures"]), "stages": len(P["stages"]), "conditional_branches": len(P.get("conditional_branches", [])),
               "tools": len(T["tools"]), "open_items": len(O["items"]), "paper_algorithm_routes": len(pol["paper_algorithm_routes"]), "route_matrix": len(pol["route_matrix"])}
M["criteria_by_validation_status"] = dict(sorted(collections.Counter(k["validation"]["status"] for k in C["criteria"]).items()))
for fn in M["files"]: M["files"][fn]["bundled_sha256"] = hashlib.sha256((ROOT / fn).read_bytes()).hexdigest()
dump("snapshot-manifest.v1.json", M); print(f"promoted {n} criteria → {a.status}; snapshot {a.snapshot_id}; status counts {M['criteria_by_validation_status']}")
