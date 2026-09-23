#!/usr/bin/env python3
"""Verify the CAD module contains only its locked Fusion execution wheel."""

from __future__ import annotations

import argparse
import ast
import base64
import configparser
import csv
import hashlib
import importlib.metadata
import importlib.util
import io
import json
from pathlib import Path
from zipfile import ZipFile


SKILL_DIR = Path(__file__).resolve().parents[1]
DIST_DIR = SKILL_DIR / "dist"
LOCK = DIST_DIR / "fusion-runtime-lock.json"
FORBIDDEN_IMPORTS = {"cad_agent", "cad_geometry_mcp", "rdflib", "pyshacl", "semantica"}


def _sha256(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def verify(*, installed: bool = False) -> dict[str, object]:
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    if lock.get("schema_version") != "ontology-engineering.cad-fusion-runtime-lock/v1":
        raise ValueError("unexpected Fusion runtime lock schema")
    wheel_name = lock["wheel_filename"]
    wheels = sorted(path.name for path in DIST_DIR.glob("*.whl"))
    if wheels != [wheel_name]:
        raise ValueError(f"CAD module must contain exactly the locked Fusion wheel: {wheels}")
    wheel = DIST_DIR / wheel_name
    if _sha256(wheel.read_bytes()) != lock["wheel_sha256"]:
        raise ValueError("Fusion runtime wheel hash mismatch")

    dist_info = "oe_cad_fusion_runtime-0.1.0+oe.1.dist-info/"
    with ZipFile(wheel) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("duplicate wheel members")
        for name in names:
            if name.startswith("/") or ".." in Path(name).parts or name.endswith("/"):
                raise ValueError(f"invalid wheel member: {name}")
            if not (name.startswith("fusion_mcp_proxy/") or name.startswith(dist_info)):
                raise ValueError(f"non-Fusion content in CAD runtime wheel: {name}")
            if name.endswith((".ttl", ".rq", ".owl", ".trig", ".nq")):
                raise ValueError(f"semantic asset in Fusion runtime wheel: {name}")
            if name.endswith(".py"):
                tree = ast.parse(archive.read(name), filename=name)
                for node in ast.walk(tree):
                    if isinstance(node, ast.Import):
                        modules = [alias.name.split(".", 1)[0] for alias in node.names]
                    elif isinstance(node, ast.ImportFrom) and node.level == 0:
                        modules = [(node.module or "").split(".", 1)[0]]
                    else:
                        continue
                    if FORBIDDEN_IMPORTS.intersection(modules):
                        raise ValueError(f"semantic backend import in Fusion wheel: {name}")
        if "fusion_mcp_proxy/cell_server.py" not in names:
            raise ValueError("Fusion cell server missing")
        entry_path = dist_info + "entry_points.txt"
        config = configparser.ConfigParser()
        config.read_string(archive.read(entry_path).decode("utf-8"))
        entries = sorted(config["console_scripts"])
        if entries != sorted(lock["console_scripts"]):
            raise ValueError(f"Fusion runtime console scripts differ: {entries}")
        record_path = dist_info + "RECORD"
        rows = list(csv.reader(io.StringIO(archive.read(record_path).decode("utf-8"))))
        if {row[0] for row in rows} != set(names):
            raise ValueError("wheel RECORD member list differs")
        for name, digest, size in rows:
            if name == record_path:
                if digest or size:
                    raise ValueError("wheel RECORD self-entry must be unhashed")
                continue
            content = archive.read(name)
            expected = "sha256=" + base64.urlsafe_b64encode(
                hashlib.sha256(content).digest()
            ).rstrip(b"=").decode("ascii")
            if digest != expected or size != str(len(content)):
                raise ValueError(f"wheel RECORD mismatch: {name}")

    if installed:
        distribution = importlib.metadata.distribution(lock["package_name"])
        if distribution.version != lock["package_version"]:
            raise ValueError("installed Fusion runtime version differs")
        for module in ("cad_agent", "cad_geometry_mcp"):
            if importlib.util.find_spec(module) is not None:
                raise ValueError(f"legacy semantic module still importable: {module}")
        if importlib.util.find_spec("fusion_mcp_proxy.cell_server") is None:
            raise ValueError("installed Fusion cell server missing")
    return {
        "passed": True,
        "wheel": wheel_name,
        "wheel_sha256": lock["wheel_sha256"],
        "installed_checked": installed,
        "semantic_backend_in_wheel": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--installed", action="store_true")
    args = parser.parse_args()
    print(json.dumps(verify(installed=args.installed), ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
