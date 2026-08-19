"""The only executable-semantic boundary retained beside the two books.

The ontology-engineering repository is a source corpus, not a second ontology
implementation.  Every CQ, query, shape, case, rule, lifecycle operation and
release receipt is discovered and executed by Semantica's built-in packages.
There is deliberately no fallback backend and no book-local package loader.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
from typing import Any, Optional, Sequence, Tuple

from semantica.chapter_packages import (
    SemanticPackageRunner,
    chapter_asset_text as _chapter_asset_text,
    list_chapter_packages as _list_chapter_packages,
    list_domain_packages as _list_domain_packages,
    package_asset_text as _package_asset_text,
    read_migration_map as _read_migration_map,
    resolve_migration_successor as _resolve_migration_successor,
    validate_chapter_registry as _validate_chapter_registry,
    validate_domain_package as _validate_domain_package,
)
from semantica.ontology.runtime import SemanticRuntime


RUNTIME_ID = "semantica"
SKILL_ROOT = Path(__file__).resolve().parents[1]
SOURCE_LOCK_PATH = SKILL_ROOT / "runtime" / "semantica-source-lock.json"
_HEX40 = re.compile(r"^[0-9a-f]{40}$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")


@dataclass(frozen=True)
class RuntimeSourceLock:
    """Validated provenance coordinates for the locally built Semantica wheel."""

    commit: str
    version: str
    artifact_filename: str
    artifact_sha256: str

    def as_dict(self) -> dict[str, str]:
        return {
            "commit": self.commit,
            "version": self.version,
            "artifact_filename": self.artifact_filename,
            "artifact_sha256": self.artifact_sha256,
        }


def read_runtime_source_lock(*, verify_vendored_artifact: bool = False) -> RuntimeSourceLock:
    """Read the fail-closed source lock used for package execution receipts."""

    try:
        document = json.loads(SOURCE_LOCK_PATH.read_text(encoding="utf-8"))
        source = document["source"]
        artifact = document["artifact"]
        lock = RuntimeSourceLock(
            commit=str(source["commit"]),
            version=str(source["version"]),
            artifact_filename=str(artifact["filename"]),
            artifact_sha256=str(artifact["sha256"]),
        )
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
        raise RuntimeError("Semantica source lock is absent or malformed") from exc

    if not _HEX40.fullmatch(lock.commit):
        raise RuntimeError("Semantica source lock has an invalid commit")
    if not lock.version or not lock.artifact_filename.endswith(".whl"):
        raise RuntimeError("Semantica source lock has an invalid artifact identity")
    if not _HEX64.fullmatch(lock.artifact_sha256):
        raise RuntimeError("Semantica source lock has an invalid artifact digest")

    if verify_vendored_artifact:
        wheel = SKILL_ROOT / "runtime" / "vendor" / lock.artifact_filename
        try:
            digest = hashlib.sha256(wheel.read_bytes()).hexdigest()
        except OSError as exc:
            raise RuntimeError("the locked Semantica wheel is not vendored") from exc
        if digest != lock.artifact_sha256:
            raise RuntimeError("the vendored Semantica wheel differs from the source lock")
    return lock


def create_runtime(**options: Any) -> SemanticRuntime:
    """Create the one authorized, explicit Semantica runtime profile."""

    options.setdefault("profile", "ontology-runtime")
    return SemanticRuntime(**options)


def create_package_runner() -> SemanticPackageRunner:
    """Create Semantica's isolated built-in chapter-package runner."""

    return SemanticPackageRunner(create_runtime())


def run_package(package_id: str, scenario_id: Optional[str] = None) -> Any:
    """Execute one allowlisted built-in package with source-locked provenance."""

    lock = read_runtime_source_lock()
    return create_package_runner().run(
        package_id=package_id,
        scenario_id=scenario_id,
        runtime_commit=lock.commit,
        runtime_artifact_sha256=lock.artifact_sha256,
        runtime_version=lock.version,
    )


def list_chapter_packages() -> Tuple[Any, ...]:
    """List Semantica-owned chapter packages; OE maintains no shadow registry."""

    return tuple(_list_chapter_packages())


def validate_chapter_registry() -> Tuple[str, ...]:
    """Validate Semantica's authoritative 29-chapter registry."""

    return tuple(_validate_chapter_registry())


def list_domain_packages() -> Tuple[Any, ...]:
    """List Semantica-owned non-chapter domain packages."""

    return tuple(_list_domain_packages())


def validate_domain_packages() -> Tuple[str, ...]:
    """Validate every Semantica-owned domain package."""

    return tuple(
        "{}: {}".format(item.package_id, issue)
        for item in _list_domain_packages()
        for issue in _validate_domain_package(item.package_id)
    )


def read_migration_map(volume: str) -> Any:
    """Read Semantica's frozen migration/provenance ledger for one volume."""

    return _read_migration_map(volume)


def resolve_migration_successor(old_path: str, volume: Optional[str] = None) -> Any:
    """Resolve every exact Semantica successor without collapsing ambiguity."""

    return _resolve_migration_successor(old_path, volume)


def package_asset_text(package_id: str, asset_id: str) -> str:
    """Read one hash-verified text asset from an allowlisted built-in package."""

    return _package_asset_text(package_id, asset_id)


def chapter_asset_text(volume: str, chapter: str, asset_id: str) -> str:
    """Read one hash-verified Semantica package asset for book tooling."""

    return _chapter_asset_text(create_runtime(), volume, chapter, asset_id)


def governed_ontology_main(argv: Optional[Sequence[str]] = None) -> int:
    """Delegate the domain-ontology lifecycle CLI to Semantica."""

    from semantica.ontology.governance import main

    return main(argv)


def run_governance_acceptance_scenario() -> Any:
    """Run Semantica's built-in learn-without-forgetting acceptance case."""

    from semantica.ontology.governance import RuntimeSourceIdentityDTO
    from semantica.ontology.governance_scenario import (
        run_governance_acceptance_scenario as run_scenario,
    )

    lock = read_runtime_source_lock()
    return run_scenario(
        RuntimeSourceIdentityDTO(
            runtime_commit=lock.commit,
            runtime_artifact_sha256=lock.artifact_sha256,
            runtime_version=lock.version,
        )
    )


def normative_engraver_main(argv: Optional[Sequence[str]] = None) -> int:
    """Delegate controlled normative engraving to Semantica."""

    from semantica.chapter_packages.normative import main

    return main(argv)


__all__ = [
    "RUNTIME_ID",
    "RuntimeSourceLock",
    "SOURCE_LOCK_PATH",
    "chapter_asset_text",
    "create_package_runner",
    "create_runtime",
    "governed_ontology_main",
    "list_chapter_packages",
    "list_domain_packages",
    "normative_engraver_main",
    "package_asset_text",
    "read_migration_map",
    "read_runtime_source_lock",
    "resolve_migration_successor",
    "run_governance_acceptance_scenario",
    "run_package",
    "validate_chapter_registry",
    "validate_domain_packages",
]
