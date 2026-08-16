#!/usr/bin/env python3
"""Search the bundled engineering-ontology book and optional local examples."""

from __future__ import annotations

import argparse
import json
import os
import re
from dataclasses import dataclass
from pathlib import Path


SKILL_DIR = Path(__file__).resolve().parents[1]
BUNDLED_WORKSPACE = SKILL_DIR / "references"
LEGACY_WORKSPACE = Path("/Users/jqwang/97-本体论")
DEFAULT_CAD_AGENT_ROOT = Path(
    "/Users/jqwang/120-agent-cad/01-fusion-tutorial/cad-agent"
)
LEGACY_CAD_AGENT_ROOT = Path(
    "/Users/jqwang/120-agent-cad/01-fusion-tutorial/cad-geometry-mcp"
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
class Hit:
    score: int
    path: Path
    line_no: int
    line: str
    context: list[str]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Search bundled ontology-engineering-book sources."
    )
    parser.add_argument("query", nargs="+", help="Search query terms.")
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="Workspace root containing ontology-engineering-book, optionally cauchyx-ai.",
    )
    parser.add_argument(
        "--scope",
        choices=("all", "book", "pde", "cad"),
        default="book",
        help=(
            "Limit search scope. PDE searches require an external root with "
            "cauchyx-ai; CAD searches use CAD_AGENT_ROOT or the local cad-agent case."
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


def discover_workspace(explicit: Path | None) -> Path:
    candidates: list[Path] = []
    if explicit:
        candidates.append(explicit)
    env_root = os.environ.get("ONTOLOGY_ENGINEERING_ROOT")
    if env_root:
        candidates.append(Path(env_root))
    candidates.append(BUNDLED_WORKSPACE)
    candidates.append(Path.cwd())
    candidates.extend(Path.cwd().parents)
    candidates.append(LEGACY_WORKSPACE)

    for candidate in candidates:
        root = candidate.expanduser().resolve()
        if root.name == "ontology-engineering-book" and root.exists():
            return root.parent
        if (root / "ontology-engineering-book").exists():
            return root

    raise SystemExit(
        "Could not find workspace containing ontology-engineering-book. "
        "Pass --root or set ONTOLOGY_ENGINEERING_ROOT."
    )


def scoped_roots(workspace: Path, scope: str) -> list[Path]:
    roots: list[Path] = []
    if scope in ("all", "book"):
        roots.append(workspace / "ontology-engineering-book")
        roots.append(workspace / "product-trustworthiness-book")
    if scope in ("all", "pde"):
        roots.append(workspace / "cauchyx-ai")
    if scope in ("all", "cad"):
        cad_root = discover_cad_agent_root()
        if cad_root is not None:
            roots.append(cad_root)
    return [root for root in roots if root.exists()]


def discover_cad_agent_root() -> Path | None:
    candidates: list[Path] = []
    env_root = os.environ.get("CAD_AGENT_ROOT")
    if env_root:
        candidates.append(Path(env_root))
    candidates.extend((DEFAULT_CAD_AGENT_ROOT, LEGACY_CAD_AGENT_ROOT))
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


def read_lines(path: Path) -> list[str]:
    try:
        return path.read_text(encoding="utf-8").splitlines()
    except UnicodeDecodeError:
        return path.read_text(encoding="utf-8", errors="ignore").splitlines()


def search(files: list[Path], query: str, limit: int, context_size: int) -> list[Hit]:
    full_query = query.lower().strip()
    terms = tokenize(query)
    hits: list[Hit] = []
    for path in files:
        lines = read_lines(path)
        path_score = score_path(path, full_query, terms)
        if path_score > 0 and lines:
            idx = best_context_index(lines, full_query, terms)
            start = max(0, idx - context_size)
            end = min(len(lines), idx + context_size + 1)
            context = [f"{line_no + 1}: {lines[line_no]}" for line_no in range(start, end)]
            hits.append(Hit(score=path_score, path=path, line_no=idx + 1, line=lines[idx], context=context))
        for idx, line in enumerate(lines):
            score = score_line(line, full_query, terms)
            if score <= 0:
                continue
            start = max(0, idx - context_size)
            end = min(len(lines), idx + context_size + 1)
            context = [f"{line_no + 1}: {lines[line_no]}" for line_no in range(start, end)]
            hits.append(Hit(score=score, path=path, line_no=idx + 1, line=line, context=context))

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


def emit_text(hits: list[Hit], workspace: Path) -> None:
    if not hits:
        print("No local matches found.")
        return
    for rank, hit in enumerate(hits, start=1):
        rel = display_path(hit.path, workspace)
        print(f"[{rank}] {rel}:{hit.line_no} score={hit.score}")
        for item in hit.context:
            print(f"    {item}")
        print()


def emit_json(hits: list[Hit], workspace: Path) -> None:
    payload = [
        {
            "score": hit.score,
            "path": display_path(hit.path, workspace),
            "line": hit.line_no,
            "text": hit.line.strip(),
            "context": hit.context,
        }
        for hit in hits
    ]
    print(json.dumps(payload, ensure_ascii=False, indent=2))


def main() -> int:
    args = parse_args()
    query = " ".join(args.query)
    workspace = discover_workspace(args.root)
    files = iter_files(scoped_roots(workspace, args.scope))
    hits = search(files, query, args.limit, args.context)
    if args.json:
        emit_json(hits, workspace)
    else:
        emit_text(hits, workspace)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
