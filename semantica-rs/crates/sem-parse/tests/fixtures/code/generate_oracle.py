"""Generate tests/fixtures/code/oracle.json (and the corpus/snippet files) from the original.

Inputs: a deterministic sample of real Python files of the original source tree (copied to
corpus/), snippet files in several languages and syntax-error / modern-syntax cases
(snippets/). Run from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python \
        <crate>/tests/fixtures/code/generate_oracle.py <crate>/tests/fixtures/code <original tree>

Absolute fixture paths are written as "$FILES". No network.
"""

import email.message  # noqa: F401
import json
import os
import shutil
import sys

sys.path.insert(0, os.getcwd())

OUT = os.path.abspath(sys.argv[1])
TREE = os.path.abspath(sys.argv[2])
CORPUS = os.path.join(OUT, "corpus")
SNIPS = os.path.join(OUT, "snippets")
os.makedirs(CORPUS, exist_ok=True)
os.makedirs(SNIPS, exist_ok=True)

from semantica.utils.progress_tracker import get_progress_tracker  # noqa: E402

get_progress_tracker().displays = []

from semantica.parse import CodeParser, CommentExtractor, DependencyAnalyzer, SyntaxTreeParser  # noqa: E402


def norm(v):
    return json.loads(json.dumps(v, default=lambda o: o.__dict__).replace(OUT, "$FILES"))


def run(fn, *a, **k):
    try:
        return {"ok": norm(fn(*a, **k))}
    except Exception as e:  # noqa: BLE001
        return {"error": type(e).__name__, "message": str(e).replace(OUT, "$FILES")}


# ------------------------------------------------------------------ corpus
candidates = []
for root, dirs, files in os.walk(TREE):
    dirs.sort()
    for name in sorted(files):
        if name.endswith(".py"):
            p = os.path.join(root, name)
            size = os.path.getsize(p)
            if 1024 < size < 40 * 1024:
                candidates.append(p)
candidates.sort()
corpus = []
for i, p in enumerate(candidates):
    if i % 16 == 0:
        rel = os.path.relpath(p, TREE).replace("/", "__")
        dst = os.path.join(CORPUS, rel)
        shutil.copyfile(p, dst)
        corpus.append(rel)

# ------------------------------------------------------------------ snippets
SNIPPETS = {
    "modern.py": '''from __future__ import annotations
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
''',
    "syntax_error.py": "def broken(:\n    pass\nclass Ok:\n    pass\nimport os\n",
    "indent_error.py": "def f():\nreturn 1\n",
    "tab_error.py": "if True:\n\tx = 1\n        y = 2\n",
    "unclosed.py": "x = (1,\ny = [\n",
    "bad_string.py": "s = 'unterminated\n",
    "walrus_bad.py": "x := 1\n",
    "assign_call.py": "f() = 1\n",
    "fstring_nested.py": "x = f\"{'a' + \"b\"}\"\nprint(x)\n",
    "print_stmt.py": "print 'hello'\n",
    "lambda_default.py": "f = lambda x=1, *a, y, **k: (x, a, y, k)\n",
    "return_outside.py": "return 5\nbreak\n",
    "nul_byte.py": "x = 1\x00\n",
    "bom.py": "﻿import os\n",
    "empty.py": "",
    "only_comments.py": "# a\n# b \"\"\"not doc\"\"\"\n'''doc'''\n",
    "decorators.py": "@a.b\n@c(1)\n@d[1](2)\nclass K(A, B.C, metaclass=M):\n    @staticmethod\n    def s(x, y=2): pass\n",
    "script.js": "import x from 'lib';\nconst y = require('fs');\nfunction hello() { // greet\n  return 1; /* block */\n}\nvar z = function() {};\nclass Foo {}\n",
    "Main.java": "import java.util.List;\npublic class Main {\n  public static void main(String[] args) { // entry\n  }\n  private int helper(int x) { /* h */ return x; }\n}\n",
    "prog.c": "#include <stdio.h>\n#include \"local.h\"\nstruct point { int x; };\nint main(void) {\n  return 0; // done\n}\n",
    "prog.cpp": "#include <vector>\nclass A {};\nint f(int x) { return x; }\n",
    "main.go": "package main\nimport \"fmt\"\nfunc main() { // go\n}\n",
    "app.rb": "require 'json'\ndef hello\nend\nclass Foo\nend\n",
    "lib.rs": "use std::io;\nfn main() { // rust\n}\nstruct S;\n",
    "page.php": "<?php\nfunction f() { // php\n}\n",
    "Prog.cs": "using System;\nclass P { }\n",
    "notes.txt": "import nothing\nfunction like text // comment\n",
}
for name, content in SNIPPETS.items():
    with open(os.path.join(SNIPS, name), "w", encoding="utf-8", newline="") as fh:
        fh.write(content)

