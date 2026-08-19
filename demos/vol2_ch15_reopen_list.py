"""第二卷需求重开佐证；可执行资产由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol2.ch15",
    claim="变更后必须机器枚举需要重开的需求与证据链。",
    source_anchor="《产品可信工程》ch15",
))
