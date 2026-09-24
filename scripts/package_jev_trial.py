#!/usr/bin/env python3
"""Wrap an intact portable skill with an explicitly public temporary Jev key.

The reusable source tree and its privacy gate stay credential-free. This command
is for an owner-authorized public trial release; it prints hashes, never keys.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def build(skill_archive, credential_file, output):
    skill_archive, credential_file, output = map(Path, (skill_archive, credential_file, output))
    if output.exists() or output.resolve().is_relative_to(ROOT):
        raise ValueError("output_must_be_new_and_outside_skill")
    if skill_archive.is_symlink() or not skill_archive.is_file() or credential_file.is_symlink():
        raise ValueError("input_requires_regular_files")
    with zipfile.ZipFile(skill_archive) as source:
        inventory_name = "ontology-engineering/PORTABLE-MANIFEST.json"
        inventory_raw = source.read(inventory_name)
        inventory = json.loads(inventory_raw)
        if inventory.get("format") != "ontology-engineering.portable-skill/v1":
            raise ValueError("portable_skill_manifest_required")
        expected = {"ontology-engineering/"+x["path"]:x["sha256"] for x in inventory["files"]}
        if len(expected) != len(inventory["files"]) or inventory_name in expected:
            raise ValueError("invalid_portable_inventory")
        members = source.infolist()
        if len(members) != len(expected)+1 or {i.filename for i in members} != set(expected)|{inventory_name}:
            raise ValueError("archive_inventory_mismatch")
        payload = []
        for info in members:
            name = info.filename
            if (name.startswith("/") or "\\" in name or any(p in {"", ".", ".."} for p in name.split("/"))
                    or stat.S_ISLNK(info.external_attr >> 16) or info.is_dir()):
                raise ValueError("unsafe_archive_member")
            data = source.read(name)
            if name != inventory_name and digest(data) != expected[name]:
                raise ValueError("archive_member_digest_mismatch")
            payload.append((info, data))
        guide = source.read("ontology-engineering/docs/JEV-TRIAL.md")
    fd = os.open(credential_file, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    try:
        mode = os.fstat(fd)
        if not stat.S_ISREG(mode.st_mode) or mode.st_uid != os.getuid() or stat.S_IMODE(mode.st_mode) != 0o600:
            raise ValueError("credential_requires_owner_and_mode_0600")
        with os.fdopen(fd, "r", encoding="utf-8", closefd=False) as stream:
            text = stream.read(65537)
        keys = set(re.findall(r"apikey_[A-Za-z0-9_-]+", text))
        if len(text) > 65536 or len(keys) != 1:
            raise ValueError("credential_requires_one_unique_key")
        credential = (keys.pop()+"\n").encode()
    finally:
        os.close(fd)
    declaration = {
        "schema":"ontology-engineering.public-jev-trial/v1",
        "authorization":"owner-approved-public-temporary-credential",
        "credential_file":"api-jev.md", "credential_sha256":digest(credential),
        "skill_manifest_sha256":digest(inventory_raw), "base_archive_sha256":digest(skill_archive.read_bytes()),
        "meaning":"Shared public trial credential. Revocable; not a private key or a signed authorization record.",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "x", zipfile.ZIP_DEFLATED) as target:
        for info, data in payload:
            target.writestr(info, data)
        extras = {
            "api-jev.md":(credential, 0o600),
            "JEV-TRIAL-MANIFEST.json":((json.dumps(declaration, indent=2)+"\n").encode(), 0o644),
            "TRY-JEV.md":(guide, 0o644),
        }
        for name, (raw, permissions) in extras.items():
            info = zipfile.ZipInfo(name)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (stat.S_IFREG | permissions) << 16
            target.writestr(info, raw)
    return {"status":"built", "public_trial_credential_included":True,
            "base_archive_sha256":declaration["base_archive_sha256"],
            "archive_sha256":digest(output.read_bytes()), "bytes":output.stat().st_size,
            "skill_members_unchanged":len(payload)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skill-archive", type=Path, required=True)
    parser.add_argument("--credential-file", type=Path, required=True, help="Owner-authorized public temporary key; never use a private production key.")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = build(args.skill_archive, args.credential_file, args.output)
    except (ValueError, OSError, KeyError, zipfile.BadZipFile):
        print(json.dumps({"status":"error", "code":"public_trial_packaging_failed"}))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
