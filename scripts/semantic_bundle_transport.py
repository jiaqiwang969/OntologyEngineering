"""Hash-locked transport of Semantica packages; contains no semantic engine.

Frozen payloads remain Semantica-owned. This module validates transport bytes
and materializes a temporary input tree for the source-locked native runner.
It never creates a registry or reuses an originating organization's authority.
"""

from __future__ import annotations

from contextlib import contextmanager
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import stat
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = "runtime/semantic-bundles.json"
MAX_EXPANDED_BYTES = 16 * 1024 * 1024


class BundleError(ValueError):
    pass


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe_relative(value: str) -> str:
    if not isinstance(value, str) or not value or "\\" in value:
        raise BundleError("invalid relative path")
    path = PurePosixPath(value)
    if path.is_absolute() or any(x in {"", ".", ".."} for x in value.split("/")):
        raise BundleError("path must remain inside its package")
    if ":" in value or any(ord(c) < 32 for c in value):
        raise BundleError("invalid path characters")
    return path.as_posix()


def _regular_inside(root: Path, relative: str) -> Path:
    relative = safe_relative(relative)
    path = root / relative
    if not path.resolve().is_relative_to(root.resolve()):
        raise BundleError("bundle path escapes skill root")
    cursor = root
    for piece in PurePosixPath(relative).parts:
        cursor = cursor / piece
        if cursor.is_symlink():
            raise BundleError("bundle path contains a symbolic link")
    if not path.is_file():
        raise BundleError("declared bundle file is missing")
    return path


def validate_archive(data: bytes, spec: dict) -> tuple[dict[str, bytes], dict]:
    """Validate container, complete inventory, manifest, and native asset hashes."""
    if digest(data) != spec["sha256"]:
        raise BundleError("bundle hash mismatch")
    listed = spec["files"]
    expected = {safe_relative(x["path"]): x["sha256"] for x in listed}
    if len(expected) != len(listed):
        raise BundleError("duplicate locked file")
    payload = {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if sum(x.file_size for x in entries) > MAX_EXPANDED_BYTES:
            raise BundleError("bundle exceeds expanded-size limit")
        for entry in entries:
            name = safe_relative(entry.filename)
            if entry.is_dir() or stat.S_IFMT(entry.external_attr >> 16) not in {0, stat.S_IFREG}:
                raise BundleError("bundle must contain regular files only")
            if name in payload:
                raise BundleError("duplicate archive member")
            if name not in expected:
                raise BundleError("unlisted archive member")
            if PurePosixPath(name).suffix not in {".json", ".ttl", ".rq", ".facts", ".rules"}:
                raise BundleError("transport contains a non-data asset")
            value = archive.read(entry)
            if digest(value) != expected[name]:
                raise BundleError("member hash mismatch")
            payload[name] = value
    if set(payload) != set(expected):
        raise BundleError("incomplete bundle inventory")
    manifest_name = safe_relative(spec["manifest"])
    if digest(payload[manifest_name]) != spec["manifest_sha256"]:
        raise BundleError("manifest hash mismatch")
    manifest = json.loads(payload[manifest_name])
    if (manifest["package_id"], manifest["version"]) != (spec["package_id"], spec["package_version"]):
        raise BundleError("package identity mismatch")
    declared = manifest["assets"]
    if len(declared) != spec["asset_count"]:
        raise BundleError("asset count mismatch")
    assets = {safe_relative(x["path"]): x for x in declared}
    if len(assets) != len(declared) or len({x["asset_id"] for x in declared}) != len(declared):
        raise BundleError("duplicate native asset")
    if set(payload) != set(assets) | {manifest_name, "origin.json"}:
        raise BundleError("transport and native asset inventories differ")
    for name, asset in assets.items():
        if digest(payload[name]) != asset["sha256"]:
            raise BundleError("native asset hash mismatch")
    registry_name = safe_relative(spec["scenario_registry"])
    registry = json.loads(payload[registry_name])
    if registry["package_id"] != spec["package_id"]:
        raise BundleError("scenario registry identity mismatch")
    scenarios = [x["id"] for x in registry["scenarios"]]
    if len(scenarios) != spec["scenario_count"] or len(set(scenarios)) != len(scenarios):
        raise BundleError("scenario inventory mismatch")
    origin = json.loads(payload["origin.json"])
    if (origin["native_candidate_sha256"], origin["native_package_sha256"]) != (spec["candidate_sha256"], spec["package_sha256"]):
        raise BundleError("native origin digest mismatch")
    return payload, {"bundle_sha256": spec["sha256"], "package_id": spec["package_id"],
                     "package_version": spec["package_version"], "assets": len(assets),
                     "files": len(payload), "scenarios": scenarios,
                     "scope": "Frozen synthetic inputs; not registry installation or project authorization."}


def load_bundle(name: str, root: Path = ROOT) -> tuple[dict, dict[str, bytes], dict]:
    lock = json.loads(_regular_inside(root, LOCK_PATH).read_text(encoding="utf-8"))
    if lock["schema_version"] != "ontology-engineering.semantica-bundle-lock/v1":
        raise BundleError("unknown bundle lock schema")
    if name not in lock["bundles"]:
        raise BundleError("bundle is not declared in the skill")
    spec = lock["bundles"][name]
    runtime = json.loads(_regular_inside(root, "runtime/semantica-source-lock.json").read_text())
    actual = {"version": runtime["source"]["version"], "commit": runtime["source"]["commit"],
              "wheel_sha256": runtime["artifact"]["sha256"]}
    if spec["runtime"] != actual:
        raise BundleError("bundle requires a different source-locked runtime")
    payload, report = validate_archive(_regular_inside(root, spec["path"]).read_bytes(), spec)
    return spec, payload, report


@contextmanager
def materialized(spec: dict, payload: dict[str, bytes]):
    with tempfile.TemporaryDirectory(prefix="semantica-frozen-input-") as temporary:
        directory = Path(temporary)
        for relative, data in payload.items():
            path = directory / safe_relative(relative)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        yield directory / spec["manifest"]
