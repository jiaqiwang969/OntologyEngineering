"""Lossless local source recovery; fixtures never perform network requests."""
import io
import json
from pathlib import Path
import shutil
import tarfile
from unittest.mock import patch

import pytest

from ontology_engineering import supplier_knowledge
from ontology_engineering.misumi import catalog, cli, source_pack
from ontology_engineering.misumi.paths import readiness, runtime_config
from test_jev_knowledge_portable import args, fixture_data


def write_shard(path, members):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(path, "w:xz") as tar:
        for name, data, kind in members:
            item = tarfile.TarInfo(name)
            if kind == "symlink":
                item.type, item.linkname = tarfile.SYMTYPE, "../../outside"
                tar.addfile(item)
            else:
                item.size = len(data)
                tar.addfile(item, io.BytesIO(data))


def fixture_pack(tmp_path):
    expanded = tmp_path / "expanded"
    original = fixture_data(expanded)
    root = tmp_path / "compressed"
    root.mkdir()
    shutil.copy2(expanded / "catalog.sqlite3", root / "catalog.sqlite3")
    relative = original.relative_to(expanded / "archive").as_posix()
    data = original.read_bytes()
    members = [(relative, data, "file"), ("tmp/pdfs/unused/pages/0002.pdf", b"untouched original image bytes", "file")]
    segment = root / "segments/pages-0001.tar.xz"
    write_shard(segment, members)
    assembly = expanded / "archive/output/pdf/catalog.pdf"
    manifest = {"schema": source_pack.SCHEMA,
        "index": {"path": "catalog.sqlite3", "sha256": catalog.file_hash(root / "catalog.sqlite3"),
                  "bytes": (root / "catalog.sqlite3").stat().st_size},
        "segments": [{"id": "pages-0001", "path": segment.relative_to(root).as_posix(),
                      "sha256": catalog.file_hash(segment), "bytes": segment.stat().st_size,
                      "files": [{"path": name, "sha256": catalog.digest(blob), "bytes": len(blob)}
                                for name, blob, _ in members]}],
        "assembled_pdfs": [{"catalog": catalog.CATALOG_CODES[0], "path": "output/pdf/catalog.pdf",
                            "sha256": catalog.file_hash(assembly), "bytes": assembly.stat().st_size,
                            "download_url": "https://example.invalid/manual-full-original.pdf"}]}
    (root / "source-pack.json").write_text(json.dumps(manifest))
    return root, manifest, members, assembly


def rewrite_pack(root, manifest, members):
    segment = root / manifest["segments"][0]["path"]
    write_shard(segment, members)
    manifest["segments"][0].update(sha256=catalog.file_hash(segment), bytes=segment.stat().st_size)
    (root / "source-pack.json").write_text(json.dumps(manifest))


def test_query_recovers_only_selected_exact_page_and_preserves_absent_full_pdf_status(tmp_path):
    root, manifest, members, _ = fixture_pack(tmp_path)
    config = runtime_config(root, tmp_path / "state")
    assert readiness(config)["status"] == "ready"
    assert not (root / "cache").exists()
    with patch.object(cli.Judge, "__init__", side_effect=AssertionError("no network or credentials")):
        report = cli.search(config, args())
    assert report["status"] == "completed"
    result = report["results"][0]
    restored = Path(result["source_pdf"])
    assert restored.read_bytes() == members[0][1]
    assert result["source_relative_path"] == members[0][0]
    assert [p for p in (root / "cache").rglob("*") if p.is_file()] == [restored]
    assert result["assembled_pdf_exists"] is False
    assert result["assembled_pdf_status"] == "not_downloaded"
    assert result["assembled_pdf_uri"] is None
    assert result["pdf_uri"] == restored.as_uri() + "#page=1"
    packet = supplier_knowledge.make_packet(report, report["context_snapshot"])
    source = packet["evidence_candidates"][0]["source"]
    assert source["assembled_pdf_status"] == "not_downloaded"
    assert source["assembled_pdf_download_url"] == manifest["assembled_pdfs"][0]["download_url"]
    assert source["pdf_uri"] == result["pdf_uri"]
    with patch.object(source_pack, "restore_page", side_effect=AssertionError("cached original is reused")):
        assert cli.search(config, args())["status"] == "completed"
    restored.write_bytes(b"corrupt cached source")
    changed = cli.search(config, args())
    assert changed["status"] == "partial"
    assert changed["errors"][0]["code"] == "source_changed_reindex_required"


def test_full_original_is_cited_only_after_exact_download_is_present(tmp_path):
    root, manifest, _, original = fixture_pack(tmp_path)
    config = runtime_config(root, tmp_path / "state")
    full = Path(config["archive_root"]) / manifest["assembled_pdfs"][0]["path"]
    full.parent.mkdir(parents=True)
    full.write_bytes(b"incorrect download")
    result = cli.search(config, args())["results"][0]
    assert result["assembled_pdf_exists"] is True
    assert result["assembled_pdf_status"] == "hash_mismatch"
    assert result["assembled_pdf_uri"] is None
    assert result["pdf_uri"] == Path(result["source_pdf"]).as_uri() + "#page=1"
    shutil.copyfile(original, full)
    report = cli.search(config, args())
    result = report["results"][0]
    assert result["assembled_pdf_status"] == "verified_present"
    assert result["assembled_pdf_uri"] == full.as_uri()
    assert result["pdf_uri"] == full.as_uri() + "#page=1"
    assert supplier_knowledge.make_packet(report, report["context_snapshot"])["evidence_candidates"][0]["source"]["assembled_pdf_status"] == "verified_present"


