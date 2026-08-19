"""第二卷发布绑定门禁佐证；可执行资产由 Semantica 独占。"""

import sys
from _common import run_package_demo

sys.exit(run_package_demo(
    "semantica.chapter_packages.vol2.ch20",
    claim="发布包成员必须绑定同一快照，偏离时必须拒绝并留下处置证据。",
    source_anchor="《产品可信工程》ch20",
))
