"""Unified CLI behavior over real, private textbook and supplier PDF fixtures.

The supplier index is a small frozen fixture, not a rebuild-parser test. All
queries use local retrieval or the existing controlled Jev protocol fixture.
"""
import copy
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys

import pytest

from ontology_engineering import supplier_knowledge as sk
from ontology_engineering.local_pdf_sources import MANIFEST_SCHEMA, TABLES, register_sources
from ontology_engineering.misumi import catalog, cli
from ontology_engineering.misumi.paths import SKILL_ROOT
from ontology_engineering.misumi import paths
from test_local_pdf_sources import pdf_fixture
from test_misumi_provider import RecordingJudge


pytestmark = pytest.mark.skipif(
    not all(shutil.which(name) for name in ("pdfinfo", "pdftotext", "pdftoppm")),
    reason="local Poppler tools required for real PDF integration",
)


class OfflineJudge(RecordingJudge):
    instances = []

    def __init__(self, root, config, fingerprint, no_cache=False):
        super().__init__(config["model"])
        self.stats = {"network_requests": 0, "cache_hits": 0}
        self.calls = []
        self.instances.append(self)


@pytest.fixture(autouse=True)
def no_online_judge(monkeypatch, tmp_path):
    OfflineJudge.instances = []
    monkeypatch.setattr(cli, "Judge", OfflineJudge)
    monkeypatch.setattr(paths, "SKILL_ROOT", tmp_path)


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    return path


def local_book(root, *, pages=None, scan=False):
    root.mkdir(parents=True)
    if pages is None:
        pages = ["Opening page about materials.", "" if scan else
                 "Shaft mechanism with stated load conditions. Preserve all operating assumptions and inspect the interfaces before application."]
    original = pdf_fixture(root / "original.pdf", pages)
    manifest = {"schema": MANIFEST_SCHEMA, "sources": [{
        "id": "mechanics", "title": "Mechanics fixture", "edition": "Edition 1",
        "path": "original.pdf", "sha256": catalog.file_hash(original),
        "chapters": [{"id": "shaft", "title": "Shaft navigation heading",
                      "start_page": 2, "end_page": len(pages), "aliases": ["导航专用词"]}],
    }]}
    path = write_json(root / "manifest.json", manifest)
    register_sources(path, root / "index")
    return root / "index", original, manifest


def supplier_archive(root, count=6):
    """Frozen MISUMI-format rows, each tied to a real independently saved PDF."""
    archive = root / "archive"
    archive.mkdir(parents=True)
    code = catalog.CATALOG_CODES[0]
    bodies = [f"Shaft supplier page {n}. Select only within the stated loading and installation conditions. Verify the complete configuration before use."
              for n in range(1, count + 1)]
    assembled = pdf_fixture(archive / "complete.pdf", ["Supplier opening page.", *bodies])
    with sqlite3.connect(root / "catalog.sqlite3") as db:
        db.executescript(TABLES + """
            CREATE TABLE catalogs(code TEXT PRIMARY KEY,label TEXT,title TEXT,page_count INTEGER,
              pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT);
        """)
        db.execute("INSERT INTO catalogs VALUES (?,?,?,?,?,?,?,?)", (
            code, "Synthetic supplier fixture", "Synthetic supplier fixture", count + 1,
            assembled.name, catalog.file_hash(assembled), "0" * 64, "0" * 64))
        cid = code + ":chapter:shaft"
        db.execute("INSERT INTO chapters VALUES (?,?,?,?,?,?)", (
            cid, code, "Shaft", 2, count + 1, "{}"))
        for n, text in enumerate(bodies, 1):
            source = pdf_fixture(archive / f"page-{n}.pdf", [text])
            raw = subprocess.run(["pdftotext", "-layout", str(source), "-"],
                                 capture_output=True, check=True).stdout.decode().removesuffix("\f")
            values = (f"{code}:page:{n}", code, n, n + 1, str(n), cid, source.name,
                      "https://example.invalid/synthetic.pdf", catalog.file_hash(source), source.stat().st_size,
                      raw, catalog.digest(raw.encode()), catalog.digest(raw.encode()), len(raw), len(raw),
                      0, "[]", "[]", 0, "{}")
            rowid = db.execute("INSERT INTO pages VALUES (" + ",".join("?" * len(values)) + ")", values).lastrowid
            db.execute("INSERT INTO search(rowid,models,titles,body,chapter) VALUES (?,?,?,?,?)",
                       (rowid, "", "", " ".join(catalog.tokens(raw)), "shaft"))
        meta = {"schema": catalog.INDEX_SCHEMA, "built_at": "2026-01-01T00:00:00Z",
                "counts": {"pages": count, "catalogs": 1, "chapters": 1}}
        meta["fingerprint"] = catalog.digest(meta)
        db.executemany("INSERT INTO meta VALUES (?,?)", [(k, json.dumps(v)) for k, v in meta.items()])
    return root


