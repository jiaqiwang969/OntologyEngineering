"""Real local JSON projections, with no model or semantic execution."""
from copy import deepcopy
import hashlib
import json

import pytest

from ontology_engineering import knowledge_context as kc


def write_source(root, value=None, relative="var/projects/fixture/discovery.json"):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    value = value if value is not None else {
        "project": {"id": "fixture-project", "revision": "r1"},
        "decision": {"id": "fixture-decision", "goal": "synthetic comparison"},
        "object": {"id": "fixture-object", "revision": "design-r2"},
        "unknowns": ["synthetic capability remains unknown"],
        "statements": [{"id": "claim-1", "text": "synthetic source assertion",
                        "status": "source_report", "source_refs": [relative + "#/statement"]}],
    }
    path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    return path


def spec(relative="var/projects/fixture/discovery.json"):
    fields = [("/project/id", "/project/id"), ("/project/revision", "/project/revision"),
              ("/decision/id", "/decision_id"), ("/decision/goal", "/goal"),
              ("/object/id", "/object/id"), ("/object/revision", "/object/revision"),
              ("/unknowns", "/unknowns"), ("/statements", "/statements")]
    return {"schema": kc.SCHEMA, "base": {"schema": kc.CONTEXT_SCHEMA, "task_id": "fixture-run"},
            "fields": [{"source": relative, "pointer": pointer, "target": target}
                       for pointer, target in fields]}


def projected(root):
    write_source(root)
    return kc.project_context(spec(), root)


def test_projection_binds_source_bytes_and_exact_fields_without_modifying_source(tmp_path):
    path = write_source(tmp_path)
    before = path.read_bytes()
    context = kc.project_context(spec(), tmp_path)
    assert context == kc.project_context(spec(), tmp_path)
    assert path.read_bytes() == before
    assert context["decision_id"] == "fixture-decision"
    assert context["statements"][0]["status"] == "source_report"
    binding, = context["source_bindings"]
    assert binding["sha256"] == hashlib.sha256(before).hexdigest()
    assert binding["path"] == "var/projects/fixture/discovery.json"
    assert binding["mappings"][2]["pointer"] == "/decision/id"
    assert kc.verify_context_sources(context, tmp_path)["status"] == "current"


def test_whole_file_change_is_stale_even_outside_selected_fields(tmp_path):
    context = projected(tmp_path)
    path = tmp_path / context["source_bindings"][0]["path"]
    document = json.loads(path.read_text())
    document["not_selected"] = "a new source revision"
    path.write_text(json.dumps(document))
    result = kc.verify_context_sources(context, tmp_path)
    assert result["status"] == "stale"
    assert result["sources"][0]["reasons"] == ["source_changed"]


@pytest.mark.parametrize("field", ["project", "object", "decision_id"])
def test_swapped_project_object_or_decision_cannot_reuse_binding(tmp_path, field):
    context = projected(tmp_path)
    if field == "decision_id":
        context[field] = "another-decision"
    else:
        context[field]["id"] = "another-" + field
    result = kc.verify_context_sources(context, tmp_path)
    assert result["status"] == "stale"
    assert "context_value_changed" in result["sources"][0]["reasons"]


def test_rehashed_tampered_context_still_must_match_source_value(tmp_path):
    context = projected(tmp_path)
    context["decision_id"] = "another-decision"
    mapping = context["source_bindings"][0]["mappings"][2]
    mapping["value_sha256"] = kc._digest(context["decision_id"])
    result = kc.verify_context_sources(context, tmp_path)
    assert result["status"] == "stale"
    assert "source_value_changed" in result["sources"][0]["reasons"]


@pytest.mark.parametrize("addition", ["goal", "statements", "object_name", "iteration"])
def test_unmapped_added_facts_cannot_be_reported_current(tmp_path, addition):
    write_source(tmp_path)
    projection = spec()
    projection["fields"] = projection["fields"][:1]
    context = kc.project_context(projection, tmp_path)
    if addition == "object_name":
        context["object"]["name"] = "new object"
    elif addition == "iteration":
        context["iteration"] = {"focus": "new direction"}
    elif addition == "statements":
        context["statements"] = [{"id": "unbound", "text": "fabricated acceptance",
                                   "status": "accepted_decision", "source_refs": []}]
    else:
        context["goal"] = "new goal"
    result = kc.verify_context_sources(context, tmp_path)
    assert result["status"] == "stale"
    assert result["reasons"] == ["unmapped_context_field"]
    assert result["unmapped_context_fields"]


