"""Real native memory tests in isolated stores; no shared binding is modified."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import resume_memory as adapter


class NativeResume(unittest.TestCase):
    def setUp(self):
        installed_root = os.environ.get('ONTOLOGY_ENGINEERING_ROOT')
        if not installed_root:
            self.skipTest('Set ONTOLOGY_ENGINEERING_ROOT to a real installed checkout')
        parent = os.environ.get('SIX_TOOLS_TEST_ARTIFACTS')
        if parent:
            Path(parent).mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix=self._testMethodName + '-', dir=parent)).resolve()
        installed = adapter.selected_runtime(installed_root)
        # Copy the verified real binary into a private installation fixture so
        # rejection tests never modify the user's installation or shared store.
        self.install_root = self.root / 'installation'
        program = self.install_root / 'var/install/bin/engineering-memory'
        program.parent.mkdir(parents=True)
        shutil.copy2(installed['runtime_binary'], program)
        self.manifest = self.install_root / 'var/install/INSTALLATION.json'
        self.manifest.write_text(json.dumps({'schema': 'oev2.installation/1',
            'root': str(self.install_root), 'programs': [{'name': 'engineering-memory',
            'path': str(program), 'sha256': installed['runtime_sha256'], 'bytes': program.stat().st_size}]}))
        selected = adapter.selected_runtime(self.install_root, self.root / 'memory')
        self.project = self.root / 'project.json'
        self.project.write_text(json.dumps({'schema': 'oev2.project/1',
                                            'project_id': 'resume-transport-test', 'revision': 'r1',
                                            'objects': {'component': 'rev-a'},
                                            'conditions': {'scope': 'explicit fixture'}}))
        self.request = {
            'schema': 'engineering-memory.session-request/1', 'id': 'source',
            'provider': 'codex', 'session_id': 'isolated-session', 'phase': 'start',
            'task': 'Review component revision under its explicit fixture conditions',
            'query': 'component explicit fixture', 'conditions': ['native isolated test'],
            'project': {'path': str(self.project), 'sha256': adapter.file_sha(self.project),
                        'project_id': 'resume-transport-test', 'revision': 'r1',
                        'object': {'object_id': 'component', 'object_revision': 'rev-a',
                                   'condition_scope': 'scope',
                                   'condition_sha256': adapter.digest(b'explicit fixture')}},
            'include_other_projects': False, 'max_candidates': 1, 'max_context_bytes': 2048,
        }
        self.selected = selected
        self.prepare()

    def prepare(self):
        path = self.root / (self.request['id'] + '.json')
        path.write_text(json.dumps(self.request))
        process = subprocess.run([self.selected['runtime_binary'], '--home', self.selected['runtime_home'],
                                  'session', 'prepare', '--input', str(path)],
                                 capture_output=True, text=True, timeout=20)
        self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
        result = json.loads(process.stdout)
        self.assertTrue(result['ok'])
        self.identity = result['data']['context_sha256']

    def call(self, **changes):
        args = dict(root=self.install_root, home=Path(self.selected['runtime_home']),
                    project=self.project, context_id=self.request['id'],
                    context_sha256=self.identity, provider='codex', session_id='isolated-session',
                    output_root=self.root / 'runs')
        args.update(changes)
        return adapter.resume(**args)

    def rejected(self, value):
        self.assertEqual(value['exit_code'], 2)
        self.assertFalse(value['fresh_context_verified'])
        self.assertTrue((Path(value['run_directory']) / 'failure.json').exists())
        self.assertEqual(value['engineering_adoption'], 'not_granted')

    def test_preserves_task_scope_budget_and_does_not_activate(self):
        before = self.project.read_bytes()
        one, two = self.call(), self.call()
        for result in (one, two):
            self.assertEqual(result['exit_code'], 0, result)
            wire = json.loads((Path(result['run_directory']) / 'request.json').read_text())
            self.assertEqual({k: v for k, v in wire.items() if k not in ('id', 'phase')},
                             {k: v for k, v in self.request.items() if k not in ('id', 'phase')})
            self.assertEqual(result['candidate_ids'], [])  # Empty is an honest native result.
        self.assertNotEqual(one['context'], two['context'])
        self.assertEqual(self.project.read_bytes(), before)
        self.assertFalse(any((Path(self.selected['runtime_home']) / 'active-sessions').glob('*.json')))

    def test_wrong_context_digest_rejected_before_prepare(self):
        result = self.call(context_sha256='0' * 64)
        self.rejected(result)
        self.assertFalse(result['request_prepared'])

    def test_other_session_or_provider_rejected(self):
        for changes in ({'session_id': 'another'}, {'provider': 'claude'}):
            result = self.call(**changes)
            self.rejected(result)
            self.assertFalse(result['request_prepared'])

    def test_stale_project_and_other_path_rejected(self):
        copied = self.root / 'other-project.json'
        copied.write_bytes(self.project.read_bytes())
        self.rejected(self.call(project=copied))
        self.project.write_bytes(self.project.read_bytes() + b' ')
        self.rejected(self.call())

    def test_native_rejects_wrong_object_condition(self):
        # Change the object and rebind the file, while retaining the old condition SHA.
        value = json.loads(self.project.read_text())
        value['conditions']['scope'] = 'changed condition'
        self.project.write_text(json.dumps(value))
        self.request['id'] = 'invalid-condition'
        self.request['project']['sha256'] = adapter.file_sha(self.project)
        path = self.root / 'invalid-condition.json'
        path.write_text(json.dumps(self.request))
        result = subprocess.run([self.selected['runtime_binary'], '--home', self.selected['runtime_home'],
                                 'session', 'prepare', '--input', str(path)],
                                capture_output=True, text=True, timeout=20)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('condition', result.stdout)

    def test_changed_runtime_identity_rejected(self):
        selected = json.loads(self.manifest.read_text())
        selected['programs'][0]['sha256'] = '0' * 64
        self.manifest.write_text(json.dumps(selected))
        result = self.call()
        self.rejected(result)
        self.assertFalse(result['request_prepared'])

    def test_project_change_after_prepare_keeps_partial_context(self):
        original = adapter.run_process

        def during(command, directory, name, *args):
            result = original(command, directory, name, *args)
            if name == 'prepare':
                self.project.write_bytes(self.project.read_bytes() + b' ')
            return result

        with patch.object(adapter, 'run_process', during):
            result = self.call()
        self.rejected(result)
        raw = Path(result['run_directory']) / 'prepare.stdout'
        self.assertTrue(json.loads(raw.read_text())['ok'])
        self.assertIn('Current Project changed', result['error'])

    def test_negative_and_new_tasks_are_not_rewritten(self):
        for i, text in enumerate(['不要继续，暂停', '继续优化另一项目的数据库']):
            self.request.update(id='new-task-' + str(i), task=text, query=text)
            self.prepare()
            result = self.call()
            self.assertEqual(result['exit_code'], 0)
            self.assertEqual(result['task'], text)
            self.assertEqual(result['query'], text)

    def test_default_home_is_inside_selected_installation(self):
        selected = adapter.selected_runtime(self.install_root)
        self.assertEqual(selected['runtime_home'], str(self.install_root / 'var/memory'))
        self.rejected(self.call(home=Path('relative-memory')))

    def test_cli_uses_root_environment_and_explicit_home_without_old_config(self):
        env = dict(os.environ, ONTOLOGY_ENGINEERING_ROOT=str(self.install_root),
                   ENGINEERING_MEMORY_CONFIG=str(self.root / 'must-not-read-old-config.json'))
        process = subprocess.run([sys.executable, str(Path(adapter.__file__).resolve()),
            '--home', self.selected['runtime_home'], '--project', str(self.project),
            '--context-id', self.request['id'], '--context-sha256', self.identity,
            '--provider', 'codex', '--session-id', 'isolated-session',
            '--output-root', str(self.root / 'cli-runs')],
            env=env, capture_output=True, text=True, timeout=30)
        self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
        result = json.loads(process.stdout)
        self.assertTrue(result['fresh_context_verified'])
        self.assertFalse(result['activation_requested'])
        self.assertEqual(result['root'], str(self.install_root))


if __name__ == '__main__':
    unittest.main()
