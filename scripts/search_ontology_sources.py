#!/usr/bin/env python3
"""Search both bundled books and optional, explicitly selected local cases."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path


SKILL_DIR = Path(__file__).resolve().parents[1]
BUNDLED_WORKSPACE = SKILL_DIR / "references"
BOOK_DIRECTORY_NAMES = (
    "ontology-engineering-book",
    "product-trustworthiness-book",
)
VOL1_CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-ontology-foundations",
    "ch03-ontology-methodology",
    "ch04-ontology-languages",
    "ch05-reasoning",
    "ch06-applications",
    "ch07-knowledge-graph",
    "ch08-ontology-llm",
    "ch09-capstone-manufacturing",
)
VOL2_CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-concepts-terminology",
    "ch03-safety-management",
    "ch04-concept-hara",
    "ch05-system-development",
    "ch06-hardware-development",
    "ch07-software-development",
    "ch08-asil-decomposition-dfa",
    "ch09-production-operation",
    "ch10-supporting-processes",
    "ch11-claim-ontology",
    "ch12-identity-ontology",
    "ch13-governance-ontology",
    "ch14-context-hazard-ontology",
    "ch15-requirements-ontology",
    "ch16-measurement-ontology",
    "ch17-change-ontology",
    "ch18-dependency-ontology",
    "ch19-field-ontology",
    "ch20-assurance-ontology",
)
BOOK_LOCK_SPECS = {
    "ontology-engineering-book": (
        ("authoring-sources.sha256", ".", "authoring"),
    ),
    "product-trustworthiness-book": (
        ("handbook/current-source.sha256", ".", "authoring"),
        ("handbook/formal-search-guides.sha256", ".", "guides"),
    ),
}
VOL1_FORMAL_GUIDES = {
    "README.md",
    "resources/README.md",
    *(f"{directory}/README.md" for directory in VOL1_CHAPTER_DIRS),
}
VOL2_FORMAL_GUIDES = {
    "README.md",
    "propositions-index.md",
    "handbook/README.md",
    *(f"{directory}/README.md" for directory in VOL2_CHAPTER_DIRS),
}
VOL1_REQUIRED_FORMAL_SOURCES = {
    *VOL1_FORMAL_GUIDES,
    "handbook/README.md",
    "handbook/main.tex",
    "handbook/preamble.tex",
    "handbook/chapters/appB-glossary.tex",
    *(f"handbook/chapters/ch{index:02d}.tex" for index in range(1, 10)),
}
VOL2_REQUIRED_FORMAL_SOURCES = {
    "front-matter/preface.md",
    "appendices/appendix-a-semiconductor.md",
    "appendices/appendix-b-motorcycle-truck.md",
    "appendices/appendix-c-glossary.md",
    "appendices/appendix-d-method-tables.md",
    "handbook/book-metadata.tex",
    "handbook/main.tex",
    "handbook/preamble.tex",
    *(f"{directory}/chapter.md" for directory in VOL2_CHAPTER_DIRS),
}
LOCK_LINE = re.compile(r"^([0-9a-f]{64})  ([^\0]+)$")
ARCHIVE_WARNING = (
    "PROVENANCE WARNING: --scope archive searches historical or non-authoritative "
    "book-adjacent files. Do not cite these hits as current book sources or "
    "executable Semantica semantics."
)
TEXT_EXTENSIONS = {
    ".java",
    ".md",
    ".owl",
    ".py",
    ".rdf",
    ".rq",
    ".shacl",
    ".sparql",
    ".swrl",
    ".tex",
    ".ttl",
    ".txt",
}
SKIP_PARTS = {
    ".git",
    ".venv",
    "__pycache__",
    "codex_outputs",
    "dist",
    "figures-deck",
    "node_modules",
    "runs",
}
SKIP_SUFFIXES = {
    ".aux",
    ".fls",
    ".log",
    ".out",
    ".pdf",
    ".png",
    ".xdv",
}


@dataclass(frozen=True)
class SearchFile:
    path: Path
    sha256: str | None = None
    lock_path: Path | None = None


@dataclass(frozen=True)
class Hit:
    score: int
    path: Path
    line_no: int
    line: str
    context: list[str]
    source_sha256: str | None = None
    source_lock: Path | None = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Search both bundled ontology-engineering book volumes."
    )
    parser.add_argument("query", nargs="+", help="Search query terms.")
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help=(
            "Optional external workspace containing cauchyx-ai. Bundled book searches "
            "always use both volumes installed beside this script."
        ),
    )
    parser.add_argument(
        "--scope",
        choices=("all", "book", "archive", "pde", "cad"),
        default="book",
        help=(
            "Limit search scope. 'book' and 'all' use only formal, author-locked "
            "book sources; 'archive' explicitly searches excluded historical files. "
            "PDE searches require an external root with cauchyx-ai; CAD searches "
            "use CAD_AGENT_ROOT."
        ),
    )
    parser.add_argument("--limit", type=int, default=12, help="Maximum hits.")
    parser.add_argument(
        "--context",
        type=int,
        default=2,
        help="Context lines before and after each hit.",
    )
    parser.add_argument("--json", action="store_true", help="Emit JSON.")
    return parser.parse_args()


def bundled_book_roots() -> tuple[Path, Path]:
    """Return both canonical book roots, failing if the installed corpus is partial."""

    roots = tuple(BUNDLED_WORKSPACE / name for name in BOOK_DIRECTORY_NAMES)
    missing = [root for root in roots if not root.is_dir()]
    if missing:
        missing_text = ", ".join(path.as_posix() for path in missing)
        raise SystemExit(
            "Bundled book corpus is incomplete; both installed volumes are required. "
            f"Missing: {missing_text}. External roots cannot substitute for bundled books."
        )
    return roots


def _pde_workspace(candidate: Path) -> Path | None:
    """Normalize either a cauchyx-ai checkout or its containing workspace."""

    root = candidate.expanduser().resolve()
    if root.name == "cauchyx-ai" and root.is_dir():
        return root.parent
    if (root / "cauchyx-ai").is_dir():
        return root
    return None


def discover_workspace(explicit: Path | None, scope: str = "book") -> Path:
    """Resolve optional-case roots without allowing them to replace bundled books."""

    if scope not in {"all", "book", "archive", "pde", "cad"}:
        raise ValueError(f"unsupported search scope: {scope}")

    if scope in {"all", "book", "archive"}:
        bundled_book_roots()
    if scope in {"book", "archive", "cad"}:
        return BUNDLED_WORKSPACE

    candidates: list[Path] = []
    if explicit:
        candidates.append(explicit)
    env_root = os.environ.get("ONTOLOGY_ENGINEERING_ROOT")
    if env_root:
        candidates.append(Path(env_root))

    for candidate in candidates:
        workspace = _pde_workspace(candidate)
        if workspace is not None:
            return workspace

    if scope == "all":
        # Optional examples are absent; the canonical two-volume corpus remains valid.
        return BUNDLED_WORKSPACE

    raise SystemExit(
        "Could not find an external workspace containing cauchyx-ai. "
        "Pass --root or set ONTOLOGY_ENGINEERING_ROOT; bundled book roots are not "
        "used as external-case fallbacks."
    )


def optional_case_roots(workspace: Path, scope: str) -> list[Path]:
    """Return only explicitly selected external case roots."""

    roots: list[Path] = []
    if scope in ("all", "pde"):
        pde_workspace = _pde_workspace(workspace)
        pde_root = pde_workspace / "cauchyx-ai" if pde_workspace else None
        if pde_root is not None:
            roots.append(pde_root)
        elif scope == "pde":
            raise SystemExit(
                "PDE scope requires a cauchyx-ai checkout supplied by --root or "
                "ONTOLOGY_ENGINEERING_ROOT."
            )
    if scope in ("all", "cad"):
        cad_root = discover_cad_agent_root()
        if cad_root is not None:
            roots.append(cad_root)
    return [root for root in roots if root.is_dir()]


def discover_cad_agent_root() -> Path | None:
    candidates: list[Path] = []
    env_root = os.environ.get("CAD_AGENT_ROOT")
    if env_root:
        candidates.append(Path(env_root))
    for candidate in candidates:
        root = candidate.expanduser().resolve()
        if (
            (root / "pyproject.toml").is_file()
            and (root / "src" / "cad_agent").is_dir()
        ):
            return root
    return None


def display_path(path: Path, workspace: Path) -> str:
    resolved = path.resolve()
    try:
        return resolved.relative_to(BUNDLED_WORKSPACE.resolve()).as_posix()
    except ValueError:
        pass
    try:
        return resolved.relative_to(workspace.resolve()).as_posix()
    except ValueError:
        pass
    cad_root = discover_cad_agent_root()
    if cad_root is not None:
        try:
            return (Path("cad-agent") / resolved.relative_to(cad_root)).as_posix()
        except ValueError:
            pass
    return resolved.as_posix()


def include_file(path: Path) -> bool:
    if any(part in SKIP_PARTS for part in path.parts):
        return False
    if path.suffix in SKIP_SUFFIXES:
        return False
    if path.suffix not in TEXT_EXTENSIONS:
        return False
    if "/handbook/fragments/" in path.as_posix():
        return False
    return True


def iter_files(roots: list[Path]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        for path in root.rglob("*"):
            if path.is_file() and include_file(path):
                files.append(path)
    return sorted(files)


def _canonical_lock_relative(
    relative: str, *, lock_path: Path, line_number: int
) -> Path:
    relative_path = Path(relative)
    if (
        not relative
        or relative == "."
        or relative != relative.strip()
        or any(char in relative for char in "\\\0\r\n")
        or relative_path.is_absolute()
        or any(part in {".", ".."} for part in relative_path.parts)
        or relative != relative_path.as_posix()
    ):
        raise SystemExit(
            f"Unsafe formal book source path: {lock_path}:{line_number}"
        )
    return relative_path


def _locked_book_files(book_root: Path) -> list[SearchFile]:
    """Resolve all formal locks for one volume as content-bound allowlists."""

    selected: dict[Path, SearchFile] = {}
    book = book_root.resolve()
    if book_root.name == "ontology-engineering-book":
        legacy_lock = book_root / "handbook" / "authoring-sources.sha256"
        if legacy_lock.exists():
            raise SystemExit(
                "Retired Vol.1 handbook-root source lock must not coexist with the "
                f"book-root lock: {legacy_lock.as_posix()}"
            )
    for lock_relative, base_relative, role in BOOK_LOCK_SPECS[book_root.name]:
        lock_path = (book_root / lock_relative).resolve()
        try:
            lock_path.relative_to(book)
        except ValueError as exc:
            raise SystemExit(
                f"Formal book source lock escapes its volume: {lock_path.as_posix()}"
            ) from exc
        if not lock_path.is_file():
            raise SystemExit(
                f"Formal book source lock is missing: {lock_path.as_posix()}. "
                "Use --scope archive only for explicit provenance research."
            )
        base = (book_root / base_relative).resolve()
        try:
            base.relative_to(book)
        except ValueError as exc:
            raise SystemExit(
                f"Formal book source base escapes its volume: {base.as_posix()}"
            ) from exc
        entries: dict[str, str] = {}
        try:
            raw_lines = lock_path.read_text(encoding="utf-8").splitlines()
        except UnicodeDecodeError as exc:
            raise SystemExit(
                f"Formal book source lock is not UTF-8: {lock_path.as_posix()}"
            ) from exc
        for line_number, raw in enumerate(raw_lines, 1):
            if not raw or raw.startswith("#"):
                continue
            match = LOCK_LINE.fullmatch(raw)
            if not match:
                raise SystemExit(
                    f"Malformed formal book source lock: {lock_path}:{line_number}"
                )
            expected_sha256, relative = match.groups()
            relative_path = _canonical_lock_relative(
                relative, lock_path=lock_path, line_number=line_number
            )
            if relative in entries:
                raise SystemExit(
                    f"Duplicate formal book source path: {lock_path}:{line_number}"
                )
            entries[relative] = expected_sha256
            candidate = (base / relative_path).resolve()
            try:
                candidate.relative_to(book)
            except ValueError as exc:
                raise SystemExit(
                    f"Formal book source escapes its volume: {lock_path}:{line_number}"
                ) from exc
            if not candidate.is_file():
                raise SystemExit(
                    f"Formal book source is missing: {candidate.as_posix()}"
                )
            if not (
                include_file(candidate)
                and _is_formal_locked_book_source(book_root.name, role, relative)
            ):
                continue
            if candidate in selected:
                previous = selected[candidate]
                raise SystemExit(
                    "Formal book source appears in multiple locks: "
                    f"{candidate.as_posix()} ({previous.lock_path}, {lock_path})"
                )
            selected[candidate] = SearchFile(
                path=candidate,
                sha256=expected_sha256,
                lock_path=lock_path,
            )
        if not entries:
            raise SystemExit(f"Formal book source lock is empty: {lock_path.as_posix()}")
        _validate_formal_lock_coverage(
            book_name=book_root.name,
            role=role,
            entries=set(entries),
            lock_path=lock_path,
        )
    return sorted(selected.values(), key=lambda source: source.path.as_posix())


def _validate_formal_lock_coverage(
    *, book_name: str, role: str, entries: set[str], lock_path: Path
) -> None:
    if book_name == "ontology-engineering-book" and role == "authoring":
        required = VOL1_REQUIRED_FORMAL_SOURCES
        unexpected: set[str] = set()
    elif book_name == "product-trustworthiness-book" and role == "authoring":
        required = VOL2_REQUIRED_FORMAL_SOURCES
        unexpected = set()
    elif book_name == "product-trustworthiness-book" and role == "guides":
        required = VOL2_FORMAL_GUIDES
        unexpected = entries - required
    else:
        raise ValueError(f"unsupported formal-lock coverage role: {book_name}/{role}")
    missing = required - entries
    if missing or unexpected:
        details: list[str] = []
        if missing:
            details.append("missing=" + ",".join(sorted(missing)))
        if unexpected:
            details.append("unexpected=" + ",".join(sorted(unexpected)))
        raise SystemExit(
            f"Formal book source lock coverage mismatch: {lock_path.as_posix()}; "
            + "; ".join(details)
        )


def _is_formal_locked_book_source(book_name: str, role: str, relative: str) -> bool:
    """Separate book content/assembly sources from locked authoring tools."""

    if book_name == "ontology-engineering-book":
        if role != "authoring":
            raise ValueError(f"unsupported Vol.1 formal-lock role: {role}")
        return (
            relative in VOL1_FORMAL_GUIDES
            or relative
            in {
                "handbook/README.md",
                "handbook/main.tex",
                "handbook/preamble.tex",
            }
            or bool(re.fullmatch(r"handbook/chapters/[^/]+\.tex", relative))
        )
    if book_name == "product-trustworthiness-book":
        if role == "guides":
            return relative in VOL2_FORMAL_GUIDES
        if role != "authoring":
            raise ValueError(f"unsupported Vol.2 formal-lock role: {role}")
        return (
            relative == "front-matter/preface.md"
            or relative.startswith("appendices/")
            or bool(re.fullmatch(r"ch\d\d-[^/]+/chapter\.md", relative))
            or relative
            in {
                "handbook/book-metadata.tex",
                "handbook/main.tex",
                "handbook/preamble.tex",
            }
        )
    raise ValueError(f"unsupported bundled book: {book_name}")


def formal_book_files() -> list[SearchFile]:
    """Return current book sources only when named and hashed by formal locks."""

    selected: dict[Path, SearchFile] = {}
    for book_root in bundled_book_roots():
        for source in _locked_book_files(book_root):
            if source.path in selected:
                raise SystemExit(
                    f"Formal book source appears in multiple volumes: {source.path}"
                )
            selected[source.path] = source
    return sorted(selected.values(), key=lambda source: source.path.as_posix())


def archive_book_files() -> list[SearchFile]:
    """Return book-adjacent text deliberately excluded from formal search."""

    formal = {source.path.resolve() for source in formal_book_files()}
    return [
        SearchFile(path.resolve())
        for path in iter_files(list(bundled_book_roots()))
        if path.resolve() not in formal
    ]


def scoped_files(workspace: Path, scope: str) -> list[SearchFile]:
    """Resolve a scope without letting archives leak into normal book results."""

    files: dict[Path, SearchFile] = {}

    def add(source: SearchFile) -> None:
        key = source.path.resolve()
        previous = files.get(key)
        if previous is not None and previous.sha256 != source.sha256:
            raise SystemExit(
                f"Search source has conflicting provenance bindings: {key.as_posix()}"
            )
        if previous is None or (previous.sha256 is None and source.sha256 is not None):
            files[key] = SearchFile(key, source.sha256, source.lock_path)

    if scope in {"all", "book"}:
        for source in formal_book_files():
            add(source)
    if scope == "archive":
        for source in archive_book_files():
            add(source)
    if scope in {"all", "pde", "cad"}:
        for path in iter_files(optional_case_roots(workspace, scope)):
            add(SearchFile(path.resolve()))
    return sorted(files.values(), key=lambda source: source.path.as_posix())


def tokenize(query: str) -> list[str]:
    query = query.strip().lower()
    terms = set(re.findall(r"[a-z0-9_+\-.]+", query))
    cjk = "".join(re.findall(r"[\u3400-\u9fff]+", query))
    if cjk:
        terms.add(cjk)
        if len(cjk) <= 4:
            for char in cjk:
                terms.add(char)
        for size in (2, 3, 4):
            for i in range(0, max(0, len(cjk) - size + 1)):
                terms.add(cjk[i : i + size])
    return sorted(terms, key=lambda item: (-len(item), item))


def score_text(text: str, full_query: str, terms: list[str]) -> int:
    haystack = text.lower()
    score = 0
    if full_query and full_query in haystack:
        score += 25 + len(full_query)
    for term in terms:
        count = haystack.count(term)
        if count:
            score += count * (4 + min(len(term), 12))
    return score


def score_line(line: str, full_query: str, terms: list[str]) -> int:
    return score_text(line, full_query, terms)


def score_path(path: Path, full_query: str, terms: list[str]) -> int:
    text = path.as_posix().lower().replace("_", " ").replace("-", " ")
    raw = path.as_posix().lower()
    score = score_text(text, full_query, terms) + score_text(raw, full_query, terms)
    return score * 2


def best_context_index(lines: list[str], full_query: str, terms: list[str]) -> int:
    if not lines:
        return 0
    scored = [
        (score_line(line, full_query, terms), idx)
        for idx, line in enumerate(lines[:200])
    ]
    scored.sort(key=lambda item: (-item[0], item[1]))
    if scored and scored[0][0] > 0:
        return scored[0][1]
    return 0


def _as_search_file(source: SearchFile | Path) -> SearchFile:
    return source if isinstance(source, SearchFile) else SearchFile(source)


def read_bytes(source: SearchFile | Path) -> bytes:
    bound = _as_search_file(source)
    data = bound.path.read_bytes()
    if bound.sha256 is not None:
        actual_sha256 = hashlib.sha256(data).hexdigest()
        if actual_sha256 != bound.sha256:
            lock = (
                bound.lock_path.as_posix()
                if bound.lock_path is not None
                else "<unknown formal lock>"
            )
            raise SystemExit(
                "Formal book source hash mismatch: "
                f"{bound.path.as_posix()}; expected {bound.sha256} from {lock}, "
                f"actual {actual_sha256}. Review the drift and refresh the owning lock "
                "deliberately before formal search."
            )
    return data


def read_text(source: SearchFile | Path) -> str:
    data = read_bytes(source)
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode("utf-8", errors="ignore")


def read_lines(source: SearchFile | Path) -> list[str]:
    return read_text(source).splitlines()


def search(
    files: list[SearchFile | Path], query: str, limit: int, context_size: int
) -> list[Hit]:
    full_query = query.lower().strip()
    terms = tokenize(query)
    hits: list[Hit] = []
    for raw_source in files:
        source = _as_search_file(raw_source)
        path = source.path
        lines = read_lines(source)
        path_score = score_path(path, full_query, terms)
        if path_score > 0 and lines:
            idx = best_context_index(lines, full_query, terms)
            start = max(0, idx - context_size)
            end = min(len(lines), idx + context_size + 1)
            context = [f"{line_no + 1}: {lines[line_no]}" for line_no in range(start, end)]
            hits.append(
                Hit(
                    score=path_score,
                    path=path,
                    line_no=idx + 1,
                    line=lines[idx],
                    context=context,
                    source_sha256=source.sha256,
                    source_lock=source.lock_path,
                )
            )
        for idx, line in enumerate(lines):
            score = score_line(line, full_query, terms)
            if score <= 0:
                continue
            start = max(0, idx - context_size)
            end = min(len(lines), idx + context_size + 1)
            context = [f"{line_no + 1}: {lines[line_no]}" for line_no in range(start, end)]
            hits.append(
                Hit(
                    score=score,
                    path=path,
                    line_no=idx + 1,
                    line=line,
                    context=context,
                    source_sha256=source.sha256,
                    source_lock=source.lock_path,
                )
            )

    hits.sort(key=lambda hit: (-hit.score, str(hit.path), hit.line_no))
    return dedupe_hits(hits)[:limit]


def dedupe_hits(hits: list[Hit]) -> list[Hit]:
    selected: list[Hit] = []
    seen_paths: dict[Path, int] = {}
    for hit in hits:
        count = seen_paths.get(hit.path, 0)
        if count >= 4:
            continue
        if any(hit.path == prev.path and abs(hit.line_no - prev.line_no) <= 2 for prev in selected):
            continue
        selected.append(hit)
        seen_paths[hit.path] = count + 1
    return selected


def emit_text(
    hits: list[Hit], workspace: Path, provenance_warning: str | None = None
) -> None:
    if provenance_warning:
        print(f"!!! {provenance_warning}")
    if not hits:
        print("No local matches found.")
        return
    for rank, hit in enumerate(hits, start=1):
        rel = display_path(hit.path, workspace)
        print(f"[{rank}] {rel}:{hit.line_no} score={hit.score}")
        for item in hit.context:
            print(f"    {item}")
        print()


def emit_json(
    hits: list[Hit], workspace: Path, provenance_warning: str | None = None
) -> None:
    if provenance_warning:
        print(f"!!! {provenance_warning}", file=sys.stderr)
    payload = [
        {
            "score": hit.score,
            "path": display_path(hit.path, workspace),
            "line": hit.line_no,
            "text": hit.line.strip(),
            "context": hit.context,
            **(
                {"source_sha256": hit.source_sha256}
                if hit.source_sha256 is not None
                else {}
            ),
            **(
                {
                    "source_status": "archive_non_authoritative",
                    "provenance_warning": provenance_warning,
                }
                if provenance_warning
                else {}
            ),
        }
        for hit in hits
    ]
    print(json.dumps(payload, ensure_ascii=False, indent=2))


def main() -> int:
    args = parse_args()
    query = " ".join(args.query)
    workspace = discover_workspace(args.root, args.scope)
    files = scoped_files(workspace, args.scope)
    hits = search(files, query, args.limit, args.context)
    warning = ARCHIVE_WARNING if args.scope == "archive" else None
    if args.json:
        emit_json(hits, workspace, warning)
    else:
        emit_text(hits, workspace, warning)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
