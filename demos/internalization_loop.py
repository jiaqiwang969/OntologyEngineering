"""行业本体内化循环佐证；场景、CQ 与语义执行均由 Semantica 独占。"""

import sys
import _common  # noqa: F401
from ontology_engineering.semantica_runtime import (
    run_governance_acceptance_scenario,
)


result = run_governance_acceptance_scenario()
print("【书中论断】行业本体必须在带理由的冲突判决与 CQ 回归中学新不忘旧。")
print("【书源锚点】《工程本体论》ch03；《产品可信工程》ch17、ch20")
print("【Semantica 治理验收】")
for check in result.checks:
    print(f"  [{'✓' if check.passed else '✗'}] {check.check_id}: {check.detail}")
print(f"【版本数】{result.version_count}")
print(f"【故意遗忘后的回归】{result.final_regression_status}")
print(f"【佐证结论】{'成立' if result.passed else '未成立'}")
sys.exit(0 if result.passed else 1)
