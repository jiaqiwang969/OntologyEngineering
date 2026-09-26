#!/usr/bin/env python3
"""Read-only local CAD helper readiness, optional direct-NX host probe."""
import argparse
import importlib.util
import json
from pathlib import Path
import sys
from nx_direct import load_profile, remote, checked_json, ps_quote
from semantic_query import capabilities
ROOT=Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--json',action='store_true');p.add_argument('--profile')
    a=p.parse_args()
    checks={'python_3_11':sys.version_info >= (3,11)}
    for name in ('jsonschema','numpy','yaml'): checks[name]=importlib.util.find_spec(name) is not None
    for name in ('scripts/nx_direct.py','assets/nx-direct/worker.py','assets/nx-direct/run-job.ps1','references/misumi-cn-guide.md'):
        checks[name]=(ROOT/name).is_file()
    try:
        policy=json.loads((ROOT/'data/execution-policy.json').read_text())
        catalog=capabilities(None)
        checks['nx_direct_default']=policy['default_cad']=='NXDirect' and policy['cad_mcp_enabled'] is False
        checks['supplier_default']=policy['default_supplier']=='MISUMI_CN'
        checks['catalog']=set(x['tool_id'] for x in catalog['records'])=={'NXDirect','MISUMI_CN'}
        checks['removed_cad_runtime_absent']=('Fusion' in policy.get('removed_cad_systems',[]) and policy.get('legacy_cad_reactivation_allowed') is False and importlib.util.find_spec('fusion_mcp_proxy') is None)
    except (OSError,ValueError,KeyError): checks['catalog']=False
    report={'ready':all(checks.values()),'checks':checks,'cad_execution':'not_run','license':'not_tested','semantic_execution':'not_run','legacy_runtime':'not_required_or_loaded','host_probe':None}
    if a.profile:
        try:
            profile=load_profile(a.profile)
            ps='$exe=[Environment]::ExpandEnvironmentVariables('+ps_quote(profile['run_journal'])+'); @{hostname=$env:COMPUTERNAME; executable_present=(Test-Path -LiteralPath $exe -PathType Leaf)} | ConvertTo-Json'
            report['host_probe']=checked_json(remote(profile,ps))
            report['ready'] &= report['host_probe']['executable_present']
        except Exception as exc:
            report['ready']=False;report['host_probe']={'error':str(exc)}
    print(json.dumps(report,ensure_ascii=False,indent=2) if a.json else ('CAD helpers ready: '+str(report['ready'])+'; native/license/engineering acceptance not assessed.\n'+json.dumps(report,ensure_ascii=False,indent=2)))
    return 0 if report['ready'] else 1
if __name__=='__main__':raise SystemExit(main())
