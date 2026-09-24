#!/usr/bin/env python3
"""Review a source-bound, explicitly non-equivalent pattern migration mapping."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ontology_engineering.judgment_evolution import migration_subject, review_migration
from ontology_engineering.judgment_contracts import digest
from ontology_engineering.jev_transport import strict_json


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode", choices=["subject", "review"])
    for name in ("mapping", "binding", "output"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--evidence-root", type=Path)
    p.add_argument("--workspace", type=Path)
    p.add_argument("--actor")
    a = p.parse_args()
    if a.mode == "subject":
        subject, _ = migration_subject(strict_json(a.mapping.read_bytes()), a.binding)
        result = {"subject": subject, "subject_sha256": digest(subject), "semantic_execution": "not_run"}
        with a.output.open("x") as stream:
            stream.write(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    else:
        if not (a.evidence_root and a.workspace and a.actor):
            p.error("review requires evidence-root, workspace and actor")
        result = review_migration(a.mapping, a.evidence_root, a.binding, a.workspace, a.actor, a.output)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 2 if result.get("status") == "blocked" else 0


if __name__ == "__main__":
    raise SystemExit(main())
