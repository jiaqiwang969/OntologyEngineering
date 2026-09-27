"""Source-bound, rebuildable MISUMI document index. No engineering rule engine."""
from __future__ import annotations

import collections
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import tempfile
import unicodedata

VERSION = "0.2.0"
INDEX_SCHEMA = "misumi.catalog-index/v1"
CATALOG_CODES = ("fabiaozhunpin202210", "fajingjixing202306")
LABELS = {CATALOG_CODES[0]: "FA标准品（2022）", CATALOG_CODES[1]: "FA经济型（2023）"}
CJK = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff]+")
ASCII = re.compile(r"[a-z][a-z0-9]*(?:[-_/][a-z0-9]+)*|[0-9]+(?:\.[0-9]+)?[a-z]+")


def now():
    return dt.datetime.now().astimezone().isoformat(timespec="seconds")


def dumps(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def digest(value):
    data = value if isinstance(value, bytes) else dumps(value).encode()
    return hashlib.sha256(data).hexdigest()


def file_hash(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(2 ** 20), b""):
            h.update(block)
    return h.hexdigest()


def write_json(path, value):
    """Atomic private record; never store authentication data in these files."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".write-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as f:
            f.write(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def normalize(text):
    return unicodedata.normalize("NFKC", str(text)).lower().translate(
        str.maketrans({"−": "-", "–": "-", "—": "-", "‐": "-", "‑": "-"}))


def tokens(text):
    text = normalize(text)
    result = []
    for word in ASCII.findall(text):
        result.append(word)
        # A complete configured code can find the series prefix in the catalog.
        prefix = re.match(r"[a-z]{2,}", word)
        if prefix and prefix[0] != word:
            result.append(prefix[0])
    for run in CJK.findall(text):
        if len(run) == 1:
            result.append(run)
        else:
            for size in (2, 3):
                result.extend(run[i:i + size] for i in range(len(run) - size + 1))
    return result


def match_expression(query):
    # Quoted terms and bound SQL parameters: query syntax is always plain text.
    terms = list(dict.fromkeys(tokens(query)))[:160]
    return " OR ".join('"' + t.replace('"', '""') + '"' for t in terms)


def connect(path):
    db = sqlite3.connect(Path(path).resolve().as_uri() + "?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    schema = db.execute("SELECT value FROM meta WHERE key='schema'").fetchone()
    if not schema or json.loads(schema[0]) != INDEX_SCHEMA:
        db.close()
        raise ValueError("index_schema_mismatch")
    return db


def metadata(db):
    return {r[0]: json.loads(r[1]) for r in db.execute("SELECT key,value FROM meta")}


def build_index(archive, destination, progress=print):
    import pymupdf

    archive, destination = Path(archive).resolve(), Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".index-", suffix=".sqlite3", dir=destination.parent)
    os.close(fd)
    db = sqlite3.connect(temporary)
    db.executescript("""
        PRAGMA journal_mode=DELETE;
        CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE catalogs(code TEXT PRIMARY KEY,label TEXT,title TEXT,page_count INTEGER,
          pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT);
        CREATE TABLE chapters(id TEXT PRIMARY KEY,catalog TEXT,title TEXT,start_page INTEGER,
          end_page INTEGER,source_json TEXT);
        CREATE TABLE pages(id TEXT PRIMARY KEY,catalog TEXT,page_id INTEGER,pdf_page INTEGER,
          printed_page TEXT,chapter_id TEXT,source_relative_path TEXT,source_url TEXT,
          source_sha256 TEXT,source_bytes INTEGER,body TEXT,text_sha256 TEXT,raw_text_sha256 TEXT,
          raw_text_chars INTEGER,text_chars INTEGER,native_text_sparse INTEGER,titles_json TEXT,
          series_json TEXT,series_count INTEGER,source_json TEXT);
        CREATE TABLE hotspots(page_id TEXT,ordinal INTEGER,kind TEXT,keyword TEXT,
          keyword_normalized TEXT,target TEXT,source_json TEXT,PRIMARY KEY(page_id,ordinal));
        CREATE INDEX hotspot_model ON hotspots(keyword_normalized,kind);
        CREATE INDEX page_chapter ON pages(chapter_id,pdf_page);
        CREATE INDEX page_position ON pages(catalog,pdf_page);
        CREATE VIRTUAL TABLE search USING fts5(models,titles,body,chapter);
    """)
    counts = collections.Counter()
    identity = []
    try:
        for code in CATALOG_CODES:
            root = archive / "tmp/pdfs" / code
            meta_path = root / "metadata.json"
            manifest_path = archive / "下载记录" / (code + "-manifest.json")
            assembly = json.loads((archive / "下载记录" / (code + "-assembly.json")).read_text())
            ebook = json.loads(meta_path.read_text())["ebook"]
            pages = ebook["pages"]["info"]
            manifests = json.loads(manifest_path.read_text())
            if ebook["catalogCode"] != code or len(manifests) != len(pages):
                raise ValueError("source_manifest_identity_mismatch")
            by_number = {m["page_number"]: m for m in manifests}
            by_id = {str(p["id"]): p for p in pages}
            if len(by_id) != len(pages) or len(by_number) != len(pages):
                raise ValueError("duplicate_source_page")
            # Verify both delivered PDF and every one-page evidence source once at build.
            pdf_path = archive / assembly["file"]
            if file_hash(pdf_path) != assembly["sha256"]:
                raise ValueError("assembled_pdf_hash_mismatch:" + code)
            cat_identity = [code, file_hash(meta_path), file_hash(manifest_path), assembly["sha256"]]
            identity.append(cat_identity)
            db.execute("INSERT INTO catalogs VALUES (?,?,?,?,?,?,?,?)", (
                code, LABELS[code], ebook["catalogTitle"], len(pages), assembly["file"],
                assembly["sha256"], cat_identity[1], cat_identity[2]))
            chapters = sorted(ebook["class"], key=lambda c: by_id[str(c["pageId"])]["pageNo"])
            chapter_at = {}
            for i, c in enumerate(chapters):
                start = by_id[str(c["pageId"])]["pageNo"]
                end = by_id[str(chapters[i + 1]["pageId"])]["pageNo"] - 1 if i + 1 < len(chapters) else len(pages)
                cid = code + ":chapter:" + str(c["id"])
                db.execute("INSERT INTO chapters VALUES (?,?,?,?,?,?)", (cid, code, c["title"], start, end, dumps(c)))
                for number in range(start, end + 1):
                    chapter_at[number] = (cid, c["title"])
            # Incoming observed table-of-contents links are valuable product headings.
            incoming = collections.defaultdict(list)
            for p in pages:
                for h in p.get("hotArea", []):
                    target = str(h.get("target", ""))
                    if target in by_id and h.get("keyword") and "目次" in h.get("type", ""):
                        incoming[target].append({"text": h["keyword"], "source_page_id": p["id"], "hotspot_id": h["id"]})
            for p in pages:
                n = p["pageNo"]
                m = by_number[n]
                if (m["page_id"], m["filename"], str(m["printed_page"])) != (p["id"], p["pdfUrl"], str(p["pageNoReal"])):
                    raise ValueError("page_mapping_mismatch")
                path = root / "pages" / p["pdfUrl"]
                raw_pdf = path.read_bytes()
                if len(raw_pdf) != m["bytes"] or digest(raw_pdf) != m["sha256"]:
                    raise ValueError("source_page_hash_mismatch:" + str(path))
                with pymupdf.open(stream=raw_pdf, filetype="pdf") as pdf:
                    if len(pdf) != 1 or pdf.is_repaired or pdf.is_encrypted:
                        raise ValueError("source_pdf_integrity_failure")
                    raw_text = pdf[0].get_text(sort=True)
                body = re.sub(r"[ \t]+", " ", raw_text)
                pid = code + ":page:" + str(p["id"])
                chapter_id, chapter_title = chapter_at.get(n, (None, "封面及前附"))
                headings = incoming.get(str(p["id"]), [])
                series = []
                for ordinal, h in enumerate(p.get("hotArea", [])):
                    db.execute("INSERT INTO hotspots VALUES (?,?,?,?,?,?,?)", (
                        pid, ordinal, h.get("type", ""), h.get("keyword", ""), normalize(h.get("keyword", "")), str(h.get("target", "")), dumps(h)))
                    counts["hotspots"] += 1
                    if h.get("type") in ("形式Type", "形式Tpye"):
                        series.append({"keyword": h.get("keyword", ""), "url": h.get("target", ""), "hotspot_id": h["id"]})
                sparse = int(len(re.sub(r"\s", "", body)) < 80)
                values = (pid, code, p["id"], n, str(p["pageNoReal"]), chapter_id,
                          str(path.relative_to(archive)), m["url"], m["sha256"], m["bytes"],
                          body, digest(body.encode()), digest(raw_text.encode()), len(raw_text), len(body), sparse,
                          dumps(headings), dumps(series), len(series), dumps({k: v for k, v in p.items() if k != "hotArea"}))
                rowid = db.execute("INSERT INTO pages VALUES (" + ",".join("?" * len(values)) + ")", values).lastrowid
                db.execute("INSERT INTO search(rowid,models,titles,body,chapter) VALUES (?,?,?,?,?)", (
                    rowid, " ".join(tokens(" ".join(s["keyword"] for s in series))),
                    " ".join(tokens(" ".join(h["text"] for h in headings))), " ".join(tokens(body)),
                    " ".join(tokens(chapter_title))))
                counts.update(pages=1, series_hotspots=len(series), native_text_sparse=sparse, native_text_chars=len(body))
                if n % 250 == 0:
                    progress(f"{LABELS[code]}：已索引 {n}/{len(pages)} 页")
            counts.update(catalogs=1, chapters=len(chapters))
            db.commit()
        meta = {"schema": INDEX_SCHEMA, "version": VERSION, "built_at": now(),
                "source_identity": identity, "counts": dict(counts),
                "extraction": "PyMuPDF native text sort=True; horizontal whitespace collapsed; no OCR",
                "pymupdf_version": pymupdf.VersionBind, "sqlite_version": sqlite3.sqlite_version,
                "builder_sha256": file_hash(Path(__file__)),
                "scope": "document_retrieval; series hotspots are not configured products; no parameter verification"}
        meta["fingerprint"] = digest(meta)
        db.executemany("INSERT INTO meta VALUES (?,?)", [(k, dumps(v)) for k, v in meta.items()])
        db.execute("INSERT INTO search(search) VALUES ('optimize')")
        db.commit()
        if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise ValueError("index_integrity_failure")
        db.close()
        os.replace(temporary, destination)
        return meta
    finally:
        db.close()
        if os.path.exists(temporary):
            os.unlink(temporary)


def chapters(db, catalog=None):
    return [dict(r) for r in db.execute("SELECT id,catalog,title,start_page,end_page FROM chapters "
                                     "WHERE (? IS NULL OR catalog=?) ORDER BY catalog,start_page", (catalog, catalog))]


def retrieve(db, query, limit=24, chapter_id=None, catalog=None):
    expression = match_expression(query)
    rows = []
    if expression:
        rows = db.execute("""SELECT p.*, bm25(search,8,5,1,0.2) AS lexical_score FROM search
            JOIN pages p ON p.rowid=search.rowid WHERE search MATCH ?
            AND (? IS NULL OR p.catalog=?) ORDER BY lexical_score,p.catalog,p.pdf_page LIMIT 300""",
                          (expression, catalog, catalog)).fetchall()
    ordered = [dict(r) for r in rows]
    # Reserve room for semantic routing, while keeping global lexical matches from both books.
    lexical_slots = limit if not chapter_id else max(1, limit * 2 // 3)
    selected = ordered[:lexical_slots]
    seen = {r["id"] for r in selected}
    if chapter_id:
        routed = [r for r in ordered if r["chapter_id"] == chapter_id]
        if len(routed) < limit:
            routed.extend(dict(r) | {"lexical_score": None} for r in db.execute(
                "SELECT * FROM pages WHERE chapter_id=? AND (? IS NULL OR catalog=?) ORDER BY series_count>0 DESC,pdf_page",
                (chapter_id, catalog, catalog)))
        for r in routed + ordered[lexical_slots:]:
            if r["id"] not in seen:
                selected.append(r)
                seen.add(r["id"])
            if len(selected) >= limit:
                break
    return selected[:limit]


def excerpt(body, query, length=700):
    if len(body) <= length:
        return {"text": body, "start": 0, "end": len(body)}
    # Offsets refer to the stored normalized native text, never to the PDF geometry.
    terms = set(tokens(query))
    positions = list(range(0, len(body), length // 2))
    start = max(positions, key=lambda i: len(terms.intersection(tokens(body[i:i + length]))))
    return {"text": body[start:start + length], "start": start, "end": min(len(body), start + length)}


def archive_path(archive, relative):
    root = Path(archive).resolve()
    rel = Path(relative)
    path = (root / rel).resolve()
    if rel.is_absolute() or ".." in rel.parts or not path.is_relative_to(root):
        raise ValueError("source_relative_path_invalid")
    return path


def source_result(db, page, archive, query):
    from .source_pack import assembly_details, restore_page

    catalog = dict(db.execute("SELECT * FROM catalogs WHERE code=?", (page["catalog"],)).fetchone())
    chapter = db.execute("SELECT title FROM chapters WHERE id=?", (page["chapter_id"],)).fetchone()
    source = archive_path(archive, page["source_relative_path"])
    # A stale or missing source must not become a valid citation, even on a cache hit.
    if not source.is_file():
        restored = restore_page(archive, page["source_relative_path"], page["source_sha256"])
        if restored is None:
            raise ValueError("source_missing:" + page["id"])
        source = restored
    if file_hash(source) != page["source_sha256"]:
        raise ValueError("source_changed_reindex_required:" + page["id"])
    assembly = assembly_details(archive, catalog)
    headings = json.loads(page["titles_json"])
    return {"id": page["id"], "catalog": page["catalog"], "catalog_title": catalog["label"],
            "chapter": chapter[0] if chapter else "封面及前附",
            "headings": list(dict.fromkeys(h["text"] for h in headings)),
            "pdf_page": page["pdf_page"], "printed_page": page["printed_page"],
            "page_id": page["page_id"], "source_pdf": str(source), "source_url": page["source_url"],
            "source_sha256": page["source_sha256"], "source_relative_path": page["source_relative_path"],
            **assembly,
            "assembled_pdf_sha256_at_index_build": catalog["pdf_sha256"],
            "pdf_uri": (assembly["assembled_pdf_uri"] + "#page=" + str(page["pdf_page"]))
                       if assembly["assembled_pdf_uri"] else source.as_uri() + "#page=1",
            "series_references": json.loads(page["series_json"]),
            "excerpt": excerpt(page["body"], query), "text_sha256": page["text_sha256"],
            "native_text_sparse": bool(page["native_text_sparse"]),
            "parameter_verification": {"status": "not_run", "product_match": "unknown"},
            "lexical_score": page.get("lexical_score")}
