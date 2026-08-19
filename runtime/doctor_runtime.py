#!/usr/bin/env python3
"""Non-mutating preflight and installed-runtime checks for ontology-engineering."""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
from email.parser import Parser
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import zipfile


RUNTIME_DIR = Path(__file__).resolve().parent
LOCK_NAME = "semantica-source-lock.json"
LOCK_SCHEMA = "ontology-engineering.semantica-source-lock/v1"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")


@dataclass(frozen=True)
class Check:
    level: str
    code: str
    message: str


@dataclass(frozen=True)
class LockIdentity:
    commit: str
    version: str
    artifact_filename: str
    artifact_sha256: str


def _check(checks: list[Check], level: str, code: str, message: str) -> None:
    checks.append(Check(level=level, code=code, message=message))


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def inspect_lock(runtime_dir: Path, checks: list[Check]) -> LockIdentity | None:
    lock_path = runtime_dir / LOCK_NAME
    if lock_path.is_symlink() or not lock_path.is_file():
        _check(
            checks,
            "error",
            "lock_invalid",
            "source lock must be a regular non-symlink file",
        )
        return None
    try:
        document = json.loads(lock_path.read_text(encoding="utf-8"))
        source = document["source"]
        artifact = document["artifact"]
        identity = LockIdentity(
            commit=str(source["commit"]),
            version=str(source["version"]),
            artifact_filename=str(artifact["filename"]),
            artifact_sha256=str(artifact["sha256"]),
        )
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
        _check(checks, "error", "lock_invalid", f"source lock is unreadable: {exc}")
        return None

    if document.get("$schema") != LOCK_SCHEMA:
        _check(checks, "error", "lock_schema", "source lock schema is not recognized")
    if not HEX40.fullmatch(identity.commit):
        _check(
            checks, "error", "lock_commit", "source lock commit is not a 40-digit SHA"
        )
    if not identity.version:
        _check(checks, "error", "lock_version", "source lock version is empty")
    filename = identity.artifact_filename
    if (
        not filename.endswith(".whl")
        or Path(filename).name != filename
        or "/" in filename
        or "\\" in filename
    ):
        _check(
            checks,
            "error",
            "lock_artifact_name",
            "locked artifact must be a wheel basename without path traversal",
        )
    if not HEX64.fullmatch(identity.artifact_sha256):
        _check(checks, "error", "lock_artifact_hash", "locked wheel SHA-256 is invalid")

    if not any(item.level == "error" for item in checks):
        _check(
            checks,
            "ok",
            "lock_identity",
            f"Semantica {identity.version} is pinned to {identity.commit}",
        )
    return identity


def _requires_python_satisfied(specifier: str) -> bool | None:
    """Evaluate the simple version clauses used by the locked wheel without dependencies."""

    current = tuple(sys.version_info[:3])
    for raw_clause in specifier.split(","):
        clause = raw_clause.strip()
        match = re.fullmatch(r"(>=|>|<=|<|==)\s*(\d+(?:\.\d+){0,2})(?:\.\*)?", clause)
        if not match:
            return None
        operator, raw_version = match.groups()
        expected_parts = tuple(int(item) for item in raw_version.split("."))
        expected = expected_parts + (0,) * (3 - len(expected_parts))
        comparisons = {
            ">=": current >= expected,
            ">": current > expected,
            "<=": current <= expected,
            "<": current < expected,
            "==": current[: len(expected_parts)] == expected_parts,
        }
        if not comparisons[operator]:
            return False
    return True


