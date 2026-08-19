#!/usr/bin/env python3
"""Thin audit of Semantica's authoritative 29-chapter package registry."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import sys
from typing import Any


SKILL_ROOT = Path(__file__).resolve().parents[1]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from ontology_engineering.semantica_runtime import (  # noqa: E402
    list_chapter_packages,
    validate_chapter_registry,
)


@dataclass(frozen=True)
class ValidationReport:
    contract_count: int
    errors: tuple[str, ...]
    blockers: tuple[str, ...]
    complete_contracts: tuple[str, ...]

    @property
    def structurally_valid(self) -> bool:
        return not self.errors and self.contract_count == 29

    @property
    def release_ready(self) -> bool:
        return self.structurally_valid and not self.blockers

    def as_dict(self) -> dict[str, Any]:
        return {
            "contract_count": self.contract_count,
            "structurally_valid": self.structurally_valid,
            "release_ready": self.release_ready,
            "complete_contracts": list(self.complete_contracts),
            "blocked_contract_count": len(self.blockers),
            "errors": list(self.errors),
            "blockers": list(self.blockers),
        }


def validate_contracts(_root: Path | None = None) -> ValidationReport:
    """Delegate all contract validation to Semantica; no OE ledger is read."""

    packages = list_chapter_packages()
    issues = tuple(validate_chapter_registry())
    blockers = tuple(
        item.package_id for item in packages if item.release_status != "complete"
    )
    complete = tuple(
        item.package_id for item in packages if item.release_status == "complete"
    )
    return ValidationReport(len(packages), issues, blockers, complete)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Validate Semantica's built-in chapter-package registry."
    )
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    report = validate_contracts()
    if args.json:
        print(json.dumps(report.as_dict(), ensure_ascii=False, sort_keys=True, indent=2))
    else:
        print(
            "contracts={} structurally_valid={} blocked={} release_ready={}".format(
                report.contract_count,
                int(report.structurally_valid),
                len(report.blockers),
                int(report.release_ready),
            )
        )
        for issue in report.errors:
            print("ERROR: " + issue, file=sys.stderr)
        if args.release:
            for package_id in report.blockers:
                print("BLOCKED: " + package_id, file=sys.stderr)
    if args.release:
        return 0 if report.release_ready else 1
    return 0 if report.structurally_valid else 1


if __name__ == "__main__":
    raise SystemExit(main())