@pytest.fixture
def workspace(tmp_path):
    local, original, manifest = local_book(tmp_path / "books")
    context = sk.validate_context({"schema": sk.CONTEXT_SCHEMA, "task_id": "unified-sources",
        "project": {"id": "fixture-project", "revision": "P1"}, "decision_id": "fixture-decision",
        "object": {"id": "shaft", "name": "Shaft", "revision": "A"},
        "goal": "Compare shaft mechanisms while retaining source conditions."})
    return {"base": tmp_path, "local": local, "original": original, "manifest": manifest,
            "context": context, "context_path": write_json(tmp_path / "context.json", context),
            "misumi": tmp_path / "missing-misumi", "state": tmp_path / "state"}


def run_cli(capsys, workspace, *arguments):
    code = cli.main(["--data-root", str(workspace["misumi"]), "--pdf-root", str(workspace["local"]),
                     "--state-root", str(workspace["state"]), "--json", *map(str, arguments)])
    captured = capsys.readouterr()
    assert not captured.err, captured.err
    return code, json.loads(captured.out)


def search(capsys, workspace, *, sources="local-pdf", query="shaft", local=True, extra=()):
    return run_cli(capsys, workspace, query, "--sources", sources,
                   "--context", workspace["context_path"], "--full-text", "--candidates", "8", "--top", "8",
                   *( ["--local"] if local else []), *extra)


@pytest.mark.parametrize("local", [True, False])
def test_mixed_query_shares_one_candidate_budget_and_returns_both_sources(tmp_path, capsys, workspace, local):
    workspace["misumi"] = supplier_archive(tmp_path / "supplier")
    workspace["local"], _, _ = local_book(tmp_path / "many-books", pages=["Opening page."] + [
        f"Shaft textbook section {n}. Mechanism applicability depends on load, support and installation. Preserve conditions before adopting a conclusion."
        for n in range(6)])
    code, report = search(capsys, workspace, sources="all", local=local)
    assert code == 0 and report["status"] == "completed"
    assert report["retrieval"]["candidates_retrieved"] == 8
    assert len(report["evaluations"]) == len(report["results"]) == 8
    assert {r["provider"] for r in report["results"]} == {"misumi_archive", "local_pdf"}
    assert len({r["id"] for r in report["results"]}) == 8
    assert report["usage"]["network_requests"] == 0
    if not local:
        assert OfflineJudge.instances[0].payloads
    for result in report["results"]:
        assert result["source_sha256"] == catalog.file_hash(result["source_pdf"])
        assert result["text_sha256"] == catalog.digest(result["native_text"].encode())
        for unit in result["knowledge_units"]:
            span = unit["source_span"]
            assert span["text"] == result["native_text"][span["start"]:span["end"]]
            assert unit["applicability"] == "not_assessed"


def test_navigation_terms_never_become_original_text_or_knowledge(capsys, workspace):
    code, report = search(capsys, workspace, query="导航专用词", local=False)
    assert code == 0 and len(report["results"]) == 1
    page = report["results"][0]
    assert page["pdf_page"] == 2
    assert page["headings"] == ["Shaft navigation heading"]
    assert page["knowledge_units"] and "Shaft mechanism" in page["native_text"]
    for text in [page["native_text"], page["excerpt"]["text"], *[u["source_span"]["text"] for u in page["knowledge_units"]]]:
        assert "导航专用词" not in text and "navigation heading" not in text
    screened = [p["state"]["page_native_text"] for p in OfflineJudge.instances[0].payloads
                if "page_native_text" in p["state"]]
    assert screened and all("navigation heading" not in text for text in screened)


