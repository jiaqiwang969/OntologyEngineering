#!/usr/bin/env python3
"""A0-A8 v3 native-CAD ledger gate; explicit v2 compatibility, no CAD calls.

The v2 checker is preserved unchanged to retain all topology, inventory, geometry,
placement, joint, evidence and reconciliation checks. V3 maps names explicitly and
adds NX file/session persistence obligations. This is not a semantic truth engine.
"""
from __future__ import annotations
import argparse
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re
import authoring_v2_compat as legacy
from authoring_v2_compat import REQUIRED_CATEGORIES, sha256

SCHEMA = 'cad-agent.assembly-authoring-manifest/v3'
legacy.CARRIER_CLASSES = legacy.CARRIER_CLASSES | {'PRT'}
KEYS = {
    'native_target': 'fusion_target', 'native_occurrence_id': 'fusion_occurrence_id',
    'native_definition_id': 'fusion_definition_id', 'native_occurrence_ids': 'fusion_occurrence_ids',
    'host_native_occurrence_ids': 'host_fusion_occurrence_ids',
}
ENUMS = {
    'PERSISTED_NATIVE_ASSEMBLY': 'PERSISTED_FUSION_ASSEMBLY',
    'NATIVE_READBACK': 'FUSION_READBACK', 'NATIVE_ROOT_COMPONENT': 'FUSION_ROOT_COMPONENT',
    'NATIVE_ONLY_AUTHORIZED_ADDITION': 'FUSION_ONLY_AUTHORIZED_ADDITION',
    'NATIVE_ONLY_UNRESOLVED': 'FUSION_ONLY_UNRESOLVED',
}

def remap(value, keys, enums):
    if isinstance(value, dict):
        out = {}
        for key, item in value.items():
            mapped = keys.get(key, key)
            if mapped in out: raise ValueError('mixed legacy/native field identity: '+mapped)
            out[mapped] = remap(item, keys, enums)
        return out
    if isinstance(value, list): return [remap(x, keys, enums) for x in value]
    return enums.get(value, value) if isinstance(value, str) else value

def native_template(document):
    """Mechanical name migration only; never synthesizes native evidence."""
    out = remap(deepcopy(document), {v:k for k,v in KEYS.items()}, {v:k for k,v in ENUMS.items()})
    out['$schema'] = SCHEMA
    target = out['native_target']
    target['cad_system'] = 'SIEMENS_NX'
    target['project_id'] = target.pop('destination_project')
    target['output_directory'] = target.pop('destination_folder')
    target['root_model_path'] = 'UNKNOWN'
    out['native_readback']['native_session_id'] = 'UNKNOWN'
    out['native_readback']['second_readback']['native_session_id'] = 'UNKNOWN'
    out['native_readback']['persisted_files'] = []
    return out

