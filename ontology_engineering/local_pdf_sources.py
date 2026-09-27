"""Private, source-bound local PDF pages for the existing engineering knowledge chain.

This is a retrieval adapter, not a semantic registry. Poppler runs only while
registering sources; querying uses SQLite and never sends data over a network.
"""
from __future__ import annotations

import json
from contextlib import closing
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import tempfile

from .misumi.catalog import (chapters as catalog_chapters, digest, dumps, excerpt,
                             file_hash, metadata, now, retrieve as catalog_retrieve,
                             tokens, write_json)

MANIFEST_SCHEMA = "ontology-engineering.local-pdf-manifest/v1"
REGISTRATION_SCHEMA = "ontology-engineering.local-pdf-registration/v1"
INDEX_SCHEMA = "ontology-engineering.local-pdf-index/v1"
LOCATOR_SCHEMA = "ontology-engineering.source-page/v1"
ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,119}\Z")
SHA = re.compile(r"[0-9a-f]{64}\Z")
TABLES = """
CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE chapters(id TEXT PRIMARY KEY,catalog TEXT,title TEXT,start_page INTEGER,
 end_page INTEGER,source_json TEXT);
CREATE TABLE pages(id TEXT PRIMARY KEY,catalog TEXT,page_id INTEGER,pdf_page INTEGER,
 printed_page TEXT,chapter_id TEXT,source_relative_path TEXT,source_url TEXT,
 source_sha256 TEXT,source_bytes INTEGER,body TEXT,text_sha256 TEXT,raw_text_sha256 TEXT,
 raw_text_chars INTEGER,text_chars INTEGER,native_text_sparse INTEGER,titles_json TEXT,
 series_json TEXT,series_count INTEGER,source_json TEXT);
CREATE INDEX page_chapter ON pages(chapter_id,pdf_page);
CREATE INDEX page_position ON pages(catalog,pdf_page);
CREATE VIRTUAL TABLE search USING fts5(models,titles,body,chapter);
"""


def _text(value, name):
    if not isinstance(value, str) or not value.strip() or len(value) > 2000:
        raise ValueError("local_pdf_invalid_" + name)
    return value


def _manifest(path):
    path = Path(path).expanduser().resolve()
    document = json.loads(path.read_text(encoding="utf-8"))
    if (not isinstance(document, dict) or document.get("schema") != MANIFEST_SCHEMA
            or set(document) != {"schema", "sources"} or not isinstance(document["sources"], list)
            or not document["sources"]):
        raise ValueError("local_pdf_manifest_invalid")
    sources, ids = [], set()
    for source in document["sources"]:
        if not isinstance(source, dict) or set(source) != {"id", "title", "edition", "path", "sha256", "chapters"}:
            raise ValueError("local_pdf_source_invalid")
        sid = source["id"]
        if not isinstance(sid, str) or not ID.fullmatch(sid) or sid in ids:
            raise ValueError("local_pdf_source_id_invalid_or_duplicate")
        ids.add(sid)
        original = Path(_text(source["path"], "path")).expanduser()
        original = (original if original.is_absolute() else path.parent / original).resolve()
        if not original.is_file() or not SHA.fullmatch(str(source["sha256"])):
            raise ValueError("local_pdf_source_missing_or_hash_invalid")
        if not isinstance(source["chapters"], list):
            raise ValueError("local_pdf_chapters_invalid")
        chapters, chapter_ids = [], set()
        for chapter in source["chapters"]:
            if not isinstance(chapter, dict) or set(chapter) != {"id", "title", "start_page", "end_page", "aliases"}:
                raise ValueError("local_pdf_chapter_invalid")
            cid = chapter["id"]
            if not isinstance(cid, str) or not ID.fullmatch(cid) or cid in chapter_ids:
                raise ValueError("local_pdf_chapter_id_invalid_or_duplicate")
            chapter_ids.add(cid)
            start, end = chapter["start_page"], chapter["end_page"]
            aliases = chapter["aliases"]
            if (type(start) is not int or type(end) is not int or not 1 <= start <= end
                    or not isinstance(aliases, list) or len(aliases) > 40):
                raise ValueError("local_pdf_chapter_range_or_aliases_invalid")
            chapters.append({"id": cid, "title": _text(chapter["title"], "chapter_title"),
                             "start_page": start, "end_page": end,
                             "aliases": [_text(a, "alias") for a in aliases]})
        sources.append({"id": sid, "title": _text(source["title"], "title"),
                        "edition": _text(source["edition"], "edition"), "path": str(original),
                        "sha256": source["sha256"], "chapters": chapters})
    return sources


