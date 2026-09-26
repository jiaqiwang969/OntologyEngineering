"""Real upstream policy/loop with a fake page and fake model; never opens Chrome."""
from copy import deepcopy
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from ontology_engineering import jev_browser as tool


class PageFixture:
    target, session = "test-owned-tab", "test-session"

    def __init__(self, uncertain=False):
        self.value, self.writes, self.closed = "", 0, False
        self.uncertain = uncertain

    def observe(self, **_):
        return {"url": "https://example.org", "title": "Local fixture", "text": "Local fixture",
                "fingerprint": self.value or "initial", "actions": [
                    {"id": "e1", "node": 1, "kind": "fill", "label": "Search", "role": "textbox", "value": self.value},
                    {"id": "wait", "kind": "wait", "label": "Wait"}]}

    def fresh(self, *_):
        return True

    def act(self, action, page, text=None):
        self.writes += 1
        self.value = text
        if self.uncertain:
            raise RuntimeError("private error detail must not become a public error")

    def close(self):
        self.closed = True


class ModelFixture:
    def __init__(self):
        self.requests = []

    def __call__(self, payload):
        self.requests.append(deepcopy(payload))
        operation = "DONE" if payload["state"]["elements"][0]["value"] else "TYPE_TEXT"
        answers = {}
        for qid, question in payload["questions"].items():
            choice = operation if qid == "operation" else next(iter(question["criteria"]))
            answers[qid] = {"type": "choice", "choice": choice, "confidence": 1.0,
                            "probabilities": {key: float(key == choice) for key in question["criteria"]}}
        return {"model": payload["model"], "answers": answers,
                "usage": {"input_tokens": 1, "output_tokens": 1}}


class BrowserToolTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.task = tool.validate_task({"schema": tool.TASK_SCHEMA, "url": "https://example.org",
                                       "goal": "Search for the exact supplied term, then stop.",
                                       "text_values": {"Search": "fixture part 15"}})

    def execute(self, *, task=None, page=None):
        task, page = task or self.task, page or PageFixture()
        model = ModelFixture()
        with tool.upstream_binding(task, model) as factory:
            with patch("jev_ultrafast.agent.Browser", return_value=page):
                result = tool.execute(task, self.root / "run", factory, tool.source_identity())
        return result, page, model

    def test_real_upstream_loop_supplies_exact_text_and_does_not_accept_done(self):
        previous = {k: os.environ.get(k) for k in ("TYPESAFE_API_KEY", "TYPESAFE_MODEL")}
        result, page, model = self.execute()
        self.assertEqual(page.value, "fixture part 15")
        self.assertEqual(page.writes, 1)
        self.assertTrue(page.closed)
        self.assertEqual(result["status"], "reported_done")
        self.assertEqual(result["verification"], "required")
        self.assertEqual(result["engineering_acceptance"], "not_evaluated")
        self.assertEqual(len(model.requests), 2)
        self.assertEqual(previous, {k: os.environ.get(k) for k in previous})
        for path in (self.root / "run").iterdir():
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        observation = json.loads((self.root / "run/observation.json").read_text())
        self.assertEqual(observation["history"][0]["text_helper"], "caller-supplied-exact-value")

    def test_missing_field_value_stops_before_mutation(self):
        self.task["text_values"] = {}
        result, page, model = self.execute()
        self.assertEqual(result["status"], "field_value_required")
        self.assertEqual(result["field"], "Search")
        self.assertEqual(page.writes, 0)
        self.assertEqual(len(model.requests), 1)

    def test_uncertain_mutation_is_not_retried_and_has_durable_intent(self):
        result, page, model = self.execute(page=PageFixture(uncertain=True))
        self.assertEqual(page.writes, 1)
        self.assertEqual(len(model.requests), 1)
        self.assertEqual(result["status"], "interrupted_review_required")
        self.assertNotIn("private error detail", json.dumps(result))
        events = [json.loads(line) for line in (self.root / "run/events.jsonl").read_text().splitlines()]
        self.assertEqual([e["phase"] for e in events], ["opening_owned_background_tab", "tick_started", "stopped"])

    def test_initial_identity_text_mismatch_prevents_model_and_action(self):
        self.task["required_initial_text"] = ["required account marker"]
        result, page, model = self.execute()
        self.assertEqual(result["status"], "initial_observation_mismatch")
        self.assertEqual(page.writes, 0)
        self.assertEqual(model.requests, [])

    def test_budget_exhaustion_preserves_unfinished_status(self):
        self.task["max_actions"] = 1
        result, page, model = self.execute()
        self.assertEqual(result["status"], "budget_exhausted")
        self.assertEqual(result["verification"], "not_run")
        self.assertEqual(page.writes, 1)
        self.assertEqual(len(model.requests), 1)

    def test_reused_output_never_opens_another_browser(self):
        (self.root / "run").mkdir()
        with self.assertRaises(FileExistsError):
            tool.execute(self.task, self.root / "run", lambda *_args, **_kwargs: self.fail("browser opened"), {})

    def test_keep_open_retains_only_owned_fixture_tab(self):
        self.task["keep_open"] = True
        result, page, _ = self.execute()
        self.assertFalse(page.closed)
        self.assertEqual(result["tab"], "retained")
        self.assertEqual(json.loads((self.root / "run/session.json").read_text())["target"], page.target)

    def test_missing_connection_cannot_auto_launch_or_query_model(self):
        model = ModelFixture()
        with patch("browser_harness.admin.require_existing_daemon", side_effect=RuntimeError("not connected")):
            with tool.upstream_binding(self.task, model) as factory:
                with patch("browser_harness.admin.ensure_daemon", side_effect=AssertionError("automatic startup")):
                    result = tool.execute(self.task, self.root / "run", factory, tool.source_identity())
        self.assertEqual(result["status"], "browser_connection_required")
        self.assertEqual(model.requests, [])

    def test_source_tamper_is_rejected_before_import(self):
        runtime = self.root / "source"
        shutil.copytree(tool.RUNTIME / "upstream", runtime / "upstream", ignore=shutil.ignore_patterns("__pycache__"))
        for name in ("source-lock.json", "requirements.txt"):
            shutil.copy2(tool.RUNTIME / name, runtime / name)
        (runtime / "upstream/jev_ultrafast/model.py").write_text("raise RuntimeError('changed')")
        with self.assertRaisesRegex(ValueError, "browser_source_mismatch"):
            tool.source_identity(runtime)

    def test_doctor_never_imports_browser_harness(self):
        code = """
import sys
class RejectBrowser:
    def find_spec(self, fullname, path=None, target=None):
        if fullname.startswith(('browser_harness', 'jev_ultrafast')):
            raise RuntimeError('doctor must not import browser code')
sys.meta_path.insert(0, RejectBrowser())
from ontology_engineering.jev_browser import doctor
assert doctor()['status'] == 'local_runtime_ready'
"""
        completed = subprocess.run([sys.executable, "-c", code], cwd=tool.ROOT, capture_output=True, text=True)
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_observation_failure_still_closes_owned_tab_and_records_failure(self):
        from jev_ultrafast.agent import Agent
        with patch.object(Agent, "snapshot", side_effect=ValueError("unreadable snapshot")):
            result, page, _ = self.execute()
        self.assertTrue(page.closed)
        self.assertEqual(result["status"], "interrupted_review_required")
        self.assertEqual(json.loads((self.root / "run/result.json").read_text())["verification"], "not_run")


if __name__ == "__main__":
    unittest.main()
