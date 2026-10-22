#!/usr/bin/env python3
"""Explicitly resume a verified task through the selected native memory runtime.

The caller selects the task context, provider, session and current Project.
This transport does not classify prompts, change queries, rank memories, activate
a session, update a Project, or grant adoption. Native Rust owns those contracts.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time
import uuid

from common import digest, file_sha, read_input, root_path, run_process, save, selected_program


def selected_runtime(root, home=None):
    root = root_path(root)
    program = selected_program(root, 'engineering-memory')
    memory_home = Path(home) if home is not None else root / 'var/memory'
    if not memory_home.is_absolute():
        raise ValueError('Memory home must be explicitly absolute')
    return {'runtime_binary': program['path'], 'runtime_sha256': program['sha256'],
            'runtime_home': str(memory_home.resolve())}


def resume(*, root=None, home=None, project, context_id, context_sha256, provider, session_id,
           output_root, max_seconds=90, max_output_bytes=8 * 1024 * 1024):
    """Reprepare the exact task after native source, scope and freshness checks."""
    directory = Path(output_root).resolve() / str(uuid.uuid4())
    directory.mkdir(parents=True, exist_ok=False, mode=0o700)
    receipt = {
        'schema': 'six-tools.memory-resume/1', 'run_directory': str(directory),
        'source_context': {'id': context_id, 'sha256': context_sha256},
        'provider': provider, 'session_id': session_id,
        'prepare_attempted': False, 'request_prepared': False, 'fresh_context_verified': False,
        'project_write': False, 'activation_requested': False,
        'engineering_adoption': 'not_granted', 'execution': 'incomplete',
        'limits': {'seconds': max_seconds, 'output_bytes_per_process': max_output_bytes},
    }
    code = 2
    deadline = time.monotonic() + max_seconds
    try:
        if not (0 < max_seconds <= 90 and 0 < max_output_bytes <= 8 * 1024 * 1024):
            raise ValueError('Limits must be positive and at most 90 seconds / 8 MiB')
        root, project = root_path(root), Path(project).resolve()
        receipt['root'] = str(root)
        raw, current = read_input(project)
        anchor = {'path': str(project), 'sha256': digest(raw),
                  'project_id': current['project_id'], 'revision': current['revision']}
        receipt['project'] = anchor
        native = selected_runtime(root, home)
        receipt['native'] = native
        executable = Path(native['runtime_binary'])
        if not executable.is_absolute() or not Path(native['runtime_home']).is_absolute():
            raise ValueError('Runtime and home must be explicitly absolute')

        def stable():
            if selected_runtime(root, home) != native:
                raise ValueError('Selected memory runtime or home changed')
            if file_sha(executable) != native['runtime_sha256']:
                raise ValueError('Selected memory executable digest changed')
            if file_sha(project) != anchor['sha256']:
                raise ValueError('Current Project changed; explicitly rebind the new revision')

        def call(name, args):
            stable()
            result = run_process([str(executable), '--home', native['runtime_home'],
                                  'session', *args], directory, name, deadline, max_output_bytes)
            if result['failure'] or not result['process_group_absent']:
                raise RuntimeError(name + ': ' + (result['failure'] or 'process_group_not_reaped'))
            _, value = read_input(directory / (name + '.stdout'))
            if result['exit_code'] != 0 or value.get('ok') is not True:
                raise ValueError('Native ' + name + ' rejected; see retained stdout/stderr')
            stable()
            return value['data']

        original = call('source-get', ['get', '--id', context_id])
        if original['context_sha256'] != context_sha256:
            raise ValueError('Source context digest differs from supplied identity')
        request = original['session']['request']
        if request['provider'] != provider or request['session_id'] != session_id:
            raise ValueError('Source context belongs to a different provider/session')
        bound = request['project']
        if not bound or any(bound.get(key) != value for key, value in anchor.items()):
            raise ValueError('Source context differs from the explicitly selected current Project')
        verified = call('source-verify', ['verify', '--id', context_id])
        if verified['context_sha256'] != context_sha256 or \
                verified['source_and_project_binding'] != 'matched':
            raise ValueError('Native source context verification identity differs')

        # Only transport identity and phase change. The native request retains
        # task, query, Project/object, conditions, cross-project policy and budgets.
        fresh = dict(request, id='resume-' + str(uuid.uuid4()), phase='resume')
        save(directory / 'request.json', fresh)
        receipt['task'] = request['task']
        receipt['query'] = request['query']
        receipt['request_sha256'] = file_sha(directory / 'request.json')
        receipt['proposed_context_id'] = fresh['id']
        receipt['prepare_attempted'] = True
        prepared = call('prepare', ['prepare', '--input', str(directory / 'request.json')])
        receipt['request_prepared'] = True
        receipt['context'] = {'id': fresh['id'], 'sha256': prepared['context_sha256']}
        if prepared['session']['request'] != fresh:
            raise ValueError('Native prepared request differs from the transported task')
        checked = call('fresh-verify', ['verify', '--id', fresh['id']])
        if checked['context_sha256'] != prepared['context_sha256'] or \
                checked['source_and_project_binding'] != 'matched':
            raise ValueError('Native fresh context verification identity differs')
        stable()
        if file_sha(directory / 'request.json') != receipt['request_sha256']:
            raise ValueError('Transported request changed during execution')
        receipt['fresh_context_verified'] = True
        receipt['candidate_ids'] = [item['id'] for item in
                                    prepared['session']['memory_context']['candidates']]
        receipt['excluded'] = prepared['session']['excluded']
        receipt['execution'] = 'verified_snapshot'
        code = 0
    except (OSError, ValueError, KeyError, TypeError, RuntimeError, subprocess.SubprocessError) as error:
        receipt['execution'] = 'rejected_or_incomplete'
        receipt['error'] = str(error)
        save(directory / 'failure.json', {'error': str(error), 'engineering_adoption': 'not_granted'})
    receipt['exit_code'] = code
    save(directory / 'receipt.json', receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path,
                        help='Absolute installed checkout; defaults to ONTOLOGY_ENGINEERING_ROOT')
    parser.add_argument('--home', type=Path,
                        help='Absolute memory store; defaults to ROOT/var/memory')
    parser.add_argument('--project', type=Path, required=True)
    parser.add_argument('--context-id', required=True)
    parser.add_argument('--context-sha256', required=True)
    parser.add_argument('--provider', required=True, choices=['codex', 'claude'])
    parser.add_argument('--session-id', required=True)
    parser.add_argument('--output-root', type=Path, required=True)
    parser.add_argument('--max-seconds', type=float, default=90)
    parser.add_argument('--max-output-bytes', type=int, default=8 * 1024 * 1024)
    args = parser.parse_args()
    result = resume(**vars(args))
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return result['exit_code']


if __name__ == '__main__':
    raise SystemExit(main())
