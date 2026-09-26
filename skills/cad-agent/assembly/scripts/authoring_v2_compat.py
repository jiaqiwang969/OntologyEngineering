#!/usr/bin/env python3
"""Validate the fail-closed source-to-Fusion assembly authoring manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any


SCHEMA = "cad-agent.assembly-authoring-manifest/v2"
CLAIM_ORDER = [
    "SOURCE_CLOSURE",
    "TREE_REPRODUCTION",
    "CONTENT_COMPLETENESS",
    "NATIVE_GEOMETRY",
    "PERSISTED_FUSION_ASSEMBLY",
    "PROCESS_AND_MOTION",
]
REQUIRED_CATEGORIES = {
    "STRUCTURAL",
    "FASTENER",
    "NUT",
    "WASHER_RING_KEY",
    "PIN_BEARING_SPACER",
    "PURCHASED_MODULE_ELECTRONICS",
    "HARNESS_FLEXIBLE",
    "OTHER_REQUIRED",
}
SOURCE_ROLES = {
    "ASSEMBLY_ROOT",
    "DEPENDENCY",
    "BOM",
    "SOP",
    "SUPPLIER_RECORD",
    "IMAGE_VIDEO_RECORD",
    "RIGHTS_RECORD",
}
CARRIER_CLASSES = {
    "STEP",
    "SLDASM",
    "SLDPRT",
    "F3D",
    "F3Z",
    "IGES",
    "PARASOLID",
    "ACIS",
    "BOM",
    "SOP",
    "IMAGE",
    "VIDEO",
    "TEXT_RECORD",
    "UNKNOWN",
}
RELEASE_INTENTS = {
    "ANALYSIS_ONLY",
    "PRACTICE_NATIVE_ASSEMBLY",
    "INTERNAL_ENGINEERING",
    "MANUFACTURING_HANDOFF",
    "PUBLIC_REDISTRIBUTION",
    "SYNTHETIC_TEST_ONLY",
}
RIGHTS_STATUSES = {
    "GRANTED_PUBLIC_REDISTRIBUTION",
    "GRANTED_PRIVATE_USE",
    "RESTRICTED",
    "DENIED",
    "UNKNOWN",
}
EVIDENCE_KINDS = {
    "SOURCE_FILE",
    "SOURCE_AUDIT",
    "TREE_AUDIT",
    "CONTENT_AUDIT",
    "GEOMETRY_AUDIT",
    "PLACEMENT_AUDIT",
    "FUSION_READBACK",
    "RIGHTS_RECORD",
    "PROCESS_RECORD",
    "SYNTHETIC_TEST",
    "OTHER",
}
ROW_STATUSES = {"PASS", "PASS_WITH_HOLDS", "HOLD", "UNKNOWN", "NOT_APPLICABLE"}
VERDICTS = {"PASS", "PASS_WITH_HOLDS", "HOLD", "UNKNOWN", "FAIL"}
QUANTITY_SEMANTICS = {
    "EXACT",
    "MINIMUM",
    "RANGE",
    "OPTIONAL",
    "PROCUREMENT_PACK",
    "UNKNOWN_UPPER",
}
OCCURRENCE_PRESENCES = {"EXACT", "PROXY", "MISSING", "UNKNOWN"}
GEOMETRY_CLASSES = {
    "NATIVE_PARAMETRIC",
    "IMPORTED_BREP",
    "SUPPLIER_EXACT_BREP",
    "ANALYTIC_DISPLAY_PROXY",
    "MESH_PROXY",
    "FACETED_BREP_PROXY",
    "SURFACE_ONLY",
    "EMPTY",
    "FLEXIBLE_REFERENCE_POSE",
}
NATIVE_GEOMETRY_CLASSES = {
    "NATIVE_PARAMETRIC",
    "IMPORTED_BREP",
    "SUPPLIER_EXACT_BREP",
}
PROXY_GEOMETRY_CLASSES = {
    "ANALYTIC_DISPLAY_PROXY",
    "MESH_PROXY",
    "FACETED_BREP_PROXY",
    "SURFACE_ONLY",
    "EMPTY",
    "FLEXIBLE_REFERENCE_POSE",
}
PROVENANCE_KINDS = {
    "SOURCE_NATIVE",
    "CONTROLLED_BREP_IMPORT",
    "SUPPLIER_BREP",
    "EVIDENCE_GROUNDED_NATIVE_MODEL",
    "ANALYTIC_PROXY",
    "TESSELLATED_PROXY",
    "FLEXIBLE_REFERENCE",
}
SOURCE_BOUND_PROVENANCE = {
    "SOURCE_NATIVE",
    "CONTROLLED_BREP_IMPORT",
    "SUPPLIER_BREP",
    "FLEXIBLE_REFERENCE",
}
PROVENANCE_BY_GEOMETRY_CLASS = {
    "NATIVE_PARAMETRIC": {"SOURCE_NATIVE", "EVIDENCE_GROUNDED_NATIVE_MODEL"},
    "IMPORTED_BREP": {"CONTROLLED_BREP_IMPORT"},
    "SUPPLIER_EXACT_BREP": {"SUPPLIER_BREP"},
    "ANALYTIC_DISPLAY_PROXY": {"ANALYTIC_PROXY"},
    "MESH_PROXY": {"TESSELLATED_PROXY"},
    "FACETED_BREP_PROXY": {"TESSELLATED_PROXY"},
    "SURFACE_ONLY": {"SOURCE_NATIVE", "CONTROLLED_BREP_IMPORT"},
    "EMPTY": {"SOURCE_NATIVE", "CONTROLLED_BREP_IMPORT"},
    "FLEXIBLE_REFERENCE_POSE": {"FLEXIBLE_REFERENCE"},
}
FILE_KINDS = {"ROOT", "EXTERNAL", "VIRTUAL_INTERNAL"}
FILE_RESOLUTION_STATUSES = {"RESOLVED", "UNRESOLVED"}
SOURCE_DEFINITION_KINDS = {
    "ASSEMBLY",
    "PART",
    "VIRTUAL_INTERNAL",
    "PURCHASED_MODULE",
    "PURCHASED_HARDWARE",
    "FLEXIBLE_ITEM",
}
REFERENCE_TYPES = {
    "NATIVE_REFERENCE",
    "EMBEDDED_VIRTUAL",
    "CONFIGURATION_REFERENCE",
    "SUPPRESSED_REFERENCE",
    "ENVELOPE_REFERENCE",
}
REFERENCE_STATUSES = {"RESOLVED", "UNRESOLVED", "SUPPRESSED", "EXCLUDED"}
RECONCILIATION_DISPOSITIONS = {
    "MATCHED",
    "SOURCE_ONLY_AUTHORIZED_EXCLUSION",
    "FUSION_ONLY_AUTHORIZED_ADDITION",
    "SOURCE_ONLY_UNRESOLVED",
    "FUSION_ONLY_UNRESOLVED",
}
RELATION_TYPES = {"RIGID_GROUP", "JOINT", "AS_BUILT_JOINT"}
HEX64 = re.compile(r"^[0-9a-fA-F]{64}$")


@dataclass
class ValidationState:
    errors: list[str] = field(default_factory=list)
    holds: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    evidence_registry: set[str] = field(default_factory=set)
    evidence_records: dict[str, dict[str, Any]] = field(default_factory=dict)
    referenced_evidence: set[str] = field(default_factory=set)

    def evidence_links(
        self, value: Any, label: str, *, required: bool = False
    ) -> list[str]:
        if not isinstance(value, list) or any(
            not isinstance(item, str) or not item for item in value
        ):
            self.errors.append(
                f"{label}: expected an array of non-empty logical evidence IDs"
            )
            return []
        if len(set(value)) != len(value):
            self.errors.append(f"{label}: duplicate evidence IDs are not allowed")
        missing = sorted(set(value) - self.evidence_registry)
        if missing:
            self.errors.append(f"{label}: unregistered evidence IDs {missing}")
        self.referenced_evidence.update(value)
        if required and not value:
            self.holds.append(f"{label}: evidence is required")
        return value


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def placeholder(value: Any) -> bool:
    if value is None:
        return True
    if not isinstance(value, str):
        return False
    normalized = value.strip().upper()
    return not normalized or "UNKNOWN" in normalized or "REPLACE" in normalized


def count_value(value: Any, label: str, state: ValidationState) -> int | None:
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        state.errors.append(f"{label}: expected a non-negative integer or null")
        return None
    return value


def object_value(
    parent: dict[str, Any], key: str, state: ValidationState
) -> dict[str, Any]:
    value = parent.get(key)
    if not isinstance(value, dict):
        state.errors.append(f"{key}: expected an object")
        return {}
    return value


def list_value(parent: dict[str, Any], key: str, state: ValidationState) -> list[Any]:
    value = parent.get(key)
    if not isinstance(value, list):
        state.errors.append(f"{key}: expected an array")
        return []
    return value


def require_known(value: Any, label: str, state: ValidationState) -> None:
    if placeholder(value):
        state.holds.append(f"{label}: missing or placeholder value")


def claim_enabled(claims: list[str], name: str) -> bool:
    return name in claims


def enum_member(value: Any, allowed: set[str]) -> bool:
    return isinstance(value, str) and value in allowed


def resolved_path(raw_path: Any, manifest_path: Path) -> Path | None:
    if placeholder(raw_path):
        return None
    path = Path(str(raw_path)).expanduser()
    return path if path.is_absolute() else manifest_path.parent / path


def valid_timestamp(value: Any) -> bool:
    if not isinstance(value, str) or placeholder(value):
        return False
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return False
    return parsed.tzinfo is not None


def valid_rigid_transform(value: Any, tolerance: float = 1e-6) -> bool:
    if (
        not isinstance(value, list)
        or len(value) != 16
        or any(
            isinstance(item, bool)
            or not isinstance(item, (int, float))
            or not math.isfinite(item)
            for item in value
        )
    ):
        return False
    if any(
        abs(value[index] - expected) > tolerance
        for index, expected in zip((12, 13, 14, 15), (0, 0, 0, 1))
    ):
        return False
    rotation = (
        (value[0], value[1], value[2]),
        (value[4], value[5], value[6]),
        (value[8], value[9], value[10]),
    )
    for row in rotation:
        if abs(sum(component * component for component in row) - 1.0) > tolerance:
            return False
    for left, right in ((0, 1), (0, 2), (1, 2)):
        if (
            abs(sum(rotation[left][axis] * rotation[right][axis] for axis in range(3)))
            > tolerance
        ):
            return False
    determinant = (
        rotation[0][0]
        * (rotation[1][1] * rotation[2][2] - rotation[1][2] * rotation[2][1])
        - rotation[0][1]
        * (rotation[1][0] * rotation[2][2] - rotation[1][2] * rotation[2][0])
        + rotation[0][2]
        * (rotation[1][0] * rotation[2][1] - rotation[1][1] * rotation[2][0])
    )
    return abs(determinant - 1.0) <= tolerance


def validate_evidence_registry(
    document: dict[str, Any],
    manifest_path: Path,
    check_files: bool,
    state: ValidationState,
) -> None:
    registry = list_value(document, "evidence_registry", state)
    for index, raw in enumerate(registry):
        label = f"evidence_registry[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        evidence_id = raw.get("evidence_id")
        if not isinstance(evidence_id, str) or not evidence_id:
            state.errors.append(f"{label}.evidence_id: expected a non-empty string")
            continue
        if evidence_id in state.evidence_registry:
            state.errors.append(f"{label}.evidence_id: duplicate {evidence_id!r}")
        else:
            state.evidence_records[evidence_id] = raw
        state.evidence_registry.add(evidence_id)
        kind = raw.get("kind")
        if not enum_member(kind, EVIDENCE_KINDS):
            state.errors.append(f"{label}.kind: unsupported value {kind!r}")
        require_known(raw.get("path"), f"{label}.path", state)
        evidence_hash = raw.get("sha256")
        if placeholder(evidence_hash):
            state.holds.append(f"{label}.sha256: exact hash missing")
        elif not isinstance(evidence_hash, str) or not HEX64.fullmatch(evidence_hash):
            state.errors.append(f"{label}.sha256: expected 64 hexadecimal characters")
        require_known(raw.get("media_type"), f"{label}.media_type", state)
        captured_at = raw.get("captured_at")
        if not valid_timestamp(captured_at):
            if placeholder(captured_at):
                state.holds.append(
                    f"{label}.captured_at: timezone-aware timestamp missing"
                )
            else:
                state.errors.append(
                    f"{label}.captured_at: expected an ISO-8601 timestamp with timezone"
                )
        if check_files:
            path = resolved_path(raw.get("path"), manifest_path)
            if path is None:
                continue
            if not path.is_file():
                state.errors.append(f"{label}.path: file not found: {path}")
            elif isinstance(evidence_hash, str) and HEX64.fullmatch(evidence_hash):
                observed_hash = sha256(path)
                if observed_hash.lower() != evidence_hash.lower():
                    state.errors.append(
                        f"{label}.sha256: {observed_hash} != {evidence_hash.lower()}"
                    )
    if not registry:
        state.holds.append("evidence_registry: no controlled evidence records")


def validate_sources(
    document: dict[str, Any],
    manifest_path: Path,
    check_files: bool,
    release_intent: Any,
    state: ValidationState,
) -> dict[str, dict[str, Any]]:
    sources = list_value(document, "sources", state)
    source_records: dict[str, dict[str, Any]] = {}
    root_ids: list[str] = []
    for index, raw in enumerate(sources):
        label = f"sources[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        source_id = raw.get("source_id")
        if not isinstance(source_id, str) or not source_id:
            state.errors.append(f"{label}.source_id: expected a non-empty string")
            continue
        if source_id in source_records:
            state.errors.append(f"{label}.source_id: duplicate {source_id!r}")
        source_records[source_id] = raw
        role = raw.get("role")
        if not enum_member(role, SOURCE_ROLES):
            state.errors.append(f"{label}.role: unsupported value {role!r}")
        if role == "ASSEMBLY_ROOT":
            root_ids.append(source_id)
        carrier = raw.get("carrier_class")
        if not enum_member(carrier, CARRIER_CLASSES):
            state.errors.append(f"{label}.carrier_class: unsupported value {carrier!r}")
        require_known(carrier, f"{label}.carrier_class", state)
        source_hash = raw.get("sha256")
        if placeholder(source_hash):
            state.holds.append(f"{label}.sha256: exact hash missing")
        elif not isinstance(source_hash, str) or not HEX64.fullmatch(source_hash):
            state.errors.append(f"{label}.sha256: expected 64 hexadecimal characters")
        source_size = count_value(raw.get("size_bytes"), f"{label}.size_bytes", state)
        if source_size is None:
            state.holds.append(f"{label}.size_bytes: exact byte size missing")
        for field_name in ("path", "revision", "authority"):
            require_known(raw.get(field_name), f"{label}.{field_name}", state)
        rights = raw.get("rights_status")
        if not enum_member(rights, RIGHTS_STATUSES):
            state.errors.append(f"{label}.rights_status: unsupported value {rights!r}")
        elif rights == "UNKNOWN":
            state.holds.append(f"{label}.rights_status: rights are unknown")
        elif rights == "DENIED":
            state.holds.append(f"{label}.rights_status: use is denied")
        state.evidence_links(
            raw.get("rights_evidence_ids"),
            f"{label}.rights_evidence_ids",
            required=True,
        )
        if (
            release_intent == "PUBLIC_REDISTRIBUTION"
            and rights != "GRANTED_PUBLIC_REDISTRIBUTION"
        ):
            state.holds.append(
                f"{label}.rights_status: {rights!r} does not authorize public redistribution"
            )
        if check_files:
            path = resolved_path(raw.get("path"), manifest_path)
            if path is None:
                continue
            if not path.is_file():
                state.errors.append(f"{label}.path: file not found: {path}")
                continue
            expected_size = raw.get("size_bytes")
            if isinstance(expected_size, int) and not isinstance(expected_size, bool):
                observed_size = path.stat().st_size
                if observed_size != expected_size:
                    state.errors.append(
                        f"{label}.size_bytes: {observed_size} != {expected_size}"
                    )
            if isinstance(source_hash, str) and HEX64.fullmatch(source_hash):
                observed_hash = sha256(path)
                if observed_hash.lower() != source_hash.lower():
                    state.errors.append(
                        f"{label}.sha256: {observed_hash} != {source_hash.lower()}"
                    )
    if not sources:
        state.holds.append("sources: no controlled source records")
    if len(root_ids) != 1:
        state.errors.append(
            f"sources: expected exactly one ASSEMBLY_ROOT, observed {len(root_ids)}"
        )
    selected = object_value(document, "selected_root", state)
    selected_id = selected.get("source_id")
    if not isinstance(selected_id, str) or selected_id not in source_records:
        state.errors.append(f"selected_root.source_id: unknown source {selected_id!r}")
    if root_ids and selected_id != root_ids[0]:
        state.errors.append(
            "selected_root.source_id: must select the sole ASSEMBLY_ROOT"
        )
    require_known(selected.get("configuration"), "selected_root.configuration", state)
    state.evidence_links(
        selected.get("selection_basis_evidence_ids"),
        "selected_root.selection_basis_evidence_ids",
        required=True,
    )
    return source_records


def validate_source_closure(
    document: dict[str, Any],
    claims: list[str],
    source_records: dict[str, dict[str, Any]],
    state: ValidationState,
) -> tuple[dict[str, int | None], dict[str, dict[str, Any]]]:
    closure = object_value(document, "source_closure", state)
    values = {
        key: count_value(closure.get(key), f"source_closure.{key}", state)
        for key in (
            "expected_external_file_count",
            "resolved_external_file_count",
            "unresolved_reference_count",
            "definition_count",
            "saved_occurrence_count",
            "expanded_occurrence_count",
        )
    }
    file_records = list_value(closure, "file_records", state)
    files: dict[str, dict[str, Any]] = {}
    roots: list[str] = []
    bound_disk_sources: set[str] = set()
    for index, raw in enumerate(file_records):
        label = f"source_closure.file_records[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        file_id = raw.get("file_id")
        if not isinstance(file_id, str) or not file_id:
            state.errors.append(f"{label}.file_id: expected a non-empty string")
            continue
        if file_id in files:
            state.errors.append(f"{label}.file_id: duplicate {file_id!r}")
        files[file_id] = raw
        kind = raw.get("file_kind")
        if not enum_member(kind, FILE_KINDS):
            state.errors.append(f"{label}.file_kind: unsupported value {kind!r}")
        if kind == "ROOT":
            roots.append(file_id)
        resolution = raw.get("resolution_status")
        if not enum_member(resolution, FILE_RESOLUTION_STATUSES):
            state.errors.append(
                f"{label}.resolution_status: unsupported value {resolution!r}"
            )
        if enum_member(kind, {"ROOT", "VIRTUAL_INTERNAL"}) and resolution != "RESOLVED":
            state.errors.append(f"{label}: {kind} records must be RESOLVED")
        source_id = raw.get("source_id")
        if resolution == "RESOLVED":
            if not isinstance(source_id, str) or source_id not in source_records:
                state.errors.append(f"{label}.source_id: unknown source {source_id!r}")
            elif enum_member(kind, {"ROOT", "EXTERNAL"}):
                if source_id in bound_disk_sources:
                    state.errors.append(
                        f"{label}.source_id: disk source {source_id!r} is bound more than once"
                    )
                bound_disk_sources.add(source_id)
        elif source_id is not None:
            state.errors.append(f"{label}.source_id: unresolved records must use null")
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
    if len(roots) != 1:
        state.errors.append(
            f"source_closure.file_records: expected one ROOT, observed {len(roots)}"
        )
    selected_root = document.get("selected_root")
    selected_root_id = (
        selected_root.get("source_id") if isinstance(selected_root, dict) else None
    )
    if len(roots) == 1 and files[roots[0]].get("source_id") != selected_root_id:
        state.errors.append(
            "source_closure.file_records: ROOT must bind selected_root.source_id"
        )

    edges = list_value(closure, "reference_edges", state)
    edge_ids: set[str] = set()
    inbound: dict[str, int] = {file_id: 0 for file_id in files}
    adjacency: dict[str, set[str]] = {file_id: set() for file_id in files}
    for index, raw in enumerate(edges):
        label = f"source_closure.reference_edges[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        edge_id = raw.get("edge_id")
        if not isinstance(edge_id, str) or not edge_id:
            state.errors.append(f"{label}.edge_id: expected a non-empty string")
        elif edge_id in edge_ids:
            state.errors.append(f"{label}.edge_id: duplicate {edge_id!r}")
        else:
            edge_ids.add(edge_id)
        parent_id = raw.get("parent_file_id")
        child_id = raw.get("child_file_id")
        if not isinstance(parent_id, str) or parent_id not in files:
            state.errors.append(f"{label}.parent_file_id: unknown file {parent_id!r}")
        if not isinstance(child_id, str) or child_id not in files:
            state.errors.append(f"{label}.child_file_id: unknown file {child_id!r}")
        if (
            isinstance(parent_id, str)
            and isinstance(child_id, str)
            and parent_id in files
            and child_id in files
        ):
            inbound[child_id] += 1
            adjacency[parent_id].add(child_id)
        reference_type = raw.get("reference_type")
        if not enum_member(reference_type, REFERENCE_TYPES):
            state.errors.append(
                f"{label}.reference_type: unsupported value {reference_type!r}"
            )
        status = raw.get("resolution_status")
        if not enum_member(status, REFERENCE_STATUSES):
            state.errors.append(
                f"{label}.resolution_status: unsupported value {status!r}"
            )
        if isinstance(child_id, str) and child_id in files:
            child_status = files[child_id].get("resolution_status")
            if status == "RESOLVED" and child_status != "RESOLVED":
                state.errors.append(
                    f"{label}: RESOLVED edge must point to a resolved file record"
                )
            if status == "UNRESOLVED" and child_status != "UNRESOLVED":
                state.errors.append(
                    f"{label}: UNRESOLVED edge must point to an unresolved file record"
                )
        if enum_member(status, {"UNRESOLVED", "SUPPRESSED", "EXCLUDED"}):
            require_known(raw.get("rationale"), f"{label}.rationale", state)
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
    root_id = roots[0] if len(roots) == 1 else None
    if root_id is not None:
        if inbound[root_id]:
            state.errors.append(
                "source_closure.reference_edges: ROOT must not have an inbound edge"
            )
        for file_id in sorted(set(files) - {root_id}):
            if inbound[file_id] == 0:
                state.errors.append(
                    f"source_closure.reference_edges: file {file_id!r} has no inbound reference edge"
                )
        reachable = {root_id}
        frontier = [root_id]
        while frontier:
            parent = frontier.pop()
            for child in adjacency[parent] - reachable:
                reachable.add(child)
                frontier.append(child)
        unreachable = sorted(set(files) - reachable)
        if unreachable:
            state.errors.append(
                f"source_closure.file_records: unreachable records {unreachable}"
            )

    definition_records = list_value(closure, "definition_records", state)
    source_definitions: dict[str, dict[str, Any]] = {}
    for index, raw in enumerate(definition_records):
        label = f"source_closure.definition_records[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        definition_id = raw.get("source_definition_id")
        if not isinstance(definition_id, str) or not definition_id:
            state.errors.append(
                f"{label}.source_definition_id: expected a non-empty string"
            )
            continue
        if definition_id in source_definitions:
            state.errors.append(
                f"{label}.source_definition_id: duplicate {definition_id!r}"
            )
        definition_kind = raw.get("definition_kind")
        if not enum_member(definition_kind, SOURCE_DEFINITION_KINDS):
            state.errors.append(
                f"{label}.definition_kind: unsupported value {definition_kind!r}"
            )
        file_id = raw.get("file_id")
        if not isinstance(file_id, str) or file_id not in files:
            state.errors.append(f"{label}.file_id: unknown file {file_id!r}")
            bound_source_id = None
        else:
            file_record = files[file_id]
            if file_record.get("resolution_status") != "RESOLVED":
                state.errors.append(
                    f"{label}.file_id: definition cannot bind an unresolved file"
                )
            bound_source_id = file_record.get("source_id")
            if not isinstance(bound_source_id, str):
                bound_source_id = None
        source_definitions[definition_id] = {
            "source_id": bound_source_id,
            "definition_kind": definition_kind,
        }
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
    if values["definition_count"] is not None and values["definition_count"] != len(
        source_definitions
    ):
        state.errors.append(
            "source_closure.definition_count: declared "
            f"{values['definition_count']} != definition ledger {len(source_definitions)}"
        )

    expected_derived = sum(raw.get("file_kind") == "EXTERNAL" for raw in files.values())
    resolved_derived = sum(
        raw.get("file_kind") == "EXTERNAL"
        and raw.get("resolution_status") == "RESOLVED"
        for raw in files.values()
    )
    unresolved_derived = expected_derived - resolved_derived
    for key, derived in (
        ("expected_external_file_count", expected_derived),
        ("resolved_external_file_count", resolved_derived),
        ("unresolved_reference_count", unresolved_derived),
    ):
        if values[key] is not None and values[key] != derived:
            state.errors.append(
                f"source_closure.{key}: declared {values[key]} != ledger {derived}"
            )
    disk_source_ids = {
        source_id
        for source_id, raw in source_records.items()
        if enum_member(raw.get("role"), {"ASSEMBLY_ROOT", "DEPENDENCY"})
    }
    if bound_disk_sources != disk_source_ids:
        missing = sorted(disk_source_ids - bound_disk_sources)
        extra = sorted(bound_disk_sources - disk_source_ids)
        state.errors.append(
            "source_closure.file_records: source binding mismatch "
            f"(missing={missing}, extra={extra})"
        )
    if claim_enabled(claims, "SOURCE_CLOSURE"):
        for key in (
            "expected_external_file_count",
            "resolved_external_file_count",
            "unresolved_reference_count",
        ):
            if values[key] is None:
                state.holds.append(f"source_closure.{key}: ledger total not recorded")
        if not files:
            state.holds.append(
                "source_closure.file_records: no per-file closure ledger"
            )
        if unresolved_derived:
            state.holds.append(
                f"source_closure: {unresolved_derived} unresolved external file records"
            )
        state.evidence_links(
            closure.get("evidence_ids"), "source_closure.evidence_ids", required=True
        )
    if claim_enabled(claims, "TREE_REPRODUCTION"):
        for key in (
            "definition_count",
            "saved_occurrence_count",
            "expanded_occurrence_count",
        ):
            value = values[key]
            if value is None or value <= 0:
                state.holds.append(
                    f"source_closure.{key}: positive count not established"
                )
        saved = values["saved_occurrence_count"]
        expanded = values["expanded_occurrence_count"]
        if saved is not None and expanded is not None and expanded < saved:
            state.errors.append(
                "source_closure: expanded occurrence count is smaller than saved occurrence count"
            )
        if closure.get("independent_tree_crosscheck") != "PASS":
            state.holds.append(
                "source_closure.independent_tree_crosscheck: PASS not established"
            )
        if not source_definitions:
            state.holds.append(
                "source_closure.definition_records: no source-definition ledger"
            )
    return values, source_definitions


def validate_quantity(
    raw: dict[str, Any], label: str, semantics: Any, state: ValidationState
) -> tuple[int | None, int | None, int | None, int | None]:
    quantity = raw.get("quantity")
    if not isinstance(quantity, dict):
        state.errors.append(f"{label}.quantity: expected an object")
        return None, None, None, None
    lower = count_value(
        quantity.get("lower_bound"), f"{label}.quantity.lower_bound", state
    )
    upper = count_value(
        quantity.get("upper_bound"), f"{label}.quantity.upper_bound", state
    )
    selected = count_value(
        quantity.get("selected_installed_count"),
        f"{label}.quantity.selected_installed_count",
        state,
    )
    pack = count_value(
        quantity.get("procurement_pack_size"),
        f"{label}.quantity.procurement_pack_size",
        state,
    )
    if semantics == "EXACT":
        if selected is None or lower != selected or upper != selected or selected <= 0:
            state.errors.append(
                f"{label}.quantity: EXACT requires equal positive lower, upper, and selected counts"
            )
        if pack is not None:
            state.errors.append(
                f"{label}.quantity: EXACT must not declare procurement_pack_size"
            )
    elif semantics == "MINIMUM":
        if lower is None or lower <= 0 or upper is not None:
            state.errors.append(
                f"{label}.quantity: MINIMUM requires positive lower and null upper"
            )
        if selected is not None and lower is not None and selected < lower:
            state.errors.append(
                f"{label}.quantity: selected count is below the minimum"
            )
        if pack is not None:
            state.errors.append(
                f"{label}.quantity: MINIMUM must not declare procurement_pack_size"
            )
    elif semantics == "RANGE":
        if lower is None or upper is None or upper < lower:
            state.errors.append(f"{label}.quantity: RANGE requires lower <= upper")
        if selected is not None and lower is not None and upper is not None:
            if not lower <= selected <= upper:
                state.errors.append(
                    f"{label}.quantity: selected count lies outside the range"
                )
        if pack is not None:
            state.errors.append(
                f"{label}.quantity: RANGE must not declare procurement_pack_size"
            )
    elif semantics == "OPTIONAL":
        if lower != 0 or upper is None or upper <= 0 or selected is None:
            state.errors.append(
                f"{label}.quantity: OPTIONAL requires lower=0, positive upper, and selected count"
            )
        elif selected > upper:
            state.errors.append(
                f"{label}.quantity: selected optional count exceeds upper"
            )
        if pack is not None:
            state.errors.append(
                f"{label}.quantity: OPTIONAL must not declare procurement_pack_size"
            )
    elif semantics == "PROCUREMENT_PACK":
        if any(value is not None for value in (lower, upper, selected)) or pack in (
            None,
            0,
        ):
            state.errors.append(
                f"{label}.quantity: PROCUREMENT_PACK requires only a positive pack size"
            )
    elif semantics == "UNKNOWN_UPPER":
        if (
            lower is None
            or upper is not None
            or selected is not None
            or pack is not None
        ):
            state.errors.append(
                f"{label}.quantity: UNKNOWN_UPPER requires a lower bound and null upper/selected/pack"
            )
    return lower, upper, selected, pack


def validate_completeness(
    document: dict[str, Any], claims: list[str], state: ValidationState
) -> tuple[dict[str, int], dict[str, dict[str, Any]]]:
    completeness = object_value(document, "completeness", state)
    if completeness.get("procurement_package_is_not_installed_count") is not True:
        state.errors.append(
            "completeness: procurement packages must not be treated as installed counts"
        )
    expected_total = count_value(
        completeness.get("expected_physical_occurrence_count"),
        "completeness.expected_physical_occurrence_count",
        state,
    )
    requirements = list_value(completeness, "requirements", state)
    requirement_ids: set[str] = set()
    occurrences: dict[str, dict[str, Any]] = {}
    derived = {
        category: {"expected": 0, "exact": 0, "proxy": 0, "missing": 0, "unknown": 0}
        for category in REQUIRED_CATEGORIES
    }
    selected_counts_known = True
    for index, raw in enumerate(requirements):
        label = f"completeness.requirements[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        requirement_id = raw.get("requirement_id")
        if not isinstance(requirement_id, str) or not requirement_id:
            state.errors.append(f"{label}.requirement_id: expected a non-empty string")
        elif requirement_id in requirement_ids:
            state.errors.append(f"{label}.requirement_id: duplicate {requirement_id!r}")
        else:
            requirement_ids.add(requirement_id)
        category = raw.get("category")
        if not enum_member(category, REQUIRED_CATEGORIES):
            state.errors.append(f"{label}.category: unsupported value {category!r}")
            category = None
        require_known(raw.get("item_identity"), f"{label}.item_identity", state)
        semantics = raw.get("quantity_semantics")
        if not enum_member(semantics, QUANTITY_SEMANTICS):
            state.errors.append(
                f"{label}.quantity_semantics: unsupported value {semantics!r}"
            )
        lower, upper, selected, _ = validate_quantity(raw, label, semantics, state)
        status = raw.get("status")
        if not enum_member(status, ROW_STATUSES):
            state.errors.append(f"{label}.status: unsupported value {status!r}")
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
        occurrence_rows = raw.get("occurrences")
        if not isinstance(occurrence_rows, list):
            state.errors.append(f"{label}.occurrences: expected an array")
            occurrence_rows = []
        if semantics == "PROCUREMENT_PACK":
            if occurrence_rows:
                state.errors.append(
                    f"{label}: procurement-pack rows cannot contain installed occurrences"
                )
            if status != "NOT_APPLICABLE":
                state.errors.append(
                    f"{label}.status: PROCUREMENT_PACK must be NOT_APPLICABLE"
                )
            require_known(raw.get("rationale"), f"{label}.rationale", state)
            continue
        if semantics == "UNKNOWN_UPPER":
            selected_counts_known = False
            if claim_enabled(claims, "CONTENT_COMPLETENESS"):
                state.holds.append(
                    f"{label}: UNKNOWN_UPPER blocks complete-content PASS"
                )
        elif selected is None:
            selected_counts_known = False
            if claim_enabled(claims, "CONTENT_COMPLETENESS"):
                state.holds.append(
                    f"{label}: selected installed count is not established"
                )
        if selected is not None and len(occurrence_rows) != selected:
            state.errors.append(
                f"{label}.occurrences: row count {len(occurrence_rows)} != selected count {selected}"
            )
        if selected is None and enum_member(semantics, {"MINIMUM", "UNKNOWN_UPPER"}):
            if lower is not None and len(occurrence_rows) < lower:
                state.errors.append(
                    f"{label}.occurrences: known rows are below the declared lower bound"
                )
        if selected is None and semantics == "RANGE":
            if (
                lower is not None
                and upper is not None
                and not lower <= len(occurrence_rows) <= upper
            ):
                state.errors.append(
                    f"{label}.occurrences: known rows lie outside the declared range"
                )
        requirement_counts = {"exact": 0, "proxy": 0, "missing": 0, "unknown": 0}
        for occurrence_index, occurrence in enumerate(occurrence_rows):
            occurrence_label = f"{label}.occurrences[{occurrence_index}]"
            if not isinstance(occurrence, dict):
                state.errors.append(f"{occurrence_label}: expected an object")
                continue
            occurrence_id = occurrence.get("occurrence_id")
            if not isinstance(occurrence_id, str) or not occurrence_id:
                state.errors.append(
                    f"{occurrence_label}.occurrence_id: expected a non-empty string"
                )
                continue
            if occurrence_id in occurrences:
                state.errors.append(
                    f"{occurrence_label}.occurrence_id: duplicate {occurrence_id!r}"
                )
            occurrences[occurrence_id] = occurrence
            presence = occurrence.get("presence")
            if not enum_member(presence, OCCURRENCE_PRESENCES):
                state.errors.append(
                    f"{occurrence_label}.presence: unsupported value {presence!r}"
                )
            else:
                requirement_counts[presence.lower()] += 1
            state.evidence_links(
                occurrence.get("evidence_ids"),
                f"{occurrence_label}.evidence_ids",
                required=True,
            )
            fusion_id = occurrence.get("fusion_occurrence_id")
            geometry_id = occurrence.get("geometry_definition_id")
            placement_id = occurrence.get("placement_contract_id")
            if enum_member(presence, {"EXACT", "PROXY"}):
                for field_name, value in (
                    ("fusion_occurrence_id", fusion_id),
                    ("geometry_definition_id", geometry_id),
                    ("placement_contract_id", placement_id),
                ):
                    if not isinstance(value, str) or not value:
                        state.errors.append(
                            f"{occurrence_label}.{field_name}: required for present occurrence"
                        )
            elif any(
                value is not None for value in (fusion_id, geometry_id, placement_id)
            ):
                state.errors.append(
                    f"{occurrence_label}: missing/unknown occurrence cannot bind Fusion geometry or placement"
                )
        if category is not None:
            derived[category]["expected"] += (
                selected if selected is not None else len(occurrence_rows)
            )
            for key, value in requirement_counts.items():
                derived[category][key] += value
        if status == "PASS" and any(
            requirement_counts[key] for key in ("proxy", "missing", "unknown")
        ):
            state.errors.append(
                f"{label}: PASS cannot contain proxy, missing, or unknown occurrences"
            )
        if status == "NOT_APPLICABLE":
            if not enum_member(
                semantics, {"OPTIONAL", "PROCUREMENT_PACK"}
            ) or selected not in (
                None,
                0,
            ):
                state.errors.append(
                    f"{label}: NOT_APPLICABLE is limited to uninstalled OPTIONAL or PROCUREMENT_PACK rows"
                )
            require_known(raw.get("rationale"), f"{label}.rationale", state)
        if claim_enabled(claims, "CONTENT_COMPLETENESS") and enum_member(
            status, {"PASS_WITH_HOLDS", "HOLD", "UNKNOWN"}
        ):
            state.holds.append(f"{label}.status: {status} blocks complete-content PASS")

    categories = list_value(completeness, "categories", state)
    observed_categories: set[str] = set()
    for index, raw in enumerate(categories):
        label = f"completeness.categories[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        category = raw.get("category")
        if not enum_member(category, REQUIRED_CATEGORIES):
            state.errors.append(f"{label}.category: unsupported value {category!r}")
            continue
        if category in observed_categories:
            state.errors.append(f"{label}.category: duplicate {category!r}")
        observed_categories.add(category)
        status = raw.get("status")
        if not enum_member(status, ROW_STATUSES):
            state.errors.append(f"{label}.status: unsupported value {status!r}")
        row_counts = {
            "expected": count_value(
                raw.get("expected_count"), f"{label}.expected_count", state
            ),
            "exact": count_value(
                raw.get("present_exact_count"), f"{label}.present_exact_count", state
            ),
            "proxy": count_value(
                raw.get("present_proxy_count"), f"{label}.present_proxy_count", state
            ),
            "missing": count_value(
                raw.get("missing_count"), f"{label}.missing_count", state
            ),
            "unknown": count_value(
                raw.get("unknown_count"), f"{label}.unknown_count", state
            ),
        }
        for key, value in row_counts.items():
            if value is not None and value != derived[category][key]:
                state.errors.append(
                    f"{label}.{key}_count: declared {value} != requirement ledger {derived[category][key]}"
                )
        if all(value is not None for value in row_counts.values()):
            if row_counts["expected"] != sum(
                row_counts[key] for key in ("exact", "proxy", "missing", "unknown")
            ):
                state.errors.append(
                    f"{label}: expected != exact + proxy + missing + unknown"
                )
        if status == "NOT_APPLICABLE":
            if any(value not in (None, 0) for value in row_counts.values()):
                state.errors.append(f"{label}: NOT_APPLICABLE requires all zero counts")
            require_known(
                raw.get("not_applicable_rationale"),
                f"{label}.not_applicable_rationale",
                state,
            )
            state.evidence_links(
                raw.get("not_applicable_evidence_ids"),
                f"{label}.not_applicable_evidence_ids",
                required=True,
            )
        else:
            state.evidence_links(
                raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
            )
        if status == "PASS" and any(
            row_counts[key] not in (None, 0) for key in ("proxy", "missing", "unknown")
        ):
            state.errors.append(
                f"{label}: PASS cannot contain proxy, missing, or unknown occurrences"
            )
        if status == "PASS" and row_counts["expected"] == 0:
            state.errors.append(
                f"{label}: zero-count category must use evidence-backed NOT_APPLICABLE"
            )
        if claim_enabled(claims, "CONTENT_COMPLETENESS") and enum_member(
            status, {"PASS_WITH_HOLDS", "HOLD", "UNKNOWN"}
        ):
            state.holds.append(f"{label}.status: {status} blocks complete-content PASS")
    missing_categories = sorted(REQUIRED_CATEGORIES - observed_categories)
    if missing_categories:
        state.errors.append(
            f"completeness.categories: missing required categories {missing_categories}"
        )
    sums = {
        key: sum(derived[category][key] for category in REQUIRED_CATEGORIES)
        for key in ("expected", "exact", "proxy", "missing", "unknown")
    }
    if expected_total is None:
        if claim_enabled(claims, "CONTENT_COMPLETENESS"):
            state.holds.append(
                "completeness.expected_physical_occurrence_count: total not established"
            )
    elif selected_counts_known and expected_total != sums["expected"]:
        state.errors.append(
            "completeness.expected_physical_occurrence_count: declared "
            f"{expected_total} != requirement ledger {sums['expected']}"
        )
    unresolved_items = list_value(completeness, "unresolved_items", state)
    for index, raw in enumerate(unresolved_items):
        label = f"completeness.unresolved_items[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        require_known(raw.get("item_id"), f"{label}.item_id", state)
        require_known(raw.get("reason"), f"{label}.reason", state)
        require_known(raw.get("next_evidence"), f"{label}.next_evidence", state)
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
    if claim_enabled(claims, "CONTENT_COMPLETENESS"):
        if not requirements:
            state.holds.append(
                "completeness.requirements: no content requirement ledger"
            )
        if sums["missing"]:
            state.holds.append(
                f"completeness: {sums['missing']} required occurrences missing"
            )
        if sums["unknown"]:
            state.holds.append(
                f"completeness: {sums['unknown']} occurrence counts unknown"
            )
        if sums["proxy"]:
            state.holds.append(
                f"completeness: {sums['proxy']} occurrences are proxy-only"
            )
        if unresolved_items:
            state.holds.append(
                f"completeness.unresolved_items: {len(unresolved_items)} unresolved items retained"
            )
    return sums, occurrences


def validate_geometry(
    document: dict[str, Any],
    claims: list[str],
    release_intent: Any,
    proxy_count: int,
    source_records: dict[str, dict[str, Any]],
    source_definitions: dict[str, dict[str, Any]],
    content_occurrences: dict[str, dict[str, Any]],
    state: ValidationState,
) -> tuple[dict[str, dict[str, Any]], dict[str, int]]:
    policy = object_value(document, "geometry_policy", state)
    accepted = policy.get("accepted_native_classes")
    if not isinstance(accepted, list) or not accepted:
        state.errors.append(
            "geometry_policy.accepted_native_classes: expected a non-empty array"
        )
        accepted = []
    else:
        if any(not isinstance(item, str) for item in accepted):
            state.errors.append(
                "geometry_policy.accepted_native_classes: every value must be a string"
            )
            accepted = [item for item in accepted if isinstance(item, str)]
        if len(set(accepted)) != len(accepted):
            state.errors.append(
                "geometry_policy.accepted_native_classes: duplicate values"
            )
    unsupported = sorted(set(accepted) - NATIVE_GEOMETRY_CLASSES)
    if unsupported:
        state.errors.append(
            "geometry_policy.accepted_native_classes: only the strict native enum is allowed; "
            f"unsupported {unsupported}"
        )
    if policy.get("triangle_mesh_delivery_allowed") is not False:
        state.errors.append(
            "geometry_policy.triangle_mesh_delivery_allowed: must be false for this manifest"
        )
    if policy.get("proxy_claim_limit") != "DISPLAY_ONLY":
        state.errors.append("geometry_policy.proxy_claim_limit: must be DISPLAY_ONLY")
    if policy.get("supplier_exact_required_for_manufacturing") is not True:
        state.errors.append(
            "geometry_policy: manufacturing claims require supplier-exact purchased geometry"
        )
    if policy.get("mesh_to_brep_selfcheck_required") is not True:
        state.errors.append(
            "geometry_policy: mesh-derived results require exact B-Rep self-check"
        )

    records = list_value(document, "geometry_definitions", state)
    definitions: dict[str, dict[str, Any]] = {}
    totals = {"brep": 0, "mesh": 0, "surface_or_empty": 0}
    for index, raw in enumerate(records):
        label = f"geometry_definitions[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        definition_id = raw.get("definition_id")
        if not isinstance(definition_id, str) or not definition_id:
            state.errors.append(f"{label}.definition_id: expected a non-empty string")
            continue
        if definition_id in definitions:
            state.errors.append(f"{label}.definition_id: duplicate {definition_id!r}")
        definitions[definition_id] = raw
        geometry_class = raw.get("geometry_class")
        if not enum_member(geometry_class, GEOMETRY_CLASSES):
            state.errors.append(
                f"{label}.geometry_class: unsupported value {geometry_class!r}"
            )
        provenance = raw.get("provenance_kind")
        if not enum_member(provenance, PROVENANCE_KINDS):
            state.errors.append(
                f"{label}.provenance_kind: unsupported value {provenance!r}"
            )
        elif enum_member(geometry_class, set(PROVENANCE_BY_GEOMETRY_CLASS)):
            if provenance not in PROVENANCE_BY_GEOMETRY_CLASS[geometry_class]:
                state.errors.append(
                    f"{label}: provenance {provenance!r} is incompatible with {geometry_class!r}"
                )
        source_ids = raw.get("source_ids")
        if (
            not isinstance(source_ids, list)
            or not source_ids
            or any(not isinstance(item, str) or not item for item in source_ids)
        ):
            state.errors.append(f"{label}.source_ids: expected one or more source IDs")
            source_ids = []
        unknown_sources = sorted(set(source_ids) - set(source_records))
        if unknown_sources:
            state.errors.append(
                f"{label}.source_ids: unknown sources {unknown_sources}"
            )
        source_definition_id = raw.get("source_definition_id")
        if enum_member(provenance, SOURCE_BOUND_PROVENANCE):
            if (
                not isinstance(source_definition_id, str)
                or source_definition_id not in source_definitions
            ):
                state.errors.append(
                    f"{label}.source_definition_id: source-bound provenance requires a ledger definition"
                )
        elif source_definition_id is not None and (
            not isinstance(source_definition_id, str)
            or source_definition_id not in source_definitions
        ):
            state.errors.append(
                f"{label}.source_definition_id: unknown source definition {source_definition_id!r}"
            )
        if (
            isinstance(source_definition_id, str)
            and source_definition_id in source_definitions
        ):
            source_definition = source_definitions[source_definition_id]
            bound_source_id = source_definition.get("source_id")
            if bound_source_id is not None and bound_source_id not in source_ids:
                state.errors.append(
                    f"{label}.source_ids: missing source bound by {source_definition_id!r}"
                )
            if release_intent == "MANUFACTURING_HANDOFF" and enum_member(
                source_definition.get("definition_kind"),
                {"PURCHASED_MODULE", "PURCHASED_HARDWARE"},
            ):
                if (
                    geometry_class != "SUPPLIER_EXACT_BREP"
                    or provenance != "SUPPLIER_BREP"
                ):
                    state.holds.append(
                        f"{label}: manufacturing handoff requires supplier-exact geometry "
                        "for purchased definitions"
                    )
        faceted = raw.get("faceted_or_tessellated")
        if not isinstance(faceted, bool):
            state.errors.append(f"{label}.faceted_or_tessellated: expected a boolean")
        counts = {
            "brep": count_value(
                raw.get("brep_body_count"), f"{label}.brep_body_count", state
            ),
            "mesh": count_value(
                raw.get("mesh_body_count"), f"{label}.mesh_body_count", state
            ),
            "surface_or_empty": count_value(
                raw.get("surface_only_or_empty_count"),
                f"{label}.surface_only_or_empty_count",
                state,
            ),
        }
        for key, value in counts.items():
            if value is not None:
                totals[key] += value
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
        if enum_member(geometry_class, NATIVE_GEOMETRY_CLASSES):
            if geometry_class not in accepted:
                state.holds.append(
                    f"{label}: native class is not accepted by geometry policy"
                )
            if faceted is not False or provenance == "TESSELLATED_PROXY":
                state.errors.append(
                    f"{label}: faceted/mesh-derived geometry cannot be claimed as native"
                )
            if counts["brep"] in (None, 0) or counts["mesh"] not in (None, 0):
                state.errors.append(
                    f"{label}: native geometry requires positive B-Rep and zero mesh bodies"
                )
            if counts["surface_or_empty"] not in (None, 0):
                state.errors.append(
                    f"{label}: native geometry cannot be surface-only or empty"
                )
        elif geometry_class == "MESH_PROXY" and counts["mesh"] in (None, 0):
            state.errors.append(
                f"{label}: MESH_PROXY requires a positive mesh body count"
            )
        elif geometry_class == "FACETED_BREP_PROXY":
            if faceted is not True or counts["brep"] in (None, 0):
                state.errors.append(
                    f"{label}: FACETED_BREP_PROXY requires faceted=true and positive B-Rep count"
                )
        elif geometry_class == "ANALYTIC_DISPLAY_PROXY" and counts["brep"] in (None, 0):
            state.errors.append(
                f"{label}: ANALYTIC_DISPLAY_PROXY requires a positive B-Rep count"
            )
        elif enum_member(geometry_class, {"SURFACE_ONLY", "EMPTY"}):
            if counts["surface_or_empty"] in (None, 0):
                state.errors.append(
                    f"{label}: {geometry_class} requires positive surface-only/empty count"
                )
    for occurrence_id, occurrence in content_occurrences.items():
        presence = occurrence.get("presence")
        if not enum_member(presence, {"EXACT", "PROXY"}):
            continue
        definition_id = occurrence.get("geometry_definition_id")
        if not isinstance(definition_id, str) or definition_id not in definitions:
            state.errors.append(
                f"completeness occurrence {occurrence_id!r}: unknown geometry definition {definition_id!r}"
            )
            continue
        geometry_class = definitions[definition_id].get("geometry_class")
        if presence == "EXACT" and not enum_member(
            geometry_class, NATIVE_GEOMETRY_CLASSES
        ):
            state.errors.append(
                f"completeness occurrence {occurrence_id!r}: EXACT uses non-native class {geometry_class!r}"
            )
        if presence == "PROXY" and not enum_member(
            geometry_class, PROXY_GEOMETRY_CLASSES
        ):
            state.errors.append(
                f"completeness occurrence {occurrence_id!r}: PROXY uses native class {geometry_class!r}"
            )
    if claim_enabled(claims, "NATIVE_GEOMETRY"):
        if not definitions:
            state.holds.append(
                "geometry_definitions: no per-definition provenance ledger"
            )
        proxy_definitions = [
            definition_id
            for definition_id, raw in definitions.items()
            if enum_member(raw.get("geometry_class"), PROXY_GEOMETRY_CLASSES)
        ]
        if proxy_count or proxy_definitions:
            state.holds.append(
                "geometry_policy: proxy occurrences or definitions block native geometry closure"
            )
    return definitions, totals


def validate_placements_and_joints(
    document: dict[str, Any],
    claims: list[str],
    content_occurrences: dict[str, dict[str, Any]],
    state: ValidationState,
) -> tuple[set[str], set[str]]:
    persisted = claim_enabled(claims, "PERSISTED_FUSION_ASSEMBLY")
    present_occurrences = {
        occurrence_id: raw
        for occurrence_id, raw in content_occurrences.items()
        if enum_member(raw.get("presence"), {"EXACT", "PROXY"})
    }
    known_fusion_ids = {
        raw.get("fusion_occurrence_id")
        for raw in present_occurrences.values()
        if isinstance(raw.get("fusion_occurrence_id"), str)
    }
    placement_root = object_value(document, "placement_root", state)
    root_anchor_id = placement_root.get("root_anchor_id")
    if not isinstance(root_anchor_id, str) or not root_anchor_id:
        state.errors.append(
            "placement_root.root_anchor_id: expected a non-empty string"
        )
        root_anchor_id = None
    elif root_anchor_id in known_fusion_ids:
        state.errors.append(
            "placement_root.root_anchor_id: must not reuse an occurrence identity"
        )
    anchor_type = placement_root.get("anchor_type")
    if not enum_member(anchor_type, {"FUSION_ROOT_COMPONENT", "WORLD_FRAME"}):
        state.errors.append(
            f"placement_root.anchor_type: unsupported value {anchor_type!r}"
        )
    root_status = placement_root.get("status")
    if not enum_member(root_status, {"PASS", "HOLD", "UNKNOWN"}):
        state.errors.append(f"placement_root.status: unsupported value {root_status!r}")
    elif persisted and root_status != "PASS":
        state.holds.append("placement_root.status: PASS not established")
    state.evidence_links(
        placement_root.get("evidence_ids"), "placement_root.evidence_ids", required=True
    )
    contracts = list_value(document, "placement_contracts", state)
    contract_ids: set[str] = set()
    contract_occurrence_ids: set[str] = set()
    host_graph: dict[str, set[str]] = {}
    for index, raw in enumerate(contracts):
        label = f"placement_contracts[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        contract_id = raw.get("placement_contract_id")
        if not isinstance(contract_id, str) or not contract_id:
            state.errors.append(
                f"{label}.placement_contract_id: expected a non-empty string"
            )
            continue
        if contract_id in contract_ids:
            state.errors.append(
                f"{label}.placement_contract_id: duplicate {contract_id!r}"
            )
        contract_ids.add(contract_id)
        occurrence_id = raw.get("requirement_occurrence_id")
        if (
            not isinstance(occurrence_id, str)
            or occurrence_id not in present_occurrences
        ):
            state.errors.append(
                f"{label}.requirement_occurrence_id: unknown present occurrence {occurrence_id!r}"
            )
        else:
            contract_occurrence_ids.add(occurrence_id)
            occurrence = present_occurrences[occurrence_id]
            if occurrence.get("placement_contract_id") != contract_id:
                state.errors.append(
                    f"{label}: content ledger binds a different placement contract"
                )
            if occurrence.get("fusion_occurrence_id") != raw.get(
                "fusion_occurrence_id"
            ):
                state.errors.append(
                    f"{label}: Fusion occurrence differs from content ledger"
                )
            if occurrence.get("source_occurrence_id") != raw.get(
                "lineage_source_occurrence_id"
            ):
                state.errors.append(
                    f"{label}: source lineage differs from content ledger"
                )
        fusion_id = raw.get("fusion_occurrence_id")
        if not isinstance(fusion_id, str) or fusion_id not in known_fusion_ids:
            state.errors.append(
                f"{label}.fusion_occurrence_id: unknown Fusion occurrence {fusion_id!r}"
            )
        hosts = raw.get("host_fusion_occurrence_ids")
        if not isinstance(hosts, list) or any(
            not isinstance(item, str) or not item for item in hosts
        ):
            state.errors.append(
                f"{label}.host_fusion_occurrence_ids: expected an array of strings"
            )
            hosts = []
        elif not hosts:
            state.errors.append(
                f"{label}.host_fusion_occurrence_ids: bind top-level placement to root_anchor_id"
            )
        elif len(set(hosts)) != len(hosts):
            state.errors.append(f"{label}.host_fusion_occurrence_ids: duplicate hosts")
        if fusion_id in hosts:
            state.errors.append(f"{label}: an occurrence cannot host itself")
        allowed_hosts = set(known_fusion_ids)
        if root_anchor_id is not None:
            allowed_hosts.add(root_anchor_id)
        unknown_hosts = sorted(set(hosts) - allowed_hosts)
        if unknown_hosts:
            state.errors.append(
                f"{label}.host_fusion_occurrence_ids: unknown hosts {unknown_hosts}"
            )
        if isinstance(fusion_id, str):
            host_graph[fusion_id] = set(hosts)
        transform = raw.get("world_transform")
        if not valid_rigid_transform(transform):
            state.errors.append(
                f"{label}.world_transform: expected a finite right-handed rigid 4x4 transform"
            )
        if raw.get("transform_convention") != "ROW_MAJOR_PARENT_TO_WORLD":
            state.errors.append(
                f"{label}.transform_convention: expected ROW_MAJOR_PARENT_TO_WORLD"
            )
        for status_field in ("placement_status", "transform_status", "lineage_status"):
            status = raw.get(status_field)
            if not enum_member(status, {"PASS", "HOLD", "UNKNOWN"}):
                state.errors.append(
                    f"{label}.{status_field}: unsupported value {status!r}"
                )
            elif persisted and status != "PASS":
                state.holds.append(f"{label}.{status_field}: PASS not established")
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
    missing_contracts = sorted(set(present_occurrences) - contract_occurrence_ids)
    extra_contracts = sorted(contract_occurrence_ids - set(present_occurrences))
    if missing_contracts or extra_contracts:
        state.errors.append(
            "placement_contracts: occurrence coverage mismatch "
            f"(missing={missing_contracts}, extra={extra_contracts})"
        )
    if root_anchor_id is not None:
        children: dict[str, set[str]] = {root_anchor_id: set()}
        for occurrence_id in known_fusion_ids:
            children.setdefault(occurrence_id, set())
        for child, hosts in host_graph.items():
            for host in hosts:
                if child in known_fusion_ids and host in children:
                    children[host].add(child)
        reachable = {root_anchor_id}
        frontier = [root_anchor_id]
        while frontier:
            host = frontier.pop()
            for child in children.get(host, set()) - reachable:
                reachable.add(child)
                frontier.append(child)
        disconnected = sorted(known_fusion_ids - reachable)
        if disconnected:
            state.errors.append(
                f"placement_contracts: occurrences disconnected from root anchor {disconnected}"
            )
    visiting: set[str] = set()
    visited: set[str] = set()

    def host_cycle(node: str) -> bool:
        if node in visiting:
            return True
        if node in visited:
            return False
        visiting.add(node)
        for host in host_graph.get(node, set()):
            if host in known_fusion_ids and host_cycle(host):
                return True
        visiting.remove(node)
        visited.add(node)
        return False

    if any(host_cycle(node) for node in sorted(known_fusion_ids)):
        state.errors.append("placement_contracts: host graph contains a cycle")
    if persisted and not contracts:
        state.holds.append("placement_contracts: no placement/transform contracts")

    joint_records = list_value(document, "joint_records", state)
    joint_ids: set[str] = set()
    healthy_count = 0
    for index, raw in enumerate(joint_records):
        label = f"joint_records[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        joint_id = raw.get("joint_id")
        if not isinstance(joint_id, str) or not joint_id:
            state.errors.append(f"{label}.joint_id: expected a non-empty string")
        elif joint_id in joint_ids:
            state.errors.append(f"{label}.joint_id: duplicate {joint_id!r}")
        else:
            joint_ids.add(joint_id)
        relation_type = raw.get("relation_type")
        if not enum_member(relation_type, RELATION_TYPES):
            state.errors.append(
                f"{label}.relation_type: unsupported value {relation_type!r}"
            )
        occurrence_ids = raw.get("fusion_occurrence_ids")
        if (
            not isinstance(occurrence_ids, list)
            or len(occurrence_ids) < 2
            or any(not isinstance(item, str) or not item for item in occurrence_ids)
        ):
            state.errors.append(
                f"{label}.fusion_occurrence_ids: expected at least two occurrences"
            )
            occurrence_ids = []
        unknown = sorted(set(occurrence_ids) - known_fusion_ids)
        if unknown:
            state.errors.append(
                f"{label}.fusion_occurrence_ids: unknown occurrences {unknown}"
            )
        health = raw.get("health_status")
        if not enum_member(health, {"PASS", "HOLD", "UNKNOWN"}):
            state.errors.append(f"{label}.health_status: unsupported value {health!r}")
        elif health == "PASS":
            healthy_count += 1
        elif persisted:
            state.holds.append(f"{label}.health_status: PASS not established")
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
    summary = object_value(document, "joint_validation", state)
    status = summary.get("status")
    if not enum_member(status, ROW_STATUSES):
        state.errors.append(f"joint_validation.status: unsupported value {status!r}")
    expected = count_value(
        summary.get("expected_joint_count"),
        "joint_validation.expected_joint_count",
        state,
    )
    healthy = count_value(
        summary.get("healthy_joint_count"),
        "joint_validation.healthy_joint_count",
        state,
    )
    unhealthy = count_value(
        summary.get("unhealthy_joint_count"),
        "joint_validation.unhealthy_joint_count",
        state,
    )
    if expected is not None and expected != len(joint_records):
        state.errors.append(
            f"joint_validation.expected_joint_count: declared {expected} != ledger {len(joint_records)}"
        )
    if healthy is not None and healthy != healthy_count:
        state.errors.append(
            f"joint_validation.healthy_joint_count: declared {healthy} != ledger {healthy_count}"
        )
    derived_unhealthy = len(joint_records) - healthy_count
    if unhealthy is not None and unhealthy != derived_unhealthy:
        state.errors.append(
            f"joint_validation.unhealthy_joint_count: declared {unhealthy} != ledger {derived_unhealthy}"
        )
    if not joint_records:
        if status != "NOT_APPLICABLE":
            state.errors.append(
                "joint_validation.status: zero joints require NOT_APPLICABLE"
            )
        require_known(summary.get("rationale"), "joint_validation.rationale", state)
        state.evidence_links(
            summary.get("evidence_ids"), "joint_validation.evidence_ids", required=True
        )
    else:
        state.evidence_links(
            summary.get("evidence_ids"), "joint_validation.evidence_ids", required=True
        )
        if persisted and status != "PASS":
            state.holds.append("joint_validation.status: PASS not established")
        if persisted and derived_unhealthy:
            state.holds.append(
                f"joint_validation: {derived_unhealthy} unhealthy joints remain"
            )
    for index, raw in enumerate(contracts):
        if not isinstance(raw, dict):
            continue
        joint_id = raw.get("joint_id")
        if joint_id is not None and (
            not isinstance(joint_id, str) or joint_id not in joint_ids
        ):
            state.errors.append(
                f"placement_contracts[{index}].joint_id: unknown joint {joint_id!r}"
            )
    return known_fusion_ids, joint_ids


def validate_fusion_readback(
    document: dict[str, Any],
    claims: list[str],
    geometry_definitions: dict[str, dict[str, Any]],
    geometry_totals: dict[str, int],
    state: ValidationState,
) -> dict[str, int | None]:
    target = object_value(document, "fusion_target", state)
    if target.get("one_writer") is not True:
        state.errors.append("fusion_target.one_writer: must be true")
    if not isinstance(target.get("overwrite_allowed"), bool):
        state.errors.append("fusion_target.overwrite_allowed: expected a boolean")
    if target.get("overwrite_allowed") is True:
        state.evidence_links(
            target.get("overwrite_authority_evidence_ids"),
            "fusion_target.overwrite_authority_evidence_ids",
            required=True,
        )
    state.evidence_links(
        target.get("writer_receipt_evidence_ids"),
        "fusion_target.writer_receipt_evidence_ids",
        required=True,
    )
    for field_name in ("document_name", "destination_project", "destination_folder"):
        require_known(target.get(field_name), f"fusion_target.{field_name}", state)
    readback = object_value(document, "native_readback", state)
    numeric_fields = {
        key: count_value(readback.get(key), f"native_readback.{key}", state)
        for key in (
            "component_definition_count",
            "occurrence_count",
            "brep_body_count",
            "mesh_body_count",
            "surface_only_or_empty_count",
            "unresolved_external_reference_count",
            "duplicate_occurrence_count",
            "orphan_occurrence_count",
        )
    }
    if not claim_enabled(claims, "PERSISTED_FUSION_ASSEMBLY"):
        return numeric_fields
    if readback.get("status") != "PASS":
        state.holds.append("native_readback.status: PASS not established")
    if readback.get("is_saved") is not True:
        state.holds.append("native_readback.is_saved: saved state not established")
    if readback.get("is_modified_after_save") is not False:
        state.holds.append(
            "native_readback.is_modified_after_save: clean persisted state not established"
        )
    for field_name in ("document_name", "document_creation_id", "data_file_identity"):
        require_known(readback.get(field_name), f"native_readback.{field_name}", state)
    if not placeholder(target.get("document_name")) and not placeholder(
        readback.get("document_name")
    ):
        if target.get("document_name") != readback.get("document_name"):
            state.errors.append(
                "native_readback.document_name: does not match fusion_target.document_name"
            )
    for field_name, value in numeric_fields.items():
        if value is None:
            state.holds.append(f"native_readback.{field_name}: count not established")
    for field_name in (
        "component_definition_count",
        "occurrence_count",
        "brep_body_count",
    ):
        if numeric_fields[field_name] == 0:
            state.holds.append(f"native_readback.{field_name}: positive count required")
    if numeric_fields["mesh_body_count"] not in (None, 0):
        state.holds.append(
            f"native_readback: {numeric_fields['mesh_body_count']} mesh bodies in native-only scope"
        )
    if numeric_fields["surface_only_or_empty_count"] not in (None, 0):
        state.holds.append(
            "native_readback: surface-only or empty definitions remain in a complete native scope"
        )
    if numeric_fields["unresolved_external_reference_count"] not in (None, 0):
        state.holds.append("native_readback: unresolved external references remain")
    if numeric_fields["duplicate_occurrence_count"] not in (None, 0):
        state.holds.append("native_readback: duplicate occurrences remain")
    if numeric_fields["orphan_occurrence_count"] not in (None, 0):
        state.holds.append("native_readback: orphan occurrences remain")
    if numeric_fields["component_definition_count"] is not None:
        if numeric_fields["component_definition_count"] != len(geometry_definitions):
            state.errors.append(
                "native_readback.component_definition_count: does not equal geometry definition ledger"
            )
    for field_name, geometry_key in (
        ("brep_body_count", "brep"),
        ("mesh_body_count", "mesh"),
        ("surface_only_or_empty_count", "surface_or_empty"),
    ):
        value = numeric_fields[field_name]
        if value is not None and value != geometry_totals[geometry_key]:
            state.errors.append(
                f"native_readback.{field_name}: declared {value} != geometry ledger "
                f"{geometry_totals[geometry_key]}"
            )
    if readback.get("close_reopen_status") != "PASS":
        state.holds.append("native_readback.close_reopen_status: PASS not established")
    state.evidence_links(
        readback.get("evidence_ids"), "native_readback.evidence_ids", required=True
    )
    second = object_value(readback, "second_readback", state)
    if second.get("status") != "PASS":
        state.holds.append(
            "native_readback.second_readback.status: PASS not established"
        )
    for field_name in ("document_creation_id", "data_file_identity"):
        require_known(
            second.get(field_name),
            f"native_readback.second_readback.{field_name}",
            state,
        )
        if not placeholder(second.get(field_name)) and second.get(
            field_name
        ) != readback.get(field_name):
            state.errors.append(
                f"native_readback.second_readback.{field_name}: identity changed after reopen"
            )
    second_occurrences = count_value(
        second.get("occurrence_count"),
        "native_readback.second_readback.occurrence_count",
        state,
    )
    if second_occurrences != numeric_fields["occurrence_count"]:
        state.errors.append(
            "native_readback.second_readback.occurrence_count: does not match first readback"
        )
    state.evidence_links(
        second.get("evidence_ids"),
        "native_readback.second_readback.evidence_ids",
        required=True,
    )
    return numeric_fields


def validate_occurrence_reconciliation(
    document: dict[str, Any],
    claims: list[str],
    source_count: int | None,
    fusion_count: int | None,
    source_definitions: dict[str, dict[str, Any]],
    geometry_definitions: dict[str, dict[str, Any]],
    content_occurrences: dict[str, dict[str, Any]],
    state: ValidationState,
) -> tuple[set[str], set[str]]:
    reconciliation = object_value(document, "occurrence_reconciliation", state)
    rows = list_value(reconciliation, "rows", state)
    row_ids: set[str] = set()
    source_ids: set[str] = set()
    fusion_ids: set[str] = set()
    reconciliation_by_source: dict[str, dict[str, Any]] = {}
    reconciliation_by_fusion: dict[str, dict[str, Any]] = {}
    unresolved_differences = 0
    for index, raw in enumerate(rows):
        label = f"occurrence_reconciliation.rows[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        row_id = raw.get("row_id")
        if not isinstance(row_id, str) or not row_id:
            state.errors.append(f"{label}.row_id: expected a non-empty string")
        elif row_id in row_ids:
            state.errors.append(f"{label}.row_id: duplicate {row_id!r}")
        else:
            row_ids.add(row_id)
        source_id = raw.get("source_occurrence_id")
        fusion_id = raw.get("fusion_occurrence_id")
        source_definition_id = raw.get("source_definition_id")
        fusion_definition_id = raw.get("fusion_definition_id")
        if source_id is not None and (not isinstance(source_id, str) or not source_id):
            state.errors.append(
                f"{label}.source_occurrence_id: expected string or null"
            )
        if fusion_id is not None and (not isinstance(fusion_id, str) or not fusion_id):
            state.errors.append(
                f"{label}.fusion_occurrence_id: expected string or null"
            )
        if isinstance(source_id, str):
            if source_id in source_ids:
                state.errors.append(
                    f"{label}.source_occurrence_id: duplicate {source_id!r}"
                )
            else:
                source_ids.add(source_id)
                reconciliation_by_source[source_id] = raw
        if isinstance(fusion_id, str):
            if fusion_id in fusion_ids:
                state.errors.append(
                    f"{label}.fusion_occurrence_id: duplicate {fusion_id!r}"
                )
            else:
                fusion_ids.add(fusion_id)
                reconciliation_by_fusion[fusion_id] = raw
        if isinstance(source_id, str):
            if (
                not isinstance(source_definition_id, str)
                or source_definition_id not in source_definitions
            ):
                state.errors.append(
                    f"{label}.source_definition_id: source occurrence requires a known source definition"
                )
        elif source_definition_id is not None:
            state.errors.append(
                f"{label}.source_definition_id: null source occurrence requires null source definition"
            )
        if isinstance(fusion_id, str):
            if (
                not isinstance(fusion_definition_id, str)
                or fusion_definition_id not in geometry_definitions
            ):
                state.errors.append(
                    f"{label}.fusion_definition_id: Fusion occurrence requires a known geometry definition"
                )
        elif fusion_definition_id is not None:
            state.errors.append(
                f"{label}.fusion_definition_id: null Fusion occurrence requires null Fusion definition"
            )
        disposition = raw.get("disposition")
        if not enum_member(disposition, RECONCILIATION_DISPOSITIONS):
            state.errors.append(
                f"{label}.disposition: unsupported value {disposition!r}"
            )
        if disposition == "MATCHED" and not (
            isinstance(source_id, str) and isinstance(fusion_id, str)
        ):
            state.errors.append(
                f"{label}: MATCHED requires both source and Fusion occurrence IDs"
            )
        elif enum_member(
            disposition,
            {"SOURCE_ONLY_AUTHORIZED_EXCLUSION", "SOURCE_ONLY_UNRESOLVED"},
        ):
            if not isinstance(source_id, str) or fusion_id is not None:
                state.errors.append(
                    f"{label}: source-only disposition requires source ID and null Fusion ID"
                )
            require_known(raw.get("rationale"), f"{label}.rationale", state)
        if (
            disposition == "MATCHED"
            and isinstance(source_definition_id, str)
            and source_definition_id in source_definitions
            and isinstance(fusion_definition_id, str)
            and fusion_definition_id in geometry_definitions
            and geometry_definitions[fusion_definition_id].get("source_definition_id")
            != source_definition_id
        ):
            state.errors.append(
                f"{label}: matched Fusion definition does not bind the reconciled source definition"
            )
        elif enum_member(
            disposition,
            {"FUSION_ONLY_AUTHORIZED_ADDITION", "FUSION_ONLY_UNRESOLVED"},
        ):
            if source_id is not None or not isinstance(fusion_id, str):
                state.errors.append(
                    f"{label}: Fusion-only disposition requires null source ID and Fusion ID"
                )
            require_known(raw.get("rationale"), f"{label}.rationale", state)
        if enum_member(
            disposition, {"SOURCE_ONLY_UNRESOLVED", "FUSION_ONLY_UNRESOLVED"}
        ):
            unresolved_differences += 1
        state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=True
        )
    if source_count is not None and len(source_ids) != source_count:
        state.errors.append(
            f"occurrence_reconciliation: source ledger {len(source_ids)} != source count {source_count}"
        )
    if fusion_count is not None and len(fusion_ids) != fusion_count:
        state.errors.append(
            f"occurrence_reconciliation: Fusion ledger {len(fusion_ids)} != Fusion count {fusion_count}"
        )
    present_content = {
        raw.get("fusion_occurrence_id")
        for raw in content_occurrences.values()
        if enum_member(raw.get("presence"), {"EXACT", "PROXY"})
        and isinstance(raw.get("fusion_occurrence_id"), str)
    }
    if fusion_ids != present_content:
        state.errors.append(
            "occurrence_reconciliation: Fusion occurrence set does not equal content ledger set"
        )
    matched_source_ids = {
        raw.get("source_occurrence_id")
        for raw in content_occurrences.values()
        if isinstance(raw.get("source_occurrence_id"), str)
    }
    unknown_content_sources = sorted(matched_source_ids - source_ids)
    if unknown_content_sources:
        state.errors.append(
            "occurrence_reconciliation: content ledger has unknown source occurrences "
            f"{unknown_content_sources}"
        )
    for occurrence_id, raw in content_occurrences.items():
        presence = raw.get("presence")
        fusion_id = raw.get("fusion_occurrence_id")
        source_id = raw.get("source_occurrence_id")
        if enum_member(presence, {"EXACT", "PROXY"}):
            if (
                not isinstance(fusion_id, str)
                or fusion_id not in reconciliation_by_fusion
            ):
                continue
            reconciliation_row = reconciliation_by_fusion[fusion_id]
            reconciled_source = reconciliation_row.get("source_occurrence_id")
            disposition = reconciliation_row.get("disposition")
            if raw.get("geometry_definition_id") != reconciliation_row.get(
                "fusion_definition_id"
            ):
                state.errors.append(
                    f"occurrence_reconciliation: content occurrence {occurrence_id!r} "
                    "does not match its Fusion occurrence/definition pair"
                )
            if isinstance(source_id, str):
                if reconciled_source != source_id or disposition != "MATCHED":
                    state.errors.append(
                        f"occurrence_reconciliation: content occurrence {occurrence_id!r} "
                        "does not match its exact source/Fusion pair"
                    )
            elif (
                reconciled_source is not None
                or disposition != "FUSION_ONLY_AUTHORIZED_ADDITION"
            ):
                state.errors.append(
                    f"occurrence_reconciliation: source-free content occurrence {occurrence_id!r} "
                    "requires an authorized Fusion-only addition"
                )
        elif isinstance(source_id, str) and source_id in reconciliation_by_source:
            reconciliation_row = reconciliation_by_source[source_id]
            if reconciliation_row.get(
                "fusion_occurrence_id"
            ) is not None or not enum_member(
                reconciliation_row.get("disposition"),
                {"SOURCE_ONLY_AUTHORIZED_EXCLUSION", "SOURCE_ONLY_UNRESOLVED"},
            ):
                state.errors.append(
                    f"occurrence_reconciliation: absent content occurrence {occurrence_id!r} "
                    "requires a source-only reconciliation disposition"
                )
    if claim_enabled(claims, "PERSISTED_FUSION_ASSEMBLY"):
        if reconciliation.get("status") != "PASS":
            state.holds.append("occurrence_reconciliation.status: PASS not established")
        if not rows:
            state.holds.append(
                "occurrence_reconciliation.rows: no occurrence-level reconciliation"
            )
        if source_count == 0 or fusion_count == 0:
            state.holds.append(
                "occurrence_reconciliation: zero source or Fusion count cannot PASS"
            )
        if unresolved_differences:
            state.holds.append(
                f"occurrence_reconciliation: {unresolved_differences} differences lack disposition closure"
            )
        state.evidence_links(
            reconciliation.get("evidence_ids"),
            "occurrence_reconciliation.evidence_ids",
            required=True,
        )
    return source_ids, fusion_ids


def validate_process_handoff(
    document: dict[str, Any], claims: list[str], state: ValidationState
) -> None:
    handoff = object_value(document, "s0_s11_handoff", state)
    certified = count_value(
        handoff.get("certified_motion_count"),
        "s0_s11_handoff.certified_motion_count",
        state,
    )
    uncertified = count_value(
        handoff.get("uncertified_motion_count"),
        "s0_s11_handoff.uncertified_motion_count",
        state,
    )
    if not claim_enabled(claims, "PROCESS_AND_MOTION"):
        return
    if handoff.get("status") != "PASS":
        state.holds.append("s0_s11_handoff.status: PASS not established")
    require_known(
        handoff.get("project_input_id"), "s0_s11_handoff.project_input_id", state
    )
    if certified is None:
        state.holds.append(
            "s0_s11_handoff.certified_motion_count: count not established"
        )
    elif certified == 0:
        state.holds.append(
            "s0_s11_handoff.certified_motion_count: positive count required for PROCESS_AND_MOTION"
        )
    if uncertified is None:
        state.holds.append(
            "s0_s11_handoff.uncertified_motion_count: count not established"
        )
    elif uncertified:
        state.holds.append(f"s0_s11_handoff: {uncertified} motions remain uncertified")
    if handoff.get("scene_readback_status") != "PASS":
        state.holds.append("s0_s11_handoff.scene_readback_status: PASS not established")
    if handoff.get("media_decode_status") != "PASS":
        state.holds.append("s0_s11_handoff.media_decode_status: PASS not established")
    media_hash = handoff.get("media_sha256")
    if not isinstance(media_hash, str) or not HEX64.fullmatch(media_hash):
        state.holds.append(
            "s0_s11_handoff.media_sha256: exact media hash not established"
        )
    handoff_evidence_ids = state.evidence_links(
        handoff.get("evidence_ids"), "s0_s11_handoff.evidence_ids", required=True
    )
    if isinstance(media_hash, str) and HEX64.fullmatch(media_hash):
        matching_media = [
            evidence_id
            for evidence_id in handoff_evidence_ids
            if evidence_id in state.evidence_records
            and state.evidence_records[evidence_id].get("sha256", "").lower()
            == media_hash.lower()
            and state.evidence_records[evidence_id].get("kind") == "PROCESS_RECORD"
            and str(
                state.evidence_records[evidence_id].get("media_type", "")
            ).startswith("video/")
        ]
        if not matching_media:
            state.errors.append(
                "s0_s11_handoff.media_sha256: must match a cited hash-checked video PROCESS_RECORD"
            )


def validate_document(
    document: Any, manifest_path: Path, check_files: bool = False
) -> tuple[dict[str, Any], int]:
    state = ValidationState()
    empty_sums = {"expected": 0, "exact": 0, "proxy": 0, "missing": 0, "unknown": 0}
    if not isinstance(document, dict):
        result = {
            "status": "FAIL",
            "declared_verdict": None,
            "errors": ["manifest root must be an object"],
            "holds": [],
            "warnings": [],
            "claim_boundary": "No source, Fusion, geometry, motion, or physical claim was evaluated.",
        }
        return result, 1
    if document.get("$schema") != SCHEMA:
        state.errors.append(f"$schema: expected {SCHEMA!r}")
    for field_name in ("project_id", "captured_at", "claim_boundary"):
        if not isinstance(document.get(field_name), str) or not document.get(
            field_name
        ):
            state.errors.append(f"{field_name}: expected a non-empty string")
        elif field_name != "claim_boundary":
            require_known(document.get(field_name), field_name, state)
    if not valid_timestamp(document.get("captured_at")):
        if not placeholder(document.get("captured_at")):
            state.errors.append(
                "captured_at: expected an ISO-8601 timestamp with timezone"
            )
    release_intent = document.get("release_intent")
    if not enum_member(release_intent, RELEASE_INTENTS):
        state.errors.append(f"release_intent: unsupported value {release_intent!r}")
    raw_claims = list_value(document, "target_claims", state)
    claims = [item for item in raw_claims if isinstance(item, str)]
    if len(claims) != len(raw_claims):
        state.errors.append("target_claims: every item must be a string")
    if not claims:
        state.holds.append("target_claims: no claim selected")
    if len(set(claims)) != len(claims):
        state.errors.append("target_claims: duplicate claims are not allowed")
    unknown_claims = sorted(set(claims) - set(CLAIM_ORDER))
    if unknown_claims:
        state.errors.append(f"target_claims: unsupported claims {unknown_claims}")
    if claims:
        expected_prefix = CLAIM_ORDER[: len(claims)]
        if claims != expected_prefix:
            state.errors.append(
                f"target_claims: must be an ordered dependency prefix {expected_prefix}"
            )
        if not check_files:
            state.holds.append(
                "file_verification: engineering claims require --check-files source/evidence hash verification"
            )

    validate_evidence_registry(document, manifest_path, check_files, state)
    sources = validate_sources(
        document, manifest_path, check_files, release_intent, state
    )
    source_values, source_definitions = validate_source_closure(
        document, claims, sources, state
    )
    sums, content_occurrences = validate_completeness(document, claims, state)
    geometry_definitions, geometry_totals = validate_geometry(
        document,
        claims,
        release_intent,
        sums["proxy"],
        sources,
        source_definitions,
        content_occurrences,
        state,
    )
    validate_placements_and_joints(document, claims, content_occurrences, state)
    readback_values = validate_fusion_readback(
        document, claims, geometry_definitions, geometry_totals, state
    )
    validate_occurrence_reconciliation(
        document,
        claims,
        source_values["expanded_occurrence_count"],
        readback_values["occurrence_count"],
        source_definitions,
        geometry_definitions,
        content_occurrences,
        state,
    )
    if claim_enabled(claims, "CONTENT_COMPLETENESS"):
        if claim_enabled(claims, "PERSISTED_FUSION_ASSEMBLY"):
            if (
                readback_values["occurrence_count"] is not None
                and sums["expected"] != readback_values["occurrence_count"]
            ):
                state.errors.append(
                    "completeness: requirement-ledger expected count does not equal Fusion occurrence count"
                )
        elif (
            source_values["expanded_occurrence_count"] is not None
            and sums["expected"] != source_values["expanded_occurrence_count"]
        ):
            state.errors.append(
                "completeness: requirement-ledger expected count does not equal source occurrence count"
            )
    validate_process_handoff(document, claims, state)

    known_unknowns = list_value(document, "known_unknowns", state)
    if known_unknowns:
        state.holds.append(
            f"known_unknowns: {len(known_unknowns)} unresolved declarations"
        )
    unused_evidence = sorted(state.evidence_registry - state.referenced_evidence)
    if unused_evidence:
        state.warnings.append(
            f"evidence_registry: unreferenced records {unused_evidence}"
        )
    declared = document.get("declared_verdict")
    if not enum_member(declared, VERDICTS):
        state.errors.append(f"declared_verdict: unsupported value {declared!r}")
    if declared == "PASS" and state.holds:
        state.errors.append(
            "declared_verdict: PASS overclaims a manifest with unresolved holds"
        )

    if state.errors or declared == "FAIL":
        status = "FAIL"
        exit_code = 1
    elif state.holds or enum_member(declared, {"PASS_WITH_HOLDS", "HOLD", "UNKNOWN"}):
        status = "HOLD"
        exit_code = 2
    else:
        status = "PASS"
        exit_code = 0
    result = {
        "status": status,
        "declared_verdict": declared,
        "target_claims": claims,
        "errors": sorted(set(state.errors)),
        "holds": sorted(set(state.holds)),
        "warnings": sorted(set(state.warnings)),
        "summary": {
            "source_count": len(document.get("sources", []))
            if isinstance(document.get("sources"), list)
            else 0,
            "file_hash_verification_performed": check_files,
            "registered_evidence_count": len(state.evidence_registry),
            "content_requirement_count": len(
                document.get("completeness", {}).get("requirements", [])
            )
            if isinstance(document.get("completeness"), dict)
            else 0,
            "expected_physical_occurrences": sums.get(
                "expected", empty_sums["expected"]
            ),
            "exact_occurrences": sums.get("exact", empty_sums["exact"]),
            "proxy_occurrences": sums.get("proxy", empty_sums["proxy"]),
            "missing_occurrences": sums.get("missing", empty_sums["missing"]),
            "unknown_occurrences": sums.get("unknown", empty_sums["unknown"]),
        },
        "claim_boundary": (
            "This validates manifest structure, evidence referential integrity, mandatory source/evidence "
            "file hashes for engineering PASS, "
            "ledger-derived closure/count arithmetic, strict geometry/rights classes, occurrence "
            "reconciliation, placement/readback gates, and overclaiming. It does not inspect Fusion "
            "or prove that the content or engineering relevance of cited evidence is truthful."
        ),
    }
    return result, exit_code


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "manifest", type=Path, help="filled assembly authoring manifest"
    )
    parser.add_argument(
        "--check-files",
        action="store_true",
        help="resolve source/evidence paths relative to the manifest and verify size/hash",
    )
    args = parser.parse_args()
    try:
        with args.manifest.open("r", encoding="utf-8") as handle:
            document = json.load(handle)
    except OSError as exc:
        print(
            json.dumps(
                {"status": "FAIL", "errors": [f"cannot read manifest: {exc}"]}, indent=2
            )
        )
        return 1
    except json.JSONDecodeError as exc:
        print(
            json.dumps(
                {
                    "status": "FAIL",
                    "errors": [
                        f"invalid JSON at line {exc.lineno}, column {exc.colno}: {exc.msg}"
                    ],
                },
                indent=2,
            )
        )
        return 1
    result, exit_code = validate_document(document, args.manifest, args.check_files)
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