def inspect_wheel(
    runtime_dir: Path, identity: LockIdentity, checks: list[Check]
) -> None:
    wheel_path = runtime_dir / "vendor" / identity.artifact_filename
    if wheel_path.is_symlink() or not wheel_path.is_file():
        _check(
            checks,
            "error",
            "wheel_missing",
            f"locked wheel must be a regular non-symlink file: {wheel_path}",
        )
        return

    try:
        digest = _sha256(wheel_path)
    except OSError as exc:
        _check(
            checks, "error", "wheel_unreadable", f"locked wheel cannot be read: {exc}"
        )
        return
    if digest != identity.artifact_sha256:
        _check(
            checks,
            "error",
            "wheel_hash",
            "vendored Semantica wheel SHA-256 differs from the source lock",
        )
        return

    try:
        with zipfile.ZipFile(wheel_path) as archive:
            metadata_names = [
                name
                for name in archive.namelist()
                if name.endswith(".dist-info/METADATA")
            ]
            wheel_names = [
                name for name in archive.namelist() if name.endswith(".dist-info/WHEEL")
            ]
            if len(metadata_names) != 1 or len(wheel_names) != 1:
                raise ValueError(
                    "wheel must contain exactly one METADATA and one WHEEL file"
                )
            metadata = Parser().parsestr(
                archive.read(metadata_names[0]).decode("utf-8", errors="strict")
            )
            wheel_metadata = Parser().parsestr(
                archive.read(wheel_names[0]).decode("utf-8", errors="strict")
            )
    except (OSError, UnicodeDecodeError, ValueError, zipfile.BadZipFile) as exc:
        _check(checks, "error", "wheel_invalid", f"vendored wheel is malformed: {exc}")
        return

    if metadata.get("Name", "").lower() != "semantica":
        _check(
            checks,
            "error",
            "wheel_name",
            "vendored wheel distribution is not Semantica",
        )
    if metadata.get("Version") != identity.version:
        _check(
            checks,
            "error",
            "wheel_version",
            "vendored wheel version differs from the source lock",
        )

    requires_python = metadata.get("Requires-Python")
    if requires_python:
        supported = _requires_python_satisfied(requires_python)
        if supported is False:
            _check(
                checks,
                "error",
                "python_version",
                f"Python {sys.version.split()[0]} does not satisfy {requires_python}",
            )
        elif supported is None:
            _check(
                checks,
                "warning",
                "python_specifier",
                f"could not evaluate wheel Requires-Python expression: {requires_python}",
            )
        else:
            _check(
                checks,
                "ok",
                "python_version",
                f"Python {sys.version.split()[0]} satisfies {requires_python}",
            )

    tags = wheel_metadata.get_all("Tag", [])
    if "py3-none-any" in tags:
        _check(checks, "ok", "wheel_platform", "locked wheel is portable py3-none-any")
    else:
        _check(
            checks,
            "warning",
            "wheel_platform",
            "wheel is platform-specific; this doctor does not claim full tag compatibility",
        )
    _check(checks, "ok", "wheel_hash", f"vendored wheel SHA-256 verified: {digest}")


