import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPTS = Path(__file__).resolve().parents[1]/'skills/cad-agent/scripts'
sys.path.insert(0, str(SCRIPTS))
import supplier_acquisition_choice as choice
import supplier_artifacts as artifacts
import misumi_acquire as acquire
import misumi_browser_download as browser_download


def context():
    return {'schema': 'cad-agent.supplier-acquisition-context/v1',
            'goal': 'Inspect supplier geometry in NX', 'background': 'Developing a mechanism',
            'object_identity': {'part': 'P1'}, 'constraints': ['No orders'],
            'needs': [{'id': 'geometry', 'purpose': '3D envelope and interface study',
                       'acceptance': 'Actual supplier file; native readback separately required'}]}


def observation():
    return {'part': 'P1', 'source_url': 'https://www.misumi.com.cn/observed', 'observation_id': 'o1',
            'candidates': [{'id': 's', 'kind': 'cad_format', 'label': 'STEP', 'value': '1'},
                           {'id': 'p', 'kind': 'cad_format', 'label': 'PARASOLID', 'value': '4'},
                           {'id': 'd', 'kind': 'cad_format', 'label': 'DXF', 'value': '5'}]}


def archive(name, data):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w') as z:
        z.writestr(name, data)
    return buf.getvalue()


STEP = (b"ISO-10303-21;\nHEADER;FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));"
        b"ENDSEC;DATA;#1=PRODUCT('P1_1','','',());#2=SI_UNIT(.MILLI.,.METRE.);ENDSEC;END-ISO-10303-21;")


