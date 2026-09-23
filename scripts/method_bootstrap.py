#!/usr/bin/env python3
"""Prepare, initialize and separately adopt a frozen Semantica method library."""
import argparse
from contextlib import redirect_stdout
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ontology_engineering import method_bootstrap as bootstrap


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("prepare", help="Write a local reviewable plan; no registry mutation")
    prepare.add_argument("--directory", type=Path, required=True)
    for field in ("project", "domain", "actor", "fact-authority", "evidence-root"):
        prepare.add_argument("--" + field, required=True)
    prepare.add_argument("--bundle", default="engineering-judgment-intake")
    for command in ("apply", "adopt"):
        sub = commands.add_parser(command)
        sub.add_argument("--directory", type=Path, required=True)
        sub.add_argument("--authorization", type=Path, required=True)
    args = parser.parse_args()
    try:
        with redirect_stdout(sys.stderr):
            if args.command == "prepare":
                result = bootstrap.prepare(args.directory, project=args.project, domain=args.domain, actor=args.actor,
                    fact_authority=args.fact_authority, evidence_root=args.evidence_root, name=args.bundle)
            else:
                kwargs = {"progress": lambda value: print(json.dumps(value), file=sys.stderr, flush=True)} if args.command == "apply" else {}
                result = getattr(bootstrap, args.command)(args.directory, args.authorization, **kwargs)
        print(json.dumps(result, ensure_ascii=False, indent=2))
    except Exception as exc:
        print(json.dumps({"status": "blocked", "error_type": type(exc).__name__, "message": str(exc)}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
