#!/usr/bin/env python3
"""Review an exact project assertion; adoption requires a separate authorization."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ontology_engineering.judgment_admission import review_admission
from ontology_engineering.judgment_admission import subject_identity
from ontology_engineering.judgment_contracts import digest, source_selection
from ontology_engineering.jev_transport import strict_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("request", "evidence-root", "binding", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--workspace", type=Path)
    parser.add_argument("--actor")
    parser.add_argument("--subject-only", action="store_true", help="Export the exact review subject and hashes without semantic execution or adoption.")
    parser.add_argument("--authorization", type=Path, help="Exact caller-controlled adoption authorization; omit for review only.")
    args = parser.parse_args()
    if args.subject_only:
        if args.authorization:
            parser.error("subject preparation cannot use an adoption authorization")
        request = strict_json(args.request.read_bytes())
        source_selection(request["source"], args.evidence_root)
        subject = subject_identity(request, digest(args.binding.read_bytes()))
        result = {"subject": subject, "subject_sha256": digest(subject),
                  "assertion_sha256": digest(request["assertion"]), "source_sha256": digest(request["source"]),
                  "semantic_execution": "not_run", "fact_admitted": False}
        with args.output.open("x") as stream:
            stream.write(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    if not (args.workspace and args.actor):
        parser.error("review or adoption requires workspace and actor")
    result = review_admission(args.request, args.evidence_root, args.binding, args.workspace, args.actor,
                              args.output, authorization=args.authorization)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["status"] != "blocked" else 2


if __name__ == "__main__":
    raise SystemExit(main())