def inspect_venv(
    runtime_dir: Path, identity: LockIdentity, checks: list[Check]
) -> None:
    venv = runtime_dir / ".venv"
    if not venv.exists():
        _check(
            checks,
            "warning",
            "venv_missing",
            "runtime venv is absent; run setup_runtime.sh",
        )
        return
    if not venv.is_dir():
        _check(
            checks,
            "error",
            "venv_invalid",
            "runtime .venv exists but is not a directory",
        )
        return

    python = venv / "bin" / "python"
    if not python.is_file() or not os.access(python, os.X_OK):
        _check(
            checks,
            "error",
            "venv_python",
            "runtime venv is incomplete: bin/python is missing",
        )
        return

    config = venv / "pyvenv.cfg"
    if not config.is_file():
        _check(checks, "error", "venv_config", "runtime venv has no pyvenv.cfg")
        return

    site_packages = sorted((venv / "lib").glob("python*/site-packages"))
    windows_site_packages = venv / "Lib" / "site-packages"
    if windows_site_packages.is_dir():
        site_packages.append(windows_site_packages)
    dist_infos = sorted(
        path
        for site_root in site_packages
        for path in site_root.glob("semantica-*.dist-info")
        if path.is_dir()
    )
    if len(dist_infos) != 1:
        _check(
            checks,
            "error",
            "installed_distribution",
            f"expected one installed Semantica dist-info directory, found {len(dist_infos)}",
        )
        return

    dist_info = dist_infos[0]
    try:
        metadata = Parser().parsestr(
            (dist_info / "METADATA").read_text(encoding="utf-8")
        )
        direct_url = json.loads(
            (dist_info / "direct_url.json").read_text(encoding="utf-8")
        )
    except (OSError, TypeError, json.JSONDecodeError) as exc:
        _check(
            checks,
            "error",
            "installed_metadata",
            f"installed Semantica provenance metadata is unreadable: {exc}",
        )
        return

    archive_info = direct_url.get("archive_info", {})
    installed_sha256 = archive_info.get("hashes", {}).get("sha256")
    if not installed_sha256:
        raw_hash = archive_info.get("hash", "")
        if raw_hash.startswith("sha256="):
            installed_sha256 = raw_hash.removeprefix("sha256=")

    if metadata.get("Version") != identity.version:
        _check(
            checks,
            "error",
            "installed_version",
            "installed Semantica version differs from the source lock",
        )
    if installed_sha256 != identity.artifact_sha256:
        _check(
            checks,
            "error",
            "installed_hash",
            "installed Semantica wheel provenance differs from the source lock",
        )
    if (
        metadata.get("Version") == identity.version
        and installed_sha256 == identity.artifact_sha256
    ):
        skill_root = runtime_dir.parent
        try:
            current_environment = Path(sys.prefix).resolve(strict=True)
            selected_environment = venv.resolve(strict=True)
        except OSError as exc:
            _check(
                checks,
                "error",
                "installed_record",
                "runtime environment identity cannot be resolved: {}".format(exc),
            )
            return
        if current_environment != selected_environment:
            _check(
                checks,
                "error",
                "installed_record",
                "doctor must run under the selected runtime/.venv interpreter",
            )
        else:
            if str(skill_root) not in sys.path:
                sys.path.insert(0, str(skill_root))
            try:
                from ontology_engineering.semantica_runtime import (
                    verify_runtime_source_identity,
                )

                selected = verify_runtime_source_identity()
                if selected.artifact_sha256 != identity.artifact_sha256:
                    raise RuntimeError(
                        "runtime identity proof differs from doctor source lock"
                    )
            except Exception as exc:
                _check(
                    checks,
                    "error",
                    "installed_record",
                    "imported Semantica path/RECORD verification failed: {}".format(
                        exc
                    ),
                )
            else:
                _check(
                    checks,
                    "ok",
                    "installed_record",
                    "import path and every wheel RECORD package file are verified",
                )
    if not any(
        item.level == "error"
        for item in checks
        if item.code.startswith("venv_") or item.code.startswith("installed_")
    ):
        _check(
            checks,
            "ok",
            "installed_runtime",
            f"installed Semantica {identity.version} matches the locked wheel",
        )


def run_checks(
    runtime_dir: Path = RUNTIME_DIR, *, include_venv: bool = True
) -> list[Check]:
    runtime_dir = runtime_dir.expanduser().resolve()
    checks: list[Check] = []
    identity = inspect_lock(runtime_dir, checks)
    if identity is not None and not any(item.level == "error" for item in checks):
        inspect_wheel(runtime_dir, identity, checks)
        if include_venv and not any(item.level == "error" for item in checks):
            inspect_venv(runtime_dir, identity, checks)
    _check(
        checks,
        "info",
        "offline_boundary",
        "the Semantica wheel is vendored, but first-time dependency installation may require a package index",
    )
    return checks


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check the pinned Semantica runtime without changing the environment."
    )
    parser.add_argument(
        "--preflight",
        action="store_true",
        help="Check the lock, wheel and current Python only; do not require an installed venv.",
    )
    parser.add_argument("--json", action="store_true", help="Emit a JSON report.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    checks = run_checks(include_venv=not args.preflight)
    ok = not any(item.level == "error" for item in checks)
    if args.json:
        print(
            json.dumps(
                {
                    "ok": ok,
                    "mode": "preflight" if args.preflight else "doctor",
                    "runtime_dir": RUNTIME_DIR.as_posix(),
                    "checks": [asdict(item) for item in checks],
                },
                ensure_ascii=False,
                indent=2,
            )
        )
    else:
        for item in checks:
            print(f"{item.level.upper():7} {item.code}: {item.message}")
        print("doctor: PASS" if ok else "doctor: FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
