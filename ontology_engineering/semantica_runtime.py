"""The only executable-semantic boundary retained beside the two books.

The ontology-engineering repository is a source corpus, not a second ontology
implementation.  Every CQ, query, shape, case, rule, lifecycle operation and
release receipt is discovered and executed by Semantica's built-in packages.
There is deliberately no fallback backend and no book-local package loader.
"""

from __future__ import annotations

import base64
import csv
from dataclasses import dataclass
import hashlib
from importlib import metadata as importlib_metadata
import io
import json
from pathlib import Path
import re
from typing import Any, Mapping, Optional, Sequence, Tuple
import zipfile

import semantica as _semantica

from semantica.chapter_packages import (
    SemanticPackageRunner,
    chapter_asset_text as _chapter_asset_text,
    list_chapter_packages as _list_chapter_packages,
    list_domain_packages as _list_domain_packages,
    load_chapter_package as _load_chapter_package,
    load_domain_package as _load_domain_package,
    package_asset_text as _package_asset_text,
    read_migration_map as _read_migration_map,
    resolve_migration_successor as _resolve_migration_successor,
    validate_chapter_registry as _validate_chapter_registry,
    validate_domain_package as _validate_domain_package,
    verify_book_source_bindings as _verify_book_source_bindings,
)
from semantica.ontology.runtime import SemanticRuntime

try:
    # This exact public module is the sole native refinery boundary.  An absent
    # or broken module is reported as unavailable and always fails closed.
    from semantica.ontology import refinery as _refinery
except ImportError:
    _refinery = None


RUNTIME_ID = "semantica"
SOURCE_LOCK_SCHEMA = "ontology-engineering.semantica-source-lock/v1"
STAGING_RUNTIME_SCHEMA = "ontology-engineering.semantica-staging-runtime/v1"
NATIVE_REFINERY_CONTRACT = "semantica.ontology.refinery/v1"
NATIVE_REFINERY_STATES = (
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted",
)
NATIVE_REFINERY_ASSET_CATEGORIES = (
    "ontology",
    "competency_questions",
    "shapes",
    "queries",
    "rules",
    "cases",
    "contract",
    "provenance",
)
NATIVE_REFINERY_BOOK_IMPACTS = (
    "none",
    "vol1-method",
    "vol2-iso-exemplar",
    "both",
)
NATIVE_REFINERY_CASE_KINDS = (
    "positive",
    "negative",
    "ambiguity",
    "prior_release",
)
NATIVE_REFINERY_REGRESSION_CHECK_IDS = (
    "cq.prior",
    "cq.current",
    "case.positive",
    "case.negative",
    "case.ambiguity",
    "case.prior_release",
)
NATIVE_REFINERY_RELEASE_CHECK_IDS = (
    "package.coverage",
    "capability.coverage",
    "receipt.binding",
    "provenance.binding",
    "source.rights",
    "io.binding",
)
NATIVE_REFINERY_TRANSITION_CONTEXT_ACTIONS = (
    "candidate",
    "proposed",
    "committed",
    "execute_candidate",
    "derive_regression_gate",
    "regression_passed",
    "derive_release_gate",
    "release_complete",
    "promoted",
)
NATIVE_REFINERY_TRANSITION_CONTEXT_REQUIRED_OPERATIONS = (
    "propose_candidate",
    "commit_candidate",
    "execute_candidate",
    "derive_gate_evidence",
    "verify_candidate",
    "promote_candidate",
)
NATIVE_REFINERY_RUNNER_CONTRACT = "semantica.chapter_packages.SemanticPackageRunner/v1"
NATIVE_REFINERY_OPERATIONS = (
    "build_refinery_acceptance_delta",
    "open_engagement",
    "propose_candidate",
    "commit_candidate",
    "execute_candidate",
    "derive_gate_evidence",
    "verify_candidate",
    "promote_candidate",
    "history",
    "resolve_package",
    "execution_manifest",
    "run_registry",
)
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


@dataclass(frozen=True)
class StagingRuntimeDescriptor:
    """Strict, non-authoritative identity for one controlled staging wheel.

    A staging descriptor exists only to break the book/implementation build
    cycle.  It never updates or supersedes ``SOURCE_LOCK_PATH`` and is valid
    only while the exact sibling wheel and installed distribution both match.
    """

    commit: str
    version: str
    wheel_filename: str
    wheel_sha256: str
    descriptor_sha256: str

    def as_dict(self) -> dict[str, str]:
        return {
            "$schema": STAGING_RUNTIME_SCHEMA,
            "commit": self.commit,
            "version": self.version,
            "wheel_filename": self.wheel_filename,
            "wheel_sha256": self.wheel_sha256,
        }

    def as_runtime_source_lock(self) -> RuntimeSourceLock:
        return RuntimeSourceLock(
            commit=self.commit,
            version=self.version,
            artifact_filename=self.wheel_filename,
            artifact_sha256=self.wheel_sha256,
        )


def _strict_runtime_identity(
    *, commit: Any, version: Any, filename: Any, digest: Any, context: str
) -> tuple[str, str, str, str]:
    if not isinstance(commit, str) or not _HEX40.fullmatch(commit):
        raise RuntimeError("{} has an invalid commit".format(context))
    if not isinstance(version, str) or not version or version != version.strip():
        raise RuntimeError("{} has an invalid version".format(context))
    if (
        not isinstance(filename, str)
        or not filename.endswith(".whl")
        or filename != Path(filename).name
        or "/" in filename
        or "\\" in filename
    ):
        raise RuntimeError("{} has an invalid wheel filename".format(context))
    if not isinstance(digest, str) or not _HEX64.fullmatch(digest):
        raise RuntimeError("{} has an invalid wheel digest".format(context))
    return commit, version, filename, digest


