#!/usr/bin/env python3
"""Read current operational CAD routes only; no ontology engine or CAD execution."""
import argparse
import json
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[1]
def capabilities(tool_id=None):
    policy=json.loads((ROOT/'data/execution-policy.json').read_text())
    rows=[]
    for entry in policy['records']:
        if tool_id is not None and entry['tool_id'] != tool_id: continue
        for field in ('entrypoint','download_entrypoint'):
            if field in entry:
                target=(ROOT/entry[field]).resolve()
                if not target.is_relative_to(ROOT) or not target.is_file(): raise ValueError('operational entrypoint missing or outside canonical root')
        rows.append(entry)
    if tool_id is not None and not rows: raise ValueError('No current route for '+tool_id+'; legacy catalogs are historical, not automatic fallbacks')
    return {'record_type':'cad-agent.operational-tool-inventory/v2','semantic_execution':'not_run','cad_execution':'not_run','default_cad':policy['default_cad'],'default_supplier':policy['default_supplier'],'records':rows}
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['capabilities']);p.add_argument('tool_id',nargs='?');a=p.parse_args()
    try: print(json.dumps(capabilities(a.tool_id),ensure_ascii=False,indent=2));return 0
    except (OSError,ValueError,KeyError) as exc: print(str(exc),file=sys.stderr);return 2
if __name__=='__main__': raise SystemExit(main())
