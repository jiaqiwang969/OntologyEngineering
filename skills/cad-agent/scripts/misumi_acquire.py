#!/usr/bin/env python3
"""Acquire one Jev-selected resource from an observed, configured MISUMI tab.

The caller verifies the account and observes the required resource controls. A subsequent invocation
receives the real receipt, failed attempts and remaining needs. No SKU/format
choice, engineering acceptance or repeated generation is hidden in this runner.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
import time

from supplier_acquisition_choice import ROOT, choose, digest, write
from supplier_artifacts import FreshSupplierDownload
from misumi_browser_download import FocusMonitor
from ontology_engineering import jev_browser
from ontology_engineering.chrome_apple_events import AppleEventsBrowser


def observe_supplier(browser):
    return browser.evaluate(r"""(() => {
      const visible=e=>e?.checkVisibility({checkOpacity:true,checkVisibilityCSS:true});
      const d=[...document.querySelectorAll('.new_cadDl')].find(visible);
      const f=d?.querySelector('#cad_format');
      const text=document.body.innerText;
      const part=new URL(location.href).searchParams.get('HissuCode');
      const displayed=text.match(/型号[：:]\s*([A-Za-z0-9_.-]+)\s*复制\s*重新选型/);
      const exact=part&&text.split(/[^A-Za-z0-9_.-]+/).includes(part);
      const specAt=text.search(/产品概述|规格概述|规格表|基本信息/);
      return {source_url:location.href,part,format_value:f?.value??null,
        configured:!!part&&((displayed&&displayed[1]===part)||
          (text.includes('型号已生成')&&exact)),displayed_part:displayed?.[1]??null,
        format_options:f?[...f.options].map(o=>({value:o.value,label:o.label,
          disabled:o.disabled||!!o.closest('optgroup[disabled]')})):[],
        dialog_text:d?.innerText??'',
        specification_excerpt:specAt>=0?text.slice(specAt,specAt+2500):null,
        observed_download_url:d?.querySelector('#cad_download_link')?.href??null,
        documents:[...document.querySelectorAll('a[href]')].filter(visible)
          .filter(e=>/\.(pdf|zip)(?:[?#]|$)/i.test(e.href)&&e.innerText.trim())
          .map(e=>({label:e.innerText.trim(),url:e.href}))};
    })()""")


def candidates(ui):
    rows = []
    for option in ui['format_options']:
        rows.append({'id': 'format_' + digest(option)[:16], 'kind': 'cad_format', **option,
                     'source': 'current_visible_cad_format_dropdown'})
    seen = set()
    for doc in ui['documents']:
        if doc['url'] in seen:
            continue
        seen.add(doc['url'])
        rows.append({'id': 'document_' + digest(doc)[:16], 'kind': 'document', **doc,
                     'scope': 'Observed supplier document; SKU applicability and contents not reviewed.'})
    if ui.get('specification_excerpt'):
        rows.append({'id': 'current_product_specification', 'kind': 'page',
                     'label': '当前商品页面的规格与说明', 'url': ui['source_url'],
                     'visible_excerpt': ui['specification_excerpt'],
                     'scope': 'Currently rendered supplier content; exact-model applicability still requires review.'})
    return rows


def execute_action(agent, action, page, result):
    if action['kind'] == 'click' and action['label'] == '数据生成':
        result['generation_state'] = 'intent_recorded_outcome_unconfirmed'
    try:
        agent.command('act', {'fingerprint': page['fingerprint']})
    finally:
        # Upstream commits execution before its post-click observation. A stale
        # readback must not turn an executed generation back into "not clicked".
        count = sum(h.get('kind') == 'click' and h.get('action') == '数据生成'
                    for h in agent.state['history'])
        result['generation_count'] = count
        if count:
            result['generation_state'] = 'executed_awaiting_artifact'


def scoped_actions(page, ui, selected):
    """Execution follows the actual Jev choice; no format is privileged here."""
    exact = [o for o in ui['format_options'] if not o['disabled'] and
             o['value'] == selected['value'] and o['label'] == selected['label']]
    if len(exact) != 1:
        raise ValueError('selected_option_changed_reobserve_required')
    ready = ui['format_value'] == selected['value']
    allowed = []
    for a in page['actions']:
        if a['kind'] == 'wait':
            allowed.append(a)
        elif not ready and a['kind'] == 'select' and a.get('value') == selected['value']:
            allowed.append(a)
        elif ready and a['kind'] == 'click' and a['label'] == '数据生成':
            allowed.append(a)
    return {**page, 'actions': allowed}


def run(task, context, directory, output):
    task = jev_browser.validate_task(task)
    if task['backend'] != 'chrome_apple_events':
        raise ValueError('authorized_existing_tab_required')
    part = context['object_identity']['part']
    output = Path(output).resolve()
    if output.is_relative_to(ROOT):
        raise ValueError('private_project_output_required')
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    write(output/'task.json', task)
    write(output/'context.json', context)
    result = {'status': 'interrupted_review_required', 'engineering_acceptance': 'not_evaluated',
              'backend': 'chrome_apple_events_dom', 'generation_count': 0,
              'source': jev_browser.source_identity()}
    monitor, agent, started = None, None, time.monotonic()
    try:
        browser = AppleEventsBrowser(task['url'], task)
        ui = observe_supplier(browser)
        write(output/'initial-ui.json', ui)
        if not ui['configured'] or ui['part'] != part:
            raise ValueError('configured_part_readback_required')
        observation = {'source_url': ui['source_url'], 'part': part,
                       'observation_id': digest(ui), 'candidates': candidates(ui)}
        if not observation['candidates']:
            raise ValueError('observe_required_resource_controls_before_acquisition')
        decision = choose(context, observation, output/'selection')
        result['selection'] = decision
        if decision['status'] != 'selected':
            result['status'] = 'selection_' + decision['status']
            return result
        selected = decision['candidate']
        if selected['kind'] != 'cad_format':
            result.update(status='observed_resource_handoff', next_resource=selected,
                          next_need=decision['need'])
            return result
        watch = FreshSupplierDownload(directory, part, selected['label'])
        write(output/'download-baseline.json', watch.before)
        transport = jev_browser.JevTransport(jev_browser.resolve_credential_file(None, skill_root=ROOT))
        execution_task = {**task, 'goal': (
            'Execute the recorded Jev acquisition choice for the confirmed part ' + part +
            '. Select the actual option labeled ' + selected['label'] + ' (observed value ' +
            selected['value'] + '), then click 数据生成 exactly once. Generation has not yet '
            'been requested. Do not choose DONE before the click. Do not change product or purchase. '
            'Purpose: ' + decision['need']['purpose'])}
        with jev_browser.upstream_binding(execution_task, transport) as factory:
            agent = factory(task['url'], execution_task['goal'], screenshots=False)
            browser = agent.browser
            raw_observe = browser.observe
            def bounded_observe(**kwargs):
                from jev_ultrafast.browser import fingerprint
                page = scoped_actions(raw_observe(**kwargs), observe_supplier(browser), selected)
                page['fingerprint'] = fingerprint(page)
                return page
            browser.observe = bounded_observe
            agent.state['page'] = browser.observe()
            monitor = FocusMonitor(); monitor.thread.start(); monitor.ready.wait(6)
            with (output/'events.jsonl').open('x') as journal:
                def event(data):
                    journal.write(json.dumps({'elapsed_seconds': round(time.monotonic()-started, 3),
                                              **data}, ensure_ascii=False) + '\n')
                    journal.flush(); os.fsync(journal.fileno())
                while time.monotonic()-started < task['max_seconds']:
                    found = watch.poll()
                    if found:
                        if result['generation_count'] != 1:
                            raise ValueError('file_before_owned_generation_ambiguous')
                        receipt, blob, members = found
                        (output/receipt['download_name']).write_bytes(blob)
                        target = output/'members'; target.mkdir(mode=0o700)
                        for name, data in members.items():
                            (target/name).write_bytes(data)
                        write(output/'receipt.json', receipt)
                        result.update(status='artifact_acquired', generation_state='artifact_acquired', artifact=receipt,
                                      need_id=decision['need_id'], acceptance='caller_review_required')
                        event({'phase': 'new_file_inspected', 'format': selected['label']})
                        break
                    if result['generation_count']:
                        time.sleep(.25)
                        continue
                    ui = observe_supplier(browser)
                    if not ui['configured'] or ui['part'] != part:
                        raise ValueError('part_changed_during_acquisition')
                    if len(agent.state['decisions']) >= task['max_decisions']:
                        raise ValueError('decision_budget_exhausted')
                    agent.state['page'] = browser.observe()
                    agent.command('predict')
                    choice = agent.state['decision']['choice']
                    if choice in {'DONE', 'BLOCKED'}:
                        result['status'] = 'model_stopped_before_generation'; break
                    page = agent.state['page']
                    action = next(a for a in page['actions'] if a['id'] == choice)
                    if len(agent.state['history']) >= task['max_actions']:
                        raise ValueError('action_budget_exhausted')
                    event({'phase': 'action_intent', 'kind': action['kind'], 'label': action['label']})
                    execute_action(agent, action, page, result)
                    event({'phase': 'action_returned', 'label': action['label']})
                    if action['label'] == '数据生成':
                        write(output/'generation-ui.json', observe_supplier(browser))
                else:
                    result['status'] = 'download_timeout' if result['generation_count'] else 'decision_budget_exhausted'
            write(output/'final-ui.json', observe_supplier(browser))
    except Exception as exc:
        result['error_type'] = type(exc).__name__
        if re.fullmatch(r'[a-zA-Z0-9_]+', str(exc)):
            result['error_code'] = str(exc)
    finally:
        if agent:
            write(output/'browser-observation.json', agent.snapshot())
            result['actions'] = agent.state['history']
        result['focus'] = monitor.finish() if monitor else {'status': 'no_browser_mutation'}
        result['elapsed_seconds'] = round(time.monotonic()-started, 3)
        write(output/'result.json', result)
    return result


def main():
    venv = ROOT/'runtime/jev-ultrafast/.venv'
    if Path(sys.prefix).resolve() != venv.resolve():
        python = venv/'bin/python'
        os.execv(str(python), [str(python), str(Path(__file__).resolve()), *sys.argv[1:]])
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--task', type=Path, required=True)
    p.add_argument('--context', type=Path, required=True)
    p.add_argument('--download-dir', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args(); os.umask(0o077)
    r = run(json.loads(a.task.read_text()), json.loads(a.context.read_text()), a.download_dir, a.output)
    summary = {k: r.get(k) for k in ('status', 'elapsed_seconds', 'error_code')}
    decision = r.get('selection', {})
    summary['selection'] = {k: decision.get(k) for k in
                            ('status', 'need_id', 'candidate_id', 'model', 'confidence')}
    if decision.get('candidate'):
        summary['selection']['label'] = decision['candidate']['label']
    print(json.dumps(summary, ensure_ascii=False))
    return int(r['status'] != 'artifact_acquired')


if __name__ == '__main__':
    raise SystemExit(main())
