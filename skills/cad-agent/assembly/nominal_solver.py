#!/usr/bin/env python3
"""Bounded assembly-order and straight-insertion screen for axis-aligned boxes.

This is a deliberately small geometric solver, not a contact or strength model.
Input boxes are final-state bounding boxes; approach vectors point from the
final position toward the start of a fixed-orientation insertion.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


def box(value: dict) -> tuple[tuple[float, ...], tuple[float, ...]]:
    low, high = value["min"], value["max"]
    if len(low) != 3 or len(high) != 3:
        raise ValueError("box needs three min and max coordinates")
    a, b = tuple(float(x) for x in low), tuple(float(x) for x in high)
    if not all(math.isfinite(x) for x in a + b) or any(x >= y for x, y in zip(a, b)):
        raise ValueError("box bounds must be finite and increasing")
    return a, b


def swept_overlap(moving: tuple, stationary: tuple, approach: tuple[float, ...]) -> bool:
    """Detect positive-volume overlap during translation, including either end."""
    m0, m1 = moving
    s0, s1 = stationary
    lower, upper = 0.0, 1.0
    for axis in range(3):
        start0 = m0[axis] + approach[axis]
        start1 = m1[axis] + approach[axis]
        delta = -approach[axis]
        if delta == 0:
            if start1 <= s0[axis] or start0 >= s1[axis]:
                return False
            continue
        if delta > 0:
            begin, end = (s0[axis] - start1) / delta, (s1[axis] - start0) / delta
        else:
            begin, end = (s1[axis] - start0) / delta, (s0[axis] - start1) / delta
        lower, upper = max(lower, begin), min(upper, end)
        if lower >= upper:
            return False
    return lower < upper


def solve(document: dict) -> dict:
    if document.get("schema") != "cad-agent.nominal-assembly/v1":
        raise ValueError("unsupported schema")
    instances = document["instances"]
    if not isinstance(instances, list) or not 1 <= len(instances) <= 12:
        raise ValueError("provide 1-12 instances")
    by_id = {entry["id"]: entry for entry in instances}
    if len(by_id) != len(instances) or any(not isinstance(k, str) or not k for k in by_id):
        raise ValueError("instance IDs must be unique nonempty strings")
    bounds = {key: box(entry["box"]) for key, entry in by_id.items()}
    initial = {key for key, entry in by_id.items() if entry.get("initial") is True}
    edges = [(edge["before"], edge["after"]) for edge in document.get("precedence", [])]
    if any(a not in by_id or b not in by_id or a == b for a, b in edges):
        raise ValueError("precedence references an unknown or identical instance")
    if any(b in initial and a not in initial for a, b in edges):
        raise ValueError("an initially installed instance cannot depend on a later installation")
    approaches = {}
    for key, entry in by_id.items():
        if key in initial:
            continue
        vectors = entry.get("approaches")
        if not isinstance(vectors, list) or not vectors:
            raise ValueError(f"{key}: approaches required")
        approaches[key] = []
        for vector in vectors:
            if not isinstance(vector, list) or len(vector) != 3:
                raise ValueError(f"{key}: approach must have three coordinates")
            values = tuple(float(x) for x in vector)
            if not all(math.isfinite(x) for x in values) or values == (0.0, 0.0, 0.0):
                raise ValueError(f"{key}: approach must be finite and nonzero")
            approaches[key].append(values)
    unresolved = set(by_id) - initial
    failed: set[frozenset[str]] = set()
    checks = 0

    def visit(placed: set[str], sequence: list[dict]) -> list[dict] | None:
        nonlocal checks
        if len(placed) == len(by_id):
            return sequence
        key_state = frozenset(placed)
        if key_state in failed:
            return None
        for key in sorted(unresolved - placed):
            if any(b == key and a not in placed for a, b in edges):
                continue
            for vector in approaches[key]:
                checks += 1
                blockers = [other for other in placed
                            if swept_overlap(bounds[key], bounds[other], vector)]
                if blockers:
                    continue
                result = visit(placed | {key}, sequence + [{"instance_id": key,
                                                            "approach_from_final": list(vector)}])
                if result is not None:
                    return result
        failed.add(key_state)
        return None

    sequence = visit(initial, [])
    return {"status": "FEASIBLE_IN_DECLARED_DOMAIN" if sequence is not None else "NO_PATH_IN_DECLARED_DOMAIN",
            "sequence": sequence, "checked_candidates": checks,
            "initial_instances": sorted(initial),
            "scope": "fixed-orientation straight translation of axis-aligned final bounding boxes; "
                     "no tool, hand, contact, tolerance, deformation, load or physical validation"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    args = parser.parse_args()
    print(json.dumps(solve(json.loads(args.input.read_text(encoding="utf-8"))),
                     ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
