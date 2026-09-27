"""Portable provider contract tests use synthetic source bytes, never a key."""
import argparse
import contextlib
import io
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
from unittest.mock import patch

import pytest

from ontology_engineering import jev_transport, supplier_knowledge
from ontology_engineering.misumi import catalog, cli, paths
from ontology_engineering.misumi.judger import Judge
from ontology_engineering.misumi.knowledge import Knowledge
from ontology_engineering.misumi.paths import SKILL_ROOT, readiness, runtime_config


def fixture_data(data):
    """Small index of a synthetic page; parser/PDF rendering are out of scope."""
    data.mkdir(parents=True)
    archive = data / "archive"
    relative = "tmp/pdfs/fabiaozhunpin202210/pages/0001.pdf"
    source = archive / relative
    source.parent.mkdir(parents=True)
    source.write_bytes(b"synthetic already-extracted source bytes")
    assembly = archive / "output/pdf/catalog.pdf"
    assembly.parent.mkdir(parents=True)
    assembly.write_bytes(b"synthetic assembly bytes")
    body = "联轴器说明与注意事项。传递扭矩时必须核对使用条件。"
    code = catalog.CATALOG_CODES[0]
    with sqlite3.connect(data / "catalog.sqlite3") as db:
        db.executescript("""
            CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE catalogs(code TEXT,label TEXT,title TEXT,page_count INTEGER,pdf_relative_path TEXT,pdf_sha256 TEXT);
            CREATE TABLE chapters(id TEXT,catalog TEXT,title TEXT,start_page INTEGER,end_page INTEGER);
            CREATE TABLE pages(id TEXT,catalog TEXT,page_id INTEGER,pdf_page INTEGER,printed_page TEXT,
                chapter_id TEXT,source_relative_path TEXT,source_url TEXT,source_sha256 TEXT,body TEXT,
                text_sha256 TEXT,native_text_sparse INTEGER,titles_json TEXT,series_json TEXT,series_count INTEGER);
            CREATE VIRTUAL TABLE search USING fts5(models,titles,body,chapter);
        """)
        meta = {"schema": catalog.INDEX_SCHEMA, "fingerprint": "fixture-index", "built_at": "fixture",
                "counts": {"pages": 1, "chapters": 1, "series_hotspots": 0, "native_text_sparse": 0}}
        db.executemany("INSERT INTO meta VALUES (?,?)", [(k, json.dumps(v)) for k, v in meta.items()])
        db.execute("INSERT INTO catalogs VALUES (?,?,?,?,?,?)", (code, "fixture", "fixture", 1, "output/pdf/catalog.pdf", catalog.file_hash(assembly)))
        db.execute("INSERT INTO chapters VALUES (?,?,?,?,?)", ("chapter", code, "联轴器", 1, 1))
        db.execute("INSERT INTO pages VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", ("fixture:page", code, 1, 1, "1", "chapter",
                   relative, "https://example.invalid/source.pdf", catalog.file_hash(source), body, catalog.digest(body.encode()), 0, "[]", "[]", 0))
        db.execute("INSERT INTO search(rowid,models,titles,body,chapter) VALUES (1,?,?,?,?)", ("", "", " ".join(catalog.tokens(body)), "联轴器"))
    return source


def args():
    return argparse.Namespace(query="联轴器的使用条件", book="all", local=True, no_cache=False,
                              knowledge=True, context=None, candidates=8, top=1, full_text=True)


def project_fixture(skill):
    """Synthetic source record; no factory observations or accepted decisions."""
    project = skill / "var/projects/synthetic"
    project.mkdir(parents=True)
    source = project / "record.json"
    source.write_text(json.dumps({"project": {"id": "synthetic", "revision": "snapshot-A"},
        "trigger": {"object_id": "coupling", "product_revision": "A"},
        "decision": {"id": "selection", "plain_language_goal": "联轴器的使用条件"},
        "unknowns": ["Load remains unknown."]}))
    mappings = [("/project/id", "/project/id"), ("/project/revision", "/project/revision"),
                ("/trigger/object_id", "/object/id"), ("/trigger/product_revision", "/object/revision"),
                ("/decision/id", "/decision_id"), ("/decision/plain_language_goal", "/goal"),
                ("/unknowns", "/unknowns")]
    spec = {"schema": "ontology-engineering.project-context-projection/v1",
            "base": {"schema": supplier_knowledge.CONTEXT_SCHEMA, "task_id": "synthetic-inquiry"},
            "fields": [{"source": source.relative_to(skill).as_posix(), "pointer": p, "target": t}
                       for p, t in mappings]}
    projection = project / "projection.json"
    projection.write_text(json.dumps(spec))
    return source, projection, spec


