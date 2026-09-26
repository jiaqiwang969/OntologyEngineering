#!/usr/bin/env python3
"""Bounded MISUMI configured-page download, using an existing authorized tab.

Requires the account and parameters to have been verified by the caller. Never
logs in, creates tabs, changes Chrome settings, purchases, or guesses CAD APIs.
Jev chooses observed controls; fixed policy is a matched baseline for comparison.
"""
from __future__ import annotations

import argparse
from contextlib import nullcontext
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import threading
import time
from urllib.parse import parse_qs, urlsplit
import zipfile

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / 'runtime/jev-ultrafast/upstream'))
from ontology_engineering import jev_browser
from ontology_engineering.chrome_apple_events import AppleEventsBrowser


def stamp(path):
    s = path.stat()
    return [s.st_size, s.st_mtime_ns, s.st_ino]


class FreshStepDownload:
    def __init__(self, directory, part):
        if not re.fullmatch(r'[A-Za-z0-9_.-]{1,120}', part):
            raise ValueError('simple_confirmed_part_number_required')
        self.directory, self.part = Path(directory).expanduser().resolve(), part
        self.pattern = re.compile(re.escape(part) + r'_STEP(?: \(\d+\))?\.zip$', re.I)
        self.before = {str(p): stamp(p) for p in self.candidates()}
        self.last = {}

    def candidates(self):
        return [p for p in self.directory.iterdir() if p.is_file() and self.pattern.fullmatch(p.name)]

    def poll(self):
        fresh = [p for p in self.candidates() if self.before.get(str(p)) != stamp(p)]
        if len(fresh) > 1:
            raise ValueError('multiple_new_matching_downloads_ambiguous')
        for p in fresh:
            state = stamp(p)
            if Path(str(p) + '.crdownload').exists() or self.last.get(str(p)) != state:
                self.last[str(p)] = state
                return None
            blob = p.read_bytes()
            if stamp(p) != state:
                return None
            receipt, step = verify_step_archive(blob, self.part)
            return {**receipt, 'original_download': str(p), 'file_stamp': state}, blob, step
        return None


def verify_step_archive(blob, part):
    if len(blob) > 20_000_000:
        raise ValueError('archive_too_large')
    with zipfile.ZipFile(io.BytesIO(blob)) as z:
        members = z.infolist()
        if len(members) != 1:
            raise ValueError('expected_one_part_STEP')
        member = members[0]
        if member.filename != part + '.stp' or member.file_size > 20_000_000:
            raise ValueError('part_or_member_path_mismatch')
        if z.testzip() is not None:
            raise ValueError('archive_crc_failure')
        step = z.read(member)
    if not (step.lstrip().startswith(b'ISO-10303-21;') and step.rstrip().endswith(b'END-ISO-10303-21;')):
        raise ValueError('incomplete_STEP')
    if not re.search(rb"PRODUCT\s*\(\s*'" + re.escape(part.encode()) + rb"'\s*,", step):
        raise ValueError('STEP_product_identity_mismatch')
    if not re.search(rb"FILE_SCHEMA\s*\(\s*\(\s*'CONFIG_CONTROL_DESIGN'", step):
        raise ValueError('STEP_AP203_schema_missing')
    if not re.search(rb'SI_UNIT\s*\(\s*\.MILLI\.\s*,\s*\.METRE\.\s*\)', step):
        raise ValueError('STEP_millimetre_unit_not_confirmed')
    return {'part_number': part, 'format': 'STEP AP203', 'units': 'mm', 'zip_crc_valid': True,
            'archive_bytes': len(blob), 'archive_sha256': hashlib.sha256(blob).hexdigest(),
            'step_bytes': len(step), 'step_sha256': hashlib.sha256(step).hexdigest()}, step


