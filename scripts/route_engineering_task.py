#!/usr/bin/env python3
"""Run the default Jev context-routing step; the agent then consumes its candidates."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from ontology_engineering.context_routing import prepare, run
from ontology_engineering.jev_transport import strict_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New private directory outside the skill")
    parser.add_argument("--credential-file", type=Path)
    args = parser.parse_args()
    try:
        prepared = prepare(strict_json(args.input.read_bytes()))
        result = run(prepared, args.output, credential_file=args.credential_file)
    except (ValueError, OSError, KeyError, TypeError):
        print(json.dumps({"status": "input_or_output_error", "model_execution": "not_confirmed",
                          "next_step": "inspect_input_shape_and_select_new_output_directory"}))
        return 2
    print(json.dumps({k: result[k] for k in ("task_id", "status", "model", "candidate_routes",
                                            "uncertain_routes", "attempts", "transport_errors")}, ensure_ascii=False))
    return 0 if result["status"] == "completed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
