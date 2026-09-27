"""Verified original-page views shared by local PDFs and catalog sources."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile

from .misumi.catalog import digest, file_hash, write_json

LOCATOR_SCHEMA = "ontology-engineering.source-page/v1"


def _tool(name, arguments):
    if name not in {"pdfinfo", "pdftoppm"}:
        raise ValueError("source_view_tool_unsupported")
    if not shutil.which(name):
        raise ValueError("source_view_requires_" + name)
    try:
        if name == "pdfinfo":
            result = subprocess.run(["pdfinfo", *map(str, arguments)], capture_output=True,
                                    timeout=120, env={**os.environ, "LC_ALL": "C"})
        else:
            result = subprocess.run(["pdftoppm", *map(str, arguments)], capture_output=True,
                                    timeout=120, env={**os.environ, "LC_ALL": "C"})
    except subprocess.TimeoutExpired:
        raise ValueError("source_view_tool_timeout") from None
    if result.returncode:
        raise ValueError("source_view_tool_failed:" + name)
    return result.stdout + result.stderr


def pdf_page_count(path):
    info = _tool("pdfinfo", [Path(path).resolve()]).decode("utf-8", errors="replace")
    match = re.search(r"^Pages:\s+(\d+)\s*$", info, re.M)
    if not match or int(match[1]) < 1:
        raise ValueError("source_pdf_page_count_unknown")
    return int(match[1])


def verify_source_locator(result):
    """Check physical position in the identified file, not printed-page truth."""
    locator = result.get("source_locator")
    if not isinstance(locator, dict) or locator.get("schema") != LOCATOR_SCHEMA:
        raise ValueError("source_locator_schema_mismatch")
    if locator.get("kind") != "local_pdf":
        raise ValueError("source_locator_kind_unsupported")
    source = Path(result["source_pdf"])
    if (not source.is_file() or file_hash(source) != result["source_sha256"]
            or locator.get("document_sha256") != result["source_sha256"]):
        raise ValueError("source_locator_document_changed")
    page, count = locator.get("physical_page"), locator.get("page_count")
    if (type(page) is not int or type(count) is not int or not 1 <= page <= count
            or result.get("pdf_page") != page or pdf_page_count(source) != count):
        raise ValueError("source_locator_page_mismatch")
    for key in ("document_id", "title", "edition"):
        if not isinstance(locator.get(key), str) or not locator[key].strip():
            raise ValueError("source_locator_identity_missing")
    if (locator.get("printed_page_status") != "unverified"
            or locator.get("printed_page") is not None):
        raise ValueError("source_locator_printed_page_not_verified")
    return dict(locator)


def render_page(result, cache_root, dpi=144):
    """Render the whole source page on demand; never crop away conditions."""
    if type(dpi) is not int or not 72 <= dpi <= 300:
        raise ValueError("source_view_dpi_out_of_range")
    source = Path(result["source_pdf"]).resolve()
    if not source.is_file() or file_hash(source) != result["source_sha256"]:
        raise ValueError("source_view_source_changed")
    locator = verify_source_locator(result) if result.get("source_locator") else None
    # Catalog source_pdf is the independently archived single page. Its
    # pdf_page is the position in the complete book, not in this source file.
    page = locator["physical_page"] if locator else 1
    if not locator and pdf_page_count(source) != 1:
        raise ValueError("source_view_requires_explicit_page_locator")
    renderer = _tool("pdftoppm", ["-v"]).decode("utf-8", errors="replace").splitlines()[0]
    identity = {"source_sha256": result["source_sha256"], "source_file_page": page,
                "dpi": dpi, "renderer": renderer, "whole_page": True}
    cache = Path(cache_root).expanduser().resolve()
    cache.mkdir(parents=True, exist_ok=True, mode=0o700)
    key = digest(identity)
    image, record = cache / (key + ".png"), cache / (key + ".json")
    valid = None
    if image.is_file() and record.is_file():
        try:
            previous = json.loads(record.read_text())
            if previous["identity"] == identity and previous["image_sha256"] == file_hash(image):
                valid = previous
        except (ValueError, KeyError, OSError):
            pass
    if valid is None:
        with tempfile.TemporaryDirectory(prefix=".page-", dir=cache) as temporary:
            prefix = Path(temporary) / "page"
            _tool("pdftoppm", ["-f", page, "-l", page, "-singlefile", "-r", dpi,
                               "-png", source, prefix])
            rendered = prefix.with_suffix(".png")
            with rendered.open("rb") as stream:
                header = stream.read(24)
            if header[:8] != b"\x89PNG\r\n\x1a\n" or len(header) != 24:
                raise ValueError("source_view_invalid_image")
            width, height = struct.unpack(">II", header[16:24])
            if not width or not height:
                raise ValueError("source_view_invalid_image")
            rendered.chmod(0o600)
            os.replace(rendered, image)
        valid = {"identity": identity, "image_sha256": file_hash(image),
                 "width": width, "height": height}
        write_json(record, valid)
    return {"schema": "ontology-engineering.source-page-view/v1", **valid,
            "source_locator": locator, "source_pdf": str(source),
            "source_pdf_uri": source.as_uri() + "#page=" + str(page),
            "catalog_pdf_page": result.get("pdf_page"), "printed_page": result.get("printed_page"),
            "image": str(image), "image_uri": image.as_uri(),
            "interpretation": "not_run", "numeric_verification": "not_run"}
