#!/usr/bin/env python3
"""Verify original bytes, then delegate Project validation/reconciliation to Rust.

This transport preserves its inputs and writes only a fresh private run directory.
It neither interprets tool output nor changes conclusions or engineering adoption.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time
import uuid

from common import file_sha, digest, read_bytes, read_input, root_path, run_process, save, selected_program


def check(root, before, candidate, sources, *, max_seconds=90, max_output_bytes=8 * 1024 * 1024):
    root = root_path(root)
    before, candidate = Path(before).resolve(), Path(candidate).resolve()
    run = root / 'var/investigations/tool-binding-checks' / str(uuid.uuid4())
    run.mkdir(parents=True, exist_ok=False, mode=0o700)
    receipt = {'schema': 'six-tools.binding-check/1', 'run_directory': str(run),
               'binding_checks_passed': False, 'engineering_adoption': 'not_granted',
               'native_results': [], 'inputs_stable': False,
               'limits': {'seconds': max_seconds, 'output_bytes_per_process': max_output_bytes},
               'coverage': 'Source bytes, native Project validation and declared change dependencies; not semantic correctness of tool output'}
    deadline = time.monotonic() + max_seconds
    try:
        if not (0 < max_seconds <= 90 and 0 < max_output_bytes <= 8 * 1024 * 1024):
            raise ValueError('Limits must be positive and at most 90 seconds / 8 MiB')
        cli = selected_program(root, 'oev2')
        receipt['native_cli'] = cli
        original_before, original_candidate = read_bytes(before), read_bytes(candidate)
        (run / 'before.json').write_bytes(original_before)
        (run / 'candidate.json').write_bytes(original_candidate)
        project = json.loads(original_candidate)
        source_bytes = {}
        for key, expected in project['evidence'].items():
            if key not in sources:
                raise ValueError('No original file supplied for evidence: ' + key)
            path = Path(sources[key])
            if not path.is_absolute():
                raise ValueError('Source paths must be absolute: ' + key)
            if file_sha(path) != expected:
                raise ValueError('Evidence bytes changed: ' + key)
            source_bytes[key] = (path, expected)
        receipt.update(before_path=str(before), before_sha256=digest(original_before),
                       candidate_path=str(candidate), candidate_sha256=digest(original_candidate),
                       sources={k: {'path': str(p), 'sha256': h} for k, (p, h) in source_bytes.items()})
        for name, args in [
            ('validate-before', ['project', 'validate', '--input', str(run / 'before.json')]),
            ('validate-candidate', ['project', 'validate', '--input', str(run / 'candidate.json')]),
            ('reconcile', ['project', 'reconcile', '--before', str(run / 'before.json'),
                           '--after', str(run / 'candidate.json'), '--output', str(run / 'impact.json')])]:
            result = run_process([cli['path'], '--root', str(root), *args], run, name,
                                 deadline, max_output_bytes)
            receipt['native_results'].append({'command': name, **result})
            if result['failure'] or not result['process_group_absent']:
                raise RuntimeError(name + ': ' + (result['failure'] or 'process_group_not_reaped'))
            if result['exit_code'] != 0:
                break
        if read_bytes(before) != original_before or read_bytes(candidate) != original_candidate:
            raise ValueError('Project changed during check')
        for key, (path, expected) in source_bytes.items():
            if file_sha(path) != expected:
                raise ValueError('Evidence changed during check: ' + key)
        if selected_program(root, 'oev2') != cli:
            raise ValueError('Selected native executable changed during check')
        # Source verification can take time; recheck the shared Project after it.
        if read_bytes(before) != original_before or read_bytes(candidate) != original_candidate:
            raise ValueError('Project changed during final check')
        receipt['inputs_stable'] = True
        receipt['binding_checks_passed'] = len(receipt['native_results']) == 3 and all(
            result['exit_code'] == 0 for result in receipt['native_results'])
        receipt['exit_code'] = 0 if receipt['binding_checks_passed'] else 1
    except (OSError, ValueError, KeyError, TypeError, RuntimeError, subprocess.SubprocessError) as error:
        receipt['error'] = str(error)
        receipt['exit_code'] = 2
        save(run / 'failure.json', {'error': str(error), 'engineering_adoption': 'not_granted'})
    save(run / 'receipt.json', receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, help='Absolute checkout; otherwise ONTOLOGY_ENGINEERING_ROOT')
    for name in ['before', 'candidate', 'sources']:
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--max-seconds', type=float, default=90)
    parser.add_argument('--max-output-bytes', type=int, default=8 * 1024 * 1024)
    args = parser.parse_args()
    _, sources = read_input(args.sources)
    result = check(args.root, args.before, args.candidate, sources,
                   max_seconds=args.max_seconds, max_output_bytes=args.max_output_bytes)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return result['exit_code']


if __name__ == '__main__':
    raise SystemExit(main())