def download_observed_url(url, part, output):
    """Fetch only the ordinary public file link actually returned by this page."""
    parsed=urlsplit(url)
    if (parsed.scheme!='https' or parsed.netloc!='rd-cn-caddata.misumi.com.cn' or
        parsed.query or parsed.fragment or not parsed.path.startswith('/prod/CadSupplyGC/') or
        not parsed.path.endswith('/STEP_AP203/'+part+'_STEP.zip') or '..' in parsed.path.split('/')):
        raise ValueError('observed_supplier_file_URL_not_supported')
    pending=Path(output)/'pending-supplier-download.zip'
    if pending.exists():
        raise ValueError('download_destination_already_exists')
    p=subprocess.run(['curl','--proto','=https','--fail','--silent','--show-error',
        '--max-time','15','--max-filesize','20000000','--retry','2','--retry-delay','1',
        '--retry-max-time','45','--retry-all-errors','--output',str(pending),
        '--write-out','%{http_code}\t%{num_retries}\t%{time_total}', '--',url],
        capture_output=True,text=True,timeout=65)
    if p.returncode:
        raise RuntimeError('supplier_file_transfer_unconfirmed')
    code,retries,seconds=p.stdout.strip().split('\t')
    if code!='200':
        raise ValueError('supplier_file_HTTP_not_200')
    blob=pending.read_bytes()
    receipt,step=verify_step_archive(blob,part)
    pending.rename(Path(output)/(part+'_STEP.zip'))
    return {**receipt,'observed_download_url':url,'delivery':'observed_page_link_https',
            'HTTP_status':int(code),'transfer_retries':int(retries),'transfer_seconds':float(seconds)},blob,step


def frontmost():
    script = 'tell application "System Events" to return bundle identifier of first application process whose frontmost is true'
    p = subprocess.run(['osascript', '-'], input=script, text=True, capture_output=True, timeout=5)
    return p.stdout.strip() if p.returncode == 0 else 'observation_failed'


def focus_context():
    script='''tell application "System Events" to set fg to bundle identifier of first application process whose frontmost is true
if fg is "com.google.Chrome" then
 tell application "Google Chrome" to return fg & "|" & (id of front window as text) & "|" & (id of active tab of front window as text)
end if
return fg'''
    p=subprocess.run(['osascript','-'],input=script,text=True,capture_output=True,timeout=5)
    if p.returncode:return {'app':'observation_failed'}
    parts=p.stdout.strip().split('|')
    return {'app':parts[0],**({'chrome_window_id':parts[1],'chrome_tab_id':parts[2]} if len(parts)==3 else {})}


class FocusMonitor:
    def __init__(self):
        self.stop = threading.Event()
        self.ready = threading.Event()
        self.samples, self.changes, self.failures = 0, [], 0
        self.thread = threading.Thread(target=self.loop, daemon=True)

    def loop(self):
        started, previous = time.monotonic(), None
        while not self.stop.is_set():
            try:
                value = focus_context()
            except Exception:
                value = {'app':'observation_failed'}
            self.samples += 1
            self.ready.set()
            self.failures += value['app'] == 'observation_failed'
            if value != previous:
                self.changes.append({'elapsed_seconds': round(time.monotonic()-started, 3), **value})
                previous = value
            self.stop.wait(0.1)

    def finish(self):
        self.stop.set()
        self.thread.join(timeout=6)
        return {'samples': self.samples, 'transitions': self.changes, 'failed_samples': self.failures,
                'unchanged_in_samples': len(self.changes) == 1 and self.failures == 0,
                'sampling_limit': '100 ms target interval plus OS query latency; not continuous event proof'}


def page_status(browser):
    return browser.evaluate("""(() => {
      const d=document.querySelector('.new_cadDl'), f=document.querySelector('#cad_format');
      const link=document.querySelector('#cad_download_link');
      return {popup:!!d, format:f?.value??null, text:d?.innerText??'',
        configured:document.body.innerText.includes('型号已生成'),
        part:new URL(location.href).searchParams.get('HissuCode'),
        observed_download_url:link?.href??null};
    })()""")


def fixed_choice(page, ui):
    if ui['format']:
        if ui['format'] != 'STEP_AP203':
            return next(a for a in page['actions'] if a['kind'] == 'select' and a['value'] == 'STEP_AP203')
        return next(a for a in page['actions'] if a['kind'] == 'click' and a['label'] == '数据生成')
    return next(a for a in page['actions'] if a['kind'] == 'click' and a['label'].startswith('CAD下载'))


def scoped_actions(page, ui):
    """Expose only transitions whose observed preconditions hold, to both policies."""
    controls=[]
    for action in page['actions']:
        if ui['format']:
            if ui['format']!='STEP_AP203':
                enabled=action['kind']=='select' and action.get('value')=='STEP_AP203'
            else:
                enabled=action['kind']=='click' and action['label']=='数据生成'
        else:
            enabled=not ui['popup'] and action['kind']=='click' and action['label'].startswith('CAD下载')
        if enabled:controls.append(action)
    # A model may wait/stop. It cannot bypass the format prerequisite or toggle
    # an already-open dialog instead of progressing the current subtask.
    available=controls+[a for a in page['actions'] if a['kind']=='wait' or (not controls and a['kind']=='scroll')]
    return {**page,'actions':available}


