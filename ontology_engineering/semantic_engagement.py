"""Source-locked semantic engagement control plane for Ontology Engineering.

This module is deliberately orchestration-only.  It validates a project
binding, injects the exact vendored Semantica identity, and delegates every
semantic operation through :mod:`ontology_engineering.semantica_runtime`.
There is no selectable backend, arbitrary package path, or fallback runtime.

The JSON response keeps transport success separate from six independent
semantic verdicts.  Consequently a CLI exit code of zero means only that a
stable response was emitted; it never means that execution, regression,
receipt, release, and learning promotion all passed.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
from typing import Any, Mapping, Optional, Sequence

from . import semantica_runtime as runtime


BINDING_SCHEMA = "ontology-engineering.semantic-project-binding/v1"
TASK_SCHEMA = "ontology-engineering.semantic-task-envelope/v1"
AUTHORIZATION_SCHEMA = "ontology-engineering.refinery-authorization/v1"
RESPONSE_SCHEMA = "ontology-engineering.semantic-engagement-response/v1"
PROMOTION_TARGET = "industry-registry"
NATIVE_REFINERY_CONTRACT = runtime.NATIVE_REFINERY_CONTRACT
STATE_MODEL = (
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted",
)
KNOWN_ACTIONS = frozenset(
    {
        "doctor",
        "discover",
        "open",
        "run",
        "propose",
        "commit",
        "verify",
        "history",
        "promote",
    }
)
_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_LOGICAL_ROOT = re.compile(r"^[a-z][a-z0-9+.-]*:[^\s]+$")
_PACKAGE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]*$")


class SemanticEngagementError(RuntimeError):
    """Base error returned as a fail-closed semantic engagement response."""


class BindingValidationError(SemanticEngagementError):
    """The project binding is absent, unsafe, or structurally invalid."""


class ActionNotAllowedError(SemanticEngagementError):
    """The requested action is outside the project's explicit authority."""


class CapabilityUnavailableError(SemanticEngagementError):
    """The exact Semantica API requested by the binding is unavailable."""


class BaselineMismatchError(SemanticEngagementError):
    """The selected corpus/workspace does not match the bound baseline."""


@dataclass(frozen=True)
class ProjectBinding:
    """Strict, path-independent project-to-Semantica authority binding."""

    binding_id: str
    project_id: str
    domain: str
    target_kind: str
    package_id: str
    package_version: Optional[str]
    workspace_id: Optional[str]
    baseline_version: str
    baseline_digest: str
    evidence_logical_root: str
    fact_authority: Mapping[str, Any]
    decision_authority: Mapping[str, Any]
    allowed_actions: tuple[str, ...]
    lifecycle_actions: tuple[str, ...]
    promotion_target: str
    created_at: str
    semantic_api: str
    predecessor: Optional[Mapping[str, str]]
    source_sha256: str

    def as_dict(self) -> dict[str, Any]:
        target: dict[str, Any] = {
            "kind": self.target_kind,
            "package_id": self.package_id,
        }
        if self.package_version is not None:
            target["package_version"] = self.package_version
        if self.workspace_id is not None:
            target["workspace_id"] = self.workspace_id
        result = {
            "binding_id": self.binding_id,
            "project": {"project_id": self.project_id, "domain": self.domain},
            "semantic_target": target,
            "baseline": {
                "version": self.baseline_version,
                "digest": self.baseline_digest,
            },
            "evidence": {"logical_root": self.evidence_logical_root},
            "authority": {
                "fact": dict(self.fact_authority),
                "decision": dict(self.decision_authority),
            },
            "allowed_actions": list(self.allowed_actions),
            "lifecycle_actions": list(self.lifecycle_actions),
            "promotion": {
                "target": self.promotion_target,
                "requires_decision_authority": True,
            },
            "created_at": self.created_at,
            "semantic_api": self.semantic_api,
            "source_sha256": self.source_sha256,
        }
        if self.predecessor is not None:
            result["predecessor"] = dict(self.predecessor)
        return result


@dataclass(frozen=True)
class SemanticTaskEnvelope:
    """Strict task identity passed from an engineering skill into Semantica."""

    task_id: str
    task_kind: str
    intent: str
    project: str
    domain: str
    requested_decision: str
    evidence: tuple[Mapping[str, str], ...]
    requested_actions: tuple[str, ...]
    actor_id: str
    required_capabilities: tuple[str, ...]
    created_at: str
    source_sha256: str

    def as_dict(self) -> dict[str, Any]:
        return {
            "task_id": self.task_id,
            "task_kind": self.task_kind,
            "intent": self.intent,
            "project": self.project,
            "domain": self.domain,
            "requested_decision": self.requested_decision,
            "evidence": [dict(item) for item in self.evidence],
            "requested_actions": list(self.requested_actions),
            "actor_id": self.actor_id,
            "required_capabilities": list(self.required_capabilities),
            "created_at": self.created_at,
            "source_sha256": self.source_sha256,
        }


def canonical_json(value: Any) -> str:
    """Encode one stable, non-lossy JSON response."""

    return json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    )


def _sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _strict_keys(
    value: Mapping[str, Any],
    *,
    location: str,
    required: set[str],
    optional: set[str] = frozenset(),
) -> None:
    keys = set(value)
    missing = sorted(required - keys)
    unknown = sorted(keys - required - set(optional))
    if missing:
        raise BindingValidationError(
            "{} is missing required fields: {}".format(location, ", ".join(missing))
        )
    if unknown:
        raise BindingValidationError(
            "{} contains unsupported fields: {}".format(location, ", ".join(unknown))
        )


