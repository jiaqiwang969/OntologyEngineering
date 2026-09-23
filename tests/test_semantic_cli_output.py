"""A native progress message must not corrupt the public JSON protocol."""
from contextlib import redirect_stdout, redirect_stderr
import io
import json
from unittest import mock

from ontology_engineering import semantic_engagement as engagement


def test_progress_is_preserved_on_stderr():
    stdout, stderr = io.StringIO(), io.StringIO()
    def noisy(_):
        print("native reasoning progress")
        return {"command_verdict": "passed", "fixture": True}
    with mock.patch.object(engagement, "_dispatch", side_effect=noisy), redirect_stdout(stdout), redirect_stderr(stderr):
        assert engagement.main(["doctor"]) == 0
    assert json.loads(stdout.getvalue()) == {"command_verdict": "passed", "fixture": True}
    assert "native reasoning progress" in stderr.getvalue()


def test_progress_then_failure_still_returns_one_json_response():
    stdout, stderr = io.StringIO(), io.StringIO()
    def noisy_failure(_):
        print("native reasoning progress")
        raise RuntimeError("controlled test failure")
    with mock.patch.object(engagement, "_dispatch", side_effect=noisy_failure), redirect_stdout(stdout), redirect_stderr(stderr):
        assert engagement.main(["doctor"]) == 0
    assert json.loads(stdout.getvalue())["command_verdict"] == "blocked"
    assert "native reasoning progress" in stderr.getvalue()
