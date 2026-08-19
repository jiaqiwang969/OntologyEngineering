#!/usr/bin/env python3
"""Check or deliberately refresh the clone-local authoring locks for both books."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
from typing import Iterable, Sequence


SKILL_ROOT = Path(__file__).resolve().parents[1]
VOL1_BOOK = SKILL_ROOT / "references" / "ontology-engineering-book"
VOL2_BOOK = SKILL_ROOT / "references" / "product-trustworthiness-book"
VOL1_CHAPTER_DIRS = (
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
VOL2_CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-concepts-terminology",
    "ch03-safety-management",
    "ch04-concept-hara",
    "ch05-system-development",
    "ch06-hardware-development",
    "ch07-software-development",
    "ch08-asil-decomposition-dfa",
    "ch09-production-operation",
    "ch10-supporting-processes",
    "ch11-claim-ontology",
    "ch12-identity-ontology",
    "ch13-governance-ontology",
    "ch14-context-hazard-ontology",
    "ch15-requirements-ontology",
    "ch16-measurement-ontology",
    "ch17-change-ontology",
    "ch18-dependency-ontology",
    "ch19-field-ontology",
    "ch20-assurance-ontology",
)

VOL1_HEADER = (
    "# SHA-256 lock for the clone-local Vol.1 book and TeX authoring tree.\n"
    "# Paths are relative to references/ontology-engineering-book; PDF and this lock are excluded.\n"
)
VOL2_SOURCE_HEADER = (
    "# SHA-256 lock for the current rewritten book sources and text factory.\n"
    "# Paths are relative to references/product-trustworthiness-book.\n"
    "# Update deliberately whenever an authoritative Markdown or factory source changes.\n"
)
VOL2_ASSET_HEADER = (
    "# SHA-256 lock for binary publication assets in this repository clone.\n"
    "# Paths are relative to references/product-trustworthiness-book.\n"
    "# Default isolated builds consume these local files; no mother repo is required.\n"
)
VOL2_GUIDE_HEADER = (
    "# SHA-256 lock for Vol.2 formal-search reader guides.\n"
    "# Paths are relative to references/product-trustworthiness-book.\n"
    "# Formal search fails closed if any guide drifts from this reviewed set.\n"
)
VOL1_HANDBOOK_ARTIFACT_SUFFIXES = (
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


class AuthoringLockError(RuntimeError):
    """Raised when an authoring tree or its lock is incomplete or inconsistent."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _canonical_relative(relative: str) -> str:
    """Return a lock-safe POSIX relative path or fail closed."""

    if (
        not relative
        or relative == "."
        or relative != relative.strip()
        or any(char in relative for char in "\\\0\r\n")
    ):
        raise AuthoringLockError(f"unsafe authoring lock path: {relative!r}")
    path = Path(relative)
    if (
        path.is_absolute()
        or any(part in {".", ".."} for part in path.parts)
        or relative != path.as_posix()
    ):
        raise AuthoringLockError(f"unsafe authoring lock path: {relative!r}")
    return relative


def _require_files(root: Path, relative_paths: Iterable[str]) -> tuple[str, ...]:
    paths = tuple(
        sorted({_canonical_relative(relative) for relative in relative_paths})
    )
    root_resolved = root.resolve()
    missing: list[str] = []
    for relative in paths:
        candidate = (root / relative).resolve()
        try:
            candidate.relative_to(root_resolved)
        except ValueError as exc:
            raise AuthoringLockError(
                f"authoring input escapes its lock root: {relative}"
            ) from exc
        if not candidate.is_file():
            missing.append(relative)
    if missing:
        raise AuthoringLockError(
            "authoring input is missing: {}".format(", ".join(missing))
        )
    return paths


def vol1_paths(root: Path = VOL1_BOOK) -> tuple[str, ...]:
    relative = {
        "README.md",
        "resources/README.md",
    }
    relative.update(f"{directory}/README.md" for directory in VOL1_CHAPTER_DIRS)
    handbook = root / "handbook"
    if not handbook.is_dir():
        raise AuthoringLockError("Vol.1 handbook directory is missing")
    legacy_lock = handbook / "authoring-sources.sha256"
    if legacy_lock.exists():
        raise AuthoringLockError(
            "retired Vol.1 handbook-root authoring lock must not coexist with the "
            "book-root authoring-sources.sha256"
        )
    relative.update(
        path.relative_to(root).as_posix()
        for path in handbook.iterdir()
        if path.is_file()
        and not path.name.startswith(".")
        and not path.name.endswith(VOL1_HANDBOOK_ARTIFACT_SUFFIXES)
    )
    for directory in ("chapters", "figures", "fragments"):
        source = handbook / directory
        if not source.is_dir():
            raise AuthoringLockError(
                f"Vol.1 authoring directory is missing: {directory}"
            )
        relative.update(
            path.relative_to(root).as_posix()
            for path in source.rglob("*")
            if path.is_file()
            and not any(part.startswith(".") for part in path.relative_to(source).parts)
        )
    return _require_files(root, relative)