def test_project_cli_reprojects_and_rejects_historical_snapshot_after_feedback(tmp_path, monkeypatch, capsys):
    skill = tmp_path / "skill"
    data = skill / "sources/.indexes/misumi"
    fixture_data(data)
    source, projection, _ = project_fixture(skill)
    monkeypatch.setattr(paths, "SKILL_ROOT", skill)
    old_bytes = source.read_bytes()
    packet_path = source.parent / "packet-A.json"
    query = ["联轴器条件", "--local", "--data-root", str(data), "--project-context", str(projection)]
    assert cli.main(query + ["--output", str(packet_path)]) == 0
    capsys.readouterr()
    packet = json.loads(packet_path.read_text())
    assert packet["project_source_freshness"]["status"] == "current"
    assert packet["adoption"] == "pending_agent_review"
    assert packet["semantic_review"] == "not_run"
    assert source.read_bytes() == old_bytes
    snapshot = source.parent / "snapshot-A.json"
    snapshot.write_text(json.dumps(packet["context_snapshot"]))
    assert cli.main(["--verify-packet", str(packet_path), "--project-context", str(projection)]) == 0
    assert json.loads(capsys.readouterr().out)["status"] == "ready_for_agent_review"
    updated = json.loads(old_bytes)
    updated["trigger"]["product_revision"] = "B"
    updated["unknowns"].append("New interface needs verification.")
    source.write_text(json.dumps(updated))
    assert cli.main(["--verify-packet", str(packet_path), "--context", str(snapshot)]) == 2
    stale = json.loads(capsys.readouterr().out)
    assert stale["status"] == "stale_context"
    assert stale["project_source_freshness"]["status"] == "stale"
    assert cli.main(["--verify-packet", str(packet_path), "--project-context", str(projection)]) == 2
    changed = json.loads(capsys.readouterr().out)
    assert changed["status"] == "changed"
    assert changed["project_source_freshness"]["status"] == "current"
    assert changed["packet_source_freshness"]["status"] == "stale"
    next_path = source.parent / "packet-B.json"
    assert cli.main(query + ["--output", str(next_path)]) == 0
    capsys.readouterr()
    next_packet = json.loads(next_path.read_text())
    assert next_packet["object"]["revision"] == "B"
    assert next_packet["context_sha256"] != packet["context_sha256"]
    assert next_packet["context_snapshot"]["unknowns"] == updated["unknowns"]
    assert json.loads(packet_path.read_text()) == packet
    assert cli.main(["--verify-packet", str(next_path), "--project-context", str(projection)]) == 0
    capsys.readouterr()


def test_stale_project_prevents_model_requests(tmp_path, monkeypatch):
    skill = tmp_path / "skill"
    data = skill / "sources/.indexes/misumi"
    fixture_data(data)
    source, _, spec = project_fixture(skill)
    context = cli.project_context(spec, skill)
    source.write_text(source.read_text() + " ")
    monkeypatch.setattr(paths, "SKILL_ROOT", skill)
    query = args()
    query.context, query.local = context, False
    config = runtime_config(data, skill / "var/state/misumi")
    with patch.object(Judge, "ask", side_effect=AssertionError("stale state sent to model")):
        result = cli.search(config, query)
    assert result["status"] == "stale_context"
    assert result["usage"]["network_requests"] == 0
    assert "review_plan" not in result
    packet = supplier_knowledge.make_packet(result, context, skill_root=skill)
    assert packet["status"] == "stale_context"