cp = CodeParser()
stp = SyntaxTreeParser()
ce = CommentExtractor()
da = DependencyAnalyzer()


def per_file(path, language=None):
    with open(path, "r", encoding="utf-8", errors="ignore") as fh:
        content = fh.read()
    rec = {"parse_code": run(cp.parse_code, path) if language is None else run(cp.parse_code, path, language=language)}
    tree = None
    try:
        tree = stp.parse_syntax_tree(content, "python")
        rec["syntax_tree_none"] = tree is None
    except Exception as e:  # noqa: BLE001
        rec["syntax_tree_none"] = {"error": type(e).__name__, "message": str(e)}
    rec["analyze_structure"] = norm(stp.analyze_structure(tree)) if not isinstance(rec["syntax_tree_none"], dict) else None
    rec["docstrings"] = run(ce.extract_docstrings, content, "python")
    return rec


OUT_JSON = {"corpus": {}, "snippets": {}, "languages": {}, "misc": {}}
for rel in corpus:
    OUT_JSON["corpus"][rel] = per_file(os.path.join(CORPUS, rel))
for name in SNIPPETS:
    OUT_JSON["snippets"][name] = per_file(os.path.join(SNIPS, name))

# The same source under every language (regex paths) and string-level APIs.
SRC = SNIPPETS["modern.py"].split("\nprint(")[0]
for lang in ["python", "javascript", "java", "c", "go", "unknown", "cpp", "ruby"]:
    OUT_JSON["languages"][lang] = {
        "structure": run(cp.extract_structure, SRC, lang),
        "comments": run(cp.extract_comments, SRC, lang),
        "dependencies": run(cp.analyze_dependencies, SRC, lang),
        "js_src_structure": run(cp.extract_structure, SNIPPETS["script.js"], lang),
        "c_src_comments": run(cp.extract_comments, SNIPPETS["prog.c"], lang),
    }
OUT_JSON["misc"]["missing_file"] = run(cp.parse_code, os.path.join(SNIPS, "missing.py"))
OUT_JSON["misc"]["explicit_language"] = run(cp.parse_code, os.path.join(SNIPS, "script.js"), language="python")
OUT_JSON["misc"]["unsupported_language"] = run(cp.parse_code, os.path.join(SNIPS, "notes.txt"), language="cobol")
OUT_JSON["misc"]["analyze_structure_none"] = norm(stp.analyze_structure(None))
OUT_JSON["misc"]["supported_languages"] = cp.supported_languages
OUT_JSON["misc"]["analyze_many"] = norm(stp.analyze_structure(stp.parse_syntax_tree(
    "\n".join(f"def f{i}(): pass" for i in range(12)), "python")))
OUT_JSON["misc"]["analyze_high"] = norm(stp.analyze_structure(stp.parse_syntax_tree(
    "\n".join(f"class C{i}: pass" for i in range(55)), "python")))
OUT_JSON["misc"]["dependencies_stdlib"] = run(da.analyze_dependencies,
                                              "import os.path, collectionsx\nfrom . import y\nfrom xml.dom import minidom\nimport requests\n", "python")
OUT_JSON["corpus_files"] = corpus

with open(os.path.join(OUT, "oracle.json"), "w", encoding="utf-8") as fh:
    json.dump(OUT_JSON, fh, ensure_ascii=False, indent=0)
print("wrote", len(corpus), "corpus files")