def read_staging_runtime_descriptor(
    path: str | Path,
) -> StagingRuntimeDescriptor:
    """Parse one exact staging descriptor without consulting the formal lock."""

    source = Path(path).expanduser()
    if source.is_symlink() or not source.is_file():
        raise RuntimeError(
            "Semantica staging descriptor must be a regular non-symlink file"
        )
    try:
        payload = source.read_bytes()
        document = json.loads(payload.decode("utf-8"))
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as exc:
        raise RuntimeError(
            "Semantica staging descriptor is not valid UTF-8 JSON"
        ) from exc
    if not isinstance(document, Mapping):
        raise RuntimeError("Semantica staging descriptor root must be an object")
    required = {
        "$schema",
        "commit",
        "version",
        "wheel_filename",
        "wheel_sha256",
    }
    missing = sorted(required - set(document))
    unknown = sorted(set(document) - required)
    if missing:
        raise RuntimeError(
            "Semantica staging descriptor is missing: {}".format(", ".join(missing))
        )
    if unknown:
        raise RuntimeError(
            "Semantica staging descriptor has unsupported fields: {}".format(
                ", ".join(unknown)
            )
        )
    if document["$schema"] != STAGING_RUNTIME_SCHEMA:
        raise RuntimeError("Semantica staging descriptor has an unsupported schema")
    commit, version, filename, digest = _strict_runtime_identity(
        commit=document["commit"],
        version=document["version"],
        filename=document["wheel_filename"],
        digest=document["wheel_sha256"],
        context="Semantica staging descriptor",
    )
    return StagingRuntimeDescriptor(
        commit=commit,
        version=version,
        wheel_filename=filename,
        wheel_sha256=digest,
        descriptor_sha256=hashlib.sha256(payload).hexdigest(),
    )


def read_runtime_source_lock(
    *, verify_vendored_artifact: bool = False
) -> RuntimeSourceLock:
    """Read the fail-closed source lock used for package execution receipts."""

    if SOURCE_LOCK_PATH.is_symlink() or not SOURCE_LOCK_PATH.is_file():
        raise RuntimeError("Semantica source lock must be a regular non-symlink file")
    try:
        document = json.loads(SOURCE_LOCK_PATH.read_text(encoding="utf-8"))
        if not isinstance(document, Mapping):
            raise RuntimeError("Semantica source lock root must be an object")
        if document.get("$schema") != SOURCE_LOCK_SCHEMA:
            raise RuntimeError("Semantica source lock has an unsupported schema")
        source = document["source"]
        artifact = document["artifact"]
        commit, version, filename, digest = _strict_runtime_identity(
            commit=source["commit"],
            version=source["version"],
            filename=artifact["filename"],
            digest=artifact["sha256"],
            context="Semantica source lock",
        )
        lock = RuntimeSourceLock(
            commit=commit,
            version=version,
            artifact_filename=filename,
            artifact_sha256=digest,
        )
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
        raise RuntimeError("Semantica source lock is absent or malformed") from exc

    if verify_vendored_artifact:
        wheel = SKILL_ROOT / "runtime" / "vendor" / lock.artifact_filename
        if wheel.is_symlink() or not wheel.is_file():
            raise RuntimeError(
                "the locked Semantica wheel must be a regular non-symlink file"
            )
        try:
            digest = hashlib.sha256(wheel.read_bytes()).hexdigest()
        except OSError as exc:
            raise RuntimeError("the locked Semantica wheel is not vendored") from exc
        if digest != lock.artifact_sha256:
            raise RuntimeError(
                "the vendored Semantica wheel differs from the source lock"
            )
    return lock


def verify_runtime_source_identity(
    *, staging_descriptor: Optional[str | Path] = None
) -> RuntimeSourceLock:
    """Fail closed unless the selected wheel is exactly installed.

    With no argument, this verifies the formal source lock, the vendored wheel
    bytes, the imported Semantica version, and pip's PEP 610 archive hash.  A
    staging descriptor instead selects one strictly described sibling wheel;
    it performs the same artifact/installation checks without reading or
    writing the formal lock.  This is intentionally an explicit preflight so
    asset readers remain usable in the controlled staging build.
    """

    if staging_descriptor is None:
        lock = read_runtime_source_lock(verify_vendored_artifact=True)
        wheel = SKILL_ROOT / "runtime" / "vendor" / lock.artifact_filename
    else:
        descriptor_path = Path(staging_descriptor).expanduser()
        descriptor = read_staging_runtime_descriptor(descriptor_path)
        wheel = descriptor_path.parent / descriptor.wheel_filename
        if wheel.is_symlink() or not wheel.is_file():
            raise RuntimeError(
                "the Semantica staging wheel must be a regular sibling file"
            )
        try:
            actual = hashlib.sha256(wheel.read_bytes()).hexdigest()
        except OSError as exc:
            raise RuntimeError("the Semantica staging wheel is unreadable") from exc
        if actual != descriptor.wheel_sha256:
            raise RuntimeError(
                "the Semantica staging wheel differs from its descriptor"
            )
        lock = descriptor.as_runtime_source_lock()

    installed = installed_runtime_version()
    if installed != lock.version:
        raise RuntimeError(
            "installed Semantica version {!r} differs from selected identity {!r}".format(
                installed, lock.version
            )
        )
    installed_artifact = installed_runtime_artifact_sha256()
    if installed_artifact != lock.artifact_sha256:
        raise RuntimeError(
            "installed Semantica wheel differs from the selected artifact"
        )
    verify_installed_runtime_record(wheel, expected_version=lock.version)
    return lock


