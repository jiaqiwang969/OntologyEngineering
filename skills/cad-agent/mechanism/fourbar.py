#!/usr/bin/env python3
"""Planar four-bar position screen with both assembly branches.

Geometry only: no mass, friction, actuator, contact, stress or dynamics.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


def position(ground: float, crank: float, coupler: float, rocker: float,
             input_deg: float) -> dict:
    lengths = (ground, crank, coupler, rocker)
    if not all(math.isfinite(x) and x > 0 for x in lengths):
        raise ValueError("link lengths must be positive finite numbers")
    if not math.isfinite(input_deg):
        raise ValueError("input angle must be finite")
    angle = math.radians(input_deg)
    bx, by = crank * math.cos(angle), crank * math.sin(angle)
    dx, dy = ground, 0.0
    ux, uy = dx - bx, dy - by
    distance = math.hypot(ux, uy)
    if distance < 1e-12:
        return {"input_deg": input_deg, "status": "DEGENERATE_COINCIDENT_CENTERS"}
    ux, uy = ux / distance, uy / distance
    along = (coupler**2 - rocker**2 + distance**2) / (2 * distance)
    height2 = coupler**2 - along**2
    tolerance = 1e-10 * max(lengths)**2
    if height2 < -tolerance:
        return {"input_deg": input_deg, "status": "NO_REAL_CLOSURE",
                "closure_margin_squared": height2}
    height = math.sqrt(max(0.0, height2))
    points = []
    for branch in (1, -1):
        cx = bx + along * ux - branch * height * uy
        cy = by + along * uy + branch * height * ux
        output_deg = math.degrees(math.atan2(cy, cx - ground))
        points.append({"branch": "positive" if branch == 1 else "negative",
                       "coupler_rocker_joint": [cx, cy],
                       "rocker_angle_deg": output_deg,
                       "coupler_residual": abs(math.hypot(cx - bx, cy - by) - coupler),
                       "rocker_residual": abs(math.hypot(cx - ground, cy) - rocker)})
    return {"input_deg": input_deg, "status": "TOGGLE" if height <= math.sqrt(tolerance) else "CLOSED",
            "crank_coupler_joint": [bx, by], "branches": points,
            "closure_margin_squared": height2}


def solve(document: dict) -> dict:
    if document.get("schema") != "cad-agent.planar-fourbar/v1":
        raise ValueError("unsupported schema")
    links = document["links"]
    angles = document["input_angles_deg"]
    if not isinstance(angles, list) or not 1 <= len(angles) <= 10000:
        raise ValueError("provide 1-10000 input angles")
    samples = [position(*(float(links[key]) for key in ("ground", "crank", "coupler", "rocker")),
                        float(angle)) for angle in angles]
    return {"status": "KINEMATIC_SCREEN_ONLY", "samples": samples,
            "scope": "planar pin-jointed rigid four-bar; both branches reported independently; "
                     "no branch continuity, collision, forces, dynamics or physical validation"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    args = parser.parse_args()
    print(json.dumps(solve(json.loads(args.input.read_text(encoding="utf-8"))),
                     ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
