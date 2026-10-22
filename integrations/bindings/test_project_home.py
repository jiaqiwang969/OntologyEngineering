"""ProjectStore transport boundaries; controlled executors are not semantic tests."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

from common import file_sha, project_data_root, project_file
from reconcile_reviewed import execute
from verify_candidate import check


class ProjectHome(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='explicit-project-home-')
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name).resolve()
        self.install = self.base / 'installation'
        self.home = self.base / 'project-a'
        self.other = self.base / 'project-b'
        self.home.mkdir(); self.other.mkdir()
        self.cli = self.install / 'var/install/bin/oev2'
        self.cli.parent.mkdir(parents=True)
        self.argv = self.home / 'observed-argv.jsonl'
        self.cli.write_text('#!' + sys.executable + '\nimport json,sys\nfrom pathlib import Path\n'
                            'with Path(' + repr(str(self.argv)) + ').open("a") as f: '
                            'f.write(json.dumps(sys.argv[1:])+"\\n")\n'
                            'print("controlled transport only")\n')
        self.cli.chmod(0o700)
        (self.install / 'var/install/INSTALLATION.json').write_text(json.dumps({
            'schema': 'oev2.installation/1', 'root': str(self.install), 'programs': [
                {'name': 'oev2', 'path': str(self.cli), 'sha256': file_sha(self.cli)}]}))
        self.project = self.home / 'current.json'
        self.project.write_text('{"project_id":"project-a","revision":"r1","evidence":{}}')
        self.request = self.home / 'request.json'
        self.request.write_text(json.dumps({'current': {'path': 'current.json',
                                                        'sha256': file_sha(self.project)}}))

    def test_review_passes_explicit_home_and_keeps_receipts_in_project(self):
        result = execute(self.install, self.request, project=self.project, project_home=self.home)
        self.assertEqual(result['exit_code'], 0, result)
        self.assertIn(self.home, Path(result['run_directory']).parents)
        args = json.loads(self.argv.read_text().splitlines()[0])
        self.assertEqual(args[:4], ['--root', str(self.install), '--project-home', str(self.home)])
        self.assertFalse((self.install / 'var/investigations').exists())
        self.assertFalse(any(self.other.iterdir()))

    def test_candidate_check_passes_explicit_home_on_every_child(self):
        candidate = self.home / 'candidate.json'
        candidate.write_bytes(self.project.read_bytes())
        result = check(self.install, self.project, candidate, {}, project_home=self.home)
        self.assertTrue(result['binding_checks_passed'], result)
        lines = self.argv.read_text().splitlines()
        self.assertEqual(len(lines), 3)
        for line in lines:
            self.assertEqual(json.loads(line)[:4],
                             ['--root', str(self.install), '--project-home', str(self.home)])
        self.assertIn(self.home, Path(result['run_directory']).parents)

    def test_other_store_cannot_supply_request_or_current_project(self):
        with self.assertRaises(ValueError):
            execute(self.install, self.request, project=self.project, project_home=self.other)
        foreign = self.other / 'current.json'
        foreign.write_bytes(self.project.read_bytes())
        result = execute(self.install, self.request, project=foreign, project_home=self.home)
        self.assertEqual(result['exit_code'], 2, result)
        self.assertFalse(result['native_started'])
        self.assertFalse(self.argv.exists())

    def test_symlink_traversal_and_software_parent_are_rejected(self):
        link = self.base / 'linked-home'
        link.symlink_to(self.home, target_is_directory=True)
        source = self.install / 'sources'
        source.mkdir()
        for selected in [link, self.install, self.base, Path('relative'), source,
                         self.install / 'var']:
            with self.assertRaises(ValueError):
                project_data_root(self.install, selected)
        local = self.install / 'var/projects/local'
        local.mkdir(parents=True)
        self.assertEqual(project_data_root(self.install, local), local)
        with self.assertRaises(ValueError):
            project_file(self.home, '../project-b/current.json')
        (self.home / 'runs').symlink_to(self.other, target_is_directory=True)
        with self.assertRaises(ValueError):
            execute(self.install, self.request, project=self.project, project_home=self.home)
        self.assertFalse(any(self.other.iterdir()))

    def test_explicit_home_does_not_fall_back_to_installation(self):
        self.project.rename(self.install / 'current.json')
        result = execute(self.install, self.request, project_home=self.home, historical=True)
        self.assertEqual(result['exit_code'], 2, result)
        self.assertFalse(result['native_started'])
        self.assertFalse(self.argv.exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