def run(task, directory, part, output, policy='jev', delivery='browser'):
    task = jev_browser.validate_task(task)
    if task['backend'] != 'chrome_apple_events' or urlsplit(task['url']).hostname != 'www.misumi.com.cn':
        raise ValueError('existing_MISUMI_background_tab_required')
    if parse_qs(urlsplit(task['url']).query).get('HissuCode') != [part]:
        raise ValueError('configured_URL_part_mismatch')
    if delivery not in {'browser','observed-url'}:
        raise ValueError('unsupported_delivery_mode')
    output = Path(output).resolve()
    if output.is_relative_to(ROOT):
        raise ValueError('use_private_project_output')
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    write = lambda name, value: jev_browser.write_json(output/name, value)
    write('task.json', task)
    identity = jev_browser.source_identity()
    transport = None
    if policy == 'jev':
        transport = jev_browser.JevTransport(jev_browser.resolve_credential_file(None, skill_root=ROOT))
    binding = jev_browser.upstream_binding(task, transport) if policy == 'jev' else nullcontext(None)
    result = {'status': 'interrupted_review_required', 'policy': policy, 'backend': 'chrome_apple_events_dom',
              'engineering_acceptance': 'not_evaluated', 'source': identity, 'human_interventions': 0,
              'account_preflight': 'caller_required; this runner never authenticates', 'delivery':delivery}
    started, browser, agent, monitor, actions = None, None, None, None, []
    try:
        with binding as factory:
            if factory:
                agent = factory(task['url'], task['goal'], screenshots=False)
                browser, page = agent.browser, agent.state['page']
            else:
                browser = AppleEventsBrowser(task['url'], task)
                page = browser.observe()
            raw_observe=browser.observe
            def workflow_observe(**kwargs):
                from jev_ultrafast.browser import fingerprint
                observation=scoped_actions(raw_observe(**kwargs),page_status(browser))
                observation['fingerprint']=fingerprint(observation)
                return observation
            browser.observe=workflow_observe
            page=browser.observe()
            if agent:agent.state['page']=page
            if not all(t in page['text'] for t in task['required_initial_text']):
                raise ValueError('initial_part_observation_mismatch')
            ui = page_status(browser)
            if not ui['configured'] or ui['part'] != part:
                raise ValueError('configuration_not_complete')
            watch = FreshStepDownload(directory, part)
            write('download-baseline.json', watch.before)
            write('initial-page.json', page)
            monitor = FocusMonitor(); monitor.thread.start()
            if not monitor.ready.wait(timeout=6):
                raise RuntimeError('foreground_monitor_unavailable')
            started = time.monotonic()
            generated = False
            with (output/'events.jsonl').open('x') as journal:
                def event(data):
                    journal.write(json.dumps({'elapsed_seconds':round(time.monotonic()-started,3),**data},ensure_ascii=False)+'\n')
                    journal.flush(); os.fsync(journal.fileno())
                while time.monotonic()-started < task['max_seconds']:
                    found = watch.poll() if delivery=='browser' else None
                    if generated and delivery=='observed-url':
                        ui=page_status(browser)
                        if ui['observed_download_url'] and '成功生成CAD数据' in ui['text']:
                            event({'phase':'fetching_current_observed_supplier_link'})
                            found=download_observed_url(ui['observed_download_url'],part,output)
                    if found:
                        if not generated:
                            raise ValueError('new_file_before_owned_generation_ambiguous')
                        receipt, archive, step = found
                        if delivery=='browser':
                            (output/(part+'_STEP.zip')).write_bytes(archive)
                        (output/(part+'.stp')).write_bytes(step)
                        ui = page_status(browser)
                        receipt['observed_download_url'] = ui['observed_download_url']
                        write('download-receipt.json', receipt)
                        result.update(status='download_verified', artifact=receipt)
                        event({'phase':'independent_file_verification_passed'})
                        break
                    if generated:
                        # A confirmed generation is never clicked again, even if the UI stays unchanged.
                        time.sleep(0.25)
                        continue
                    ui = page_status(browser)
                    if '没有您指定条件的CAD' in ui['text']:
                        result['status'] = 'supplier_reported_no_cad'
                        break
                    if ui['popup'] and not ui['format']:
                        time.sleep(0.25)
                        continue
                    if not ui['configured'] or ui['part'] != part:
                        raise ValueError('part_changed_during_download')
                    if agent:
                        if len(agent.state['decisions']) >= task['max_decisions']:
                            result['status']='decision_budget_exhausted'; break
                        agent.state['page'] = browser.observe()
                        event({'phase':'decision_started'})
                        agent.command('predict')
                        decision=agent.state['decision']
                        if decision['choice'] in {'DONE','BLOCKED'}:
                            result['status']='model_stopped_without_verified_download'; break
                        page=agent.state['page']
                        action=next(a for a in page['actions'] if a['id']==decision['choice'])
                    else:
                        page=browser.observe(); action=fixed_choice(page,ui)
                    if len(actions) >= task['max_actions']:
                        result['status']='action_budget_exhausted'; break
                    if action['label']=='数据生成' and ui['format']!='STEP_AP203':
                        raise ValueError('format_guard_rejected_generation')
                    if action['label']=='数据生成' and ui['observed_download_url']:
                        raise ValueError('prior_generation_link_must_be_cleared_before_new_run')
                    event({'phase':'action_intent','kind':action['kind'],'label':action['label']})
                    # Preserve ambiguous outcomes; no automatic replay after an exception.
                    if agent:
                        agent.command('act', {'fingerprint':page['fingerprint']})
                    else:
                        browser.act(action,page)
                    actions.append({'kind':action['kind'],'label':action['label']})
                    event({'phase':'action_returned','label':action['label']})
                    generated=action['label']=='数据生成'
                    if generated:
                        event({'phase':'waiting_for_new_download_no_reclick' if delivery=='browser'
                               else 'waiting_for_current_generated_link_no_reclick'})
                else:
                    result['status']='download_timeout' if generated else 'browser_budget_exhausted'
            if agent:
                write('observation.json',agent.snapshot())
                result['model_calls']=len(agent.state['decisions'])
                result['usage']={k:sum(d.get('usage',{}).get(k,0) for d in agent.state['decisions'])
                                 for k in ('input_tokens','output_tokens')}
            else:
                write('observation.json',browser.observe()); result.update(model_calls=0,usage=None)
    except Exception as exc:
        result.update(error_type=type(exc).__name__)
        if re.fullmatch(r'[a-z0-9_]+',str(exc)):
            result['error_code']=str(exc)
        import traceback
        result['error_site']=[{'file':Path(f.filename).name,'line':f.lineno,'function':f.name}
                              for f in traceback.extract_tb(exc.__traceback__)[-4:]]
    finally:
        if agent is not None:
            if not (output/'observation.json').exists():
                write('observation.json',agent.snapshot())
            result['model_calls']=len(agent.state['decisions'])
            result['usage']={k:sum(d.get('usage',{}).get(k,0) for d in agent.state['decisions'])
                             for k in ('input_tokens','output_tokens')}
        result['elapsed_seconds']=round(time.monotonic()-started,3) if started else None
        result['actions']=actions
        result['focus']=monitor.finish() if monitor else {'status':'not_started'}
        result['tab']='retained_existing_tab'
        result['settings_changed_by_runner']=False
        result['ui_retry_count']=0
        result['transfer_retry_count']=result.get('artifact',{}).get('transfer_retries',0) if delivery=='observed-url' else None
        write('result.json',result)
    return result


def main():
    venv=ROOT/'runtime/jev-ultrafast/.venv'
    python=venv/'bin/python'
    if python.is_file() and Path(sys.prefix).resolve()!=venv.resolve():
        os.execv(str(python),[str(python),str(Path(__file__).resolve()),*sys.argv[1:]])
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--task',type=Path,required=True)
    parser.add_argument('--download-dir',type=Path,required=True)
    parser.add_argument('--part',required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--policy',choices=['jev','fixed'],default='jev')
    parser.add_argument('--delivery',choices=['browser','observed-url'],default='browser')
    args=parser.parse_args();os.umask(0o077)
    result=run(json.loads(args.task.read_text()),args.download_dir,args.part,args.output,args.policy,args.delivery)
    print(json.dumps({k:result.get(k) for k in ('status','policy','elapsed_seconds','model_calls','error_type')},ensure_ascii=False))
    return 0 if result['status']=='download_verified' else 1


if __name__=='__main__':
    raise SystemExit(main())
