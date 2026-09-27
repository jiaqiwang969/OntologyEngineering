"""Provider data and private operational state belonging to this skill."""
from __future__ import annotations

from pathlib import Path
import json
import sqlite3

from .catalog import connect, metadata
from .source_pack import check_layout, load_pack
from ..local_paths import skill_path

SKILL_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MODEL = "jev-1.13.0"


def default_data_root(component="misumi"):
    """Resolve indexes beside this installation, never from a user's home cache."""
    return SKILL_ROOT / "sources/.indexes" / component


def runtime_config(data_root=None, state_root=None, *, include_misumi=True):
    data = Path(data_root).expanduser().resolve() if data_root else default_data_root()
    index, archive = data / "catalog.sqlite3", data / "archive"
    pack = None
    registration = data / "source.json"
    if include_misumi and registration.is_file():
        location = json.loads(registration.read_text())
        if location.get("schema") != "ontology-engineering.misumi-source-location/v1":
            raise ValueError("invalid_source_registration")
        index = (data / Path(location["index_path"]).expanduser()).resolve()
        archive = (data / Path(location["archive_root"]).expanduser()).resolve()
    elif include_misumi and (data / "source-pack.json").is_file():
        pack = data / "source-pack.json"
        archive = data / "cache/archive"
    state = (Path(state_root).expanduser().resolve() if state_root
             else SKILL_ROOT / "var/state/misumi")
    cache = state / "cache" if state_root else SKILL_ROOT / "var/cache/misumi"
    if any(path.is_relative_to(source) for path in (state, cache) for source in (data, archive)):
        raise ValueError("misumi_state_root_must_be_outside_source_data")
    return {"schema": "ontology-engineering.misumi-runtime/v1", "model": DEFAULT_MODEL,
            "data_root": str(data), "index_path": str(index),
            "archive_root": str(archive), "state_root": str(state), "cache_root": str(cache),
            "source_pack": str(pack) if pack else None}


def readiness(config):
    """Read-only layout/schema check; selected PDF hashes are checked at use."""
    missing = []
    index = Path(config["index_path"])
    archive = Path(config["archive_root"])
    compressed = bool(config.get("source_pack"))
    if not compressed and not index.is_file():
        missing.append({"kind": "index_missing", "path": str(index)})
    if not compressed and not archive.is_dir():
        missing.append({"kind": "archive_missing", "path": str(archive)})
    if compressed:
        try:
            manifest = load_pack(config["data_root"])
            missing.extend(check_layout(config["data_root"], manifest))
        except (OSError, ValueError) as exc:
            missing.append({"kind": str(exc).split(":", 1)[0], "path": config["source_pack"]})
    meta = None
    if index.is_file():
        try:
            with connect(index) as db:
                meta = metadata(db)
                for key in ("fingerprint", "built_at", "counts"):
                    if key not in meta:
                        raise ValueError("index_metadata_incomplete")
        except (OSError, ValueError, KeyError, sqlite3.Error):
            missing.append({"kind": "index_invalid", "path": str(index)})
    return {"schema": "ontology-engineering.jev-knowledge-readiness/v1",
            "status": "not_ready" if missing else "ready", "provider": "misumi_archive",
            "data_root": config["data_root"], "state_root": config["state_root"],
            "index_path": str(index), "archive_root": str(archive), "missing": missing,
            "source_mode": "local_lossless_shards" if compressed else "expanded_archive",
            "network_download": "not_run_manual_install_only",
            "verification": "index_hash_and_shard_layout_checked_selected_shard_and_page_hashes_checked_at_use" if compressed
                            else "layout_and_index_schema_only_selected_source_hashes_checked_at_use",
            "index": meta,
            "setup": {"required_layout": ["catalog.sqlite3", "source-pack.json", "segments/*.tar.xz"] if compressed
                                        else ["catalog.sqlite3", "archive/"],
                      "instruction": "Download or import the declared Drive files into <skill>/sources using scripts/source_library.py. Install with python3 runtime/misumi/setup.py install --from sources/misumi --data-root sources/.indexes/misumi. Repair missing shards from the same verified source set; queries never download. Only selected original pages are restored under cache/archive. Use restore-books explicitly for whole PDFs. See docs/PORTABLE-DISTRIBUTION.md for the exact layout.",
                      "check": "python3 scripts/jev_knowledge.py --data-root <data-directory> --status --json"}}


def prepare_state(path):
    state = Path(path)
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    state.chmod(0o700)
    return state
