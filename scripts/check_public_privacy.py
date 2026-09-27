#!/usr/bin/env python3
"""Fail closed on common privacy and secret leaks in a public repository candidate."""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


BINARY_SUFFIXES = {
    ".7z",
    ".avi",
    ".doc",
    ".docx",
    ".gif",
    ".gz",
    ".heic",
    ".jpeg",
    ".jpg",
    ".mov",
    ".mp3",
    ".mp4",
    ".pdf",
    ".png",
    ".ppt",
    ".pptx",
    ".tar",
    ".webp",
    ".whl",
    ".xls",
    ".xlsx",
    ".xz",
    ".zip",
}

FORBIDDEN_FILENAMES = {
    ".env",
    ".env.local",
    ".env.production",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "id_ed25519",
    "id_rsa",
}

FORBIDDEN_PARTS = {"private", "sessions", "rollouts", "__pycache__"}
FORBIDDEN_PATH_FRAGMENTS = {
    "sources/raw",
    "structured/mineru",
    "reports/imagegen-events/raw",
    ".claude/projects",
    ".codex/sessions",
}
FORBIDDEN_SUFFIXES = {".key", ".p12", ".pem", ".pfx", ".pyc"}


@dataclass(frozen=True)
class ContentRule:
    name: str
    pattern: re.Pattern[str]


CONTENT_RULES = (
    ContentRule(
        "macOS personal absolute path",
        re.compile(r"/Users/(?!<)[^/\s`'\"<>]+/"),
    ),
    ContentRule(
        "Linux personal absolute path",
        re.compile(r"/home/(?!<)[^/\s`'\"<>]+/"),
    ),
    ContentRule(
        "Windows personal absolute path",
        re.compile(r"[A-Za-z]:\\Users\\(?!<)[^\\\s`'\"<>]+\\"),
    ),
    ContentRule(
        "macOS attachment cache path",
        re.compile(r"/var/folders/[A-Za-z0-9_/.-]+"),
    ),
    ContentRule(
        "private key material",
        re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    ),
    ContentRule(
        "OpenAI-style secret token",
        re.compile(r"\bsk-[A-Za-z0-9_-]{20,}\b"),
    ),
    ContentRule(
        "GitHub secret token",
        re.compile(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b|\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
    ),
    ContentRule(
        "AWS access key",
        re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    ),
    ContentRule(
        "Jev API token",
        re.compile(r"\bapikey_[A-Za-z0-9_-]{32,}\b"),
    ),
    ContentRule(
        "clipboard attachment identifier",
        re.compile(r"codex-clipboard-[A-Za-z0-9_-]+"),
    ),
)


@dataclass(frozen=True)
class Finding:
    path: str
    rule: str
    line: int | None = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check tracked and unignored files for common public-release privacy leaks."
    )
    parser.add_argument("--root", type=Path, default=Path.cwd(), help="Repository root.")
    parser.add_argument(
        "--tracked-only",
        action="store_true",
        help="Check only tracked files instead of tracked plus unignored untracked files.",
    )
    parser.add_argument(
        "--include-ignored",
        action="store_true",
        help="Scan the full worktree, including ignored files; useful before making a manual archive.",
    )
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    return parser.parse_args()


def git_candidates(root: Path, tracked_only: bool) -> list[Path] | None:
    command = ["git", "ls-files", "-z"]
    if not tracked_only:
        command.extend(["--cached", "--others", "--exclude-standard"])
    result = subprocess.run(command, cwd=root, capture_output=True, check=False)
    if result.returncode != 0:
        return None
    names = [name for name in result.stdout.decode("utf-8", "surrogateescape").split("\0") if name]
    paths = [root / name for name in names]
    # ``git ls-files`` continues to report tracked paths deleted in the
    # worktree.  A release-candidate scan must evaluate the prospective tree,
    # not misclassify intentional deletions as special filesystem nodes.
    existing = [path for path in paths if path.exists() or path.is_symlink()]
    return sorted(existing, key=lambda path: path.as_posix())


