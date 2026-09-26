import sys
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'skills/cad-agent/scripts'))
import misumi_session as module


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.identity = {'schema': 'cad-agent.misumi-account-identity/v1',
                         'match_fields': {'email': 'engineer@example.com'}}
        self.ui = {'url': module.ACCOUNT_URL, 'title': '个人信息管理',
                   'account_fields': {'username': 'supplier-generated-id',
                                      'email': 'engineer@example.com'},
                   'login_form_visible': False}

    def test_exact_labeled_identity_not_profile_name_or_substring(self):
        good = module.classify(self.ui, self.identity)
        self.assertTrue(good['account_verified'])
        self.assertFalse(good['password_reauthentication_verified'])
        self.ui['account_fields']['email'] = 'other-engineer@example.com'
        bad = module.classify(self.ui, self.identity)
        self.assertEqual(bad['status'], 'different_account')
        self.assertNotIn('other-engineer@example.com', str(bad))
        self.assertNotIn('logout', str(bad))

    def test_same_text_on_unprotected_page_is_not_authentication(self):
        self.ui['url'] = module.ORIGIN+'/'
        self.assertFalse(module.classify(self.ui, self.identity)['account_verified'])
        self.ui['url'] = 'https://untrusted.example/my/user/manage/'
        self.assertEqual(module.classify(self.ui, self.identity)['status'], 'unexpected_origin')

    def test_loading_account_fields_cannot_pass(self):
        self.ui['account_fields'] = {'email': None}
        self.assertEqual(module.classify(self.ui, self.identity)['status'], 'account_state_unresolved')

    def test_password_form_is_recovery_candidate_not_success(self):
        self.ui.update(url=module.ORIGIN+'/mydesk2/s/login/', title='登录',
                       login_form_visible=True, autofill_ready=True)
        result = module.classify(self.ui, self.identity)
        self.assertEqual(result['status'], 'login_required')
        self.assertTrue(result['autofill_ready'])
        self.assertFalse(result['account_verified'])

    def test_challenge_and_access_denied_do_not_trigger_password_retry(self):
        self.ui['challenge_visible'] = True
        self.assertEqual(module.classify(self.ui, self.identity)['status'], 'verification_challenge')
        self.ui['challenge_visible'] = False
        self.ui['access_denied'] = True
        result = module.classify(self.ui, self.identity)
        self.assertEqual(result['status'], 'site_access_denied')
        self.assertNotIn('login', result['recovery'])

    def test_live_check_receipt_omits_identity_and_secrets(self):
        calls = []
        def fake(tab, expression, origin):
            calls.append((tab, origin)); return self.ui
        result = module.check(17, self.identity, max_seconds=1, evaluate=fake)
        self.assertTrue(result['account_verified'])
        self.assertEqual(calls, [(17, module.ORIGIN)])
        self.assertNotIn('engineer@example.com', str(result))
        self.assertFalse(result['browser_storage_accessed'])


if __name__ == '__main__':
    unittest.main()