def verify_installed_runtime_record(wheel: str | Path, *, expected_version: str) -> int:
    """Bind the imported package to every hashed file in the selected wheel.

    The installed ``RECORD`` is not trusted: its authoritative rows are read
    from the already hash-verified formal or staging wheel.  This detects a
    ``PYTHONPATH`` shadow package, modified installed source/data, size drift,
    and unrecorded files injected below the imported ``semantica`` package.
    """

    artifact = Path(wheel).expanduser()
    if artifact.is_symlink() or not artifact.is_file():
        raise RuntimeError("selected Semantica wheel is not a regular file")
    try:
        distribution = importlib_metadata.distribution("semantica")
    except importlib_metadata.PackageNotFoundError as exc:
        raise RuntimeError("installed Semantica distribution is unavailable") from exc
    if str(distribution.version) != expected_version:
        raise RuntimeError(
            "installed Semantica distribution metadata differs from selected identity"
        )
    package_root = Path(distribution.locate_file("semantica"))
    imported_file_value = getattr(_semantica, "__file__", None)
    if not isinstance(imported_file_value, str) or not imported_file_value:
        raise RuntimeError("imported Semantica has no concrete package file")
    imported_file = Path(imported_file_value)
    if package_root.is_symlink() or imported_file.is_symlink():
        raise RuntimeError("installed or imported Semantica package is a symbolic link")
    try:
        package_root_resolved = package_root.resolve(strict=True)
        imported_resolved = imported_file.resolve(strict=True)
    except OSError as exc:
        raise RuntimeError("installed Semantica package cannot be resolved") from exc
    if imported_resolved != package_root_resolved / "__init__.py":
        raise RuntimeError(
            "imported Semantica is outside the selected installed distribution"
        )
    imported_paths = getattr(_semantica, "__path__", ())
    try:
        resolved_paths = tuple(
            Path(item).resolve(strict=True) for item in imported_paths
        )
    except (OSError, TypeError) as exc:
        raise RuntimeError("imported Semantica package path is invalid") from exc
    if resolved_paths != (package_root_resolved,):
        raise RuntimeError(
            "imported Semantica package path is shadowed or namespace-extended"
        )

    try:
        with zipfile.ZipFile(artifact, "r") as archive:
            record_names = [
                name
                for name in archive.namelist()
                if name.endswith(".dist-info/RECORD") and Path(name).name == "RECORD"
            ]
            if len(record_names) != 1:
                raise RuntimeError("selected Semantica wheel has no unique RECORD")
            record_text = archive.read(record_names[0]).decode("utf-8")
    except (OSError, UnicodeError, zipfile.BadZipFile, KeyError) as exc:
        raise RuntimeError("selected Semantica wheel RECORD is unreadable") from exc

    expected_files: dict[str, tuple[str, int]] = {}
    try:
        rows = csv.reader(io.StringIO(record_text))
        for row in rows:
            if len(row) != 3:
                raise RuntimeError("selected Semantica wheel RECORD row is malformed")
            relative, hash_field, size_field = row
            if not relative.startswith("semantica/"):
                continue
            relative_path = Path(relative)
            if (
                relative_path.is_absolute()
                or ".." in relative_path.parts
                or "\\" in relative
            ):
                raise RuntimeError(
                    "selected Semantica wheel RECORD contains an unsafe package path"
                )
            if not hash_field.startswith("sha256=") or not size_field.isdigit():
                raise RuntimeError(
                    "selected Semantica wheel package row lacks hash or size"
                )
            if relative in expected_files:
                raise RuntimeError(
                    "selected Semantica wheel RECORD duplicates a package path"
                )
            expected_files[relative] = (
                hash_field.removeprefix("sha256="),
                int(size_field),
            )
    except csv.Error as exc:
        raise RuntimeError("selected Semantica wheel RECORD is invalid CSV") from exc
    if "semantica/__init__.py" not in expected_files:
        raise RuntimeError("selected Semantica wheel RECORD has no package root")

    for relative, (expected_hash, expected_size) in expected_files.items():
        installed_path = Path(distribution.locate_file(relative))
        if installed_path.is_symlink() or not installed_path.is_file():
            raise RuntimeError("installed Semantica RECORD file is missing or symbolic")
        try:
            resolved = installed_path.resolve(strict=True)
            resolved.relative_to(package_root_resolved)
            payload = installed_path.read_bytes()
        except (OSError, ValueError) as exc:
            raise RuntimeError(
                "installed Semantica RECORD file escapes the package root"
            ) from exc
        actual_hash = (
            base64.urlsafe_b64encode(hashlib.sha256(payload).digest())
            .decode("ascii")
            .rstrip("=")
        )
        if len(payload) != expected_size or actual_hash != expected_hash:
            raise RuntimeError(
                "installed Semantica package file differs from wheel RECORD: {}".format(
                    relative
                )
            )

    installed_entries = tuple(package_root_resolved.rglob("*"))
    if any(path.is_symlink() for path in installed_entries):
        raise RuntimeError("installed Semantica package contains a symbolic link")
    actual_files = {
        path.relative_to(package_root_resolved).as_posix()
        for path in installed_entries
        if path.is_file()
        and "__pycache__" not in path.parts
        and path.suffix not in {".pyc", ".pyo"}
    }
    recorded_files = {
        Path(relative).relative_to("semantica").as_posix()
        for relative in expected_files
    }
    extras = sorted(actual_files - recorded_files)
    if extras:
        raise RuntimeError(
            "installed Semantica package contains unrecorded files: {}".format(
                ", ".join(extras[:5])
            )
        )
    return len(expected_files)


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


def run_and_verify_package(
    package_id: str, scenario_id: Optional[str] = None
) -> Tuple[Any, Any]:
    """Execute and re-verify one package in the same retained runner context."""

    lock = read_runtime_source_lock()
    runner = create_package_runner()
    result = runner.run(
        package_id=package_id,
        scenario_id=scenario_id,
        runtime_commit=lock.commit,
        runtime_artifact_sha256=lock.artifact_sha256,
        runtime_version=lock.version,
    )
    return result, runner.verify(result)


def installed_runtime_version() -> str:
    """Return the installed Semantica version without consulting a moving ref."""

    return str(getattr(_semantica, "__version__", ""))


def installed_runtime_artifact_sha256() -> str:
    """Read the installed wheel hash recorded by pip's PEP 610 metadata."""

    try:
        distribution = importlib_metadata.distribution("semantica")
        direct_url_text = distribution.read_text("direct_url.json")
        if direct_url_text is None:
            raise RuntimeError("installed Semantica has no direct_url.json")
        direct_url = json.loads(direct_url_text)
        archive = direct_url.get("archive_info", {})
        digest = archive.get("hashes", {}).get("sha256", "")
        if not digest:
            legacy = str(archive.get("hash", ""))
            if legacy.startswith("sha256="):
                digest = legacy.removeprefix("sha256=")
    except (ImportError, OSError, TypeError, ValueError, json.JSONDecodeError) as exc:
        raise RuntimeError(
            "installed Semantica wheel provenance is unavailable"
        ) from exc
    if not isinstance(digest, str) or not _HEX64.fullmatch(digest):
        raise RuntimeError("installed Semantica wheel provenance is invalid")
    return digest


