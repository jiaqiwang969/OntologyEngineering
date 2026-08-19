#!/usr/bin/env python3
"""Create and verify the byte-closed two-book release-artifact manifest.

This verifier does not implement ontology semantics.  It binds candidate book
artifacts and independently replays the machine-verifiable evidence against
the sole Semantica executable package.  Version 1 deliberately cannot grant
rights or publication approval: unsigned, self-authored JSON is never treated
as human authority, so the artifact remains a candidate until a future trusted
approval mechanism is introduced.
"""

from __future__ import annotations

import argparse
import base64
import csv
from dataclasses import asdict
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tempfile
from typing import Any, Mapping, Sequence
import zipfile


SKILL_ROOT = Path(__file__).resolve().parents[1]
SCHEMA = "ontology-engineering.book-release-artifacts/v1"
SOURCE_LOCK_SCHEMA = "ontology-engineering.semantica-source-lock/v1"
REGRESSION_SCHEMA = "ontology-engineering.regression-evidence/v2"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
PACKAGE_MANIFEST = re.compile(
    r"^semantica/chapter_packages/(vol[12])/(ch\d{2})/manifest\.yaml$"
)
EXPECTED_PACKAGE_COORDINATES = frozenset(
    [("vol1", f"ch{index:02d}") for index in range(1, 10)]
    + [("vol2", f"ch{index:02d}") for index in range(1, 21)]
)

MANIFEST_PATH = "references/book-release-artifacts.json"
SOURCE_LOCK_PATH = "runtime/semantica-source-lock.json"
EVIDENCE_PATHS = {
    "authoring_locks": "references/release-evidence/authoring-locks.json",
    "book_source_bindings": "references/release-evidence/book-source-bindings.json",
    "ontology_engineering_regression": "references/release-evidence/ontology-engineering-regression.json",
    "pdf_qa": "references/release-evidence/pdf-qa.json",
    "privacy": "references/release-evidence/privacy.json",
    "runtime_identity": "references/release-evidence/runtime-identity.json",
    "semantica_regression": "references/release-evidence/semantica-regression.json",
}
GOVERNANCE_PATHS = {
    "publication_approval": "references/release-evidence/publication-approval.json",
    "rights": "references/release-evidence/rights.json",
}
REGRESSION_LOG_PATHS = {
    "ontology_engineering_regression": (
        "references/release-evidence/ontology-engineering-regression.log"
    ),
    "semantica_regression": "references/release-evidence/semantica-regression.log",
}
REGRESSION_COMMANDS = {
    "ontology_engineering_regression": (
        "runtime-python",
        "-m",
        "pytest",
        "-q",
        "tests",
    ),
    "semantica_regression": (
        "runtime-python",
        "-m",
        "pytest",
        "-q",
        "tests/ontology",
        "tests/chapter_packages",
    ),
}
REGRESSION_REPOSITORIES = {
    "ontology_engineering_regression": (
        "https://github.com/jiaqiwang969/OntologyEngineering.git"
    ),
    "semantica_regression": "https://github.com/jiaqiwang969/semantica.git",
}
TEST_SUMMARY = re.compile(
    r"(?P<passed>\d+) passed(?:, (?P<extra>[^\n]+?))? in [0-9.]+s"
)
BOOK_SPECS = {
    "vol1": {
        "title": "工程本体论",
        "pdf": "references/ontology-engineering-book/handbook/工程本体论-全书.pdf",
        "lock_prefix": "references/ontology-engineering-book/",
    },
    "vol2": {
        "title": "产品可信工程",
        "pdf": "references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf",
        "lock_prefix": "references/product-trustworthiness-book/",
    },
}


class ReleaseArtifactError(RuntimeError):
    """Raised when a release-artifact claim is malformed or unbound."""


