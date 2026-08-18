#!/usr/bin/env python3
"""Fail closed on common privacy and secret leaks in a public repository candidate."""

from __future__ import annotations

import argparse
import json
import re
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
    return sorted((root / name for name in names), key=lambda path: path.as_posix())


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


def content_findings(path: Path, root: Path) -> list[Finding]:
    if path.suffix.lower() in BINARY_SUFFIXES:
        return []
    try:
        raw = path.read_bytes()
    except OSError:
        return [Finding(path.relative_to(root).as_posix(), "unreadable candidate file")]
    if b"\0" in raw[:8192]:
        return [Finding(path.relative_to(root).as_posix(), "NUL byte in declared text candidate")]
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        return [Finding(path.relative_to(root).as_posix(), "invalid UTF-8 text candidate")]
    rel = path.relative_to(root).as_posix()
    findings: list[Finding] = []
    for offset, character in enumerate(text):
        if (ord(character) < 32 and character not in "\n\r\t") or ord(character) == 127:
            line_number = text.count("\n", 0, offset) + 1
            findings.append(Finding(rel, "invalid control character in text candidate", line_number))
            break
    for line_number, line in enumerate(text.splitlines(), 1):
        for rule in CONTENT_RULES:
            if rule.pattern.search(line):
                findings.append(Finding(rel, rule.name, line_number))
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
