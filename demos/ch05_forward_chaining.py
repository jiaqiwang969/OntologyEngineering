"""第一卷前向链佐证；事实、规则与 oracle 均由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol1.ch05",
    claim="受支持的单调规则可由前向链复算并留下逐步推理证据。",
    source_anchor="《工程本体论》ch05",
))