def _mapping(value: Any, location: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise BindingValidationError("{} must be an object".format(location))
    return value


def _text(value: Any, location: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise BindingValidationError("{} must be non-empty text".format(location))
    return value.strip()


def _authority(value: Any, location: str) -> dict[str, Any]:
    item = _mapping(value, location)
    _strict_keys(
        item,
        location=location,
        required={"authority_id", "scope"},
    )
    authority_id = _text(item["authority_id"], location + ".authority_id")
    scope = item["scope"]
    if (
        not isinstance(scope, list)
        or not scope
        or not all(isinstance(entry, str) and entry.strip() for entry in scope)
    ):
        raise BindingValidationError(
            "{}.scope must be a non-empty unique text list".format(location)
        )
    normalized_scope = [entry.strip() for entry in scope]
    if len(set(normalized_scope)) != len(normalized_scope):
        raise BindingValidationError(
            "{}.scope must be a non-empty unique text list".format(location)
        )
    return {
        "authority_id": authority_id,
        "scope": sorted(normalized_scope),
    }


def _timestamp(value: Any, location: str) -> str:
    text = _text(value, location)
    candidate = text[:-1] + "+00:00" if text.endswith("Z") else text
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError as exc:
        raise BindingValidationError(
            "{} must be an ISO 8601 timestamp".format(location)
        ) from exc
    if parsed.tzinfo is None:
        raise BindingValidationError("{} must include a timezone".format(location))
    return text


def _forbid_backend_or_fallback(value: Any, location: str = "binding") -> None:
    if isinstance(value, Mapping):
        for key, child in value.items():
            normalized = str(key).lower().replace("-", "_")
            if "backend" in normalized or "fallback" in normalized:
                raise BindingValidationError(
                    "{} contains forbidden runtime-selection field {!r}".format(
                        location, key
                    )
                )
            _forbid_backend_or_fallback(child, location + "." + str(key))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _forbid_backend_or_fallback(child, "{}[{}]".format(location, index))


def read_project_binding(path: Path | str) -> ProjectBinding:
    """Read and validate one strict project binding without resolving evidence."""

    source = Path(path).expanduser()
    if source.is_symlink() or not source.is_file():
        raise BindingValidationError(
            "binding must be a regular, non-symbolic-link JSON file"
        )
    try:
        payload = source.read_bytes()
        value = json.loads(payload.decode("utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise BindingValidationError("binding is not valid UTF-8 JSON") from exc
    root = _mapping(value, "binding")
    _forbid_backend_or_fallback(root)
    _strict_keys(
        root,
        location="binding",
        required={
            "$schema",
            "binding_id",
            "project",
            "semantic_target",
            "baseline",
            "evidence",
            "authority",
            "allowed_actions",
            "lifecycle_actions",
            "promotion",
            "created_at",
            "semantic_api",
        },
        optional={"predecessor"},
    )
    if root["$schema"] != BINDING_SCHEMA:
        raise BindingValidationError("binding has an unsupported $schema")

    project = _mapping(root["project"], "binding.project")
    _strict_keys(
        project,
        location="binding.project",
        required={"project_id", "domain"},
    )
    target = _mapping(root["semantic_target"], "binding.semantic_target")
    kind = _text(target.get("kind"), "binding.semantic_target.kind")
    if kind == "package":
        _strict_keys(
            target,
            location="binding.semantic_target",
            required={"kind", "package_id", "package_version"},
        )
        package_version: Optional[str] = _text(
            target["package_version"], "binding.semantic_target.package_version"
        )
        workspace_id: Optional[str] = None
    elif kind == "workspace":
        _strict_keys(
            target,
            location="binding.semantic_target",
            required={"kind", "workspace_id", "package_id"},
        )
        package_version = None
        workspace_id = _text(
            target["workspace_id"], "binding.semantic_target.workspace_id"
        )
    else:
        raise BindingValidationError(
            "binding.semantic_target.kind must be package or workspace"
        )
    package_id = _text(target["package_id"], "binding.semantic_target.package_id")
    if not _PACKAGE_ID.fullmatch(package_id):
        raise BindingValidationError(
            "binding.semantic_target.package_id has invalid syntax"
        )

    baseline = _mapping(root["baseline"], "binding.baseline")
    _strict_keys(
        baseline,
        location="binding.baseline",
        required={"version", "digest"},
    )
    baseline_version = _text(baseline["version"], "binding.baseline.version")
    baseline_digest = _text(baseline["digest"], "binding.baseline.digest")
    if not _HEX64.fullmatch(baseline_digest):
        raise BindingValidationError(
            "binding.baseline.digest must be a lowercase SHA-256 digest"
        )
    if package_version is not None and package_version != baseline_version:
        raise BindingValidationError(
            "package target version must equal the bound baseline version"
        )

    evidence = _mapping(root["evidence"], "binding.evidence")
    _strict_keys(
        evidence,
        location="binding.evidence",
        required={"logical_root"},
    )
    evidence_root = _text(evidence["logical_root"], "binding.evidence.logical_root")
    if not _LOGICAL_ROOT.fullmatch(evidence_root) or evidence_root.lower().startswith(
        ("file:", "http:", "https:")
    ):
        raise BindingValidationError(
            "evidence.logical_root must be a non-filesystem logical URI"
        )

    authority = _mapping(root["authority"], "binding.authority")
    _strict_keys(
        authority,
        location="binding.authority",
        required={"fact", "decision"},
    )
    fact_authority = _authority(authority["fact"], "binding.authority.fact")
    decision_authority = _authority(authority["decision"], "binding.authority.decision")

    actions = root["allowed_actions"]
    if (
        not isinstance(actions, list)
        or not actions
        or not all(isinstance(item, str) for item in actions)
        or len(actions) != len(set(actions))
    ):
        raise BindingValidationError(
            "binding.allowed_actions must be a non-empty unique text list"
        )
    unknown_actions = sorted(set(actions) - KNOWN_ACTIONS)
    if unknown_actions:
        raise BindingValidationError(
            "binding.allowed_actions contains unsupported actions: {}".format(
                ", ".join(unknown_actions)
            )
        )
    lifecycle_actions = root["lifecycle_actions"]
    if (
        not isinstance(lifecycle_actions, list)
        or not all(isinstance(item, str) for item in lifecycle_actions)
        or len(lifecycle_actions) != len(set(lifecycle_actions))
    ):
        raise BindingValidationError(
            "binding.lifecycle_actions must be a unique text list"
        )
    unknown_lifecycle = sorted(set(lifecycle_actions) - set(STATE_MODEL))
    if unknown_lifecycle:
        raise BindingValidationError(
            "binding.lifecycle_actions contains unsupported states: {}".format(
                ", ".join(unknown_lifecycle)
            )
        )
    if (
        lifecycle_actions
        and tuple(lifecycle_actions) != STATE_MODEL[: len(lifecycle_actions)]
    ):
        raise BindingValidationError(
            "binding.lifecycle_actions must be an ordered prefix of the refinery state model"
        )
    if kind == "workspace" and not lifecycle_actions:
        raise BindingValidationError(
            "workspace bindings require at least the candidate lifecycle action"
        )
    if kind == "package":
        if lifecycle_actions:
            raise BindingValidationError(
                "built-in package bindings cannot grant lifecycle states"
            )
        non_read_actions = sorted(
            set(actions) - {"doctor", "discover", "run", "verify"}
        )
        if non_read_actions:
            raise BindingValidationError(
                "built-in package bindings are read-only; unsupported actions: {}".format(
                    ", ".join(non_read_actions)
                )
            )
    state_command = {
        "candidate": "propose",
        "proposed": "propose",
        "committed": "commit",
        "regression_passed": "verify",
        "release_complete": "verify",
        "promoted": "promote",
    }
    unauthorized_states = [
        state for state in lifecycle_actions if state_command[state] not in actions
    ]
    if unauthorized_states:
        raise BindingValidationError(
            "lifecycle actions lack corresponding command authority: {}".format(
                ", ".join(unauthorized_states)
            )
        )

    promotion = _mapping(root["promotion"], "binding.promotion")
    _strict_keys(
        promotion,
        location="binding.promotion",
        required={"target", "requires_decision_authority"},
    )
    if promotion["requires_decision_authority"] is not True:
        raise BindingValidationError(
            "promotion must require the declared decision authority"
        )

    promotion_target = _text(promotion["target"], "binding.promotion.target")
    if promotion_target != PROMOTION_TARGET:
        raise BindingValidationError(
            "binding.promotion.target must be the industry-registry channel"
        )

    semantic_api = _text(root["semantic_api"], "binding.semantic_api")
    if semantic_api != runtime.NATIVE_REFINERY_CONTRACT:
        raise BindingValidationError(
            "binding.semantic_api must be semantica.ontology.refinery/v1; "
            "no compatibility or fallback lifecycle is selectable"
        )
    predecessor: Optional[dict[str, str]] = None
    if "predecessor" in root:
        raw_predecessor = _mapping(root["predecessor"], "binding.predecessor")
        _strict_keys(
            raw_predecessor,
            location="binding.predecessor",
            required={"binding_sha256", "promotion_record_sha256"},
        )
        predecessor = {
            "binding_sha256": _text(
                raw_predecessor["binding_sha256"],
                "binding.predecessor.binding_sha256",
            ),
            "promotion_record_sha256": _text(
                raw_predecessor["promotion_record_sha256"],
                "binding.predecessor.promotion_record_sha256",
            ),
        }
        for field, digest in predecessor.items():
            if not _HEX64.fullmatch(digest):
                raise BindingValidationError(
                    "binding.predecessor.{} must be a lowercase SHA-256 digest".format(
                        field
                    )
                )
    return ProjectBinding(
        binding_id=_text(root["binding_id"], "binding.binding_id"),
        project_id=_text(project["project_id"], "binding.project.project_id"),
        domain=_text(project["domain"], "binding.project.domain"),
        target_kind=kind,
        package_id=package_id,
        package_version=package_version,
        workspace_id=workspace_id,
        baseline_version=baseline_version,
        baseline_digest=baseline_digest,
        evidence_logical_root=evidence_root,
        fact_authority=fact_authority,
        decision_authority=decision_authority,
        allowed_actions=tuple(sorted(actions)),
        lifecycle_actions=tuple(lifecycle_actions),
        promotion_target=promotion_target,
        created_at=_timestamp(root["created_at"], "binding.created_at"),
        semantic_api=semantic_api,
        predecessor=predecessor,
        source_sha256=_sha256_bytes(payload),
    )


def read_task_envelope(
    path: Path | str, *, binding: ProjectBinding
) -> SemanticTaskEnvelope:
    """Read and bind a task to one project, domain, evidence root, and authority."""

    source = Path(path).expanduser()
    if source.is_symlink() or not source.is_file():
        raise BindingValidationError(
            "task envelope must be a regular, non-symbolic-link JSON file"
        )
    try:
        payload = source.read_bytes()
        value = json.loads(payload.decode("utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise BindingValidationError("task envelope is not valid UTF-8 JSON") from exc
    root = _mapping(value, "task")
    _forbid_backend_or_fallback(root, "task")
    _strict_keys(
        root,
        location="task",
        required={
            "$schema",
            "task_id",
            "task_kind",
            "intent",
            "project",
            "domain",
            "requested_decision",
            "evidence",
            "requested_actions",
            "actor_id",
            "required_capabilities",
            "created_at",
        },
    )
    if root["$schema"] != TASK_SCHEMA:
        raise BindingValidationError("task has an unsupported $schema")
    project = _text(root["project"], "task.project")
    domain = _text(root["domain"], "task.domain")
    if project != binding.project_id:
        raise BindingValidationError("task.project differs from the project binding")
    if domain != binding.domain:
        raise BindingValidationError("task.domain differs from the project binding")

    raw_evidence = root["evidence"]
    if not isinstance(raw_evidence, list) or not raw_evidence:
        raise BindingValidationError(
            "task.evidence must be a non-empty evidence identity list"
        )
    logical_prefix = binding.evidence_logical_root.rstrip("/")
    evidence_refs: list[dict[str, str]] = []
    source_ids: set[str] = set()
    uris: set[str] = set()
    for index, raw_reference in enumerate(raw_evidence):
        location = "task.evidence[{}]".format(index)
        reference = _mapping(raw_reference, location)
        _strict_keys(
            reference,
            location=location,
            required={"source_id", "uri", "sha256", "media_type", "captured_at"},
        )
        source_id = _text(reference["source_id"], location + ".source_id")
        uri = _text(reference["uri"], location + ".uri")
        digest = _text(reference["sha256"], location + ".sha256")
        if not _HEX64.fullmatch(digest):
            raise BindingValidationError(location + ".sha256 must be a SHA-256 digest")
        if uri != logical_prefix and not uri.startswith(logical_prefix + "/"):
            raise BindingValidationError(
                "task evidence reference escapes the bound logical evidence root"
            )
        if source_id in source_ids or uri in uris:
            raise BindingValidationError(
                "task evidence source_id and uri values must be unique"
            )
        source_ids.add(source_id)
        uris.add(uri)
        evidence_refs.append(
            {
                "source_id": source_id,
                "uri": uri,
                "sha256": digest,
                "media_type": _text(reference["media_type"], location + ".media_type"),
                "captured_at": _timestamp(
                    reference["captured_at"], location + ".captured_at"
                ),
            }
        )

    actions = root["requested_actions"]
    if (
        not isinstance(actions, list)
        or not actions
        or not all(isinstance(item, str) for item in actions)
        or len(actions) != len(set(actions))
    ):
        raise BindingValidationError(
            "task.requested_actions must be a non-empty unique text list"
        )
    unsupported = sorted(set(actions) - KNOWN_ACTIONS)
    unauthorized = sorted(set(actions) - set(binding.allowed_actions))
    if unsupported:
        raise BindingValidationError(
            "task requests unsupported actions: {}".format(", ".join(unsupported))
        )
    if unauthorized:
        raise ActionNotAllowedError(
            "task requests actions outside the binding: {}".format(
                ", ".join(unauthorized)
            )
        )
    required_capabilities = root["required_capabilities"]
    if (
        not isinstance(required_capabilities, list)
        or not required_capabilities
        or not all(
            isinstance(item, str) and item.strip() for item in required_capabilities
        )
        or len(required_capabilities) != len(set(required_capabilities))
    ):
        raise BindingValidationError(
            "task.required_capabilities must be a non-empty unique text list"
        )
    return SemanticTaskEnvelope(
        task_id=_text(root["task_id"], "task.task_id"),
        task_kind=_text(root["task_kind"], "task.task_kind"),
        intent=_text(root["intent"], "task.intent"),
        project=project,
        domain=domain,
        requested_decision=_text(root["requested_decision"], "task.requested_decision"),
        evidence=tuple(evidence_refs),
        requested_actions=tuple(sorted(actions)),
        actor_id=_text(root["actor_id"], "task.actor_id"),
        required_capabilities=tuple(sorted(required_capabilities)),
        created_at=_timestamp(root["created_at"], "task.created_at"),
        source_sha256=_sha256_bytes(payload),
    )


def _empty_section(status: str = "not_run", **details: Any) -> dict[str, Any]:
    return {"status": status, **details}


def _learning(
    *,
    verdict: str = "no_delta",
    current_state: Optional[str] = None,
    completed_states: Sequence[str] = (),
    status: str = "not_run",
    binding: Optional[ProjectBinding] = None,
    **details: Any,
) -> dict[str, Any]:
    if verdict not in {"no_delta", "candidate"}:
        raise ValueError("learning verdict must be no_delta or candidate")
    result: dict[str, Any] = {
        "status": status,
        "verdict": verdict,
        "state_model": list(STATE_MODEL),
        "current_state": current_state,
        "completed_states": list(completed_states),
        "promotion": {
            "status": "not_requested",
            "target": binding.promotion_target if binding else None,
        },
        "publication": {
            "status": "not_requested",
            "decision": "external_decision_required",
            "published": None,
            "decision_authority": (
                dict(binding.decision_authority) if binding else None
            ),
        },
    }
    result.update(details)
    return result


def _runtime_identity(*, verify_artifact: bool = True) -> dict[str, Any]:
    try:
        lock = (
            runtime.verify_runtime_source_identity()
            if verify_artifact
            else runtime.read_runtime_source_lock(verify_vendored_artifact=False)
        )
    except RuntimeError as exc:
        raise CapabilityUnavailableError(str(exc)) from exc
    installed = runtime.installed_runtime_version()
    installed_artifact = runtime.installed_runtime_artifact_sha256()
    if installed != lock.version:
        raise CapabilityUnavailableError(
            "installed Semantica version {!r} differs from source lock {!r}".format(
                installed, lock.version
            )
        )
    if installed_artifact != lock.artifact_sha256:
        raise CapabilityUnavailableError(
            "installed Semantica wheel differs from the source-locked artifact"
        )
    return {
        "runtime_id": runtime.RUNTIME_ID,
        "commit": lock.commit,
        "version": lock.version,
        "wheel_filename": lock.artifact_filename,
        "wheel_sha256": lock.artifact_sha256,
        "vendored_wheel_verified": bool(verify_artifact),
        "installed_version": installed,
        "installed_version_matches": True,
        "installed_wheel_sha256": installed_artifact,
        "installed_wheel_matches": True,
    }


def _envelope(
    command: str,
    *,
    binding: Optional[ProjectBinding] = None,
    task: Optional[SemanticTaskEnvelope] = None,
    runtime_identity: Optional[Mapping[str, Any]] = None,
) -> dict[str, Any]:
    return {
        "$schema": RESPONSE_SCHEMA,
        "command": command,
        "command_verdict": "blocked",
        "transport": {
            "status": "completed",
            "exit_code_semantics": (
                "exit 0 confirms stable JSON delivery only; inspect every semantic section"
            ),
        },
        "runtime_source": dict(runtime_identity or {}),
        "binding": binding.as_dict() if binding else None,
        "task": task.as_dict() if task else None,
        "corpus_found": _empty_section(),
        "execution": _empty_section(),
        "regression": _empty_section(),
        "receipt": _empty_section(),
        "release": _empty_section("not_checked"),
        "learning": _learning(binding=binding),
        "capabilities": runtime.semantic_refinery_capabilities(),
        "diagnostics": [],
    }


def _authorize(binding: ProjectBinding, action: str) -> None:
    if action not in binding.allowed_actions:
        raise ActionNotAllowedError(
            "action {!r} is not allowed by binding {!r}".format(
                action, binding.binding_id
            )
        )


def _load_context(
    command: str,
    binding_path: Path | str,
    *,
    task_path: Optional[Path | str] = None,
    require_task: bool = False,
) -> tuple[
    ProjectBinding,
    Optional[SemanticTaskEnvelope],
    dict[str, Any],
    dict[str, Any],
]:
    identity = _runtime_identity()
    binding = read_project_binding(binding_path)
    _authorize(binding, command)
    if require_task and task_path is None:
        raise BindingValidationError("this command requires --task")
    task = (
        read_task_envelope(task_path, binding=binding)
        if task_path is not None
        else None
    )
    if task is not None and command not in task.requested_actions:
        raise ActionNotAllowedError(
            "current action is not requested by the task envelope"
        )
    return (
        binding,
        task,
        identity,
        _envelope(command, binding=binding, task=task, runtime_identity=identity),
    )


def _native_authority_token(value: Mapping[str, Any]) -> str:
    """Bind both authority identity and scope into one native opaque token."""

    return canonical_json(
        {
            "authority_id": value["authority_id"],
            "scope": list(value["scope"]),
        }
    )


def _native_task(task: SemanticTaskEnvelope, action: str) -> dict[str, Any]:
    """Project one current OE invocation to exactly one native transition.

    The original OE envelope may request several user-facing commands, but a
    native transition context is deliberately narrower: it carries only the
    single action being written now.  Therefore an earlier candidate task can
    never replay as authority for commit, gates, release, or promotion.
    """

    return {
        "schema_version": "1.0",
        "task_id": task.task_id,
        "task_kind": task.task_kind,
        "project_id": task.project,
        "domain": task.domain,
        "intent": task.intent,
        "requested_decision": task.requested_decision,
        "actor_id": task.actor_id,
        "requested_actions": [action],
        "required_capabilities": list(task.required_capabilities),
        "evidence": [dict(item) for item in task.evidence],
        "created_at": task.created_at,
    }


def _native_binding(binding: ProjectBinding) -> dict[str, Any]:
    """Project a native workspace binding without inferring lifecycle authority."""

    if binding.target_kind != "workspace" or binding.workspace_id is None:
        raise BindingValidationError(
            "native refinery lifecycle requires a workspace target binding"
        )
    if binding.semantic_api != runtime.NATIVE_REFINERY_CONTRACT:
        raise CapabilityUnavailableError(
            "native binding projection requires the exact refinery contract"
        )
    return {
        "schema_version": "1.0",
        "binding_id": binding.binding_id,
        "project_id": binding.project_id,
        "domain": binding.domain,
        "package_id": binding.package_id,
        "workspace_id": binding.workspace_id,
        "baseline_version": binding.baseline_version,
        "baseline_package_sha256": binding.baseline_digest,
        "evidence_root": binding.evidence_logical_root,
        "fact_authorities": [_native_authority_token(binding.fact_authority)],
        "decision_authorities": [_native_authority_token(binding.decision_authority)],
        "allowed_actions": list(binding.lifecycle_actions),
        "semantic_api_contract": binding.semantic_api,
        "promotion_target": binding.promotion_target,
        "created_at": binding.created_at,
    }


def _next_binding_projection(
    binding: ProjectBinding, descriptor: Mapping[str, Any]
) -> dict[str, Any]:
    """Build, but never persist, the controlled successor workspace binding."""

    if binding.target_kind != "workspace" or binding.workspace_id is None:
        raise BindingValidationError(
            "a promoted successor binding requires a workspace target"
        )
    required = {
        "package_id",
        "version",
        "package_sha256",
        "manifest_sha256",
        "promotion_record_sha256",
        "promoted_at",
    }
    missing = sorted(required - set(descriptor))
    if missing:
        raise CapabilityUnavailableError(
            "promotion descriptor cannot project the next binding; missing: {}".format(
                ", ".join(missing)
            )
        )
    if descriptor["package_id"] != binding.package_id:
        raise BaselineMismatchError(
            "promotion descriptor package_id differs from the binding"
        )
    version = _text(descriptor["version"], "promotion descriptor version")
    package_sha256 = _text(
        descriptor["package_sha256"], "promotion descriptor package_sha256"
    )
    manifest_sha256 = _text(
        descriptor["manifest_sha256"], "promotion descriptor manifest_sha256"
    )
    promotion_record_sha256 = _text(
        descriptor["promotion_record_sha256"],
        "promotion descriptor promotion_record_sha256",
    )
    for field, digest in (
        ("package_sha256", package_sha256),
        ("manifest_sha256", manifest_sha256),
        ("promotion_record_sha256", promotion_record_sha256),
    ):
        if not _HEX64.fullmatch(digest):
            raise CapabilityUnavailableError(
                "promotion descriptor {} is not a SHA-256 digest".format(field)
            )
    promoted_at = _timestamp(
        descriptor["promoted_at"], "promotion descriptor promoted_at"
    )
    if (
        version == binding.baseline_version
        and package_sha256 == binding.baseline_digest
    ):
        raise BaselineMismatchError(
            "promotion did not produce a successor registry baseline"
        )
    document = {
        "$schema": BINDING_SCHEMA,
        "binding_id": "binding:promotion:{}".format(promotion_record_sha256),
        "project": {
            "project_id": binding.project_id,
            "domain": binding.domain,
        },
        "semantic_target": {
            "kind": "workspace",
            "workspace_id": binding.workspace_id,
            "package_id": binding.package_id,
        },
        "baseline": {"version": version, "digest": package_sha256},
        "evidence": {"logical_root": binding.evidence_logical_root},
        "authority": {
            "fact": dict(binding.fact_authority),
            "decision": dict(binding.decision_authority),
        },
        "allowed_actions": list(binding.allowed_actions),
        "lifecycle_actions": list(binding.lifecycle_actions),
        "promotion": {
            "target": binding.promotion_target,
            "requires_decision_authority": True,
        },
        "created_at": promoted_at,
        "semantic_api": binding.semantic_api,
        "predecessor": {
            "binding_sha256": binding.source_sha256,
            "promotion_record_sha256": promotion_record_sha256,
        },
    }
    return {
        "status": "proposed",
        "requires_control_plane_approval": True,
        "auto_applied": False,
        "predecessor_binding_sha256": binding.source_sha256,
        "promotion_record_sha256": promotion_record_sha256,
        "registry_coordinates": {
            "workspace_id": binding.workspace_id,
            "package_id": binding.package_id,
            "version": version,
            "package_sha256": package_sha256,
            "manifest_sha256": manifest_sha256,
        },
        "projection_sha256": _sha256_bytes(canonical_json(document).encode("utf-8")),
        "document": document,
    }


def _read_json_mapping(path: Path | str, location: str) -> Mapping[str, Any]:
    source = Path(path).expanduser()
    if source.is_symlink() or not source.is_file():
        raise BindingValidationError(
            "{} must be a regular, non-symbolic-link JSON file".format(location)
        )
    try:
        value = json.loads(source.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise BindingValidationError(
            "{} is not valid UTF-8 JSON".format(location)
        ) from exc
    if not isinstance(value, Mapping):
        raise BindingValidationError("{} JSON root must be an object".format(location))
    _forbid_backend_or_fallback(value, location)
    return value


def _engagement_input(path: Path | str) -> Mapping[str, Any]:
    value = _read_json_mapping(path, "engagement evidence")
    if value.get("$schema") == RESPONSE_SCHEMA:
        execution = value.get("execution")
        if not isinstance(execution, Mapping):
            raise BindingValidationError("semantic response has no execution section")
        native_receipt = execution.get("native_engagement_receipt")
        if not isinstance(native_receipt, Mapping):
            raise BindingValidationError(
                "semantic response has no native engagement receipt"
            )
        return native_receipt
    return value


def _native_delta_input(
    path: Path | str, *, binding: ProjectBinding
) -> Mapping[str, Any]:
    value = _read_json_mapping(path, "native package delta")
    required = {
        "schema_version",
        "package_id",
        "base_version",
        "base_package_sha256",
        "target_version",
        "rationale",
        "created_by",
        "created_at",
        "required_capabilities",
        "source_evidence",
        "book_impact",
        "ontology",
        "competency_questions",
        "shapes",
        "queries",
        "rules",
        "cases",
        "contract",
        "provenance",
    }
    optional = {"delta_sha256"}
    _strict_keys(
        value,
        location="native package delta",
        required=required,
        optional=optional,
    )
    if value["package_id"] != binding.package_id:
        raise BaselineMismatchError("native delta package_id differs from binding")
    if value["base_version"] != binding.baseline_version:
        raise BaselineMismatchError("native delta base_version differs from binding")
    if value["base_package_sha256"] != binding.baseline_digest:
        raise BaselineMismatchError("native delta base digest differs from binding")
    declared_creator = binding.fact_authority["authority_id"]
    if value["created_by"] != declared_creator:
        raise ActionNotAllowedError(
            "native delta created_by must match the declared fact authority_id"
        )
    asset_required = {
        "category",
        "asset_id",
        "operation",
        "media_type",
        "sha256",
        "content_base64",
    }
    asset_optional = {"replaces_sha256", "role", "case_kind"}
    for category in (
        "ontology",
        "competency_questions",
        "shapes",
        "queries",
        "rules",
        "cases",
        "contract",
        "provenance",
    ):
        assets = value[category]
        if not isinstance(assets, list):
            raise BindingValidationError(
                "native package delta.{} must be a list".format(category)
            )
        for index, asset in enumerate(assets):
            asset_mapping = _mapping(
                asset, "native package delta.{}[{}]".format(category, index)
            )
            _strict_keys(
                asset_mapping,
                location="native package delta.{}[{}]".format(category, index),
                required=asset_required,
                optional=asset_optional,
            )
    projected = dict(value)
    projected["created_by"] = _native_authority_token(binding.fact_authority)
    return projected


def _native_authorization_input(
    path: Path | str,
    *,
    binding: ProjectBinding,
    expected_action: str,
    candidate_sha256: str,
) -> Mapping[str, Any]:
    value = _read_json_mapping(path, "native refinery authorization")
    _strict_keys(
        value,
        location="native refinery authorization",
        required={
            "$schema",
            "authorization_id",
            "action",
            "actor_id",
            "authority_id",
            "authority_scope",
            "package_id",
            "delta_sha256",
            "promotion_target",
            "reason",
            "source",
            "issued_at",
            "decisions",
        },
    )
    if value["$schema"] != AUTHORIZATION_SCHEMA:
        raise BindingValidationError("authorization has an unsupported $schema")
    if value["action"] != expected_action:
        raise ActionNotAllowedError(
            "authorization action must be {}".format(expected_action)
        )
    declared = binding.decision_authority
    if value["actor_id"] != declared["authority_id"]:
        raise ActionNotAllowedError(
            "authorization actor_id is not the declared decision authority"
        )
    if value["authority_id"] != declared["authority_id"]:
        raise ActionNotAllowedError(
            "authorization authority_id is not the declared decision authority"
        )
    authority_scope = value["authority_scope"]
    if (
        not isinstance(authority_scope, list)
        or not authority_scope
        or not all(isinstance(item, str) and item.strip() for item in authority_scope)
    ):
        raise BindingValidationError(
            "authorization authority_scope must be a non-empty text list"
        )
    normalized_scope = [item.strip() for item in authority_scope]
    if len(set(normalized_scope)) != len(normalized_scope):
        raise BindingValidationError(
            "authorization authority_scope must not contain duplicates"
        )
    if set(normalized_scope) != set(declared["scope"]):
        raise ActionNotAllowedError(
            "authorization authority_scope differs from the project binding"
        )
    if value["package_id"] != binding.package_id:
        raise BaselineMismatchError("authorization package_id differs from binding")
    if value["delta_sha256"] != candidate_sha256:
        raise BaselineMismatchError("authorization does not bind the exact candidate")
    if value["promotion_target"] != binding.promotion_target:
        raise BaselineMismatchError(
            "authorization promotion_target differs from binding"
        )
    source = _mapping(value["source"], "authorization.source")
    _strict_keys(
        source,
        location="authorization.source",
        required={"source_id", "uri", "sha256", "media_type", "captured_at"},
    )
    uri = _text(source["uri"], "authorization.source.uri")
    root = binding.evidence_logical_root.rstrip("/")
    if uri != root and not uri.startswith(root + "/"):
        raise BindingValidationError(
            "authorization evidence URI escapes the bound logical evidence root"
        )
    digest = _text(source["sha256"], "authorization.source.sha256")
    if not _HEX64.fullmatch(digest):
        raise BindingValidationError("authorization source sha256 is invalid")
    _timestamp(source["captured_at"], "authorization.source.captured_at")
    _timestamp(value["issued_at"], "authorization.issued_at")
    raw_decisions = value["decisions"]
    if not isinstance(raw_decisions, list):
        raise BindingValidationError("authorization.decisions must be a list")
    decisions: list[dict[str, str]] = []
    decision_keys: set[tuple[str, str, str, str]] = set()
    for index, raw_decision in enumerate(raw_decisions):
        location = "authorization.decisions[{}]".format(index)
        decision = _mapping(raw_decision, location)
        _strict_keys(
            decision,
            location=location,
            required={
                "category",
                "asset_id",
                "operation",
                "replaces_sha256",
                "verdict",
                "reason",
            },
        )
        category = _text(decision["category"], location + ".category")
        if category not in runtime.NATIVE_REFINERY_ASSET_CATEGORIES:
            raise BindingValidationError(location + ".category is unsupported")
        operation = _text(decision["operation"], location + ".operation")
        if operation not in {"replace", "remove"}:
            raise BindingValidationError(
                location + ".operation must be replace or remove"
            )
        replaces_sha256 = _text(
            decision["replaces_sha256"], location + ".replaces_sha256"
        )
        if not _HEX64.fullmatch(replaces_sha256):
            raise BindingValidationError(
                location + ".replaces_sha256 must be a SHA-256 digest"
            )
        verdict = _text(decision["verdict"], location + ".verdict")
        if verdict != "approve":
            raise BindingValidationError(location + ".verdict must be approve")
        normalized = {
            "category": category,
            "asset_id": _text(decision["asset_id"], location + ".asset_id"),
            "operation": operation,
            "replaces_sha256": replaces_sha256,
            "verdict": verdict,
            "reason": _text(decision["reason"], location + ".reason"),
        }
        key = (
            normalized["category"],
            normalized["asset_id"],
            normalized["operation"],
            normalized["replaces_sha256"],
        )
        if key in decision_keys:
            raise BindingValidationError(
                "authorization.decisions must not contain duplicate asset decisions"
            )
        decision_keys.add(key)
        decisions.append(normalized)
    token = _native_authority_token(declared)
    return {
        "authorization_id": _text(
            value["authorization_id"], "authorization.authorization_id"
        ),
        "action": expected_action,
        "actor_id": token,
        "authority": token,
        "package_id": binding.package_id,
        "delta_sha256": candidate_sha256,
        "promotion_target": binding.promotion_target,
        "reason": _text(value["reason"], "authorization.reason"),
        "source": dict(source),
        "issued_at": value["issued_at"],
        "decisions": sorted(
            decisions,
            key=lambda item: (
                item["category"],
                item["asset_id"],
                item["operation"],
                item["replaces_sha256"],
            ),
        ),
    }


def _descriptor_dict(
    item: Any, package_type: str, *, include_binding_metadata: bool = False
) -> dict[str, Any]:
    result = {
        "type": package_type,
        "package_id": str(item.package_id),
        "version": str(item.version),
        "status": str(item.status),
        "release_status": str(item.release_status),
    }
    if package_type == "chapter":
        result.update(
            {
                "volume": str(item.volume),
                "chapter": str(item.chapter),
                "title": str(item.title),
            }
        )
    else:
        result["domain"] = str(item.domain)
    if include_binding_metadata:
        result.update(runtime.package_binding_metadata(str(item.package_id)))
    return result


def _all_descriptors(*, include_binding_metadata: bool = False) -> list[dict[str, Any]]:
    chapters = [
        _descriptor_dict(
            item,
            "chapter",
            include_binding_metadata=include_binding_metadata,
        )
        for item in runtime.list_chapter_packages()
    ]
    domains = [
        _descriptor_dict(
            item,
            "domain",
            include_binding_metadata=include_binding_metadata,
        )
        for item in runtime.list_domain_packages()
    ]
    return sorted(chapters + domains, key=lambda item: item["package_id"])


def _bound_package(
    binding: ProjectBinding, descriptors: Sequence[Mapping[str, Any]]
) -> Mapping[str, Any]:
    matches = [
        item for item in descriptors if item.get("package_id") == binding.package_id
    ]
    if len(matches) != 1:
        raise BaselineMismatchError(
            "bound package is absent from Semantica's fixed registry"
        )
    item = matches[0]
    if item.get("version") != binding.package_version:
        raise BaselineMismatchError(
            "registered package version differs from the bound baseline"
        )
    discovered_digest = item.get("package_digest")
    if discovered_digest is not None and discovered_digest != binding.baseline_digest:
        raise BaselineMismatchError(
            "loaded package digest differs from the bound baseline"
        )
    return item


def _require_lifecycle_api(binding: ProjectBinding) -> str:
    capabilities = runtime.semantic_refinery_capabilities()
    if binding.semantic_api != runtime.NATIVE_REFINERY_CONTRACT:
        raise CapabilityUnavailableError(
            "the binding does not name Semantica's sole refinery control plane"
        )
    native = capabilities["native"]
    if not native["available"]:
        details = []
        if native.get("missing_operations"):
            details.append(
                "operations={}".format(",".join(native["missing_operations"]))
            )
        if native.get("missing_symbols"):
            details.append("symbols={}".format(",".join(native["missing_symbols"])))
        if native.get("constant_mismatches"):
            details.append(
                "constants={}".format(",".join(native["constant_mismatches"]))
            )
        if native.get("declaration_mismatches"):
            details.append(
                "declaration={}".format(",".join(native["declaration_mismatches"]))
            )
        if native.get("declaration_error"):
            details.append("capabilities={}".format(native["declaration_error"]))
        raise CapabilityUnavailableError(
            "native Semantica refinery API is contract-incompatible: {}".format(
                "; ".join(details) or "unknown capability failure"
            )
        )
    return "native"


def doctor(binding_path: Optional[Path | str] = None) -> dict[str, Any]:
    """Verify source lock, installed runtime, registries, binding, and API surface."""

    identity = _runtime_identity()
    binding = read_project_binding(binding_path) if binding_path is not None else None
    if binding is not None:
        _authorize(binding, "doctor")
    response = _envelope("doctor", binding=binding, runtime_identity=identity)
    chapter_issues = list(runtime.validate_chapter_registry())
    domain_issues = list(runtime.validate_domain_packages())
    descriptors = _all_descriptors()
    response["corpus_found"] = _empty_section(
        "found" if not chapter_issues and not domain_issues else "blocked",
        chapter_package_count=sum(
            1 for item in descriptors if item["type"] == "chapter"
        ),
        domain_package_count=sum(1 for item in descriptors if item["type"] == "domain"),
        registry_issues=chapter_issues + domain_issues,
    )
    native = response["capabilities"]["native"]
    response["execution"] = _empty_section(
        "not_run",
        source_lock_verified=True,
        native_refinery_status=native["status"],
    )
    response["learning"] = _learning(
        status="passed" if native["available"] else "blocked",
        binding=binding,
        reason=(
            "native refinery API is available"
            if native["available"]
            else "native refinery capability is missing; no compatibility fallback was selected"
        ),
    )
    response["command_verdict"] = (
        "passed"
        if response["corpus_found"]["status"] == "found" and native["available"]
        else "blocked"
    )
    return response


def discover(
    binding_path: Optional[Path | str] = None,
    *,
    workspace: Optional[Path | str] = None,
) -> dict[str, Any]:
    """Discover registry packages before or after selecting a project binding."""

    if binding_path is None:
        binding = None
        response = _envelope("discover", runtime_identity=_runtime_identity())
    else:
        binding, _, _, response = _load_context("discover", binding_path)
    descriptors = _all_descriptors(include_binding_metadata=True)
    selected: Optional[Mapping[str, Any]] = None
    if binding is None:
        status = "found"
    elif binding.target_kind == "package":
        selected = _bound_package(binding, descriptors)
        status = "found"
    else:
        _require_lifecycle_api(binding)
        if workspace is None:
            raise BindingValidationError(
                "bound workspace discovery requires --workspace"
            )
        selected = runtime.native_refinery_discover_package(
            str(workspace), binding=_native_binding(binding)
        )
        status = "found"
    response["corpus_found"] = _empty_section(
        status,
        selected=dict(selected) if selected else None,
        subject=(
            dict(selected)
            if selected is not None
            and binding is not None
            and binding.target_kind == "workspace"
            else None
        ),
        packages=descriptors,
        native_workspace_bootstrap={
            "status": (
                "available"
                if response["capabilities"]["native"]["available"]
                else "blocked"
            ),
            "semantic_api": runtime.NATIVE_REFINERY_CONTRACT,
            "baseline_version": "0",
            "baseline_digest": (
                runtime.native_refinery_empty_package_sha256()
                if response["capabilities"]["native"]["available"]
                else None
            ),
            "required_capabilities": ["semantic.engagement"],
        },
        note=(
            "discovery returns a hash-checked binding coordinate; release truth "
            "still requires a source-locked execution receipt"
        ),
    )
    response["execution"] = _empty_section("not_run")
    response["learning"] = _learning(
        status="passed",
        binding=binding,
        reason="discovery alone produced no reusable ontology delta",
    )
    response["command_verdict"] = "passed" if status != "blocked" else "blocked"
    return response


def open_engagement(
    binding_path: Path | str,
    *,
    task: Path | str,
    workspace: Optional[Path | str] = None,
    engagement: Optional[Path | str] = None,
) -> dict[str, Any]:
    """Open a source-locked task and record its native engagement boundary."""

    binding, task_envelope, _, response = _load_context(
        "open", binding_path, task_path=task, require_task=True
    )
    _require_lifecycle_api(binding)
    if binding.target_kind != "workspace":
        raise BindingValidationError("open is a refinery workspace lifecycle command")
    if workspace is None:
        raise BindingValidationError("native refinery requires --workspace")
    if task_envelope is None:
        raise BindingValidationError("native refinery requires a task envelope")
    engagement_value = _engagement_input(engagement) if engagement is not None else None
    native_receipt = runtime.native_refinery_open_engagement(
        str(workspace),
        envelope=_native_task(task_envelope, "engagement"),
        binding=_native_binding(binding),
        engagement=engagement_value,
    )
    response["corpus_found"] = _empty_section(
        "found",
        workspace_id=binding.workspace_id,
        package_id=binding.package_id,
        baseline_version=binding.baseline_version,
        baseline_package_sha256=binding.baseline_digest,
    )
    response["execution"] = _empty_section(
        native_receipt["execution"]["status"],
        operation="native_open_engagement",
        semantic_api=binding.semantic_api,
        mutation_performed=True,
        ontology_truth_mutated=False,
        task_sha256=task_envelope.source_sha256,
        native_envelope_sha256=native_receipt["envelope_sha256"],
        native_binding_sha256=native_receipt["binding_sha256"],
        native_engagement_receipt=native_receipt,
    )
    response["regression"] = _empty_section(
        native_receipt["regression"]["status"],
        phase=native_receipt["regression"],
    )
    response["receipt"] = _empty_section(
        "verified",
        receipt_sha256=native_receipt["receipt_sha256"],
        phase=native_receipt["receipt"],
        runtime_source=native_receipt["runtime_source"],
    )
    release_status = (
        "complete" if native_receipt["release"]["status"] == "passed" else "blocked"
    )
    response["release"] = _empty_section(
        release_status,
        phase=native_receipt["release"],
        engagement_status=native_receipt["status"],
        blocked_reasons=native_receipt["blocked_reasons"],
    )
    learning_value = native_receipt["learning"]
    response["learning"] = _learning(
        verdict=learning_value["status"],
        status=("passed" if native_receipt["status"] == "complete" else "blocked"),
        binding=binding,
        native=learning_value,
        reason=learning_value["rationale"],
    )
    response["command_verdict"] = (
        "passed" if native_receipt["status"] == "complete" else "blocked"
    )
    return response


def _receipt_status(receipt: Any, identity: Mapping[str, Any]) -> tuple[str, list[str]]:
    reasons: list[str] = []
    if receipt is None:
        return "missing", ["execution produced no receipt"]
    try:
        if not receipt.verify_integrity():
            reasons.append("receipt content integrity failed")
    except Exception:
        reasons.append("receipt does not expose verifiable integrity")
    expected = {
        "runtime_commit": identity["commit"],
        "runtime_version": identity["version"],
        "runtime_artifact_sha256": identity["wheel_sha256"],
    }
    for name, wanted in expected.items():
        if getattr(receipt, name, None) != wanted:
            reasons.append("receipt {} differs from source lock".format(name))
    return ("verified" if not reasons else "invalid"), reasons


def _mapping_receipt_status(
    receipt: Any, identity: Mapping[str, Any]
) -> tuple[str, list[str]]:
    """Check a Semantica-validated pure-data receipt against the source lock."""

    if not isinstance(receipt, Mapping):
        return "missing", ["execution produced no native receipt"]
    reasons: list[str] = []
    expected = {
        "runtime_commit": identity["commit"],
        "runtime_version": identity["version"],
        "runtime_artifact_sha256": identity["wheel_sha256"],
    }
    for name, wanted in expected.items():
        if receipt.get(name) != wanted:
            reasons.append("receipt {} differs from source lock".format(name))
    digest = receipt.get("receipt_sha256")
    if not isinstance(digest, str) or not _HEX64.fullmatch(digest):
        reasons.append("receipt has no valid content identity")
    return ("verified" if not reasons else "invalid"), reasons


def _package_execution(
    command: str,
    binding_path: Path | str,
    *,
    scenario_id: Optional[str] = None,
    task: Optional[Path | str] = None,
) -> dict[str, Any]:
    binding, task_envelope, identity, response = _load_context(
        command, binding_path, task_path=task
    )
    if binding.target_kind != "package":
        raise BindingValidationError(
            "package execution requires a package target binding"
        )
    selected = _bound_package(binding, _all_descriptors())
    result, verified_release = runtime.run_and_verify_package(
        binding.package_id, scenario_id
    )
    if result.package_version != binding.package_version:
        raise BaselineMismatchError(
            "executed package version differs from the bound baseline"
        )
    if result.package_digest != binding.baseline_digest:
        raise BaselineMismatchError(
            "executed package digest differs from the bound baseline"
        )
    receipt_status, receipt_reasons = _receipt_status(result.receipt, identity)
    response["corpus_found"] = _empty_section(
        "found",
        selected=dict(selected),
        package_digest=result.package_digest,
        baseline_digest_verified=True,
    )
    response["execution"] = _empty_section(
        result.status,
        package_id=result.package_id,
        package_version=result.package_version,
        scenario_id=result.scenario_id,
        operations=[item.as_dict() for item in result.operations],
        oracle_checks=[item.as_dict() for item in result.oracle_checks],
        reasons=list(result.reasons),
        task_sha256=(task_envelope.source_sha256 if task_envelope else None),
    )
    response["regression"] = _empty_section(
        result.cq_report.status,
        report=result.cq_report.as_dict(),
    )
    response["receipt"] = _empty_section(
        receipt_status,
        receipt=(result.receipt.as_dict() if result.receipt is not None else None),
        reasons=receipt_reasons,
    )
    response["release"] = _empty_section(
        verified_release.status,
        verdict=verified_release.as_dict(),
    )
    signals = []
    if result.status != "passed":
        signals.append("scenario execution did not pass")
    if verified_release.status != "complete":
        signals.append("package release is not complete")
    response["learning"] = _learning(
        status="passed",
        binding=binding,
        reason="execution evidence alone does not assert a reusable practice delta",
        signals=signals,
    )
    response["command_verdict"] = (
        "passed"
        if result.status == "passed"
        and receipt_status == "verified"
        and verified_release.status == "complete"
        else "blocked"
    )
    return response


def run(
    binding_path: Path | str,
    *,
    workspace: Optional[Path | str] = None,
    scenario_id: Optional[str] = None,
    task: Optional[Path | str] = None,
    recorded_at: Optional[str] = None,
) -> dict[str, Any]:
    """Run one exact bound built-in or promoted registry package."""

    binding = read_project_binding(binding_path)
    if binding.target_kind == "package":
        return _package_execution(
            "run", binding_path, scenario_id=scenario_id, task=task
        )
    bound, task_envelope, identity, response = _load_context(
        "run", binding_path, task_path=task
    )
    _require_lifecycle_api(bound)
    if workspace is None:
        raise BindingValidationError("workspace run requires --workspace")
    native = runtime.native_refinery_run_package(
        str(workspace),
        binding=_native_binding(bound),
        scenario_id=scenario_id,
        created_at=recorded_at,
    )
    subject = native["subject"]
    result = native["executor"]
    release = native["release"]
    receipt_status, receipt_reasons = _mapping_receipt_status(
        result.get("receipt"), identity
    )
    response["corpus_found"] = _empty_section(
        "found",
        subject=subject,
        baseline_subject_verified=True,
    )
    response["execution"] = _empty_section(
        str(result["status"]),
        operation="native_run_registry",
        subject=subject,
        executor={
            "package_id": result["package_id"],
            "version": result["package_version"],
            "digest": result["package_digest"],
            "scenario_id": result["scenario_id"],
            "receipt": result.get("receipt"),
        },
        operations=result["operations"],
        oracle_checks=result["oracle_checks"],
        reasons=result["reasons"],
        task_sha256=(task_envelope.source_sha256 if task_envelope else None),
    )
    response["regression"] = _empty_section(
        result["cq_report"]["status"], report=result["cq_report"]
    )
    response["receipt"] = _empty_section(
        receipt_status,
        receipt=result.get("receipt"),
        reasons=receipt_reasons,
    )
    response["release"] = _empty_section(release["status"], verdict=release)
    signals = []
    if result["status"] != "passed":
        signals.append("registry scenario execution did not pass")
    if release["status"] != "complete":
        signals.append("registry package release is not complete")
    response["learning"] = _learning(
        status="passed",
        binding=bound,
        reason="execution evidence alone does not assert a reusable practice delta",
        signals=signals,
    )
    response["command_verdict"] = (
        "passed"
        if result["status"] == "passed"
        and receipt_status == "verified"
        and release["status"] == "complete"
        else "blocked"
    )
    return response


def propose(
    binding_path: Path | str,
    *,
    workspace: Path | str,
    delta: Path | str,
    task: Optional[Path | str] = None,
    engagement: Optional[Path | str] = None,
    recorded_at: Optional[str] = None,
) -> dict[str, Any]:
    """Retain and propose a native candidate without changing registry truth."""

    binding, task_envelope, _, response = _load_context(
        "propose", binding_path, task_path=task, require_task=True
    )
    _require_lifecycle_api(binding)
    if task_envelope is None:
        raise BindingValidationError("native propose requires --task")
    if binding.target_kind != "workspace":
        raise BindingValidationError("propose requires a refinery workspace binding")
    if engagement is None:
        raise BindingValidationError(
            "native propose requires --engagement with a candidate learning receipt"
        )
    delta_value = _native_delta_input(delta, binding=binding)
    engagement_value = _engagement_input(engagement)
    native = runtime.native_refinery_propose_candidate(
        str(workspace),
        delta=delta_value,
        envelope=_native_task(task_envelope, "candidate"),
        proposed_envelope=_native_task(task_envelope, "proposed"),
        binding=_native_binding(binding),
        engagement=engagement_value,
        recorded_at=recorded_at,
    )
    state = native["state"]
    native_delta = native["delta"]
    native_receipt = native["engagement"]
    transition_contexts = native["transition_contexts"]
    if state["state"] != "proposed":
        raise CapabilityUnavailableError(
            "native propose did not return the proposed lifecycle state"
        )
    response["corpus_found"] = _empty_section(
        "found",
        workspace_id=binding.workspace_id,
        package_id=binding.package_id,
        baseline_version=binding.baseline_version,
        baseline_package_sha256=binding.baseline_digest,
    )
    response["execution"] = _empty_section(
        "passed",
        operation="native_propose_candidate",
        mutation_performed=True,
        ontology_truth_mutated=False,
        state=state,
        native_engagement_receipt=native_receipt,
        transition_contexts=transition_contexts,
    )
    response["regression"] = _empty_section(
        "not_run", reason="candidate regression occurs only after commit"
    )
    response["receipt"] = _empty_section(
        "verified",
        receipt_sha256=native_receipt["receipt_sha256"],
        runtime_source=native_receipt["runtime_source"],
    )
    response["release"] = _empty_section(
        "blocked", reasons=["candidate state is proposed, not release_complete"]
    )
    response["learning"] = _learning(
        verdict="candidate",
        current_state="proposed",
        completed_states=("candidate", "proposed"),
        status="passed",
        binding=binding,
        candidate={
            "candidate_sha256": native_delta["delta_sha256"],
            "delta_sha256": native_delta["delta_sha256"],
            "package_id": native_delta["package_id"],
            "target_version": native_delta["target_version"],
            "book_impact": native_delta["book_impact"],
            "task_sha256": task_envelope.source_sha256,
        },
        native_state=state,
        decision_required=True,
    )
    response["command_verdict"] = "passed"
    return response


def commit(
    binding_path: Path | str,
    *,
    workspace: Path | str,
    candidate_sha256: Optional[str] = None,
    authorization: Optional[Path | str] = None,
    task: Path | str,
    recorded_at: Optional[str] = None,
) -> dict[str, Any]:
    """Commit a non-empty candidate; promotion and publication remain separate."""

    binding, task_envelope, _, response = _load_context(
        "commit", binding_path, task_path=task, require_task=True
    )
    _require_lifecycle_api(binding)
    if binding.target_kind != "workspace":
        raise BindingValidationError("commit requires a refinery workspace binding")
    if task_envelope is None:
        raise BindingValidationError("native commit requires a task envelope")
    if candidate_sha256 is None or not _HEX64.fullmatch(candidate_sha256):
        raise BindingValidationError(
            "native commit requires --candidate with an exact delta SHA-256"
        )
    if authorization is None:
        raise BindingValidationError("native commit requires --authorization")
    authorization_value = _native_authorization_input(
        authorization,
        binding=binding,
        expected_action="commit",
        candidate_sha256=candidate_sha256,
    )
    native = runtime.native_refinery_commit_candidate(
        str(workspace),
        delta_sha256=candidate_sha256,
        authorization=authorization_value,
        envelope=_native_task(task_envelope, "committed"),
        binding=_native_binding(binding),
        recorded_at=recorded_at,
    )
    state = native["state"]
    if state["state"] != "committed":
        raise CapabilityUnavailableError(
            "native commit did not return the committed lifecycle state"
        )
    response["corpus_found"] = _empty_section(
        "found",
        workspace_id=binding.workspace_id,
        package_id=binding.package_id,
        candidate_sha256=candidate_sha256,
    )
    response["execution"] = _empty_section(
        "passed",
        operation="native_commit_candidate",
        semantic_api=binding.semantic_api,
        state=state,
        transition_context=native["transition_context"],
    )
    response["regression"] = _empty_section(
        "not_run", reason="committed candidate has not completed regression"
    )
    response["receipt"] = _empty_section(
        "bound",
        event_sha256=state["event_sha256"],
        package_sha256=state["package_sha256"],
    )
    response["release"] = _empty_section(
        "blocked", reasons=["candidate state is committed, not release_complete"]
    )
    response["learning"] = _learning(
        verdict="candidate",
        current_state="committed",
        completed_states=("candidate", "proposed", "committed"),
        status="passed",
        binding=binding,
        candidate={"candidate_sha256": candidate_sha256},
        native_state=state,
    )
    response["command_verdict"] = "passed"
    return response


def verify(
    binding_path: Path | str,
    *,
    workspace: Optional[Path | str] = None,
    scenario_id: Optional[str] = None,
    task: Optional[Path | str] = None,
    candidate_sha256: Optional[str] = None,
    recorded_at: Optional[str] = None,
) -> dict[str, Any]:
    """Verify a package run or derive native gates for a committed candidate."""

    binding = read_project_binding(binding_path)
    if binding.target_kind == "package":
        return _package_execution(
            "verify", binding_path, scenario_id=scenario_id, task=task
        )
    bound, task_envelope, _, response = _load_context(
        "verify", binding_path, task_path=task, require_task=True
    )
    if workspace is None:
        raise BindingValidationError("workspace target requires --workspace")
    _require_lifecycle_api(bound)
    if task_envelope is None:
        raise BindingValidationError("native verify requires a task envelope")
    if candidate_sha256 is None or not _HEX64.fullmatch(candidate_sha256):
        raise BindingValidationError(
            "native verify requires --candidate with an exact delta SHA-256"
        )
    native = runtime.native_refinery_verify_candidate(
        str(workspace),
        delta_sha256=candidate_sha256,
        envelopes={
            action: _native_task(task_envelope, action)
            for action in (
                "execute_candidate",
                "derive_regression_gate",
                "regression_passed",
                "derive_release_gate",
                "release_complete",
            )
        },
        binding=_native_binding(bound),
        recorded_at=recorded_at,
    )
    state = native["state"]
    suite = native["execution_suite"]
    regression = native["regression_evidence"]
    release = native["release_evidence"]
    transition_contexts = native["transition_contexts"]
    if state["state"] != "release_complete":
        raise CapabilityUnavailableError(
            "native verify did not return the release_complete lifecycle state"
        )
    if suite is not None:
        subject = {
            "package_id": suite["subject_package_id"],
            "version": suite["subject_package_version"],
            "package_sha256": suite["subject_package_sha256"],
            "manifest_sha256": suite["subject_manifest_sha256"],
            "execution_projection_sha256": suite["execution_projection_sha256"],
        }
        execution_status = "passed" if suite["status"] == "complete" else "blocked"
        execution_receipts = [
            {
                "scenario_id": item["scenario_id"],
                "receipt_sha256": item["receipt_sha256"],
                "receipt_object_sha256": item["receipt_object_sha256"],
            }
            for item in suite["runs"]
        ]
        suite_runtime_source = suite["runtime_source"]
    else:
        subject = {
            "package_id": bound.package_id,
            "version": None,
            "package_sha256": release["package_sha256"],
            "manifest_sha256": None,
            "execution_projection_sha256": None,
        }
        execution_status = "passed"
        execution_receipts = []
        suite_runtime_source = release["runtime_source"]
        expected_runtime = {
            "runtime_commit": response["runtime_source"]["commit"],
            "runtime_version": response["runtime_source"]["version"],
            "runtime_artifact_sha256": response["runtime_source"]["wheel_sha256"],
        }
        if any(
            suite_runtime_source.get(field) != expected
            for field, expected in expected_runtime.items()
        ):
            raise CapabilityUnavailableError(
                "retained native verification runtime differs from the OE source lock"
            )
    response["corpus_found"] = _empty_section(
        "found",
        workspace_id=bound.workspace_id,
        package_id=bound.package_id,
        candidate_sha256=candidate_sha256,
        subject=subject,
    )
    response["execution"] = _empty_section(
        execution_status,
        operation="native_execute_and_verify_candidate",
        state=state,
        subject_execution_suite=suite,
        subject_execution_suite_sha256=native["execution_suite_sha256"],
        recovered_from_immutable_evidence=(suite is None),
        transition_contexts=transition_contexts,
        native_verification=native["native_verification"],
    )
    response["regression"] = _empty_section(
        "passed" if regression["status"] == "complete" else "blocked",
        evidence=regression,
    )
    response["receipt"] = _empty_section(
        "verified" if execution_status == "passed" else "invalid",
        execution_suite_sha256=native["execution_suite_sha256"],
        execution_receipts=execution_receipts,
        recovered_from_immutable_evidence=(suite is None),
        runtime_source=suite_runtime_source,
    )
    response["release"] = _empty_section(
        "complete" if release["status"] == "complete" else "blocked",
        evidence=release,
    )
    response["learning"] = _learning(
        verdict="candidate",
        current_state="release_complete",
        completed_states=(
            "candidate",
            "proposed",
            "committed",
            "regression_passed",
            "release_complete",
        ),
        status="passed",
        binding=bound,
        candidate={"candidate_sha256": candidate_sha256},
        native_state=state,
    )
    response["command_verdict"] = (
        "passed"
        if execution_status == "passed"
        and regression["status"] == "complete"
        and release["status"] == "complete"
        else "blocked"
    )
    return response


def history(
    binding_path: Path | str,
    *,
    workspace: Path | str,
    candidate_sha256: Optional[str] = None,
) -> dict[str, Any]:
    """Return a verified, unbroken workspace history without changing it."""

    binding, _, _, response = _load_context("history", binding_path)
    _require_lifecycle_api(binding)
    if binding.target_kind != "workspace":
        raise BindingValidationError("history requires a refinery workspace binding")
    if candidate_sha256 is None or not _HEX64.fullmatch(candidate_sha256):
        raise BindingValidationError(
            "native history requires --candidate with an exact delta SHA-256"
        )
    events = runtime.native_refinery_history(
        str(workspace),
        delta_sha256=candidate_sha256,
        binding=_native_binding(binding),
    )
    if not events:
        raise BaselineMismatchError("native candidate history is empty")
    latest = events[-1]
    states = [str(item["state"]) for item in events]
    response["corpus_found"] = _empty_section(
        "found",
        workspace_id=binding.workspace_id,
        package_id=binding.package_id,
        candidate_sha256=candidate_sha256,
    )
    response["execution"] = _empty_section(
        "passed", operation="native_history", events=list(events)
    )
    response["regression"] = _empty_section(
        "passed" if "regression_passed" in states else "not_run",
        source="native immutable event chain",
    )
    response["receipt"] = _empty_section("bound", event_sha256=latest["event_sha256"])
    response["release"] = _empty_section(
        "complete"
        if "release_complete" in states or "promoted" in states
        else "blocked",
        source="native immutable event chain",
    )
    response["learning"] = _learning(
        verdict="candidate",
        current_state=str(latest["state"]),
        completed_states=tuple(states),
        status="passed",
        binding=binding,
        candidate={"candidate_sha256": candidate_sha256},
        native_state=latest,
    )
    response["command_verdict"] = "passed"
    return response


def promote(
    binding_path: Path | str,
    *,
    workspace: Path | str,
    task: Path | str,
    candidate_sha256: Optional[str] = None,
    authorization: Optional[Path | str] = None,
    recorded_at: Optional[str] = None,
) -> dict[str, Any]:
    """Promote only through the native refinery API; publication stays external."""

    binding, task_envelope, _, response = _load_context(
        "promote", binding_path, task_path=task, require_task=True
    )
    _require_lifecycle_api(binding)
    if binding.target_kind != "workspace":
        raise BindingValidationError("promote requires a refinery workspace binding")
    if task_envelope is None:
        raise BindingValidationError("native promote requires a task envelope")
    if candidate_sha256 is None or not _HEX64.fullmatch(candidate_sha256):
        raise BindingValidationError(
            "native promote requires --candidate with an exact delta SHA-256"
        )
    if authorization is None:
        raise BindingValidationError("native promote requires --authorization")
    authorization_value = _native_authorization_input(
        authorization,
        binding=binding,
        expected_action="promote",
        candidate_sha256=candidate_sha256,
    )
    native = runtime.native_refinery_promote_candidate(
        str(workspace),
        delta_sha256=candidate_sha256,
        authorization=authorization_value,
        envelope=_native_task(task_envelope, "promoted"),
        binding=_native_binding(binding),
        recorded_at=recorded_at,
    )
    descriptor = native["descriptor"]
    next_binding = _next_binding_projection(binding, descriptor)
    response["corpus_found"] = _empty_section(
        "found",
        workspace_id=binding.workspace_id,
        package_id=binding.package_id,
        candidate_sha256=candidate_sha256,
    )
    response["execution"] = _empty_section(
        "passed",
        operation="native_promote_candidate",
        descriptor=descriptor,
        transition_context=native["transition_context"],
    )
    response["regression"] = _empty_section(
        "passed", source="promotion descriptor binds regression evidence"
    )
    response["receipt"] = _empty_section(
        "verified",
        engagement_receipt_sha256=descriptor["engagement_receipt_sha256"],
        promotion_record_sha256=descriptor["promotion_record_sha256"],
    )
    response["release"] = _empty_section(
        "complete",
        release_evidence_sha256=descriptor["release_evidence_sha256"],
    )
    response["learning"] = _learning(
        verdict="candidate",
        current_state="promoted",
        completed_states=STATE_MODEL,
        status="passed",
        binding=binding,
        candidate={"candidate_sha256": candidate_sha256},
        promotion={
            "status": "promoted",
            "target": binding.promotion_target,
            "descriptor": descriptor,
        },
        next_binding=next_binding,
    )
    response["command_verdict"] = "passed"
    return response


def _error_response(command: str, exc: Exception) -> dict[str, Any]:
    try:
        identity = _runtime_identity()
    except Exception:
        identity = {}
    response = _envelope(command, runtime_identity=identity)
    response["transport"] = {
        "status": "completed",
        "exit_code_semantics": (
            "exit 0 confirms stable JSON delivery only; inspect every semantic section"
        ),
    }
    response["execution"] = _empty_section(
        "blocked",
        error_type=type(exc).__name__,
        message=str(exc),
    )
    response["learning"] = _learning(
        status="blocked", reason="semantic engagement failed closed"
    )
    response["diagnostics"] = [{"code": type(exc).__name__, "message": str(exc)}]
    return response


class _ArgumentParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise BindingValidationError(message)


def build_argument_parser() -> argparse.ArgumentParser:
    parser = _ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)

    command = commands.add_parser("doctor")
    command.add_argument("--binding")

    command = commands.add_parser("discover")
    command.add_argument("--binding")
    command.add_argument("--workspace")

    command = commands.add_parser("open")
    command.add_argument("--binding", required=True)
    command.add_argument("--task", required=True)
    command.add_argument("--workspace")
    command.add_argument("--engagement")

    command = commands.add_parser("run")
    command.add_argument("--binding", required=True)
    command.add_argument("--workspace")
    command.add_argument("--scenario")
    command.add_argument("--task")
    command.add_argument("--recorded-at")

    command = commands.add_parser("propose")
    command.add_argument("--binding", required=True)
    command.add_argument("--workspace", required=True)
    command.add_argument("--delta", required=True)
    command.add_argument("--task", required=True)
    command.add_argument("--engagement")
    command.add_argument("--recorded-at")

    command = commands.add_parser("commit")
    command.add_argument("--binding", required=True)
    command.add_argument("--workspace", required=True)
    command.add_argument("--candidate")
    command.add_argument("--authorization")
    command.add_argument("--task", required=True)
    command.add_argument("--recorded-at")

    command = commands.add_parser("verify")
    command.add_argument("--binding", required=True)
    command.add_argument("--workspace")
    command.add_argument("--scenario")
    command.add_argument("--task")
    command.add_argument("--candidate")
    command.add_argument("--recorded-at")

    command = commands.add_parser("history")
    command.add_argument("--binding", required=True)
    command.add_argument("--workspace", required=True)
    command.add_argument("--candidate")

    command = commands.add_parser("promote")
    command.add_argument("--binding", required=True)
    command.add_argument("--workspace", required=True)
    command.add_argument("--task", required=True)
    command.add_argument("--candidate")
    command.add_argument("--authorization")
    command.add_argument("--recorded-at")
    return parser


def _dispatch(args: argparse.Namespace) -> dict[str, Any]:
    if args.command == "doctor":
        return doctor(args.binding)
    if args.command == "discover":
        return discover(args.binding, workspace=args.workspace)
    if args.command == "open":
        return open_engagement(
            args.binding,
            task=args.task,
            workspace=args.workspace,
            engagement=args.engagement,
        )
    if args.command == "run":
        return run(
            args.binding,
            workspace=args.workspace,
            scenario_id=args.scenario,
            task=args.task,
            recorded_at=args.recorded_at,
        )
    if args.command == "propose":
        return propose(
            args.binding,
            workspace=args.workspace,
            delta=args.delta,
            task=args.task,
            engagement=args.engagement,
            recorded_at=args.recorded_at,
        )
    if args.command == "commit":
        return commit(
            args.binding,
            workspace=args.workspace,
            candidate_sha256=args.candidate,
            authorization=args.authorization,
            task=args.task,
            recorded_at=args.recorded_at,
        )
    if args.command == "verify":
        return verify(
            args.binding,
            workspace=args.workspace,
            scenario_id=args.scenario,
            task=args.task,
            candidate_sha256=args.candidate,
            recorded_at=args.recorded_at,
        )
    if args.command == "history":
        return history(
            args.binding,
            workspace=args.workspace,
            candidate_sha256=args.candidate,
        )
    return promote(
        args.binding,
        workspace=args.workspace,
        task=args.task,
        candidate_sha256=args.candidate,
        authorization=args.authorization,
        recorded_at=args.recorded_at,
    )


def main(argv: Optional[Sequence[str]] = None) -> int:
    """Emit one canonical response; semantic gates live in JSON, not exit zero."""

    command = "unknown"
    try:
        args = build_argument_parser().parse_args(argv)
        command = str(args.command)
        response = _dispatch(args)
    except (SemanticEngagementError, RuntimeError, OSError, ValueError) as exc:
        response = _error_response(command, exc)
    print(canonical_json(response))
    return 0


__all__ = [
    "ActionNotAllowedError",
    "AUTHORIZATION_SCHEMA",
    "BaselineMismatchError",
    "BINDING_SCHEMA",
    "BindingValidationError",
    "CapabilityUnavailableError",
    "NATIVE_REFINERY_CONTRACT",
    "KNOWN_ACTIONS",
    "ProjectBinding",
    "PROMOTION_TARGET",
    "RESPONSE_SCHEMA",
    "SemanticEngagementError",
    "STATE_MODEL",
    "TASK_SCHEMA",
    "SemanticTaskEnvelope",
    "build_argument_parser",
    "canonical_json",
    "commit",
    "discover",
    "doctor",
    "history",
    "main",
    "open_engagement",
    "promote",
    "propose",
    "read_project_binding",
    "read_task_envelope",
    "run",
    "verify",
]