def test_call_metadata_and_reference_only_changes_are_allowed(tmp_path):
    context = projected(tmp_path)
    context["task_id"] = "next-call"
    context["record_refs"] = ["var/projects/fixture/note.json"]
    assert kc.verify_context_sources(context, tmp_path)["status"] == "current"


def test_missing_source_reports_stale_and_projection_fails_without_source_text(tmp_path):
    context = projected(tmp_path)
    (tmp_path / context["source_bindings"][0]["path"]).unlink()
    result = kc.verify_context_sources(context, tmp_path)
    assert result["status"] == "stale"
    assert result["sources"][0]["reasons"] == ["source_missing"]
    with pytest.raises(ValueError, match="^knowledge_context_source_missing$"):
        kc.project_context(spec(), tmp_path)


@pytest.mark.parametrize("path", ["/tmp/source.json", "../source.json", "var/projects/../source.json",
                                   "var/projects/p/../../source.json", "var/cache/source.json",
                                   "var//projects/source.json", "var/projects/./source.json",
                                   "var\\projects\\source.json"])
def test_source_path_cannot_leave_project_records(tmp_path, path):
    with pytest.raises(ValueError, match="source_path_invalid"):
        kc.project_context(spec(path), tmp_path)


@pytest.mark.parametrize("component", ["file", "parent", "projects"])
def test_symlinks_are_rejected_even_when_target_is_inside_skill(tmp_path, component):
    path = write_source(tmp_path)
    if component == "file":
        target = path.with_name("original.json")
        path.rename(target)
        path.symlink_to(target)
    else:
        directory = path.parent if component == "parent" else path.parent.parent
        target = directory.with_name(directory.name + "-original")
        directory.rename(target)
        directory.symlink_to(target, target_is_directory=True)
    with pytest.raises(ValueError, match="source_unreadable_or_symlink"):
        kc.project_context(spec(), tmp_path)


def test_source_replaced_by_external_symlink_becomes_stale(tmp_path):
    root = tmp_path / "skill"
    context = projected(root)
    source = root / context["source_bindings"][0]["path"]
    outside = tmp_path / "outside.json"
    source.rename(outside)
    source.symlink_to(outside)
    result = kc.verify_context_sources(context, root)
    assert result["status"] == "stale"
    assert result["sources"][0]["reasons"] == ["source_unreadable_or_symlink"]


def test_removed_mapped_context_field_is_stale(tmp_path):
    context = projected(tmp_path)
    del context["decision_id"]
    result = kc.verify_context_sources(context, tmp_path)
    assert result["status"] == "stale"
    assert "context_value_changed" in result["sources"][0]["reasons"]


def test_missing_project_identity_stays_unknown_and_legacy_context_stays_unbound(tmp_path):
    write_source(tmp_path)
    projection = spec()
    projection["fields"] = [projection["fields"][3]]
    context = kc.project_context(projection, tmp_path)
    assert context["project"] == {"id": "", "revision": ""}
    assert context["object"]["id"] == context["object"]["revision"] == ""
    assert "decision_id" not in context
    assert kc.verify_context_sources(context, tmp_path)["status"] == "current"
    del context["source_bindings"]
    assert kc.verify_context_sources(context, tmp_path)["status"] == "not_bound"


def test_entire_skill_project_tree_can_move_without_rebinding(tmp_path):
    original, moved = tmp_path / "original", tmp_path / "moved"
    context = projected(original)
    original.rename(moved)
    assert not original.exists()
    assert kc.verify_context_sources(context, moved)["status"] == "current"


def test_two_sources_remain_independently_auditable(tmp_path):
    write_source(tmp_path)
    other = write_source(tmp_path, {"changes": ["synthetic change"]}, "var/projects/fixture/revision.json")
    projection = spec()
    projection["fields"].append({"source": other.relative_to(tmp_path).as_posix(),
                                 "pointer": "/changes", "target": "/changes"})
    context = kc.project_context(projection, tmp_path)
    other.unlink()
    result = kc.verify_context_sources(context, tmp_path)
    assert [s["status"] for s in result["sources"]] == ["current", "stale"]


