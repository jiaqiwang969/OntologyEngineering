"""Exercise installation preflight without pip, a browser or network access."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class JevInstallationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'relocated skill'
        self.runtime = self.root / 'runtime/jev-ultrafast'
        shutil.copytree(ROOT / 'runtime/jev-ultrafast', self.runtime,
                        ignore=shutil.ignore_patterns('.venv', '__pycache__'))
        package = self.root / 'ontology_engineering'
        package.mkdir()
        for name in ('__init__.py', 'jev_browser.py', 'jev_transport.py', 'local_paths.py'):
            shutil.copy2(ROOT / 'ontology_engineering' / name, package / name)
        self.log = Path(self.temp.name) / 'interpreter-calls.jsonl'

    def interpreter(self, path, version):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('#!' + sys.executable + '\n'
                        'import json, pathlib, sys\n'
                        f'with pathlib.Path({str(self.log)!r}).open("a") as stream:\n'
                        f'    stream.write(json.dumps({{"version": {version!r}, "args": sys.argv[1:]}}) + "\\n")\n'
                        'if sys.argv[1] != "-":\n'
                        '    raise SystemExit("Unexpected install or network-capable command")\n'
                        'sys.argv = sys.argv[1:]\n'
                        f'sys.version_info = {version!r}\n'
                        'exec(compile(sys.stdin.read(), "setup-preflight", "exec"))\n')
        path.chmod(0o700)
        return path

    def setup(self, python):
        return subprocess.run(['bash', str(ROOT / 'tests/fixtures/jev_setup_preflight.sh'), str(self.root)], cwd=self.temp.name,
                              env={**os.environ, 'OE_JEV_PYTHON': str(python), 'PYTHONDONTWRITEBYTECODE': '1'},
                              capture_output=True, text=True)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_custom_old_python_stops_before_creating_environment(self):
        python = self.interpreter(Path(self.temp.name) / 'python with spaces', (3, 11, 9))
        result = self.setup(python)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Python >= 3.12', result.stderr)
        self.assertIn('OE_JEV_PYTHON', result.stderr)
        self.assertFalse((self.runtime / '.venv').exists())
        self.assertEqual(len(self.calls()), 1)

    def test_old_existing_environment_stops_before_pip_and_is_preserved(self):
        python = self.interpreter(Path(self.temp.name) / 'selected python', (3, 12, 0))
        existing = self.interpreter(self.runtime / '.venv/bin/python', (3, 11, 9))
        before = existing.read_bytes()
        result = self.setup(python)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Existing Jev environment', result.stderr)
        self.assertIn('move runtime/jev-ultrafast/.venv aside', result.stderr)
        self.assertEqual(existing.read_bytes(), before)
        self.assertEqual([call['version'] for call in self.calls()], [[3, 12, 0], [3, 11, 9]])


if __name__ == '__main__':
    unittest.main()
