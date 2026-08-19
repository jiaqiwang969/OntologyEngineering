"""第一卷 CQ 验收佐证；CQ、数据、查询与 oracle 由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol1.ch03",
    claim="能力问题既是需求规格，也是可以机器执行的验收测试。",
    source_anchor="《工程本体论》ch03、ch04",
))
