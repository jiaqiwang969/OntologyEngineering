#!/usr/bin/env python3
"""Collect reproducible evidence for the two-book artifact manifest.

The collector only records technical observations and initializes pending
governance records.  It never grants rights or publication approval.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Any, Mapping, Sequence

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts import book_release_artifacts as release  # noqa: E402
from scripts import check_public_privacy as privacy  # noqa: E402
from scripts import update_book_authoring_locks as authoring  # noqa: E402


EVIDENCE_ROOT = ROOT / "references" / "release-evidence"
TEST_REPORT_SCHEMA = release.REGRESSION_SCHEMA
PDF_QA_SCHEMA = "ontology-engineering.pdf-qa-evidence/v1"
VISUAL_REVIEW_SCHEMA = "ontology-engineering.pdf-visual-review/v1"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
TEST_SUMMARY = re.compile(
    r"(?P<passed>\d+) passed(?:, (?P<extra>[^\n]+))? in (?P<seconds>[0-9.]+)s"
)


class EvidenceCollectionError(RuntimeError):
    """Raised when evidence cannot be collected without guessing."""


def _write_json(path: Path, value: Mapping[str, Any]) -> None:
    release._atomic_write(path, release.canonical_bytes(value))


def _read_json(path: Path, *, label: str) -> Mapping[str, Any]:
    if not path.is_file():
        raise EvidenceCollectionError(f"{label} is missing: {path}")
    return release._load_json_bytes(path.read_bytes(), label=label, canonical=True)


def initialize_governance(
    root: Path, *, reset_existing: bool = False
) -> dict[str, Any]:
    """Initialize pending records without overwriting an existing decision."""

    evidence_root = root / "references" / "release-evidence"
    rights_path = evidence_root / "rights.json"
    approval_path = evidence_root / "publication-approval.json"
    existing_paths = [path.is_file() for path in (rights_path, approval_path)]
    if any(existing_paths) and not all(existing_paths):
        raise EvidenceCollectionError(
            "governance evidence is partial; refusing to guess or overwrite it"
        )
    if all(existing_paths) and not reset_existing:
        rights_existing = _read_json(rights_path, label=rights_path.name)
        approval_existing = _read_json(approval_path, label=approval_path.name)
        try:
            release._verify_governance_evidence(
                root, kind="rights", document=rights_existing
            )
            release._verify_governance_evidence(
                root,
                kind="publication_approval",
                document=approval_existing,
                rights_evidence_sha256=release.sha256_file(rights_path),
            )
        except release.ReleaseArtifactError as exc:
            raise EvidenceCollectionError(str(exc)) from exc
        return {
            "publication_approval": approval_existing["status"],
            "rights": rights_existing["status"],
        }
    source_paths = (
        "docs/PRIVACY-AND-RIGHTS.md",
        "docs/PUBLIC-RELEASE-STATUS.md",
        "references/product-trustworthiness-book/handbook/book-metadata.tex",
    )
    rights = {
        "$schema": "ontology-engineering.rights-evidence/v1",
        "reason": (
            "Rights holder, licenses, ISBN/CIP, and public redistribution approval "
            "remain unconfirmed; technical integration cannot decide them."
        ),
        "required_authority": "authorized-rights-holder-and-publisher",
        "sources": [release.artifact_ref(root, item) for item in source_paths],
        "status": "pending",
    }
    _write_json(rights_path, rights)
    rights_sha = release.sha256_file(rights_path)
    approval = {
        "$schema": "ontology-engineering.publication-approval/v1",
        "reason": (
            "No authorized person has approved public release of the two-book "
            "artifact; this record is intentionally pending."
        ),
        "required_authority": "authorized-publication-approver",
        "rights_evidence_sha256": rights_sha,
        "scope": "two-book-artifact-publication",
        "status": "pending",
    }
    _write_json(approval_path, approval)
    return {
        "publication_approval": approval["status"],
        "rights": rights["status"],
    }


def collect_static(root: Path) -> dict[str, Any]:
    authoring_report = authoring.check_or_write(write=False, skill_root=root)
    _write_json(root / release.EVIDENCE_PATHS["authoring_locks"], authoring_report)

    from runtime import doctor_runtime

    checks = doctor_runtime.run_checks(root / "runtime", include_venv=True)
    doctor_report = {
        "checks": [asdict(item) for item in checks],
        "mode": "doctor",
        "ok": not any(item.level == "error" for item in checks),
    }
    _write_json(root / release.EVIDENCE_PATHS["runtime_identity"], doctor_report)

    findings, _checked = privacy.run(root, tracked_only=False, include_ignored=False)
    privacy_report = {
        "findings": [asdict(item) for item in findings],
        "ok": not findings,
        "scope": "tracked-and-unignored-worktree",
    }
    _write_json(root / release.EVIDENCE_PATHS["privacy"], privacy_report)
    return {
        "authoring_locks": bool(authoring_report.get("passed")),
        "privacy": privacy_report["ok"],
        "runtime_identity": doctor_report["ok"],
    }


def collect_book_bindings(root: Path) -> dict[str, Any]:
    from ontology_engineering import semantica_runtime

    result = semantica_runtime.verify_book_source_bindings(root)
    _write_json(root / release.EVIDENCE_PATHS["book_source_bindings"], result)
    return {
        "passed": bool(result.get("passed")),
        "check_count": result.get("check_count"),
    }


def _git_head(root: Path) -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    value = completed.stdout.strip()
    if completed.returncode != 0 or not HEX40.fullmatch(value):
        raise EvidenceCollectionError(f"cannot resolve a full Git commit at {root}")
    return value


def _regression_report(
    *,
    repository: str,
    commit: str,
    display_command: Sequence[str],
    completed: subprocess.CompletedProcess[str],
    log: Mapping[str, Any],
) -> dict[str, Any]:
    output = (completed.stdout + "\n" + completed.stderr).encode("utf-8")
    matches = list(TEST_SUMMARY.finditer(output.decode("utf-8", errors="replace")))
    summary = matches[-1].group(0) if matches else "no pytest pass summary"
    return {
        "$schema": TEST_REPORT_SCHEMA,
        "command": list(display_command),
        "commit": commit,
        "log": dict(log),
        "passed": completed.returncode == 0 and bool(matches),
        "passed_count": int(matches[-1].group("passed")) if matches else 0,
        "repository": repository,
        "return_code": completed.returncode,
        "summary": summary,
    }


def _normalized_regression_output(
    completed: subprocess.CompletedProcess[str],
    *,
    root: Path,
    semantica_root: Path,
) -> bytes:
    """Remove only the two known checkout prefixes from a stored test log.

    Pytest warnings include absolute source paths.  Those paths are useful while
    running locally but are neither stable evidence nor safe public metadata.
    Keep every other absolute path intact so the privacy gate can still reject
    an unrelated leak instead of having a broad sanitizer hide it.
    """

    output = completed.stdout + "\n" + completed.stderr
    replacements = (
        (root.expanduser().resolve(), "<ontology-engineering-root>"),
        (semantica_root.expanduser().resolve(), "<semantica-root>"),
    )
    for checkout, token in sorted(
        replacements, key=lambda item: len(str(item[0])), reverse=True
    ):
        prefix = re.escape(str(checkout))
        output = re.sub(rf"{prefix}(?=$|[/\\])", token, output)
    return output.encode("utf-8")


def collect_regressions(root: Path, semantica_root: Path) -> dict[str, Any]:
    source_lock = json.loads(
        (root / release.SOURCE_LOCK_PATH).read_text(encoding="utf-8")
    )
    expected_semantica_commit = source_lock["source"]["commit"]
    actual_semantica_commit = _git_head(semantica_root)
    if actual_semantica_commit != expected_semantica_commit:
        raise EvidenceCollectionError(
            "Semantica checkout HEAD differs from the formal source lock"
        )
    status = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=all"],
        cwd=semantica_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if status.returncode != 0 or status.stdout:
        raise EvidenceCollectionError("Semantica regression checkout is not clean")
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(semantica_root)
    semantica_completed = subprocess.run(
        [
            sys.executable,
            "-m",
            "pytest",
            "-q",
            "tests/ontology",
            "tests/chapter_packages",
        ],
        cwd=semantica_root,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )
    semantica_report = _regression_report(
        repository=str(source_lock["source"]["canonical_repository"]),
        commit=actual_semantica_commit,
        display_command=(
            "runtime-python",
            "-m",
            "pytest",
            "-q",
            "tests/ontology",
            "tests/chapter_packages",
        ),
        completed=semantica_completed,
        log={},
    )
    semantica_output = _normalized_regression_output(
        semantica_completed,
        root=root,
        semantica_root=semantica_root,
    )
    semantica_log_path = root / release.REGRESSION_LOG_PATHS["semantica_regression"]
    release._atomic_write(semantica_log_path, semantica_output)
    semantica_report["log"] = release.artifact_ref(
        root, release.REGRESSION_LOG_PATHS["semantica_regression"]
    )
    _write_json(root / release.EVIDENCE_PATHS["semantica_regression"], semantica_report)

    oe_completed = subprocess.run(
        [sys.executable, "-m", "pytest", "-q", "tests"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    oe_report = _regression_report(
        repository="https://github.com/jiaqiwang969/OntologyEngineering.git",
        commit=_git_head(root),
        display_command=("runtime-python", "-m", "pytest", "-q", "tests"),
        completed=oe_completed,
        log={},
    )
    oe_output = _normalized_regression_output(
        oe_completed,
        root=root,
        semantica_root=semantica_root,
    )
    oe_log_path = root / release.REGRESSION_LOG_PATHS["ontology_engineering_regression"]
    release._atomic_write(oe_log_path, oe_output)
    oe_report["log"] = release.artifact_ref(
        root, release.REGRESSION_LOG_PATHS["ontology_engineering_regression"]
    )
    _write_json(
        root / release.EVIDENCE_PATHS["ontology_engineering_regression"],
        oe_report,
    )
    return {
        "ontology_engineering": oe_report["passed"],
        "semantica": semantica_report["passed"],
    }


def _pdfinfo(path: Path) -> Mapping[str, str]:
    completed = subprocess.run(
        ["pdfinfo", str(path)], check=False, capture_output=True, text=True
    )
    if completed.returncode != 0:
        raise EvidenceCollectionError(f"pdfinfo failed for {path.name}")
    result: dict[str, str] = {}
    for line in completed.stdout.splitlines():
        if ":" in line:
            key, value = line.split(":", 1)
            result[key.strip()] = value.strip()
    return result


def _fonts_embedded(path: Path) -> tuple[bool, int]:
    try:
        return release._pdf_font_report(path)
    except release.ReleaseArtifactError as exc:
        raise EvidenceCollectionError(str(exc)) from exc


def _text_characters(path: Path) -> int:
    completed = subprocess.run(
        ["pdftotext", str(path), "-"], check=False, capture_output=True
    )
    if completed.returncode != 0:
        raise EvidenceCollectionError(f"pdftotext failed for {path.name}")
    return len(completed.stdout.decode("utf-8", errors="replace").strip())


def collect_pdf_qa(root: Path, visual_review_path: Path) -> dict[str, Any]:
    visual = _read_json(visual_review_path, label="PDF visual review")
    if visual.get("$schema") != VISUAL_REVIEW_SCHEMA:
        raise EvidenceCollectionError("PDF visual review schema is not recognized")
    visual_ref = release.artifact_ref(
        root, visual_review_path.relative_to(root).as_posix()
    )
    books = []
    for volume in ("vol1", "vol2"):
        relative = release.BOOK_SPECS[volume]["pdf"]
        path = root / relative
        info = _pdfinfo(path)
        embedded, font_count = _fonts_embedded(path)
        pages = int(info.get("Pages", "0"))
        characters = _text_characters(path)
        visual_book = next(
            (
                item
                for item in visual.get("books", [])
                if isinstance(item, dict) and item.get("volume") == volume
            ),
            None,
        )
        visual_passed = bool(visual_book and visual_book.get("passed") is True)
        books.append(
            {
                "font_count": font_count,
                "fonts_embedded": embedded,
                "page_count": pages,
                "pdf": release.artifact_ref(root, relative),
                "text_characters": characters,
                "visual_passed": visual_passed,
                "volume": volume,
            }
        )
    report = {
        "$schema": PDF_QA_SCHEMA,
        "books": books,
        "passed": bool(
            visual.get("passed") is True
            and all(
                item["fonts_embedded"]
                and item["page_count"] > 0
                and item["text_characters"] > 1000
                and item["visual_passed"]
                for item in books
            )
        ),
        "visual_review": visual_ref,
    }
    _write_json(root / release.EVIDENCE_PATHS["pdf_qa"], report)
    return {"passed": report["passed"], "books": len(books)}


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    subparsers = parser.add_subparsers(dest="command", required=True)
    governance = subparsers.add_parser(
        "governance",
        help="Initialize pending governance records without overwriting decisions.",
    )
    governance.add_argument(
        "--reset-existing",
        action="store_true",
        help="Explicitly replace both existing governance records with pending records.",
    )
    subparsers.add_parser("static", help="Collect lock, runtime, and privacy evidence.")
    subparsers.add_parser("book-bindings", help="Collect 29 chapter book bindings.")
    regressions = subparsers.add_parser(
        "regressions", help="Run both repository suites."
    )
    regressions.add_argument("--semantica-root", type=Path, required=True)
    pdf = subparsers.add_parser(
        "pdf-qa", help="Collect deterministic and reviewed PDF QA."
    )
    pdf.add_argument("--visual-review", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    root = args.root.expanduser().resolve()
    try:
        if args.command == "governance":
            report = initialize_governance(root, reset_existing=args.reset_existing)
        elif args.command == "static":
            report = collect_static(root)
        elif args.command == "book-bindings":
            report = collect_book_bindings(root)
        elif args.command == "regressions":
            report = collect_regressions(
                root, args.semantica_root.expanduser().resolve()
            )
        else:
            visual = args.visual_review.expanduser()
            if not visual.is_absolute():
                visual = root / visual
            report = collect_pdf_qa(root, visual.resolve())
        passed = all(value is not False for value in report.values())
        output = {"passed": passed, "results": report}
    except (
        KeyError,
        OSError,
        TypeError,
        UnicodeError,
        ValueError,
        EvidenceCollectionError,
        release.ReleaseArtifactError,
    ) as exc:
        output = {"passed": False, "error": str(exc)}
    print(json.dumps(output, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if output["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
