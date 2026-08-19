#!/usr/bin/env python3
"""Validate a book corpus and its Semantica-only execution binding.

This validator checks book-package structure and cross-file evidence.  It does
not implement or invoke an RDF backend.  Any live Semantica operation must be
routed through ``ontology_engineering.semantica_runtime`` by the surrounding
workflow.
"""

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
    "chapters/chapter-register.csv",
    "propositions/proposition-register.csv",
    "semantica/package-proposal.yaml",
    "semantica/package-binding.yaml",
    "figures/figure-register.csv",
    "release/public-assets.csv",
    "release/package-lock.csv",
    "privacy/public-export.yaml",
    "skill/SKILL.md",
)
MANIFESTABLE_REQUIRED_FILES = {"skill/SKILL.md"}
SUPPORTED_SCHEMA_VERSION = "1.0"
MAX_SEMANTICA_EVIDENCE_BYTES = 4_000_000
BOOK_SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
SEMANTICA_PACKAGE_ID_RE = re.compile(
    r"^semantica\.[a-z0-9]+(?:[._][a-z0-9]+)*$"
)
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
    "book-local ontology, CQ, shape, query, case, rule, fixture or runner payloads",
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
    "figure",
    "metadata",
    "reader-book",
    "release-verdict",
    "runtime-receipt",
    "source-lock",
    "skill",
    "style",
}
ROLE_SUFFIXES = {
    "figure": {".jpeg", ".jpg", ".pdf", ".png", ".svg", ".webp"},
    "metadata": {".csv", ".json", ".md", ".txt", ".yaml", ".yml"},
    "reader-book": {".epub", ".html", ".md", ".pdf"},
    "release-verdict": {".json"},
    "runtime-receipt": {".json"},
    "source-lock": {".json"},
    "skill": {".md"},
    "style": {".sty", ".tex"},
}
CLAIM_CLASSES = {
    "author-explanation",
    "best-practice",
    "standard-grounded",
    "teaching-assumption",
}
ALLOWED_ASSET_SUFFIXES = {
    ".csv", ".epub", ".html", ".jpeg", ".jpg", ".json", ".md", ".pdf",
    ".png", ".sty", ".svg", ".tex", ".txt", ".webp", ".yaml", ".yml",
}
FORBIDDEN_EXECUTABLE_SEMANTIC_SUFFIXES = {
    ".owl", ".rdf", ".rq", ".shacl", ".sparql", ".ttl",
}
FORBIDDEN_PARALLEL_SEMANTIC_ROOTS = {
    "case", "cases", "cq", "cqs", "fixture", "fixtures", "ontologies", "ontology",
    "queries", "query", "rule", "rules", "shape", "shapes",
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
    "chapters/chapter-register.csv": [
        "chapter_id",
        "title",
        "reader_problem",
        "semantica_cq_ids",
        "source_ids",
        "figure_ids",
        "review_status",
    ],
    "propositions/proposition-register.csv": [
        "proposition_id",
        "chapter_id",
        "semantica_cq_ids",
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


def expected_semantica_package_id(slug: str) -> str:
    return "semantica.books." + slug.replace("-", "_")


def validate_semantica_metadata(
    root: Path,
    book_metadata: dict[str, object],
    *,
    release: bool,
) -> tuple[list[str], dict[str, object], dict[str, object]]:
    """Validate the proposal/binding without importing Semantica.

    These documents are references to the sole executable implementation, not
    a second package manifest.  Release evidence is validated separately.
    """

    errors: list[str] = []
    proposal_path = root / "semantica" / "package-proposal.yaml"
    binding_path = root / "semantica" / "package-binding.yaml"
    proposal = simple_yaml(proposal_path) if proposal_path.is_file() else {}
    binding = simple_yaml(binding_path) if binding_path.is_file() else {}
    append_yaml_errors(proposal, "semantica/package-proposal.yaml", errors)
    append_yaml_errors(binding, "semantica/package-binding.yaml", errors)

    slug = text_value(book_metadata.get("slug"))
    expected_package_id = expected_semantica_package_id(slug) if slug else ""
    required_semantics = {
        "ontology",
        "competency-questions",
        "shapes",
        "queries",
        "cases",
        "engineering-rules",
    }
    if proposal:
        if proposal.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
            errors.append("Semantica package proposal schema_version is unsupported")
        if proposal.get("book_slug") != slug:
            errors.append("Semantica package proposal book_slug does not match book.yaml")
        proposed_id = text_value(proposal.get("proposed_package_id"))
        if not SEMANTICA_PACKAGE_ID_RE.fullmatch(proposed_id):
            errors.append("Semantica proposed_package_id is missing or invalid")
        elif proposed_id != expected_package_id:
            errors.append("Semantica proposed_package_id is not the stable book-derived ID")
        expected_proposal_values = {
            "external_specification_kind": "book",
            "external_source_register": "sources/source-register.csv",
            "chapter_register": "chapters/chapter-register.csv",
            "proposition_register": "propositions/proposition-register.csv",
            "execution_owner": "Semantica",
        }
        for key, expected in expected_proposal_values.items():
            if proposal.get(key) != expected:
                errors.append(f"Semantica package proposal {key} must be {expected}")
        requested = proposal.get("requested_semantics")
        if (
            not isinstance(requested, list)
            or any(not isinstance(item, str) or not item for item in requested)
            or len(requested) != len(set(requested))
            or set(requested) != required_semantics
        ):
            errors.append("Semantica package proposal must request the complete semantic payload")
        allowed_statuses = {"accepted"} if release else {"draft", "accepted"}
        if normalized(proposal.get("proposal_status")) not in allowed_statuses:
            errors.append(
                "Semantica package proposal is not accepted"
                if release
                else "Semantica package proposal status is invalid"
            )

    if binding:
        if binding.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
            errors.append("Semantica package binding schema_version is unsupported")
        if binding.get("book_slug") != slug:
            errors.append("Semantica package binding book_slug does not match book.yaml")
        package_id = text_value(binding.get("semantica_package_id"))
        if not SEMANTICA_PACKAGE_ID_RE.fullmatch(package_id):
            errors.append("Semantica package binding package_id is missing or invalid")
        elif package_id != expected_package_id:
            errors.append("Semantica package binding package_id differs from the accepted proposal")
        expected_binding_values = {
            "execution_authority": "semantica-only",
            "runtime_gateway": "ontology_engineering.semantica_runtime",
            "source_lock": "release/semantica-source-lock.json",
            "runtime_receipt": "release/semantica-runtime-receipt.json",
            "release_verdict": "release/semantica-release-verdict.json",
        }
        for key, expected in expected_binding_values.items():
            if binding.get(key) != expected:
                errors.append(f"Semantica package binding {key} must be {expected}")
        allowed_statuses = {"bound"} if release else {"proposed", "bound"}
        if normalized(binding.get("binding_status")) not in allowed_statuses:
            errors.append(
                "Semantica package binding is not bound"
                if release
                else "Semantica package binding status is invalid"
            )
        version = text_value(binding.get("semantica_package_version"))
        if release and (
            not version
            or version == "unbound"
            or has_unresolved_marker(version)
            or not re.fullmatch(r"[0-9]+(?:\.[0-9]+){1,3}(?:[-+][A-Za-z0-9._-]+)?", version)
        ):
            errors.append("Semantica package binding has no stable package version")
        cq_ids = binding.get("bound_cq_ids")
        if not isinstance(cq_ids, list) or any(
            not isinstance(item, str) or not ID_RE.fullmatch(item) for item in cq_ids
        ):
            errors.append("Semantica package binding bound_cq_ids must be a string ID list")
        elif len(cq_ids) != len(set(cq_ids)):
            errors.append("Semantica package binding bound_cq_ids contains duplicates")
        elif release and not cq_ids:
            errors.append("Semantica package binding has no bound competency questions")
    return errors, proposal, binding


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


def valid_iso_timestamp(value: object) -> bool:
    timestamp = text_value(value)
    if not re.fullmatch(
        r"\d{4}-\d{2}-\d{2}T(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d"
        r"(?:\.\d{1,6})?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)",
        timestamp,
    ):
        return False
    try:
        parsed = datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
    except ValueError:
        return False
    return parsed.tzinfo is not None and parsed.utcoffset() is not None


def canonical_json_sha256(value: object) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def load_json_evidence(path: Path, label: str, errors: list[str]) -> dict[str, object] | None:
    try:
        if path.stat().st_size > MAX_SEMANTICA_EVIDENCE_BYTES:
            errors.append(
                f"{label} exceeds {MAX_SEMANTICA_EVIDENCE_BYTES} bytes: "
                f"{path.name}"
            )
            return None
        value = json.loads(
            path.read_text(encoding="utf-8"),
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
        errors.append(f"{label} is not valid UTF-8 JSON: {path.name}")
        return None
    if not isinstance(value, dict):
        errors.append(f"{label} must be a JSON object: {path.name}")
        return None
    return value


def validate_semantica_release_evidence(
    root: Path,
    metadata: dict[str, object],
    binding: dict[str, object],
    source_rows: list[dict[str, str]],
    bound_cq_ids: set[str],
    evidence_paths: dict[str, Path],
) -> list[str]:
    """Cross-check source lock, native receipt and native release verdict.

    This is a strict serialization/integrity check, not a replacement for
    Semantica's release verifier.  Evidence must already have been produced
    through ``ontology_engineering.semantica_runtime``.
    """

    errors: list[str] = []
    lock_path = evidence_paths.get("source-lock")
    receipt_path = evidence_paths.get("runtime-receipt")
    verdict_path = evidence_paths.get("release-verdict")
    if lock_path is None or receipt_path is None or verdict_path is None:
        return errors

    source_lock = load_json_evidence(lock_path, "Semantica source lock", errors)
    receipt = load_json_evidence(receipt_path, "Semantica runtime receipt", errors)
    verdict = load_json_evidence(verdict_path, "Semantica release verdict", errors)
    if source_lock is None or receipt is None or verdict is None:
        return errors

    package_id = text_value(binding.get("semantica_package_id"))
    package_version = text_value(binding.get("semantica_package_version"))
    if source_lock.get("$schema") != "ontology-engineering.book-semantica-source-lock/v1":
        errors.append("Semantica source lock schema is unsupported")
    expected_lock_values = {
        "book_slug": text_value(metadata.get("slug")),
        "package_id": package_id,
        "package_version": package_version,
    }
    for key, expected in expected_lock_values.items():
        if source_lock.get(key) != expected:
            errors.append(f"Semantica source lock {key} differs from package binding")
    for key in (
        "package_digest",
        "runtime_artifact_sha256",
        "source_register_sha256",
        "chapter_register_sha256",
        "proposition_register_sha256",
    ):
        if not SHA256_RE.fullmatch(normalized(source_lock.get(key))):
            errors.append(f"Semantica source lock {key} is not a SHA-256 digest")
    runtime_commit = text_value(source_lock.get("runtime_commit"))
    if not re.fullmatch(r"[0-9a-f]{40,64}", runtime_commit):
        errors.append("Semantica source lock runtime_commit is not a source revision")
    if has_unresolved_marker(source_lock.get("runtime_version")):
        errors.append("Semantica source lock runtime_version is unresolved")
    if not valid_iso_timestamp(source_lock.get("created_at")):
        errors.append("Semantica source lock created_at is not an ISO-8601 timestamp")

    for key, relative in (
        ("source_register_sha256", "sources/source-register.csv"),
        ("chapter_register_sha256", "chapters/chapter-register.csv"),
        ("proposition_register_sha256", "propositions/proposition-register.csv"),
    ):
        actual = safe_sha256(root / relative)
        if actual is None or source_lock.get(key) != actual:
            errors.append(f"Semantica source lock {key} does not bind {relative}")
    expected_source_hashes = {
        text_value(row.get("source_id")): normalized(row.get("sha256"))
        for row in source_rows
        if text_value(row.get("source_id"))
    }
    locked_source_hashes = source_lock.get("source_hashes")
    if locked_source_hashes != expected_source_hashes:
        errors.append("Semantica source lock source_hashes differ from the source register")

    receipt_fields = (
        "schema_version", "created_at", "runtime_version", "runtime_commit",
        "runtime_artifact_sha256", "package_id", "package_version", "package_digest",
        "asset_hashes", "chapter_contract_sha256", "dataset_sha256",
        "dataset_quad_count", "dataset_revision", "capability_report", "cq_report",
        "shacl_report", "oracle_report", "output_hashes", "provenance_bundle",
        "receipt_sha256",
    )
    for field in receipt_fields:
        if field not in receipt:
            errors.append(f"Semantica runtime receipt lacks {field}")
    if receipt.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
        errors.append("Semantica runtime receipt schema_version is unsupported")
    if not valid_iso_timestamp(receipt.get("created_at")):
        errors.append("Semantica runtime receipt created_at is not an ISO-8601 timestamp")
    receipt_lock_bindings = {
        "runtime_version": "runtime_version",
        "runtime_commit": "runtime_commit",
        "runtime_artifact_sha256": "runtime_artifact_sha256",
        "package_id": "package_id",
        "package_version": "package_version",
        "package_digest": "package_digest",
    }
    for receipt_key, lock_key in receipt_lock_bindings.items():
        if receipt.get(receipt_key) != source_lock.get(lock_key):
            errors.append(
                f"Semantica runtime receipt {receipt_key} differs from source lock"
            )
    for key in (
        "runtime_artifact_sha256", "package_digest", "chapter_contract_sha256",
        "dataset_sha256", "receipt_sha256",
    ):
        if not SHA256_RE.fullmatch(normalized(receipt.get(key))):
            errors.append(f"Semantica runtime receipt {key} is not a SHA-256 digest")
    for key in ("dataset_quad_count", "dataset_revision"):
        value = receipt.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            errors.append(f"Semantica runtime receipt {key} must be a non-negative integer")
    for key in ("asset_hashes", "output_hashes"):
        hashes = receipt.get(key)
        if (
            not isinstance(hashes, dict)
            or (key == "asset_hashes" and not hashes)
            or any(
                not isinstance(name, str)
                or not name
                or not isinstance(digest, str)
                or not SHA256_RE.fullmatch(digest)
                for name, digest in hashes.items()
            )
        ):
            errors.append(f"Semantica runtime receipt {key} is not a digest map")

    expected_report_kinds = {
        "capability_report": "capability",
        "cq_report": "cq",
        "shacl_report": "shacl",
        "oracle_report": "oracle",
    }
    for key, kind in expected_report_kinds.items():
        report = receipt.get(key)
        if not isinstance(report, dict):
            errors.append(f"Semantica runtime receipt {key} is not an execution report")
            continue
        if report.get("kind") != kind or report.get("status") != "passed":
            errors.append(f"Semantica runtime receipt {key} is not a passed {kind} report")
        report_hash = report.get("sha256")
        report_content = {name: value for name, value in report.items() if name != "sha256"}
        if not isinstance(report_hash, str) or report_hash != canonical_json_sha256(report_content):
            errors.append(f"Semantica runtime receipt {key} hash does not verify")
    cq_report = receipt.get("cq_report")
    cq_payload = cq_report.get("payload") if isinstance(cq_report, dict) else None
    receipt_cq_ids = (
        cq_payload.get("competency_question_ids")
        if isinstance(cq_payload, dict)
        else None
    )
    if (
        not isinstance(receipt_cq_ids, list)
        or any(not isinstance(item, str) for item in receipt_cq_ids)
        or len(receipt_cq_ids) != len(set(receipt_cq_ids))
        or set(receipt_cq_ids) != bound_cq_ids
    ):
        errors.append("Semantica runtime receipt does not cover the exact bound CQ set")

    provenance = receipt.get("provenance_bundle")
    if not isinstance(provenance, dict):
        errors.append("Semantica runtime receipt provenance_bundle is missing")
    else:
        bundle_hash = provenance.get("bundle_sha256")
        bundle_content = {
            name: value for name, value in provenance.items() if name != "bundle_sha256"
        }
        if not isinstance(bundle_hash, str) or bundle_hash != canonical_json_sha256(bundle_content):
            errors.append("Semantica runtime receipt provenance bundle hash does not verify")
        if not isinstance(provenance.get("records"), list) or not provenance.get("records"):
            errors.append("Semantica runtime receipt provenance bundle has no records")

    receipt_hash = receipt.get("receipt_sha256")
    receipt_content = {
        name: value for name, value in receipt.items() if name != "receipt_sha256"
    }
    if not isinstance(receipt_hash, str) or receipt_hash != canonical_json_sha256(receipt_content):
        errors.append("Semantica runtime receipt content hash does not verify")

    if verdict.get("schema_version") != SUPPORTED_SCHEMA_VERSION:
        errors.append("Semantica release verdict schema_version is unsupported")
    if verdict.get("status") != "complete":
        errors.append("Semantica release verdict is not complete")
    if verdict.get("receipt_sha256") != receipt_hash:
        errors.append("Semantica release verdict does not bind the runtime receipt")
    if not valid_iso_timestamp(verdict.get("checked_at")):
        errors.append("Semantica release verdict checked_at is not an ISO-8601 timestamp")
    checks = verdict.get("checks")
    if (
        not isinstance(checks, list)
        or not checks
        or any(
            not isinstance(check, dict)
            or not text_value(check.get("check_id"))
            or check.get("passed") is not True
            or not text_value(check.get("message"))
            for check in checks
        )
    ):
        errors.append("Semantica release verdict does not contain only passed checks")
    if verdict.get("reasons") != []:
        errors.append("Semantica complete release verdict must have no reasons")
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
        semantic_path_parts = {
            part.lower() for part in relative.parts[:-1]
        } & FORBIDDEN_PARALLEL_SEMANTIC_ROOTS
        semantic_file_stem = (
            path.stem.lower() in FORBIDDEN_PARALLEL_SEMANTIC_ROOTS
            or path.name.lower() == "cq-register.csv"
        ) if path.is_file() else False
        semantic_directory = (
            path.is_dir() and path.name.lower() in FORBIDDEN_PARALLEL_SEMANTIC_ROOTS
        )
        if semantic_path_parts or semantic_file_stem or semantic_directory:
            errors.append(
                f"parallel semantic root is forbidden; move it into Semantica: {relative.as_posix()}"
            )
        if path.is_file() and path.suffix.lower() in FORBIDDEN_EXECUTABLE_SEMANTIC_SUFFIXES:
            errors.append(
                f"executable semantic artifact is forbidden in book corpus: {relative.as_posix()}"
            )
    metadata: dict[str, object] = {}
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
    semantica_errors, _, _ = validate_semantica_metadata(
        root, metadata, release=False
    )
    errors.extend(semantica_errors)
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
    if charter.is_file():
        question_section = markdown_sections(charter_text).get("初始能力问题", "")
        question_lines = [
            re.sub(r"^\s*[-*+]\s+", "", line).strip()
            for line in question_section.splitlines()
            if re.match(r"^\s*[-*+]\s+", line)
        ]
        question_lines = [
            item for item in question_lines
            if item and not PLACEHOLDER_RE.match(item) and not INLINE_PLACEHOLDER_RE.search(item)
        ]
        if not 10 <= len(question_lines) <= 30:
            errors.append(
                "charter stage requires 10-30 proposed reader questions in "
                f"book-charter.md; found {len(question_lines)}"
            )
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

    semantica_metadata_errors, _, semantica_binding = validate_semantica_metadata(
        root, metadata, release=True
    )
    errors.extend(semantica_metadata_errors)
    bound_cq_values = semantica_binding.get("bound_cq_ids", [])
    bound_cq_ids = {
        item for item in bound_cq_values
        if isinstance(item, str) and ID_RE.fullmatch(item)
    } if isinstance(bound_cq_values, list) else set()
    cq_by_id = {cq_id: {} for cq_id in bound_cq_ids}

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
            (
                "chapter_id", "title", "reader_problem", "semantica_cq_ids",
                "source_ids", "review_status",
            ),
            "chapter", index, errors,
        )
        reject_placeholders(
            row, ("title", "reader_problem"), "chapter", index, errors
        )
        chapter_id = (row.get("chapter_id") or "").strip()
        cq_refs = parse_refs(row.get("semantica_cq_ids"))
        source_refs = parse_refs(row.get("source_ids"))
        figure_refs = parse_refs(row.get("figure_ids"))
        chapter_cq_refs[chapter_id] = cq_refs
        chapter_source_refs[chapter_id] = source_refs
        chapter_figure_refs[chapter_id] = figure_refs
        check_refs(
            cq_refs, set(cq_by_id), f"chapter {chapter_id} semantica_cq_ids", errors
        )
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
                "proposition_id", "chapter_id", "semantica_cq_ids", "source_ids", "statement_summary",
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
        cq_refs = parse_refs(row.get("semantica_cq_ids"))
        source_refs = parse_refs(row.get("source_ids"))
        if chapter_id not in chapter_by_id:
            errors.append(f"proposition {proposition_id} references unknown chapter {chapter_id}")
        else:
            proposition_chapters.add(chapter_id)
        check_refs(
            cq_refs,
            set(cq_by_id),
            f"proposition {proposition_id} semantica_cq_ids",
            errors,
        )
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
    reader_assets = 0
    reader_chapter_coverage: set[str] = set()
    released_figure_counts: dict[str, int] = {}
    skill_assets = 0
    skill_chapter_coverage: set[str] = set()
    evidence_paths: dict[str, Path] = {}
    evidence_counts = {
        "source-lock": 0,
        "runtime-receipt": 0,
        "release-verdict": 0,
    }
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
        elif role in evidence_counts and set(chapter_refs) != set(chapter_by_id):
            errors.append(
                f"public {role} asset row {index} does not cover the exact chapter set"
            )

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
            elif role in evidence_counts:
                evidence_counts[role] += 1
                expected_path = text_value(
                    semantica_binding.get(
                        {
                            "source-lock": "source_lock",
                            "runtime-receipt": "runtime_receipt",
                            "release-verdict": "release_verdict",
                        }[role]
                    )
                )
                if raw_path != expected_path:
                    errors.append(
                        f"public {role} asset row {index} does not match package binding"
                    )
                else:
                    evidence_paths[role] = candidate
    if reader_assets < 1:
        errors.append("release has no reader-book asset")
    for chapter_id in chapter_by_id:
        if chapter_id not in reader_chapter_coverage:
            errors.append(f"chapter {chapter_id} is not covered by a reader-book asset")
        if chapter_id not in skill_chapter_coverage:
            errors.append(f"chapter {chapter_id} is not covered by a released Skill asset")
    if skill_assets != 1:
        errors.append(f"release must have exactly one canonical Skill asset; found {skill_assets}")
    for role, count in evidence_counts.items():
        if count != 1:
            errors.append(
                f"release must have exactly one Semantica {role} asset; found {count}"
            )
    for figure_id in figure_by_id:
        count = released_figure_counts.get(figure_id, 0)
        if count != 1:
            errors.append(f"figure {figure_id} must have exactly one released public figure asset; found {count}")

    errors.extend(
        validate_semantica_release_evidence(
            root,
            metadata,
            semantica_binding,
            source_rows,
            bound_cq_ids,
            evidence_paths,
        )
    )

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
