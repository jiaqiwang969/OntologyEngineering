"""Source-bound data installation, closure checks and a real optional rebuild."""
import json
from pathlib import Path
import sqlite3
import shutil

import pytest

from ontology_engineering.misumi import catalog
from ontology_engineering.misumi.paths import SKILL_ROOT

from runtime.misumi import setup
from misumi_reindex_fixture import exercise_reindex
from test_misumi_source_pack import write_shard
from ontology_engineering.misumi.source_pack import SCHEMA as PACK_SCHEMA


def fixture_archive(tmp_path):
    archive, index = tmp_path / "archive", tmp_path / "source.sqlite3"
    archive.mkdir()
    identities, catalog_rows, pages = [], [], []
    for i, code in enumerate(catalog.CATALOG_CODES):
        prefix = archive / "tmp/pdfs" / code
        (prefix / "pages").mkdir(parents=True)
        source = prefix / "pages/0001.pdf"
        source.write_bytes(("source " + code).encode())
        assembled = archive / "output/pdf" / (code + ".pdf")
        assembled.parent.mkdir(parents=True, exist_ok=True)
        assembled.write_bytes(source.read_bytes())
        meta_path = prefix / "metadata.json"
        meta_path.write_text(json.dumps({"ebook": {"catalogCode": code, "pages": {"info": [{"id": i + 1}]}}}))
        records = archive / "下载记录"
        records.mkdir(exist_ok=True)
        manifest_path = records / (code + "-manifest.json")
        manifest_path.write_text(json.dumps([{"page_number": 1, "page_id": i + 1, "filename": "0001.pdf",
            "bytes": source.stat().st_size, "sha256": setup.file_hash(source)}]))
        assembly = {"book": code, "pages": 1, "file": assembled.relative_to(archive).as_posix(), "sha256": setup.file_hash(assembled)}
        (records / (code + "-assembly.json")).write_text(json.dumps(assembly))
        identity = [code, setup.file_hash(meta_path), setup.file_hash(manifest_path), setup.file_hash(assembled)]
        identities.append(identity)
        catalog_rows.append((code, 1, assembly["file"], identity[3], identity[1], identity[2]))
        pages.append((code, 1, source.relative_to(archive).as_posix(), setup.file_hash(source), source.stat().st_size, i + 1))
    with sqlite3.connect(index) as db:
        db.executescript("""
            CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE catalogs(code TEXT,page_count INTEGER,pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT);
            CREATE TABLE pages(catalog TEXT,pdf_page INTEGER,source_relative_path TEXT,source_sha256 TEXT,source_bytes INTEGER,page_id INTEGER);
        """)
        db.executemany("INSERT INTO catalogs VALUES (?,?,?,?,?,?)", catalog_rows)
        db.executemany("INSERT INTO pages VALUES (?,?,?,?,?,?)", pages)
        db.executemany("INSERT INTO meta VALUES (?,?)", [(k, json.dumps(v)) for k, v in {
            "schema": catalog.INDEX_SCHEMA, "source_identity": identities, "counts": {"pages": 2}, "fingerprint": "fixture"}.items()])
    return archive, index


def test_import_verifies_source_hashes_and_publishes_closed_bundle(tmp_path):
    archive, index = fixture_archive(tmp_path)
    target = tmp_path / "data"
    result = setup.import_data(archive, index, target)
    assert result["status"] == "imported"
    assert result["files"] == 11
    assert setup.verify(target)["status"] == "verified"
    assert not list(tmp_path.glob(".data-import-*"))


def test_setup_cli_rejects_external_target_and_external_registration(tmp_path, monkeypatch, capsys):
    skill = tmp_path / "skill"
    skill.mkdir()
    monkeypatch.setattr(setup, "ROOT", skill)
    assert setup.main(["verify", "--data-root", str(tmp_path / "outside")]) == 2
    assert "must_be_inside_skill" in capsys.readouterr().out
    assert setup.main(["register", "--data-root", str(skill / "sources/index"),
        "--archive", str(tmp_path / "outside"), "--index", str(skill / "catalog.sqlite3")]) == 2
    assert "must_be_inside_skill" in capsys.readouterr().out
    assert not list(skill.iterdir())