class SupplierAcquisitionTests(unittest.TestCase):
    def test_choice_records_use_private_skill_directory_and_never_overwrite(self):
        calls = []
        def transport(payload):
            calls.append(payload)
            answers = {}
            for key, question in payload['questions'].items():
                selected = next(iter(question['criteria']))
                answers[key] = {'type': 'choice', 'choice': selected, 'confidence': 1,
                    'probabilities': {k: float(k == selected) for k in question['criteria']}}
            return {'model': payload['model'], 'answers': answers,
                    'usage': {'input_tokens': 1, 'output_tokens': 1}}
        with tempfile.TemporaryDirectory() as name, patch('ontology_engineering.local_paths.SKILL_ROOT', Path(name)):
            output = Path(name) / 'var/projects/fixture/supplier-choice'
            result = choice.choose(context(), observation(), output, transport=transport)
            self.assertEqual(result['execution'], 'not_started')
            self.assertTrue((output / 'request.json').is_file())
            original = (output / 'request.json').read_bytes()
            with self.assertRaises(FileExistsError):
                choice.choose(context(), observation(), output, transport=transport)
            self.assertEqual(len(calls), 1)
            self.assertEqual((output / 'request.json').read_bytes(), original)

    def test_supplier_writers_reject_core_or_external_outputs_before_execution(self):
        task = {'schema': acquire.jev_browser.TASK_SCHEMA,
            'url': 'https://www.misumi.com.cn/vona2/detail/?HissuCode=P1', 'goal': 'Inspect observed download',
            'backend': 'chrome_apple_events', 'tab_id': 123, 'keep_open': True,
            'allowed_actions': [{'kind': 'click', 'label': 'Download'}]}
        with tempfile.TemporaryDirectory() as name, patch('ontology_engineering.local_paths.SKILL_ROOT', Path(name)), \
                patch.object(acquire, 'AppleEventsBrowser', side_effect=AssertionError('browser must not open')):
            for output in (Path(name) / 'references/run', Path(name).parent / 'outside-supplier'):
                with self.assertRaisesRegex(ValueError, 'must_be_inside_skill'):
                    choice.choose(context(), observation(), output, transport=lambda _: self.fail('model called'))
                with self.assertRaisesRegex(ValueError, 'must_be_inside_skill'):
                    acquire.run(task, context(), Path(name) / 'input-downloads', output)
                with self.assertRaisesRegex(ValueError, 'must_be_inside_skill'):
                    browser_download.run(task, Path(name) / 'input-downloads', 'P1', output, policy='fixed')
                self.assertFalse(output.exists())

    def test_help_does_not_need_browser_environment(self):
        with tempfile.TemporaryDirectory() as d, patch.object(acquire, 'ROOT', Path(d)), \
                patch.object(sys, 'argv', ['misumi_acquire.py', '--help']), \
                patch.object(acquire.os, 'execv') as restart, \
                patch.object(acquire, 'run') as run, \
                patch('sys.stdout', new_callable=io.StringIO) as output:
            with self.assertRaises(SystemExit) as caught:
                acquire.main()
        self.assertEqual(caught.exception.code, 0)
        self.assertIn('--download-dir', output.getvalue())
        restart.assert_not_called()
        run.assert_not_called()

    def test_missing_environment_returns_setup_without_reading_task_or_running(self):
        args = ['misumi_acquire.py', '--task', 'missing-task.json', '--context', 'missing-context.json',
                '--download-dir', 'unused', '--output', 'unused-output']
        with tempfile.TemporaryDirectory() as d, patch.object(acquire, 'ROOT', Path(d)), \
                patch.object(sys, 'argv', args), patch.object(acquire.os, 'execv') as restart, \
                patch.object(acquire, 'run') as run, \
                patch('sys.stdout', new_callable=io.StringIO) as output:
            status = acquire.main()
        self.assertEqual(status, 2)
        result = json.loads(output.getvalue())
        self.assertEqual(result['status'], 'setup_required')
        self.assertEqual(result['browser_execution'], 'not_run')
        self.assertIn('runtime/jev-ultrafast/setup.sh', result['next_step'])
        restart.assert_not_called()
        run.assert_not_called()

    def test_all_observed_formats_are_choices_and_no_early_finish(self):
        payload, choices = choice.prepare(context(), observation())
        self.assertEqual({v['candidate_id'] for v in choices.values()}, {'s', 'p', 'd'})
        self.assertNotIn('FINISH', payload['questions']['next_acquisition']['criteria'])

    def test_downloaded_bytes_alone_do_not_satisfy_need(self):
        c = context(); c['fulfilled_needs'] = [{'need_id': 'geometry', 'accepted': False, 'evidence_ids': ['file1']}]
        payload, _ = choice.prepare(c, observation())
        self.assertNotIn('FINISH', payload['questions']['next_acquisition']['criteria'])
        c['fulfilled_needs'][0]['accepted'] = True
        payload, actions = choice.prepare(c, observation())
        self.assertFalse(actions)
        self.assertIn('FINISH', payload['questions']['next_acquisition']['criteria'])

    def test_ambiguous_generation_not_reoffered_without_recovery_evidence(self):
        c = context(); c['attempts'] = [{'candidate_id': 'p', 'outcome': 'ambiguous'}]
        o = observation(); o['candidates'].append({'id': 'spec', 'kind': 'page',
            'label': 'Observed specifications', 'url': 'https://www.misumi.com.cn/observed'})
        _, choices = choice.prepare(c, o)
        self.assertEqual({v['candidate_id'] for v in choices.values()}, {'spec'})
        c['attempts'][0]['recovery_basis'] = 'Prior request reconciled with independent evidence'
        _, choices = choice.prepare(c, o)
        self.assertEqual({v['candidate_id'] for v in choices.values()}, {'s', 'p', 'd', 'spec'})

    def test_service_failure_does_not_loop_through_other_formats(self):
        c = context(); c['attempts'] = [{'candidate_id': 'p', 'outcome': 'failed',
                                       'failure_scope': 'generation_service'}]
        _, choices = choice.prepare(c, observation())
        self.assertFalse(choices)

    def test_post_click_observation_error_preserves_generation_execution(self):
        class Agent:
            state = {'history': []}
            def command(self, *args):
                self.state['history'].append({'kind': 'click', 'action': '数据生成'})
                raise RuntimeError('post_click_observation_failed')
        result = {'generation_count': 0}
        with self.assertRaises(RuntimeError):
            acquire.execute_action(Agent(), {'kind': 'click', 'label': '数据生成'},
                                   {'fingerprint': 'old'}, result)
        self.assertEqual(result['generation_count'], 1)
        self.assertEqual(result['generation_state'], 'executed_awaiting_artifact')

    def test_part_identity_mismatch_and_duplicate_options_rejected(self):
        o = observation(); o['part'] = 'P2'
        with self.assertRaises(ValueError): choice.prepare(context(), o)
        o = observation(); o['candidates'].append(o['candidates'][0])
        with self.assertRaises(ValueError): choice.prepare(context(), o)

    def test_execution_uses_model_selected_DXF_not_STEP(self):
        ui = {'format_value': '1', 'format_options': [{**o, 'disabled': False} for o in observation()['candidates']]}
        page = {'actions': [{'kind': 'select', 'value': '1', 'label': 'STEP'},
                            {'kind': 'select', 'value': '5', 'label': 'DXF'},
                            {'kind': 'click', 'label': '数据生成'}]}
        selected = observation()['candidates'][2]
        result = acquire.scoped_actions(page, ui, selected)
        self.assertEqual(result['actions'], [page['actions'][1]])
        ui['format_value'] = '5'
        self.assertEqual(acquire.scoped_actions(page, ui, selected)['actions'], [page['actions'][2]])
        selected = {**selected, 'label': 'invented format'}
        with self.assertRaises(ValueError): acquire.scoped_actions(page, ui, selected)

    def test_timestamp_zip_AP214_step_suffix_and_old_file_baseline(self):
        data = archive('P1.step', STEP)
        with tempfile.TemporaryDirectory() as d:
            path = Path(d); old = path/'P1_STEP_20260926151235966.zip'; old.write_bytes(data)
            watch = artifacts.FreshSupplierDownload(path, 'P1', 'STEP')
            self.assertIsNone(watch.poll())
            new = path/'P1_STEP_20260926151245966.zip'; new.write_bytes(data)
            self.assertIsNone(watch.poll())
            receipt, _, _ = watch.poll()
            self.assertEqual(receipt['files'][0]['product'], 'P1_1')
            self.assertIn('AUTOMOTIVE_DESIGN', receipt['files'][0]['schema'])
            self.assertEqual(receipt['files'][0]['units'], 'unknown')

    def test_wrong_STEP_identity_html_path_traversal_and_format_rejected(self):
        for name, data, fmt in [('P1.step', STEP.replace(b'P1_1', b'P2_1'), 'STEP'),
                                ('P1.dxf', b'<html>Login</html>', 'DXF'),
                                ('../P1.step', STEP, 'STEP'),
                                ('P1.step', STEP, 'DXF')]:
            with self.assertRaises(ValueError):
                artifacts.inspect_download('P1.zip', archive(name, data), 'P1', fmt)

    def test_native_bytes_do_not_claim_geometry_validation(self):
        receipt, _ = artifacts.inspect_download('P1.zip', archive('P1.x_t', b'example native bytes'), 'P1', 'PARASOLID')
        self.assertEqual(receipt['files'][0]['format_check'], 'native_reader_required')
        self.assertEqual(receipt['engineering_acceptance'], 'not_evaluated')


if __name__ == '__main__':
    unittest.main()