def _extract(source):
    original = Path(source["path"])
    if file_hash(original) != source["sha256"]:
        raise ValueError("local_pdf_source_hash_mismatch")
    try:
        info = subprocess.run(["pdfinfo", "-enc", "UTF-8", str(original)], capture_output=True,
                              timeout=180, env=dict(os.environ, LC_ALL="C"))
        if info.returncode:
            raise ValueError("local_pdf_pdfinfo_failed")
        match = re.search(rb"^Pages:\s+(\d+)\s*$", info.stdout, re.M)
        if not match or re.search(rb"^Encrypted:\s+yes\b", info.stdout, re.M):
            raise ValueError("local_pdf_page_count_or_encryption_invalid")
        count = int(match[1])
        if not count or any(c["end_page"] > count for c in source["chapters"]):
            raise ValueError("local_pdf_chapter_outside_document")
        extracted = subprocess.run(["pdftotext", "-layout", "-enc", "UTF-8", str(original), "-"],
                                   capture_output=True, timeout=180)
        if extracted.returncode:
            raise ValueError("local_pdf_text_extraction_failed")
    except FileNotFoundError:
        raise ValueError("local_pdf_poppler_required_for_registration") from None
    except subprocess.TimeoutExpired:
        raise ValueError("local_pdf_poppler_timeout") from None
    text = extracted.stdout.decode("utf-8")
    pages = text.split("\f")
    if pages and not pages[-1]:
        pages.pop()
    if len(pages) != count:
        raise ValueError("local_pdf_extracted_page_count_mismatch")
    if file_hash(original) != source["sha256"]:
        raise ValueError("local_pdf_source_changed_during_registration")
    return pages


def register_sources(manifest_path, data_root, replace=False, relative_paths=False):
    """Build a private index, optionally binding originals relative to its final root.

    Relative bindings may traverse parents so books can live beside the index.
    They retain the same original-byte hash checks as absolute registrations.
    """
    root = Path(data_root).expanduser().absolute()
    if root.is_symlink():
        raise ValueError("local_pdf_data_root_symlink")
    if root.exists():
        if not replace:
            raise FileExistsError("local_pdf_data_root_exists")
        if (not root.is_dir() or {p.name for p in root.iterdir()} != {"registration.json", "catalog.sqlite3"}
                or json.loads((root / "registration.json").read_text()).get("schema") != REGISTRATION_SCHEMA):
            raise ValueError("local_pdf_replace_requires_owned_registration")
    sources = _manifest(manifest_path)
    if any(Path(s["path"]).is_relative_to(root.resolve()) for s in sources):
        raise ValueError("local_pdf_original_must_be_outside_index_directory")
    root.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".local-pdf-index-", dir=root.parent))
    backup = None
    try:
        index = stage / "catalog.sqlite3"
        with closing(sqlite3.connect(index)) as db, db:
            db.executescript(TABLES)
            counts = {"catalogs": len(sources), "pages": 0, "chapters": 0,
                      "native_text_sparse": 0, "native_text_missing": 0, "native_text_chars": 0}
            for source in sources:
                bodies = _extract(source)
                source.update(page_count=len(bodies), bytes=Path(source["path"]).stat().st_size)
                code = "local_pdf:" + source["id"]
                for chapter in source["chapters"]:
                    cid = code + ":chapter:" + chapter["id"]
                    db.execute("INSERT INTO chapters VALUES (?,?,?,?,?,?)", (cid, code, chapter["title"],
                        chapter["start_page"], chapter["end_page"], dumps(chapter)))
                    counts["chapters"] += 1
                for n, raw in enumerate(bodies, 1):
                    body = re.sub(r"[ \t]+", " ", raw)
                    relevant = [c for c in source["chapters"] if c["start_page"] <= n <= c["end_page"]]
                    chapter = min(relevant, key=lambda c: c["end_page"] - c["start_page"], default=None)
                    cid = code + ":chapter:" + chapter["id"] if chapter else None
                    headings = [{"text": c["title"], "origin": "declared_chapter_navigation"} for c in relevant]
                    terms = " ".join(c["title"] + " " + " ".join(c["aliases"]) for c in relevant)
                    sparse = int(len(re.sub(r"\s", "", body)) < 80)
                    values = (code + ":page:" + str(n), code, n, n, None, cid, "", "", source["sha256"],
                        source["bytes"], body, digest(body.encode()), digest(raw.encode()), len(raw), len(body),
                        sparse, dumps(headings), "[]", 0, dumps({"document_id": source["id"], "text_origin": "poppler_native_text"}))
                    rowid = db.execute("INSERT INTO pages VALUES (" + ",".join("?" * len(values)) + ")", values).lastrowid
                    db.execute("INSERT INTO search(rowid,models,titles,body,chapter) VALUES (?,?,?,?,?)",
                        (rowid, "", " ".join(tokens(terms)), " ".join(tokens(body)), " ".join(tokens(chapter["title"] if chapter else ""))))
                    counts["pages"] += 1
                    counts["native_text_sparse"] += sparse
                    counts["native_text_missing"] += int(not body.strip())
                    counts["native_text_chars"] += len(body)
            if relative_paths:
                for source in sources:
                    source["path"] = os.path.relpath(source["path"], root.resolve())
            identity = {"schema": REGISTRATION_SCHEMA, "sources": sources}
            meta = {"schema": INDEX_SCHEMA, "built_at": now(), "counts": counts,
                    "registration_sha256": digest(identity), "builder_sha256": file_hash(Path(__file__)),
                    "extraction": "pdftotext -layout -enc UTF-8; horizontal whitespace collapsed; no OCR",
                    "scope": "native text retrieval plus declared chapter navigation; no claim extraction or semantic adoption"}
            meta["fingerprint"] = digest(meta)
            db.executemany("INSERT INTO meta VALUES (?,?)", [(k, dumps(v)) for k, v in meta.items()])
            db.execute("INSERT INTO search(search) VALUES ('optimize')")
            if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                raise ValueError("local_pdf_index_integrity_failure")
        index.chmod(0o600)
        write_json(stage / "registration.json", {**identity, "index_sha256": file_hash(index)})
        (stage / "registration.json").chmod(0o600)
        if root.exists():
            backup = Path(tempfile.mkdtemp(prefix=".local-pdf-before-", dir=root.parent))
            backup.rmdir()
            root.rename(backup)
        try:
            stage.rename(root)
        except OSError:
            if backup is not None:
                backup.rename(root)
                backup = None
            raise
        if backup is not None:
            shutil.rmtree(backup)
        return {"status": "registered", "provider": "local_pdf", "data_root": str(root), "index": meta,
                "originals_modified": False, "network_requests": 0}
    finally:
        if stage.exists():
            shutil.rmtree(stage)


