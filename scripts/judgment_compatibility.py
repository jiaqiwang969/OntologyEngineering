#!/usr/bin/env python3
"""Freeze observed deployment combinations or check their exact shadow scope."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
from ontology_engineering.judgment_contracts import prepare_batch
from ontology_engineering.judgment_compatibility import check_combination, record_combination
from ontology_engineering.jev_transport import strict_json
from ontology_engineering.semantica_runtime import verify_runtime_source_identity


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="command",required=True)
    for name in ("record","check"):
        cmd=sub.add_parser(name)
        for field in ("input","evidence-root","lock","output"):
            cmd.add_argument("--"+field,type=Path,required=True)
        if name=="record":
            cmd.add_argument("--journal",type=Path,required=True)
        else:
            cmd.add_argument("--catalog",type=Path,required=True)
            cmd.add_argument("--catalog-root",type=Path,required=True)
    args=parser.parse_args(argv)
    if args.output.exists():
        parser.error("output exists; choose a new path")
    verify_runtime_source_identity()
    prepared=prepare_batch(strict_json(args.input.read_bytes()),args.evidence_root,strict_json(args.lock.read_bytes()))
    if args.command=="record":
        result=record_combination(prepared,args.evidence_root,args.journal,args.output)
        print(json.dumps({"entry":str(args.output/"entry.json"),"deployment_sha256":result["deployment_sha256"],
                          "observed":result["observed"],"model_quality_qualification":"not_established"}))
        return 0
    result=check_combination(prepared,strict_json(args.catalog.read_bytes()),args.catalog_root)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open("x") as stream:
        json.dump(result,stream,ensure_ascii=False,indent=2,allow_nan=False);stream.write("\n")
    print(json.dumps(result,ensure_ascii=False))
    return 0 if result["status"]=="supported_shadow" else 1


if __name__=="__main__":
    try:
        raise SystemExit(main())
    except (ValueError,OSError) as exc:
        print(json.dumps({"status":"error","error_type":type(exc).__name__}),file=sys.stderr)
        raise SystemExit(2)
