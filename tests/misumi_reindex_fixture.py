"""Fixed optional-parser smoke fixture; run with a PyMuPDF-enabled Python."""
from __future__ import annotations

import contextlib
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from ontology_engineering.misumi import catalog, cli
from runtime.misumi import setup


def fresh_archive(data):
    import pymupdf
    archive = data / "archive"
    records = archive / "下载记录"
    records.mkdir(parents=True)
    for i, code in enumerate(catalog.CATALOG_CODES):
        root = archive / "tmp/pdfs" / code
        (root / "pages").mkdir(parents=True)
        source = root / "pages/0001.pdf"
        pdf = pymupdf.open()
        page = pdf.new_page()
        page.insert_text((72, 72), "Coupling assembly. Check the conditions before applying this fixture source.")
        pdf.save(source)
        pdf.close()
        raw = source.read_bytes()
        sha = hashlib.sha256(raw).hexdigest()
        target = archive / "output/pdf" / (code + ".pdf")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
        info = {"id": i + 1, "pageNo": 1, "pageNoReal": 1, "pdfUrl": "0001.pdf", "hotArea": []}
        ebook = {"catalogCode": code, "catalogTitle": "Synthetic fixture", "pages": {"info": [info]},
                 "class": [{"id": 1, "pageId": i + 1, "title": "Coupling"}]}
        (root / "metadata.json").write_text(json.dumps({"ebook": ebook}))
        (records / (code + "-manifest.json")).write_text(json.dumps([{"page_number": 1, "page_id": i + 1,
            "filename": "0001.pdf", "printed_page": 1, "bytes": len(raw), "sha256": sha, "url": "https://example.invalid/fixture.pdf"}]))
        (records / (code + "-assembly.json")).write_text(json.dumps({"book": code, "pages": 1,
            "file": target.relative_to(archive).as_posix(), "sha256": sha}))


def exercise_reindex(work):
    work = work.resolve()
    data = work / "data"
    fresh_archive(data)
    assert not (data / "catalog.sqlite3").exists()
    output = io.StringIO()
    with contextlib.redirect_stdout(output):
        code = cli.main(["--data-root", str(data), "--state-root", str(work / "state"), "--reindex", "--json"])
    assert code == 0, output.getvalue()
    built = json.loads(output.getvalue())
    assert built["index"]["counts"]["pages"] == 2
    with catalog.connect(data / "catalog.sqlite3") as db:
        pages = db.execute("SELECT body,source_relative_path FROM pages").fetchall()
        assert len(pages) == 2
        assert all("Coupling assembly" in row["body"] for row in pages)
        assert all(not Path(row["source_relative_path"]).is_absolute() for row in pages)
    imported = work / "imported"
    setup.import_data(data / "archive", data / "catalog.sqlite3", imported)
    result = setup.verify(imported)
    assert result["status"] == "verified"
    return {**result, "rebuilt_pages": 2, "source": "newly_generated_real_pdf_pages_not_existing_database"}


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="oe-misumi-reindex-test-") as directory:
        print(json.dumps(exercise_reindex(Path(directory)), ensure_ascii=False))