def _duplicate_rejector(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ReleaseArtifactError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _reject_json_constant(value: str) -> None:
    raise ReleaseArtifactError(f"non-finite JSON number is forbidden: {value}")


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _require_exact_keys(
    value: Mapping[str, Any], expected: set[str], label: str
) -> None:
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise ReleaseArtifactError(
            f"{label} keys differ; missing={missing!r}, extra={extra!r}"
        )


def _load_json_bytes(raw: bytes, *, label: str, canonical: bool) -> Mapping[str, Any]:
    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=_duplicate_rejector,
            parse_constant=_reject_json_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, ReleaseArtifactError) as exc:
        raise ReleaseArtifactError(f"{label} is not strict UTF-8 JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise ReleaseArtifactError(f"{label} must be a JSON object")
    if canonical:
        try:
            expected = canonical_bytes(value)
        except (TypeError, ValueError) as exc:
            raise ReleaseArtifactError(f"{label} is not canonical JSON: {exc}") from exc
        if raw != expected:
            raise ReleaseArtifactError(
                f"{label} is not canonical JSON (UTF-8, sorted compact keys, one LF)"
            )
    return value


def _safe_file(root: Path, relative: Any, *, label: str) -> Path:
    if not isinstance(relative, str) or not relative:
        raise ReleaseArtifactError(f"{label}.path must be a non-empty string")
    pure = PurePosixPath(relative)
    if (
        pure.is_absolute()
        or ".." in pure.parts
        or "." in pure.parts
        or pure.as_posix() != relative
        or "\\" in relative
    ):
        raise ReleaseArtifactError(f"{label}.path is unsafe: {relative!r}")
    root_resolved = root.resolve()
    path = root.joinpath(*pure.parts)
    cursor = root
    for part in pure.parts:
        cursor = cursor / part
        if cursor.is_symlink():
            raise ReleaseArtifactError(f"{label}.path traverses a symlink: {relative}")
    if not path.is_file():
        raise ReleaseArtifactError(f"{label}.path is not a regular file: {relative}")
    try:
        path.resolve().relative_to(root_resolved)
    except ValueError as exc:
        raise ReleaseArtifactError(f"{label}.path escapes the repository") from exc
    return path


def artifact_ref(root: Path, relative: str) -> dict[str, Any]:
    path = _safe_file(root, relative, label="artifact")
    return {
        "path": relative,
        "sha256": sha256_file(path),
        "size_bytes": path.stat().st_size,
    }


def verify_artifact_ref(root: Path, value: Any, *, label: str) -> Path:
    if not isinstance(value, dict):
        raise ReleaseArtifactError(f"{label} must be an artifact object")
    _require_exact_keys(value, {"path", "sha256", "size_bytes"}, label)
    if not isinstance(value["sha256"], str) or not HEX64.fullmatch(value["sha256"]):
        raise ReleaseArtifactError(f"{label}.sha256 is not a lowercase SHA-256")
    if not isinstance(value["size_bytes"], int) or value["size_bytes"] < 0:
        raise ReleaseArtifactError(f"{label}.size_bytes is invalid")
    path = _safe_file(root, value["path"], label=label)
    if path.stat().st_size != value["size_bytes"]:
        raise ReleaseArtifactError(f"{label} size differs from the manifest")
    if sha256_file(path) != value["sha256"]:
        raise ReleaseArtifactError(f"{label} SHA-256 differs from the manifest")
    return path


def _pdf_pages(path: Path) -> int:
    completed = subprocess.run(
        ["pdfinfo", str(path)], check=False, capture_output=True, text=True
    )
    if completed.returncode != 0:
        raise ReleaseArtifactError(
            f"pdfinfo could not inspect {path.name}: {completed.stderr.strip()}"
        )
    for line in completed.stdout.splitlines():
        if line.startswith("Pages:"):
            try:
                pages = int(line.split(":", 1)[1].strip())
            except ValueError as exc:
                raise ReleaseArtifactError(
                    f"invalid pdfinfo page count for {path.name}"
                ) from exc
            if pages > 0:
                return pages
    raise ReleaseArtifactError(
        f"pdfinfo returned no positive page count for {path.name}"
    )


def _decode_package_manifest(raw: bytes, member: str) -> Mapping[str, Any]:
    # Chapter manifests are deliberately emitted as JSON-compatible YAML.  Do
    # not silently invoke a permissive YAML parser for release identity.
    return _load_json_bytes(raw, label=member, canonical=False)


def _safe_wheel_member(info: zipfile.ZipInfo) -> None:
    name = info.filename
    pure = PurePosixPath(name)
    if (
        not name
        or pure.is_absolute()
        or ".." in pure.parts
        or "." in pure.parts
        or pure.as_posix() != name
        or "\\" in name
    ):
        raise ReleaseArtifactError(f"wheel contains an unsafe member path: {name!r}")
    file_type = (info.external_attr >> 16) & 0o170000
    if file_type not in {0, 0o100000}:
        raise ReleaseArtifactError(
            f"wheel contains a non-regular special member: {name}"
        )


def _verify_wheel_record(
    archive: zipfile.ZipFile, *, infos: Sequence[zipfile.ZipInfo]
) -> set[str]:
    names = [info.filename for info in infos]
    if len(names) != len(set(names)):
        raise ReleaseArtifactError("wheel contains duplicate ZIP member names")
    for info in infos:
        _safe_wheel_member(info)
        if info.is_dir():
            raise ReleaseArtifactError(
                f"wheel contains an unexpected directory entry: {info.filename}"
            )
    records = [name for name in names if name.endswith(".dist-info/RECORD")]
    if len(records) != 1:
        raise ReleaseArtifactError("wheel must contain exactly one dist-info/RECORD")
    record_member = records[0]
    try:
        rows = list(
            csv.reader(
                io.StringIO(
                    archive.read(record_member).decode("utf-8", errors="strict"),
                    newline="",
                ),
                strict=True,
            )
        )
    except (UnicodeDecodeError, csv.Error) as exc:
        raise ReleaseArtifactError(f"wheel RECORD is malformed: {exc}") from exc
    recorded: set[str] = set()
    for index, row in enumerate(rows):
        if len(row) != 3:
            raise ReleaseArtifactError(f"wheel RECORD row {index} is malformed")
        member, digest, size = row
        if member in recorded:
            raise ReleaseArtifactError(f"wheel RECORD duplicates member: {member}")
        recorded.add(member)
        if member not in names:
            raise ReleaseArtifactError(f"wheel RECORD names a missing member: {member}")
        if member == record_member:
            if digest or size:
                raise ReleaseArtifactError(
                    "wheel RECORD self-row must omit hash and size"
                )
            continue
        raw = archive.read(member)
        encoded = base64.urlsafe_b64encode(hashlib.sha256(raw).digest()).rstrip(b"=")
        expected_digest = "sha256=" + encoded.decode("ascii")
        if digest != expected_digest or size != str(len(raw)):
            raise ReleaseArtifactError(f"wheel RECORD differs for member: {member}")
    if recorded != set(names):
        missing = sorted(set(names) - recorded)
        raise ReleaseArtifactError(f"wheel has members absent from RECORD: {missing!r}")
    return set(names)


def package_inventory(wheel: Path) -> list[dict[str, Any]]:
    try:
        with zipfile.ZipFile(wheel) as archive:
            names = _verify_wheel_record(archive, infos=archive.infolist())
            manifest_members = sorted(
                name for name in names if PACKAGE_MANIFEST.fullmatch(name)
            )
            coordinates = {
                match.groups()
                for name in manifest_members
                if (match := PACKAGE_MANIFEST.fullmatch(name)) is not None
            }
            if coordinates != EXPECTED_PACKAGE_COORDINATES:
                raise ReleaseArtifactError(
                    "wheel chapter-package manifest coordinates are not exactly "
                    "vol1/ch01-09 and vol2/ch01-20"
                )
            packages: list[dict[str, Any]] = []
            for member in manifest_members:
                match = PACKAGE_MANIFEST.fullmatch(member)
                assert match is not None
                volume, chapter = match.groups()
                raw = archive.read(member)
                document = _decode_package_manifest(raw, member)
                assets = document.get("assets")
                if not isinstance(assets, list):
                    raise ReleaseArtifactError(f"{member}.assets must be a list")
                package_dir = PurePosixPath(member).parent
                normalized_assets: list[dict[str, Any]] = []
                seen_ids: set[str] = set()
                seen_members: set[str] = set()
                for index, asset in enumerate(assets):
                    label = f"{member}.assets[{index}]"
                    if not isinstance(asset, dict):
                        raise ReleaseArtifactError(f"{label} must be an object")
                    asset_id = asset.get("asset_id")
                    role = asset.get("role")
                    path = asset.get("path")
                    digest = asset.get("sha256")
                    if not all(
                        isinstance(item, str) and item
                        for item in (asset_id, role, path)
                    ):
                        raise ReleaseArtifactError(
                            f"{label} identity fields are invalid"
                        )
                    if asset_id in seen_ids:
                        raise ReleaseArtifactError(
                            f"duplicate asset_id in {member}: {asset_id}"
                        )
                    seen_ids.add(asset_id)
                    pure_path = PurePosixPath(path)
                    if (
                        pure_path.is_absolute()
                        or ".." in pure_path.parts
                        or pure_path.as_posix() != path
                    ):
                        raise ReleaseArtifactError(
                            f"unsafe package asset path: {path!r}"
                        )
                    asset_member = (package_dir / pure_path).as_posix()
                    if asset_member in seen_members:
                        raise ReleaseArtifactError(
                            f"duplicate asset member in {member}: {asset_member}"
                        )
                    seen_members.add(asset_member)
                    if asset_member not in names:
                        raise ReleaseArtifactError(
                            f"wheel asset is missing: {asset_member}"
                        )
                    actual = sha256_bytes(archive.read(asset_member))
                    if not isinstance(digest, str) or digest != actual:
                        raise ReleaseArtifactError(
                            f"wheel asset hash differs from {member}: {asset_id}"
                        )
                    normalized_assets.append(
                        {
                            "asset_id": asset_id,
                            "member": asset_member,
                            "role": role,
                            "sha256": actual,
                        }
                    )
                normalized_assets.sort(key=lambda item: item["asset_id"])
                package_id = document.get("package_id")
                version = document.get("version")
                status = document.get("status")
                release_status = document.get("release_status")
                if not all(
                    isinstance(item, str) and item
                    for item in (package_id, version, status, release_status)
                ):
                    raise ReleaseArtifactError(
                        f"{member} package identity is incomplete"
                    )
                expected_package_id = f"semantica.chapter_packages.{volume}.{chapter}"
                if package_id != expected_package_id:
                    raise ReleaseArtifactError(
                        f"{member} package_id differs from its physical coordinates"
                    )
                chapter_identity = document.get("chapter")
                if isinstance(chapter_identity, Mapping):
                    declared_volume = chapter_identity.get("volume")
                    declared_chapter = chapter_identity.get("chapter")
                else:
                    declared_volume = document.get("volume")
                    declared_chapter = chapter_identity
                if (declared_volume, declared_chapter) != (volume, chapter):
                    raise ReleaseArtifactError(
                        f"{member} chapter identity differs from its physical coordinates"
                    )
                physical_members = {
                    name for name in names if PurePosixPath(name).parent == package_dir
                }
                if physical_members != {member, *seen_members}:
                    raise ReleaseArtifactError(
                        f"{member} package directory contains undeclared or missing members"
                    )
                packages.append(
                    {
                        "assets": normalized_assets,
                        "chapter": chapter,
                        "manifest": {"member": member, "sha256": sha256_bytes(raw)},
                        "package_id": package_id,
                        "release_status": release_status,
                        "status": status,
                        "version": version,
                        "volume": volume,
                    }
                )
    except (OSError, KeyError, zipfile.BadZipFile) as exc:
        raise ReleaseArtifactError(
            f"Semantica wheel package inventory failed: {exc}"
        ) from exc
    packages.sort(key=lambda item: item["package_id"])
    identifiers = [item["package_id"] for item in packages]
    if len(packages) != 29 or len(set(identifiers)) != len(identifiers):
        raise ReleaseArtifactError(
            f"expected 29 unique two-book chapter packages in wheel; found {len(packages)}"
        )
    return packages


def _evidence_status(kind: str, document: Mapping[str, Any]) -> str:
    if kind == "book_source_bindings":
        passed = document.get("passed") is True and document.get("status") == "passed"
    elif kind in {"runtime_identity", "privacy"}:
        passed = document.get("ok") is True
    else:
        passed = document.get("passed") is True
    return "passed" if passed else "blocked"


def _require_same_document(
    actual: Mapping[str, Any], expected: Mapping[str, Any], *, label: str
) -> None:
    if canonical_bytes(actual) != canonical_bytes(expected):
        raise ReleaseArtifactError(f"{label} differs from a fresh local replay")


def _runtime_report(root: Path) -> Mapping[str, Any]:
    from runtime import doctor_runtime

    checks = doctor_runtime.run_checks(root / "runtime", include_venv=True)
    return {
        "checks": [asdict(item) for item in checks],
        "mode": "doctor",
        "ok": not any(item.level == "error" for item in checks),
    }


def _privacy_report(root: Path) -> Mapping[str, Any]:
    from scripts import check_public_privacy as privacy

    findings, _checked = privacy.run(root, tracked_only=False, include_ignored=False)
    return {
        "findings": [asdict(item) for item in findings],
        "ok": not findings,
        "scope": "tracked-and-unignored-worktree",
    }


def _pdf_font_report(path: Path) -> tuple[bool, int]:
    completed = subprocess.run(
        ["pdffonts", str(path)], check=False, capture_output=True, text=True
    )
    if completed.returncode != 0:
        raise ReleaseArtifactError(f"pdffonts could not inspect {path.name}")
    lines = completed.stdout.splitlines()
    if len(lines) < 3:
        return False, 0
    header_fields = lines[0].lower().split()
    if "emb" not in header_fields or "sub" not in header_fields:
        raise ReleaseArtifactError("pdffonts output lacks the emb column")
    rows = [line for line in lines[2:] if line.strip()]
    embedded_values: list[bool] = []
    for line in rows:
        fields = line.rsplit(maxsplit=5)
        if (
            len(fields) != 6
            or not fields[0].strip()
            or any(field.lower() not in {"yes", "no"} for field in fields[1:4])
        ):
            raise ReleaseArtifactError("pdffonts output contains a malformed font row")
        try:
            int(fields[4])
            int(fields[5])
        except ValueError as exc:
            raise ReleaseArtifactError(
                "pdffonts output contains a malformed font row"
            ) from exc
        embedded_values.append(fields[1].lower() == "yes")
    embedded = bool(embedded_values) and all(embedded_values)
    return embedded, len(rows)


def _pdf_text_characters(path: Path) -> int:
    completed = subprocess.run(
        ["pdftotext", str(path), "-"], check=False, capture_output=True
    )
    if completed.returncode != 0:
        raise ReleaseArtifactError(f"pdftotext could not inspect {path.name}")
    return len(completed.stdout.decode("utf-8", errors="replace").strip())


def _summary_count(raw: bytes, *, label: str) -> int:
    matches = list(TEST_SUMMARY.finditer(raw.decode("utf-8", errors="replace")))
    if not matches:
        raise ReleaseArtifactError(f"{label} has no pytest pass summary")
    return int(matches[-1].group("passed"))


def _run_regression(
    kind: str, *, root: Path, semantica_root: Path | None
) -> subprocess.CompletedProcess[str]:
    if kind == "semantica_regression":
        if semantica_root is None:
            raise ReleaseArtifactError(
                "Semantica source checkout is required to replay its regression"
            )
        cwd = semantica_root.resolve()
        environment = dict(os.environ)
        environment["PYTHONPATH"] = str(cwd)
        return subprocess.run(
            [
                sys.executable,
                "-m",
                "pytest",
                "-q",
                "tests/ontology",
                "tests/chapter_packages",
            ],
            cwd=cwd,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )
    if kind == "ontology_engineering_regression":
        return subprocess.run(
            [sys.executable, "-m", "pytest", "-q", "tests"],
            cwd=root,
            env=dict(os.environ),
            check=False,
            capture_output=True,
            text=True,
        )
    raise ReleaseArtifactError(f"unknown regression evidence kind: {kind}")


def _verify_technical_evidence(
    root: Path,
    *,
    kind: str,
    document: Mapping[str, Any],
    manifest: Mapping[str, Any],
    semantica_root: Path | None,
    replay_regressions: bool,
) -> None:
    """Replay the evidence meaning used by the deterministic candidate gate."""

    if kind == "authoring_locks":
        from scripts import update_book_authoring_locks as authoring

        expected = authoring.check_or_write(write=False, skill_root=root)
        _require_same_document(document, expected, label="authoring-lock evidence")
        return
    if kind == "book_source_bindings":
        from ontology_engineering import semantica_runtime

        expected = semantica_runtime.verify_book_source_bindings(root)
        _require_same_document(document, expected, label="book-source binding evidence")
        return
    if kind == "runtime_identity":
        expected = _runtime_report(root)
        _require_same_document(document, expected, label="runtime identity evidence")
        checks = expected["checks"]
        if expected["ok"] is True and not any(
            isinstance(item, dict)
            and item.get("code") == "installed_record"
            and item.get("level") == "ok"
            for item in checks
        ):
            raise ReleaseArtifactError("green runtime evidence lacks RECORD proof")
        return
    if kind == "privacy":
        expected = _privacy_report(root)
        _require_same_document(document, expected, label="privacy evidence")
        return
    if kind in {"ontology_engineering_regression", "semantica_regression"}:
        _require_exact_keys(
            document,
            {
                "$schema",
                "command",
                "commit",
                "log",
                "passed",
                "passed_count",
                "repository",
                "return_code",
                "summary",
            },
            f"{kind} evidence",
        )
        if document.get("$schema") != REGRESSION_SCHEMA:
            raise ReleaseArtifactError(f"{kind} evidence schema is not recognized")
        expected_commit = (
            manifest["ontology_engineering"]["source_commit"]
            if kind == "ontology_engineering_regression"
            else manifest["semantica"]["commit"]
        )
        log = verify_artifact_ref(root, document.get("log"), label=f"{kind}.log")
        if document["log"]["path"] != REGRESSION_LOG_PATHS[kind]:
            raise ReleaseArtifactError(f"{kind} uses the wrong fixed log path")
        stored_count = _summary_count(log.read_bytes(), label=f"{kind} log")
        if (
            document.get("commit") != expected_commit
            or document.get("command") != list(REGRESSION_COMMANDS[kind])
            or document.get("repository") != REGRESSION_REPOSITORIES[kind]
            or document.get("passed_count") != stored_count
            or document.get("passed") is not True
            or document.get("return_code") != 0
            or not isinstance(document.get("summary"), str)
            or not TEST_SUMMARY.fullmatch(document["summary"])
        ):
            raise ReleaseArtifactError(
                f"{kind} evidence is inconsistent or commit-unbound"
            )
        if replay_regressions:
            completed = _run_regression(kind, root=root, semantica_root=semantica_root)
            replay = (completed.stdout + "\n" + completed.stderr).encode("utf-8")
            if completed.returncode != 0:
                raise ReleaseArtifactError(f"{kind} failed during local replay")
            if _summary_count(replay, label=f"{kind} replay") != stored_count:
                raise ReleaseArtifactError(
                    f"{kind} replay test count differs from recorded evidence"
                )
        return
    if kind == "pdf_qa":
        _require_exact_keys(
            document, {"$schema", "books", "passed", "visual_review"}, "PDF QA"
        )
        if document.get("$schema") != "ontology-engineering.pdf-qa-evidence/v1":
            raise ReleaseArtifactError("PDF QA evidence schema is not recognized")
        visual = verify_artifact_ref(
            root, document.get("visual_review"), label="pdf_qa.visual_review"
        )
        visual_document = _load_json_bytes(
            visual.read_bytes(), label="pdf_qa.visual_review", canonical=True
        )
        if (
            visual_document.get("$schema")
            != "ontology-engineering.pdf-visual-review/v1"
        ):
            raise ReleaseArtifactError("PDF visual review schema is not recognized")
        _require_exact_keys(
            visual_document,
            {"$schema", "advisory_only", "books", "passed", "review_basis"},
            "PDF visual review",
        )
        if visual_document.get("advisory_only") is not True:
            raise ReleaseArtifactError(
                "PDF visual review must not claim publication authority"
            )
        visual_books = visual_document.get("books")
        if not isinstance(visual_books, list) or [
            item.get("volume") for item in visual_books if isinstance(item, dict)
        ] != ["vol1", "vol2"]:
            raise ReleaseArtifactError("PDF visual review must cover vol1 and vol2")
        books = document.get("books")
        manifest_books = {
            item["volume"]: item for item in manifest["books"] if isinstance(item, dict)
        }
        if not isinstance(books, list) or [
            item.get("volume") for item in books if isinstance(item, dict)
        ] != ["vol1", "vol2"]:
            raise ReleaseArtifactError("PDF QA must cover ordered vol1 and vol2")
        qa_items_passed = []
        for item in books:
            volume = item["volume"]
            _require_exact_keys(
                item,
                {
                    "font_count",
                    "fonts_embedded",
                    "page_count",
                    "pdf",
                    "text_characters",
                    "visual_passed",
                    "volume",
                },
                f"pdf_qa.{volume}",
            )
            reference = item.get("pdf")
            pdf_path = verify_artifact_ref(
                root, reference, label=f"pdf_qa.{volume}.pdf"
            )
            expected_pdf = manifest_books[volume]["pdf"]
            if not isinstance(reference, dict):
                raise ReleaseArtifactError(f"PDF QA reference is invalid for {volume}")
            identity_matches = not any(
                reference.get(key) != expected_pdf.get(key)
                for key in ("path", "sha256", "size_bytes")
            ) and item.get("page_count") == expected_pdf.get("page_count")
            if not identity_matches:
                raise ReleaseArtifactError(f"PDF QA identity differs for {volume}")
            embedded, font_count = _pdf_font_report(pdf_path)
            text_characters = _pdf_text_characters(pdf_path)
            visual_item = next(
                item
                for item in visual_books
                if isinstance(item, dict) and item.get("volume") == volume
            )
            _require_exact_keys(
                visual_item, {"pages_reviewed", "passed", "volume"}, f"visual.{volume}"
            )
            pages_reviewed = visual_item.get("pages_reviewed")
            if (
                not isinstance(pages_reviewed, list)
                or not pages_reviewed
                or any(
                    not isinstance(page, int)
                    or page < 1
                    or page > expected_pdf["page_count"]
                    for page in pages_reviewed
                )
                or pages_reviewed != sorted(set(pages_reviewed))
            ):
                raise ReleaseArtifactError(f"visual.{volume}.pages_reviewed is invalid")
            if (
                item.get("fonts_embedded") != embedded
                or item.get("font_count") != font_count
                or item.get("text_characters") != text_characters
                or item.get("visual_passed") != (visual_item.get("passed") is True)
            ):
                raise ReleaseArtifactError(f"PDF QA observations differ for {volume}")
            qa_items_passed.append(
                embedded
                and item.get("visual_passed") is True
                and text_characters > 1000
            )
        expected_passed = bool(
            visual_document.get("passed") is True and all(qa_items_passed)
        )
        if document.get("passed") != expected_passed:
            raise ReleaseArtifactError("PDF QA aggregate is inconsistent")


def _verify_governance_evidence(
    root: Path,
    *,
    kind: str,
    document: Mapping[str, Any],
    rights_evidence_sha256: str | None = None,
) -> None:
    common = {"$schema", "reason", "required_authority", "status"}
    if kind == "rights":
        _require_exact_keys(document, common | {"sources"}, "rights evidence")
        if document.get("$schema") != "ontology-engineering.rights-evidence/v1":
            raise ReleaseArtifactError("rights evidence schema is not recognized")
        sources = document.get("sources")
        if not isinstance(sources, list) or not sources:
            raise ReleaseArtifactError("rights evidence must bind at least one source")
        for index, source in enumerate(sources):
            verify_artifact_ref(root, source, label=f"rights evidence.sources[{index}]")
    else:
        _require_exact_keys(
            document,
            common | {"rights_evidence_sha256", "scope"},
            "publication approval evidence",
        )
        if document.get("$schema") != "ontology-engineering.publication-approval/v1":
            raise ReleaseArtifactError("publication approval schema is not recognized")
        if document.get("scope") != "two-book-artifact-publication":
            raise ReleaseArtifactError(
                "publication approval scope is not the two-book artifact"
            )
        if document.get("rights_evidence_sha256") != rights_evidence_sha256:
            raise ReleaseArtifactError(
                "publication approval does not bind the current rights evidence"
            )
    status = document.get("status")
    if status not in {"blocked", "pending"}:
        raise ReleaseArtifactError(f"{kind} evidence status is invalid")
    authority = document.get("required_authority")
    reason = document.get("reason")
    if not isinstance(authority, str) or not authority.strip():
        raise ReleaseArtifactError(f"{kind} required_authority is required")
    if not isinstance(reason, str) or not reason.strip():
        raise ReleaseArtifactError(f"{kind} evidence reason is required")


def _load_evidence(
    root: Path, kind: str, relative: str
) -> tuple[dict[str, Any], Mapping[str, Any]]:
    reference = artifact_ref(root, relative)
    path = root / relative
    document = _load_json_bytes(path.read_bytes(), label=relative, canonical=True)
    return reference, document


def _git_output(root: Path, args: Sequence[str], *, label: str) -> str:
    completed = subprocess.run(
        ["git", *args], cwd=root, capture_output=True, check=False, text=True
    )
    if completed.returncode != 0:
        raise ReleaseArtifactError(
            f"cannot inspect {label}: {completed.stderr.strip()}"
        )
    return completed.stdout.strip()


def _git_commit_exists(root: Path, commit: str) -> None:
    if not HEX40.fullmatch(commit):
        raise ReleaseArtifactError(
            "ontology_engineering.source_commit must be 40 lowercase hex digits"
        )
    completed = subprocess.run(
        ["git", "cat-file", "-e", f"{commit}^{{commit}}"],
        cwd=root,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ReleaseArtifactError(
            "ontology_engineering.source_commit is not present locally"
        )


def _allowed_artifact_path(relative: str) -> bool:
    fixed = {
        MANIFEST_PATH,
        f"{MANIFEST_PATH}.sha256",
        "references/release-evidence/pdf-visual-review.json",
        *EVIDENCE_PATHS.values(),
        *GOVERNANCE_PATHS.values(),
        *REGRESSION_LOG_PATHS.values(),
        *(spec["pdf"] for spec in BOOK_SPECS.values()),
    }
    return relative in fixed


def _git_source_boundary(root: Path, commit: str) -> str:
    """Bind source to one commit while allowing only fixed generated artifacts."""

    _git_commit_exists(root, commit)
    head = _git_output(root, ["rev-parse", "HEAD"], label="repository HEAD")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", commit, head],
        cwd=root,
        capture_output=True,
        check=False,
    )
    if ancestor.returncode != 0:
        raise ReleaseArtifactError(
            "ontology_engineering.source_commit is not an ancestor of current HEAD"
        )
    tracked = _git_output(
        root, ["diff", "--name-only", commit, "--"], label="source delta"
    ).splitlines()
    untracked = _git_output(
        root,
        ["ls-files", "--others", "--exclude-standard"],
        label="untracked source delta",
    ).splitlines()
    unexpected = sorted(
        path
        for path in {*tracked, *untracked}
        if path and not _allowed_artifact_path(path)
    )
    if unexpected:
        raise ReleaseArtifactError(
            "files outside the fixed generated-artifact set differ from the source "
            f"commit: {unexpected!r}"
        )
    tree = _git_output(root, ["rev-parse", f"{commit}^{{tree}}"], label="source tree")
    if not HEX40.fullmatch(tree):
        raise ReleaseArtifactError("ontology_engineering.source_tree is invalid")
    return tree


def _verify_semantica_checkout(root: Path, commit: str) -> None:
    if root is None or not root.is_dir():
        raise ReleaseArtifactError("Semantica source checkout is required")
    head = _git_output(root, ["rev-parse", "HEAD"], label="Semantica HEAD")
    if head != commit:
        raise ReleaseArtifactError(
            "Semantica checkout HEAD differs from the source lock"
        )
    status = _git_output(
        root,
        ["status", "--porcelain", "--untracked-files=all"],
        label="Semantica worktree",
    )
    if status:
        raise ReleaseArtifactError("Semantica regression checkout is not clean")


def _authoring_lock_paths(root: Path) -> dict[str, list[str]]:
    candidates = {
        "vol1": [
            "references/ontology-engineering-book/authoring-sources.sha256",
            "references/ontology-engineering-book/handbook/authoring-sources.sha256",
        ],
        "vol2": [
            "references/product-trustworthiness-book/handbook/current-source.sha256",
            "references/product-trustworthiness-book/handbook/authoring-assets.sha256",
            "references/product-trustworthiness-book/handbook/formal-search-guides.sha256",
        ],
    }
    result: dict[str, list[str]] = {}
    for volume, paths in candidates.items():
        selected = [relative for relative in paths if (root / relative).is_file()]
        if volume == "vol1":
            # The new book-root lock and the retired handbook-root lock must
            # never coexist as competing authorities.
            expected = "references/ontology-engineering-book/authoring-sources.sha256"
            if selected != [expected]:
                raise ReleaseArtifactError(
                    "Vol.1 must expose exactly one book-root authoring lock"
                )
        elif len(selected) != 3:
            raise ReleaseArtifactError(
                "Vol.2 source, asset, and formal-search guide locks are incomplete"
            )
        result[volume] = sorted(selected)
    return result


def compute_release_blockers(manifest: Mapping[str, Any]) -> list[str]:
    blockers: list[str] = []
    verification = manifest.get("verification", {})
    if isinstance(verification, dict):
        for kind in sorted(verification):
            item = verification[kind]
            if not isinstance(item, dict) or item.get("status") != "passed":
                blockers.append(f"verification.{kind}")
    else:
        blockers.append("verification.invalid")
    governance = manifest.get("governance", {})
    if isinstance(governance, dict):
        for kind in ("publication_approval", "rights"):
            # v1 has no trusted signature/identity mechanism.  Even a hostile
            # document that spells ``approved`` cannot remove these blockers.
            blockers.append(f"governance.{kind}")
    else:
        blockers.append("governance.invalid")
    packages = manifest.get("packages", [])
    if not isinstance(packages, list) or not packages:
        blockers.append("packages.missing")
    else:
        for package in packages:
            if not isinstance(package, dict):
                blockers.append("package.invalid")
                continue
            ready = (
                package.get("status") == "complete"
                and package.get("release_status") == "complete"
            )
            if not ready:
                blockers.append(f"package.{package.get('package_id', 'invalid')}")
    return sorted(set(blockers))


def create_manifest(
    root: Path,
    *,
    oe_source_commit: str,
    semantica_root: Path | None = None,
    claim_release: bool = False,
) -> dict[str, Any]:
    root = root.resolve()
    if claim_release:
        raise ReleaseArtifactError(
            "v1 cannot claim release; rights and publication require a trusted "
            "human approval mechanism that is intentionally not implemented"
        )
    source_tree = _git_source_boundary(root, oe_source_commit)
    source_lock_ref = artifact_ref(root, SOURCE_LOCK_PATH)
    source_lock = _load_json_bytes(
        (root / SOURCE_LOCK_PATH).read_bytes(), label=SOURCE_LOCK_PATH, canonical=False
    )
    if source_lock.get("$schema") != SOURCE_LOCK_SCHEMA:
        raise ReleaseArtifactError("Semantica source lock schema is not recognized")
    try:
        source = source_lock["source"]
        artifact = source_lock["artifact"]
        if not isinstance(source, Mapping) or not isinstance(artifact, Mapping):
            raise TypeError("source and artifact must be objects")
        wheel_relative = f"runtime/vendor/{artifact['filename']}"
        wheel_ref = artifact_ref(root, wheel_relative)
    except (KeyError, TypeError) as exc:
        raise ReleaseArtifactError("Semantica source lock is incomplete") from exc
    if wheel_ref["sha256"] != artifact.get("sha256"):
        raise ReleaseArtifactError(
            "vendored wheel differs from the Semantica source lock"
        )

    verification: dict[str, Any] = {}
    for kind, relative in sorted(EVIDENCE_PATHS.items()):
        reference, evidence = _load_evidence(root, kind, relative)
        verification[kind] = {
            "evidence": reference,
            "status": _evidence_status(kind, evidence),
        }

    governance: dict[str, Any] = {}
    rights_reference: dict[str, Any] | None = None
    for kind, relative in sorted(GOVERNANCE_PATHS.items()):
        reference, evidence = _load_evidence(root, kind, relative)
        if kind == "rights":
            _verify_governance_evidence(root, kind=kind, document=evidence)
            rights_reference = reference
        else:
            if rights_reference is None:
                # sorted order visits publication approval first; load and
                # validate the fixed rights evidence independently.
                rights_reference, rights_document = _load_evidence(
                    root, "rights", GOVERNANCE_PATHS["rights"]
                )
                _verify_governance_evidence(
                    root, kind="rights", document=rights_document
                )
            _verify_governance_evidence(
                root,
                kind=kind,
                document=evidence,
                rights_evidence_sha256=rights_reference["sha256"],
            )
        status = evidence.get("status")
        if status not in {"blocked", "pending"}:
            raise ReleaseArtifactError(f"{relative}.status is invalid")
        authority = evidence.get("required_authority")
        if not isinstance(authority, str) or not authority.strip():
            raise ReleaseArtifactError(f"{relative}.required_authority is required")
        governance[kind] = {
            "evidence": reference,
            "required_authority": authority,
            "status": status,
        }

    locks = _authoring_lock_paths(root)
    books: list[dict[str, Any]] = []
    for volume in ("vol1", "vol2"):
        spec = BOOK_SPECS[volume]
        pdf_ref = artifact_ref(root, spec["pdf"])
        books.append(
            {
                "authoring_locks": [artifact_ref(root, item) for item in locks[volume]],
                "pdf": {**pdf_ref, "page_count": _pdf_pages(root / spec["pdf"])},
                "title": spec["title"],
                "volume": volume,
            }
        )

    manifest: dict[str, Any] = {
        "$schema": SCHEMA,
        "artifact_status": "candidate",
        "books": books,
        "governance": governance,
        "ontology_engineering": {
            "repository": "https://github.com/jiaqiwang969/OntologyEngineering.git",
            "source_commit": oe_source_commit,
            "source_tree": source_tree,
        },
        "packages": package_inventory(root / wheel_relative),
        "release_verdict": {"blockers": [], "eligible": False},
        "semantica": {
            "commit": source.get("commit"),
            "repository": source.get("canonical_repository"),
            "source_lock": source_lock_ref,
            "version": source.get("version"),
            "wheel": wheel_ref,
        },
        "verification": verification,
    }
    if semantica_root is None:
        raise ReleaseArtifactError(
            "Semantica source checkout is required to create a replayed candidate"
        )
    _verify_semantica_checkout(semantica_root.resolve(), str(source.get("commit")))
    for kind, relative in sorted(EVIDENCE_PATHS.items()):
        _reference, evidence = _load_evidence(root, kind, relative)
        _verify_technical_evidence(
            root,
            kind=kind,
            document=evidence,
            manifest=manifest,
            semantica_root=semantica_root.resolve(),
            replay_regressions=True,
        )
    blockers = compute_release_blockers(manifest)
    eligible = not blockers
    manifest["artifact_status"] = "candidate"
    manifest["release_verdict"] = {"blockers": blockers, "eligible": eligible}
    return manifest


def _validate_package_shape(package: Any, *, label: str) -> None:
    if not isinstance(package, dict):
        raise ReleaseArtifactError(f"{label} must be an object")
    _require_exact_keys(
        package,
        {
            "assets",
            "chapter",
            "manifest",
            "package_id",
            "release_status",
            "status",
            "version",
            "volume",
        },
        label,
    )


def verify_manifest(
    root: Path,
    manifest_path: str = MANIFEST_PATH,
    *,
    semantica_root: Path | None = None,
    replay_regressions: bool = True,
) -> dict[str, Any]:
    root = root.resolve()
    path = _safe_file(root, manifest_path, label="manifest")
    raw = path.read_bytes()
    manifest = _load_json_bytes(raw, label=manifest_path, canonical=True)
    _require_exact_keys(
        manifest,
        {
            "$schema",
            "artifact_status",
            "books",
            "governance",
            "ontology_engineering",
            "packages",
            "release_verdict",
            "semantica",
            "verification",
        },
        "manifest",
    )
    if manifest["$schema"] != SCHEMA:
        raise ReleaseArtifactError("book release manifest schema is not recognized")
    if manifest["artifact_status"] != "candidate":
        raise ReleaseArtifactError(
            "v1 accepts candidate artifacts only; unsigned JSON cannot authorize release"
        )

    sidecar_relative = manifest_path + ".sha256"
    sidecar = _safe_file(root, sidecar_relative, label="manifest sidecar")
    expected_sidecar = f"{sha256_bytes(raw)}  {Path(manifest_path).name}\n"
    if sidecar.read_text(encoding="ascii") != expected_sidecar:
        raise ReleaseArtifactError("manifest SHA-256 sidecar is missing or stale")

    oe = manifest["ontology_engineering"]
    if not isinstance(oe, dict):
        raise ReleaseArtifactError("ontology_engineering must be an object")
    _require_exact_keys(
        oe,
        {"repository", "source_commit", "source_tree"},
        "ontology_engineering",
    )
    if oe["repository"] != "https://github.com/jiaqiwang969/OntologyEngineering.git":
        raise ReleaseArtifactError("ontology_engineering.repository is not canonical")
    source_tree = _git_source_boundary(root, oe["source_commit"])
    if oe["source_tree"] != source_tree:
        raise ReleaseArtifactError("ontology_engineering.source_tree differs")

    semantica = manifest["semantica"]
    if not isinstance(semantica, dict):
        raise ReleaseArtifactError("semantica must be an object")
    _require_exact_keys(
        semantica,
        {"commit", "repository", "source_lock", "version", "wheel"},
        "semantica",
    )
    source_lock_path = verify_artifact_ref(
        root, semantica["source_lock"], label="semantica.source_lock"
    )
    wheel_path = verify_artifact_ref(root, semantica["wheel"], label="semantica.wheel")
    source_lock = _load_json_bytes(
        source_lock_path.read_bytes(), label="semantica source lock", canonical=False
    )
    if source_lock.get("$schema") != SOURCE_LOCK_SCHEMA:
        raise ReleaseArtifactError("Semantica source lock schema is not recognized")
    try:
        source = source_lock["source"]
        artifact = source_lock["artifact"]
        if not isinstance(source, Mapping) or not isinstance(artifact, Mapping):
            raise TypeError("source and artifact must be objects")
    except (KeyError, TypeError) as exc:
        raise ReleaseArtifactError("Semantica source lock is incomplete") from exc
    expected_semantica = {
        "commit": source.get("commit"),
        "repository": source.get("canonical_repository"),
        "version": source.get("version"),
    }
    for key, expected in expected_semantica.items():
        if semantica[key] != expected:
            raise ReleaseArtifactError(f"semantica.{key} differs from source lock")
    if semantica["source_lock"]["path"] != SOURCE_LOCK_PATH:
        raise ReleaseArtifactError(
            "semantica.source_lock must use the fixed runtime lock"
        )
    expected_wheel_path = f"runtime/vendor/{artifact.get('filename')}"
    if semantica["wheel"]["path"] != expected_wheel_path:
        raise ReleaseArtifactError(
            "semantica.wheel must use the locked vendored artifact"
        )
    if semantica["wheel"]["sha256"] != artifact.get("sha256"):
        raise ReleaseArtifactError("Semantica wheel hash differs from source lock")
    if semantica_root is None:
        raise ReleaseArtifactError(
            "Semantica source checkout is required to verify regression provenance"
        )
    _verify_semantica_checkout(semantica_root.resolve(), str(semantica["commit"]))

    books = manifest["books"]
    if not isinstance(books, list) or [
        item.get("volume") for item in books if isinstance(item, dict)
    ] != ["vol1", "vol2"]:
        raise ReleaseArtifactError("books must contain ordered vol1 and vol2 records")
    expected_lock_paths = _authoring_lock_paths(root)
    for book in books:
        volume = book["volume"]
        label = f"books.{volume}"
        _require_exact_keys(book, {"authoring_locks", "pdf", "title", "volume"}, label)
        if book["title"] != BOOK_SPECS[volume]["title"]:
            raise ReleaseArtifactError(
                f"{label}.title differs from the fixed volume identity"
            )
        locks = book["authoring_locks"]
        if not isinstance(locks, list) or not locks:
            raise ReleaseArtifactError(f"{label}.authoring_locks is empty")
        lock_paths = [item.get("path") for item in locks if isinstance(item, dict)]
        if lock_paths != sorted(lock_paths) or len(lock_paths) != len(locks):
            raise ReleaseArtifactError(
                f"{label}.authoring_locks must be sorted artifacts"
            )
        if lock_paths != expected_lock_paths[volume]:
            raise ReleaseArtifactError(
                f"{label}.authoring_locks differs from the sole current lock set"
            )
        for index, lock in enumerate(locks):
            verify_artifact_ref(root, lock, label=f"{label}.authoring_locks[{index}]")
        pdf = book["pdf"]
        if not isinstance(pdf, dict):
            raise ReleaseArtifactError(f"{label}.pdf must be an object")
        _require_exact_keys(
            pdf, {"page_count", "path", "sha256", "size_bytes"}, f"{label}.pdf"
        )
        pdf_path = verify_artifact_ref(
            root,
            {key: pdf[key] for key in ("path", "sha256", "size_bytes")},
            label=f"{label}.pdf",
        )
        if pdf["path"] != BOOK_SPECS[volume]["pdf"] or pdf["page_count"] != _pdf_pages(
            pdf_path
        ):
            raise ReleaseArtifactError(f"{label}.pdf identity/page count differs")

    verification = manifest["verification"]
    if not isinstance(verification, dict) or set(verification) != set(EVIDENCE_PATHS):
        raise ReleaseArtifactError("verification evidence set is incomplete")
    for kind, item in verification.items():
        if not isinstance(item, dict):
            raise ReleaseArtifactError(f"verification.{kind} must be an object")
        _require_exact_keys(item, {"evidence", "status"}, f"verification.{kind}")
        if (
            not isinstance(item["evidence"], dict)
            or item["evidence"].get("path") != EVIDENCE_PATHS[kind]
        ):
            raise ReleaseArtifactError(
                f"verification.{kind} uses the wrong evidence path"
            )
        evidence_path = verify_artifact_ref(
            root, item["evidence"], label=f"verification.{kind}.evidence"
        )
        evidence = _load_json_bytes(
            evidence_path.read_bytes(),
            label=f"verification.{kind}.evidence",
            canonical=True,
        )
        _verify_technical_evidence(
            root,
            kind=kind,
            document=evidence,
            manifest=manifest,
            semantica_root=semantica_root.resolve(),
            replay_regressions=replay_regressions,
        )
        if item["status"] != _evidence_status(kind, evidence):
            raise ReleaseArtifactError(
                f"verification.{kind}.status differs from evidence"
            )

    governance = manifest["governance"]
    if not isinstance(governance, dict) or set(governance) != set(GOVERNANCE_PATHS):
        raise ReleaseArtifactError("governance evidence set is incomplete")
    governance_documents: dict[str, Mapping[str, Any]] = {}
    for kind, item in governance.items():
        if not isinstance(item, dict):
            raise ReleaseArtifactError(f"governance.{kind} must be an object")
        _require_exact_keys(
            item,
            {"evidence", "required_authority", "status"},
            f"governance.{kind}",
        )
        if (
            not isinstance(item["evidence"], dict)
            or item["evidence"].get("path") != GOVERNANCE_PATHS[kind]
        ):
            raise ReleaseArtifactError(
                f"governance.{kind} uses the wrong evidence path"
            )
        evidence_path = verify_artifact_ref(
            root, item["evidence"], label=f"governance.{kind}.evidence"
        )
        evidence = _load_json_bytes(
            evidence_path.read_bytes(),
            label=f"governance.{kind}.evidence",
            canonical=True,
        )
        governance_documents[kind] = evidence
        if item["status"] != evidence.get("status") or item[
            "required_authority"
        ] != evidence.get("required_authority"):
            raise ReleaseArtifactError(f"governance.{kind} differs from evidence")
    rights_reference = governance["rights"]["evidence"]
    _verify_governance_evidence(
        root, kind="rights", document=governance_documents["rights"]
    )
    _verify_governance_evidence(
        root,
        kind="publication_approval",
        document=governance_documents["publication_approval"],
        rights_evidence_sha256=rights_reference["sha256"],
    )

    packages = manifest["packages"]
    if not isinstance(packages, list):
        raise ReleaseArtifactError("packages must be a list")
    for index, package in enumerate(packages):
        _validate_package_shape(package, label=f"packages[{index}]")
    expected_inventory = package_inventory(wheel_path)
    expected_ids = [item["package_id"] for item in expected_inventory]
    actual_ids = [item.get("package_id") for item in packages]
    if actual_ids != expected_ids:
        raise ReleaseArtifactError(
            "packages must contain the exact sorted 29-package wheel inventory"
        )
    if packages != expected_inventory:
        raise ReleaseArtifactError(
            "package/asset inventory differs from the bound Semantica wheel"
        )

    blockers = compute_release_blockers(manifest)
    expected_verdict = {"blockers": blockers, "eligible": not blockers}
    if manifest["release_verdict"] != expected_verdict:
        raise ReleaseArtifactError(
            "release_verdict is not the deterministic gate result"
        )
    return {
        "artifact_status": manifest["artifact_status"],
        "manifest_sha256": sha256_bytes(raw),
        "package_count": len(packages),
        "passed": True,
        "release_eligible": not blockers,
        "release_blockers": blockers,
        "schema_version": SCHEMA,
    }


def _atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    except Exception:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def write_manifest(
    root: Path, manifest: Mapping[str, Any], relative: str = MANIFEST_PATH
) -> None:
    raw = canonical_bytes(manifest)
    path = root / relative
    _atomic_write(path, raw)
    sidecar = f"{sha256_bytes(raw)}  {path.name}\n".encode("ascii")
    _atomic_write(root / f"{relative}.sha256", sidecar)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=SKILL_ROOT)
    subparsers = parser.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser(
        "create", help="Create a byte-bound candidate manifest."
    )
    create.add_argument("--oe-source-commit", required=True)
    create.add_argument("--semantica-root", type=Path, required=True)
    create.add_argument(
        "--claim-release",
        action="store_true",
        help="Reserved fail-closed flag; v1 never machine-authorizes publication.",
    )
    verify = subparsers.add_parser(
        "verify", help="Verify the checked-in manifest and sidecar."
    )
    verify.add_argument("--manifest", default=MANIFEST_PATH)
    verify.add_argument("--semantica-root", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    root = args.root.expanduser().resolve()
    try:
        if args.command == "create":
            manifest = create_manifest(
                root,
                oe_source_commit=args.oe_source_commit,
                semantica_root=args.semantica_root.expanduser().resolve(),
                claim_release=args.claim_release,
            )
            write_manifest(root, manifest)
            report = verify_manifest(
                root, semantica_root=args.semantica_root.expanduser().resolve()
            )
        else:
            report = verify_manifest(
                root,
                args.manifest,
                semantica_root=args.semantica_root.expanduser().resolve(),
            )
    except (
        KeyError,
        OSError,
        ReleaseArtifactError,
        subprocess.SubprocessError,
        TypeError,
        UnicodeError,
        ValueError,
    ) as exc:
        report = {"passed": False, "error": str(exc), "schema_version": SCHEMA}
    print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if report.get("passed") else 1


if __name__ == "__main__":
    raise SystemExit(main())
