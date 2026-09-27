"""Independent tiny fixtures for candidate transport, privacy and frozen scope."""
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import shutil
import subprocess
import sys
from unittest.mock import patch
import zipfile

import pytest

from scripts import build_engineering_candidate as candidate
from scripts import package_skill
from scripts.check_public_privacy import content_findings


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def fixture_root(tmp_path):
    root = tmp_path / "source"
    root.mkdir()
    (root / "SKILL.md").write_text("Synthetic test entry\n")
    (root / "VERSION").write_text("0.6.0\n")
    write_json(root / candidate.BASELINE, {"format": "ontology-engineering.shareable-core-assets/v1",
        "excludes": ["private project records"], "files": [{"path": "SKILL.md", "origin": "source"}]})
    source = root / candidate.DATA / "archive/page.pdf"
    source.parent.mkdir(parents=True)
    source.write_bytes(b"synthetic offline source page")
    write_json(root / candidate.DATA_MANIFEST, {"schema": "ontology-engineering.misumi-data/v1",
        "source_catalogs": {"synthetic": True}, "files": [{"path": "archive/page.pdf", "sha256": package_skill.file_sha256(source),
        "bytes": source.stat().st_size, "media_type": "application/pdf", "role": "source_page_pdf"}]})
    return root


def freeze_fixture(root, *, include_source_data=False, source_pack=None):
    with patch.object(candidate, "EXTRA_FILES", set()):
        return candidate.refresh_ledger(root, include_source_data=include_source_data, source_pack=source_pack)


def packed_fixture(tmp_path, *, private_text=False, unsafe_metadata=False):
    from test_misumi_source_pack import fixture_pack, write_shard
    root, manifest, _, assembly = fixture_pack(tmp_path)
    database = root / "catalog.sqlite3"
    if private_text:
        with sqlite3.connect(database) as connection:
            connection.execute("CREATE TABLE private_fixture(note TEXT)")
            connection.execute("INSERT INTO private_fixture VALUES (?)", ("/" + "Users/fixture-person/private-note",))
    manifest["index"] = {"path": "catalog.sqlite3", "sha256": package_skill.file_sha256(database), "bytes": database.stat().st_size}
    metadata = root / "catalog-and-metadata.tar.xz"
    members = [("catalog.sqlite3", database.read_bytes(), "file")]
    if unsafe_metadata:
        members.append(("../../escaped-note", b"must not escape", "file"))
    write_shard(metadata, members)
    manifest["metadata"] = {"path": metadata.name, "sha256": package_skill.file_sha256(metadata), "bytes": metadata.stat().st_size,
                            "files": [dict(manifest["index"])]}
    books = root / "full-pdfs.tar.xz"
    write_shard(books, [(manifest["assembled_pdfs"][0]["path"], assembly.read_bytes(), "file")])
    manifest["full_pdfs_archive"] = {"path": books.name, "sha256": package_skill.file_sha256(books), "bytes": books.stat().st_size,
                                   "files": [{k: v for k, v in manifest["assembled_pdfs"][0].items() if k in {"path", "sha256", "bytes"}}]}
    write_json(root / "source-pack.json", manifest)
    (root / "producer-not-for-distribution.json").write_text("{}")
    return root, manifest


