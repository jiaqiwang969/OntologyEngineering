#!/usr/bin/env python3
"""SkillOpt-style local validation gate for the ontology-engineering skill."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


SKILL_DIR = Path(__file__).resolve().parents[1]
DEFAULT_CASES = SKILL_DIR / "references" / "eval-cases.json"
if str(SKILL_DIR) not in sys.path:
    sys.path.insert(0, str(SKILL_DIR))

from scripts import search_ontology_sources as search_module  # noqa: E402


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run deterministic retrieval checks for ontology-engineering."
    )
    parser.add_argument("--cases", type=Path, default=DEFAULT_CASES)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="Workspace root containing ontology-engineering-book.",
    )
    parser.add_argument("--split", default="valid", help="Case split to run, or 'all'.")
    parser.add_argument("--limit", type=int, default=12)
    parser.add_argument("--json", action="store_true", help="Emit JSON report.")
    return parser.parse_args()


def load_cases(path: Path, split: str) -> list[dict[str, Any]]:
    cases = json.loads(path.read_text(encoding="utf-8"))
    if split == "all":
        return cases
    return [case for case in cases if case.get("split", "valid") == split]


def hit_blob(hits: list[Any], workspace: Path) -> str:
    parts: list[str] = []
    for hit in hits:
        rel = search_module_display_path(hit.path, workspace)
        parts.append(rel)
        parts.append(hit.line)
        parts.extend(hit.context)
    return "\n".join(parts)


def hit_file_blob(hits: list[Any], workspace: Path) -> str:
    parts: list[str] = []
    seen: set[Path] = set()
    for hit in hits:
        if hit.path in seen:
            continue
        seen.add(hit.path)
        parts.append(search_module_display_path(hit.path, workspace))
        try:
            parts.append(hit.path.read_text(encoding="utf-8"))
        except UnicodeDecodeError:
            parts.append(hit.path.read_text(encoding="utf-8", errors="ignore"))
    return "\n".join(parts)


def search_module_display_path(path: Path, workspace: Path) -> str:
    return search_module.display_path(path, workspace)


def evaluate_case(search_module: Any, workspace: Path, case: dict[str, Any], limit: int) -> dict[str, Any]:
    roots = search_module.scoped_roots(workspace, case["scope"])
    files = search_module.iter_files(roots)
    hits = search_module.search(files, case["query"], limit, context_size=2)
    blob = hit_blob(hits, workspace)
    full_blob = hit_file_blob(hits, workspace)
    blob_lower = blob.lower()
    full_blob_lower = full_blob.lower()

    errors: list[str] = []
    min_hits = int(case.get("min_hits", 1))
    if len(hits) < min_hits:
        errors.append(f"expected at least {min_hits} hits, got {len(hits)}")

    required_any_paths = case.get("required_any_paths", [])
    if required_any_paths and not any(path in blob for path in required_any_paths):
        errors.append("none of required_any_paths appeared in search results")

    for term in case.get("required_terms", []):
        if term.lower() not in blob_lower and term.lower() not in full_blob_lower:
            errors.append(f"required term not found: {term}")

    top_paths = [
        search_module.display_path(hit.path, workspace)
        for hit in hits[:5]
    ]
    return {
        "id": case["id"],
        "query": case["query"],
        "scope": case["scope"],
        "passed": not errors,
        "errors": errors,
        "hit_count": len(hits),
        "top_paths": top_paths,
    }


def print_text_report(results: list[dict[str, Any]]) -> None:
    for result in results:
        status = "PASS" if result["passed"] else "FAIL"
        print(f"{status} {result['id']} ({result['scope']}): {result['query']}")
        for path in result["top_paths"]:
            print(f"  - {path}")
        for error in result["errors"]:
            print(f"  error: {error}")
    passed = sum(1 for result in results if result["passed"])
    print(f"\nResults: {passed}/{len(results)} passed")


def main() -> int:
    args = parse_args()
    workspace = search_module.discover_workspace(args.root)
    cases = load_cases(args.cases, args.split)
    if not cases:
        raise SystemExit(f"No eval cases selected from {args.cases} for split={args.split}")

    results = [
        evaluate_case(search_module, workspace, case, limit=args.limit)
        for case in cases
    ]
    if args.json:
        print(json.dumps(results, ensure_ascii=False, indent=2))
    else:
        print_text_report(results)
    return 0 if all(result["passed"] for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