def semantic_refinery_capabilities() -> Mapping[str, Any]:
    """Feature-detect Semantica's sole native refinery control-plane API.

    Detection is descriptive and fail closed.  A missing or drifting operation
    is never aliased to another lifecycle implementation.
    """

    module_operations = (
        "build_refinery_acceptance_delta",
        "open_engagement",
        "propose_candidate",
        "commit_candidate",
        "execute_candidate",
        "derive_gate_evidence",
        "verify_candidate",
        "promote_candidate",
        "history",
    )
    operation_presence = {
        name: _refinery is not None and callable(getattr(_refinery, name, None))
        for name in module_operations
    }
    registry_type = (
        getattr(_refinery, "IndustryOntologyRegistry", None)
        if _refinery is not None
        else None
    )
    operation_presence.update(
        {
            "resolve_package": callable(
                getattr(registry_type, "resolve_package", None)
            ),
            "execution_manifest": callable(
                getattr(registry_type, "execution_manifest", None)
            ),
            "run_registry": callable(
                getattr(SemanticPackageRunner, "run_registry", None)
            ),
        }
    )
    required_symbols = (
        "CandidateVerificationDTO",
        "EngagementPhaseDTO",
        "ExecutionReceiptReferenceDTO",
        "IndustryOntologyRegistry",
        "IndustryPackageDescriptorDTO",
        "LearningResultDTO",
        "PackageDelta",
        "ProjectOntologyBinding",
        "RefineryAuthorizationDTO",
        "RefineryGateEvidenceDTO",
        "RefineryStateDTO",
        "RuntimeSourceIdentityDTO",
        "SemanticEngagementReceipt",
        "SemanticTaskEnvelope",
        "SourceEvidenceDTO",
        "SubjectExecutionSuiteDTO",
        "TransitionContextDTO",
    )
    missing_symbols = tuple(
        name
        for name in required_symbols
        if _refinery is None or not callable(getattr(_refinery, name, None))
    )
    constant_mismatches = []
    if _refinery is None or getattr(_refinery, "REFINERY_CONTRACT", None) != (
        NATIVE_REFINERY_CONTRACT
    ):
        constant_mismatches.append("REFINERY_CONTRACT")
    if (
        _refinery is None
        or getattr(_refinery, "REFINERY_SCHEMA_VERSION", None) != "1.0"
    ):
        constant_mismatches.append("REFINERY_SCHEMA_VERSION")
    empty_package = (
        getattr(_refinery, "EMPTY_PACKAGE_SHA256", None)
        if _refinery is not None
        else None
    )
    if not isinstance(empty_package, str) or not _HEX64.fullmatch(empty_package):
        constant_mismatches.append("EMPTY_PACKAGE_SHA256")
    present = tuple(
        name for name in NATIVE_REFINERY_OPERATIONS if operation_presence[name]
    )
    missing = tuple(
        name for name in NATIVE_REFINERY_OPERATIONS if not operation_presence[name]
    )
    declaration: Mapping[str, Any] = {}
    declaration_error: Optional[str] = None
    if _refinery is not None and callable(
        getattr(_refinery, "refinery_capabilities", None)
    ):
        try:
            candidate = _refinery.refinery_capabilities()
            if isinstance(candidate, Mapping):
                declaration = candidate
            else:
                declaration_error = "capability declaration is not a mapping"
        except Exception as exc:
            declaration_error = "capability declaration failed: {}".format(exc)
    else:
        declaration_error = "refinery_capabilities is missing"
    contract_matches = declaration.get("contract") == NATIVE_REFINERY_CONTRACT
    expected_declaration = {
        "schema_version": "1.0",
        "states": NATIVE_REFINERY_STATES,
        "asset_categories": NATIVE_REFINERY_ASSET_CATEGORIES,
        "delta_categories": NATIVE_REFINERY_ASSET_CATEGORIES + ("book_impact",),
        "book_impacts": NATIVE_REFINERY_BOOK_IMPACTS,
        "case_kinds": NATIVE_REFINERY_CASE_KINDS,
        "regression_check_ids": NATIVE_REFINERY_REGRESSION_CHECK_IDS,
        "release_check_ids": NATIVE_REFINERY_RELEASE_CHECK_IDS,
        "transition_context_actions": NATIVE_REFINERY_TRANSITION_CONTEXT_ACTIONS,
        "transition_context_required_operations": (
            NATIVE_REFINERY_TRANSITION_CONTEXT_REQUIRED_OPERATIONS
        ),
        "runner_contract": NATIVE_REFINERY_RUNNER_CONTRACT,
        "operations": NATIVE_REFINERY_OPERATIONS,
        "publication_owned_externally": True,
    }
    declaration_mismatches = []
    for field, expected in expected_declaration.items():
        actual = declaration.get(field)
        if isinstance(expected, tuple):
            if not isinstance(actual, (list, tuple)) or tuple(actual) != expected:
                declaration_mismatches.append(field)
        elif actual != expected:
            declaration_mismatches.append(field)
    declaration_matches = contract_matches and not declaration_mismatches
    native_available = (
        _refinery is not None
        and not missing
        and not missing_symbols
        and not constant_mismatches
        and declaration_error is None
        and declaration_matches
    )
    return {
        "native": {
            "contract": NATIVE_REFINERY_CONTRACT,
            "available": native_available,
            "status": "available" if native_available else "blocked",
            "operations": list(present),
            "missing_operations": list(missing),
            "missing_symbols": list(missing_symbols),
            "constant_mismatches": constant_mismatches,
            "contract_matches": contract_matches,
            "declaration_matches": declaration_matches,
            "declaration_mismatches": declaration_mismatches,
            "declaration": dict(declaration),
            "declaration_error": declaration_error,
        },
    }


def _native_refinery_module() -> Any:
    capabilities = semantic_refinery_capabilities()["native"]
    if not capabilities["available"] or _refinery is None:
        raise RuntimeError(
            "native Semantica refinery API is unavailable or contract-incompatible"
        )
    return _refinery


def _native_source_identity() -> Any:
    module = _native_refinery_module()
    lock = read_runtime_source_lock()
    return module.RuntimeSourceIdentityDTO(
        runtime_commit=lock.commit,
        runtime_artifact_sha256=lock.artifact_sha256,
        runtime_version=lock.version,
    )


def native_refinery_empty_package_sha256() -> str:
    """Return the native empty-baseline digest from the frozen refinery API."""

    return str(_native_refinery_module().EMPTY_PACKAGE_SHA256)