def test_compressed_pack_exact_selection_explicit_scope_and_stored_xz(tmp_path):
    root = fixture_root(tmp_path)
    packed, manifest = packed_fixture(tmp_path)
    report = freeze_fixture(root, source_pack=packed)
    assert report["source_pack"]["files"] == 4
    ledger = json.loads((root / candidate.LEDGER).read_text())
    assert str(packed) not in json.dumps(ledger)
    assert all(not e["source"].startswith("/") for e in ledger["files"])
    with pytest.raises(ValueError, match="matching --source-pack"):
        candidate.stage(tmp_path / "missing-opt-in", root)
    assert not (tmp_path / "missing-opt-in").exists()
    target = tmp_path / "stage"
    frozen = candidate.stage(target, root, source_pack=packed)
    files = package_skill.candidates(target)
    assert package_skill.candidate_issues(target, files) == []
    assert not (target / candidate.DATA).exists()
    assert not list(target.rglob("catalog.sqlite3"))
    assert not list(target.rglob("producer-not-for-distribution.json"))
    output = tmp_path / "integrated.zip"
    package_skill.write_archive(target, files, output, frozen["hashes"])
    with zipfile.ZipFile(output) as archive:
        assert all(member.compress_type == zipfile.ZIP_STORED for member in archive.infolist() if member.filename.endswith(".xz"))
    with pytest.raises(ValueError, match="cannot be combined"):
        freeze_fixture(root, include_source_data=True, source_pack=packed)
    freeze_fixture(root)
    with pytest.raises(ValueError, match="matching --source-pack"):
        candidate.stage(tmp_path / "wrong-scope", root, source_pack=packed)


