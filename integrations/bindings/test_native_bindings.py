"""Real Rust Project checks on synthetic inputs; no private historical Project."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest

from common import root_path, file_sha
from verify_candidate import check


class NativeBindings(unittest.TestCase):
    def test_five_tool_bindings_and_source_tampering(self):
        root = root_path(os.environ.get('ONTOLOGY_ENGINEERING_ROOT'))
        with tempfile.TemporaryDirectory(prefix='ontology-bindings-') as temporary:
            directory = Path(temporary).resolve()
            source = directory / 'result.json'
            source.write_text('{"observation":"synthetic binding test, not business evidence"}')
            for tool in ['leantoken', 'gitcontribute', 'jacobian', 'smokinggun', 'flameox']:
                digest = file_sha(source)
                base = {'schema':'oev2.project/1','project_id':'synthetic-'+tool,'revision':'test-1',
                        'purpose':'Synthetic binding rejection test only','objects':{'business-object':'v1'},
                        'conditions':{'scope':'synthetic software test'},'evidence':{'raw':digest},
                        'decisions':[{'id':'observation','object_id':'business-object','object_revision':'v1',
                         'condition_scope':'scope','source_sha256':digest,'conclusion':'supported','adopted':True,'basis':['raw']},
                         {'id':'downstream','object_id':'business-object','object_revision':'v1',
                         'condition_scope':'scope','source_sha256':digest,'conclusion':'supported','adopted':True,'basis':['observation']}],
                        'dependencies':{'downstream':['observation']}}
                before, after = directory/'before.json', directory/'candidate.json'
                before.write_text(json.dumps(base))
                for change in ['unchanged','object','condition','source']:
                    with self.subTest(tool=tool, change=change):
                        candidate=copy.deepcopy(base);candidate['revision']='test-2'
                        if change=='object':candidate['objects']['business-object']='v2'
                        if change=='condition':candidate['conditions']['scope']='changed condition'
                        selected=source
                        if change=='source':
                            selected=directory/'new-result.json';selected.write_text('{"observation":"changed evidence"}')
                            candidate['evidence']['raw']=file_sha(selected)
                        after.write_text(json.dumps(candidate))
                        result=check(root,before,after,{'raw':str(selected)})
                        self.assertEqual(result['binding_checks_passed'],change=='unchanged',result)
                        self.assertEqual(result['engineering_adoption'],'not_granted')
                        if change!='unchanged':
                            impact=json.loads((Path(result['run_directory'])/'impact.json').read_text())
                            self.assertIn('downstream',impact['stale_adoptions'])
                after.write_text(json.dumps(base));source.write_text('changed without updating hash')
                result=check(root,before,after,{'raw':str(source)})
                self.assertEqual(result['exit_code'],2)
                self.assertIn('Evidence bytes changed',result['error'])
                self.assertEqual(result['native_results'],[])
                source.write_text('{"observation":"synthetic binding test, not business evidence"}')


if __name__ == '__main__':
    unittest.main(verbosity=2)
