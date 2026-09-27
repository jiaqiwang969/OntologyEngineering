"""Source placement, immutable bytes and usable registration, without Drive writes."""
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from ontology_engineering import source_library as library
from ontology_engineering.local_pdf_sources import LocalPDFSources
from ontology_engineering.misumi import paths
from test_local_pdf_sources import pdf_fixture, POPPLER


@pytest.fixture
def sources(tmp_path):
    incoming = tmp_path / "browser-download"
    incoming.mkdir()
    pdf = pdf_fixture(incoming / "mechanics.pdf", ["Opening page.", "Shaft support requires stated load and alignment conditions."])
    entry = {"relative_path": "books/mechanics.pdf", "title": "Synthetic mechanics",
             "bytes": pdf.stat().st_size, "sha256": hashlib.sha256(pdf.read_bytes()).hexdigest(),
             "drive": {"file_id": "synthetic_drive_id_0001", "view_url": "https://drive.google.com/file/d/synthetic_drive_id_0001/view"},
             "registration": {"id": "mechanics", "edition": "Synthetic edition",
                              "chapters": [{"id": "support", "title": "Shaft support", "start_page": 2, "end_page": 2, "aliases": []}]}}
    return incoming, pdf, entry, tmp_path / "skill/sources"


def test_flat_browser_download_is_placed_without_changing_or_overwriting(sources):
    incoming, original, entry, root = sources
    before = original.read_bytes()
    report = library.place_sources([entry], root, incoming=incoming)
    assert report["status"] == "placed" and report["network_download"] == "not_run"
    assert original.read_bytes() == before == (root / entry["relative_path"]).read_bytes()
    assert library.place_sources([entry], root, incoming=incoming)["files"][0]["status"] == "already_verified"
    assert library.inspect_sources([entry], root, verify=True)["status"] == "verified"


def test_wrong_bytes_remain_untouched_and_never_become_a_source(sources):
    incoming, original, entry, root = sources
    original.write_bytes(b"wrong download")
    with pytest.raises(ValueError, match="missing_or_mismatched"):
        library.place_sources([entry], root, incoming=incoming)
    assert not (root / entry["relative_path"]).exists()
    destination = root / entry["relative_path"]
    destination.parent.mkdir(parents=True)
    destination.write_bytes(b"existing user file")
    with pytest.raises(ValueError, match="existing_file"):
        library.place_sources([entry], root, incoming=incoming)
    assert destination.read_bytes() == b"existing user file"


@pytest.mark.parametrize("name", ["../outside.pdf", "/absolute.pdf", "books//a.pdf", "books/../a.pdf", "books\\a.pdf"])
def test_delivery_paths_cannot_escape_sources(tmp_path, name):
    with pytest.raises(ValueError, match="invalid_relative_path"):
        library.source_path(tmp_path, name)


def test_symlink_target_is_rejected(sources, tmp_path):
    incoming, _, entry, root = sources
    root.mkdir(parents=True)
    outside = tmp_path / "outside"
    outside.mkdir()
    (root / "books").symlink_to(outside)
    with pytest.raises(ValueError, match="symlink"):
        library.place_sources([entry], root, incoming=incoming)
    assert not list(outside.iterdir())


@pytest.mark.skipif(not POPPLER, reason="Poppler required")
def test_library_registration_uses_same_provider_and_survives_whole_directory_move(sources, tmp_path):
    incoming, original, entry, root = sources
    library.place_sources([entry], root, incoming=incoming)
    report = library.register_books([entry], root)
    assert report["status"] == "registered"
    registration = json.loads((root / ".indexes/local-pdf/registration.json").read_text())
    assert not Path(registration["sources"][0]["path"]).is_absolute()
    original.unlink()
    moved = tmp_path / "other-machine-skill"
    root.parent.rename(moved)
    provider = LocalPDFSources(moved / "sources/.indexes/local-pdf")
    try:
        result = provider.source_result(provider.retrieve("shaft")[0], "shaft")
        assert result["source_sha256"] == entry["sha256"]
        assert result["pdf_page"] == 2
        assert Path(result["source_pdf"]) == moved / "sources/books/mechanics.pdf"
    finally:
        provider.close()