def native_refinery_acceptance_delta(
    *,
    package_id: str,
    source_evidence: Mapping[str, Any],
    created_by: str,
    created_at: str,
    target_version: str = "1.0.0",
) -> Mapping[str, Any]:
    """Obtain Semantica-owned executable acceptance data for boundary tests.

    OE deliberately carries no RDF, query, shape, rule, case, or execution
    projection fixture.  The returned declared delta digest is removed because
    the OE adapter still has to replace the plain fact-authority ID with its
    content-bound native authority token before strong DTO parsing.
    """

    module = _native_refinery_module()
    delta = module.build_refinery_acceptance_delta(
        package_id=package_id,
        source_evidence=module.SourceEvidenceDTO.from_dict(source_evidence),
        created_by=created_by,
        created_at=created_at,
        target_version=target_version,
    )
    document = delta.as_dict()
    document.pop("delta_sha256", None)
    return document


def _native_task_and_binding(
    envelope: Mapping[str, Any], binding: Mapping[str, Any]
) -> Tuple[Any, Any]:
    module = _native_refinery_module()
    return (
        module.SemanticTaskEnvelope.from_dict(envelope),
        module.ProjectOntologyBinding.from_dict(binding),
    )


def _native_transition_context(
    module: Any,
    *,
    action: str,
    delta_sha256: str,
    envelope: Mapping[str, Any],
    binding: Any,
) -> Tuple[Any, Any]:
    """Create one exact current-invocation context through Semantica's DTOs.

    The public OE task may authorize several user-facing commands.  A native
    lifecycle write is intentionally narrower: its projected envelope must
    request only the single Semantica action being executed now.  Parsing and
    hashing stay inside Semantica so OE cannot invent a parallel context
    contract.
    """

    task = module.SemanticTaskEnvelope.from_dict(envelope)
    context = module.TransitionContextDTO.create(
        action=action,
        delta_sha256=delta_sha256,
        envelope=task,
        binding=binding,
    )
    return task, context


def _native_registry(
    workspace: str, binding: Any, *, allow_create: bool = False
) -> Any:
    module = _native_refinery_module()
    path = Path(workspace).expanduser()
    if path.is_symlink():
        raise RuntimeError("native refinery workspace must not be a symbolic link")
    if path.exists():
        if not path.is_dir():
            raise RuntimeError("native refinery workspace is not a directory")
        registry = module.IndustryOntologyRegistry(path)
        if registry.registry_id != binding.workspace_id:
            raise RuntimeError(
                "native refinery registry_id differs from the project binding"
            )
        return registry
    if not allow_create:
        raise RuntimeError(
            "native refinery workspace does not exist; only an authorised open may create it"
        )
    if (
        binding.baseline_version != "0"
        or binding.baseline_package_sha256 != module.EMPTY_PACKAGE_SHA256
    ):
        raise RuntimeError(
            "only an exact native empty baseline may create a refinery workspace"
        )
    registry = module.IndustryOntologyRegistry.create(
        path,
        registry_id=binding.workspace_id,
        created_at=binding.created_at,
    )
    if registry.registry_id != binding.workspace_id:
        raise RuntimeError(
            "native refinery registry_id differs from the project binding"
        )
    return registry


def _native_receipt(
    module: Any,
    envelope: Any,
    binding: Any,
    value: Optional[Mapping[str, Any]],
) -> Any:
    if value is not None and "receipt_sha256" in value:
        receipt = module.SemanticEngagementReceipt.from_dict(value)
    elif value is not None:
        required = {
            "engagement_id",
            "execution",
            "regression",
            "receipt",
            "release",
            "learning",
            "execution_receipts",
            "created_at",
        }
        missing = sorted(required - set(value))
        unknown = sorted(set(value) - required)
        if missing:
            raise RuntimeError(
                "native engagement evidence is missing: {}".format(", ".join(missing))
            )
        if unknown:
            raise RuntimeError(
                "native engagement evidence has unsupported fields: {}".format(
                    ", ".join(unknown)
                )
            )
        phase_required = {
            "name",
            "status",
            "required_capabilities",
            "observed_capabilities",
            "evidence_sha256",
            "details",
        }
        for phase_name in ("execution", "regression", "receipt", "release"):
            phase = value[phase_name]
            if not isinstance(phase, Mapping):
                raise RuntimeError(
                    "native engagement {} must be a mapping".format(phase_name)
                )
            phase_missing = sorted(phase_required - set(phase))
            phase_unknown = sorted(set(phase) - phase_required)
            if phase_missing or phase_unknown:
                raise RuntimeError(
                    "native engagement {} fields mismatch; missing={} unknown={}".format(
                        phase_name,
                        ",".join(phase_missing) or "none",
                        ",".join(phase_unknown) or "none",
                    )
                )
        learning = value["learning"]
        if not isinstance(learning, Mapping):
            raise RuntimeError("native engagement learning must be a mapping")
        learning_allowed = {"status", "rationale", "delta_sha256"}
        learning_missing = sorted({"status", "rationale"} - set(learning))
        learning_unknown = sorted(set(learning) - learning_allowed)
        if learning_missing or learning_unknown:
            raise RuntimeError(
                "native engagement learning fields mismatch; missing={} unknown={}".format(
                    ",".join(learning_missing) or "none",
                    ",".join(learning_unknown) or "none",
                )
            )
        references = value["execution_receipts"]
        if not isinstance(references, list):
            raise RuntimeError("native execution_receipts must be a list")
        reference_fields = {
            "receipt_sha256",
            "package_id",
            "package_version",
            "package_digest",
        }
        for reference in references:
            if not isinstance(reference, Mapping) or set(reference) != reference_fields:
                raise RuntimeError(
                    "native execution receipt reference has non-exact fields"
                )
        receipt = module.SemanticEngagementReceipt.create(
            engagement_id=str(value["engagement_id"]),
            envelope=envelope,
            binding=binding,
            runtime_source=_native_source_identity(),
            execution=module.EngagementPhaseDTO.from_dict(value["execution"]),
            regression=module.EngagementPhaseDTO.from_dict(value["regression"]),
            receipt=module.EngagementPhaseDTO.from_dict(value["receipt"]),
            release=module.EngagementPhaseDTO.from_dict(value["release"]),
            learning=module.LearningResultDTO.from_dict(value["learning"]),
            execution_receipts=tuple(
                module.ExecutionReceiptReferenceDTO.from_dict(item)
                for item in value["execution_receipts"]
            ),
            created_at=str(value["created_at"]),
        )
    else:
        required_capabilities = tuple(envelope.required_capabilities)
        receipt = module.SemanticEngagementReceipt.create(
            engagement_id="engagement:{}".format(envelope.task_id),
            envelope=envelope,
            binding=binding,
            runtime_source=_native_source_identity(),
            execution=module.EngagementPhaseDTO.evaluate(
                "execution",
                required_capabilities=required_capabilities,
                observed_capabilities=(),
                evidence_sha256=envelope.envelope_sha256,
                details={
                    "reason": "engagement opened; no execution evidence submitted"
                },
                execution_status="blocked",
            ),
            regression=module.EngagementPhaseDTO.evaluate(
                "regression",
                required_capabilities=required_capabilities,
                observed_capabilities=(),
                evidence_sha256=None,
                details={"reason": "regression has not run"},
                execution_status="blocked",
            ),
            receipt=module.EngagementPhaseDTO.evaluate(
                "receipt",
                required_capabilities=required_capabilities,
                observed_capabilities=(),
                evidence_sha256=None,
                details={"reason": "execution receipt has not been submitted"},
                execution_status="blocked",
            ),
            release=module.EngagementPhaseDTO.evaluate(
                "release",
                required_capabilities=required_capabilities,
                observed_capabilities=(),
                evidence_sha256=None,
                details={"reason": "release verification has not run"},
                execution_status="blocked",
            ),
            learning=module.LearningResultDTO(
                status="no_delta",
                rationale="opening a task does not assert reusable industry knowledge",
            ),
            execution_receipts=(),
            created_at=envelope.created_at,
        )
    lock = read_runtime_source_lock()
    source = receipt.runtime_source
    if (
        source.runtime_commit != lock.commit
        or source.runtime_artifact_sha256 != lock.artifact_sha256
        or source.runtime_version != lock.version
    ):
        raise RuntimeError(
            "native engagement receipt runtime identity differs from the OE source lock"
        )
    return receipt