@pytest.mark.parametrize("local", [True, False])
def test_scan_navigation_returns_page_without_fabricated_knowledge(tmp_path, capsys, workspace, local):
    workspace["local"], _, _ = local_book(tmp_path / "scan", scan=True)
    code, report = search(capsys, workspace, query="导航专用词", local=local)
    assert code == 0 and len(report["results"]) == 1
    page = report["results"][0]
    assert page["navigation_only"] and page["native_text_missing"]
    assert page["judgment"]["relation"] == "navigation_only"
    assert not page["native_text"].strip() and not page["excerpt"]["text"].strip()
    assert page["knowledge_units"] == []
    assert page["source_locator"]["physical_page"] == 2
    if not local:
        assert not any("page_native_text" in p["state"] for p in OfflineJudge.instances[0].payloads)


@pytest.mark.parametrize("missing", ["misumi", "local-pdf", "both", "original"])
def test_explicit_source_failures_are_not_success_or_no_match(tmp_path, capsys, workspace, missing):
    if missing != "misumi" and missing != "both":
        workspace["misumi"] = supplier_archive(tmp_path / "supplier")
    if missing in {"local-pdf", "both"}:
        workspace["local"] = tmp_path / "missing-books"
    if missing == "original":
        workspace["original"].unlink()
    code, report = search(capsys, workspace, sources="all")
    assert code == 2
    assert report["status"] == ("not_ready" if missing == "both" else "partial")
    assert report["errors"]
    if missing == "both":
        assert report["results"] == []
    else:
        assert report["results"]
        assert len({r["provider"] for r in report["results"]}) == 1


def test_local_only_never_checks_missing_supplier_readiness(monkeypatch, capsys, workspace):
    def forbidden(_):
        raise AssertionError("local PDF query must not require a supplier installation")
    monkeypatch.setattr(cli, "readiness", forbidden)
    code, report = search(capsys, workspace)
    assert code == 0 and report["status"] == "completed"
    assert {r["provider"] for r in report["results"]} == {"local_pdf"}
    assert report["usage"]["network_requests"] == 0


def test_local_only_is_independent_of_broken_unselected_supplier_registration(capsys, workspace):
    workspace["misumi"].mkdir()
    write_json(workspace["misumi"] / "source.json", {"schema": "broken-unused-registration"})
    code, report = search(capsys, workspace)
    assert code == 0 and report["status"] == "completed"
    assert {r["provider"] for r in report["results"]} == {"local_pdf"}


@pytest.mark.parametrize("record_kind", ["report", "packet"])
@pytest.mark.parametrize("provider", ["local_pdf", "misumi_archive"])
def test_historical_record_uses_matching_relocated_source_without_rewriting(tmp_path, capsys, workspace, record_kind, provider):
    workspace["misumi"] = supplier_archive(tmp_path / "supplier", count=1)
    code, report = search(capsys, workspace, sources="all")
    assert code == 0
    result = next(r for r in report["results"] if r["provider"] == provider)
    selected = report["results"].index(result) + 1
    record = sk.make_packet(report, workspace["context"]) if record_kind == "packet" else report
    record_path = write_json(tmp_path / "historical.json", record)
    saved = record_path.read_bytes()
    old_pdf = Path(result["source_pdf"])
    if provider == "local_pdf":
        moved = tmp_path / "moved-book"
        moved.mkdir()
        shutil.copyfile(workspace["original"], moved / "original.pdf")
        manifest = write_json(moved / "manifest.json", workspace["manifest"])
        workspace["local"] = moved / "index"
        register_sources(manifest, workspace["local"])
        old_pdf.unlink()
    else:
        moved = tmp_path / "moved-supplier"
        shutil.move(workspace["misumi"], moved)
        workspace["misumi"] = moved
    code, view = run_cli(capsys, workspace, "--view-record", record_path, "--render", selected, "--render-dpi", "72")
    assert code == 0
    assert Path(view["source_pdf"]).is_relative_to(moved)
    assert view["identity"]["source_sha256"] == result["source_sha256"]
    assert view["identity"]["source_file_page"] == (2 if provider == "local_pdf" else 1)
    assert record_path.read_bytes() == saved
    historical = record["evidence_candidates"][selected - 1]["source"] if record_kind == "packet" else record["results"][selected - 1]
    historical["source_sha256"] = "0" * 64
    changed = write_json(tmp_path / "changed-record.json", record)
    code, refused = run_cli(capsys, workspace, "--view-record", changed, "--render", selected)
    assert code == 2 and refused["errors"] == [{"code": "source_view_record_identity_changed"}]


