"""Reject another installation or a changed executable before launching a child."""
import json
from pathlib import Path
import tempfile
import unittest

from common import file_sha, selected_program


class InstallationIdentity(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='ontology-install-identity-');self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name).resolve();self.binary=self.root/'var/install/bin/oev2'
        self.binary.parent.mkdir(parents=True);self.binary.write_bytes(b'transport identity fixture; never executed')
        self.record=self.root/'var/install/INSTALLATION.json'
        self.value={'schema':'oev2.installation/1','root':str(self.root),
                    'programs':[{'name':'oev2','path':str(self.binary),'sha256':file_sha(self.binary)}]}
        self.save()

    def save(self):self.record.write_text(json.dumps(self.value))

    def test_exact_installation_and_unrelated_metadata(self):
        before=selected_program(self.root,'oev2');self.value['unrelated']='preserved';self.save()
        self.assertEqual(selected_program(self.root,'oev2'),before)

    def test_wrong_root_rejected(self):
        self.value['root']=str(self.root/'other');self.save()
        with self.assertRaisesRegex(ValueError,'root'):selected_program(self.root,'oev2')

    def test_duplicate_program_rejected(self):
        self.value['programs']*=2;self.save()
        with self.assertRaisesRegex(ValueError,'exactly one'):selected_program(self.root,'oev2')

    def test_other_program_path_rejected(self):
        self.value['programs'][0]['path']=str(self.root/'other');self.save()
        with self.assertRaisesRegex(ValueError,'outside'):selected_program(self.root,'oev2')

    def test_symlinked_bin_parent_rejected(self):
        actual=self.root/'external-bin'
        self.binary.parent.rename(actual)
        self.binary.parent.symlink_to(actual,target_is_directory=True)
        with self.assertRaisesRegex(ValueError,'outside'):selected_program(self.root,'oev2')

    def test_symlinked_installation_record_rejected(self):
        original=self.root/'external-installation.json'
        self.record.rename(original);self.record.symlink_to(original)
        with self.assertRaisesRegex(ValueError,'real installation'):selected_program(self.root,'oev2')

    def test_changed_executable_rejected(self):
        self.binary.write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError,'digest changed'):selected_program(self.root,'oev2')


if __name__=='__main__':unittest.main(verbosity=2)
