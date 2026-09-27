"""Actual local PDF intake, immutable source identity and shared page retrieval."""
import json
from pathlib import Path
import shutil
import sqlite3
from unittest.mock import patch

import pytest

from ontology_engineering.local_pdf_sources import (
    LocalPDFSources, MANIFEST_SCHEMA, register_sources,
)
from ontology_engineering.misumi.catalog import digest, file_hash


POPPLER = bool(shutil.which("pdfinfo") and shutil.which("pdftotext"))
pytestmark = pytest.mark.skipif(not POPPLER, reason="local Poppler not installed")


def pdf_fixture(path, pages):
    """Write a small real PDF without optional Python libraries or downloads."""
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>", b"", b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
    page_ids = []
    for text in pages:
        page_id = len(objects) + 1
        stream_id = page_id + 1
        page_ids.append(page_id)
        objects.append((f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 3 0 R >> >> /Contents {stream_id} 0 R >>").encode())
        if text:
            escaped = text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
            stream = ("BT /F1 10 Tf 20 750 Td (" + escaped + ") Tj ET").encode("ascii")
        else:
            stream = b"0.3 g 30 30 90 90 re f"  # visible graphics, no text layer
        objects.append(b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"\nendstream")
    objects[1] = ("<< /Type /Pages /Count " + str(len(pages)) + " /Kids [" + " ".join(f"{i} 0 R" for i in page_ids) + "] >>").encode()
    output = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(output))
        output.extend(str(i).encode() + b" 0 obj\n" + obj + b"\nendobj\n")
    xref = len(output)
    output.extend((f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n").encode())
    for offset in offsets[1:]:
        output.extend(f"{offset:010d} 00000 n \n".encode())
    output.extend((f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").encode())
    path.write_bytes(output)
    return path


def manifest_fixture(tmp_path, scan=False):
    source = pdf_fixture(tmp_path / "original.pdf", ["Opening page about materials.", "Shaft mechanism under stated load conditions. Preserve the operating assumptions and check all interfaces before application."] if not scan else ["", ""])
    manifest = {"schema": MANIFEST_SCHEMA, "sources": [{
        "id": "mechanics", "title": "Mechanics fixture", "edition": "Test edition 1", "path": "original.pdf",
        "sha256": file_hash(source), "chapters": [{"id": "mechanism", "title": "Mechanism",
            "start_page": 2, "end_page": 2, "aliases": ["运动副", "轴机构"]}]}]}
    path = tmp_path / "manifest.json"
    path.write_text(json.dumps(manifest, ensure_ascii=False))
    return path, manifest, source


def save_manifest(path, manifest):
    path.write_text(json.dumps(manifest, ensure_ascii=False))


def test_native_registration_reuses_page_fields_and_keeps_original_pdf_identity(tmp_path):
    manifest, _, original = manifest_fixture(tmp_path)
    before = original.read_bytes()
    result = register_sources(manifest, tmp_path / "data")
    assert result["status"] == "registered"
    assert result["index"]["counts"]["pages"] == 2
    registration = json.loads((tmp_path / "data/registration.json").read_text())
    assert registration["sources"][0]["path"] == str(original)
    provider = LocalPDFSources(tmp_path / "data")
    try:
        assert provider.status()["status"] == "ready"
        assert provider.meta["fingerprint"]
        pages = provider.retrieve("shaft", None, 8)
        assert len(pages) == 1 and pages[0]["pdf_page"] == 2
        evidence = provider.source_result(pages[0], "shaft")
        assert evidence["source_pdf"] == str(original)
        assert evidence["source_sha256"] == file_hash(original)
        assert evidence["pdf_uri"].endswith("#page=2")
        assert evidence["source_locator"] == {
            "schema": "ontology-engineering.source-page/v1", "kind": "local_pdf", "document_id": "mechanics",
            "document_sha256": file_hash(original), "physical_page": 2, "page_count": 2,
            "title": "Mechanics fixture", "edition": "Test edition 1", "printed_page": None,
            "printed_page_status": "unverified"}
        assert evidence["text_origin"] == "poppler_native_text"
        assert evidence["excerpt"]["text"] in pages[0]["body"]
        assert original.read_bytes() == before
        assert {p.name for p in (tmp_path / "data").iterdir()} == {"registration.json", "catalog.sqlite3"}
    finally:
        provider.close()


@pytest.mark.skipif(not shutil.which("pdftoppm"), reason="local Poppler renderer not installed")
def test_books_and_relative_index_relocate_without_old_sources_and_render_same_page(tmp_path):
    from ontology_engineering.source_citations import render_page

    old_library = tmp_path / "author location" / "sources"
    books = old_library / "books"
    books.mkdir(parents=True)
    manifest, _, original = manifest_fixture(books)
    index = old_library / ".indexes/local-pdf"
    registered = register_sources(manifest, index, relative_paths=True)
    registration_bytes = (index / "registration.json").read_bytes()
    registration = json.loads(registration_bytes)
    assert registration["sources"][0]["path"] == "../../books/original.pdf"
    assert str(old_library) not in registration_bytes.decode()
    before = LocalPDFSources(index)
    try:
        old_result = before.source_result(before.retrieve("shaft", None, 8)[0], "shaft")
        old_view = render_page(old_result, tmp_path / "old-render", dpi=72)
    finally:
        before.close()

    moved_library = tmp_path / "received skill" / "sources"
    shutil.copytree(old_library, moved_library)
    shutil.rmtree(old_library.parent)
    assert not original.exists()
    moved_index = moved_library / ".indexes/local-pdf"
    assert (moved_index / "registration.json").read_bytes() == registration_bytes
    provider = LocalPDFSources(moved_index)
    try:
        assert provider.status()["status"] == "ready"
        assert provider.meta["fingerprint"] == registered["index"]["fingerprint"]
        page = provider.retrieve("shaft", None, 8)[0]
        result = provider.source_result(page, "shaft")
        moved_original = moved_library / "books/original.pdf"
        assert result["source_pdf"] == str(moved_original)
        assert result["pdf_uri"] == moved_original.as_uri() + "#page=2"
        for key in ("id", "source_sha256", "text_sha256", "source_locator"):
            assert result[key] == old_result[key]
        view = render_page(result, tmp_path / "moved-render", dpi=72)
        assert view["identity"]["source_file_page"] == 2
        assert view["image_sha256"] == old_view["image_sha256"]
        moved_original.write_bytes(moved_original.read_bytes() + b"\n% changed after relocation\n")
        with pytest.raises(ValueError, match="source_changed_reindex_required"):
            provider.source_result(page, "shaft")
    finally:
        provider.close()


def test_chapter_alias_and_route_find_foreign_text_without_becoming_source_text(tmp_path):
    manifest, _, _ = manifest_fixture(tmp_path)
    register_sources(manifest, tmp_path / "data")
    provider = LocalPDFSources(tmp_path / "data")
    try:
        chapters = provider.chapters()
        assert chapters[0]["aliases"] == ["运动副", "轴机构"]
        pages = provider.retrieve("运动副", None, 8)
        assert len(pages) == 1 and pages[0]["pdf_page"] == 2
        assert "运动副" not in pages[0]["body"]
        assert pages[0]["text_sha256"] == digest(pages[0]["body"].encode())
        routed = provider.retrieve("完全没有词汇匹配", {"chapters": chapters}, 8)
        assert len(routed) == 1 and routed[0]["pdf_page"] == 2
        assert routed[0]["retrieval_pool"] == "local_pdf_selected_chapter"
    finally:
        provider.close()


def test_scan_can_be_navigated_but_never_gains_invented_body(tmp_path):
    manifest, _, _ = manifest_fixture(tmp_path, scan=True)
    register_sources(manifest, tmp_path / "data")
    provider = LocalPDFSources(tmp_path / "data")
    try:
        assert provider.meta["counts"]["native_text_missing"] == 2
        pages = provider.retrieve("运动副", None, 8)
        assert len(pages) == 1
        assert pages[0]["native_text_missing"] and pages[0]["navigation_only"]
        assert not pages[0]["body"].strip()
        result = provider.source_result(pages[0], "运动副")
        assert result["native_text_missing"] and result["navigation_only"]
        assert not result["excerpt"]["text"].strip()
        assert result["printed_page"] is None
    finally:
        provider.close()


def test_each_document_gets_a_candidate_before_one_book_fills_the_limit(tmp_path):
    path, manifest, _ = manifest_fixture(tmp_path)
    first = pdf_fixture(tmp_path / "original.pdf", ["Shaft shaft shaft mechanism." for _ in range(3)])
    manifest["sources"][0]["sha256"] = file_hash(first)
    second = pdf_fixture(tmp_path / "second.pdf", ["A shaft under load."])
    manifest["sources"].append({"id": "second", "title": "Second", "edition": "1", "path": "second.pdf",
        "sha256": file_hash(second), "chapters": []})
    save_manifest(path, manifest)
    register_sources(path, tmp_path / "data")
    provider = LocalPDFSources(tmp_path / "data")
    try:
        pages = provider.retrieve("shaft", None, 2)
        assert len(pages) == 2
        assert {page["catalog"] for page in pages} == {"local_pdf:mechanics", "local_pdf:second"}
    finally:
        provider.close()


def test_selected_chapter_and_global_lexical_pool_both_survive(tmp_path):
    path, _, _ = manifest_fixture(tmp_path)
    register_sources(path, tmp_path / "data")
    provider = LocalPDFSources(tmp_path / "data")
    try:
        pages = provider.retrieve("materials", {"chapters": provider.chapters()}, 2)
        assert [page["pdf_page"] for page in pages] == [2, 1]
        assert pages[0]["retrieval_pool"] == "local_pdf_selected_chapter"
        assert pages[1]["retrieval_pool"] == "local_pdf_native_or_declared_navigation"
    finally:
        provider.close()


def test_small_cross_book_budget_retains_selected_foreign_language_chapters(tmp_path):
    sources = []
    for number in range(4):
        pdf = pdf_fixture(tmp_path / f"book-{number}.pdf", [
            "Precision materials overview with a lexical match.",
            "Mechanism interfaces under stated load conditions. Check assumptions before applying the method."])
        sources.append({"id": f"book-{number}", "title": f"Book {number}", "edition": "synthetic",
            "path": pdf.name, "sha256": file_hash(pdf), "chapters": [{"id": "method",
                "title": "Method", "start_page": 2, "end_page": 2, "aliases": []}]})
    manifest = tmp_path / "manifest.json"
    save_manifest(manifest, {"schema": MANIFEST_SCHEMA, "sources": sources})
    register_sources(manifest, tmp_path / "data")
    provider = LocalPDFSources(tmp_path / "data")
    try:
        route = {"chapters": [c for c in provider.chapters()
                              if c["catalog"] in {"local_pdf:book-2", "local_pdf:book-3"}]}
        pages = provider.retrieve("precision", route, 2)
        assert {(p["catalog"], p["pdf_page"]) for p in pages} == {
            ("local_pdf:book-2", 2), ("local_pdf:book-3", 2)}
        assert all(p["retrieval_pool"] == "local_pdf_selected_chapter" for p in pages)
        assert all("precision" not in p["body"].lower() for p in pages)
        # The routed reservations still leave the remaining quota available to
        # independent lexical sources, without duplicates or a larger budget.
        expanded = provider.retrieve("precision", route, 4)
        assert len(expanded) == len({p["id"] for p in expanded}) == 4
        assert [p["id"] for p in expanded[:2]] == [p["id"] for p in pages]
        assert {p["catalog"] for p in expanded} == {f"local_pdf:book-{n}" for n in range(4)}
    finally:
        provider.close()


@pytest.mark.parametrize("fault", ["wrong_hash", "outside", "reversed", "bool_page", "duplicate_source", "duplicate_chapter"])
def test_bad_manifest_or_identity_never_publishes_partial_index(tmp_path, fault):
    manifest_path, manifest, _ = manifest_fixture(tmp_path)
    source = manifest["sources"][0]
    chapter = source["chapters"][0]
    if fault == "wrong_hash": source["sha256"] = "0" * 64
    elif fault == "outside": chapter["end_page"] = 3
    elif fault == "reversed": chapter["start_page"], chapter["end_page"] = 2, 1
    elif fault == "bool_page": chapter["start_page"] = True
    elif fault == "duplicate_source": manifest["sources"].append(source.copy())
    elif fault == "duplicate_chapter": source["chapters"].append(chapter.copy())
    save_manifest(manifest_path, manifest)
    with pytest.raises(ValueError): register_sources(manifest_path, tmp_path / "data")
    assert not (tmp_path / "data").exists()
    assert not list(tmp_path.glob(".local-pdf-*"))


def test_replace_is_explicit_and_failed_replacement_keeps_old_index(tmp_path):
    manifest_path, manifest, _ = manifest_fixture(tmp_path)
    target = tmp_path / "data"
    register_sources(manifest_path, target)
    before = {p.name: p.read_bytes() for p in target.iterdir()}
    with pytest.raises(FileExistsError): register_sources(manifest_path, target)
    manifest["sources"][0]["sha256"] = "0" * 64
    save_manifest(manifest_path, manifest)
    with pytest.raises(ValueError, match="hash_mismatch"): register_sources(manifest_path, target, replace=True)
    assert {p.name: p.read_bytes() for p in target.iterdir()} == before
    assert not list(tmp_path.glob(".local-pdf-*"))


def test_successful_replace_changes_identity_and_accepts_absolute_source_path(tmp_path):
    path, manifest, original = manifest_fixture(tmp_path)
    root = tmp_path / "data"
    before = register_sources(path, root)["index"]["fingerprint"]
    manifest["sources"][0].update(path=str(original), edition="Test edition 2")
    save_manifest(path, manifest)
    after = register_sources(path, root, replace=True)["index"]["fingerprint"]
    assert before != after
    provider = LocalPDFSources(root)
    try:
        page = provider.retrieve("shaft", None, 8)[0]
        assert provider.source_result(page, "shaft")["source_locator"]["edition"] == "Test edition 2"
        assert not list(tmp_path.glob(".local-pdf-*"))
        assert root.stat().st_mode & 0o777 == 0o700
        assert all(p.stat().st_mode & 0o777 == 0o600 for p in root.iterdir())
    finally:
        provider.close()


def test_missing_poppler_preserves_inputs_and_reports_dependency(tmp_path):
    manifest, _, original = manifest_fixture(tmp_path)
    before = original.read_bytes()
    with patch("ontology_engineering.local_pdf_sources.subprocess.run", side_effect=FileNotFoundError):
        with pytest.raises(ValueError, match="poppler_required_for_registration"):
            register_sources(manifest, tmp_path / "data")
    assert original.read_bytes() == before
    assert not (tmp_path / "data").exists()
    assert not list(tmp_path.glob(".local-pdf-*"))


@pytest.mark.parametrize("relative_paths", [False, True])
def test_source_change_during_extraction_is_rejected(tmp_path, relative_paths):
    path, _, original = manifest_fixture(tmp_path)
    from ontology_engineering import local_pdf_sources
    run = local_pdf_sources.subprocess.run
    def changing_run(argv, **kwargs):
        response = run(argv, **kwargs)
        if argv[0] == "pdftotext":
            original.write_bytes(original.read_bytes() + b"\n% changed during extraction\n")
        return response
    with patch.object(local_pdf_sources.subprocess, "run", side_effect=changing_run):
        with pytest.raises(ValueError, match="source_changed_during_registration"):
            register_sources(path, tmp_path / "data", relative_paths=relative_paths)
    assert not (tmp_path / "data").exists()


def test_refuses_to_replace_unrelated_files_or_a_source_inside_index(tmp_path):
    path, manifest, original = manifest_fixture(tmp_path)
    root = tmp_path / "data"
    root.mkdir()
    unrelated = root / "project.txt"
    unrelated.write_text("keep")
    with pytest.raises(ValueError, match="replace_requires_owned_registration"):
        register_sources(path, root, replace=True)
    assert unrelated.read_text() == "keep"
    inner_root = tmp_path / "new-index"
    # A manifest pointing at its own future index root is never permitted.
    inner_root.mkdir()
    moved = inner_root / "original.pdf"
    moved.write_bytes(original.read_bytes())
    manifest["sources"][0]["path"] = str(moved)
    save_manifest(path, manifest)
    with pytest.raises(ValueError, match="replace_requires_owned_registration"):
        register_sources(path, inner_root, replace=True)
    assert moved.read_bytes() == original.read_bytes()


def test_changed_original_is_rejected_on_every_result_even_same_provider(tmp_path):
    manifest, _, original = manifest_fixture(tmp_path)
    register_sources(manifest, tmp_path / "data")
    provider = LocalPDFSources(tmp_path / "data")
    try:
        page = provider.retrieve("shaft", None, 8)[0]
        provider.source_result(page, "shaft")
        original.write_bytes(original.read_bytes() + b"\n% modified\n")
        with pytest.raises(ValueError, match="source_changed_reindex_required"):
            provider.source_result(page, "shaft")
    finally:
        provider.close()


@pytest.mark.parametrize("fault", ["index", "registration", "metadata"])
def test_tampered_index_registration_and_metadata_are_not_ready(tmp_path, fault):
    manifest, _, _ = manifest_fixture(tmp_path)
    root = tmp_path / "data"
    register_sources(manifest, root)
    registration = json.loads((root / "registration.json").read_text())
    if fault == "index":
        with sqlite3.connect(root / "catalog.sqlite3") as db:
            db.execute("UPDATE pages SET body='invented'")
    elif fault == "registration":
        registration["sources"][0]["title"] = "changed"
        (root / "registration.json").write_text(json.dumps(registration))
    else:
        with sqlite3.connect(root / "catalog.sqlite3") as db:
            db.execute("UPDATE meta SET value=? WHERE key='counts'", (json.dumps({"pages": 99}),))
        registration["index_sha256"] = file_hash(root / "catalog.sqlite3")
        (root / "registration.json").write_text(json.dumps(registration))
    provider = LocalPDFSources(root)
    assert provider.status()["status"] == "not_ready"
    with pytest.raises(ValueError): provider.retrieve("shaft", None, 8)


def test_queries_do_not_need_poppler_or_launch_any_process(tmp_path):
    manifest, _, _ = manifest_fixture(tmp_path)
    register_sources(manifest, tmp_path / "data")
    with patch("ontology_engineering.local_pdf_sources.subprocess.run", side_effect=AssertionError("query must not launch tools")):
        provider = LocalPDFSources(tmp_path / "data")
        try:
            page = provider.retrieve("shaft", None, 8)[0]
            provider.source_result(page, "shaft")
            assert provider.status()["network_requests"] == 0
        finally:
            provider.close()


def test_missing_index_and_altered_candidate_are_explicit_failures(tmp_path):
    assert LocalPDFSources(tmp_path / "missing").status()["status"] == "not_ready"
    manifest, _, _ = manifest_fixture(tmp_path)
    register_sources(manifest, tmp_path / "data")
    provider = LocalPDFSources(tmp_path / "data")
    try:
        page = provider.retrieve("shaft", None, 8)[0]
        with pytest.raises(ValueError, match="candidate_identity_mismatch"):
            provider.source_result({**page, "body": "invented"}, "shaft")
    finally:
        provider.close()
