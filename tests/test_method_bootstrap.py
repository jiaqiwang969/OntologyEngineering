"""Portable initialization uses real native lifecycle; no mocked PASS verdicts."""
from copy import deepcopy
import io
import json
from pathlib import Path
import zipfile

import pytest

from ontology_engineering import method_bootstrap as bootstrap
from ontology_engineering import semantic_engagement as entry
from ontology_engineering.judgment_review import review_plan
from scripts.semantic_bundle_transport import BundleError, validate_data_archive


def authorization(directory, action="initialize-frozen-methods", **changes):
    plan = bootstrap.read(directory / "plan.json")
    value = {"schema": "ontology-engineering.method-bootstrap-authorization/v1", "action": action,
             "actor_id": plan["actor"], "plan_sha256": bootstrap.digest(bootstrap.encoded(plan)),
             "reason": "Isolated synthetic integration test; no human identity or physical approval claim.",
             "issued_at": bootstrap.now()}
    if action == "adopt-project-binding":
        value["binding_sha256"] = bootstrap.digest((directory / "proposed-project-binding.json").read_bytes())
    value.update(changes)
    path = directory / ("request-" + action + ".json")
    path.write_bytes(bootstrap.encoded(value))
    return path


def prepare(directory):
    return bootstrap.prepare(directory, project="judgment-intake-maintenance", domain="discrete-manufacturing",
        actor="recipient-test-operator", fact_authority="recipient-controlled-synthetic-records",
        evidence_root="evidence:recipient-test")


def test_locked_capsule_has_exact_source_chain_and_no_publisher_decisions():
    spec, payload, capsule = bootstrap.load_capsule()
    assert len(capsule["steps"]) == 4
    assert capsule["target_package_sha256"] == spec["package_sha256"]
    assert all(not any(part in name for part in ("authorization", "registry-events", "engagements")) for name in payload)
    for step in capsule["steps"]:
        delta = json.loads(payload[step["delta_path"]])
        assert delta["created_by"] == capsule["native_source_authority"]


@pytest.mark.parametrize("name", ["../escape.json", "/absolute.json", "nested/../../escape.json"])
def test_capsule_transport_rejects_path_escape(name):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w") as archive:
        archive.writestr(name, "{}")
    raw = stream.getvalue()
    spec = {"sha256": bootstrap.digest(raw), "files": [{"path": name, "sha256": bootstrap.digest(b"{}")}]}
    with pytest.raises(BundleError):
        validate_data_archive(raw, spec)


def test_plan_does_not_create_registry_and_wrong_authority_cannot_apply(tmp_path):
    directory = tmp_path / "recipient"
    report = prepare(directory)
    assert report["workspace_created"] is False and not (directory / "registry").exists()
    path = authorization(directory, actor_id="different-operator")
    with pytest.raises(bootstrap.BootstrapError, match="actor"):
        bootstrap.apply(directory, path)
    assert not (directory / "registry").exists()


def test_changed_configuration_requires_a_separate_plan(tmp_path, monkeypatch):
    directory = tmp_path / "recipient"
    prepare(directory)
    path = authorization(directory)
    monkeypatch.setattr(bootstrap, "code_identity", lambda root: {"changed": "0" * 64})
    with pytest.raises(bootstrap.BootstrapError, match="configuration changed"):
        bootstrap.apply(directory, path)
    assert not (directory / "registry").exists()


@pytest.mark.parametrize("field", ["project", "semantic_target", "baseline", "authority"])
def test_project_adoption_cannot_change_the_authorized_plan_subject(tmp_path, monkeypatch, field):
    directory = tmp_path / "recipient"
    prepare(directory)
    plan = bootstrap.read(directory / "plan.json")
    proposal = bootstrap.read(directory / "project-binding-template.json")
    proposal["baseline"] = {"version": plan["package_version"], "digest": plan["package_sha256"]}
    # Even an authorization matching the changed file cannot silently change
    # the project, package or authority described by the initialization plan.
    if field == "project": proposal[field]["project_id"] = "other-project"
    elif field == "semantic_target": proposal[field]["package_id"] = "other-package"
    elif field == "baseline": proposal[field]["digest"] = "0" * 64
    else: proposal[field]["decision"]["authority_id"] = "other-authority"
    (directory / "proposed-project-binding.json").write_bytes(bootstrap.encoded(proposal))
    path = authorization(directory, "adopt-project-binding")
    monkeypatch.setattr(entry, "discover", lambda *a, **k: pytest.fail("must reject before native discovery"))
    with pytest.raises(bootstrap.BootstrapError, match="exceeds the exact plan"):
        bootstrap.adopt(directory, path)


