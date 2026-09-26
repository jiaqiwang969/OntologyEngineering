#!/usr/bin/env python3
"""Compute the deterministic content digest of the canonical CAD Agent tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from collections.abc import Iterator
from pathlib import Path


DIGEST_FORMAT = "cad-agent-canonical-content-sha256-v1"
DEFAULT_ROOT = Path(__file__).resolve().parents[1]
EXCLUDED_DIRECTORIES = frozenset(
    {
        ".cache",
        ".git",
        ".mypy_cache",
        ".pytest_cache",
        ".ruff_cache",
        ".venv",
        "__pycache__",
    }
)
EXCLUDED_FILES = frozenset({"BUILD_INFO", ".DS_Store"})
EXCLUDED_SUFFIXES = frozenset({".pyc", ".pyo"})


def _tree_entries(
    root: Path, directory: Path | None = None
) -> Iterator[tuple[str, str, bytes]]:
    """Yield sorted regular-file and symlink records without following links."""

    current = root if directory is None else directory
    with os.scandir(current) as scan:
        entries = sorted(scan, key=lambda entry: entry.name)
    for entry in entries:
        if entry.name in EXCLUDED_FILES or Path(entry.name).suffix in EXCLUDED_SUFFIXES:
            continue
        path = Path(entry.path)
        relative = path.relative_to(root).as_posix()
        if entry.is_symlink():
            if entry.name in EXCLUDED_DIRECTORIES:
                continue
            yield (
                "L",
                relative,
                os.readlink(path).encode("utf-8", errors="surrogateescape"),
            )
        elif entry.is_dir(follow_symlinks=False):
            if entry.name not in EXCLUDED_DIRECTORIES:
                yield from _tree_entries(root, path)
        elif entry.is_file(follow_symlinks=False):
            yield "F", relative, path.read_bytes()


def canonical_digest(root: Path) -> tuple[str, int]:
    """Return SHA-256 and entry count for the canonical, length-framed stream."""

    resolved = root.expanduser().resolve()
    if not (resolved / "SKILL.md").is_file():
        raise ValueError(f"not a CAD Agent skill root (missing SKILL.md): {resolved}")
    digest = hashlib.sha256()
    digest.update(DIGEST_FORMAT.encode("ascii") + b"\0")
    count = 0
    for kind, relative, payload in _tree_entries(resolved):
        path_bytes = relative.encode("utf-8", errors="surrogateescape")
        digest.update(kind.encode("ascii"))
        digest.update(len(path_bytes).to_bytes(8, "big"))
        digest.update(path_bytes)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
        count += 1
    return digest.hexdigest(), count


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=DEFAULT_ROOT,
        help="skill root; defaults to the parent of this script regardless of cwd",
    )
    parser.add_argument("--json", action="store_true", help="emit metadata as JSON")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        value, count = canonical_digest(args.root)
    except (OSError, ValueError) as exc:
        raise SystemExit(f"ERROR: {exc}") from exc
    if args.json:
        print(
            json.dumps(
                {
                    "algorithm": "sha256",
                    "digest": value,
                    "entry_count": count,
                    "format": DIGEST_FORMAT,
                    "root": str(args.root.expanduser().resolve()),
                },
                ensure_ascii=False,
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print(value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
