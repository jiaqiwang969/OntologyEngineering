#!/usr/bin/env python3
"""Behavioral regressions for native ledger migration and bounded file/job intake."""
from copy import deepcopy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'assembly/scripts'))
from self_test import complete_authoring_fixture,bind_authoring_fixture_files
from validate_authoring_manifest import native_template,validate_document
from nx_direct import prepare,bundle,safe_members,load_profile,remote,main as nx_main
from misumi_archive import validate_url,check_2d
from semantic_query import capabilities

class MigrationTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name)
    def tearDown(self):self.tmp.cleanup()
    def fixture(self):
        source=self.root/'source.step';source.write_bytes(b'synthetic source, not geometry')
        evidence=self.root/'observation.json';evidence.write_text('{"fixture": true}')
        doc=native_template(bind_authoring_fixture_files(complete_authoring_fixture(),source,evidence))
        model=self.root/'native.prt';model.write_bytes(b'synthetic PRT-shaped ledger fixture, not real CAD')
        sha=hashlib.sha256(model.read_bytes()).hexdigest()
        doc['native_target']['root_model_path']='native.prt'
        read=doc['native_readback'];read['native_session_id']='fixture-job-A:pid-1:start-A'
        read['second_readback']['native_session_id']='fixture-job-B:pid-2:start-B'
        read['persisted_files']=[{'path':'native.prt','sha256':sha}]
        read['data_file_identity']='sha256:'+sha;read['second_readback']['data_file_identity']='sha256:'+sha
        return doc
    def validate(self,doc,check=True):return validate_document(doc,self.root/'manifest.json',check)
    def test_v3_positive_ledger(self):
        result,code=self.validate(self.fixture());self.assertEqual(code,0,result)
        self.assertEqual(result['status'],'PASS');self.assertIn('Does not execute NX',result['claim_boundary'])
    def test_actual_file_hash_change_fails(self):
        doc=self.fixture();(self.root/'native.prt').write_bytes(b'changed')
        result,code=self.validate(doc);self.assertEqual(code,1);self.assertTrue(any('hash mismatch' in x for x in result['errors']))
    def test_same_session_cannot_prove_fresh_reopen(self):
        doc=self.fixture();doc['native_readback']['second_readback']['native_session_id']=doc['native_readback']['native_session_id']
        self.assertEqual(self.validate(doc)[0]['status'],'FAIL')
    def test_unchecked_files_hold(self):self.assertNotEqual(self.validate(self.fixture(),False)[0]['status'],'PASS')
    def test_v3_missing_nut_still_fails(self):
        doc=self.fixture();doc['completeness']['requirements'].pop()
        self.assertEqual(self.validate(doc)[0]['status'],'FAIL')
    def test_v3_singular_transform_still_fails(self):
        doc=self.fixture();doc['placement_contracts'][0]['world_transform']=[0.0]*16
        self.assertEqual(self.validate(doc)[0]['status'],'FAIL')
    def test_mixed_legacy_fields_fail(self):
        doc=self.fixture();doc['fusion_target']=deepcopy(doc['native_target'])
        self.assertEqual(self.validate(doc)[0]['status'],'FAIL')
    def test_legacy_claim_cannot_bypass_nx_persistence(self):
        doc=self.fixture()
        doc['target_claims']=[x.replace('PERSISTED_NATIVE_ASSEMBLY','PERSISTED_FUSION_ASSEMBLY') for x in doc['target_claims']]
        doc['native_readback']['persisted_files']=[]
        doc['native_readback']['native_session_id']='UNKNOWN'
        doc['native_readback']['second_readback']['native_session_id']='UNKNOWN'
        self.assertEqual(self.validate(doc)[0]['status'],'FAIL')
    def test_unknown_cad_does_not_pass(self):
        doc=self.fixture();doc['native_target']['cad_system']='UNKNOWN'
        self.assertEqual(self.validate(doc)[0]['status'],'FAIL')
    def test_input_tampering_and_reuse_fail(self):
        journal=self.root/'source.py';journal.write_text('def run(session,job,params): return {}\n')
        dest=self.root/'job';prepare(journal,{},[],dest,'task-001')
        bundle(dest)
        (dest/'journal.py').write_text('changed')
        with self.assertRaisesRegex(ValueError,'changed'):bundle(dest)
        with self.assertRaises(FileExistsError):prepare(journal,{},[],dest,'task-001')
    def test_job_id_traversal_rejected(self):
        with self.assertRaises(ValueError):prepare('missing',{},[],self.root/'job','../other')
    def test_archive_escape_and_windows_collision_rejected(self):
        for names in [('../escape.dxf',),('A.dxf','a.dxf'),('x:stream.dxf',),('/abs.dxf',)]:
            memory=io.BytesIO()
            with zipfile.ZipFile(memory,'w') as z:
                for name in names:z.writestr(name,b'x')
            with zipfile.ZipFile(io.BytesIO(memory.getvalue())) as z:
                with self.assertRaises(ValueError):safe_members(z)
    def test_external_or_credential_url_rejected(self):
        for url in ['https://evil.invalid/a.zip','https://www.misumi.com.cn/linked/material/a.zip?token=secret','http://www.misumi.com.cn/linked/material/a.zip','https://user@www.misumi.com.cn/linked/material/a.zip']:
            with self.assertRaises(ValueError):validate_url(url)
    def test_html_renamed_dxf_rejected(self):
        with self.assertRaises(ValueError):check_2d('part.dxf',b'<html>login</html>')
    def test_active_catalog_excludes_retired_mcp(self):
        self.assertEqual({x['tool_id'] for x in capabilities()['records']},{'NXDirect','MISUMI_CN'})
        with self.assertRaises(ValueError):capabilities('FusionMCP')
    def test_retired_cli_has_no_implicit_remote_call(self):
        result=subprocess.run([sys.executable,str(ROOT/'scripts/nx_call.py'),'--status'],capture_output=True,text=True)
        self.assertNotEqual(result.returncode,0);self.assertIn('Retired CAD MCP',result.stderr)
    def test_timeout_does_not_retry(self):
        profile=self.root/'profile.json';profile.write_text(json.dumps({'ssh_command':['ssh','example'],'remote_root':'%USERPROFILE%\\work\\cad-agent-jobs','run_journal':'C:\\NX\\run_journal.exe'}))
        args=['nx_direct','run','--profile',str(profile),'--job-id','timeout-test']
        with patch.object(sys,'argv',args),patch('nx_direct.remote',side_effect=subprocess.TimeoutExpired('ssh',45)) as call,patch('sys.stdout',new_callable=io.StringIO) as output:
            self.assertEqual(nx_main(),3);self.assertEqual(call.call_count,1);self.assertIn('UNKNOWN_AFTER_DISPATCH',output.getvalue())
    def test_fleet_discovery_cannot_consume_upload_input(self):
        profile={'ssh_command':['fleet','ssh','cad-host']}
        answers=[subprocess.CompletedProcess([],0,b'ssh -o StrictHostKeyChecking=yes user@192.0.2.10\n',b''),subprocess.CompletedProcess([],0,b'{}',b'')]
        with patch('nx_direct.subprocess.run',side_effect=answers) as run:
            remote(profile,'Write-Output 1',b'upload bytes')
            self.assertEqual(run.call_args_list[0].kwargs['stdin'],subprocess.DEVNULL)
            self.assertEqual(run.call_args_list[1].kwargs['input'],b'upload bytes')
            self.assertEqual(run.call_args_list[1].args[0][0],'ssh')
if __name__=='__main__':unittest.main(verbosity=2)
