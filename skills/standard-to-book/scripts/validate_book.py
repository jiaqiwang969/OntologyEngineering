#!/usr/bin/env python3
"""Validate a standard-to-book package at structure, charter or release stage."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
from datetime import date, datetime
from pathlib import Path


REQUIRED_FILES = (
    "book.yaml",
    "book-charter.md",
    "sources/source-register.csv",
    "cqs/cq-register.csv",
    "chapters/chapter-register.csv",
    "propositions/proposition-register.csv",
    "ontology/package-manifest.yaml",
    "figures/figure-register.csv",
    "release/public-assets.csv",
    "release/package-lock.csv",
    "privacy/public-export.yaml",
    "skill/SKILL.md",
)
MANIFESTABLE_REQUIRED_FILES = {"skill/SKILL.md"}
SUPPORTED_SCHEMA_VERSION = "1.0"
MAX_TEST_REPORT_BYTES = 1_000_000
BOOK_SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
REQUIRED_CHARTER_SECTIONS = (
    "目标读者",
    "目标标准或知识域",
    "要解决的制造业问题",
    "读完后允许作出的决定",
    "必须升级给专家或责任人的决定",
    "适用范围与排除范围",
    "审阅责任",
    "公共与私有边界",
    "初始能力问题",
)
REQUIRED_PRIVACY_PATHS = {
    "sources/raw",
    "private",
    "evidence",
    "sessions",
    "rollouts",
}
REQUIRED_PRIVACY_CONTENT_RULES = {
    "standard originals or restricted extracts",
    "enterprise, customer, supplier, worker or product identifiers",
    "personal absolute paths",
    "credentials, tokens, keys or cookies",
    "private model sessions or attachment caches",
    "assets with pending input rights",
}

FORBIDDEN_PACKAGE_PATHS = (
    "private",
    "sources/raw",
    "evidence",
    "sessions",
    "rollouts",
)
FORBIDDEN_ANYWHERE_PARTS = {"evidence", "private", "rollouts", "sessions"}
APPROVED = {"approved", "accepted", "passed", "released"}
RELEASE_BOOK_STATUSES = {"release-candidate", "released"}
RIGHTS_STATUSES = {
    "author-owned",
    "cleared-for-declared-use",
    "open-licensed",
    "permission-granted",
    "public-domain",
}
ID_RE = re.compile(r"^[A-Za-z][A-Za-z0-9._:-]{2,127}$")
REF_SPLIT_RE = re.compile(r"[;,|\s]+")
ASSET_ROLES = {
    "constraint",
    "figure",
    "fixture",
    "metadata",
    "ontology",
    "query",
    "reader-book",
    "script",
    "skill",
    "style",
    "test-report",
}
ROLE_SUFFIXES = {
    "constraint": {".shacl", ".ttl"},
    "figure": {".jpeg", ".jpg", ".pdf", ".png", ".svg", ".webp"},
    "fixture": {".json", ".rdf", ".ttl"},
    "metadata": {".csv", ".json", ".md", ".txt", ".yaml", ".yml"},
    "ontology": {".owl", ".rdf", ".ttl"},
    "query": {".rq", ".sparql"},
    "reader-book": {".epub", ".html", ".md", ".pdf"},
    "script": {".py"},
    "skill": {".md"},
    "style": {".sty", ".tex"},
    "test-report": {".json"},
}
CLAIM_CLASSES = {
    "author-explanation",
    "best-practice",
    "standard-grounded",
    "teaching-assumption",
}
ONTOLOGY_BINDINGS = {
    "tbox": {"ontology"},
    "controlled_abox_or_adapter": {"fixture", "ontology", "script"},
    "queries": {"query"},
    "constraints": {"constraint"},
    "positive_fixtures": {"fixture"},
    "single_fault_negative_fixtures": {"fixture"},
    "runner": {"script"},
}
ALLOWED_ASSET_SUFFIXES = {
    ".csv", ".epub", ".html", ".jpeg", ".jpg", ".json", ".md", ".owl", ".pdf",
    ".png", ".py", ".rdf", ".rq", ".shacl", ".sparql", ".sty", ".svg", ".tex",
    ".ttl", ".txt", ".webp", ".yaml", ".yml",
}
PENDING_MARKERS = ("pending", "unknown", "todo", "tbd", "review_required", "not-cleared")
PLACEHOLDER_RE = re.compile(
    r"^(?:todo|tbd|pending|unknown|review_required|not[-_ ]cleared|待定|待补|未知)(?:$|\s|[:：])",
    re.IGNORECASE,
)
INLINE_PLACEHOLDER_RE = re.compile(
    r"\b(?:todo|tbd|review_required)\b|not[-_ ]cleared|待补|待定",
    re.IGNORECASE,
)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
BINARY_SUFFIXES = {".pdf", ".png", ".jpg", ".jpeg", ".webp", ".zip", ".gz"}
PRIVACY_PATTERNS = (
    ("personal macOS path", re.compile(r"/" + r"Users/(?!<)[^/\s`'\"<>]+/")),
    ("personal Linux path", re.compile(r"/" + r"home/(?!<)[^/\s`'\"<>]+/")),
    ("personal Windows path", re.compile(r"[A-Za-z]:\\Users\\(?!<)[^\\\s`'\"<>]+\\")),
    ("macOS attachment cache path", re.compile(r"/var/folders/[A-Za-z0-9_/.-]+")),
    ("private key", re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----")),
    ("secret token", re.compile(r"\b(?:sk-[A-Za-z0-9_-]{20,}|ghp_[A-Za-z0-9]{20,})\b")),
    (
        "GitHub secret token",
        re.compile(
            r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b|"
            r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"
        ),
    ),
    ("AWS access key", re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b")),
    ("clipboard attachment identifier", re.compile(r"codex-clipboard-[A-Za-z0-9_-]+")),
)
FORBIDDEN_CANDIDATE_FILENAMES = {
    ".env",
    ".env.local",
    ".env.production",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "id_ed25519",
    "id_rsa",
}

CSV_HEADERS = {
    "sources/source-register.csv": [
        "source_id",
        "title",
        "edition",
        "content_owner",
        "rights_basis",
        "rights_status",
        "private_logical_id",
        "sha256",
        "public_distribution",
        "technical_review",
        "notes",
    ],
    "cqs/cq-register.csv": [
        "cq_id",
        "question",
        "reader_decision",
        "evidence_required",
        "expected_answer_form",
        "acceptance_oracle",
        "status",
    ],
    "chapters/chapter-register.csv": [
        "chapter_id",
        "title",
        "reader_problem",
        "cq_ids",
        "source_ids",
        "figure_ids",
        "review_status",
    ],
    "propositions/proposition-register.csv": [
        "proposition_id",
        "chapter_id",
        "cq_ids",
        "source_ids",
        "statement_summary",
        "claim_class",
        "authority_limit",
        "evidence_oracle",
        "review_status",
    ],
    "figures/figure-register.csv": [
        "figure_id",
        "chapter_id",
        "source_ids",
        "visual_question",
        "semantic_baseline",
        "input_rights",
        "input_rights_status",
        "generator",
        "sha256",
        "caption",
        "alt_text",
        "release_status",
    ],
    "release/public-assets.csv": [
        "asset_id",
        "relative_path",
        "asset_role",
        "chapter_ids",
        "figure_id",
        "sha256",
        "creator_or_method",
        "rights_basis",
        "rights_status",
        "contains_personal_data",
        "technical_review",
        "privacy_review",
        "release_status",
    ],
    "release/package-lock.csv": ["relative_path", "sha256"],
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate a standard-to-book package.")
    parser.add_argument("package", type=Path, help="Book package root.")
    parser.add_argument(
        "--stage",
        choices=("structure", "charter", "release"),
        default="structure",
        help="Validation maturity stage.",
    )
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    parser.add_argument(
        "--write-lock",
        action="store_true",
        help="Freeze hashes for every package file except release/package-lock.csv before validating.",
    )
    return parser.parse_args()


def csv_rows(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    try:
        with path.open(encoding="utf-8", newline="") as handle:
            reader = csv.DictReader(handle)
            return list(reader.fieldnames or []), list(reader)
    except (OSError, UnicodeError, csv.Error):
        return ["__invalid_or_unreadable_csv__"], []


def simple_yaml(path: Path) -> dict[str, object]:
    values: dict[str, object] = {}
    parse_errors: list[str] = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError):
        return {"__parse_errors__": ["file is not readable UTF-8"]}
    current_list_key: str | None = None
    for line_number, line in enumerate(lines, 1):
        if not line or line.startswith("#"):
            continue
        if line.startswith(" "):
            if current_list_key is None or not line.startswith("  - "):
                parse_errors.append(f"unsupported indented mapping at line {line_number}")
                continue
            item_raw = line[4:].strip()
            try:
                item = json.loads(item_raw)
            except json.JSONDecodeError:
                parse_errors.append(
                    f"malformed quoted list item for {current_list_key} at line {line_number}"
                )
                continue
            if not isinstance(item, str):
                parse_errors.append(
                    f"list item for {current_list_key} must be a quoted string"
                )
                continue
            list_value = values.get(current_list_key)
            if isinstance(list_value, list):
                list_value.append(item)
            continue
        current_list_key = None
        if ":" not in line:
            parse_errors.append(f"malformed top-level line {line_number}")
            continue
        raw_key, raw = line.split(":", 1)
        key = raw_key.strip()
        if raw_key != key or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*", key):
            parse_errors.append(f"non-canonical top-level key at line {line_number}")
        if key in values:
            parse_errors.append(f"duplicate top-level key {key} at line {line_number}")
            continue
        value = raw.strip()
        if not value:
            values[key] = []
            current_list_key = key
            continue
        try:
            parsed = json.loads(value)
        except json.JSONDecodeError:
            parse_errors.append(f"malformed quoted scalar for {key} at line {line_number}")
            continue
        if not isinstance(parsed, str):
            parse_errors.append(f"top-level scalar for {key} must be a quoted string")
            continue
        values[key] = parsed
    if parse_errors:
        values["__parse_errors__"] = parse_errors
    return values


def simple_frontmatter(text: str) -> dict[str, str] | None:
    lines = text.splitlines()
    if not lines or lines[0] != "---":
        return None
    try:
        end = lines.index("---", 1)
    except ValueError:
        return None
    values: dict[str, str] = {}
    for line in lines[1:end]:
        if not line or line.startswith("#"):
            continue
        if line.startswith(" ") or ":" not in line:
            return None
        key, raw = line.split(":", 1)
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*", key):
            return None
        raw = raw.strip()
        if not raw:
            return None
        if key in values:
            return None
        try:
            value = json.loads(raw)
        except json.JSONDecodeError:
            return None
        if not isinstance(value, str):
            return None
        values[key] = value
    return values


def append_yaml_errors(values: dict[str, object], label: str, errors: list[str]) -> None:
    parse_errors = values.get("__parse_errors__")
    if isinstance(parse_errors, list):
        for error in parse_errors:
            errors.append(f"{label}: {error}")


def reject_duplicate_json_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
    values: dict[str, object] = {}
    for key, value in pairs:
        if key in values:
            raise ValueError(f"duplicate JSON key: {key}")
        values[key] = value
    return values


def reject_nonstandard_json_constant(value: str) -> object:
    raise ValueError(f"non-standard JSON constant: {value}")


def markdown_sections(text: str) -> dict[str, str]:
    sections: dict[str, list[str]] = {}
    current: str | None = None
    for line in text.splitlines():
        if line.startswith("## "):
            current = line[3:].strip()
            sections.setdefault(current, [])
        elif current is not None:
            sections[current].append(line)
    return {name: "\n".join(lines).strip() for name, lines in sections.items()}


def has_substantive_markdown(text: str) -> bool:
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        value = re.sub(r"^[\s>*+-]+", "", stripped).strip()
        if (
            len(value) >= 2
            and not PLACEHOLDER_RE.match(value)
            and not INLINE_PLACEHOLDER_RE.search(value)
        ):
            return True
    return False


def text_value(value: object) -> str:
    return value.strip() if isinstance(value, str) else ""


def has_unresolved_marker(value: object) -> bool:
    normalized = text_value(value).lower()
    return not normalized or any(marker in normalized for marker in PENDING_MARKERS)


def normalized(value: object) -> str:
    return text_value(value).lower()


def parse_refs(value: object) -> list[str]:
    """Parse a semicolon-first ID list while tolerating common CSV separators."""
    return [item for item in REF_SPLIT_RE.split(text_value(value)) if item]


def index_rows(
    rows: list[dict[str, str]], key: str, label: str, errors: list[str]
) -> dict[str, dict[str, str]]:
    indexed: dict[str, dict[str, str]] = {}
    for index, row in enumerate(rows, 2):
        value = (row.get(key) or "").strip()
        if not ID_RE.fullmatch(value):
            errors.append(f"{label} row {index} has an invalid {key}")
            continue
        if value in indexed:
            errors.append(f"{label} row {index} duplicates {key} {value}")
            continue
        indexed[value] = row
    return indexed


def require_fields(
    row: dict[str, str], fields: tuple[str, ...], label: str, index: int, errors: list[str]
) -> None:
    for field in fields:
        if not (row.get(field) or "").strip():
            errors.append(f"{label} row {index} lacks {field}")


def reject_placeholders(
    row: dict[str, str], fields: tuple[str, ...], label: str, index: int, errors: list[str]
) -> None:
    for field in fields:
        value = text_value(row.get(field))
        if PLACEHOLDER_RE.match(value) or INLINE_PLACEHOLDER_RE.search(value):
            errors.append(f"{label} row {index} has unresolved placeholder in {field}")


def check_refs(
    refs: list[str], known: set[str], label: str, errors: list[str], *, required: bool = True
) -> None:
    if required and not refs:
        errors.append(f"{label} has no references")
    seen: set[str] = set()
    for ref in refs:
        if ref in seen:
            errors.append(f"{label} duplicates reference {ref}")
        seen.add(ref)
        if ref not in known:
            errors.append(f"{label} references unknown ID {ref}")


def write_package_lock(root: Path) -> Path:
    """Write an integrity snapshot without following symlinks."""
    if not root.is_dir():
        raise ValueError(f"package is not a directory: {root}")
    lock_path = root / "release" / "package-lock.csv"
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    rows: list[tuple[str, str]] = []
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if ".git" in relative.parts or relative.as_posix() == "release/package-lock.csv":
            continue
        if has_path_control_characters(relative.as_posix()):
            raise ValueError("refusing to freeze a path containing control characters")
        if path.is_symlink():
            raise ValueError(f"refusing to freeze symbolic link: {relative.as_posix()}")
        if path.is_file():
            try:
                rows.append((relative.as_posix(), sha256(path)))
            except OSError as exc:
                raise ValueError(f"refusing to freeze unreadable file: {relative.as_posix()}") from exc
        elif not path.is_dir():
            raise ValueError(f"refusing to freeze special filesystem node: {relative.as_posix()}")
    with lock_path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(CSV_HEADERS["release/package-lock.csv"])
        writer.writerows(sorted(rows))
    return lock_path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_sha256(path: Path) -> str | None:
    try:
        return sha256(path)
    except OSError:
        return None


def has_path_control_characters(value: str) -> bool:
    return any(ord(character) < 32 or ord(character) == 127 for character in value)


def safe_package_asset(root: Path, raw: object) -> tuple[Path | None, str | None]:
    if not isinstance(raw, str):
        return None, "asset path must be a string"
    raw = raw.strip()
    if not raw:
        return None, "empty relative_path"
    if has_path_control_characters(raw):
        return None, "asset path contains control characters"
    if "\\" in raw:
        return None, "asset path must use canonical POSIX separators"
    relative = Path(raw)
    if relative.is_absolute() or re.match(r"^[A-Za-z]:[\\/]", raw):
        return None, "absolute asset path"
    if ".." in relative.parts:
        return None, "asset path escapes package"
    if relative.as_posix() != raw or "." in relative.parts:
        return None, "asset path is not canonical"
    if forbidden_package_path(relative):
        return None, "asset path uses a forbidden private/session component"
    candidate = (root / relative).resolve()
    try:
        candidate.relative_to(root.resolve())
    except ValueError:
        return None, "asset path resolves outside package"
    return candidate, None


def forbidden_package_path(relative: Path) -> bool:
    parts = tuple(part.lower() for part in relative.parts)
    if set(parts) & FORBIDDEN_ANYWHERE_PARTS:
        return True
    return any(parts[index:index + 2] == ("sources", "raw") for index in range(len(parts) - 1))


def privacy_findings(root: Path) -> list[str]:
    errors: list[str] = []
    for path in root.rglob("*"):
        if path.is_symlink():
            continue
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if (
            path.name == ".DS_Store"
            or path.name in FORBIDDEN_CANDIDATE_FILENAMES
            or path.name.startswith(".env.") and path.name != ".env.example"
        ):
            errors.append(f"forbidden private/metadata file: {rel}")
        if path.suffix.lower() in {".key", ".pem", ".p12", ".pfx", ".pyc"}:
            errors.append(f"forbidden secret/generated file: {rel}")
        if path.suffix.lower() in BINARY_SUFFIXES:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            errors.append(f"unreadable public candidate file: {rel}")
            continue
        except UnicodeDecodeError:
            errors.append(f"unrecognized binary public candidate file: {rel}")
            continue
        if any(
            (ord(character) < 32 and character not in "\n\r\t") or ord(character) == 127
            for character in text
        ):
            errors.append(f"invalid control character in text public candidate: {rel}")
        for name, pattern in PRIVACY_PATTERNS:
            if pattern.search(text):
                errors.append(f"{name} in {rel}")
    return errors


def validate_structure(root: Path) -> list[str]:
    errors: list[str] = []
    for relative in REQUIRED_FILES:
        if not (root / relative).is_file():
            errors.append(f"missing required file: {relative}")
    for relative, expected in CSV_HEADERS.items():
        path = root / relative
        if not path.is_file():
            continue
        actual, rows = csv_rows(path)
        if actual != expected:
            errors.append(f"unexpected CSV header: {relative}")
        for index, row in enumerate(rows, 2):
            if None in row or any(value is None for value in row.values()):
                errors.append(f"malformed CSV row {index}: {relative}")
    for relative in FORBIDDEN_PACKAGE_PATHS:
        if (root / relative).exists():
            errors.append(f"private evidence path exists inside public package: {relative}")
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if ".git" in relative.parts:
            continue
        if has_path_control_characters(relative.as_posix()):
            errors.append("public package contains a path with control characters")
        if forbidden_package_path(relative):
            errors.append(
                f"forbidden private/session path exists inside public package: {relative.as_posix()}"
            )
    if (root / "book.yaml").is_file():
        metadata = simple_yaml(root / "book.yaml")
        append_yaml_errors(metadata, "book.yaml", errors)
        if metadata.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
            errors.append("book.yaml schema_version is unsupported")
        slug = text_value(metadata.get("slug"))
        if not BOOK_SLUG_RE.fullmatch(slug):
            errors.append("book.yaml slug is missing or invalid")
        for key in ("title", "standard_family", "audience", "mission"):
            value = text_value(metadata.get(key))
            if not value or PLACEHOLDER_RE.match(value):
                errors.append(f"book.yaml {key} is missing or unresolved")
        created = text_value(metadata.get("created"))
        try:
            date.fromisoformat(created)
        except ValueError:
            errors.append("book.yaml created is not a valid ISO date")
        expected_policies = {
            "source_policy": "private-controlled",
            "private_evidence_location": "external-required",
            "private_evidence_linkage": "logical-id-and-sha256-only",
            "public_release_policy": "allowlist",
        }
        for key, expected in expected_policies.items():
            if metadata.get(key) != expected:
                errors.append(f"book.yaml {key} must be {expected}")
    return errors


def validate_charter(root: Path) -> list[str]:
    errors = validate_structure(root)
    charter = root / "book-charter.md"
    if charter.is_file():
        try:
            charter_text = charter.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            errors.append("book-charter.md is not readable UTF-8")
            charter_text = ""
        if re.search(r"\b(?:TODO|TBD)\b", charter_text, re.IGNORECASE):
            errors.append("book-charter.md still contains TODO/TBD")
        sections = markdown_sections(charter_text)
        for section in REQUIRED_CHARTER_SECTIONS:
            if section not in sections:
                errors.append(f"book-charter.md lacks required section: {section}")
            elif not has_substantive_markdown(sections[section]):
                errors.append(f"book-charter.md section has no substantive content: {section}")
        reviewer_section = sections.get("审阅责任", "")
        for reviewer in ("领域审阅者", "普通工程师冷读者", "权利与隐私审阅者", "发布责任人"):
            match = re.search(rf"(?m)^-\s*{re.escape(reviewer)}：\s*(.+)$", reviewer_section)
            if not match or not has_substantive_markdown(match.group(1)):
                errors.append(f"book-charter.md lacks a resolved {reviewer}")
    source_path = root / "sources" / "source-register.csv"
    if source_path.is_file():
        _, rows = csv_rows(source_path)
        if not rows:
            errors.append("source register has no source")
        for index, row in enumerate(rows, 2):
            if not row.get("private_logical_id") or not row.get("rights_basis"):
                errors.append(f"source row {index} lacks logical ID or rights basis")
    cq_path = root / "cqs" / "cq-register.csv"
    if cq_path.is_file():
        _, rows = csv_rows(cq_path)
        if not 10 <= len(rows) <= 30:
            errors.append(f"charter stage requires 10-30 competency questions; found {len(rows)}")
        for index, row in enumerate(rows, 2):
            if not row.get("question") or not row.get("acceptance_oracle"):
                errors.append(f"CQ row {index} lacks question or acceptance oracle")
    return errors


def validate_release(root: Path) -> list[str]:
    """Fail closed unless the package forms one internally consistent release graph."""
    errors = validate_charter(root)
    control_files = (set(REQUIRED_FILES) - MANIFESTABLE_REQUIRED_FILES) | {
        ".gitignore",
        "README.md",
    }

    metadata_path = root / "book.yaml"
    metadata: dict[str, str] = {}
    if metadata_path.is_file():
        metadata = simple_yaml(metadata_path)
        if normalized(metadata.get("status")) not in RELEASE_BOOK_STATUSES:
            errors.append("book.yaml status is not release-candidate or released")
        for key in ("rights_status", "technical_review_status", "reader_review_status"):
            if normalized(metadata.get(key)) not in APPROVED:
                errors.append(f"book.yaml {key} is not approved")

    source_path = root / "sources" / "source-register.csv"
    source_rows: list[dict[str, str]] = []
    if source_path.is_file():
        _, source_rows = csv_rows(source_path)
    source_by_id = index_rows(source_rows, "source_id", "source", errors)
    logical_ids: set[str] = set()
    for index, row in enumerate(source_rows, 2):
        require_fields(
            row,
            (
                "source_id", "title", "edition", "content_owner", "rights_basis",
                "rights_status", "private_logical_id", "sha256", "public_distribution",
                "technical_review",
            ),
            "source", index, errors,
        )
        reject_placeholders(
            row, ("title", "edition", "content_owner", "rights_basis"),
            "source", index, errors,
        )
        if has_unresolved_marker(row.get("rights_basis")):
            errors.append(f"source row {index} rights basis is unresolved")
        if normalized(row.get("rights_status")) not in RIGHTS_STATUSES:
            errors.append(f"source row {index} rights status is not cleared")
        if normalized(row.get("technical_review")) not in APPROVED:
            errors.append(f"source row {index} technical review is not approved")
        if normalized(row.get("public_distribution")) not in {"yes", "no", "true", "false"}:
            errors.append(f"source row {index} lacks an explicit public_distribution decision")
        if not SHA256_RE.fullmatch(normalized(row.get("sha256"))):
            errors.append(f"source row {index} has an invalid sha256")
        logical_id = (row.get("private_logical_id") or "").strip()
        if not ID_RE.fullmatch(logical_id):
            errors.append(f"source row {index} does not use a strict logical ID")
        elif logical_id in logical_ids:
            errors.append(f"source row {index} duplicates private_logical_id {logical_id}")
        if logical_id:
            logical_ids.add(logical_id)

    cq_path = root / "cqs" / "cq-register.csv"
    cq_rows: list[dict[str, str]] = []
    if cq_path.is_file():
        _, cq_rows = csv_rows(cq_path)
    cq_by_id = index_rows(cq_rows, "cq_id", "CQ", errors)
    for index, row in enumerate(cq_rows, 2):
        require_fields(
            row,
            (
                "cq_id", "question", "reader_decision", "evidence_required",
                "expected_answer_form", "acceptance_oracle", "status",
            ),
            "CQ", index, errors,
        )
        reject_placeholders(
            row,
            (
                "question", "reader_decision", "evidence_required",
                "expected_answer_form", "acceptance_oracle",
            ),
            "CQ", index, errors,
        )
        if normalized(row.get("status")) not in APPROVED:
            errors.append(f"CQ row {index} is not approved")

    chapter_path = root / "chapters" / "chapter-register.csv"
    chapter_rows: list[dict[str, str]] = []
    if chapter_path.is_file():
        _, chapter_rows = csv_rows(chapter_path)
    if not chapter_rows:
        errors.append("release has no registered chapter")
    chapter_by_id = index_rows(chapter_rows, "chapter_id", "chapter", errors)
    chapter_cq_refs: dict[str, list[str]] = {}
    chapter_source_refs: dict[str, list[str]] = {}
    chapter_figure_refs: dict[str, list[str]] = {}
    covered_cqs: set[str] = set()
    for index, row in enumerate(chapter_rows, 2):
        require_fields(
            row,
            ("chapter_id", "title", "reader_problem", "cq_ids", "source_ids", "review_status"),
            "chapter", index, errors,
        )
        reject_placeholders(
            row, ("title", "reader_problem"), "chapter", index, errors
        )
        chapter_id = (row.get("chapter_id") or "").strip()
        cq_refs = parse_refs(row.get("cq_ids"))
        source_refs = parse_refs(row.get("source_ids"))
        figure_refs = parse_refs(row.get("figure_ids"))
        chapter_cq_refs[chapter_id] = cq_refs
        chapter_source_refs[chapter_id] = source_refs
        chapter_figure_refs[chapter_id] = figure_refs
        check_refs(cq_refs, set(cq_by_id), f"chapter {chapter_id} cq_ids", errors)
        check_refs(source_refs, set(source_by_id), f"chapter {chapter_id} source_ids", errors)
        covered_cqs.update(cq_refs)
        if normalized(row.get("review_status")) not in APPROVED:
            errors.append(f"chapter row {index} is not approved")
    for cq_id in cq_by_id:
        if cq_id not in covered_cqs:
            errors.append(f"CQ {cq_id} is not covered by a registered chapter")

    proposition_path = root / "propositions" / "proposition-register.csv"
    proposition_rows: list[dict[str, str]] = []
    if proposition_path.is_file():
        _, proposition_rows = csv_rows(proposition_path)
    if not proposition_rows:
        errors.append("release has no registered proposition")
    proposition_by_id = index_rows(
        proposition_rows, "proposition_id", "proposition", errors
    )
    proposition_chapters: set[str] = set()
    proposition_cqs: set[str] = set()
    for index, row in enumerate(proposition_rows, 2):
        require_fields(
            row,
            (
                "proposition_id", "chapter_id", "cq_ids", "source_ids", "statement_summary",
                "claim_class", "authority_limit", "evidence_oracle", "review_status",
            ),
            "proposition", index, errors,
        )
        reject_placeholders(
            row,
            ("statement_summary", "authority_limit", "evidence_oracle"),
            "proposition", index, errors,
        )
        proposition_id = (row.get("proposition_id") or "").strip()
        chapter_id = (row.get("chapter_id") or "").strip()
        cq_refs = parse_refs(row.get("cq_ids"))
        source_refs = parse_refs(row.get("source_ids"))
        if chapter_id not in chapter_by_id:
            errors.append(f"proposition {proposition_id} references unknown chapter {chapter_id}")
        else:
            proposition_chapters.add(chapter_id)
        check_refs(cq_refs, set(cq_by_id), f"proposition {proposition_id} cq_ids", errors)
        check_refs(
            source_refs, set(source_by_id), f"proposition {proposition_id} source_ids", errors
        )
        for cq_id in cq_refs:
            if cq_id not in chapter_cq_refs.get(chapter_id, []):
                errors.append(
                    f"proposition {proposition_id} uses CQ {cq_id} outside chapter {chapter_id}"
                )
        for source_id in source_refs:
            if source_id not in chapter_source_refs.get(chapter_id, []):
                errors.append(
                    f"proposition {proposition_id} uses source {source_id} outside chapter {chapter_id}"
                )
        proposition_cqs.update(cq_refs)
        if normalized(row.get("claim_class")) not in CLAIM_CLASSES:
            errors.append(f"proposition row {index} has an unsupported claim_class")
        if normalized(row.get("review_status")) not in APPROVED:
            errors.append(f"proposition row {index} is not approved")
    for chapter_id in chapter_by_id:
        if chapter_id not in proposition_chapters:
            errors.append(f"chapter {chapter_id} has no reviewed proposition")
    for cq_id in cq_by_id:
        if cq_id not in proposition_cqs:
            errors.append(f"CQ {cq_id} has no reviewed proposition")

    figure_path = root / "figures" / "figure-register.csv"
    figure_rows: list[dict[str, str]] = []
    if figure_path.is_file():
        _, figure_rows = csv_rows(figure_path)
    if not figure_rows:
        errors.append("release has no registered teaching figure")
    figure_by_id = index_rows(figure_rows, "figure_id", "figure", errors)
    for index, row in enumerate(figure_rows, 2):
        require_fields(
            row,
            (
                "figure_id", "chapter_id", "source_ids", "visual_question",
                "semantic_baseline", "input_rights", "input_rights_status", "generator",
                "sha256", "caption", "alt_text", "release_status",
            ),
            "figure", index, errors,
        )
        reject_placeholders(
            row,
            (
                "visual_question", "semantic_baseline", "input_rights", "generator",
                "caption", "alt_text",
            ),
            "figure", index, errors,
        )
        figure_id = (row.get("figure_id") or "").strip()
        chapter_id = (row.get("chapter_id") or "").strip()
        if chapter_id not in chapter_by_id:
            errors.append(f"figure {figure_id} references unknown chapter {chapter_id}")
        figure_source_refs = parse_refs(row.get("source_ids"))
        check_refs(
            figure_source_refs, set(source_by_id), f"figure {figure_id} source_ids", errors
        )
        for source_id in figure_source_refs:
            if source_id not in chapter_source_refs.get(chapter_id, []):
                errors.append(
                    f"figure {figure_id} uses source {source_id} outside chapter {chapter_id}"
                )
        if figure_id not in chapter_figure_refs.get(chapter_id, []):
            errors.append(f"figure {figure_id} is not linked from chapter {chapter_id}")
        if has_unresolved_marker(row.get("input_rights")):
            errors.append(f"figure row {index} input rights are unresolved")
        if normalized(row.get("input_rights_status")) not in RIGHTS_STATUSES:
            errors.append(f"figure row {index} input rights status is not cleared")
        if not SHA256_RE.fullmatch(normalized(row.get("sha256"))):
            errors.append(f"figure row {index} has an invalid sha256")
        if normalized(row.get("release_status")) not in APPROVED:
            errors.append(f"figure row {index} is not approved")
    for chapter_id, figure_refs in chapter_figure_refs.items():
        check_refs(
            figure_refs, set(figure_by_id), f"chapter {chapter_id} figure_ids", errors,
            required=False,
        )
        for figure_id in figure_refs:
            figure = figure_by_id.get(figure_id)
            if figure and (figure.get("chapter_id") or "").strip() != chapter_id:
                errors.append(f"chapter {chapter_id} links figure {figure_id} owned by another chapter")

    assets_path = root / "release" / "public-assets.csv"
    asset_rows: list[dict[str, str]] = []
    if assets_path.is_file():
        _, asset_rows = csv_rows(assets_path)
    if not asset_rows:
        errors.append("release has no registered public asset")
    index_rows(asset_rows, "asset_id", "public asset", errors)
    manifest_paths: set[str] = set()
    assets_by_path: dict[str, dict[str, str]] = {}
    reader_assets = 0
    reader_chapter_coverage: set[str] = set()
    released_figure_counts: dict[str, int] = {}
    skill_assets = 0
    skill_chapter_coverage: set[str] = set()
    test_report_paths: list[Path] = []
    test_report_chapter_refs: dict[Path, list[str]] = {}
    for index, row in enumerate(asset_rows, 2):
        require_fields(
            row,
            (
                "asset_id", "relative_path", "asset_role", "chapter_ids", "sha256",
                "creator_or_method", "rights_basis", "rights_status",
                "contains_personal_data", "technical_review", "privacy_review", "release_status",
            ),
            "public asset", index, errors,
        )
        reject_placeholders(
            row, ("creator_or_method", "rights_basis"), "public asset", index, errors
        )
        raw_path = (row.get("relative_path") or "").strip()
        if raw_path in manifest_paths:
            errors.append(f"public asset row {index} duplicates relative_path {raw_path}")
        if raw_path:
            manifest_paths.add(raw_path)
            assets_by_path.setdefault(raw_path, row)
        if raw_path in control_files:
            errors.append(f"public asset row {index} attempts to publish a package control file")

        role = normalized(row.get("asset_role"))
        if role not in ASSET_ROLES:
            errors.append(f"public asset row {index} has an unsupported asset_role")
        chapter_refs = parse_refs(row.get("chapter_ids"))
        check_refs(chapter_refs, set(chapter_by_id), f"public asset row {index} chapter_ids", errors)
        if role == "reader-book":
            reader_assets += 1
            reader_chapter_coverage.update(chapter_refs)
        elif role == "skill":
            skill_assets += 1
            skill_chapter_coverage.update(chapter_refs)

        figure_id = (row.get("figure_id") or "").strip()
        if role == "figure":
            if figure_id not in figure_by_id:
                errors.append(f"public figure asset row {index} lacks a valid figure_id")
            else:
                released_figure_counts[figure_id] = released_figure_counts.get(figure_id, 0) + 1
                figure = figure_by_id[figure_id]
                figure_chapter = (figure.get("chapter_id") or "").strip()
                if chapter_refs != [figure_chapter]:
                    errors.append(
                        f"public figure asset row {index} chapter_ids must exactly match its registered chapter"
                    )
                if normalized(figure.get("sha256")) != normalized(row.get("sha256")):
                    errors.append(f"public figure asset row {index} hash differs from figure register")
        elif figure_id:
            errors.append(f"non-figure public asset row {index} must not set figure_id")

        if normalized(row.get("contains_personal_data")) not in {"no", "false"}:
            errors.append(f"public asset row {index} contains or has unknown personal data")
        if normalized(row.get("release_status")) not in APPROVED:
            errors.append(f"public asset row {index} is not approved")
        if has_unresolved_marker(row.get("rights_basis")):
            errors.append(f"public asset row {index} rights basis is unresolved")
        if normalized(row.get("rights_status")) not in RIGHTS_STATUSES:
            errors.append(f"public asset row {index} rights status is not cleared")
        if normalized(row.get("technical_review")) not in APPROVED:
            errors.append(f"public asset row {index} technical review is not approved")
        if normalized(row.get("privacy_review")) not in APPROVED:
            errors.append(f"public asset row {index} privacy review is not approved")

        expected_hash = normalized(row.get("sha256"))
        if not SHA256_RE.fullmatch(expected_hash):
            errors.append(f"public asset row {index} has an invalid sha256")
        candidate, path_error = safe_package_asset(root, raw_path)
        if path_error:
            errors.append(f"public asset row {index}: {path_error}")
        elif candidate is None or not candidate.is_file():
            errors.append(f"public asset row {index} path does not exist")
        else:
            suffix = candidate.suffix.lower()
            if suffix not in ALLOWED_ASSET_SUFFIXES:
                errors.append(f"public asset row {index} has an unsupported file type")
            elif role in ROLE_SUFFIXES and suffix not in ROLE_SUFFIXES[role]:
                errors.append(f"public asset row {index} file type is incompatible with {role}")
            if SHA256_RE.fullmatch(expected_hash):
                actual_hash = safe_sha256(candidate)
                if actual_hash is None:
                    errors.append(f"public asset row {index} path is unreadable")
                elif actual_hash != expected_hash:
                    errors.append(f"public asset row {index} sha256 mismatch")
            if role == "skill":
                if raw_path != "skill/SKILL.md":
                    errors.append(
                        f"public skill asset row {index} must point to the canonical skill/SKILL.md"
                    )
                try:
                    skill_text = candidate.read_text(encoding="utf-8")
                except (OSError, UnicodeError):
                    errors.append(f"public skill asset row {index} is not readable UTF-8")
                    continue
                frontmatter = simple_frontmatter(skill_text)
                if INLINE_PLACEHOLDER_RE.search(skill_text) or frontmatter is None:
                    errors.append(f"public skill asset row {index} is unfinished or lacks frontmatter")
                else:
                    skill_name = text_value(frontmatter.get("name"))
                    skill_description = text_value(frontmatter.get("description"))
                    if (
                        not skill_name
                        or skill_name != text_value(metadata.get("slug"))
                        or not skill_description
                    ):
                        errors.append(
                            f"public skill asset row {index} has missing or mismatched identity"
                        )
                    skill_sections = markdown_sections(skill_text)
                    authority = skill_sections.get("Authority boundary") or skill_sections.get("权限边界", "")
                    workflow = skill_sections.get("Workflow") or skill_sections.get("工作流程", "")
                    if not has_substantive_markdown(authority):
                        errors.append(f"public skill asset row {index} lacks an authority boundary")
                    if not has_substantive_markdown(workflow):
                        errors.append(f"public skill asset row {index} lacks a workflow")
            elif role == "test-report":
                test_report_paths.append(candidate)
                test_report_chapter_refs[candidate] = chapter_refs
    if reader_assets < 1:
        errors.append("release has no reader-book asset")
    for chapter_id in chapter_by_id:
        if chapter_id not in reader_chapter_coverage:
            errors.append(f"chapter {chapter_id} is not covered by a reader-book asset")
        if chapter_id not in skill_chapter_coverage:
            errors.append(f"chapter {chapter_id} is not covered by a released Skill asset")
    if skill_assets != 1:
        errors.append(f"release must have exactly one canonical Skill asset; found {skill_assets}")
    if len(test_report_paths) != 1:
        errors.append(
            f"release must have exactly one machine test-report asset; found {len(test_report_paths)}"
        )
    for figure_id in figure_by_id:
        count = released_figure_counts.get(figure_id, 0)
        if count != 1:
            errors.append(f"figure {figure_id} must have exactly one released public figure asset; found {count}")

    ontology_path = root / "ontology" / "package-manifest.yaml"
    ontology: dict[str, str] = {}
    if ontology_path.is_file():
        ontology = simple_yaml(ontology_path)
        append_yaml_errors(ontology, "ontology/package-manifest.yaml", errors)
        if ontology.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
            errors.append("ontology schema_version is unsupported")
        if normalized(ontology.get("status")) not in APPROVED:
            errors.append("ontology package status is not approved")
        if has_unresolved_marker(ontology.get("namespace")):
            errors.append("ontology namespace is unresolved")
        elif not re.match(r"^(?:https?://|urn:)[^\s]+$", text_value(ontology.get("namespace"))):
            errors.append("ontology namespace is not an absolute HTTP(S) or URN identifier")
        if (
            not text_value(ontology.get("book_slug"))
            or ontology.get("book_slug") != metadata.get("slug")
        ):
            errors.append("ontology book_slug does not match book.yaml")
        if ontology.get("competency_question_register") != "cqs/cq-register.csv":
            errors.append("ontology competency_question_register must use the package-root path")

        binding_paths: dict[str, str] = {}
        for key, allowed_roles in ONTOLOGY_BINDINGS.items():
            raw_path = text_value(ontology.get(key))
            candidate, path_error = safe_package_asset(root, raw_path)
            if path_error:
                errors.append(f"ontology {key}: {path_error}")
            elif candidate is None or not candidate.is_file():
                errors.append(f"ontology {key} path does not exist")
            if raw_path in binding_paths:
                errors.append(f"ontology {key} reuses the {binding_paths[raw_path]} artifact")
            elif raw_path:
                binding_paths[raw_path] = key
            asset = assets_by_path.get(raw_path)
            if asset is None:
                errors.append(f"ontology {key} is not registered in public-assets.csv")
            elif normalized(asset.get("asset_role")) not in allowed_roles:
                expected = ", ".join(sorted(allowed_roles))
                errors.append(f"ontology {key} requires asset role: {expected}")

    for report_path in test_report_paths:
        relative = report_path.relative_to(root).as_posix()
        try:
            if report_path.stat().st_size > MAX_TEST_REPORT_BYTES:
                errors.append(f"test report exceeds {MAX_TEST_REPORT_BYTES} bytes: {relative}")
                continue
        except OSError:
            errors.append(f"test report is unreadable: {relative}")
            continue
        if set(test_report_chapter_refs.get(report_path, [])) != set(chapter_by_id):
            errors.append(f"test report asset {relative} does not cover the exact chapter set")
        try:
            report = json.loads(
                report_path.read_text(encoding="utf-8"),
                object_pairs_hook=reject_duplicate_json_pairs,
                parse_constant=reject_nonstandard_json_constant,
            )
        except (
            OSError,
            UnicodeDecodeError,
            json.JSONDecodeError,
            ValueError,
            RecursionError,
            MemoryError,
        ):
            errors.append(f"test report is not valid UTF-8 JSON: {relative}")
            continue
        if not isinstance(report, dict):
            errors.append(f"test report must be a JSON object: {relative}")
            continue
        required_report_fields = (
            "schema_version",
            "book_slug",
            "status",
            "command",
            "tool",
            "executed_at",
            "runner",
            "runner_sha256",
            "ontology_manifest_sha256",
        )
        for field in required_report_fields:
            if not text_value(report.get(field)):
                errors.append(f"test report {relative} lacks {field}")
        if report.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
            errors.append(f"test report {relative} schema_version is unsupported")
        if (
            not text_value(report.get("book_slug"))
            or report.get("book_slug") != metadata.get("slug")
        ):
            errors.append(f"test report {relative} book_slug does not match book.yaml")
        if normalized(str(report.get("status", ""))) not in {"passed"}:
            errors.append(f"test report {relative} status is not passed")
        if has_unresolved_marker(str(report.get("command", ""))):
            errors.append(f"test report {relative} command is unresolved")
        if has_unresolved_marker(str(report.get("tool", ""))):
            errors.append(f"test report {relative} tool is unresolved")
        executed_at = text_value(report.get("executed_at"))
        timestamp_shape_is_valid = bool(
            re.fullmatch(
                r"\d{4}-\d{2}-\d{2}T(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d"
                r"(?:\.\d{1,6})?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)",
                executed_at,
            )
        )
        try:
            parsed_executed_at = datetime.fromisoformat(executed_at.replace("Z", "+00:00"))
            timestamp_is_valid = (
                timestamp_shape_is_valid
                and
                parsed_executed_at.tzinfo is not None
                and parsed_executed_at.utcoffset() is not None
            )
        except ValueError:
            timestamp_is_valid = False
        if not timestamp_is_valid:
            errors.append(f"test report {relative} executed_at is not an ISO-8601 timestamp")
        covered_report_cqs = report.get("covered_cq_ids")
        if not isinstance(covered_report_cqs, list) or any(
            not isinstance(item, str) for item in covered_report_cqs
        ):
            errors.append(f"test report {relative} covered_cq_ids must be a string list")
        elif len(covered_report_cqs) != len(set(covered_report_cqs)) or set(covered_report_cqs) != set(cq_by_id):
            errors.append(f"test report {relative} does not cover the exact CQ set")
        covered_report_propositions = report.get("covered_proposition_ids")
        if not isinstance(covered_report_propositions, list) or any(
            not isinstance(item, str) for item in covered_report_propositions
        ):
            errors.append(
                f"test report {relative} covered_proposition_ids must be a string list"
            )
        elif (
            len(covered_report_propositions) != len(set(covered_report_propositions))
            or set(covered_report_propositions) != set(proposition_by_id)
        ):
            errors.append(f"test report {relative} does not cover the exact proposition set")
        checks = report.get("checks")
        if not isinstance(checks, list) or not checks or any(
            not isinstance(check, dict)
            or not str(check.get("check_id", "")).strip()
            or normalized(str(check.get("status", ""))) != "passed"
            for check in checks
        ):
            errors.append(f"test report {relative} has no complete passed check list")
        runner = text_value(ontology.get("runner"))
        if report.get("runner") != runner:
            errors.append(f"test report {relative} runner does not match ontology manifest")
        runner_path, runner_error = safe_package_asset(root, runner)
        runner_hash = (
            safe_sha256(runner_path)
            if runner_error is None and runner_path is not None and runner_path.is_file()
            else None
        )
        if (
            runner_error
            or runner_path is None
            or not runner_path.is_file()
            or runner_hash is None
            or report.get("runner_sha256") != runner_hash
        ):
            errors.append(f"test report {relative} runner_sha256 does not match")
        ontology_hash = safe_sha256(ontology_path) if ontology_path.is_file() else None
        if ontology_path.is_file() and (
            ontology_hash is None or report.get("ontology_manifest_sha256") != ontology_hash
        ):
            errors.append(f"test report {relative} ontology_manifest_sha256 does not match")

    privacy_manifest = root / "privacy" / "public-export.yaml"
    if privacy_manifest.is_file():
        privacy = simple_yaml(privacy_manifest)
        append_yaml_errors(privacy, "privacy/public-export.yaml", errors)
        if privacy.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
            errors.append("privacy schema_version is unsupported")
        expected_privacy = {
            "policy": "default-deny",
            "controlled_evidence_root": "external-required",
            "evidence_linkage": "logical-id-and-sha256-only",
            "public_manifest": "release/public-assets.csv",
        }
        for key, expected in expected_privacy.items():
            if privacy.get(key) != expected:
                errors.append(f"privacy {key} must be {expected}")
        for key, required_values in (
            ("forbidden_package_paths", REQUIRED_PRIVACY_PATHS),
            ("forbidden_public_content", REQUIRED_PRIVACY_CONTENT_RULES),
        ):
            values = privacy.get(key)
            if not isinstance(values, list) or any(
                not isinstance(value, str) or not value.strip() for value in values
            ):
                errors.append(f"privacy {key} must be a non-empty string list")
                continue
            if len(values) != len(set(values)):
                errors.append(f"privacy {key} contains duplicate entries")
            for missing in sorted(required_values - set(values)):
                errors.append(f"privacy {key} is missing required entry: {missing}")
        if normalized(privacy.get("human_privacy_review")) not in APPROVED:
            errors.append("human privacy review is not approved")

    actual_public_files: set[str] = set()
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        rel_parts = relative.parts
        if ".git" in rel_parts:
            continue
        if has_path_control_characters(relative.as_posix()):
            errors.append("path with control characters is not allowed in release package")
            continue
        if path.is_symlink():
            errors.append(f"symlink is not allowed in release package: {path.relative_to(root).as_posix()}")
            continue
        if path.is_file():
            actual_public_files.add(path.relative_to(root).as_posix())
        elif not path.is_dir():
            errors.append(
                f"special filesystem node is not allowed in release package: "
                f"{path.relative_to(root).as_posix()}"
            )
    unmanifested = sorted(actual_public_files - control_files - manifest_paths)
    for relative in unmanifested:
        errors.append(f"unmanifested public file: {relative}")
    missing_manifested = sorted(manifest_paths - actual_public_files)
    for relative in missing_manifested:
        errors.append(f"manifested public file is missing: {relative}")

    lock_relative = "release/package-lock.csv"
    lock_path = root / lock_relative
    lock_rows: list[dict[str, str]] = []
    if lock_path.is_file():
        _, lock_rows = csv_rows(lock_path)
    locked_paths: set[str] = set()
    for index, row in enumerate(lock_rows, 2):
        raw_path = (row.get("relative_path") or "").strip()
        expected_hash = normalized(row.get("sha256"))
        if raw_path == lock_relative:
            errors.append(f"package lock row {index} must not lock itself")
        if raw_path in locked_paths:
            errors.append(f"package lock row {index} duplicates relative_path {raw_path}")
        if raw_path:
            locked_paths.add(raw_path)
        if not SHA256_RE.fullmatch(expected_hash):
            errors.append(f"package lock row {index} has an invalid sha256")
        candidate, path_error = safe_package_asset(root, raw_path)
        if path_error:
            errors.append(f"package lock row {index}: {path_error}")
        elif candidate is None or not candidate.is_file() or (root / raw_path).is_symlink():
            errors.append(f"package lock row {index} path does not exist as a regular file")
        elif SHA256_RE.fullmatch(expected_hash):
            actual_hash = safe_sha256(candidate)
            if actual_hash is None:
                errors.append(f"package lock row {index} path is unreadable")
            elif actual_hash != expected_hash:
                errors.append(f"package lock row {index} sha256 mismatch")
    expected_locked_paths = actual_public_files - {lock_relative}
    for relative in sorted(expected_locked_paths - locked_paths):
        errors.append(f"package lock is missing file: {relative}")
    for relative in sorted(locked_paths - expected_locked_paths):
        errors.append(f"package lock contains unknown file: {relative}")
    errors.extend(privacy_findings(root))
    return errors


def run(root: Path, stage: str) -> list[str]:
    resolved = root.expanduser().resolve()
    if not resolved.is_dir():
        return [f"package is not a directory: {resolved}"]
    if stage == "structure":
        return validate_structure(resolved)
    if stage == "charter":
        return validate_charter(resolved)
    return validate_release(resolved)


def main() -> int:
    args = parse_args()
    if args.write_lock:
        if args.stage != "release":
            print("error: --write-lock requires --stage release", file=sys.stderr)
            return 2
        try:
            lock_path = write_package_lock(args.package.expanduser().resolve())
        except ValueError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        if not args.json:
            print(f"wrote package lock: {lock_path}")
    errors = run(args.package, args.stage)
    if args.json:
        print(json.dumps({"ok": not errors, "stage": args.stage, "errors": errors}, ensure_ascii=False, indent=2))
    elif errors:
        print(f"{args.stage} validation failed: {len(errors)} issue(s)")
        for error in errors:
            print(f"- {error}")
    else:
        print(f"{args.stage} validation passed")
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
