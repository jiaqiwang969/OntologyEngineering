"""第一卷开放世界与封闭校验佐证；可执行语义由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol1.ch04",
    claim="OWL 开放世界用于推理，SHACL 封闭检查把缺件变成可审计违规。",
    source_anchor="《工程本体论》ch04、ch07",
))