def test_register_reuses_existing_sources_without_copying_pdfs(tmp_path):
    from ontology_engineering.misumi.paths import runtime_config
    archive, index = fixture_archive(tmp_path)
    target = tmp_path / "registered"
    result = setup.register(archive, index, target)
    assert result["copied_source_files"] == 0
    assert [p.name for p in target.iterdir()] == ["source.json"]
    config = runtime_config(target, tmp_path / "state")
    assert Path(config["archive_root"]) == archive.resolve()
    assert Path(config["index_path"]) == index.resolve()
    assert setup.verify(target)["status"] == "verified"
    with pytest.raises(ValueError, match="registration_already_exists"):
        setup.register(archive, index, target)


def test_registered_archive_and_index_follow_whole_library_move(tmp_path):
    from ontology_engineering.misumi.paths import runtime_config
    original = tmp_path / "library"
    original.mkdir()
    archive, index = fixture_archive(original)
    target = original / "registration"
    setup.register(archive, index, target)
    source_bytes = (target / "source.json").read_bytes()
    assert str(original).encode() not in source_bytes
    moved = tmp_path / "moved-library"
    shutil.move(original, moved)
    config = runtime_config(moved / "registration", moved / "private-state")
    assert Path(config["archive_root"]) == moved / archive.relative_to(original)
    assert Path(config["index_path"]) == moved / index.relative_to(original)
    assert setup.verify(moved / "registration")["status"] == "verified"
    assert (moved / "registration/source.json").read_bytes() == source_bytes
    assert not original.exists()


def test_registered_index_drift_is_not_silently_accepted(tmp_path):
    archive, index = fixture_archive(tmp_path)
    target = tmp_path / "registered"
    setup.register(archive, index, target)
    with sqlite3.connect(index) as db:
        db.execute("UPDATE meta SET value='\"changed\"' WHERE key='fingerprint'")
    with pytest.raises(ValueError, match="registered_index_changed"):
        setup.verify(target)


@pytest.mark.parametrize("path", ["../escape", "/absolute", "a\\b", "a//b", "", None])
def test_relative_path_validation_rejects_escapes_and_ambiguous_paths(tmp_path, path):
    with pytest.raises(ValueError, match="invalid_relative_data_path"):
        setup.safe_path(tmp_path, path)


def test_import_refuses_existing_target_and_preserves_it(tmp_path):
    archive, index = fixture_archive(tmp_path)
    target = tmp_path / "data"
    target.mkdir()
    marker = target / "keep.txt"
    marker.write_text("owner data")
    with pytest.raises(ValueError, match="data_root_already_exists"):
        setup.import_data(archive, index, target)
    assert marker.read_text() == "owner data"


def test_failed_copy_hash_check_leaves_no_partial_target(tmp_path):
    archive, index = fixture_archive(tmp_path)
    source = next(archive.glob("tmp/pdfs/*/pages/0001.pdf"))
    source.write_bytes(b"x" * source.stat().st_size)
    target = tmp_path / "data"
    with pytest.raises(ValueError, match="source_hash_mismatch"):
        setup.import_data(archive, index, target)
    assert not target.exists()
    assert not list(tmp_path.glob(".data-import-*"))


def test_index_changed_between_discovery_and_copy_is_not_published(tmp_path, monkeypatch):
    archive, index = fixture_archive(tmp_path)
    original = setup.required_files
    def change_after_discovery(source_archive, source_index):
        result = original(source_archive, source_index)
        if source_index == index:
            with sqlite3.connect(index) as db:
                db.execute("UPDATE catalogs SET pdf_sha256=?", ("0" * 64,))
        return result
    monkeypatch.setattr(setup, "required_files", change_after_discovery)
    target = tmp_path / "data"
    with pytest.raises(ValueError, match="source_catalog_identity_mismatch"):
        setup.import_data(archive, index, target)
    assert not target.exists()
    assert not list(tmp_path.glob(".data-import-*"))


