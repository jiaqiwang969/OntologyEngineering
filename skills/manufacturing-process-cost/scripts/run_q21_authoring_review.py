#!/usr/bin/env python3
"""Run a proposed q21 package against a CAD handoff projection for authoring only.

The Semantica DecisionReviewRunner is the sole semantic executor. This entry
cannot issue a project review receipt or promote a package.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
import sys
from pathlib import Path
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from ontology_engineering.semantica_runtime import native_candidate_authoring_review


def _digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--delta", type=Path, required=True)
    parser.add_argument("--projection-audit", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        delta_path = args.delta.resolve()
        raw_delta = delta_path.read_bytes()
        delta = json.loads(raw_delta)
        delta_sha256 = delta.get("delta_sha256")
        state = json.loads((delta_path.parent / "current.json").read_text(encoding="utf-8"))
        if state.get("delta_sha256") != delta_sha256 or state.get("state") != "proposed" or state.get("package_sha256") is not None:
            raise ValueError("candidate is not the exact proposed, unpromoted delta")
        audit_path = args.projection_audit.resolve()
        audit = json.loads(audit_path.read_text(encoding="utf-8"))
        if audit.get("package_state") != "proposed_not_promoted" or audit.get("proposed_delta_sha256") != delta_sha256 or audit.get("source_integrity") != "verified":
            raise ValueError("projection audit is not bound to this proposed candidate")
        view_path = Path(audit["view_path"]).resolve()
        view = view_path.read_bytes()
        if _digest(view) != audit["view_sha256"]:
            raise ValueError("projection RDF bytes differ from the integrity audit")
        native_result = native_candidate_authoring_review(
            delta=delta,
            evidence=view,
            evidence_id="cad-process-q21-" + audit["packet_sha256"][:12],
            evidence_uri="urn:ontology-engineering:cad-process:q21-view:" + quote(_digest(view), safe=""),
            evidence_captured_at=datetime.fromtimestamp(view_path.stat().st_mtime, timezone.utc).isoformat(),
            scope_id="cad-process-authoring:" + audit["packet_sha256"][:12],
            focus_iri=audit["project_abox_focus"],
            focus_type_iri=audit["focus_type"],
            query_asset_id=audit["query_asset_id"],
            shape_asset_id=audit["shape_asset_id"],
        )
        if native_result["delta_sha256"] != delta_sha256:
            raise ValueError("native Semantica delta identity differs from proposed state")
        review = native_result["review"]
        output = {
            "record_type": "ontology-engineering.mfg-q21-authoring-review/v1",
            "authority": "semantica.ontology.DecisionReviewRunner.evaluate_manifest",
            "review_scope": "proposed_package_authoring_only",
            "project_review_receipt": None,
            "engineering_verdict": "not_assessed",
            "delta_sha256": delta_sha256,
            "delta_file_sha256": _digest(raw_delta),
            "projection_audit_sha256": _digest(audit_path.read_bytes()),
            "projection_sha256": _digest(view),
            "semantica_review": review,
        }
        content = (json.dumps(output, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8")
        output_dir = args.output_dir.resolve()
        output_dir.mkdir(parents=True, exist_ok=True)
        path = output_dir / f"q21-authoring-review-{_digest(view)[:12]}-{delta_sha256[:12]}-{_digest(content)[:8]}.json"
        temporary = path.with_name(path.name + f".tmp.{os.getpid()}")
        try:
            temporary.write_bytes(content)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)
    except (OSError, ValueError, KeyError, TypeError, RuntimeError) as exc:
        print(json.dumps({"status": "blocked", "error": str(exc)}, ensure_ascii=False))
        return 2
    print(json.dumps({"status": review["status"], "scope": "authoring_only", "report_path": str(path), "finding_count": len(review["findings"]), "violation_count": len(review["violations"])}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
