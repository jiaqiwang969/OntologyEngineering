#!/usr/bin/env python3
"""Repack one pinned upstream wheel; never install or alter native/Python code."""
import argparse
import base64
import csv
import copy
import hashlib
import io
import json
from pathlib import Path
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def record_digest(data):
    return "sha256=" + base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("=")


def repack(source, output):
    manifest = json.loads(Path(__file__).with_name("z3-macos-wheel.json").read_text())
    raw = source.read_bytes()
    if digest(raw) != manifest["source_sha256"]:
        raise ValueError("upstream wheel SHA256 mismatch; no output created")
    with zipfile.ZipFile(io.BytesIO(raw)) as original:
        entries = original.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)):
            raise ValueError("duplicate wheel members")
        contents = {name: original.read(name) for name in names}
        wheel, record = manifest["metadata_path"], manifest["record_path"]
        if digest(contents[wheel]) != manifest["wheel_metadata_sha256"]:
            raise ValueError("upstream WHEEL metadata mismatch")
        records = list(csv.reader(io.StringIO(contents[record].decode())))
        if len(records) != len(names) or {row[0] for row in records} != set(names):
            raise ValueError("upstream RECORD membership mismatch")
        for name, checksum, size in records:
            if name == record:
                if checksum or size:
                    raise ValueError("RECORD must not hash itself")
            elif checksum != record_digest(contents[name]) or size != str(len(contents[name])):
                raise ValueError("upstream RECORD mismatch: " + name)
        old_line = ("Tag: " + manifest["original_tag"] + "\n").encode()
        if contents[wheel].count(old_line) != 1 or b"Build:" in contents[wheel]:
            raise ValueError("unexpected upstream tag/build fields")
        contents[wheel] = contents[wheel].replace(old_line, (
            "Build: " + manifest["build"] + "\nTag: " + manifest["replacement_tag"] + "\n"
        ).encode())
        rows = io.StringIO(newline="")
        writer = csv.writer(rows, lineterminator="\n")
        for name in names:
            writer.writerow([name, "", ""] if name == record else
                            [name, record_digest(contents[name]), len(contents[name])])
        contents[record] = rows.getvalue().encode()
        # Refuse all pre-existing destinations. A partial failure retains its new
        # directory and bytes for inspection, never deleting or replacing input.
        output.mkdir(parents=True, exist_ok=False)
        target = output / manifest["output_filename"]
        with zipfile.ZipFile(target, "x") as result:
            for entry in entries:
                result.writestr(copy.copy(entry), contents[entry.filename])
        with zipfile.ZipFile(target) as result:
            changed = [name for name in names if original.read(name) != result.read(name)]
            if set(changed) != set(manifest["changed_members"]):
                raise ValueError("unexpected payload change; do not install output")
            preserved = [{"path": name, "sha256": digest(result.read(name))}
                         for name in names if name not in changed]
    receipt = {
        "schema": "oev2.repacked-wheel/1", "status": "payload_preserved",
        "source_url": manifest["source_url"], "source_sha256": digest(raw),
        "wheel": str(target.resolve()), "wheel_sha256": digest(target.read_bytes()),
        "manifest_sha256": digest(Path(__file__).with_name("z3-macos-wheel.json").read_bytes()),
        "script_sha256": digest(Path(__file__).read_bytes()),
        "original_tag": manifest["original_tag"], "replacement_tag": manifest["replacement_tag"],
        "changed_members": changed, "unchanged_members": preserved,
        "installation_or_runtime_validation": "not_run"
    }
    (output / "REPACK.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New directory; must not exist")
    args = parser.parse_args()
    print(json.dumps(repack(args.wheel, args.output), indent=2))


if __name__ == "__main__":
    main()