@pytest.mark.parametrize("also_rewrite_manifest", [False, True])
def test_verify_detects_changed_pdf_even_if_bundle_hash_is_rewritten(tmp_path, also_rewrite_manifest):
    archive, index = fixture_archive(tmp_path)
    target = tmp_path / "data"
    setup.import_data(archive, index, target)
    manifest_path = target / "data-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    entry = next(e for e in manifest["files"] if e["role"] == "source_page_pdf")
    source = target / entry["path"]
    source.write_bytes(b"x" * source.stat().st_size)
    if also_rewrite_manifest:
        entry["sha256"] = setup.file_hash(source)
        manifest_path.write_text(json.dumps(manifest))
    with pytest.raises(ValueError, match="data_(source_)?hash_mismatch"):
        setup.verify(target)


def test_data_symlink_escape_is_rejected(tmp_path):
    inside = tmp_path / "data"
    inside.mkdir()
    outside = tmp_path / "outside.pdf"
    outside.write_bytes(b"outside")
    (inside / "source.pdf").symlink_to(outside)
    with pytest.raises(ValueError, match="data_path_escape"):
        setup.safe_path(inside, "source.pdf")


def test_reindex_builds_fresh_index_from_real_pdf_pages(tmp_path):
    pytest.importorskip("pymupdf", reason="optional rebuild parser is absent; the fixed misumi_reindex_fixture.py also runs with the rebuild interpreter")
    result = exercise_reindex(tmp_path)
    assert result["status"] == "verified"
    assert result["rebuilt_pages"] == 2


def fixture_install_pack(tmp_path):
    archive, index = fixture_archive(tmp_path)
    downloads = tmp_path / "downloads"
    downloads.mkdir()
    _, identity = setup.required_files(archive, index)
    metadata = [("catalog.sqlite3", index.read_bytes(), "file")]
    metadata += [("archive/" + p.relative_to(archive).as_posix(), p.read_bytes(), "file")
                 for p in sorted(archive.rglob("*.json"))]
    pages = [(p.relative_to(archive).as_posix(), p.read_bytes(), "file") for p in sorted(archive.glob("tmp/pdfs/*/pages/*.pdf"))]
    def archive_entry(name, members):
        path = downloads / name
        write_shard(path, members)
        return {"path": name, "sha256": setup.file_hash(path), "bytes": path.stat().st_size,
                "files": [{"path": n, "sha256": catalog.digest(b), "bytes": len(b)} for n, b, _ in members]}
    meta = archive_entry("catalog-and-metadata.tar.xz", metadata)
    segment = {"id": "fixture-pages", **archive_entry("segments/fixture-pages.tar.xz", pages)}
    assembled = [{"catalog": code, "path": "output/pdf/" + code + ".pdf",
                  "sha256": setup.file_hash(archive / "output/pdf" / (code + ".pdf")),
                  "bytes": (archive / "output/pdf" / (code + ".pdf")).stat().st_size,
                  "download_path": "full/" + code + ".pdf", "download_url": None}
                 for code in catalog.CATALOG_CODES]
    manifest = {"schema": PACK_SCHEMA, "index": meta["files"][0], "metadata": meta,
                "segments": [segment], "assembled_pdfs": assembled, "source_catalogs": identity}
    (downloads / "source-pack.json").write_text(json.dumps(manifest))
    return downloads, manifest, metadata, archive


def replace_metadata(downloads, manifest, members, *, rebind=True):
    path = downloads / manifest["metadata"]["path"]
    write_shard(path, members)
    manifest["metadata"].update(sha256=setup.file_hash(path), bytes=path.stat().st_size)
    if rebind:
        manifest["metadata"]["files"] = [{"path": n, "sha256": catalog.digest(b), "bytes": len(b)} for n, b, _ in members]
    (downloads / "source-pack.json").write_text(json.dumps(manifest))


