import sys
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from ontology_engineering.chrome_apple_events import AppleEventsBrowser, evaluate_tab
from ontology_engineering.jev_browser import validate_task, TASK_SCHEMA

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'skills/cad-agent/scripts'))
import misumi_browser_download as download


def archive(part='P1',member='P1.stp'):
    import zipfile
    s=("ISO-10303-21;\nHEADER;FILE_SCHEMA (('CONFIG_CONTROL_DESIGN'));ENDSEC;DATA;"
       "#1=PRODUCT('"+part+"','part','',());#2=SI_UNIT(.MILLI.,.METRE.);ENDSEC;END-ISO-10303-21;").encode()
    f=io.BytesIO()
    with zipfile.ZipFile(f,'w') as z:z.writestr(member,s)
    return f.getvalue()


class DownloadTests(unittest.TestCase):
    def test_old_file_cannot_satisfy_new_download_then_stable_new_file_passes(self):
        with tempfile.TemporaryDirectory() as t:
            p=Path(t);(p/'P1_STEP.zip').write_bytes(archive())
            w=download.FreshStepDownload(p,'P1')
            self.assertIsNone(w.poll());self.assertIsNone(w.poll())
            (p/'P1_STEP (1).zip').write_bytes(archive())
            self.assertIsNone(w.poll())
            r,_,_=w.poll();self.assertEqual(r['part_number'],'P1');self.assertTrue(r['zip_crc_valid'])

    def test_partial_download_cannot_pass(self):
        with tempfile.TemporaryDirectory() as t:
            p=Path(t);w=download.FreshStepDownload(p,'P1')
            (p/'P1_STEP.zip').write_bytes(archive());(p/'P1_STEP.zip.crdownload').write_bytes(b'pending')
            self.assertIsNone(w.poll());self.assertIsNone(w.poll())

    def test_renamed_wrong_part_and_path_traversal_rejected(self):
        for blob in [archive('P2'),archive(member='../P1.stp')]:
            with self.assertRaises(ValueError):download.verify_step_archive(blob,'P1')

    def test_html_or_truncated_archive_rejected(self):
        import zipfile
        for blob in [b'<html>login required</html>',archive()[:-30]]:
            with self.assertRaises((ValueError,zipfile.BadZipFile)):
                download.verify_step_archive(blob,'P1')

    def test_concurrent_matching_downloads_are_ambiguous(self):
        with tempfile.TemporaryDirectory() as t:
            p=Path(t);w=download.FreshStepDownload(p,'P1')
            (p/'P1_STEP.zip').write_bytes(archive());(p/'P1_STEP (1).zip').write_bytes(archive())
            with self.assertRaisesRegex(ValueError,'ambiguous'):w.poll()

    def test_existing_tab_requires_retention_and_explicit_scope(self):
        base={'schema':TASK_SCHEMA,'url':'https://example.org','goal':'Read',
              'backend':'chrome_apple_events','tab_id':123,'keep_open':True,
              'allowed_actions':[{'kind':'click','label':'Download'}]}
        self.assertEqual(validate_task(base)['tab_id'],123)
        for delta in [{'keep_open':False},{'tab_id':True},{'allowed_actions':[]}]:
            with self.assertRaises(ValueError):validate_task({**base,**delta})

    def test_executor_error_has_no_fallback_or_private_error_text(self):
        calls=[]
        def failing(*args,**kwargs):
            calls.append((args,kwargs))
            return subprocess.CompletedProcess(args,1,'','private page details')
        with self.assertRaisesRegex(RuntimeError,'^apple_events_execution_unconfirmed$'):
            evaluate_tab(123,'document.title','https://example.org',runner=failing)
        self.assertEqual(len(calls),1)

    def test_scope_rejection_prevents_any_page_evaluation(self):
        b=object.__new__(AppleEventsBrowser)
        b.allowed=[{'kind':'click','label':'Download'}]
        b.evaluate=lambda _:self.fail('An out-of-scope action must not reach the page')
        with self.assertRaisesRegex(ValueError,'outside_declared_scope'):
            b.act({'kind':'click','label':'Place order'}, {})

    def test_observed_file_transport_rejects_wrong_host_format_and_credentials(self):
        for url in ['http://rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/a/STEP_AP203/P1_STEP.zip',
                    'https://example.org/prod/CadSupplyGC/a/STEP_AP203/P1_STEP.zip',
                    'https://user:pass@rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/a/STEP_AP203/P1_STEP.zip',
                    'https://rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/a/STEP_AP214/P1_STEP.zip']:
            with self.assertRaisesRegex(ValueError,'not_supported'):
                download.download_observed_url(url,'P1','/nonexistent-test-dir')

    def test_format_precondition_removes_generate_and_open_dialog_actions(self):
        p={'actions':[{'kind':'click','label':'CAD下载 (含2D/3D)'},
                      {'kind':'click','label':'数据生成'},
                      {'kind':'select','label':'STEP(AP203)','value':'STEP_AP203'},
                      {'kind':'wait','label':'Wait'}]}
        wrong=download.scoped_actions(p,{'format':'DXF','popup':True})['actions']
        self.assertEqual([a['kind'] for a in wrong],['select','wait'])
        right=download.scoped_actions(p,{'format':'STEP_AP203','popup':True})['actions']
        self.assertEqual([a['label'] for a in right],['数据生成','Wait'])


if __name__=='__main__':unittest.main()