def filesystem_candidates(root: Path) -> list[Path]:
    return sorted(
        (
            path
            for path in root.rglob("*")
            if (path.is_symlink() or not path.is_dir())
            and ".git" not in path.relative_to(root).parts
        ),
        key=lambda path: path.as_posix(),
    )


def path_findings(path: Path, root: Path) -> list[Finding]:
    raw_rel = path.relative_to(root).as_posix()
    has_control = any(ord(character) < 32 or ord(character) == 127 for character in raw_rel)
    rel = raw_rel.encode("unicode_escape").decode("ascii") if has_control else raw_rel
    parts = set(path.relative_to(root).parts)
    findings: list[Finding] = []
    if has_control:
        findings.append(Finding(rel, "control character in candidate path"))
    if path.is_symlink():
        findings.append(Finding(rel, "symbolic link is not allowed in a public candidate"))
    elif not path.is_file():
        findings.append(Finding(rel, "special filesystem node is not allowed in a public candidate"))
    if path.name in FORBIDDEN_FILENAMES or (
        path.name.startswith(".env.") and path.name != ".env.example"
    ):
        findings.append(Finding(rel, "credential/config filename"))
    if path.name == ".DS_Store":
        findings.append(Finding(rel, "OS metadata file"))
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        findings.append(Finding(rel, "secret or generated binary suffix"))
    if parts & FORBIDDEN_PARTS:
        findings.append(Finding(rel, "private/session/generated path component"))
    if any(fragment in rel for fragment in FORBIDDEN_PATH_FRAGMENTS):
        findings.append(Finding(rel, "forbidden private evidence path"))
    return findings


def _text_rules(text: str, rel: str, line: int | None = None) -> list[Finding]:
    findings = []
    if any((ord(c) < 32 and c not in "\n\r\t") or ord(c) == 127 for c in text):
        findings.append(Finding(rel, "invalid control character in text candidate", line))
    for rule in CONTENT_RULES:
        if rule.pattern.search(text):
            findings.append(Finding(rel, rule.name, line))
    return findings


def sqlite_findings(path: Path, root: Path) -> list[Finding]:
    """Read only declared SQL text values; never UTF-8-decode the database/BLOBs.

    This is a direct-identifier scan of live text, not a copyright or general
    forensic audit. A standalone vacuumed database is required: WAL/journal and
    free pages cannot be treated as checked live text.
    """
    rel = path.relative_to(root).as_posix()
    findings: list[Finding] = []
    if any(Path(str(path) + suffix).exists() for suffix in ("-wal", "-shm", "-journal")):
        return [Finding(rel, "SQLite candidate must be standalone without sidecars")]
    try:
        with path.open("rb") as source:
            if source.read(16) != b"SQLite format 3\x00":
                return [Finding(rel, "invalid SQLite candidate header")]
        connection = sqlite3.connect(path.resolve().as_uri() + "?mode=ro&immutable=1", uri=True)
        try:
            connection.execute("PRAGMA query_only=ON")
            connection.execute("PRAGMA trusted_schema=OFF")
            if connection.execute("PRAGMA freelist_count").fetchone()[0]:
                findings.append(Finding(rel, "SQLite free pages require a vacuumed distribution copy"))
            schema = connection.execute("SELECT type,name,sql FROM sqlite_schema ORDER BY name").fetchall()
            for kind, name, definition in schema:
                findings.extend(_text_rules(name, rel))
                if definition:
                    findings.extend(_text_rules(definition, rel))
                if kind != "table":
                    continue
                if definition and "CREATE VIRTUAL TABLE" in definition.upper() and not re.search(r"\bUSING\s+fts5\b", definition, re.I):
                    findings.append(Finding(rel, "unsupported SQLite virtual table for privacy scan"))
                    continue
                table = '"' + name.replace('"', '""') + '"'
                columns = connection.execute("PRAGMA table_xinfo(" + table + ")").fetchall()
                for column in columns:
                    identifier = '"' + column[1].replace('"', '""') + '"'
                    # Bound memory even if a malformed index contains a huge
                    # text field. Oversized values fail closed for separate review.
                    rows = connection.execute("SELECT length(" + identifier + "), substr(" + identifier
                        + ",1,1048576) FROM " + table + " WHERE typeof(" + identifier + ")='text'")
                    for length, value in rows:
                        if length > 1048576:
                            findings.append(Finding(rel, "SQLite text value exceeds bounded privacy scan"))
                            continue
                        findings.extend(_text_rules(value, rel))
        finally:
            connection.close()
    except (OSError, sqlite3.Error, UnicodeError):
        findings.append(Finding(rel, "unreadable or invalid SQLite candidate"))
    return list(dict.fromkeys(findings))