def native_refinery_open_engagement(
    workspace: str,
    *,
    envelope: Mapping[str, Any],
    binding: Mapping[str, Any],
    engagement: Optional[Mapping[str, Any]] = None,
) -> Mapping[str, Any]:
    """Open and record a native engagement through exact strong DTOs."""

    module = _native_refinery_module()
    task, project_binding = _native_task_and_binding(envelope, binding)
    receipt = _native_receipt(module, task, project_binding, engagement)
    registry = _native_registry(workspace, project_binding, allow_create=True)
    return module.open_engagement(
        registry,
        envelope=task,
        binding=project_binding,
        receipt=receipt,
    ).as_dict()


def native_refinery_propose_candidate(
    workspace: str,
    *,
    delta: Mapping[str, Any],
    envelope: Mapping[str, Any],
    proposed_envelope: Mapping[str, Any],
    binding: Mapping[str, Any],
    engagement: Mapping[str, Any],
    recorded_at: Optional[str] = None,
) -> Mapping[str, Any]:
    """Submit a complete native package delta and advance it to proposed."""

    module = _native_refinery_module()
    task, project_binding = _native_task_and_binding(envelope, binding)
    package_delta = module.PackageDelta.from_dict(delta)
    _, candidate_context = _native_transition_context(
        module,
        action="candidate",
        delta_sha256=package_delta.delta_sha256,
        envelope=envelope,
        binding=project_binding,
    )
    _, proposed_context = _native_transition_context(
        module,
        action="proposed",
        delta_sha256=package_delta.delta_sha256,
        envelope=proposed_envelope,
        binding=project_binding,
    )
    engagement_value: Mapping[str, Any] = engagement
    if "receipt_sha256" not in engagement:
        learning = engagement.get("learning")
        if isinstance(learning, Mapping) and learning.get("status") == "candidate":
            declared = learning.get("delta_sha256")
            if declared not in {None, package_delta.delta_sha256}:
                raise RuntimeError(
                    "engagement learning delta differs from the native package delta"
                )
            learning_value = dict(learning)
            learning_value["delta_sha256"] = package_delta.delta_sha256
            engagement_copy = dict(engagement)
            engagement_copy["learning"] = learning_value
            engagement_value = engagement_copy
    receipt = _native_receipt(module, task, project_binding, engagement_value)
    registry = _native_registry(workspace, project_binding)
    state = module.propose_candidate(
        registry,
        delta=package_delta,
        envelope=task,
        binding=project_binding,
        engagement=receipt,
        context=proposed_context,
        recorded_at=recorded_at,
    )
    return {
        "state": state.as_dict(),
        "delta": package_delta.as_dict(),
        "engagement": receipt.as_dict(),
        "transition_contexts": {
            "candidate": candidate_context.as_dict(),
            "proposed": proposed_context.as_dict(),
        },
    }


def native_refinery_commit_candidate(
    workspace: str,
    *,
    delta_sha256: str,
    authorization: Mapping[str, Any],
    envelope: Mapping[str, Any],
    binding: Mapping[str, Any],
    recorded_at: Optional[str] = None,
) -> Mapping[str, Any]:
    """Commit one exact proposed native candidate with typed authorization."""

    module = _native_refinery_module()
    project_binding = module.ProjectOntologyBinding.from_dict(binding)
    registry = _native_registry(workspace, project_binding)
    _, context = _native_transition_context(
        module,
        action="committed",
        delta_sha256=delta_sha256,
        envelope=envelope,
        binding=project_binding,
    )
    result = module.commit_candidate(
        registry,
        delta_sha256=delta_sha256,
        authorization=module.RefineryAuthorizationDTO.from_dict(authorization),
        context=context,
        recorded_at=recorded_at,
    )
    return {
        "state": result.as_dict(),
        "transition_context": context.as_dict(),
    }


