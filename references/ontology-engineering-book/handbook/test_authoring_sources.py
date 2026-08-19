"""Integrity tests for the Vol.1 clone-local TeX authoring tree."""

from __future__ import annotations

import hashlib
import re
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent
LOCK = HERE / "authoring-sources.sha256"
LOCK_LINE = re.compile(r"^([0-9a-f]{64})  ([^\0]+)$")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_authoring_paths() -> set[str]:
    paths = {
        "README.md",
        "build_handbook.py",
        "gen_figures.py",
        "main.tex",
        "make_deck_plan.py",
        "preamble.tex",
        "test_authoring_sources.py",
    }
    paths.update(
        path.relative_to(HERE).as_posix()
        for directory in ("chapters", "figures", "fragments")
        for path in (HERE / directory).iterdir()
        if path.is_file() and not path.name.startswith(".")
    )
    return paths


def read_lock() -> dict[str, str]:
    entries: dict[str, str] = {}
    for line_number, raw in enumerate(LOCK.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        match = LOCK_LINE.fullmatch(line)
        if not match:
            raise AssertionError(f"{LOCK.name}:{line_number}: invalid lock line")
        digest, relative = match.groups()
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts or relative != path.as_posix():
            raise AssertionError(f"{LOCK.name}:{line_number}: unsafe relative path: {relative}")
        if relative in entries:
            raise AssertionError(f"{LOCK.name}:{line_number}: duplicate path: {relative}")
        entries[relative] = digest
    return entries


class AuthoringSourceLockTests(unittest.TestCase):
    def test_lock_is_closed_over_current_authoring_tree(self) -> None:
        entries = read_lock()
        self.assertEqual(set(entries), expected_authoring_paths())
        self.assertEqual(list(entries), sorted(entries))

    def test_every_locked_hash_matches(self) -> None:
        for relative, expected in read_lock().items():
            path = HERE / relative
            self.assertTrue(path.is_file(), relative)
            self.assertEqual(sha256_file(path), expected, relative)

    def test_pdf_and_lock_do_not_claim_to_be_authoring_inputs(self) -> None:
        entries = read_lock()
        self.assertNotIn("工程本体论-全书.pdf", entries)
        self.assertNotIn(LOCK.name, entries)


if __name__ == "__main__":
    unittest.main()