@pytest.mark.parametrize("record_kind", ["report", "packet"])
@pytest.mark.parametrize("provider", ["local_pdf", "misumi_archive"])
def test_view_record_renders_actual_whole_file_page(tmp_path, capsys, workspace, record_kind, provider):
    workspace["misumi"] = supplier_archive(tmp_path / "supplier", count=1)
    code, report = search(capsys, workspace, sources="all")
    assert code == 0
    # Each selected source claims physical book page 2, but only the textbook
    # original is a whole book; the catalog's archived source is a single page.
    result = next(r for r in report["results"] if r["provider"] == provider)
    selected = report["results"].index(result) + 1
    record = sk.make_packet(report, workspace["context"]) if record_kind == "packet" else report
    record_path = write_json(tmp_path / f"{record_kind}.json", record)
    code, view = run_cli(capsys, workspace, "--view-record", record_path, "--render", selected, "--render-dpi", "72")
    assert code == 0
    page = 2 if provider == "local_pdf" else 1
    assert view["identity"]["source_file_page"] == page
    assert view["catalog_pdf_page"] == 2 and view["identity"]["whole_page"] is True
    assert view["source_pdf_uri"].endswith(f"#page={page}")
    assert (view["width"], view["height"]) == (600, 800)
    expected = tmp_path / "independently-rendered"
    subprocess.run(["pdftoppm", "-f", str(page), "-l", str(page), "-singlefile", "-r", "72", "-png",
                    result["source_pdf"], str(expected)], capture_output=True, check=True)
    assert catalog.file_hash(view["image"]) == catalog.file_hash(expected.with_suffix(".png"))


def test_view_rebuilds_damaged_cache_but_refuses_changed_original(tmp_path, capsys, workspace):
    code, report = search(capsys, workspace)
    assert code == 0
    record = write_json(tmp_path / "report.json", report)
    code, view = run_cli(capsys, workspace, "--view-record", record, "--render-dpi", "72")
    assert code == 0
    image = Path(view["image"])
    expected = image.read_bytes()
    image.write_bytes(b"corrupted image cache")
    code, rebuilt = run_cli(capsys, workspace, "--view-record", record, "--render-dpi", "72")
    assert code == 0 and Path(rebuilt["image"]).read_bytes() == expected
    image.with_suffix(".json").write_text("broken cache metadata")
    code, rebuilt = run_cli(capsys, workspace, "--view-record", record, "--render-dpi", "72")
    assert code == 0 and rebuilt["image_sha256"] == catalog.digest(expected)
    workspace["original"].write_bytes(workspace["original"].read_bytes() + b"\n% changed original\n")
    code, failed = run_cli(capsys, workspace, "--view-record", record, "--render-dpi", "72")
    assert code == 2 and failed["status"] == "failed"
    assert failed["errors"] == [{"code": "source_view_source_changed"}]
    assert image.read_bytes() == expected


