#!/usr/bin/env python3
"""Review source-bound candidate roles or a declared engineering review plan."""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from ontology_engineering.judgment_contracts import prepare_batch
from ontology_engineering.judgment_review import review_candidates, review_plan, review_impact, execute_record
from ontology_engineering.jev_transport import strict_json
from ontology_engineering.semantica_runtime import verify_runtime_source_identity


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode", choices=("candidates", "plan", "impact", "record"))
    for name in ("input", "evidence-root", "binding", "workspace", "output"):
        p.add_argument("--"+name, type=Path, required=True)
    p.add_argument("--actor", required=True)
    p.add_argument("--lock", type=Path)
    p.add_argument("--journal", type=Path)
    p.add_argument("--bundle", help="For record mode only; an exact bundle declared in the transport lock.")
    a = p.parse_args()
    verify_runtime_source_identity()
    if a.mode != "record" and a.bundle:
        p.error("--bundle is only supported for one source-bound record")
    if a.mode == "candidates":
        if not a.lock or not a.journal:
            p.error("candidate review requires --lock and --journal")
        prepared = prepare_batch(strict_json(a.input.read_bytes()), a.evidence_root, strict_json(a.lock.read_bytes()))
        result = review_candidates(prepared, a.journal, a.binding, a.workspace, a.actor, a.output)
    elif a.mode == "plan":
        if a.lock or a.journal:
            p.error("plan mode does not import model results")
        result = review_plan(a.input, a.evidence_root, a.binding, a.workspace, a.actor, a.output)
    elif a.mode == "impact":
        if a.lock or a.journal:
            p.error("impact mode does not import model results")
        result = review_impact(a.input, a.evidence_root, a.binding, a.workspace, a.actor, a.output)
    else:
        if a.lock or a.journal:
            p.error("record mode does not import model results")
        binding = strict_json(a.binding.read_bytes())
        result = execute_record(a.input, a.evidence_root, a.binding, a.workspace, a.actor, a.output,
            project_id=binding["project"]["project_id"], bundle_name=a.bundle or "engineering-evidence-methods")
    print(json.dumps({"mode":a.mode, "fact_admission":"not_performed", "output":str(a.output)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