def test_native_crash_recovery_separate_adoption_and_actual_project_review(tmp_path, monkeypatch):
    directory = tmp_path / "recipient"
    prepare(directory)
    path = authorization(directory)
    expected = {"propose": "proposed", "commit": "committed", "verify": "release_complete", "promote": "promoted"}
    # Each interruption occurs after the native mutation and before the wrapper
    # saves its response. Resumption must inspect the actual native event chain.
    for action in expected:
        original = getattr(entry, action)
        def interrupt(*args, _original=original, _action=action, **kwargs):
            _original(*args, **kwargs)
            raise InterruptedError("injected-after-native-" + _action)
        with monkeypatch.context() as m:
            m.setattr(entry, action, interrupt)
            with pytest.raises(InterruptedError, match="injected-after-native"):
                bootstrap.apply(directory, path)
        _, _, capsule = bootstrap.load_capsule()
        first = capsule["steps"][0]
        result = entry.history(directory / "step-000/binding.json", workspace=directory / "registry",
                               candidate_sha256=first["delta_sha256"])
        assert result["learning"]["current_state"] == expected[action]
    result = bootstrap.apply(directory, path)
    assert result["project_adopted"] is False and not (directory / "project-binding.json").exists()
    event_ids = []
    for i, step in enumerate(capsule["steps"]):
        step_dir = directory / ("step-" + str(i).zfill(3))
        history = entry.history(step_dir / "binding.json", workspace=directory / "registry", candidate_sha256=step["delta_sha256"])
        events = history["execution"]["events"]
        assert [e["state"] for e in events] == bootstrap.STATES
        event_ids.extend(e["event_sha256"] for e in events)
        assert bootstrap.read(step_dir / "authorization-promote.json")["actor_id"] == "recipient-test-operator"
        assert bootstrap.read(step_dir / "binding.json")["authority"]["fact"] == capsule["publisher_fact_authority"]
    # No cached success is trusted: same request revalidates native history.
    assert bootstrap.apply(directory, path)["package_sha256"] == result["package_sha256"]
    again = []
    for i, step in enumerate(capsule["steps"]):
        history = entry.history(directory / ("step-" + str(i).zfill(3)) / "binding.json",
            workspace=directory / "registry", candidate_sha256=step["delta_sha256"])
        again.extend(e["event_sha256"] for e in history["execution"]["events"])
    assert again == event_ids
    adopted = bootstrap.adopt(directory, authorization(directory, "adopt-project-binding"))
    binding = bootstrap.read(adopted["binding"])
    assert binding["authority"]["fact"]["authority_id"] == "recipient-controlled-synthetic-records"
    assert "promote" not in binding["allowed_actions"]
    examples = bootstrap.ROOT / "examples/judgment_intake"
    report = review_plan(examples / "cad-review-plan.json", examples, adopted["binding"], adopted["workspace"],
                         "recipient-test-operator", tmp_path / "review")
    assert report["semantic_reviewed_functions"] == ["structure_support"]
    assert report["coverage"]["status"] == "blocked"
    # Changing retained request metadata cannot turn it into the context that
    # actually occurred. Only local bookkeeping is changed in this negative test.
    task_path = directory / "step-000/task-commit.json"
    task = bootstrap.read(task_path); task["intent"] += " changed"
    task_path.write_bytes(bootstrap.encoded(task))
    with pytest.raises(bootstrap.BootstrapError, match="differs from native event"):
        bootstrap.apply(directory, path)