def test_view_page_reaches_scan_page_outside_truncated_query_without_model(tmp_path, capsys, workspace):
    workspace["local"], original, _ = local_book(tmp_path / "scan-book", pages=["Opening", "", "", ""])
    index = workspace["local"] / "catalog.sqlite3"
    before = catalog.file_hash(index)
    code, report = run_cli(capsys, workspace, "导航专用词", "--sources", "local-pdf", "--local",
                           "--top", "1", "--candidates", "8", "--full-text")
    assert code == 0 and len(report["results"]) == 1
    page_id = "local_pdf:mechanics:page:4"
    assert page_id not in {r["id"] for r in report["results"]}
    code, view = run_cli(capsys, workspace, "--view-page", page_id, "--render-dpi", "72")
    assert code == 0 and view["registered_page_id"] == page_id
    assert view["identity"]["source_file_page"] == view["catalog_pdf_page"] == 4
    assert view["source_locator"]["edition"] == "Edition 1"
    assert view["identity"]["source_sha256"] == catalog.file_hash(original)
    assert view["interpretation"] == view["numeric_verification"] == "not_run"
    assert view["usage"]["network_requests"] == 0 and not OfflineJudge.instances
    assert Path(view["image"]).is_file() and catalog.file_hash(index) == before


def test_view_page_resolves_supplier_archive_page_without_local_books(tmp_path, capsys, workspace):
    workspace["misumi"] = supplier_archive(tmp_path / "supplier", count=1)
    workspace["local"] = tmp_path / "no-books"
    identifier = catalog.CATALOG_CODES[0] + ":page:1"
    code, view = run_cli(capsys, workspace, "--view-page", identifier, "--render-dpi", "72")
    assert code == 0 and view["registered_page_id"] == identifier
    assert view["identity"]["source_file_page"] == 1 and view["catalog_pdf_page"] == 2
    assert view["source_pdf_uri"].endswith("#page=1")
    assert view["usage"]["network_requests"] == 0 and not OfflineJudge.instances


@pytest.mark.parametrize("identifier", ["local_pdf:mechanics:page:0", "local_pdf:mechanics:page:999",
                                         "local_pdf:unknown:page:1", "local_pdf:mechanics:page:x",
                                         "../../original.pdf", "unknown-supplier-page"])
def test_view_page_rejects_ids_not_in_current_registration(capsys, workspace, identifier):
    code, report = run_cli(capsys, workspace, "--view-page", identifier)
    assert code == 2 and report["errors"] == [{"code": "source_view_page_unavailable"}]
    assert not OfflineJudge.instances


@pytest.mark.parametrize("provider", ["local_pdf", "misumi_archive"])
def test_view_page_refuses_changed_source_bytes(tmp_path, capsys, workspace, provider):
    if provider == "local_pdf":
        source, identifier = workspace["original"], "local_pdf:mechanics:page:2"
        error = "local_pdf_source_changed_reindex_required"
    else:
        workspace["misumi"] = supplier_archive(tmp_path / "supplier", count=1)
        source = workspace["misumi"] / "archive/page-1.pdf"
        identifier = catalog.CATALOG_CODES[0] + ":page:1"
        error = "source_changed_reindex_required"
    source.write_bytes(source.read_bytes() + b"\n% changed source bytes\n")
    code, report = run_cli(capsys, workspace, "--view-page", identifier)
    assert code == 2 and report["errors"] == [{"code": error}]
    assert not OfflineJudge.instances


def test_view_page_keeps_registration_roots_inside_skill(tmp_path, capsys, workspace):
    workspace["local"] = tmp_path.parent / "outside-skill-index"
    code, report = run_cli(capsys, workspace, "--view-page", "local_pdf:mechanics:page:2")
    assert code == 2 and report["errors"] == [{"code": "knowledge_pdf_root_must_be_inside_skill"}]
    assert not OfflineJudge.instances


@pytest.mark.parametrize("other", [["--render", "1"], ["--open", "1"], ["--view-record", "record.json"], ["--status"]])
def test_view_page_conflicts_with_result_number_and_other_management_actions(capsys, other):
    with pytest.raises(SystemExit) as exc:
        cli.main(["--view-page", "local_pdf:mechanics:page:2", *other])
    assert exc.value.code == 2
    assert capsys.readouterr().err
    assert not OfflineJudge.instances