def content_findings(path: Path, root: Path) -> list[Finding]:
    if path.suffix.lower() in {".sqlite", ".sqlite3", ".db"}:
        return sqlite_findings(path, root)
    if path.suffix.lower() in BINARY_SUFFIXES:
        return []
    rel = path.relative_to(root).as_posix()
    findings: list[Finding] = []
    try:
        # Text files and JSON metadata are streamed. A pathological unbroken
        # line is rejected rather than loaded into unbounded memory.
        with path.open("r", encoding="utf-8") as source:
            line_number = 0
            while True:
                line = source.readline(16 * 1024 * 1024 + 1)
                if not line:
                    break
                line_number += 1
                if len(line) > 16 * 1024 * 1024:
                    return [Finding(rel, "text line exceeds bounded privacy scan", line_number)]
                if "\0" in line:
                    return [Finding(rel, "NUL byte in declared text candidate", line_number)]
                findings.extend(_text_rules(line, rel, line_number))
    except OSError:
        return [Finding(rel, "unreadable candidate file")]
    except UnicodeDecodeError:
        return [Finding(rel, "invalid UTF-8 text candidate")]
    return findings


def run(root: Path, tracked_only: bool, include_ignored: bool = False) -> tuple[list[Finding], int]:
    resolved = root.expanduser().resolve()
    if not resolved.is_dir():
        raise ValueError(f"root is not a directory: {resolved}")
    candidates = None if include_ignored else git_candidates(resolved, tracked_only)
    if candidates is None:
        candidates = filesystem_candidates(resolved)
    findings: list[Finding] = []
    for path in candidates:
        if path.is_symlink():
            findings.extend(path_findings(path, resolved))
            continue
        if not path.is_file():
            findings.extend(path_findings(path, resolved))
            continue
        findings.extend(path_findings(path, resolved))
        if any(
            ord(character) < 32 or ord(character) == 127
            for character in path.relative_to(resolved).as_posix()
        ):
            continue
        findings.extend(content_findings(path, resolved))
    unique = sorted(set(findings), key=lambda item: (item.path, item.line or 0, item.rule))
    return unique, len(candidates)


def main() -> int:
    args = parse_args()
    try:
        if args.tracked_only and args.include_ignored:
            raise ValueError("--tracked-only and --include-ignored are mutually exclusive")
        findings, checked = run(args.root, args.tracked_only, args.include_ignored)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(
            json.dumps(
                {
                    "ok": not findings,
                    "checked_files": checked,
                    "findings": [asdict(finding) for finding in findings],
                },
                ensure_ascii=False,
                indent=2,
            )
        )
    elif findings:
        print(f"privacy gate failed: {len(findings)} finding(s) in {checked} file(s)")
        for finding in findings:
            location = f"{finding.path}:{finding.line}" if finding.line else finding.path
            print(f"- {location}: {finding.rule}")
    else:
        print(f"privacy gate passed: checked {checked} file(s)")
    return 1 if findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
