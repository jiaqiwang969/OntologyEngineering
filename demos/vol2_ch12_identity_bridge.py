"""第二卷身份桥接佐证；可执行资产由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol2.ch12",
    claim="跨系统身份桥必须可追溯、可验证，并拒绝错误映射。",
    source_anchor="《产品可信工程》ch12",
))