def native_refinery_verify_candidate(
    workspace: str,
    *,
    delta_sha256: str,
    envelopes: Mapping[str, Mapping[str, Any]],
    binding: Mapping[str, Any],
    recorded_at: Optional[str] = None,
) -> Mapping[str, Any]:
    """Execute the subject and derive both fixed gates inside Semantica.

    OE intentionally accepts no caller-authored pass/fail evidence.  The exact
    committed subject is run by Semantica, its suite is retained in the native
    CAS.  Regression is derived and recorded first; only then may Semantica
    derive release from the same suite plus the recorded regression closure.
    """

    module = _native_refinery_module()
    expected_actions = (
        "execute_candidate",
        "derive_regression_gate",
        "regression_passed",
        "derive_release_gate",
        "release_complete",
    )
    missing = sorted(set(expected_actions) - set(envelopes))
    unknown = sorted(set(envelopes) - set(expected_actions))
    if missing or unknown:
        raise RuntimeError(
            "native verification transition envelopes mismatch; missing={} unknown={}".format(
                ",".join(missing) or "none",
                ",".join(unknown) or "none",
            )
        )
    project_binding = module.ProjectOntologyBinding.from_dict(binding)
    registry = _native_registry(workspace, project_binding)
    current = registry.status(delta_sha256)
    suite = None
    if current.state == "committed":
        active_actions = expected_actions
        contexts = {
            action: _native_transition_context(
                module,
                action=action,
                delta_sha256=delta_sha256,
                envelope=envelopes[action],
                binding=project_binding,
            )[1]
            for action in active_actions
        }
        source = _native_source_identity()
        suite = module.execute_candidate(
            registry,
            delta_sha256=delta_sha256,
            context=contexts["execute_candidate"],
            runtime_source=source,
            created_at=recorded_at,
        )
        execution_suite_sha256 = suite.suite_sha256
        regression = module.derive_gate_evidence(
            registry,
            delta_sha256=delta_sha256,
            context=contexts["derive_regression_gate"],
            gate="regression",
            execution_suite_sha256=execution_suite_sha256,
            recorded_at=recorded_at,
        )
        regression_context = contexts["regression_passed"]
    elif current.state in {"regression_passed", "release_complete"}:
        active_actions = ("derive_release_gate", "release_complete")
        contexts = {
            action: _native_transition_context(
                module,
                action=action,
                delta_sha256=delta_sha256,
                envelope=envelopes[action],
                binding=project_binding,
            )[1]
            for action in active_actions
        }
        lifecycle = module.history(registry, delta_sha256=delta_sha256)
        regression_event = next(
            (event for event in lifecycle if event.get("state") == "regression_passed"),
            None,
        )
        if not isinstance(regression_event, Mapping):
            raise RuntimeError(
                "native regression_passed state has no verified regression event"
            )
        payload = regression_event.get("payload")
        if not isinstance(payload, Mapping):
            raise RuntimeError("native regression event payload is invalid")
        execution_suite_sha256 = payload.get("execution_suite_sha256")
        if not isinstance(execution_suite_sha256, str) or not _HEX64.fullmatch(
            execution_suite_sha256
        ):
            raise RuntimeError(
                "native regression event has no exact execution suite identity"
            )
        regression = None
        regression_context = None
    else:
        raise RuntimeError(
            "native verification requires committed, regression_passed, or release_complete"
        )
    verification = module.verify_candidate(
        registry,
        delta_sha256=delta_sha256,
        execution_suite_sha256=execution_suite_sha256,
        regression_evidence=regression,
        regression_context=regression_context,
        release_derivation_context=contexts["derive_release_gate"],
        release_context=contexts["release_complete"],
        recorded_at=recorded_at,
    )
    return {
        "state": verification.state.as_dict(),
        "execution_suite": suite.as_dict() if suite is not None else None,
        "execution_suite_sha256": verification.execution_suite_sha256,
        "regression_evidence": verification.regression_evidence.as_dict(),
        "release_evidence": verification.release_evidence.as_dict(),
        "native_verification": verification.as_dict(),
        "transition_contexts": {
            action: contexts[action].as_dict() for action in active_actions
        },
    }


def _native_registry_subject(
    registry: Any, project_binding: Any
) -> Tuple[Any, Mapping[str, Any]]:
    """Resolve and bind the current promoted subject, never executor identity."""

    descriptor = registry.resolve_package(project_binding.package_id)
    if descriptor.package_id != project_binding.package_id:
        raise RuntimeError(
            "native registry subject package_id differs from the project binding"
        )
    if descriptor.version != project_binding.baseline_version:
        raise RuntimeError(
            "native registry subject version differs from the project binding"
        )
    if descriptor.package_sha256 != project_binding.baseline_package_sha256:
        raise RuntimeError(
            "native registry subject digest differs from the project binding"
        )
    manifest = registry.execution_manifest(
        descriptor.package_id, version=descriptor.version
    )
    assets = manifest.get("assets") if isinstance(manifest, Mapping) else None
    if not isinstance(assets, list):
        raise RuntimeError("native registry execution manifest has invalid assets")
    contracts = [
        item
        for item in assets
        if isinstance(item, Mapping) and item.get("role") == "contract"
    ]
    if len(contracts) != 1:
        raise RuntimeError(
            "native registry execution manifest must have one contract projection"
        )
    projection_sha256 = contracts[0].get("sha256")
    if not isinstance(projection_sha256, str) or not _HEX64.fullmatch(
        projection_sha256
    ):
        raise RuntimeError(
            "native registry execution projection has an invalid identity"
        )
    subject = {
        "package_id": descriptor.package_id,
        "version": descriptor.version,
        "package_sha256": descriptor.package_sha256,
        "manifest_sha256": descriptor.manifest_sha256,
        "execution_projection_sha256": projection_sha256,
        "promotion_record_sha256": descriptor.promotion_record_sha256,
    }
    return descriptor, subject


def native_refinery_discover_package(
    workspace: str, *, binding: Mapping[str, Any]
) -> Mapping[str, Any]:
    """Resolve the current promoted subject through its bound native registry."""

    module = _native_refinery_module()
    project_binding = module.ProjectOntologyBinding.from_dict(binding)
    registry = _native_registry(workspace, project_binding)
    _, subject = _native_registry_subject(registry, project_binding)
    return subject


def native_refinery_run_package(
    workspace: str,
    *,
    binding: Mapping[str, Any],
    scenario_id: Optional[str] = None,
    created_at: Optional[str] = None,
) -> Mapping[str, Any]:
    """Run a promoted registry subject while preserving executor separation."""

    module = _native_refinery_module()
    project_binding = module.ProjectOntologyBinding.from_dict(binding)
    registry = _native_registry(workspace, project_binding)
    descriptor, subject = _native_registry_subject(registry, project_binding)
    lock = read_runtime_source_lock()
    runner = create_package_runner()
    result = runner.run_registry(
        registry,
        descriptor.package_id,
        scenario_id,
        version=descriptor.version,
        runtime_commit=lock.commit,
        runtime_artifact_sha256=lock.artifact_sha256,
        runtime_version=lock.version,
        created_at=created_at,
    )
    verdict = runner.verify(result, checked_at=created_at)
    return {
        "subject": dict(subject),
        "executor": result.as_dict(),
        "release": verdict.as_dict(),
    }


