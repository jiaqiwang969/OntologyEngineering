"""Common thin launcher for Semantica-owned chapter-package demonstrations."""

from __future__ import annotations

import os
from pathlib import Path
import sys
from typing import Optional


os.environ.setdefault("SEMANTICA_DISABLE_PROGRESS", "1")
SKILL_ROOT = Path(__file__).resolve().parents[1]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from ontology_engineering.semantica_runtime import run_package


def run_package_demo(
    package_id: str,
    *,
    claim: str,
    source_anchor: str,
    scenario_id: Optional[str] = None,
) -> int:
    """Run one built-in Semantica package and print its immutable evidence."""

    result = run_package(package_id, scenario_id)
    print(f"【书中论断】{claim}")
    print(f"【书源锚点】{source_anchor}")
    print(f"【Semantica 包】{result.package_id}@{result.package_version}")
    print(f"【场景】{result.scenario_id}")
    print(f"【包摘要】{result.package_digest}")
    print(f"【执行状态】{result.status}")

    for check in result.oracle_checks:
        mark = "✓" if check.status == "passed" else "✗"
        print(f"  [{mark}] {check.check_id}: {check.status} — {check.message}")

    release = "complete" if result.release_verdict.complete else "blocked"
    print(f"【发布状态】{release}")
    if result.reasons:
        print("【未闭合项】" + ", ".join(result.reasons))

    passed = result.status == "passed"
    print(
        "【佐证结论】"
        + ("成立" if passed else "未成立")
        + "：CQ、数据、查询/形状/规则、案例与精确 oracle 均由 Semantica 内建包执行。"
    )
    return 0 if passed else 1
