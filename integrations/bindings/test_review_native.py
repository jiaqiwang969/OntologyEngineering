"""Execute actual installed Rust review and memory on new synthetic snapshots."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from common import digest, file_sha, root_path, selected_program
from reconcile_reviewed import execute


class NativeReview(unittest.TestCase):
    def test_current_history_memory_and_source_rejection(self):
        root=root_path(os.environ.get('ONTOLOGY_ENGINEERING_ROOT'))
        parent=root/'var/tests/portable-bindings';parent.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='review-',dir=parent) as temporary:
            directory=Path(temporary).resolve()
            raw=directory/'original.txt';raw.write_text('New synthetic source; no engineering claim.')
            source_hash=file_sha(raw)
            project={'schema':'oev2.project/1','project_id':'portable-review-test','revision':'r1',
                     'purpose':'Exercise current invocation guards, with no affected decisions',
                     'objects':{'component':'rev-a'},'conditions':{'scope':'synthetic'},
                     'evidence':{'raw':source_hash},'decisions':[],'dependencies':{}}
            before=directory/'before.json';before.write_text(json.dumps(project))
            project['revision']='r2';current=directory/'current.json';current.write_text(json.dumps(project))
            def ref(path):return {'path':str(path.relative_to(root)),'sha256':file_sha(path)}
            request=directory/'request.json';request.write_text(json.dumps({
                'schema':'oev2.reviewed-reconciliation-request/1','before':ref(before),
                'current':ref(current),'sources':[ref(raw)],'reviews':[]}))
            result=execute(root,request,project=current)
            self.assertEqual(result['exit_code'],0,result)
            native=json.loads((Path(result['run_directory'])/'native.json').read_text())
            self.assertEqual(native['status'],'review_complete')
            self.assertEqual(native['review_satisfied'],[])
            self.assertEqual(native['engineering_adoption'],'not_granted')
            historical=execute(root,request,historical=True)
            self.assertEqual(historical['current_applicability'],'historical_only',historical)
            self.assertEqual(historical['exit_code'],0,historical)
            memory=selected_program(root,'engineering-memory');home=directory/'memory'
            memory_request={'schema':'engineering-memory.session-request/1','id':'current-synthetic',
                'provider':'codex','session_id':'portable-test','phase':'start','task':'Check synthetic component',
                'query':'synthetic component','conditions':['synthetic'],
                'project':{'path':str(current),'sha256':file_sha(current),'project_id':project['project_id'],
                           'revision':'r2','object':{'object_id':'component','object_revision':'rev-a',
                           'condition_scope':'scope','condition_sha256':digest(b'synthetic')}},
                'include_other_projects':False,'max_candidates':1,'max_context_bytes':2048}
            memory_file=directory/'memory-request.json';memory_file.write_text(json.dumps(memory_request))
            child=subprocess.run([memory['path'],'--home',str(home),'session','prepare','--input',str(memory_file)],
                                 capture_output=True,text=True,timeout=20)
            self.assertEqual(child.returncode,0,child.stdout+child.stderr)
            context_sha=json.loads(child.stdout)['data']['context_sha256']
            bound=execute(root,request,project=current,memory_context='current-synthetic',memory_sha=context_sha,memory_home=home)
            self.assertEqual(bound['exit_code'],0,bound)
            self.assertTrue(bound['memory']['verified_before']);self.assertTrue(bound['memory']['verified_after'])
            wrong=execute(root,request,project=current,memory_context='current-synthetic',memory_sha='0'*64,memory_home=home)
            self.assertEqual(wrong['exit_code'],2);self.assertFalse(wrong['native_started'])
            raw.write_text('tampered source')
            rejected=execute(root,request,project=current)
            self.assertNotEqual(rejected['exit_code'],0)
            self.assertTrue(rejected['native_started'])
            self.assertEqual(rejected['engineering_adoption'],'not_granted')


if __name__=='__main__':unittest.main(verbosity=2)