@pytest.mark.parametrize("identifier", ["", " ", "\t\n"])
def test_view_page_rejects_empty_identifier_instead_of_succeeding_with_help(capsys, identifier):
    with pytest.raises(SystemExit) as exc:
        cli.main(["--view-page", identifier])
    assert exc.value.code == 2
    assert "--view-page 需要非空" in capsys.readouterr().err
    assert not OfflineJudge.instances


@pytest.mark.parametrize("binding", ["matching", "changed", "unbound"])
def test_output_packet_can_be_verified_against_current_project_only(tmp_path, capsys, workspace, binding):
    if binding == "unbound":
        workspace["context"].pop("decision_id")
        write_json(workspace["context_path"], workspace["context"])
    output = tmp_path / "packet.json"
    code, created = search(capsys, workspace, extra=("--output", output))
    assert code == 0 and created["evidence_candidates"] == 1
    current = copy.deepcopy(workspace["context"])
    if binding == "changed":
        current["object"]["revision"] = "B"
    current_path = write_json(tmp_path / "current.json", current)
    code, verified = run_cli(capsys, workspace, "--verify-packet", output, "--context", current_path)
    assert code == (0 if binding == "matching" else 2)
    assert verified["status"] == {"matching": "ready_for_agent_review", "changed": "changed", "unbound": "unbound"}[binding]
    assert verified["evidence_candidate_count"] == 1
    assert verified["adoption"] == "pending_agent_review" and verified["semantic_review"] == "not_run"
    if binding == "changed":
        assert "object" in verified["changed_fields"]
    if binding == "unbound":
        assert "decision_id" in verified["missing_identity"]


def test_relocating_and_registering_same_book_preserves_citation_identity(tmp_path, capsys, workspace):
    code, first = search(capsys, workspace)
    assert code == 0
    destination = tmp_path / "new-location"
    destination.mkdir()
    shutil.copyfile(workspace["original"], destination / "original.pdf")
    manifest_path = write_json(destination / "manifest.json", workspace["manifest"])
    workspace["local"] = destination / "registered"
    code, registered = run_cli(capsys, workspace, "--register-pdfs", manifest_path)
    assert code == 0 and registered["status"] == "registered"
    code, second = search(capsys, workspace)
    assert code == 0
    old, moved = first["results"][0], second["results"][0]
    assert old["source_pdf"] != moved["source_pdf"]
    for key in ("id", "source_sha256", "text_sha256", "source_locator"):
        assert old[key] == moved[key]
    assert [u["id"] for u in old["knowledge_units"]] == [u["id"] for u in moved["knowledge_units"]]
    packet = sk.make_packet(second, workspace["context"])
    assert sk.verify_packet_binding(packet, workspace["context"])["status"] == "ready_for_agent_review"


