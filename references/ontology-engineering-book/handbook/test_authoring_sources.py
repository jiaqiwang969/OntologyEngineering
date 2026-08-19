"""Integrity tests for the Vol.1 clone-local TeX authoring tree."""

from __future__ import annotations

import hashlib
import re
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent
BOOK_ROOT = HERE.parent
LOCK = BOOK_ROOT / "authoring-sources.sha256"
LOCK_LINE = re.compile(r"^([0-9a-f]{64})  ([^\0]+)$")
CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-ontology-foundations",
    "ch03-ontology-methodology",
    "ch04-ontology-languages",
    "ch05-reasoning",
    "ch06-applications",
    "ch07-knowledge-graph",
    "ch08-ontology-llm",
    "ch09-capstone-manufacturing",
)
HANDBOOK_ARTIFACT_SUFFIXES = (
    ".aux",
    ".bbl",
    ".bcf",
    ".blg",
    ".dvi",
    ".fdb_latexmk",
    ".fls",
    ".idx",
    ".ilg",
    ".ind",
    ".lof",
    ".log",
    ".lot",
    ".nav",
    ".out",
    ".pdf",
    ".run.xml",
    ".snm",
    ".synctex.gz",
    ".toc",
    ".vrb",
    ".xdv",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_authoring_paths() -> set[str]:
    paths = {
        "README.md",
        "resources/README.md",
    }
    paths.update(f"{directory}/README.md" for directory in CHAPTER_DIRS)
    paths.update(
        path.relative_to(BOOK_ROOT).as_posix()
        for path in HERE.iterdir()
        if path.is_file()
        and not path.name.startswith(".")
        and path.name != LOCK.name
        and not path.name.endswith(HANDBOOK_ARTIFACT_SUFFIXES)
    )
    paths.update(
        path.relative_to(BOOK_ROOT).as_posix()
        for directory in ("chapters", "figures", "fragments")
        for path in (HERE / directory).rglob("*")
        if path.is_file()
        and not any(
            part.startswith(".") for part in path.relative_to(HERE / directory).parts
        )
    )
    return paths


def read_lock() -> dict[str, str]:
    entries: dict[str, str] = {}
    for line_number, raw in enumerate(LOCK.read_text(encoding="utf-8").splitlines(), 1):
        if not raw or raw.startswith("#"):
            continue
        match = LOCK_LINE.fullmatch(raw)
        if not match:
            raise AssertionError(f"{LOCK.name}:{line_number}: invalid lock line")
        digest, relative = match.groups()
        path = Path(relative)
        if (
            not relative
            or relative == "."
            or relative != relative.strip()
            or any(char in relative for char in "\\\0\r\n")
            or path.is_absolute()
            or any(part in {".", ".."} for part in path.parts)
            or relative != path.as_posix()
        ):
            raise AssertionError(f"{LOCK.name}:{line_number}: unsafe relative path: {relative}")
        if relative in entries:
            raise AssertionError(f"{LOCK.name}:{line_number}: duplicate path: {relative}")
        try:
            (BOOK_ROOT / path).resolve().relative_to(BOOK_ROOT.resolve())
        except ValueError as exc:
            raise AssertionError(
                f"{LOCK.name}:{line_number}: path escapes book root: {relative}"
            ) from exc
        entries[relative] = digest
    return entries


class AuthoringSourceLockTests(unittest.TestCase):
    def test_lock_declares_the_volume_root(self) -> None:
        self.assertFalse(
            (HERE / LOCK.name).exists(),
            "retired handbook-root lock must not coexist with the volume-root lock",
        )
        text = LOCK.read_text(encoding="utf-8")
        self.assertIn(
            "Paths are relative to references/ontology-engineering-book", text
        )
        self.assertNotIn("relative to this handbook directory", text)

    def test_lock_is_closed_over_current_authoring_tree(self) -> None:
        entries = read_lock()
        self.assertEqual(set(entries), expected_authoring_paths())
        self.assertEqual(list(entries), sorted(entries))

    def test_every_locked_hash_matches(self) -> None:
        for relative, expected in read_lock().items():
            path = BOOK_ROOT / relative
            self.assertTrue(path.is_file(), relative)
            self.assertEqual(sha256_file(path), expected, relative)

    def test_pdf_and_lock_do_not_claim_to_be_authoring_inputs(self) -> None:
        entries = read_lock()
        self.assertNotIn("handbook/工程本体论-全书.pdf", entries)
        self.assertNotIn("authoring-sources.sha256", entries)


if __name__ == "__main__":
    unittest.main()
