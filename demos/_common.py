"""demos 公共入口：静默 Semantica 进度条、定位 skill 根与 fixtures。

所有 demo 的第一行 import 应为 `import _common`（或 from _common import ...），
保证在任何 semantica 模块加载前关闭进度输出，佐证输出保持干净。
数据（fixtures/*.ttl）与断言（demo 代码）分离：图可以被单独审阅与复用。
"""

import os
from pathlib import Path

os.environ.setdefault("SEMANTICA_DISABLE_PROGRESS", "1")

SKILL_ROOT = Path(__file__).resolve().parents[1]
FIXTURES = Path(__file__).resolve().parent / "fixtures"


def load_fixture(name: str):
    """加载 fixtures/<name>.ttl 为 rdflib Graph。"""
    from rdflib import Graph
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g
