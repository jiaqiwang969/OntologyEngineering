#!/usr/bin/env python3
"""Verify the supplier account in an owned Chrome tab without reading stores.

Run before product configuration. A live account page proves session reuse, not
password reauthentication. Login recovery is handed back with the observed form
state; passwords, cookies and autofilled password values never enter receipts.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import stat
import sys
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from ontology_engineering.chrome_apple_events import evaluate_tab

ORIGIN = 'https://www.misumi.com.cn'
ACCOUNT_URL = ORIGIN + '/my/user/manage/'

OBSERVE = r"""(() => {
  const visible=e=>e?.checkVisibility({checkOpacity:true,checkVisibilityCSS:true});
  const value=id=>{const e=document.getElementById(id);return visible(e)?e.innerText.trim():null};
  const inputs=[...document.querySelectorAll('input')].filter(visible);
  const pw=inputs.filter(e=>e.type==='password');
  const ids=inputs.filter(e=>['text','email','tel'].includes(e.type)&&
    !/search|搜索/i.test([e.name,e.id,e.placeholder].join(' ')));
  const challenge=[...document.querySelectorAll(
    'iframe[title*="reCAPTCHA"],iframe[title*="hCaptcha"],input[autocomplete="one-time-code"],input[name*="captcha"]'
  )].some(visible);
  return {url:location.origin+location.pathname,title:document.title,
    ready:document.readyState,
    account_fields:{username:value('userName'),email:value('userEmail')},
    login_form_visible:pw.length===1&&/\/login\/?$/.test(location.pathname),
    autofill_ready:pw.length===1&&!!pw[0].value&&ids.length===1&&!!ids[0].value,
    challenge_visible:challenge,access_denied:document.title==='Access Denied'};
})()"""


def validate_identity(identity):
    if identity.get('schema') != 'cad-agent.misumi-account-identity/v1':
        raise ValueError('identity_schema_required')
    fields = identity.get('match_fields')
    if not isinstance(fields, dict) or not fields or set(fields)-{'username', 'email'}:
        raise ValueError('exact_account_match_fields_required')
    if any(not isinstance(v, str) or not v.strip() for v in fields.values()):
        raise ValueError('nonempty_identity_values_required')
    return fields


def classify(ui, identity):
    expected = validate_identity(identity)
    result = {'status': 'account_state_unresolved', 'account_verified': False,
              'password_reauthentication_verified': False,
              'credential_values_recorded': False,
              'recovery': 'observe_current_page'}
    if ui.get('access_denied'):
        result.update(status='site_access_denied', recovery='diagnose_access_path')
        return result
    if ui.get('challenge_visible'):
        result.update(status='verification_challenge', recovery='normal_site_verification_required')
        return result
    u = urlsplit(ui.get('url', ''))
    if u.scheme != 'https' or u.netloc != 'www.misumi.com.cn':
        result.update(status='unexpected_origin', recovery='reobserve_bound_tab')
        return result
    if u.path.rstrip('/') == '/my/user/manage' and ui.get('title') == '个人信息管理':
        fields = ui.get('account_fields', {})
        if all(isinstance(fields.get(k), str) and fields[k].strip() for k in expected):
            matches = {k: (fields[k].strip().casefold() == v.strip().casefold()
                           if k == 'email' else fields[k].strip() == v.strip())
                       for k, v in expected.items()}
            ok = all(matches.values())
            result.update(status='session_ready' if ok else 'different_account',
                          account_verified=ok, exact_field_matches=matches,
                          recovery='reuse_browser_session' if ok else 'resolve_account_identity')
            return result
    if ui.get('login_form_visible'):
        result.update(status='login_required', autofill_ready=bool(ui.get('autofill_ready')),
                      recovery='inspect_and_submit_normal_login_form' if ui.get('autofill_ready')
                      else 'use_authorized_local_credentials_or_browser_autofill')
    return result


def check(tab_id, identity, *, navigate=False, max_seconds=40, evaluate=evaluate_tab):
    validate_identity(identity)
    if not 1 <= max_seconds <= 60:
        raise ValueError('bounded_wait_required')
    if navigate:
        from misumi_browser_download import focus_context
        if focus_context().get('app') in {'Google Chrome', 'com.google.Chrome', 'observation_failed'}:
            raise ValueError('background_owned_tab_required')
        evaluate(tab_id, "(()=>{location.assign("+json.dumps(ACCOUNT_URL)+
                 ");return {navigation:'account_preflight'}})()", ORIGIN)
    deadline = time.monotonic()+max_seconds
    result = {'status': 'account_state_unresolved', 'account_verified': False}
    while time.monotonic() < deadline:
        try:
            result = classify(evaluate(tab_id, OBSERVE, ORIGIN), identity)
        except (RuntimeError, json.JSONDecodeError):
            # Navigation may temporarily replace the page. No error text or
            # script source, potentially containing identity, is recorded.
            result = {'status': 'account_observation_unavailable', 'account_verified': False}
        if result['status'] not in {'account_state_unresolved', 'account_observation_unavailable'}:
            break
        time.sleep(.5)
    result.update(tab_id=tab_id, checked_at=datetime.now(timezone.utc).isoformat(),
                  evidence_kind='live_normal_account_DOM', browser_storage_accessed=False)
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--tab-id', type=int, required=True)
    p.add_argument('--identity', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--navigate-account', action='store_true')
    a = p.parse_args()
    if stat.S_IMODE(a.identity.stat().st_mode) & 0o077:
        raise ValueError('private_identity_file_permissions_required')
    if a.output.resolve().is_relative_to(ROOT):
        raise ValueError('private_project_receipt_required')
    result = check(a.tab_id, json.loads(a.identity.read_text()), navigate=a.navigate_account)
    fd = os.open(a.output, os.O_CREAT|os.O_EXCL|os.O_WRONLY, 0o600)
    with os.fdopen(fd, 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=2); f.write('\n')
    print(json.dumps(result, ensure_ascii=False))
    return int(not result['account_verified'])


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError) as exc:
        # Never echo arbitrary file contents or browser/OS exceptions.
        print(json.dumps({'status': 'preflight_failed', 'error_type': type(exc).__name__}))
        raise SystemExit(2)
