"""第二卷依赖与独立性佐证；可执行资产由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol2.ch18",
    claim="依赖图中的共同原因与独立性要求必须由机器求交并门禁。",
    source_anchor="《产品可信工程》ch18",
))