@pytest.mark.parametrize("damage", ["manifest", "archive"])
def test_compressed_freeze_rejects_drift(tmp_path, damage):
    root = fixture_root(tmp_path)
    packed, manifest = packed_fixture(tmp_path)
    freeze_fixture(root, source_pack=packed)
    if damage == "manifest":
        manifest["unexpected_after_freeze"] = True
        write_json(packed / "source-pack.json", manifest)
    else:
        archive = packed / manifest["segments"][0]["path"]
        data = bytearray(archive.read_bytes())
        data[len(data) // 2] ^= 1
        archive.write_bytes(data)
    with pytest.raises(ValueError, match="changed since candidate freeze"):
        candidate.stage(tmp_path / "damaged", root, source_pack=packed)


@pytest.mark.parametrize("private_text,unsafe_metadata,reason", [(True, False, "macOS personal absolute path"), (False, True, "path must remain inside its package")])
def test_compressed_metadata_checks_hidden_sqlite_text_and_tar_boundaries(tmp_path, private_text, unsafe_metadata, reason):
    root = fixture_root(tmp_path)
    packed, _ = packed_fixture(tmp_path, private_text=private_text, unsafe_metadata=unsafe_metadata)
    freeze_fixture(root, source_pack=packed)
    target = tmp_path / "stage"
    candidate.stage(target, root, source_pack=packed)
    issues = package_skill.candidate_issues(target, package_skill.candidates(target))
    assert issues and reason in json.dumps(issues)
    assert not (tmp_path / "escaped-note").exists()


def test_candidate_freeze_keeps_old_approval_and_rejects_subsequent_drift(tmp_path):
    root = fixture_root(tmp_path)
    old_hash = package_skill.file_sha256(root / candidate.BASELINE)
    report = freeze_fixture(root)
    assert report["status"] == "candidate_not_published"
    assert package_skill.file_sha256(root / candidate.BASELINE) == old_hash
    target = tmp_path / "stage"
    result = candidate.stage(target, root)
    assert set(result["hashes"]) == {p.relative_to(target).as_posix() for p in package_skill.candidates(target)}
    assert package_skill.candidate_issues(target, package_skill.candidates(target)) == []
    assert json.loads((target / "CANDIDATE-STATUS.json").read_text())["publication_approval"] == "not_granted"
    (root / "SKILL.md").write_text("changed after frozen review")
    with pytest.raises(ValueError, match="changed since candidate freeze"):
        candidate.stage(tmp_path / "drift", root)


def test_data_manifest_cannot_smuggle_extra_state_or_escape(tmp_path):
    root = fixture_root(tmp_path)
    extra = root / candidate.DATA / "history.json"
    extra.write_text("{}")
    with pytest.raises(ValueError, match="outside data manifest"):
        freeze_fixture(root, include_source_data=True)
    extra.unlink()
    manifest_path = root / candidate.DATA_MANIFEST
    value = json.loads(manifest_path.read_text())
    value["files"][0]["path"] = "../outside.pdf"
    write_json(manifest_path, value)
    with pytest.raises(ValueError, match="relative path"):
        freeze_fixture(root, include_source_data=True)


def test_zip64_is_streamed_and_delivery_hashes_actual_bytes(tmp_path):
    root = tmp_path / "stage"
    root.mkdir()
    source = root / "SKILL.md"
    source.write_bytes(b"synthetic" * 20000)
    output = tmp_path / "candidate.zip"
    # Forcing a tiny ZIP64 threshold exercises the actual format with a small
    # fixture. Any accidental whole-file read fails this test.
    with patch.object(Path, "read_bytes", side_effect=AssertionError("whole-file read prohibited")), patch.object(zipfile, "ZIP64_LIMIT", 100), patch.object(package_skill, "CHUNK_BYTES", 97):
        report = package_skill.write_archive(root, [source], output)
        assert report["format"] == "ZIP64"
        assert report["sha256"] == package_skill.file_sha256(output)
        with zipfile.ZipFile(output) as archive:
            member = archive.getinfo("ontology-engineering/SKILL.md")
            assert member.extract_version >= 45
            value = archive.read(member)
            manifest = json.loads(archive.read("ontology-engineering/PORTABLE-MANIFEST.json"))
            assert manifest["files"] == [{"path": "SKILL.md", "sha256": hashlib.sha256(value).hexdigest(), "bytes": len(value)}]
            unpacked = tmp_path / "unpacked"
            archive.extractall(unpacked)
        actual = unpacked / "ontology-engineering"
        assert package_skill.delivery_issues(actual, [actual / "SKILL.md"]) == []


def test_stream_failure_never_leaves_a_published_or_partial_zip(tmp_path):
    root = tmp_path / "stage"
    root.mkdir()
    source = root / "SKILL.md"
    source.write_text("changed fixture")
    output = tmp_path / "candidate.zip"
    with pytest.raises(ValueError, match="changed since frozen hash"):
        package_skill.write_archive(root, [source], output, {"SKILL.md": "0" * 64})
    assert not output.exists()
    assert not list(tmp_path.glob("candidate.zip.partial-*"))


@pytest.mark.parametrize("name", ["sources/books/mechanics.pdf", "sources/misumi/pages.tar.xz", "var/state/query.json"])
def test_core_writer_rejects_explicit_local_sources_without_reading_them(tmp_path, name):
    root = tmp_path / "skill"
    source = root / name
    source.parent.mkdir(parents=True)
    source.write_bytes(b"private downloaded source fixture")
    output = tmp_path / "must-not-exist.zip"
    with patch.object(package_skill, "file_sha256", side_effect=AssertionError("source contents must not be read")):
        with pytest.raises(ValueError, match="local sources or private state must not be included"):
            package_skill.write_archive(root, [source], output)
    assert not output.exists()
    assert not list(tmp_path.glob("must-not-exist.zip.partial-*"))


@pytest.mark.parametrize("name", ["sources/books/mechanics.pdf", "var/projects/private-case.json"])
def test_core_freeze_excludes_local_sources_even_if_legacy_selection_lists_them(tmp_path, name):
    root = fixture_root(tmp_path)
    baseline = json.loads((root / candidate.BASELINE).read_text())
    # The absent original must never be opened or required to build the core.
    baseline["files"].append({"path": name, "origin": "source"})
    write_json(root / candidate.BASELINE, baseline)
    before = (root / candidate.BASELINE).read_bytes()
    freeze_fixture(root)
    ledger = json.loads((root / candidate.LEDGER).read_text())
    assert not any(entry["path"].split("/", 1)[0] in {"sources", "var"} for entry in ledger["files"])
    assert (root / candidate.BASELINE).read_bytes() == before
    candidate.stage(tmp_path / "stage", root)
    # A hand-edited candidate ledger must not bypass the same boundary.
    ledger["files"].insert(0, {"path": name, "source": name})
    write_json(root / candidate.LEDGER, ledger)
    with pytest.raises(ValueError, match="local sources or private state must not be included"):
        candidate.stage(tmp_path / "invalid-stage", root)
    assert not (tmp_path / "invalid-stage" / name).exists()
    ledger["files"][0]["path"] = "references/renamed-private-record.json"
    write_json(root / candidate.LEDGER, ledger)
    with pytest.raises(ValueError, match="local sources or private state must not be included"):
        candidate.stage(tmp_path / "aliased-stage", root)


def test_packaging_cli_retains_only_requested_builds_under_skill_var(tmp_path, capsys):
    root = fixture_root(tmp_path)
    freeze_fixture(root)
    checked_stages = []

    def tiny_check(directory):
        checked_stages.append(directory)
        assert directory.is_relative_to(root / "var")
        return {"passed": True}, package_skill.candidates(directory)

    output = root / "var/builds/candidate.zip"
    with patch.object(candidate, "ROOT", root), patch.object(candidate, "check", tiny_check):
        assert candidate.main(["--output", str(output)]) == 0
        assert output.is_file() and not checked_stages[-1].exists()
        retained = root / "var/builds/review-stage"
        assert candidate.main(["--stage", str(retained), "--inspect-only"]) == 0
        assert (retained / "SKILL.md").is_file()
        assert candidate.main(["--output", str(tmp_path / "external.zip")]) == 1
        assert candidate.main(["--stage", str(tmp_path / "external-stage")]) == 1
    assert not (tmp_path / "external.zip").exists()
    assert not (tmp_path / "external-stage").exists()
    assert not list((root / "var/builds").glob("oe-engineering-candidate-*"))
    with zipfile.ZipFile(output) as archive:
        assert not any(name.startswith("ontology-engineering/var/") for name in archive.namelist())
    with patch.object(package_skill, "ROOT", root), patch.object(package_skill, "check", return_value=({"passed": True}, [root / "SKILL.md"])):
        assert package_skill.main(["--output", str(root / "var/builds/direct.zip")]) == 0
        with pytest.raises(SystemExit) as error:
            package_skill.main(["--output", str(tmp_path / "external-direct.zip")])
        assert error.value.code == 2
    outside = tmp_path / "outside"
    outside.mkdir()
    (root / "var/escape").symlink_to(outside, target_is_directory=True)
    with pytest.raises(ValueError, match="inside_skill"):
        package_skill.new_output_path(root / "var/escape/escaped.zip", root)
    assert not list(outside.iterdir())
    capsys.readouterr()


def test_shareable_cli_uses_local_temporary_stage_without_changing_approval(tmp_path, capsys):
    from scripts import build_shareable_core as shareable

    root = tmp_path / "skill"
    root.mkdir()
    (root / "SKILL.md").write_text("Synthetic reviewed source\n")
    ledger_path = root / "distribution/shareable-core-assets.json"
    write_json(ledger_path, {"format": "ontology-engineering.shareable-core-assets/v1",
        "distribution_scope": shareable.PUBLIC_CORE_SCOPE + "; owner-approved", "files": [{
            "path": "SKILL.md", "origin": "source", "sha256": package_skill.file_sha256(root / "SKILL.md"),
            "privacy_review": "screened_for_known_identifiers_and_direct_secrets",
            "rights_scope": shareable.PUBLIC_CORE_SCOPE, "public_approval": "owner_approved",
            "has_personal_data": False, "license_or_authority": "synthetic fixture",
            "author_or_generation": "synthetic fixture", "input_rights": "synthetic fixture"}]})
    approval_before = ledger_path.read_bytes()
    observed = []

    def tiny_check(directory):
        assert directory.is_relative_to(root / "var/builds")
        observed.append(directory)
        return {"passed": True}, package_skill.candidates(directory)

    output = root / "var/builds/shareable.zip"
    with patch.object(shareable, "ROOT", root), patch.object(shareable, "ASSETS", ledger_path), patch.object(shareable, "check", tiny_check):
        assert shareable.main(["--output", str(output)]) == 0
        with pytest.raises(SystemExit) as error:
            shareable.main(["--output", str(tmp_path / "outside.zip")])
        assert error.value.code == 2
    assert output.is_file() and all(not path.exists() for path in observed)
    assert ledger_path.read_bytes() == approval_before
    assert not (tmp_path / "outside.zip").exists()
    assert not list((root / "var/builds").glob("oe-shareable-core-*"))
    capsys.readouterr()


def test_sqlite_privacy_reads_text_without_decoding_blobs_or_writing_database(tmp_path):
    database = tmp_path / "catalog.sqlite3"
    personal = "/" + "Users/fixture-person/" + "old-private-path"
    with sqlite3.connect(database) as connection:
        connection.execute("CREATE TABLE facts(text_value TEXT, payload BLOB)")
        connection.execute("INSERT INTO facts VALUES (?,?)", (personal, b"\xff\x00binary"))
    before = package_skill.file_sha256(database)
    findings = content_findings(database, tmp_path)
    assert [f.rule for f in findings] == ["macOS personal absolute path"]
    assert package_skill.file_sha256(database) == before
    assert not list(tmp_path.glob("*-wal"))
    with sqlite3.connect(database) as connection:
        connection.execute("UPDATE facts SET text_value='source-bound synthetic text'")
        connection.commit()
        connection.execute("VACUUM")
    assert content_findings(database, tmp_path) == []


def test_candidate_rejects_data_manifest_identity_drift_and_unlisted_files(tmp_path):
    root = fixture_root(tmp_path)
    freeze_fixture(root, include_source_data=True)
    stage = tmp_path / "stage"
    candidate.stage(stage, root, include_source_data=True)
    (stage / "references").mkdir()
    (stage / "references/extra.md").write_text("unlisted content")
    assert "inventory differs" in package_skill.candidate_issues(stage, package_skill.candidates(stage))[0]["reason"]
    (stage / "references/extra.md").unlink()
    source = stage / candidate.DATA / "archive/page.pdf"
    source.write_bytes(b"changed data")
    assert "bytes differ" in package_skill.candidate_issues(stage, package_skill.candidates(stage))[0]["reason"]


def test_lightweight_default_never_reads_source_data_and_builds_without_it(tmp_path):
    root = fixture_root(tmp_path)
    # A broken source manifest must not enter the lightweight build path.
    (root / candidate.DATA_MANIFEST).write_text("invalid JSON fixture")
    with patch.object(candidate, "data_entries", side_effect=AssertionError("default touched source data")):
        report = freeze_fixture(root)
    assert report["source_data_included"] is False
    assert report["data_files"] == 0
    shutil.rmtree(root / candidate.DATA)
    report = freeze_fixture(root)
    target = tmp_path / "stage"
    candidate.stage(target, root)
    assert not (target / candidate.DATA).exists()
    assert package_skill.candidate_issues(target, package_skill.candidates(target)) == []


def test_full_source_inventory_requires_explicit_opt_in_at_build_too(tmp_path):
    root = fixture_root(tmp_path)
    freeze_fixture(root, include_source_data=True)
    target = tmp_path / "must-not-create"
    with pytest.raises(ValueError, match="matching explicit"):
        candidate.stage(target, root)
    assert not target.exists()
    candidate.stage(tmp_path / "explicit-stage", root, include_source_data=True)
    freeze_fixture(root)
    with pytest.raises(ValueError, match="matching explicit"):
        candidate.stage(tmp_path / "scope-mismatch", root, include_source_data=True)


def test_retired_baseline_is_explicit_and_other_missing_assets_still_fail(tmp_path):
    root = fixture_root(tmp_path)
    baseline = json.loads((root / candidate.BASELINE).read_text())
    retired = "skills/cad-agent/references/fusion-execution.md"
    baseline["files"].append({"path": retired, "origin": "source"})
    write_json(root / candidate.BASELINE, baseline)
    before = package_skill.file_sha256(root / candidate.BASELINE)
    freeze_fixture(root)
    frozen = json.loads((root / candidate.LEDGER).read_text())
    assert retired in frozen["retired_baseline_assets"]
    assert retired not in {e["path"] for e in frozen["files"]}
    assert package_skill.file_sha256(root / candidate.BASELINE) == before
    baseline["files"].append({"path": "references/unknown-missing.md", "origin": "source"})
    write_json(root / candidate.BASELINE, baseline)
    with pytest.raises(ValueError, match="missing or unsafe candidate file"):
        freeze_fixture(root)


def test_next_candidate_uses_canonical_control_plane_without_rewriting_old_approval(tmp_path):
    root = fixture_root(tmp_path)
    name = "ontology_engineering/semantic_engagement.py"
    baseline = json.loads((root / candidate.BASELINE).read_text())
    baseline["files"].append({"path": name, "origin": "override", "sha256": "0" * 64})
    write_json(root / candidate.BASELINE, baseline)
    for directory, content in (("", "CURRENT = True\n"),
                               ("distribution/shareable-overrides", "LEGACY = True\n"),
                               (candidate.OVERRIDES, "STALE_NEXT = True\n")):
        source = root / directory / name
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(content)
    approval_before = (root / candidate.BASELINE).read_bytes()
    legacy_before = (root / "distribution/shareable-overrides" / name).read_bytes()
    freeze_fixture(root)
    ledger = json.loads((root / candidate.LEDGER).read_text())
    selected = next(entry for entry in ledger["files"] if entry["path"] == name)
    assert selected["source"] == name
    stage = tmp_path / "stage"
    candidate.stage(stage, root)
    assert (stage / name).read_text() == "CURRENT = True\n"
    assert (root / candidate.BASELINE).read_bytes() == approval_before
    assert (root / "distribution/shareable-overrides" / name).read_bytes() == legacy_before


@pytest.mark.skipif(not all(shutil.which(name) for name in ("pdfinfo", "pdftotext", "pdftoppm")),
                    reason="Poppler required for real candidate PDF handoff")
def test_actual_candidate_zip_supports_pdf_handoff_with_an_empty_home(tmp_path):
    """Exercise the real package selection, not a handpicked dependency clone."""
    from test_local_pdf_sources import pdf_fixture

    # Only the isolated snapshot's candidate ledger is refreshed. Copy the whole
    # source tree except environments, caches and downloaded originals. Inject
    # tiny originals below so the real builder must enforce their exclusion.
    protected = (candidate.BASELINE, candidate.LEDGER)
    original_hashes = {name: (package_skill.file_sha256(candidate.ROOT / name)
                              if (candidate.ROOT / name).exists() else None)
                       for name in protected}
    snapshot = tmp_path / "source-snapshot"
    skip_generated = shutil.ignore_patterns(".git", ".venv", "__pycache__", ".pytest_cache", "*.pyc", ".DS_Store")

    def snapshot_ignores(directory, names):
        ignored = set(skip_generated(directory, names))
        if Path(directory) == candidate.ROOT:
            ignored.update({"sources", "var"})
        return ignored

    shutil.copytree(candidate.ROOT, snapshot, symlinks=True, ignore=snapshot_ignores)
    for name in ("sources/books/mechanics.pdf", "sources/misumi/pages.tar.xz", "var/state/query.json"):
        original_fixture = snapshot / name
        original_fixture.parent.mkdir(parents=True, exist_ok=True)
        original_fixture.write_bytes(b"downloaded original excluded from the core")
    baseline_before = package_skill.file_sha256(snapshot / candidate.BASELINE)
    frozen = candidate.refresh_ledger(snapshot)
    assert frozen["source_pack"] is None and frozen["source_data_included"] is False
    assert package_skill.file_sha256(snapshot / candidate.BASELINE) == baseline_before
    ledger = json.loads((snapshot / candidate.LEDGER).read_text())
    selected = {entry["path"]: entry for entry in ledger["files"]}
    required = {"ontology_engineering/local_pdf_sources.py", "ontology_engineering/source_citations.py",
                "ontology_engineering/semantic_bundle_transport.py", "ontology_engineering/semantic_engagement.py",
                "runtime/source-delivery.json", "tests/test_local_pdf_sources.py", "tests/test_unified_sources.py",
                "tests/test_knowledge_navigation.py", ".gitignore", "ontology_engineering/source_library.py",
                "scripts/source_library.py", "tests/test_source_library.py", "ontology_engineering/local_paths.py"}
    assert required <= selected.keys()
    assert not any(name.split("/", 1)[0] in {"sources", "var"} for name in selected)
    assert selected["ontology_engineering/semantic_engagement.py"]["source"] == "ontology_engineering/semantic_engagement.py"
    stage = tmp_path / "stage"
    staged = candidate.stage(stage, snapshot)
    report, files = package_skill.check(stage)
    assert report["passed"], report["issues"]
    output = tmp_path / "actual-candidate.zip"
    package_skill.write_archive(stage, files, output, staged["hashes"])
    with zipfile.ZipFile(output) as archive:
        assert archive.testzip() is None
        assert "ontology-engineering/runtime/source-delivery.json" in archive.namelist()
        assert not any(name.startswith("ontology-engineering/sources/") for name in archive.namelist())
        assert not any(name.startswith("ontology-engineering/var/") for name in archive.namelist())
        archive.extractall(tmp_path / "received")
    received = tmp_path / "received/ontology-engineering"
    assert package_skill.delivery_issues(received, package_skill.candidates(received)) == []
    payload_before = {name: package_skill.file_sha256(received / name) for name in staged["hashes"]}
    delivery = json.loads((received / "runtime/source-delivery.json").read_text())
    assert delivery["schema"] == "ontology-engineering.source-delivery/v1" and delivery["files"]
    books = [entry for entry in delivery["files"] if entry["relative_path"].startswith("books/")]
    assert len(books) == 6
    assert all(entry["registration"]["chapters"] for entry in books)
    assert "/sources/" in (received / ".gitignore").read_text().splitlines()
    assert "/var/" in (received / ".gitignore").read_text().splitlines()
    shutil.rmtree(snapshot)
    shutil.rmtree(stage)

    sources = received / "sources/books"
    sources.mkdir(parents=True)
    original = pdf_fixture(sources / "book.pdf", ["Materials opening page.",
        "Shaft mechanism under stated load conditions. Preserve assumptions and inspect every interface before application."])
    original_sha = package_skill.file_sha256(original)
    manifest = sources / "manifest.json"
    write_json(manifest, {"schema": "ontology-engineering.local-pdf-manifest/v1", "sources": [{
        "id": "packaged-mechanics", "title": "Packaged mechanics fixture", "edition": "Test edition 1",
        "path": "book.pdf", "sha256": original_sha,
        "chapters": [{"id": "mechanism", "title": "Mechanism", "start_page": 2, "end_page": 2,
                      "aliases": ["shaft"]}]}]})
    context = received / "var/projects/fixture/context.json"
    write_json(context, {"schema": "ontology-engineering.supplier-knowledge-context/v1", "task_id": "candidate-closure",
        "project": {"id": "packaged-project", "revision": "P1"}, "decision_id": "select-shaft",
        "object": {"id": "shaft", "name": "Shaft", "revision": "A"},
        "goal": "Compare mechanisms while retaining their applicability conditions."})
    data, state = received / "sources/.indexes/local-pdf", received / "var/state/misumi"
    empty_home = tmp_path / "empty-home"
    empty_home.mkdir()
    tool_dirs = sorted({str(Path(shutil.which(name)).parent) for name in ("pdfinfo", "pdftotext", "pdftoppm")})
    env = {"PATH": os.pathsep.join(tool_dirs), "HOME": str(empty_home), "PYTHONPATH": "",
           "PYTHONDONTWRITEBYTECODE": "1", "LC_ALL": "C"}
    base_python = Path(getattr(sys, "_base_executable", sys.executable)).resolve()
    listed = subprocess.run(
        [sys.executable, "-S", str(received / "scripts/source_library.py"), "list", "--select", "books"],
        executable=str(base_python), cwd=tmp_path, env=env, capture_output=True, text=True, timeout=60)
    assert listed.returncode == 0 and not listed.stderr, listed.stderr + listed.stdout
    listing = json.loads(listed.stdout)
    assert listing["status"] == "listed" and listing["network_requests"] == 0
    assert Path(listing["sources_root"]) == received / "sources"
    assert {entry["book_id"] for entry in listing["files"]} == {entry["registration"]["id"] for entry in books}
    assert all(entry["status"] == "missing" for entry in listing["files"])
    assert listing["bytes"] == sum(entry["bytes"] for entry in books)
    cad_identity = subprocess.run(
        [sys.executable, "-S", str(received / "skills/cad-agent/scripts/canonical_content_digest.py"), "--json"],
        executable=str(base_python), cwd=tmp_path, env=env, capture_output=True, text=True, timeout=60)
    assert cad_identity.returncode == 0 and not cad_identity.stderr, cad_identity.stderr + cad_identity.stdout
    cad_stamp = dict(line.split(": ", 1) for line in (received / "skills/cad-agent/BUILD_INFO").read_text().splitlines() if ": " in line)
    assert json.loads(cad_identity.stdout)["digest"] == cad_stamp["canonical_content_tree_sha256"]
    assert cad_stamp["candidate_status"] == "candidate_not_published"

    def knowledge(*arguments, expected=0):
        result = subprocess.run(
            [sys.executable, "-S", str(received / "scripts/jev_knowledge.py"),
             "--sources", "local-pdf", "--pdf-root", str(data), "--state-root", str(state),
             "--data-root", str(received / "sources/.indexes/misumi"), "--json", *map(str, arguments)],
            executable=str(base_python), cwd=tmp_path, env=env, capture_output=True, text=True, timeout=60)
        assert result.returncode == expected, result.stderr + result.stdout
        assert not result.stderr, result.stderr
        return json.loads(result.stdout)

    missing = knowledge("--status", expected=2)
    assert missing["status"] == "not_ready"
    registered = knowledge("--register-pdfs", manifest)
    assert registered["status"] == "registered" and registered["network_requests"] == 0
    packet_path = received / "var/projects/fixture/evidence.json"
    found = knowledge("shaft", "--local", "--context", context, "--output", packet_path)
    assert found["status"] == "completed" and found["evidence_candidates"] == 1
    packet = json.loads(packet_path.read_text())
    assert packet["usage"]["network_requests"] == 0
    view = knowledge("--view-record", packet_path, "--render", "1", "--render-dpi", "72")
    assert view["identity"]["source_file_page"] == 2 and view["identity"]["whole_page"] is True
    assert Path(view["image"]).is_relative_to(state)
    verified = knowledge("--verify-packet", packet_path, "--context", context)
    assert verified["status"] == "ready_for_agent_review" and verified["semantic_review"] == "not_run"
    # Semantica execution requires its separately installed environment. The
    # source-bound wheel and lock can still be checked without installing it.
    preflight = subprocess.run(
        [sys.executable, "-S", str(received / "runtime/doctor_runtime.py"), "--preflight", "--json"],
        executable=str(base_python), cwd=tmp_path, env=env, capture_output=True, text=True, timeout=60)
    assert preflight.returncode == 0, preflight.stderr + preflight.stdout
    assert json.loads(preflight.stdout)["ok"] and not preflight.stderr
    assert package_skill.file_sha256(original) == original_sha
    assert package_skill.delivery_issues(received, package_skill.candidates(received)) == []
    assert {name: package_skill.file_sha256(received / name) for name in payload_before} == payload_before
    assert {name: (package_skill.file_sha256(candidate.ROOT / name)
                   if (candidate.ROOT / name).exists() else None)
            for name in protected} == original_hashes
    assert not list(empty_home.iterdir())