def vol2_source_paths(root: Path = VOL2_BOOK) -> tuple[str, ...]:
    relative = {
        "front-matter/preface.md",
        "handbook/book-metadata.tex",
        "handbook/build_handbook.py",
        "handbook/build_isolated.py",
        "handbook/main.tex",
        "handbook/preamble.tex",
        "handbook/test_build_handbook.py",
    }
    relative.update(
        path.relative_to(root).as_posix()
        for path in (root / "appendices").glob("appendix-*.md")
        if path.is_file()
    )
    relative.update(f"{directory}/chapter.md" for directory in VOL2_CHAPTER_DIRS)
    paths = _require_files(root, relative)
    chapters = [
        path for path in paths if path.startswith("ch") and path.endswith("/chapter.md")
    ]
    appendices = [path for path in paths if path.startswith("appendices/")]
    if len(chapters) != 20 or len(appendices) != 4:
        raise AuthoringLockError(
            f"Vol.2 requires 20 chapter sources and 4 appendices; found "
            f"{len(chapters)} and {len(appendices)}"
        )
    return paths


def vol2_search_guide_paths(root: Path = VOL2_BOOK) -> tuple[str, ...]:
    relative = {
        "README.md",
        "propositions-index.md",
        "handbook/README.md",
        *(f"{directory}/README.md" for directory in VOL2_CHAPTER_DIRS),
    }
    paths = _require_files(root, relative)
    chapter_guides = [
        path for path in paths if path.startswith("ch") and path.endswith("/README.md")
    ]
    if len(chapter_guides) != 20:
        raise AuthoringLockError(
            f"Vol.2 requires 20 formal-search chapter guides; found {len(chapter_guides)}"
        )
    return paths


def vol2_asset_paths(root: Path = VOL2_BOOK) -> tuple[str, ...]:
    relative: set[str] = set()
    for directory in ("handbook/figures-imagegen", "handbook/figures-rendered"):
        source = root / directory
        if not source.is_dir():
            raise AuthoringLockError(f"Vol.2 asset directory is missing: {directory}")
        relative.update(
            path.relative_to(root).as_posix()
            for path in source.rglob("*")
            if path.is_file()
        )
    return _require_files(root, relative)


def render_lock(root: Path, relative_paths: Sequence[str], header: str) -> str:
    paths = _require_files(root, relative_paths)
    body = "".join(
        f"{sha256_file(root / relative)}  {relative}\n" for relative in paths
    )
    return header + body


def lock_specs(
    skill_root: Path = SKILL_ROOT,
) -> tuple[tuple[str, Path, Path, str], ...]:
    vol1 = skill_root / "references" / "ontology-engineering-book"
    vol2 = skill_root / "references" / "product-trustworthiness-book"
    return (
        (
            "vol1_sources",
            vol1,
            vol1 / "authoring-sources.sha256",
            render_lock(vol1, vol1_paths(vol1), VOL1_HEADER),
        ),
        (
            "vol2_sources",
            vol2,
            vol2 / "handbook" / "current-source.sha256",
            render_lock(vol2, vol2_source_paths(vol2), VOL2_SOURCE_HEADER),
        ),
        (
            "vol2_assets",
            vol2,
            vol2 / "handbook" / "authoring-assets.sha256",
            render_lock(vol2, vol2_asset_paths(vol2), VOL2_ASSET_HEADER),
        ),
        (
            "vol2_search_guides",
            vol2,
            vol2 / "handbook" / "formal-search-guides.sha256",
            render_lock(
                vol2,
                vol2_search_guide_paths(vol2),
                VOL2_GUIDE_HEADER,
            ),
        ),
    )


def _atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    except Exception:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def check_or_write(*, write: bool, skill_root: Path = SKILL_ROOT) -> dict[str, object]:
    results: list[dict[str, object]] = []
    for label, _root, lock_path, expected in lock_specs(skill_root):
        actual = lock_path.read_text(encoding="utf-8") if lock_path.is_file() else None
        matched = actual == expected
        if write and not matched:
            _atomic_write(lock_path, expected)
            matched = True
        results.append(
            {
                "lock": label,
                "path": lock_path.relative_to(skill_root).as_posix(),
                "matched": matched,
                "updated": bool(write and actual != expected),
            }
        )
    return {
        "schema_version": "1.0",
        "mode": "write" if write else "check",
        "passed": all(bool(item["matched"]) for item in results),
        "locks": results,
    }


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="Deliberately replace drifted lock files after reviewed authoring changes.",
    )
    parser.add_argument(
        "--json", action="store_true", help="Emit machine-readable JSON."
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        report = check_or_write(write=args.write)
    except (OSError, AuthoringLockError) as exc:
        report = {
            "schema_version": "1.0",
            "mode": "write" if args.write else "check",
            "passed": False,
            "error": str(exc),
        }
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        for item in report.get("locks", []):
            state = (
                "UPDATED"
                if item["updated"]
                else ("PASS" if item["matched"] else "DRIFT")
            )
            print(f"{state} {item['lock']}: {item['path']}")
        if "error" in report:
            print(f"ERROR: {report['error']}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