def validate_document(document, manifest_path, check_files=False):
    if isinstance(document, dict) and document.get('$schema') == legacy.SCHEMA:
        result, code = legacy.validate_document(document, manifest_path, check_files)
        result['schema_mode'] = 'legacy_v2_read_only_compatibility'
        return result, code
    errors, holds = [], []
    try:
        if not isinstance(document, dict) or document.get('$schema') != SCHEMA:
            raise ValueError('expected explicit v3 native or historical v2 schema')
        def reject_legacy_fields(value):
            if isinstance(value,dict):
                if set(value) & set(KEYS.values()): raise ValueError('v3 contains legacy Fusion field names')
                for item in value.values(): reject_legacy_fields(item)
            elif isinstance(value,list):
                for item in value: reject_legacy_fields(item)
            elif isinstance(value,str) and value in ENUMS.values():
                raise ValueError('v3 contains a legacy Fusion enum; translate the claim explicitly')
        reject_legacy_fields(document)
        mapped = remap(document, KEYS, ENUMS)
        mapped['$schema'] = legacy.SCHEMA
        target = mapped.get('fusion_target', {})
        if not isinstance(target, dict): raise ValueError('native_target must be an object')
        if target.get('cad_system') != 'SIEMENS_NX': errors.append('native_target.cad_system: v3 currently supports SIEMENS_NX')
        for old, new in [('destination_project','project_id'),('destination_folder','output_directory')]:
            if old in target: errors.append('native_target: legacy destination fields are not valid v3 fields')
            target[old] = target.get(new)
        result, code = legacy.validate_document(mapped, Path(manifest_path), check_files)
        def render(value):
            if isinstance(value,str): return value.replace('FUSION','NATIVE').replace('Fusion','native CAD').replace('fusion','native')
            if isinstance(value,list): return [render(x) for x in value]
            if isinstance(value,dict): return {k:render(v) for k,v in value.items()}
            return value
        result = render(result)
        if 'PERSISTED_NATIVE_ASSEMBLY' in document.get('target_claims', []):
            read = document.get('native_readback', {})
            second = read.get('second_readback', {})
            first_id, second_id = read.get('native_session_id'), second.get('native_session_id')
            for label, value in [('first',first_id),('second',second_id)]:
                if not isinstance(value,str) or legacy.placeholder(value): holds.append(label+' native session identity not established')
            if first_id == second_id: errors.append('persistence requires a different fresh native session')
            rows = read.get('persisted_files')
            if not isinstance(rows,list) or not rows: holds.append('native_readback.persisted_files: PRT closure missing'); rows=[]
            seen, roots = set(), []
            root_path = document['native_target'].get('root_model_path')
            for row in rows:
                if not isinstance(row,dict): errors.append('persisted file must be an object'); continue
                path, expected = row.get('path'), row.get('sha256')
                if not isinstance(path,str) or Path(path).suffix.lower() != '.prt' or not isinstance(expected,str) or not re.fullmatch('[0-9a-f]{64}', expected):
                    errors.append('persisted file requires a .prt path and SHA-256'); continue
                resolved = (Path(manifest_path).resolve().parent / path).resolve()
                if resolved in seen: errors.append('duplicate persisted file identity')
                seen.add(resolved)
                if path == root_path: roots.append(row)
                if check_files:
                    if not resolved.is_file() or hashlib.sha256(resolved.read_bytes()).hexdigest() != expected: errors.append('persisted file missing or hash mismatch: '+path)
            if len(roots) != 1: errors.append('root_model_path must bind exactly one persisted PRT')
            elif read.get('data_file_identity') != 'sha256:'+roots[0]['sha256']: errors.append('data_file_identity must equal root PRT sha256 identity')
            if not check_files: holds.append('NX persisted-file hashes not checked')
        result['errors'] = sorted(set(result['errors']+errors))
        result['holds'] = sorted(set(result['holds']+holds))
        result['schema_mode'] = 'native_v3'
        result['cad_system'] = 'SIEMENS_NX'
        result['claim_boundary'] = 'Ledger, file-integrity and evidence-reference checks only. Does not execute NX, evaluate formal semantics or prove referenced observations or physical performance.'
        if result['errors']: result['status'], code = 'FAIL',1
        elif result['holds']: result['status'], code = 'HOLD',2
        return result, code
    except (TypeError, ValueError, KeyError, AttributeError, OSError) as exc:
        return {'status':'FAIL','errors':[str(exc)],'holds':[],'schema_mode':'invalid','claim_boundary':'No engineering acceptance'},1

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('manifest',type=Path); p.add_argument('--check-files',action='store_true')
    a=p.parse_args()
    try: document=json.loads(a.manifest.read_text())
    except (OSError,ValueError) as exc:
        print(json.dumps({'status':'FAIL','errors':[str(exc)]})); return 1
    result,code=validate_document(document,a.manifest,a.check_files)
    print(json.dumps(result,ensure_ascii=False,indent=2)); return code
if __name__=='__main__': raise SystemExit(main())
