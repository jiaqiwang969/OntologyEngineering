#!/usr/bin/env python3
"""List locked method profiles or project source-bound facts for Semantica review."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ontology_engineering.method_evidence import profiles, project_record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list")
    p = commands.add_parser("project")
    p.add_argument("--record", required=True, type=Path)
    p.add_argument("--evidence-root", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        if args.command == "list":
            _, value = profiles()
            print(json.dumps(value, ensure_ascii=False, indent=2))
            return 0
        rdf, audit = project_record(args.record, args.evidence_root)
        args.output.mkdir(parents=True, exist_ok=False)
        (args.output / "evidence.ttl").write_bytes(rdf)
        (args.output / "projection.json").write_text(json.dumps(audit, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps({"source_integrity": audit["source_integrity"], "semantic_execution": "not_run", "projection_sha256": audit["projection_sha256"], "output": str(args.output)}, ensure_ascii=False))
        return 0
    except (ValueError, OSError, KeyError, TypeError) as exc:
        print(json.dumps({"status": "blocked", "reason": str(exc)}, ensure_ascii=False))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