def test_rfc6901_escaped_keys_and_list_indices_are_exact(tmp_path):
    write_source(tmp_path, {"a/b": {"~key": ["first", "second"]}})
    projection = spec()
    projection["fields"] = [{"source": projection["fields"][0]["source"],
                              "pointer": "/a~1b/~0key/1", "target": "/goal"}]
    context = kc.project_context(projection, tmp_path)
    assert context["goal"] == "second"
    assert kc.verify_context_sources(context, tmp_path)["status"] == "current"


@pytest.mark.parametrize("pointer", ["/absent", "/unknowns/01", "/unknowns/-", "/unknowns/9", "/bad~escape"])
def test_missing_or_invalid_pointer_never_gets_a_positive_default(tmp_path, pointer):
    write_source(tmp_path)
    projection = spec()
    projection["fields"] = [{"source": projection["fields"][0]["source"],
                              "pointer": pointer, "target": "/goal"}]
    with pytest.raises(ValueError, match="pointer_(invalid|missing)"):
        kc.project_context(projection, tmp_path)


@pytest.mark.parametrize("target", ["/project", "/object", "/iteration", "/project/name",
                                     "/statements/0/text", "/schema", "/task_id", "/source_bindings"])
def test_targets_cannot_overwrite_identity_contract_or_partial_arrays(tmp_path, target):
    write_source(tmp_path)
    projection = spec()
    projection["fields"][0]["target"] = target
    with pytest.raises(ValueError, match="target_invalid"):
        kc.project_context(projection, tmp_path)


def test_fact_values_cannot_be_injected_through_base(tmp_path):
    projection = spec()
    projection["base"]["goal"] = "unbound fact"
    with pytest.raises(ValueError, match="projection_base_invalid"):
        kc.project_context(projection, tmp_path)


def test_duplicate_targets_and_excessive_inventory_are_rejected(tmp_path):
    write_source(tmp_path)
    projection = spec()
    projection["fields"].append(deepcopy(projection["fields"][0]))
    with pytest.raises(ValueError, match="duplicate_target"):
        kc.project_context(projection, tmp_path)
    projection["fields"] *= 8
    with pytest.raises(ValueError, match="fields_invalid"):
        kc.project_context(projection, tmp_path)
    context = projected(tmp_path)
    with pytest.raises(ValueError, match="bindings_invalid"):
        kc.validate_bindings(context["source_bindings"] * 17)


def test_binding_byte_budget_is_separate_and_bounded(tmp_path):
    context = projected(tmp_path)
    context["source_bindings"][0]["mappings"][0]["pointer"] = "/" + "x" * kc.MAX_BINDING_BYTES
    with pytest.raises(ValueError, match="bindings_too_large"):
        kc.validate_bindings(context["source_bindings"])


@pytest.mark.parametrize("change", ["source_path", "source_hash", "value_hash", "duplicate_target"])
def test_tampered_binding_structure_is_rejected(tmp_path, change):
    context = projected(tmp_path)
    binding = context["source_bindings"][0]
    if change == "source_path":
        binding["path"] = "var/projects/../outside.json"
    elif change == "source_hash":
        binding["sha256"] = "not-a-hash"
    elif change == "value_hash":
        binding["mappings"][0]["value_sha256"] = "not-a-hash"
    else:
        binding["mappings"].append(deepcopy(binding["mappings"][0]))
    with pytest.raises(ValueError, match="knowledge_context_"):
        kc.verify_context_sources(context, tmp_path)


def test_source_arrays_must_already_match_context_contract(tmp_path):
    write_source(tmp_path, {"statements": [{"id": "s", "text": "synthetic", "status": "hypothesis"}]})
    projection = spec()
    projection["fields"] = [projection["fields"][-1]]
    with pytest.raises(ValueError, match="normalization_changed_source_value"):
        kc.project_context(projection, tmp_path)


@pytest.mark.parametrize("raw", [b'{"goal":"first","goal":"second"}', b'{"goal":NaN}', b'not JSON'])
def test_ambiguous_or_invalid_source_json_is_rejected(tmp_path, raw):
    path = write_source(tmp_path)
    path.write_bytes(raw)
    with pytest.raises(ValueError, match="^knowledge_context_source_json_invalid$"):
        kc.project_context(spec(), tmp_path)


def test_binding_validation_has_no_alias_mutation(tmp_path):
    context = projected(tmp_path)
    bindings = kc.validate_bindings(context["source_bindings"])
    bindings[0]["mappings"][0]["pointer"] = "/other"
    assert context["source_bindings"][0]["mappings"][0]["pointer"] == "/project/id"
