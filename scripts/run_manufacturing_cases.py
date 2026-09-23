#!/usr/bin/env python3
"""Replay bundled manufacturing fixtures using only source-locked Semantica."""

import argparse
from contextlib import redirect_stdout
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from semantic_bundle_transport import load_bundle, materialized


def main(bundle_name="manufacturing-process-cost"):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--list", action="store_true", help="Check transport and list declared scenarios; no semantic execution.")
    mode.add_argument("--run", action="store_true", help="Execute and verify native synthetic cases.")
    parser.add_argument("--scenario", action="append", help="Exact declared scenario ID; repeatable. Default: all.")
    parser.add_argument("--output", type=Path, help="New output directory; existing paths are never overwritten.")
    args = parser.parse_args()
    spec, payload, report = load_bundle(bundle_name)
    if args.list:
        if args.scenario or args.output:
            parser.error("--list takes neither --scenario nor --output")
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    if args.output is None:
        parser.error("--run requires --output")
    selected = args.scenario or report["scenarios"]
    if len(set(selected)) != len(selected) or set(selected) - set(report["scenarios"]):
        parser.error("unknown or repeated scenario ID")
    output = args.output.expanduser().resolve()
    if output.exists():
        parser.error("output already exists; choose a new directory")

    from ontology_engineering.semantica_runtime import (
        create_package_runner, read_runtime_source_lock, verify_runtime_source_identity,
    )
    verify_runtime_source_identity()
    source = read_runtime_source_lock()
    output.mkdir(parents=True)
    runner = create_package_runner()
    rows = []
    with materialized(spec, payload) as manifest:
        for sid in selected:
            with redirect_stdout(sys.stderr):
                result = runner.run_manifest(manifest, sid, runtime_commit=source.commit,
                                             runtime_artifact_sha256=source.artifact_sha256,
                                             runtime_version=source.version)
                execution = result.as_dict()
                verification = runner.verify(result).as_dict()
            evidence = json.dumps({"execution": execution, "verification": verification}, ensure_ascii=False, indent=2) + "\n"
            file = output / (sid + ".json")
            file.write_text(evidence, encoding="utf-8")
            failed = [x for x in execution.get("oracle_checks", []) if x.get("status") != "passed"]
            passed = execution.get("status") == "passed" and verification.get("status") == "complete" and not failed
            rows.append({"scenario": sid, "passed": passed, "execution": execution.get("status"),
                         "verification": verification.get("status"), "failed_oracles": failed,
                         "receipt": file.name, "sha256": hashlib.sha256(evidence.encode()).hexdigest()})
    summary = {"passed": all(x["passed"] for x in rows), "executed": len(rows),
               "bundle": report, "runtime": spec["runtime"], "results": rows,
               "scope": "Native fixture replay only. No project validation, registry transition, promotion or publication.",
               "learning": "no_delta; unchanged frozen Semantica inputs replayed in this environment"}
    (output / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": summary["passed"], "executed": len(rows), "output": str(output)}, ensure_ascii=False))
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
