"""Operational routing must preserve uncertainty and never dispatch model output."""
from copy import deepcopy
import json
import subprocess
import sys

import pytest

from ontology_engineering import context_routing as routing
from ontology_engineering.jev_transport import TransportError


def test_routing_import_works_without_semantica_or_site_packages():
    code = """
import sys
class RejectSemantica:
    def find_spec(self, fullname, path=None, target=None):
        if fullname == 'semantica' or fullname.startswith('semantica.'):
            raise RuntimeError('routing must not import a semantic backend')
sys.meta_path.insert(0, RejectSemantica())
from ontology_engineering.context_routing import prepare, INPUT_SCHEMA
result = prepare({'schema': INPUT_SCHEMA, 'task_id': 'bootstrap', 'context': '', 'request': '准备接入'})
assert result['payload']['questions']
"""
    result = subprocess.run([sys.executable, "-S", "-c", code], cwd=routing.ROOT, capture_output=True, text=True)
    assert result.returncode == 0, result.stderr


def test_public_semantic_exports_still_resolve_to_single_adapter():
    import ontology_engineering
    from ontology_engineering import semantica_runtime
    assert ontology_engineering.verify_runtime_source_identity is semantica_runtime.verify_runtime_source_identity


def prepared():
    return routing.prepare({"schema": routing.INPUT_SCHEMA, "task_id": "synthetic-task",
                            "context": "仿真已明确停止，现有记录可读。", "request": "分析假设影响。"})


def response(payload, choice="needed", *, omit=()):
    return {"model": payload["model"], "usage": {"input_tokens": 10, "output_tokens": 5},
            "answers": {qid: {"type": "choice", "choice": choice,
                              "probabilities": {key: float(key == choice) for key in q["criteria"]},
                              "confidence": 1.0}
                        for qid, q in payload["questions"].items() if qid not in omit}}


class Fixture:
    kind = "fixture"

    def __init__(self, fn):
        self.fn, self.calls = fn, []

    def __call__(self, payload):
        self.calls.append(deepcopy(payload))
        return self.fn(payload)


def test_context_and_real_sources_are_bound_without_paths_in_model_payload():
    item = prepared()
    assert "停止" in item["payload"]["state"]["context"]
    assert all(source["sha256"] for source in item["sources"] if source["path"])
    assert str(routing.ROOT) not in json.dumps(item["payload"])
    assert item["identity"]["request_sha256"] == routing.digest(item["payload"])
    assert item["sources"][-1]["availability"] == "requires_session_discovery"


def test_every_question_receives_the_frozen_instruction_and_its_own_capability():
    item = prepared()
    raw = (routing.ROOT / "references/context-routing-instructions.json").read_bytes()
    specification = json.loads(raw)
    assert item["identity"]["instruction_sha256"] == routing.digest(raw)
    for qid, question in item["payload"]["questions"].items():
        actual = deepcopy(question["instructions"])
        capability = actual.pop("this_capability")
        assert capability["id"] == qid
        assert actual == specification["instructions"]
    first, second = list(item["payload"]["questions"].values())[:2]
    first["instructions"]["this_capability"]["capability"] = "changed caller copy"
    assert second["instructions"]["this_capability"]["capability"] != "changed caller copy"


def test_missing_source_keeps_needed_candidate_and_no_execution(tmp_path):
    item = prepared()
    item["sources"][0].update(availability="source_missing", sha256=None)
    fake = Fixture(response)
    result = routing.run(item, tmp_path / "run", transport=fake)
    assert result["status"] == "completed" and result["execution_kind"] == "fixture"
    assert "cad" in result["candidate_routes"]
    assert result["routes"][0]["source"]["availability"] == "source_missing"
    assert all(r["execution"] == "not_started" for r in result["routes"])
    assert result["automatic_tool_dispatch"] is False and result["fact_admission"] is False


def test_partial_retry_preserves_first_answers_and_only_retries_missing(tmp_path):
    fake = Fixture(lambda payload: response(payload, "needed", omit=("cad",)) if len(payload["questions"]) > 1
                   else response(payload, "not_needed"))
    result = routing.run(prepared(), tmp_path / "run", transport=fake, sleeper=lambda _: None)
    assert len(fake.calls) == 2 and set(fake.calls[1]["questions"]) == {"cad"}
    assert result["status"] == "completed" and "cad" not in result["candidate_routes"]
    assert result["candidate_routes"]
    assert result["usage"]["input_tokens"] == 20
    assert (tmp_path / "run/request-1.json").exists()
    assert (tmp_path / "run/response-2.json").exists()


def test_unknown_answer_is_distinct_from_service_failure(tmp_path):
    unknown = routing.run(prepared(), tmp_path / "unknown", transport=Fixture(lambda p: response(p, "not_established")))
    assert unknown["status"] == "completed" and not unknown["candidate_routes"]
    assert all(r["model_status"] == "answered" for r in unknown["routes"])
    def timeout(_):
        raise TransportError("connection_or_timeout", retryable=True)
    fake = Fixture(timeout)
    failed = routing.run(prepared(), tmp_path / "failed", transport=fake, sleeper=lambda _: None)
    assert len(fake.calls) == 2 and failed["status"] == "unavailable"
    assert all(r["model_status"] == "not_answered" and r["need"] == "not_established" for r in failed["routes"])
    assert failed["usage"]["unreported_attempts"] == 2


def test_missing_credential_produces_explicit_unavailable_report(tmp_path, monkeypatch):
    def absent(*args, **kwargs):
        raise ValueError("jev_credential_missing")
    monkeypatch.setattr(routing, "resolve_credential_file", absent)
    result = routing.run(prepared(), tmp_path / "run")
    assert result["status"] == "unavailable" and result["attempts"] == 0
    assert result["transport_errors"] == ["credential_unavailable"]


def test_model_identity_mismatch_cannot_supply_candidates(tmp_path):
    def wrong_model(payload):
        value = response(payload); value["model"] = "jev-99.0.0"; return value
    result = routing.run(prepared(), tmp_path / "run", transport=Fixture(wrong_model))
    assert result["status"] == "unavailable" and not result["candidate_routes"]
    assert result["transport_errors"] == ["resolved_model_mismatch"]


def test_existing_output_is_not_overwritten_or_reissued(tmp_path):
    output = tmp_path / "run"; output.mkdir(); (output / "history").write_text("retained")
    fake = Fixture(response)
    with pytest.raises(FileExistsError): routing.run(prepared(), output, transport=fake)
    assert not fake.calls and (output / "history").read_text() == "retained"


def test_external_skill_must_resolve_and_reference_labels_are_rejected(tmp_path):
    document = {"schema": routing.INPUT_SCHEMA, "task_id": "t", "context": "", "request": "任务"}
    document["expected_answers"] = {"cad": "needed"}
    with pytest.raises(ValueError, match="invalid_context_input"): routing.prepare(document)
    document.pop("expected_answers")
    document["external_skills"] = [{"id": "solver", "skill_path": str(tmp_path / "SKILL.md"), "capability": "运行求解器"}]
    with pytest.raises(ValueError, match="external_skill_source_missing"): routing.prepare(document)
    (tmp_path / "SKILL.md").write_text("Synthetic solver skill")
    result = routing.prepare(document)
    assert "external_solver" in result["payload"]["questions"]
    assert str(tmp_path) not in json.dumps(result["payload"])
