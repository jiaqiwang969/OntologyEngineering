#!/usr/bin/env python3
"""Run native review against an explicit current Project, or replay history.

This adapter binds bytes and supervises owned processes. Native v2 and memory
retain all review semantics. It writes only a new private invocation directory.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time
import uuid

from common import (file_sha, root_path, selected_program, save, read_bytes, read_input,
                    digest, run_process, project_data_root, project_file, cli_prefix)


def selected_cli(root):
    return selected_program(root, 'oev2')


def execute(root, request, *, project=None, historical=False, memory_context=None,
            memory_sha=None, memory_home=None, project_home=None,
            max_seconds=90, max_output_bytes=8 * 1024 * 1024):
    root = root_path(root)
    data_root = project_data_root(root, project_home)
    request = project_file(data_root, request) if project_home is not None else Path(request).resolve()
    directory = data_root / ('runs/tool-reviewed-reconciliation' if project_home is not None
                            else 'var/investigations/tool-reviewed-reconciliation') / str(uuid.uuid4())
    if project_home is not None:
        directory = project_file(data_root, directory)
    directory.mkdir(parents=True, exist_ok=False, mode=0o700)
    receipt = {'schema':'six-tools.review-invocation/2', 'run_directory':str(directory),
               'installation_root':str(root),
               'project_home':str(data_root) if project_home is not None else None,
               'mode':'historical' if historical else 'current',
               'current_applicability':'not_verified', 'engineering_adoption':'not_granted',
               'request_path':str(request), 'native_started':False, 'inputs_stable':False,
               'limits':{'seconds':max_seconds, 'output_bytes_per_process':max_output_bytes}}
    deadline = time.monotonic() + max_seconds
    code = 2
    try:
        if not (0 < max_seconds <= 90 and 0 < max_output_bytes <= 8 * 1024 * 1024):
            raise ValueError('Limits must be positive and at most 90 seconds / 8 MiB')
        if bool(project) == bool(historical):
            raise ValueError('Select exactly one: --project CURRENT_PROJECT or --historical-snapshot')
        if bool(memory_context) != bool(memory_sha):
            raise ValueError('Memory context ID and SHA256 must be supplied together')
        if historical and memory_context:
            raise ValueError('A current memory binding cannot authorize historical mode')
        raw = read_bytes(request)
        receipt['request_sha256'] = digest(raw)
        (directory / 'request.json').write_bytes(raw)
        wire = json.loads(raw)
        current_ref = wire['current']
        current_path = project_file(data_root, current_ref['path'])
        current_raw, current = read_input(current_path)
        if digest(current_raw) != current_ref['sha256']:
            raise ValueError('Request current Project reference changed')
        receipt['request_project'] = {'path':str(current_path), 'sha256':digest(current_raw),
                                      'project_id':current['project_id'], 'revision':current['revision']}
        anchor = None
        if project:
            anchor = project_file(data_root, project) if project_home is not None else Path(project).resolve()
            anchor_raw, anchor_json = read_input(anchor)
            receipt['project_anchor'] = {'path':str(anchor), 'sha256':digest(anchor_raw),
                                         'project_id':anchor_json['project_id'], 'revision':anchor_json['revision']}
            if anchor_raw != current_raw:
                raise ValueError('Request snapshot does not match the explicitly selected current Project')
        cli = selected_cli(root)
        receipt['native_cli'] = cli
        if file_sha(cli['path']) != cli['sha256']:
            raise ValueError('Selected native executable identity changed')

        def child(name, command):
            result = run_process(list(map(str, command)), directory, name, deadline, max_output_bytes)
            if result['failure'] or not result['process_group_absent']:
                raise RuntimeError(name + ': ' + (result['failure'] or 'process_group_not_reaped'))
            return result

        memory_program = None
        if memory_home and not Path(memory_home).is_absolute():
            raise ValueError('Memory home must be absolute')
        home = Path(memory_home).resolve() if memory_home else root / 'var/memory'

        def memory_call(name, action):
            if selected_program(root, 'engineering-memory') != memory_program:
                raise ValueError('Selected memory executable changed during invocation')
            result = child(name, [memory_program['path'], '--home', str(home), 'session', action,
                                  '--id', memory_context])
            raw_result = json.loads((directory / (name + '.stdout')).read_text())
            if result['exit_code'] != 0 or not raw_result.get('ok'):
                raise ValueError('Memory ' + action + ' rejected; see retained ' + name + '.stdout')
            data = raw_result['data']
            if data['context_sha256'] != memory_sha:
                raise ValueError('Memory context digest differs from supplied identity')
            if selected_program(root, 'engineering-memory') != memory_program:
                raise ValueError('Selected memory executable changed during invocation')
            return data

        if memory_context:
            memory_program = selected_program(root, 'engineering-memory')
            receipt['memory_program'] = memory_program
            receipt['memory_home'] = str(home)
            context = memory_call('memory-get', 'get')
            bound = context['session']['request']['project']
            expected = receipt['project_anchor']
            if not bound or any(bound[k] != expected[k] for k in ('path','sha256','project_id','revision')):
                raise ValueError('Memory context is bound to a different Project snapshot')
            memory_call('memory-verify-before', 'verify')
            receipt['memory'] = {'context_id':memory_context, 'context_sha256':memory_sha,
                                 'project':bound, 'verified_before':True}
        command = [*cli_prefix(cli['path'], root, project_home),'project','reconcile-reviewed','--input',
                   str(directory / 'request.json'),'--output',str(directory / 'native.json')]
        receipt['native_started'] = True
        result = child('native', command)
        receipt['native_exit_code'] = result['exit_code']
        if anchor and file_sha(anchor) != receipt['project_anchor']['sha256']:
            raise ValueError('Current Project changed during invocation')
        if file_sha(request) != receipt['request_sha256']:
            raise ValueError('Request changed during invocation')
        if selected_cli(root) != cli or file_sha(cli['path']) != cli['sha256']:
            raise ValueError('Selected native executable changed during invocation')
        if project_data_root(root, project_home) != data_root:
            raise ValueError('Selected project home changed during invocation')
        if memory_context:
            memory_call('memory-verify-after', 'verify')
            receipt['memory']['verified_after'] = True
        # Memory verification takes time: recheck the shared inputs after it too.
        if (anchor and file_sha(anchor) != receipt['project_anchor']['sha256']) or \
           file_sha(request) != receipt['request_sha256'] or selected_cli(root) != cli or \
           file_sha(cli['path']) != cli['sha256']:
            raise ValueError('Shared binding changed during final verification')
        receipt['inputs_stable'] = True
        receipt['current_applicability'] = 'historical_only' if historical else 'bound_snapshot_only'
        code = result['exit_code'] if result['exit_code'] >= 0 else 2
        receipt['execution'] = 'native_completed'
    except (OSError, ValueError, KeyError, TypeError, RuntimeError, subprocess.SubprocessError) as error:
        receipt['execution'] = 'rejected_or_incomplete'
        receipt['error'] = str(error)
        save(directory / 'failure.json', {'error':str(error), 'engineering_adoption':'not_granted'})
    receipt['exit_code'] = code
    save(directory / 'invocation.json', receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, help='Absolute checkout; otherwise ONTOLOGY_ENGINEERING_ROOT')
    parser.add_argument('--project-home', type=Path, help='Explicit independent ProjectStore; no cwd inference')
    parser.add_argument('--request', required=True, type=Path)
    parser.add_argument('--project', type=Path)
    parser.add_argument('--historical-snapshot', action='store_true')
    parser.add_argument('--memory-context')
    parser.add_argument('--memory-context-sha256')
    parser.add_argument('--memory-home', type=Path)
    parser.add_argument('--max-seconds', type=float, default=90)
    parser.add_argument('--max-output-bytes', type=int, default=8 * 1024 * 1024)
    args = parser.parse_args()
    result = execute(args.root, args.request, project=args.project, historical=args.historical_snapshot,
                     memory_context=args.memory_context, memory_sha=args.memory_context_sha256, memory_home=args.memory_home,
                     project_home=args.project_home,
                     max_seconds=args.max_seconds, max_output_bytes=args.max_output_bytes)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return result['exit_code']


if __name__ == '__main__':
    raise SystemExit(main())
