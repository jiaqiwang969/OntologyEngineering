"""第一卷推理模式佐证；可执行语义由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol1.ch02",
    claim="单调推理不会因新增事实撤销旧结论，开放世界中的未知不等于假。",
    source_anchor="《工程本体论》ch02",
))