class LocalPDFSources:
    def __init__(self, data_root):
        self.root = Path(data_root).expanduser().resolve()
        self.meta, self.db, self.sources, self.error = {}, None, {}, None
        try:
            registration = json.loads((self.root / "registration.json").read_text())
            if (set(registration) != {"schema", "sources", "index_sha256"}
                    or registration["schema"] != REGISTRATION_SCHEMA):
                raise ValueError("local_pdf_registration_invalid")
            index = self.root / "catalog.sqlite3"
            if file_hash(index) != registration["index_sha256"]:
                raise ValueError("local_pdf_index_hash_mismatch")
            self.db = sqlite3.connect(index.as_uri() + "?mode=ro", uri=True)
            self.db.row_factory = sqlite3.Row
            self.meta = metadata(self.db)
            base = {key: registration[key] for key in ("schema", "sources")}
            if (self.meta.get("schema") != INDEX_SCHEMA or self.meta.get("registration_sha256") != digest(base)
                    or self.meta.get("fingerprint") != digest({k: v for k, v in self.meta.items() if k != "fingerprint"})):
                raise ValueError("local_pdf_registration_identity_mismatch")
            self.sources = {"local_pdf:" + s["id"]: s for s in registration["sources"]}
        except (OSError, ValueError, KeyError, sqlite3.Error) as exc:
            self.close()
            self.error = str(exc).split(":", 1)[0] if isinstance(exc, ValueError) else "local_pdf_index_or_registration_missing"

    def status(self):
        error = self.error or ("local_pdf_source_closed" if self.db is None else None)
        return {"status": "not_ready" if error else "ready", "provider": "local_pdf",
                "index": self.meta, "errors": [{"code": error}] if error else [],
                "source_verification": "index_and_registration_checked_original_hash_checked_at_return",
                "ocr": "not_run", "network_requests": 0}

    def _ready(self):
        if self.error or self.db is None:
            raise ValueError(self.error or "local_pdf_source_closed")

    def chapters(self):
        self._ready()
        rows = catalog_chapters(self.db)
        by_id = {row["id"]: json.loads(row["source_json"]) for row in self.db.execute("SELECT id,source_json FROM chapters")}
        return [{**row, "aliases": by_id[row["id"]]["aliases"], "source_kind": "local_pdf"} for row in rows]

    def retrieve(self, query, route=None, limit=24):
        self._ready()
        if type(limit) is not int or not 1 <= limit <= 96:
            raise ValueError("local_pdf_candidate_limit_invalid")
        chapter_ids = {c["id"] for c in (route or {}).get("chapters", [])}
        chapters = self.chapters()
        documents, routed_pools = [], {}
        # Give each document a turn. One large book must not consume the whole
        # provider budget; within each book retain lexical and routed pools.
        for code in self.sources:
            lexical = [row | {"retrieval_pool": "local_pdf_native_or_declared_navigation"}
                       for row in catalog_retrieve(self.db, query, limit, catalog=code)]
            pools = [lexical]
            for chapter in chapters:
                if chapter["catalog"] == code and chapter["id"] in chapter_ids:
                    routed = [dict(row) | {"lexical_score": None, "retrieval_pool": "local_pdf_selected_chapter"}
                        for row in self.db.execute(
                            "SELECT * FROM pages WHERE catalog=? AND pdf_page BETWEEN ? AND ? ORDER BY native_text_sparse,pdf_page LIMIT ?",
                            (code, chapter["start_page"], chapter["end_page"], limit))]
                    routed_pools[chapter["id"]] = routed
                    pools.append(routed)
            documents.append(_round_robin(pools, limit))
        # Reserve one candidate per selected chapter before cross-book lexical
        # round-robin can consume a small budget. This matters when the query's
        # language differs from the source body. A selected chapter is still
        # only a navigation candidate; its page must pass the normal screen.
        ordered = [routed_pools[c["id"]] for c in (route or {}).get("chapters", [])
                   if c["id"] in routed_pools and routed_pools[c["id"]]]
        selected = _round_robin(ordered, min(limit, len(ordered))) if ordered else []
        seen = {row["id"] for row in selected}
        for row in _round_robin(documents, limit):
            if len(selected) == limit:
                break
            if row["id"] not in seen:
                selected.append(row)
                seen.add(row["id"])
        for row in selected:
            row.update(source_kind="local_pdf", text_origin="poppler_native_text",
                       native_text_missing=not row["body"].strip(), navigation_only=not row["body"].strip())
        return selected

    def source_result(self, page, query):
        self._ready()
        # Reload the registered row, so caller-supplied text/locators cannot alter evidence.
        row = self.db.execute("SELECT * FROM pages WHERE id=?", (page["id"],)).fetchone()
        if row is None:
            raise ValueError("local_pdf_page_not_in_index")
        row = dict(row)
        source = self.sources[row["catalog"]]
        path = Path(source["path"])
        path = (path if path.is_absolute() else self.root / path).resolve()
        # Deliberately rehash on every returned page; no cross-query hash cache.
        if not path.is_file() or file_hash(path) != source["sha256"]:
            raise ValueError("local_pdf_source_changed_reindex_required")
        if any(page.get(k) != row[k] for k in ("body", "text_sha256", "pdf_page", "source_sha256")):
            raise ValueError("local_pdf_candidate_identity_mismatch")
        chapter = self.db.execute("SELECT title FROM chapters WHERE id=?", (row["chapter_id"],)).fetchone()
        locator = {"schema": LOCATOR_SCHEMA, "kind": "local_pdf", "document_id": source["id"],
                   "document_sha256": source["sha256"], "physical_page": row["pdf_page"],
                   "page_count": source["page_count"], "title": source["title"], "edition": source["edition"],
                   "printed_page": None, "printed_page_status": "unverified"}
        return {"id": row["id"], "catalog": row["catalog"], "catalog_title": source["title"],
                "chapter": chapter[0] if chapter else "未登记章节", "headings": [h["text"] for h in json.loads(row["titles_json"])],
                "pdf_page": row["pdf_page"], "printed_page": None, "page_id": row["page_id"],
                "source_pdf": str(path), "source_url": "", "source_sha256": source["sha256"],
                "pdf_uri": path.as_uri() + "#page=" + str(row["pdf_page"]), "source_locator": locator,
                "text_origin": "poppler_native_text", "native_text_missing": not row["body"].strip(),
                "navigation_only": not row["body"].strip(), "series_references": [],
                "excerpt": excerpt(row["body"], query), "text_sha256": row["text_sha256"],
                "native_text_sparse": bool(row["native_text_sparse"]),
                "parameter_verification": {"status": "not_run", "product_match": "unknown"},
                "lexical_score": page.get("lexical_score")}

    def close(self):
        if self.db is not None:
            self.db.close()
            self.db = None


def _round_robin(pools, limit):
    selected, seen = [], set()
    for offset in range(limit):
        for pool in pools:
            if offset < len(pool) and pool[offset]["id"] not in seen:
                seen.add(pool[offset]["id"])
                selected.append(pool[offset])
                if len(selected) == limit:
                    return selected
    return selected
