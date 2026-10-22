#!/usr/bin/env python3
"""Invoke the shared Rust normalizer and locked official ggen; never render types."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import uuid


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', required=True, type=Path)
    parser.add_argument('--ggen', required=True, type=Path)
    parser.add_argument('--normalizer-root', type=Path,
                        help='Matching source root for a separately installed normalizer')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    contracts = Path(__file__).resolve().parent
    module = contracts.parent
    root = module.parent.parent
    cli, ggen = args.cli.resolve(strict=True), args.ggen.resolve(strict=True)
    normalizer_root = (args.normalizer_root.resolve(strict=True)
                       if args.normalizer_root else root)
    lock = json.loads((root / 'runtime/ggen-lock.json').read_text())
    if sha(ggen) != lock['binary_sha256']:
        raise RuntimeError('ggen_executable_identity_mismatch')
    run = root / 'var/maintenance' / ('memory-contracts-' + uuid.uuid4().hex)
    run.mkdir(parents=True)
    spec = contracts / 'specs/engineering-memory.toml'
    inputs = {str(p.relative_to(module)): sha(p)
              for p in [spec, *sorted((contracts / 'templates').glob('*'))]}

    def execute(argv, cwd, name):
        result = subprocess.run([str(x) for x in argv], cwd=cwd,
                                capture_output=True, timeout=120)
        (run / (name + '.stdout')).write_bytes(result.stdout)
        (run / (name + '.stderr')).write_bytes(result.stderr)
        if result.returncode:
            raise RuntimeError(f'{name}_failed:{result.returncode}; see {run}')

    output = Path('var/maintenance') / run.name / 'normalized'
    normalized = normalizer_root / output
    execute([cli, '--root', normalizer_root, 'design', 'normalize', '--spec', spec,
             '--output', output], root, 'normalize')
    shutil.copyfile(normalized / 'model.ttl', run / 'model.ttl')
    shutil.copyfile(contracts / 'templates/ggen.toml', run / 'ggen.toml')
    shutil.copytree(contracts / 'templates', run / 'templates')
    execute([ggen, 'sync', 'run'], run, 'ggen')
    for name, expected in inputs.items():
        if sha(module / name) != expected:
            raise RuntimeError('generation_input_changed:' + name)
    targets = {
        'contracts.rs': module / 'runtime/src/evidence/contracts.rs',
        'workflow.rs': module / 'runtime/src/generated.rs',
    }
    products = {}
    for name, target in targets.items():
        product = run / 'out' / name
        products[str(target.relative_to(module))] = sha(product)
        if args.check:
            if not target.exists() or target.read_bytes() != product.read_bytes():
                raise RuntimeError('generated_contract_drift:' + str(target))
        else:
            shutil.copyfile(product, target)
    receipt = dict(schema='engineering-memory.generation/2', inputs=inputs,
                   normalizer_sha256=sha(cli), ggen_sha256=sha(ggen),
                   ggen_commit=lock['upstream_commit'], products=products,
                   mode='check' if args.check else 'generate',
                   scope='actual Rust normalization and official ggen; no semantic adoption')
    (run / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'status': 'matched' if args.check else 'generated',
                      'receipt': str(run / 'receipt.json')}))


if __name__ == '__main__':
    main()