@pytest.mark.parametrize("corruption", ["metadata", "missing_metadata", "assembled_identity"])
def test_compressed_install_checks_metadata_and_assemblies_against_index(tmp_path, corruption):
    downloads, manifest, members, _ = fixture_install_pack(tmp_path)
    if corruption == "metadata":
        number = next(i for i, (name, _, _) in enumerate(members) if name.endswith("/metadata.json"))
        name, raw, kind = members[number]
        changed = json.loads(raw)
        changed["ebook"]["unreviewed_extra_field"] = "drift"
        members[number] = (name, json.dumps(changed).encode(), kind)
        replace_metadata(downloads, manifest, members)
    elif corruption == "missing_metadata":
        members = [m for m in members if not m[0].endswith("/metadata.json")]
        replace_metadata(downloads, manifest, members)
    else:
        manifest["assembled_pdfs"][0]["sha256"] = "0" * 64
        (downloads / "source-pack.json").write_text(json.dumps(manifest))
    target = tmp_path / "installed"
    with pytest.raises((ValueError, OSError)):
        setup.install_pack(downloads, target)
    assert not target.exists()
    assert not list(tmp_path.glob(".pack-install-*"))


@pytest.mark.parametrize("with_full", [False, True])
def test_compressed_install_preserves_metadata_and_shards_without_expanding_pages(tmp_path, with_full):
    downloads, manifest, _, archive = fixture_install_pack(tmp_path)
    if with_full:
        for entry in manifest["assembled_pdfs"]:
            dest = downloads / entry["download_path"]
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(archive / entry["path"], dest)
    target = tmp_path / "installed"
    result = setup.install_pack(downloads, target)
    assert result["pages_expanded"] == 0
    assert result["pages_available"] == 2
    assert result["full_pdfs_installed"] == (2 if with_full else 0)
    assert not list(target.glob("**/pages/*.pdf"))
    assert len(list((target / "cache/archive").rglob("*.pdf"))) == (2 if with_full else 0)
    assert setup.verify(target)["status"] == "verified"
    assert (target / "source-pack.json").read_bytes() == (downloads / "source-pack.json").read_bytes()
    assert not list(tmp_path.glob(".pack-install-*"))
    with pytest.raises(ValueError, match="data_root_already_exists"):
        setup.install_pack(downloads, target)


@pytest.mark.parametrize("corruption", ["traversal", "symlink", "duplicate", "hash", "missing"])
def test_compressed_metadata_damage_does_not_leave_partial_install(tmp_path, corruption):
    downloads, manifest, members, _ = fixture_install_pack(tmp_path)
    if corruption == "traversal":
        members.append(("../../outside", b"bad", "file"))
    elif corruption == "symlink":
        members[1] = (members[1][0], b"", "symlink")
    elif corruption == "duplicate":
        members.append(members[1])
    elif corruption == "hash":
        members[1] = (members[1][0], b"x" * len(members[1][1]), "file")
    else:
        members.pop()
    replace_metadata(downloads, manifest, members, rebind=False)
    target = tmp_path / "installed"
    with pytest.raises(ValueError):
        setup.install_pack(downloads, target)
    assert not target.exists()
    assert not list(tmp_path.glob(".pack-install-*"))
    assert not (tmp_path / "outside").exists()


@pytest.mark.parametrize("source", ["catalog-and-metadata.tar.xz", "segments/fixture-pages.tar.xz"])
def test_install_rejects_symlinked_source_archives(tmp_path, source):
    downloads, _, _, _ = fixture_install_pack(tmp_path)
    original = downloads / source
    elsewhere = tmp_path / "elsewhere.tar.xz"
    original.rename(elsewhere)
    original.symlink_to(elsewhere)
    with pytest.raises(ValueError, match="symlink"):
        setup.install_pack(downloads, tmp_path / "installed")
    assert not (tmp_path / "installed").exists()
    assert not list(tmp_path.glob(".pack-install-*"))


