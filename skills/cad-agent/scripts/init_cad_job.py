#!/usr/bin/env python3
"""Create a traceable AI-CAD engineering job directory from skill templates."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import shutil
import sys
from pathlib import Path


def slugify(value: str) -> str:
    value = value.strip().lower()
    value = re.sub(r"[^a-z0-9._-]+", "-", value)
    value = re.sub(r"-+", "-", value).strip("-.")
    return value


def copy_template(source: Path, target: Path) -> None:
    if not source.is_file():
        raise FileNotFoundError(f"Missing skill template: {source}")
    shutil.copyfile(source, target)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Initialize an agentic CAD engineering job with traceability templates."
    )
    parser.add_argument("name", help="Human-readable job name")
    parser.add_argument(
        "--parent",
        default=".",
        help="Parent directory for the new job (default: current directory)",
    )
    parser.add_argument(
        "--job-id",
        default="",
        help="Optional stable job identifier; defaults to a slug of the name",
    )
    parser.add_argument(
        "--owner",
        default="",
        help="Optional design owner; an empty owner intentionally blocks validation",
    )
    args = parser.parse_args()

    job_slug = slugify(args.job_id or args.name)
    if not job_slug:
        parser.error("The job name or job ID must contain at least one ASCII letter or digit.")

    parent = Path(args.parent).expanduser().resolve()
    job_dir = parent / job_slug
    if job_dir.exists():
        print(f"Refusing to overwrite existing path: {job_dir}", file=sys.stderr)
        return 2

    skill_root = Path(__file__).resolve().parents[1]
    assets = skill_root / "assets"

    job_dir.mkdir(parents=True)
    for directory in ("model", "scripts", "checks", "exports", "references"):
        (job_dir / directory).mkdir()

    brief_path = job_dir / "design-brief.yaml"
    copy_template(assets / "design-brief-template.yaml", brief_path)
    timestamp = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()
    brief = brief_path.read_text(encoding="utf-8")
    replacements = {
        '  id: ""': f"  id: {json.dumps(job_slug)}",
        '  name: ""': f"  name: {json.dumps(args.name)}",
        '  owner: ""': f"  owner: {json.dumps(args.owner)}",
        '  created_at: "YYYY-MM-DDTHH:MM:SSZ"': f"  created_at: {json.dumps(timestamp)}",
        '  updated_at: "YYYY-MM-DDTHH:MM:SSZ"': f"  updated_at: {json.dumps(timestamp)}",
    }
    for old, new in replacements.items():
        if old not in brief:
            raise RuntimeError(f"Template placeholder not found: {old}")
        brief = brief.replace(old, new, 1)
    brief_path.write_text(brief, encoding="utf-8")

    copy_template(assets / "execution-plan-template.md", job_dir / "execution-plan.md")
    copy_template(
        assets / "verification-report-template.md",
        job_dir / "checks" / "verification-report.md",
    )

    change_log = (
        "# Change Log\n\n"
        "| Timestamp (UTC) | Actor | Source revision | Target revision | "
        "Change/reason | Checks | Unresolved effects |\n"
        "|---|---|---|---|---|---|---|\n"
        f"| {timestamp} | job initializer | n/a | 0.1.0 | "
        f"Created job `{job_slug}` | not-run | Complete design brief |\n"
    )
    (job_dir / "change-log.md").write_text(change_log, encoding="utf-8")

    readme = (
        f"# {args.name}\n\n"
        "This directory was initialized by the `agentic-cad-engineering` skill.\n\n"
        "1. Complete `design-brief.yaml`.\n"
        "2. Validate it with the skill's `validate_design_brief.py`.\n"
        "3. Complete and approve `execution-plan.md` before model writes.\n"
        "4. Store machine-readable check results in `checks/`.\n"
        "5. Treat `exports/` as derived output, not the design source.\n"
    )
    (job_dir / "README.md").write_text(readme, encoding="utf-8")

    print(job_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