@pytest.mark.parametrize("stage", ["route", "review"])
def test_changes_during_query_preserve_historical_results_without_current_claim(tmp_path, monkeypatch, stage):
    skill = tmp_path / "skill"
    data = skill / "sources/.indexes/misumi"
    fixture_data(data)
    source, _, spec = project_fixture(skill)
    monkeypatch.setattr(paths, "SKILL_ROOT", skill)
    query = args()
    query.context = cli.project_context(spec, skill)
    original = getattr(Knowledge, stage)
    def changing(self, *arguments, **kwargs):
        result = original(self, *arguments, **kwargs)
        source.write_text(source.read_text() + " ")
        return result
    with patch.object(Knowledge, stage, changing):
        result = cli.search(runtime_config(data, skill / "var/state/misumi"), query)
    assert result["status"] == "stale_context"
    assert result["results"]
    assert result["usage"]["network_requests"] == 0
    assert ("review_plan" in result) is (stage == "review")
    historical = json.loads(Path(result["record_file"]).read_text())
    assert historical["project_source_freshness"]["status"] == "stale"
    packet = supplier_knowledge.make_packet(result, query.context, skill_root=skill)
    assert packet["status"] == "stale_context"
    check = supplier_knowledge.verify_packet_binding(packet, query.context, skill_root=skill)
    assert check["status"] == "stale_context"


def test_local_projection_bindings_are_not_sent_to_jev(tmp_path):
    _, _, spec = project_fixture(tmp_path)
    context = cli.project_context(spec, tmp_path)
    state = supplier_knowledge.model_context(context)
    assert "source_bindings" not in state
    assert "var/projects/" not in json.dumps(state)
    assert state["context_sha256"] == supplier_knowledge.fingerprint(context)
    assert state["unknowns"] == ["Load remains unknown."]


def test_missing_data_is_explicit_not_ready_and_never_constructs_transport(tmp_path):
    config = runtime_config(tmp_path / "missing", tmp_path / "state")
    with patch.object(Judge, "__init__", side_effect=AssertionError("no transport for missing data")):
        report = cli.search(config, args())
    assert report["status"] == "not_ready"
    assert {e["code"] for e in report["errors"]} == {"index_missing", "archive_missing"}
    assert report["usage"]["network_requests"] == 0
    assert not (tmp_path / "missing").exists()
    assert not (tmp_path / "state").exists()


def test_data_relocation_private_state_and_source_integrity(tmp_path):
    old = tmp_path / "old"
    fixture_data(old)
    moved = tmp_path / "moved"
    old.rename(moved)
    state = tmp_path / "private-state"
    config = runtime_config(moved, state)
    before = {p.relative_to(moved): p.read_bytes() for p in moved.rglob("*") if p.is_file()}
    with patch.object(jev_transport, "resolve_credential_file", side_effect=AssertionError("offline must not resolve credentials")):
        report = cli.search(config, args())
    assert report["status"] == "completed"
    assert report["review_plan"]["status"] == "not_run"
    result = report["results"][0]
    assert Path(result["source_pdf"]).is_relative_to(moved)
    assert result["source_relative_path"] == "tmp/pdfs/fabiaozhunpin202210/pages/0001.pdf"
    assert result["assembled_relative_path"] == "output/pdf/catalog.pdf"
    assert {p.relative_to(moved): p.read_bytes() for p in moved.rglob("*") if p.is_file()} == before
    assert Path(report["record_file"]).is_relative_to(state)
    assert state.stat().st_mode & 0o777 == 0o700
    assert Path(report["record_file"]).stat().st_mode & 0o777 == 0o600
    Path(result["source_pdf"]).write_bytes(b"changed source")
    changed = cli.search(config, args())
    assert changed["status"] == "partial"
    assert changed["errors"][0]["code"] == "source_changed_reindex_required"


def test_shared_typed_backend_and_contract_are_parent_package_modules(tmp_path):
    config = runtime_config(tmp_path / "data", tmp_path / "state")
    judge = Judge(tmp_path / "state", config, "fixture")
    knowledge = Knowledge(config, {"schema": supplier_knowledge.CONTEXT_SCHEMA, "task_id": "fixture"}, judge)
    assert judge.module is jev_transport
    assert knowledge.contract is supplier_knowledge
    assert judge.transport is None
    assert "ontology_skill_root" not in config


def test_state_stays_separate_from_sources_but_can_belong_to_skill(tmp_path):
    with pytest.raises(ValueError, match="outside_source_data"):
        runtime_config(tmp_path / "data", tmp_path / "data/state")
    config = runtime_config(tmp_path / "data", SKILL_ROOT / "var/state/misumi")
    assert config["state_root"] == str(SKILL_ROOT / "var/state/misumi")


