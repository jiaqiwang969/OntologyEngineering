#!/usr/bin/env python3
"""Source-bound Jev batch annotations. All outputs remain non-authoritative candidates."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))

from ontology_engineering.judgment_contracts import contracts,deployment_lock,prepare_batch
from ontology_engineering.judgment_batch import execute,journal,report
from ontology_engineering.jev_transport import JevTransport,strict_json


def write_new(path,value):
    path=Path(path)
    path.parent.mkdir(parents=True,exist_ok=True)
    with path.open("x",encoding="utf-8") as stream:
        stream.write(json.dumps(value,ensure_ascii=False,indent=2,allow_nan=False)+"\n")


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="command",required=True)
    sub.add_parser("catalog",help="Verify and list Semantica-owned question and pattern identities.")
    init=sub.add_parser("init-lock",help="Create an experimental configuration; does not grant project adoption.")
    init.add_argument("--model",required=True)
    init.add_argument("--strategy",choices=("batched","single"),default="batched")
    init.add_argument("--workers",type=int,default=2)
    init.add_argument("--max-attempts",type=int,default=3)
    init.add_argument("--max-state-chars",type=int,default=24000)
    init.add_argument("--output",type=Path,required=True)
    for name in ("prepare","run","status"):
        cmd=sub.add_parser(name)
        cmd.add_argument("--input",type=Path,required=True)
        cmd.add_argument("--evidence-root",type=Path,required=True)
        cmd.add_argument("--lock",type=Path,required=True)
        cmd.add_argument("--output",type=Path,required=True)
        if name!="prepare":
            cmd.add_argument("--journal",type=Path,required=True)
        if name=="run":
            cmd.add_argument("--credential-file",type=Path,default=Path.home()/".codex/api-jev.md")
            route=cmd.add_mutually_exclusive_group(required=True)
            route.add_argument("--experiment",action="store_true",help="Explicit unqualified shadow experiment.")
            route.add_argument("--compatibility",type=Path,help="Allowlist of complete observed combinations.")
            cmd.add_argument("--compatibility-root",type=Path)
    args=parser.parse_args(argv)
    if args.command=="catalog":
        value=contracts()
        print(json.dumps({"identity":value["identity"],"patterns":value["patterns"],"questions":list(value["catalog"]["questions"]),"allowed_use":"candidate_only"},ensure_ascii=False))
        return 0
    if args.output.exists():
        parser.error("output exists; select a new report path")
    if args.command=="init-lock":
        value=deployment_lock(model=args.model,strategy=args.strategy,workers=args.workers,max_attempts=args.max_attempts,max_state_chars=args.max_state_chars)
        write_new(args.output,value)
        print(json.dumps({"configuration":str(args.output),"mode":value["mode"]}))
        return 0
    from ontology_engineering.semantica_runtime import verify_runtime_source_identity
    verify_runtime_source_identity()
    prepared=prepare_batch(strict_json(args.input.read_bytes()),args.evidence_root,strict_json(args.lock.read_bytes()))
    if args.command=="prepare":
        value={"input_sha256":prepared["input_sha256"],"deployment_sha256":prepared["deployment_sha256"],
               "items":len(prepared["items"]),"questions":sum(len(i["question_ids"]) for i in prepared["items"]),
               "source_integrity":"verified","model_execution":"not_run","fact_admission":"not_performed"}
    elif args.command=="run":
        if args.compatibility and not args.compatibility_root:
            parser.error("--compatibility requires --compatibility-root")
        if args.experiment and args.compatibility_root:
            parser.error("--experiment does not use a compatibility root")
        value=execute(prepared,args.journal,JevTransport(args.credential_file),experiment=args.experiment,
            compatibility_catalog=strict_json(args.compatibility.read_bytes()) if args.compatibility else None,
            compatibility_root=args.compatibility_root)
    else:
        if not args.journal.is_file():
            parser.error("journal missing; status cannot create a run")
        with journal(args.journal,prepared["input"]["project_id"],prepared["input"]["access_scope"]) as db:
            row=db.execute("SELECT input_sha,lock_sha FROM runs WHERE id=?",(prepared["input"]["batch_id"],)).fetchone()
            if not row or tuple(row)!=(prepared["input_sha256"],prepared["deployment_sha256"]):
                parser.error("journal run identity does not match")
            value=report(db,prepared)
    write_new(args.output,value)
    print(json.dumps({k:value[k] for k in ("status","items","counts","expected_questions","live_attempts","replays") if k in value and (k!="items" or isinstance(value[k],int))},ensure_ascii=False))
    return 1 if value.get("status")=="incomplete" else 0


if __name__=="__main__":
    try:
        raise SystemExit(main())
    except (ValueError,OSError) as exc:
        # Local paths and arbitrary exception text are not echoed on failure.
        print(json.dumps({"status":"error","error_type":type(exc).__name__}),file=sys.stderr)
        raise SystemExit(2)