def test_relocated_local_pdf_cli_completes_handoff_with_stdlib_and_poppler_only(tmp_path):
    clone = tmp_path / "relocated-skill"
    package = clone / "ontology_engineering"
    package.mkdir(parents=True)
    for name in ("__init__.py", "jev_transport.py", "supplier_knowledge.py", "judgment_contracts.py",
                 "method_evidence.py", "local_pdf_sources.py", "source_citations.py", "semantic_bundle_transport.py", "local_paths.py", "knowledge_context.py"):
        shutil.copy2(SKILL_ROOT / "ontology_engineering" / name, package / name)
    shutil.copytree(SKILL_ROOT / "ontology_engineering/misumi", package / "misumi",
                    ignore=shutil.ignore_patterns("__pycache__"))
    (clone / "scripts").mkdir()
    for name in ("jev_knowledge.py", "semantic_bundle_transport.py"):
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

    # Deliver only the original PDF and a relative manifest. Remove the old
    # location before any subprocess so it cannot rescue a hidden path binding.
    acquired = tmp_path / "old-source-location"
    acquired.mkdir()
    source = pdf_fixture(acquired / "original.pdf", ["First page about materials.",
        "Shaft mechanism under stated load conditions. Preserve the original assumptions and inspect the interfaces before applying this method."])
    received = clone / "sources/books"
    received.mkdir(parents=True)
    original = received / "book.pdf"
    shutil.copy2(source, original)
    source_sha = catalog.file_hash(original)
    shutil.rmtree(acquired)
    manifest = write_json(received / "manifest.json", {"schema": MANIFEST_SCHEMA, "sources": [{
        "id": "portable-mechanics", "title": "Portable mechanics fixture", "edition": "Edition 1",
        "path": "book.pdf", "sha256": source_sha,
        "chapters": [{"id": "mechanism", "title": "Mechanism", "start_page": 2, "end_page": 2,
                      "aliases": ["shaft"]}]}]})
    context = write_json(clone / "project-context.json", {
        "schema": sk.CONTEXT_SCHEMA, "task_id": "portable-local-pdf",
        "project": {"id": "portable-project", "revision": "P1"}, "decision_id": "select-shaft",
        "object": {"id": "shaft", "name": "Shaft", "revision": "A"},
        "goal": "Compare mechanisms with their applicability conditions."})
    data, state = clone / "sources/.indexes/local-pdf", clone / "var/state/misumi"
    empty_home = tmp_path / "empty-home"
    empty_home.mkdir()
    tool_dirs = sorted({str(Path(shutil.which(name)).parent) for name in ("pdfinfo", "pdftotext", "pdftoppm")})
    env = {"PATH": os.pathsep.join(tool_dirs), "HOME": str(empty_home), "PYTHONPATH": "",
           "PYTHONDONTWRITEBYTECODE": "1", "LC_ALL": "C"}
    python = Path(getattr(sys, "_base_executable", sys.executable)).resolve()
    assert not python.is_relative_to(SKILL_ROOT)

    def isolated(*arguments):
        # A direct call to the scanned CLI keeps the subprocess target visible
        # to the backend policy. Use the same interpreter's base executable so
        # startup itself does not depend on the author's virtual-environment path.
        result = subprocess.run(
            [sys.executable, "-S", str(clone / "scripts/jev_knowledge.py"),
             "--sources", "local-pdf", "--pdf-root", str(data),
             "--data-root", str(clone / "sources/.indexes/misumi"), "--json", *map(str, arguments)],
            executable=str(python), cwd=tmp_path, env=env, capture_output=True, text=True, timeout=60)
        assert result.returncode == 0, result.stderr + result.stdout
        assert not result.stderr, result.stderr
        return json.loads(result.stdout)

    registered = isolated("--register-pdfs", manifest)
    assert registered["status"] == "registered" and registered["network_requests"] == 0
    assert registered["index"]["counts"]["pages"] == 2
    packet_path = clone / "evidence-packet.json"
    created = isolated("shaft", "--local", "--context", context, "--output", packet_path)
    assert created["status"] == "completed" and created["evidence_candidates"] == 1
    packet = json.loads(packet_path.read_text())
    report = json.loads(Path(packet["search_record"]).read_text())
    assert packet["usage"]["network_requests"] == report["usage"]["network_requests"] == 0
    assert report["review_plan"]["status"] == "not_run"
    assert all(item["status"] == "not_bundled" for item in report["method_bundle"]["source_availability"])
    assert Path(report["record_file"]).is_relative_to(state)
    result = report["results"][0]
    assert result["source_pdf"] == str(original)
    assert result["source_sha256"] == source_sha and result["source_locator"]["physical_page"] == 2
    assert "Shaft mechanism" in result["native_text"] and result["knowledge_units"]
    view = isolated("--view-record", packet_path, "--render", "1", "--render-dpi", "72")
    assert view["identity"]["source_file_page"] == 2 and view["identity"]["whole_page"] is True
    assert (view["width"], view["height"]) == (600, 800)
    assert Path(view["image"]).is_relative_to(clone / "var/cache/misumi/pages")
    assert catalog.file_hash(view["image"]) == view["image_sha256"]
    verified = isolated("--verify-packet", packet_path, "--context", context)
    assert verified["status"] == "ready_for_agent_review" and verified["evidence_candidate_count"] == 1
    assert verified["semantic_review"] == "not_run" and verified["adoption"] == "pending_agent_review"
    assert catalog.file_hash(original) == source_sha
    assert not list(empty_home.iterdir()) and not (clone / "sources/.indexes/misumi").exists()