def native_refinery_promote_candidate(
    workspace: str,
    *,
    delta_sha256: str,
    authorization: Mapping[str, Any],
    envelope: Mapping[str, Any],
    binding: Mapping[str, Any],
    recorded_at: Optional[str] = None,
) -> Mapping[str, Any]:
    """Promote through native Semantica without claiming external publication."""

    module = _native_refinery_module()
    project_binding = module.ProjectOntologyBinding.from_dict(binding)
    registry = _native_registry(workspace, project_binding)
    _, context = _native_transition_context(
        module,
        action="promoted",
        delta_sha256=delta_sha256,
        envelope=envelope,
        binding=project_binding,
    )
    result = module.promote_candidate(
        registry,
        delta_sha256=delta_sha256,
        authorization=module.RefineryAuthorizationDTO.from_dict(authorization),
        context=context,
        recorded_at=recorded_at,
    )
    return {
        "descriptor": result.as_dict(),
        "transition_context": context.as_dict(),
    }


def native_refinery_history(
    workspace: str, *, delta_sha256: str, binding: Mapping[str, Any]
) -> Tuple[Mapping[str, Any], ...]:
    """Return the native refinery's verified immutable candidate history."""

    module = _native_refinery_module()
    project_binding = module.ProjectOntologyBinding.from_dict(binding)
    registry = _native_registry(workspace, project_binding)
    events = tuple(module.history(registry, delta_sha256=delta_sha256))
    if not events or any(
        event.get("package_id") != project_binding.package_id for event in events
    ):
        raise RuntimeError(
            "native candidate history package_id differs from the project binding"
        )
    return events


def list_chapter_packages() -> Tuple[Any, ...]:
    """List Semantica-owned chapter packages; OE maintains no shadow registry."""

    return tuple(_list_chapter_packages())


def validate_chapter_registry() -> Tuple[str, ...]:
    """Validate Semantica's authoritative 29-chapter registry."""

    return tuple(_validate_chapter_registry())


def list_domain_packages() -> Tuple[Any, ...]:
    """List Semantica-owned non-chapter domain packages."""

    return tuple(_list_domain_packages())


def package_binding_metadata(package_id: str) -> Mapping[str, Any]:
    """Load one registered package and return path-free binding coordinates."""

    chapter_matches = [
        item for item in _list_chapter_packages() if item.package_id == package_id
    ]
    domain_matches = [
        item for item in _list_domain_packages() if item.package_id == package_id
    ]
    if len(chapter_matches) + len(domain_matches) != 1:
        raise RuntimeError("package ID is absent or ambiguous in Semantica registry")
    semantic_runtime = create_runtime()
    if chapter_matches:
        descriptor = chapter_matches[0]
        loaded = _load_chapter_package(
            semantic_runtime, descriptor.volume, descriptor.chapter
        )
    else:
        loaded = _load_domain_package(semantic_runtime, package_id)
    return {
        "package_digest": str(loaded.identity.digest),
        "required_capabilities": ["semantic.package.load"],
        "available_capabilities": sorted(semantic_runtime.profile.capabilities),
    }


def validate_domain_packages() -> Tuple[str, ...]:
    """Validate every Semantica-owned domain package."""

    return tuple(
        "{}: {}".format(item.package_id, issue)
        for item in _list_domain_packages()
        for issue in _validate_domain_package(item.package_id)
    )


def verify_book_source_bindings(book_root: str | Path) -> Mapping[str, Any]:
    """Verify both book-source families through the source-locked Semantica API."""

    verify_runtime_source_identity()
    return _verify_book_source_bindings(Path(book_root)).as_dict()


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


def normative_engraver_main(argv: Optional[Sequence[str]] = None) -> int:
    """Delegate controlled normative engraving to Semantica."""

    from semantica.chapter_packages.normative import main

    return main(argv)


__all__ = [
    "NATIVE_REFINERY_CONTRACT",
    "NATIVE_REFINERY_ASSET_CATEGORIES",
    "NATIVE_REFINERY_BOOK_IMPACTS",
    "NATIVE_REFINERY_CASE_KINDS",
    "NATIVE_REFINERY_REGRESSION_CHECK_IDS",
    "NATIVE_REFINERY_RELEASE_CHECK_IDS",
    "NATIVE_REFINERY_TRANSITION_CONTEXT_ACTIONS",
    "NATIVE_REFINERY_TRANSITION_CONTEXT_REQUIRED_OPERATIONS",
    "NATIVE_REFINERY_RUNNER_CONTRACT",
    "NATIVE_REFINERY_OPERATIONS",
    "NATIVE_REFINERY_STATES",
    "RUNTIME_ID",
    "SOURCE_LOCK_SCHEMA",
    "STAGING_RUNTIME_SCHEMA",
    "RuntimeSourceLock",
    "StagingRuntimeDescriptor",
    "SOURCE_LOCK_PATH",
    "chapter_asset_text",
    "create_package_runner",
    "create_runtime",
    "installed_runtime_artifact_sha256",
    "installed_runtime_version",
    "list_chapter_packages",
    "list_domain_packages",
    "native_refinery_commit_candidate",
    "native_refinery_acceptance_delta",
    "native_refinery_discover_package",
    "native_refinery_empty_package_sha256",
    "native_refinery_history",
    "native_refinery_open_engagement",
    "native_refinery_promote_candidate",
    "native_refinery_propose_candidate",
    "native_refinery_run_package",
    "native_refinery_verify_candidate",
    "normative_engraver_main",
    "package_asset_text",
    "package_binding_metadata",
    "read_migration_map",
    "read_runtime_source_lock",
    "read_staging_runtime_descriptor",
    "resolve_migration_successor",
    "run_and_verify_package",
    "run_package",
    "semantic_refinery_capabilities",
    "validate_chapter_registry",
    "validate_domain_packages",
    "verify_runtime_source_identity",
    "verify_installed_runtime_record",
    "verify_book_source_bindings",
]