def test_default_sources_and_query_history_follow_the_moved_skill(tmp_path, monkeypatch):
    original, moved, home = tmp_path / "skill", tmp_path / "moved-skill", tmp_path / "home"
    legacy = home / ".local/share/ontology-engineering/knowledge-sources/misumi"
    legacy.mkdir(parents=True)
    monkeypatch.setattr(Path, "home", classmethod(lambda cls: home))
    monkeypatch.setattr(paths, "SKILL_ROOT", original)
    fixture_data(original / "sources/.indexes/misumi")
    config = runtime_config()
    assert config["data_root"] == str(original / "sources/.indexes/misumi")
    assert config["state_root"] == str(original / "var/state/misumi")
    assert config["cache_root"] == str(original / "var/cache/misumi")
    first = cli.search(config, args())
    assert Path(first["record_file"]).is_relative_to(original / "var/state/misumi/history")
    shutil.move(original, moved)
    monkeypatch.setattr(paths, "SKILL_ROOT", moved)
    second = cli.search(runtime_config(), args())
    assert second["status"] == "completed"
    assert Path(second["record_file"]).is_relative_to(moved / "var/state/misumi/history")
    assert Path(second["results"][0]["source_pdf"]).is_relative_to(moved / "sources")
    assert not original.exists() and not (home / ".local/state").exists()


@pytest.mark.parametrize("option", ["data-root", "pdf-root", "state-root", "output", "context", "project-context",
                                     "view-record", "verify-packet", "register-pdfs"])
def test_cli_rejects_external_persistent_paths_before_work(tmp_path, monkeypatch, capsys, option):
    skill = tmp_path / "skill"
    skill.mkdir()
    monkeypatch.setattr(paths, "SKILL_ROOT", skill)
    operation = option in {"view-record", "verify-packet", "register-pdfs"}
    arguments = [] if operation else ["shaft", "--local"]
    arguments += ["--json", "--" + option, str(tmp_path / "outside.json")]
    if option == "verify-packet":
        arguments += ["--context", str(skill / "context.json")]
    assert cli.main(arguments) == 2
    report = json.loads(capsys.readouterr().out)
    assert "must_be_inside_skill" in json.dumps(report)
    assert not (skill / "var").exists() and not (tmp_path / "outside.json").exists()


def test_cli_rejects_symlink_redirect_and_external_registered_sources(tmp_path, monkeypatch, capsys):
    skill, outside = tmp_path / "skill", tmp_path / "outside"
    skill.mkdir(); outside.mkdir()
    monkeypatch.setattr(paths, "SKILL_ROOT", skill)
    (skill / "redirect").symlink_to(outside, target_is_directory=True)
    assert cli.main(["shaft", "--local", "--state-root", str(skill / "redirect/state"), "--json"]) == 2
    assert "must_be_inside_skill" in capsys.readouterr().out
    data = skill / "sources/.indexes/misumi"
    data.mkdir(parents=True)
    (data / "source.json").write_text(json.dumps({"schema": "ontology-engineering.misumi-source-location/v1",
        "archive_root": str(outside / "archive"), "index_path": str(outside / "catalog.sqlite3")}))
    assert cli.main(["--status", "--json"]) == 2
    assert "must_be_inside_skill" in capsys.readouterr().out
    assert not list(outside.iterdir())


def test_default_jev_cache_is_separate_from_query_history(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "SKILL_ROOT", tmp_path)
    config = runtime_config()
    judge = Judge(config["state_root"], config, "fixture")
    judge.transport = lambda payload: {"model": payload["model"], "usage": {"input_tokens": 0, "output_tokens": 0}, "answers": {"q": {
        "type": "choice", "choice": "yes", "confidence": 1,
        "probabilities": {"yes": 1, "no": 0}}}}
    payload = {"model": judge.model, "state": "fixture", "questions": {"q": {
        "type": "choice", "instructions": "fixture", "criteria": {"yes": "yes", "no": "no"}}}}
    _, first = judge.ask(payload)
    _, second = judge.ask(payload)
    assert first["execution"] == "live" and second["execution"] == "cache_replay"
    assert (tmp_path / "var/cache/misumi" / (first["key"] + ".json")).is_file()
    assert not (tmp_path / "var/state/misumi/cache").exists()


def test_missing_reindex_dependency_has_machine_ready_state(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "SKILL_ROOT", tmp_path)
    data = tmp_path / "data"
    (data / "archive").mkdir(parents=True)
    error = ModuleNotFoundError("optional parser absent", name="pymupdf")
    stream = io.StringIO()
    with patch.object(cli, "build_index", side_effect=error), contextlib.redirect_stdout(stream):
        code = cli.main(["--reindex", "--data-root", str(data), "--state-root", str(tmp_path / "state"), "--json"])
    assert code == 2
    report = json.loads(stream.getvalue())
    assert report["status"] == "not_ready"
    assert report["errors"] == [{"code": "reindex_requires_pymupdf"}]


