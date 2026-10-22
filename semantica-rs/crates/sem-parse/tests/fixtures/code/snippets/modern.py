from __future__ import annotations
import asyncio, typing as t
from . import sibling
from ..pkg import (a as b, c)
from ...deep import *

@dataclass(frozen=True, slots=True)
@app.route("/x/<id>", methods=["GET", 'POST'])
class Point(Base[int], metaclass=Meta, **extra):
    """Doc."""
    x: int = 0

    @property
    def norm(self, /, scale: float = 1.0, *args, key=None, **kw) -> float:
        # inline comment
        return (self.x ** 2) ** .5  # trailing

    async def fetch(self):
        async with lock as l, other():
            await asyncio.sleep(0)

def outer(a, b=lambda x: x + 1, *, c):
    def inner():
        nonlocal a
        return [y for y in range(3) if (z := y)]
    class Local: pass
    match a:
        case {"k": [1, *rest]} | Point(x=0) if rest:
            def in_case(): ...
        case _:
            pass
    try:
        pass
    except* ValueError as eg:
        def in_handler(): pass
    finally:
        import json
    with (open("x") as f, open("y") as g):
        pass
    if a:
        def f1(): pass
    elif b:
        def f2(): pass
    else:
        def f3(): pass
    for i in range(3):
        def loop_fn(): pass
    else:
        pass
    while False:
        def w(): pass
    return f"{a!r:>{b}} {'q'} {c=}"

@decorator.with_args(1, x=[1, 2], *a, **k)
@(lambda f: f)
@a[0].b(c)[d]
def decorated(): pass

print("Hello")
x = 1_000 + 0x1F + 0o17 + 0b11 + 1e3 + 2j