def test_installed_metadata_drift_is_checked_again_by_verify(tmp_path):
    downloads, manifest, _, _ = fixture_install_pack(tmp_path)
    target = tmp_path / "installed"
    setup.install_pack(downloads, target)
    meta = next(e for e in manifest["metadata"]["files"] if e["path"].endswith("/metadata.json"))
    path = target / meta["path"]
    changed = json.loads(path.read_text())
    changed["ebook"]["extra"] = "unreviewed drift"
    path.write_text(json.dumps(changed))
    with pytest.raises(ValueError, match="metadata_hash"):
        setup.verify(target)


def add_full_archive(downloads, manifest, original):
    members = [(e["download_path"], (original / e["path"]).read_bytes(), "file") for e in manifest["assembled_pdfs"]]
    path = downloads / "full-pdfs.tar.xz"
    write_shard(path, members)
    manifest["full_pdfs_archive"] = {"path": path.name, "sha256": setup.file_hash(path), "bytes": path.stat().st_size,
        "files": [{"path": n, "sha256": catalog.digest(b), "bytes": len(b)} for n, b, _ in members]}
    (downloads / "source-pack.json").write_text(json.dumps(manifest))
    return members


def test_full_books_stay_compressed_until_explicit_restoration(tmp_path):
    downloads, manifest, _, original = fixture_install_pack(tmp_path)
    add_full_archive(downloads, manifest, original)
    # Even when the build workspace still has original books beside the
    # declared compressed archive, default installation leaves them compressed.
    for entry in manifest["assembled_pdfs"]:
        path = downloads / entry["download_path"]
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(original / entry["path"], path)
    target = tmp_path / "installed"
    installed = setup.install_pack(downloads, target)
    assert installed["full_pdfs_archive_preserved"] is True
    assert installed["full_pdfs_installed"] == 0
    assert not list((target / "cache/archive").rglob("*.pdf"))
    result = setup.restore_books(target)
    assert result["status"] == "restored" and result["restored"] == 2
    for entry in manifest["assembled_pdfs"]:
        restored = target / "cache/archive" / entry["path"]
        assert restored.read_bytes() == (original / entry["path"]).read_bytes()
    assert setup.restore_books(target)["restored"] == 0
    assert not list(target.glob(".restore-books-*"))
    assert not list(target.glob("**/pages/*.pdf"))


def test_declared_full_archive_is_required_during_install(tmp_path):
    downloads, manifest, _, original = fixture_install_pack(tmp_path)
    add_full_archive(downloads, manifest, original)
    (downloads / "full-pdfs.tar.xz").unlink()
    with pytest.raises(ValueError, match="downloaded_file_identity_mismatch"):
        setup.install_pack(downloads, tmp_path / "installed")
    assert not (tmp_path / "installed").exists()
    assert not list(tmp_path.glob(".pack-install-*"))


@pytest.mark.parametrize("corruption", ["member_hash", "symlink", "traversal", "missing"])
def test_full_restore_failure_publishes_no_partial_books(tmp_path, corruption):
    downloads, manifest, _, original = fixture_install_pack(tmp_path)
    members = add_full_archive(downloads, manifest, original)
    if corruption == "member_hash":
        members[0] = (members[0][0], b"x" * len(members[0][1]), "file")
    elif corruption == "symlink":
        members[0] = (members[0][0], b"", "symlink")
    elif corruption == "traversal":
        members.append(("../../outside", b"bad", "file"))
    else:
        members.pop()
    archive = downloads / "full-pdfs.tar.xz"
    write_shard(archive, members)
    manifest["full_pdfs_archive"].update(sha256=setup.file_hash(archive), bytes=archive.stat().st_size)
    (downloads / "source-pack.json").write_text(json.dumps(manifest))
    target = tmp_path / "installed"
    setup.install_pack(downloads, target)
    with pytest.raises(ValueError):
        setup.restore_books(target)
    assert not list((target / "cache/archive").rglob("*.pdf"))
    assert not list(target.glob(".restore-books-*"))
