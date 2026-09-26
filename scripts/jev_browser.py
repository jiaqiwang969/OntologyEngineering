#!/usr/bin/env python3
"""Run the optional Jev browser tool; doctor never connects to Chrome."""
import argparse
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("doctor")
    runner = commands.add_parser("run")
    runner.add_argument("--task", type=Path, required=True)
    runner.add_argument("--output", type=Path, required=True)
    runner.add_argument("--credential-file", type=Path)
    args = parser.parse_args()
    # Use only the separate optional environment, leaving Semantica's venv intact.
    venv = ROOT / "runtime/jev-ultrafast/.venv"
    python = venv / "bin/python"
    if python.is_file() and Path(sys.prefix).resolve() != venv.resolve():
        os.execv(str(python), [str(python), str(Path(__file__).resolve()), *sys.argv[1:]])
    from ontology_engineering import jev_browser
    try:
        if args.command == "doctor":
            result = jev_browser.doctor()
        else:
            result = jev_browser.run(json.loads(args.task.read_text()), args.output,
                                     credential_file=args.credential_file)
            result = {k: result[k] for k in ("status", "verification", "engineering_acceptance", "actions", "decisions") if k in result}
            result["output"] = str(args.output)
    except Exception as error:
        print(json.dumps({"status": "not_started", "error_type": type(error).__name__}))
        return 1
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["status"] in {"local_runtime_ready", "reported_done"} else 1


if __name__ == "__main__":
    raise SystemExit(main())