def test_missing_segment_is_explicit_and_never_creates_cache(tmp_path):
    root, manifest, _, _ = fixture_pack(tmp_path)
    (root / manifest["segments"][0]["path"]).unlink()
    config = runtime_config(root, tmp_path / "state")
    status = readiness(config)
    assert status["status"] == "not_ready"
    assert status["missing"][0]["kind"] == "missing_source_segment"
    assert "never download" in status["setup"]["instruction"]
    report = cli.search(config, args())
    assert report["errors"] == [{"code": "missing_source_segment"}]
    assert not (root / "cache").exists()


def test_query_does_not_require_optional_full_book_archive(tmp_path):
    root, manifest, _, _ = fixture_pack(tmp_path)
    manifest["full_pdfs_archive"] = {"path": "full-pdfs.tar.xz", "sha256": "0" * 64, "bytes": 10,
        "files": [{"path": "full/catalog.pdf", "sha256": manifest["assembled_pdfs"][0]["sha256"],
                   "bytes": manifest["assembled_pdfs"][0]["bytes"]}]}
    (root / "source-pack.json").write_text(json.dumps(manifest))
    config = runtime_config(root, tmp_path / "state")
    assert readiness(config)["status"] == "ready"
    report = cli.search(config, args())
    assert report["status"] == "completed"
    assert report["results"][0]["assembled_pdf_status"] == "not_downloaded"
    assert report["usage"]["network_requests"] == 0
    # The same-package original archive is present, but not yet materialized.
    # Page queries do not claim its bytes were checked or restore both books.
    (root / "full-pdfs.tar.xz").write_bytes(b"fixture compressed archive locator")
    report = cli.search(config, args())
    assert report["results"][0]["assembled_pdf_status"] == "not_restored"
    packet = supplier_knowledge.make_packet(report, report["context_snapshot"])
    assert packet["evidence_candidates"][0]["source"]["assembled_pdf_status"] == "not_restored"


def test_changed_index_is_not_a_valid_pack(tmp_path):
    root, _, _, _ = fixture_pack(tmp_path)
    with (root / "catalog.sqlite3").open("ab") as stream:
        stream.write(b"changed")
    assert readiness(runtime_config(root, tmp_path / "state"))["missing"][0]["kind"] == "source_pack_index_changed"


def test_full_layout_verification_hashes_shards_and_reports_missing_index(tmp_path):
    root, manifest, _, _ = fixture_pack(tmp_path)
    assert source_pack.check_layout(root, manifest, verify_segments=True) == []
    shard = root / manifest["segments"][0]["path"]
    data = bytearray(shard.read_bytes())
    data[len(data) // 2] ^= 1
    shard.write_bytes(data)
    assert source_pack.check_layout(root, manifest) == []
    assert source_pack.check_layout(root, manifest, verify_segments=True)[0]["kind"] == "source_segment_hash_mismatch"
    (root / "catalog.sqlite3").unlink()
    status = readiness(runtime_config(root, tmp_path / "state"))
    assert [entry["kind"] for entry in status["missing"]] == ["index_missing"]
    assert not (root / "cache").exists()


@pytest.mark.parametrize("damage", ["shard_hash", "page_hash", "traversal", "symlink", "duplicate", "missing_member"])
def test_damaged_shard_never_publishes_partial_or_other_pages(tmp_path, damage):
    root, manifest, members, _ = fixture_pack(tmp_path)
    if damage == "shard_hash":
        segment = root / manifest["segments"][0]["path"]
        raw = bytearray(segment.read_bytes())
        raw[len(raw) // 2] ^= 1
        segment.write_bytes(raw)
    elif damage == "page_hash":
        members[0] = (members[0][0], b"x" * len(members[0][1]), "file")
    elif damage == "traversal":
        members.append(("../../outside", b"forbidden", "file"))
    elif damage == "symlink":
        members[0] = (members[0][0], b"", "symlink")
    elif damage == "duplicate":
        members.append(members[0])
    else:
        members.pop()
    if damage != "shard_hash":
        rewrite_pack(root, manifest, members)
    archive = root / "cache/archive"
    entry = manifest["segments"][0]["files"][0]
    with pytest.raises(ValueError, match="source_"):
        source_pack.restore_page(archive, entry["path"], entry["sha256"])
    assert not [p for p in (root / "cache").rglob("*") if p.is_file()]
    assert not (tmp_path / "outside").exists()


def test_cache_symlink_cannot_redirect_source_writes(tmp_path):
    root, manifest, _, _ = fixture_pack(tmp_path)
    elsewhere = tmp_path / "outside"
    elsewhere.mkdir()
    (root / "cache").symlink_to(elsewhere, target_is_directory=True)
    entry = manifest["segments"][0]["files"][0]
    with pytest.raises(ValueError, match="symlink"):
        source_pack.restore_page(root / "cache/archive", entry["path"], entry["sha256"])
    assert list(elsewhere.iterdir()) == []


@pytest.mark.parametrize("relative", ["../escape", "/absolute", "x\\y", "x//y", ".", ""])
def test_manifest_paths_remain_canonical_relative(tmp_path, relative):
    root, manifest, _, _ = fixture_pack(tmp_path)
    manifest["segments"][0]["files"][0]["path"] = relative
    (root / "source-pack.json").write_text(json.dumps(manifest))
    with pytest.raises(ValueError, match="source_pack_invalid_path"):
        source_pack.load_pack(root)


def test_packet_does_not_drop_or_forge_missing_full_original_status(tmp_path):
    root, _, _, _ = fixture_pack(tmp_path)
    report = cli.search(runtime_config(root, tmp_path / "state"), args())
    report["results"][0]["assembled_pdf_uri"] = Path(report["results"][0]["assembled_pdf"]).as_uri()
    with pytest.raises(ValueError, match="assembled_uri"):
        supplier_knowledge.make_packet(report, report["context_snapshot"])