def test_default_data_location_never_falls_back_to_legacy_external_cache(tmp_path, monkeypatch):
    skill, home = tmp_path / "skill", tmp_path / "home"
    monkeypatch.setattr(paths, "SKILL_ROOT", skill)
    monkeypatch.setattr(Path, "home", classmethod(lambda cls: home))
    local = skill / "sources/.indexes/local-pdf"
    assert paths.default_data_root("local-pdf") == local
    legacy = home / ".local/share/ontology-engineering/knowledge-sources/local-pdf"
    legacy.mkdir(parents=True)
    assert paths.default_data_root("local-pdf") == local
    local.mkdir(parents=True)
    assert paths.default_data_root("local-pdf") == local


def test_cli_rejects_external_or_escaped_source_root_before_download(tmp_path, monkeypatch, capsys):
    skill = tmp_path / "installed-skill"
    skill.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()
    (skill / "escaped").symlink_to(outside, target_is_directory=True)
    monkeypatch.setattr(library, "ROOT", skill)
    def unexpected(*args, **kwargs):
        raise AssertionError("an invalid destination must be rejected before reading the manifest or using gws")
    monkeypatch.setattr(library, "load_delivery", unexpected)
    for destination in (outside, skill / "escaped"):
        assert library.main(["fetch", "--select", "books", "--root", str(destination)]) == 2
        assert json.loads(capsys.readouterr().out)["error"] == "source_root_must_be_inside_skill"
    assert not list(outside.iterdir())


def test_gws_fetch_is_explicit_and_checks_download_identity(sources, monkeypatch):
    _, original, entry, root = sources
    observed = []
    def get(command, **kwargs):
        observed.append(command)
        assert command[:4] == ["gws", "drive", "files", "get"]
        assert json.loads(command[5]) == {"fileId": entry["drive"]["file_id"], "alt": "media"}
        Path(command[-1]).write_bytes(original.read_bytes())
        return SimpleNamespace(returncode=0)
    monkeypatch.setattr(library.subprocess, "run", get)
    assert library.inspect_sources([entry], root)["files"][0]["status"] == "missing"
    assert not observed
    assert library.place_sources([entry], root)["network_download"] == "gws_explicit_fetch"
    assert len(observed) == 1
    assert library.inspect_sources([entry], root, verify=True)["status"] == "verified"


def test_failed_fetch_discards_partial_file(sources, monkeypatch):
    _, _, entry, root = sources
    def get(command, **kwargs):
        Path(command[-1]).write_bytes(b"login page instead of PDF")
        return SimpleNamespace(returncode=0)
    monkeypatch.setattr(library.subprocess, "run", get)
    with pytest.raises(ValueError, match="download_identity_mismatch"):
        library.place_sources([entry], root)
    assert not list(root.rglob("*.pdf")) and not list(root.rglob(".incoming-*"))


def test_shipped_delivery_has_six_portable_book_navigation_maps():
    delivery = library.load_delivery()
    books = library.select_entries(delivery, "books")
    assert len(books) == 6
    assert {b["registration"]["id"] for b in books} == {"vol1", "vol2", "roloff-matek", "norton", "dfma", "interchangeability"}
    assert sum(len(b["registration"]["chapters"]) for b in books) == 148
    for book in books:
        assert not any("path" == k for k in book["registration"])
        chapters = book["registration"]["chapters"]
        assert all(1 <= c["start_page"] <= c["end_page"] for c in chapters)
        if "page_count" in book["registration"]:
            assert max(c["end_page"] for c in chapters) == book["registration"]["page_count"]


@pytest.mark.parametrize("value", [None, [], {"schema": "wrong"}])
def test_bad_manifest_returns_structured_failure(tmp_path, capsys, value):
    path = tmp_path / "invalid.json"
    path.write_text(json.dumps(value))
    assert library.main(["list", "--manifest", str(path)]) == 2
    assert json.loads(capsys.readouterr().out)["error"] == "source_library_manifest_schema"


def test_fetch_without_selection_never_reaches_network(monkeypatch):
    def unexpected(*args, **kwargs):
        pytest.fail("unselected fetch reached the network")
    monkeypatch.setattr(library.subprocess, "run", unexpected)
    with pytest.raises(SystemExit) as exc:
        library.main(["fetch"])
    assert exc.value.code == 2
