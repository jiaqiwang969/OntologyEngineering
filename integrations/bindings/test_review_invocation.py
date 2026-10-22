"""Process/byte-transport probes; stub executors are not native semantic evidence."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import time
from unittest.mock import patch
import common

import reconcile_reviewed as wrapper


class InvocationTests(unittest.TestCase):
    def setUp(self):
        retained = os.environ.get('ONTOLOGY_BINDING_TEST_ARTIFACTS')
        if retained:
            self.root = Path(retained) / self._testMethodName
            self.root.mkdir(parents=True, exist_ok=False)
        else:
            self.tmp = tempfile.TemporaryDirectory(prefix='six-tools-current-binding-')
            self.addCleanup(self.tmp.cleanup)
            self.root = Path(self.tmp.name)
        self.root = self.root.resolve()
        (self.root/'var/install/bin').mkdir(parents=True)
        self.project = self.root/'project.json'
        self.project.write_text('{"project_id":"controlled-transport-test","revision":"r2"}')
        self.request = self.root/'request.json'
        self.request.write_text(json.dumps({'current':{'path':'project.json','sha256':wrapper.file_sha(self.project)}}))
        self.cli = self.root/'var/install/bin/oev2'
        self.marker = self.root/'executed'
        self.executor("print('complete transport fixture')\n")

    def executor(self, body):
        self.cli.write_text('#!'+sys.executable+'\nfrom pathlib import Path\nimport sys,os,json,time,subprocess,signal\n'+
                            'Path('+repr(str(self.marker))+').write_text("started")\n'+body)
        self.cli.chmod(0o700)
        (self.root/'var/install/INSTALLATION.json').write_text(json.dumps({'schema':'oev2.installation/1','root':str(self.root),'programs':[{'name':'oev2','path':str(self.cli),'sha256':wrapper.file_sha(self.cli)}]}))

    def call(self, **kwargs):
        return wrapper.execute(self.root,self.request,**kwargs)

    def assertRejected(self, result, launched):
        self.assertEqual(result['exit_code'],2)
        self.assertEqual(result['native_started'],launched)
        self.assertFalse(result['inputs_stable'])
        self.assertEqual(result['engineering_adoption'],'not_granted')
        self.assertTrue((Path(result['run_directory'])/'failure.json').is_file())

    def test_explicit_mode_required(self):
        self.assertRejected(self.call(),False)
        self.assertRejected(self.call(project=self.project,historical=True),False)
        self.assertFalse(self.marker.exists())

    def test_current_and_history_are_distinct(self):
        current=self.call(project=self.project)
        historical=self.call(historical=True)
        self.assertEqual(current['exit_code'],0)
        self.assertEqual(current['current_applicability'],'bound_snapshot_only')
        self.assertEqual(historical['current_applicability'],'historical_only')
        self.assertNotEqual(current['run_directory'],historical['run_directory'])

    def test_stale_snapshot_does_not_launch(self):
        old=self.root/'old.json';old.write_text('{"project_id":"controlled-transport-test","revision":"r1"}')
        self.request.write_text(json.dumps({'current':{'path':'old.json','sha256':wrapper.file_sha(old)}}))
        self.assertRejected(self.call(project=self.project),False)
        self.assertFalse(self.marker.exists())

    def test_invalid_json_preserves_original_bytes(self):
        self.request.write_bytes(b'{incomplete')
        result=self.call(project=self.project);self.assertRejected(result,False)
        self.assertEqual((Path(result['run_directory'])/'request.json').read_bytes(),b'{incomplete')

    def test_project_change_after_start_is_not_reusable(self):
        self.executor('Path('+repr(str(self.project))+').write_text("changed")\n')
        result=self.call(project=self.project);self.assertRejected(result,True)
        self.assertIn('Project changed',result['error'])

    def test_request_change_after_start_is_not_reusable(self):
        self.executor('Path('+repr(str(self.request))+').write_text("changed")\n')
        result=self.call(project=self.project);self.assertRejected(result,True)
        self.assertIn('Request changed',result['error'])

    def test_selected_program_change_is_rejected(self):
        status=self.root/'var/install/INSTALLATION.json'
        self.executor('p=Path('+repr(str(status))+')\nv=json.loads(p.read_text());v["programs"][0]["path"]="different";p.write_text(json.dumps(v))\n')
        result=self.call(project=self.project);self.assertRejected(result,True)
        self.assertIn('Selected native executable',result['error'])

    def test_unrelated_status_change_is_preserved(self):
        status=self.root/'var/install/INSTALLATION.json'
        self.executor('p=Path('+repr(str(status))+')\nv=json.loads(p.read_text());v["unrelated"]="preserved";p.write_text(json.dumps(v))\n')
        result=self.call(project=self.project)
        self.assertEqual(result['exit_code'],0)
        self.assertEqual(json.loads(status.read_text())['unrelated'],'preserved')

    def test_output_limit_retains_prefix_and_stops_group(self):
        self.executor('sys.stdout.write("x"*50000);sys.stdout.flush();time.sleep(5)\n')
        result=self.call(project=self.project,max_output_bytes=1024,max_seconds=3)
        self.assertRejected(result,True)
        run=Path(result['run_directory']);state=json.loads((run/'native.process.json').read_text())
        self.assertEqual(state['failure'],'output_limit');self.assertTrue(state['process_group_absent'])
        self.assertEqual((run/'native.stdout').stat().st_size,1024)
        self.assertFalse(state['output_complete'])

    def test_timeout_reclaims_term_resistant_child_and_retains_output(self):
        self.executor('child=subprocess.Popen([sys.executable,"-c","import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(20)"])\nprint("partial evidence",child.pid,flush=True)\ntime.sleep(20)\n')
        result=self.call(project=self.project,max_seconds=0.4);self.assertRejected(result,True)
        run=Path(result['run_directory']);state=json.loads((run/'native.process.json').read_text())
        self.assertEqual(state['failure'],'timeout');self.assertTrue(state['process_group_absent'])
        self.assertIn('partial evidence',(run/'native.stdout').read_text())

    def test_started_receipt_failure_reclaims_started_parent_and_child(self):
        ready=self.root/'child-ready'
        child_code='import signal,time;from pathlib import Path;signal.signal(signal.SIGTERM,signal.SIG_IGN);Path('+repr(str(ready))+').write_text("ready");time.sleep(20)'
        self.executor('subprocess.Popen([sys.executable,"-c",'+repr(child_code)+'])\ntime.sleep(20)\n')
        real_save=common.save
        def failing_save(path,value):
            if path.name=='native.started.json':
                deadline=time.monotonic()+2
                while not ready.exists() and time.monotonic()<deadline:time.sleep(0.01)
                self.assertTrue(ready.exists())
                raise OSError('injected started receipt write failure')
            return real_save(path,value)
        with patch.object(common,'save',failing_save):
            result=self.call(project=self.project)
        self.assertRejected(result,True)
        state=json.loads((Path(result['run_directory'])/'native.process.json').read_text())
        self.assertIn('started receipt write failure',state['failure'])
        self.assertTrue(state['process_group_absent']);self.assertFalse(state['output_complete'])
        self.assertFalse(common.group_exists(state['pid']))

    def test_selector_construction_failure_reclaims_spawned_group(self):
        self.executor('time.sleep(20)\n')
        with patch.object(common.selectors,'DefaultSelector',side_effect=OSError('injected selector failure')):
            result=self.call(project=self.project)
        self.assertRejected(result,True)
        state=json.loads((Path(result['run_directory'])/'native.process.json').read_text())
        self.assertIn('selector failure',state['failure']);self.assertTrue(state['process_group_absent'])
        self.assertFalse(common.group_exists(state['pid']))

    def test_native_nonzero_preserved(self):
        self.executor('print("native error",file=sys.stderr);sys.exit(7)\n')
        result=self.call(project=self.project)
        self.assertEqual(result['exit_code'],7);self.assertTrue(result['inputs_stable'])
        self.assertIn('native error',(Path(result['run_directory'])/'native.stderr').read_text())

    def test_memory_requires_full_identity_and_current_mode(self):
        self.assertRejected(self.call(project=self.project,memory_context='id'),False)
        self.assertRejected(self.call(historical=True,memory_context='id',memory_sha='a'*64),False)
        self.assertFalse(self.marker.exists())


if __name__=='__main__':unittest.main(verbosity=2)