def test_relocated_skill_entry_and_compatibility_bridge_work_with_stdlib_only(tmp_path):
    clone = tmp_path / "relocated-skill"
    package = clone / "ontology_engineering"
    package.mkdir(parents=True)
    for name in ("__init__.py", "jev_transport.py", "supplier_knowledge.py", "judgment_contracts.py",
                 "method_evidence.py", "semantic_bundle_transport.py", "local_paths.py", "knowledge_context.py"):
        shutil.copy2(SKILL_ROOT / "ontology_engineering" / name, package / name)
    shutil.copytree(SKILL_ROOT / "ontology_engineering/misumi", package / "misumi", ignore=shutil.ignore_patterns("__pycache__"))
    (clone / "scripts").mkdir()
    for name in ("jev_knowledge.py", "search_misumi_knowledge.py", "semantic_bundle_transport.py"):
        shutil.copy2(SKILL_ROOT / "scripts" / name, clone / "scripts" / name)
    (clone / "references").mkdir()
    for name in ("supplier-knowledge-policy.json", "context-routing-instructions.json"):
        shutil.copy2(SKILL_ROOT / "references" / name, clone / "references" / name)
    lock = json.loads((SKILL_ROOT / "runtime/semantic-bundles.json").read_text())
    bundle = lock["bundles"]["engineering-judgment-intake"]["path"]
    for name in ("runtime/semantic-bundles.json", "runtime/semantica-source-lock.json", bundle):
        destination = clone / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(SKILL_ROOT / name, destination)
    data = clone / "runtime/misumi/data"
    fixture_data(data)
    state = clone / "var/state/misumi"
    # No PATH misumi, third-party site packages, original tree or application
    # is available through these process inputs. The copied skill stands alone.
    env = {"PATH": "/nonexistent", "HOME": str(tmp_path / "home"), "PYTHONPATH": "", "PYTHONDONTWRITEBYTECODE": "1"}
    command = [sys.executable, "-S", str(clone / "scripts/jev_knowledge.py"), "联轴器条件", "--local", "--json", "--state-root", str(state), "--data-root", str(data)]
    run = subprocess.run(command, cwd=tmp_path, env=env, capture_output=True, text=True)
    assert run.returncode == 0, run.stderr + run.stdout
    report = json.loads(run.stdout)
    assert report["status"] == "completed"
    assert report["usage"]["network_requests"] == 0
    assert Path(report["results"][0]["source_pdf"]).is_relative_to(data)
    assert report["review_plan"]["status"] == "not_run"
    assert report["routing"]["pattern_refs"] == []
    assert report["routing"]["pattern_identity"] == report["method_bundle"]["pattern_contract"]["identity"]
    assert all(a["status"] == "not_bundled" for a in report["method_bundle"]["source_availability"])
    context = clone / "context.json"
    context.write_text(json.dumps({"schema": supplier_knowledge.CONTEXT_SCHEMA, "task_id": "portable", "goal": "联轴器使用条件"}))
    output = clone / "packet.json"
    run = subprocess.run([sys.executable, "-S", str(clone / "scripts/search_misumi_knowledge.py"), "联轴器条件", "--context", str(context),
                          "--output", str(output), "--local", "--state-root", str(state), "--data-root", str(data)], cwd=tmp_path, env=env, capture_output=True, text=True)
    assert run.returncode == 0, run.stderr + run.stdout
    packet = json.loads(output.read_text())
    assert packet["schema"] == supplier_knowledge.PACKET_SCHEMA
    assert packet["producer"]["entrypoint"] == "scripts/jev_knowledge.py"
    assert packet["integrity"] == "source_hash_and_exact_text_spans_checked"
    assert packet["adoption"] == "pending_agent_review"
    assert output.stat().st_mode & 0o777 == 0o600


def test_relative_source_paths_cannot_escape_archive(tmp_path):
    with pytest.raises(ValueError, match="source_relative_path_invalid"):
        catalog.archive_path(tmp_path, "../outside.pdf")
    with pytest.raises(ValueError, match="source_relative_path_invalid"):
        catalog.archive_path(tmp_path, "/private/outside.pdf")
