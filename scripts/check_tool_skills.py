#!/usr/bin/env python3
"""Read-only verification of bundled Skill bytes and duplicate host entries.

This checks packaging, not runtime availability or engineering capability.
Host scans skip runtime/data directories; use a new Agent session to verify
actual discovery, which can have different traversal rules and limits.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def regular_file(root, relative):
    part = Path(relative)
    if part.is_absolute() or ".." in part.parts:
        raise ValueError("expected repository-relative path: " + relative)
    path = root / part
    if path.absolute() != path.resolve() or not path.is_file():
        raise ValueError("missing, non-regular or symlinked file: " + str(path))
    return path


def skill_name(path):
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        return None
    front = text.split("---", 2)[1]
    fields = re.findall(r"^name:[ \t]*(.*)$", front, re.MULTILINE)
    if len(fields) != 1:
        return None
    # Accept the normal scalar spellings, including YAML trailing comments.
    # Unrecognised YAML is reported as incomplete instead of silently ignored.
    match = re.fullmatch(r'''(?:([\w-]+)|"([\w-]+)"|'([\w-]+)')(?:[ \t]+\#.*)?[ \t]*''', fields[0])
    return next((part for part in match.groups() if part), None) if match else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--skill-root", type=Path, action="append", default=[],
                        help="Optional host Skill root to inspect for duplicate names; repeatable")
    args = parser.parse_args()
    root = args.root.absolute()
    errors, checked, expected = [], [], {}
    try:
        lock = json.loads(regular_file(root, "integrations/tools.lock.json").read_text())
        for tool in lock["tools"]:
            name = tool["skill_name"]
            directory = tool["skill_directory"]
            if name in expected or directory != "modules/" + name:
                raise ValueError("duplicate name or unexpected module directory: " + name)
            entry = regular_file(root, directory + "/SKILL.md")
            expected[name] = entry
            if skill_name(entry) != name:
                errors.append("entry name mismatch: " + str(entry))
            files = tool["skill_files"]
            if len({item["path"] for item in files}) != len(files):
                raise ValueError("duplicate file in lock: " + name)
            for item in files:
                relative = directory + "/" + item["path"]
                path = regular_file(root, relative)
                actual = digest(path)
                matched = actual == item["sha256"]
                checked.append({"path": relative, "sha256": actual, "matched": matched})
                if not matched:
                    errors.append("digest mismatch: " + relative)
            if digest(entry) != tool["integrated_skill_sha256"]:
                errors.append("entry digest mismatch: " + name)
            if tool.get("patch"):
                patch = regular_file(root, "integrations/" + tool["patch"])
                if digest(patch) != tool["patch_sha256"]:
                    errors.append("patch digest mismatch: " + name)
        if len(expected) != 8:
            errors.append("expected eight declared professional modules")
    except (OSError, ValueError, KeyError, TypeError) as error:
        errors.append(str(error))

    # These are packaging roots, not an emulation of any Agent's loader.
    skipped = {".git", ".system", "var", "sources", "node_modules", "__pycache__"}
    scan_roots = [root] + [p.absolute() for p in args.skill_root]
    seen, found = set(), {name: [] for name in expected}
    for scan_root in scan_roots:
        if not scan_root.exists():
            continue
        if scan_root.is_symlink() or scan_root.absolute() != scan_root.resolve():
            errors.append("symlinked scan root: " + str(scan_root))
            continue
        def scan_error(error):
            errors.append(str(error))
        for parent, directories, files in os.walk(scan_root, followlinks=False, onerror=scan_error):
            directories[:] = sorted(d for d in directories if d not in skipped)
            # Inspect a symlinked Skill entry without descending into its tree.
            candidates = [Path(parent) / d / "SKILL.md" for d in directories
                          if (Path(parent) / d).is_symlink()
                          and ((Path(parent) / d / "SKILL.md").exists()
                               or (Path(parent) / d / "SKILL.md").is_symlink())]
            if "SKILL.md" in files:
                candidates.append(Path(parent) / "SKILL.md")
            for candidate in candidates:
                if str(candidate) in seen:
                    continue
                seen.add(str(candidate))
                try:
                    name = skill_name(candidate)
                    if name is None:
                        raise ValueError("unrecognised Skill name/frontmatter: " + str(candidate))
                    if name in found:
                        found[name].append(str(candidate))
                except (OSError, ValueError) as error:
                    errors.append(str(error))
    for name, entries in found.items():
        if entries != [str(expected[name])]:
            errors.append("non-unique or unexpected entry for " + name + ": " + repr(entries))
    print(json.dumps({"status": "failed" if errors else "matched", "root": str(root),
                      "module_count": len(expected), "files": checked, "entries": found,
                      "scan_exclusions": sorted(skipped), "errors": errors,
                      "scope": "Skill packaging only; actual host discovery and runtime calls require separate verification"},
                     ensure_ascii=False, indent=2))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
