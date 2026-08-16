#!/usr/bin/env python3
"""把 ontology-engineering-book 的 README 与示例代码转换为 XeLaTeX 片段。

用法：python3 build_handbook.py
输出：handbook/fragments/*.tex
"""
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
OUT = HERE / "fragments"
OUT.mkdir(exist_ok=True)

# ---------------------------------------------------------------- 基础转换
TEX_SPECIALS = {
    "\\": r"\textbackslash{}",
    "{": r"\{",
    "}": r"\}",
    "$": r"\$",
    "&": r"\&",
    "#": r"\#",
    "^": r"\^{}",
    "_": r"\_",
    "%": r"\%",
    "~": r"\textasciitilde{}",
}


def tex_escape(s: str) -> str:
    return "".join(TEX_SPECIALS.get(c, c) for c in s)


# 在转义之后应用：替换结果是最终 LaTeX，使用数学宏以避免字体缺字
SYMBOL_MAP = [
    ("⚠️", ""),
    ("⚠", ""),
    ("️", ""),
    ("🔗", "链接"),
    ("¬", r"\(\lnot\)"),
    ("×", r"\(\times\)"),
    ("↑", r"\(\uparrow\)"),
    ("→", r"\(\rightarrow\)"),
    ("∀", r"\(\forall\)"),
    ("∃", r"\(\exists\)"),
    ("∧", r"\(\land\)"),
    ("∨", r"\(\lor\)"),
    ("≈", r"\(\approx\)"),
    ("≡", r"\(\equiv\)"),
    ("≤", r"\(\le\)"),
    ("≥", r"\(\ge\)"),
    ("⊏", r"\(\sqsubset\)"),
    ("⊑", r"\(\sqsubseteq\)"),
    ("⊓", r"\(\sqcap\)"),
    ("⊔", r"\(\sqcup\)"),
    ("⊥", r"\(\bot\)"),
    ("✓", r"\(\checkmark\)"),
    ("✗", r"\(\times\)"),
]


def map_symbols(s: str) -> str:
    for k, v in SYMBOL_MAP:
        s = s.replace(k, v)
    return s


def conv_text(s: str) -> str:
    return map_symbols(tex_escape(s))


def dwidth(s: str) -> int:
    """终端显示宽度估计：CJK 记 2，其余记 1。"""
    return sum(2 if ord(c) > 0x2E00 else 1 for c in s)


# ---------------------------------------------------------------- 代码渲染
MAXW = 84

COMMENT_MARKERS = {
    ".txt": ("#",),
    ".py": ("#",),
    ".sh": ("#",),
    ".ttl": ("#",),
    ".sparql": ("#",),
    ".rq": ("#",),
    ".swrl": ("#",),
    ".owl": ("#",),
    ".java": ("//", "*", "/*"),
}
XML_EXTS = (".rdf", ".xml")

FENCE_LANG_EXT = {
    "bash": ".sh",
    "shell": ".sh",
    "python": ".py",
    "java": ".java",
    "xml": ".xml",
    "turtle": ".ttl",
    "sparql": ".sparql",
    "text": ".none",
    "": ".none",
}


def wrap_line(line: str):
    """按显示宽度折行，续行缩进 4 个空格。"""
    if dwidth(line) <= MAXW:
        return [line]
    indent = len(line) - len(line.lstrip(" "))
    cont = " " * (indent + 4)
    toks = re.split(r"(\s+)", line.lstrip(" "))
    out, cur = [], " " * indent
    for tok in toks:
        if cur.strip() and dwidth(cur) + dwidth(tok.rstrip()) > MAXW:
            out.append(cur.rstrip())
            cur = cont
        cur += tok
    if cur.strip():
        out.append(cur.rstrip())
    # 单个超长 token 的兜底硬切
    final = []
    for piece in out:
        while dwidth(piece) > MAXW + 12:
            final.append(piece[:MAXW])
            piece = "    " + piece[MAXW:]
        final.append(piece)
    return final or [""]


def emit_code(lines, ext: str):
    out = [r"\begin{codebox}"]
    markers = COMMENT_MARKERS.get(ext, ())
    xml_mode = ext in XML_EXTS
    in_xml = False
    for raw in lines:
        line = raw.rstrip("\n").rstrip().expandtabs(4)
        if not line.strip():
            out.append(r"\codeblank{}")
            continue
        for piece in wrap_line(line):
            n = len(piece) - len(piece.lstrip(" "))
            body = piece.lstrip(" ")
            if xml_mode:
                is_cmt = in_xml or body.startswith("<!--")
                if "<!--" in body and "-->" not in body:
                    in_xml = True
                if "-->" in body:
                    in_xml = False
            else:
                is_cmt = any(body.startswith(m) for m in markers)
            tex = conv_text(body).replace(" ", r"\ ")
            pre = (r"\hspace*{%d\ttcw}" % n) if n else ""
            if is_cmt:
                out.append(r"\codeline{%s\textcolor{cmtgray}{%s}}" % (pre, tex))
            else:
                out.append(r"\codeline{%s%s}" % (pre, tex))
    out.append(r"\end{codebox}")
    return out


# ---------------------------------------------------------------- Markdown 内联
INLINE_RE = re.compile(
    r"(?P<code>`[^`]+`)"
    r"|(?P<bold>\*\*[^*]+\*\*)"
    r"|(?P<ital>\*[^*\s][^*]*\*)"
    r"|(?P<link>\[[^\]]*\]\([^)]+\))"
    r"|(?P<auto><https?://[^>]+>)"
)


def esc_url(u: str) -> str:
    return u.replace("%", r"\%").replace("#", r"\#")


def conv_inline(s: str) -> str:
    out, pos = [], 0
    for m in INLINE_RE.finditer(s):
        out.append(conv_text(s[pos : m.start()]))
        g = m.lastgroup
        t = m.group(0)
        if g == "code":
            out.append(r"\texttt{%s}" % conv_text(t[1:-1]))
        elif g == "bold":
            out.append(r"\textbf{%s}" % conv_inline(t[2:-2]))
        elif g == "ital":
            out.append(r"\textit{%s}" % conv_inline(t[1:-1]))
        elif g == "link":
            mm = re.match(r"\[([^\]]*)\]\(([^)]+)\)", t)
            text, url = mm.group(1), mm.group(2)
            if url.startswith("http"):
                out.append(r"\href{%s}{%s}" % (esc_url(url), conv_inline(text)))
            else:  # 文内锚点：只保留文字
                out.append(conv_inline(text))
        elif g == "auto":
            out.append(r"\url{%s}" % t[1:-1])
        pos = m.end()
    out.append(conv_text(s[pos:]))
    return "".join(out)


# ---------------------------------------------------------------- Markdown 块级
HEAD_RE = re.compile(r"^(#{1,4})\s+(.*)$")
TABLE_SEP_RE = re.compile(r"^\s*\|?[\s:|-]+\|?\s*$")
OL_RE = re.compile(r"^(\s*)\d+[.)]\s+(.*)$")
UL_RE = re.compile(r"^(\s*)[-*]\s+(.*)$")

SEC_CMDS = ["", "\\section", "\\subsection", "\\subsubsection", "\\paragraph"]


def render_table(rows):
    ncols = max(len(r) for r in rows)
    rows = [r + [""] * (ncols - len(r)) for r in rows]
    colw = [max(dwidth(r[i]) for r in rows) for i in range(ncols)]
    if sum(colw) <= 58:  # 窄表用自然宽度
        spec = "".join("l" for _ in range(ncols))
        env = "tabular"
    else:
        spec = "".join("X" if colw[i] > 16 else "l" for i in range(ncols))
        if "X" not in spec:
            spec = spec[:-1] + "X"
        env = "tabularx"
    out = [r"\begin{center}"]
    if env == "tabularx":
        out.append(r"\begin{tabularx}{\linewidth}{@{}%s@{}}" % spec)
    else:
        out.append(r"\begin{tabular}{@{}%s@{}}" % spec)
    out.append(r"\toprule")
    head = rows[0]
    out.append(" & ".join(r"\textbf{%s}" % conv_inline(c) for c in head) + r" \\")
    out.append(r"\midrule")
    for r in rows[1:]:
        out.append(" & ".join(conv_inline(c) for c in r) + r" \\")
    out.append(r"\bottomrule")
    out.append(r"\end{%s}" % env)
    out.append(r"\end{center}")
    return out


def split_row(line: str):
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|"):
        line = line[:-1]
    return [c.strip() for c in line.split("|")]


def convert_md_lines(lines, sec_offset=1, starred=False):
    """sec_offset: '##' 映射到 SEC_CMDS[sec_offset]。h1 一律丢弃（章标题由主文档给出）。"""
    out = []
    i, n = 0, len(lines)
    star = "*" if starred else ""
    while i < n:
        line = lines[i].rstrip("\n")
        s = line.strip()

        if not s:
            out.append("")
            i += 1
            continue
        if s in ("---", "***", "___"):
            i += 1
            continue

        m = HEAD_RE.match(s)
        if m:
            level = len(m.group(1))
            if level == 1:
                i += 1
                continue
            cmd = SEC_CMDS[min(level - 2 + sec_offset, 4)]
            title = conv_inline(m.group(2).strip())
            if cmd == "\\paragraph":
                out.append(r"\paragraph{%s}" % title)
            else:
                out.append("%s%s{%s}" % (cmd, star, title))
            i += 1
            continue

        if s.startswith("```"):
            lang = s[3:].strip().lower()
            ext = FENCE_LANG_EXT.get(lang, ".none")
            block = []
            i += 1
            while i < n and not lines[i].strip().startswith("```"):
                block.append(lines[i])
                i += 1
            i += 1  # 跳过结尾 ```
            out.extend(emit_code(block, ext))
            continue

        if s.startswith("|"):
            rows = []
            while i < n and lines[i].strip().startswith("|"):
                row = lines[i].strip()
                if not TABLE_SEP_RE.match(row):
                    rows.append(split_row(row))
                i += 1
            if rows:
                out.extend(render_table(rows))
            continue

        if s.startswith(">"):
            block = []
            while i < n and lines[i].strip().startswith(">"):
                block.append(re.sub(r"^\s*>\s?", "", lines[i].rstrip("\n")))
                i += 1
            out.append(r"\begin{noteblock}")
            out.extend(convert_md_lines(block, sec_offset, starred))
            out.append(r"\end{noteblock}")
            continue

        if UL_RE.match(line) or OL_RE.match(line):
            ordered = bool(OL_RE.match(line))
            env = "enumerate" if ordered else "itemize"
            item_re = OL_RE if ordered else UL_RE
            out.append(r"\begin{%s}" % env)
            sub = None  # (env, base_indent)
            while i < n:
                mm = item_re.match(lines[i].rstrip("\n"))
                mm2 = (UL_RE if ordered else OL_RE).match(lines[i].rstrip("\n"))
                if mm and len(mm.group(1)) == 0:
                    if sub:
                        out.append(r"\end{%s}" % sub)
                        sub = None
                    out.append(r"\item %s" % conv_inline(mm.group(2)))
                    i += 1
                elif (mm and len(mm.group(1)) > 0) or (mm2 and len(mm2.group(1)) > 0):
                    mx = mm or mm2
                    sub_env = "enumerate" if (mx is mm and ordered) or (mx is mm2 and not ordered) else "itemize"
                    if not sub:
                        sub = "itemize" if not OL_RE.match(lines[i].rstrip("\n")) else "enumerate"
                        out.append(r"\begin{%s}" % sub)
                    out.append(r"\item %s" % conv_inline(mx.group(2)))
                    i += 1
                elif i < n and lines[i].strip() and not HEAD_RE.match(lines[i].strip()) \
                        and not lines[i].strip().startswith(("|", "```", ">")) \
                        and not UL_RE.match(lines[i]) and not OL_RE.match(lines[i]) \
                        and lines[i].startswith("  "):
                    # 悬挂续行
                    out.append(conv_inline(lines[i].strip()))
                    i += 1
                else:
                    break
            if sub:
                out.append(r"\end{%s}" % sub)
            out.append(r"\end{%s}" % env)
            continue

        # 普通段落
        out.append(conv_inline(s))
        i += 1
    return out


def convert_md_file(path: Path, sec_offset=1, starred=False, skip_sections=()):
    lines = path.read_text(encoding="utf-8").splitlines()
    if skip_sections:
        kept, skipping = [], False
        for ln in lines:
            m = HEAD_RE.match(ln.strip())
            if m and m.group(2).strip() in skip_sections:
                skipping = True
                continue
            if m and skipping:
                skipping = False
            if not skipping:
                kept.append(ln)
        lines = kept
    return convert_md_lines(lines, sec_offset, starred)


# ---------------------------------------------------------------- 代码节选
# 书中正文只排印关键片段：按起止标记从真实文件中节选，保证与仓库一致。
# 格式：片段名: (仓库相对路径, 起始标记, 结束标记, 结束行后额外行数)
SNIPPETS = {
    "snip-ch01-paths": ("ch01-introduction/examples/ontology-in-ai-era.txt",
                        "路径A", "被严格区分", 0),
    "snip-ch02-hier": ("ch02-ontology-foundations/examples/core-concepts.txt",
                       "# 设备分类层次", "工装设备 (ToolingEquipment)", 0),
    "snip-ch02-default": ("ch02-ontology-foundations/examples/reasoning-examples.txt",
                          "# 默认规则（Reiter", "无法表达的", 0),
    "snip-ch03-cq": ("ch03-ontology-methodology/examples/competency-questions.txt",
                     "# CQ1 的验收查询", "需修复后复测", 0),
    "snip-ch03-ontoclean": ("ch03-ontology-methodology/examples/ontoclean-evaluation.txt",
                            "# 违规示例1", "角色通过对象属性关联", 0),
    "snip-ch04-turtle": ("ch04-ontology-languages/examples/rdf-turtle-examples.ttl",
                         "@prefix rdf:", "工单知识", 0),
    "snip-ch04-manchester": ("ch04-ontology-languages/examples/owl-classes.owl",
                             "Class: :Equipment", "SubClassOf: :CNCMachine", 0),
    "snip-ch04-sparql": ("ch04-ontology-languages/examples/sparql-queries.sparql",
                         "# CQ1:", "ORDER BY ASC(?name)", 0),
    "snip-ch05-swrl": ("ch05-reasoning/examples/swrl-rules.swrl",
                       "# 规则：高功率设备需要冷却", "Lathe_003 需要冷却", 0),
    "snip-ch05-bayes": ("ch05-reasoning/examples/probabilistic-reasoning.txt",
                        "# 第一步，用全概率公式", "维修时优先排查冷却系统", 0),
    "snip-ch07-shacl": ("ch07-knowledge-graph/examples/kg-quality-shacl.ttl",
                        "mfgsh:EquipmentShape", "EQ-YYYY-NNNN 的序列号", 1),
    "snip-ch07-cypher": ("ch07-knowledge-graph/examples/kg-storage-query.txt",
                         "# SPARQL（三元组库）", "RETURN e.name, e.power", 0),
    "snip-ch08-demo": ("ch08-ontology-llm/examples/hallucination-control.txt",
                       "能加工钛合金吗", "请联系工艺部门", 1),
    "snip-ch08-agent": ("ch08-ontology-llm/examples/ontology-guided-agent.py",
                        "def validate(self, proposal", "通过全部本体校验", 1),
    "snip-ch09-axioms": ("ch09-capstone-manufacturing/src/manufacturing.owl",
                         "# 描述逻辑约束", "canProcess.Material", 0),
    "snip-ch09-java": ("ch09-capstone-manufacturing/src/QueryService.java",
                       "// 执行查询（使用Jena 4.x", "return results;", 0),
    "snip-ch09-py": ("ch09-capstone-manufacturing/src/reasoner.py",
                     "def run_reasoner", "推理完成\")", 0),
}


def emit_snippet(relpath: str, start: str, end: str, extend: int):
    fp = ROOT / relpath
    lines = fp.read_text(encoding="utf-8").splitlines()
    s = e = None
    for i, ln in enumerate(lines):
        if s is None and start in ln:
            s = i
        elif s is not None and end in ln:
            e = i
            break
    if s is None or e is None:
        raise SystemExit(f"节选标记未命中: {relpath} [{start} .. {end}]")
    return emit_code(lines[s:e + 1 + extend], fp.suffix.lower())


# ---------------------------------------------------------------- 小节自动切分
# 仓库示例文件普遍使用 “# ===== / # N. 标题 / # =====” 分节。
# 为每个小节生成独立片段 sec-<文件标识>-<n>.tex，供正文按需引用。
DIVIDER_RE = re.compile(r"^\s*(#|//)\s*={8,}\s*$")


def split_sections(lines):
    """返回 [(标题, start, end)]；文件头（首个分节前）记为第0节。"""
    heads = []  # (title_line_index, title)
    i = 0
    while i < len(lines) - 2:
        if DIVIDER_RE.match(lines[i]) and DIVIDER_RE.match(lines[i + 2]):
            title = re.sub(r"^\s*(#|//)\s*", "", lines[i + 1]).strip()
            heads.append((i, title))
            i += 3
        else:
            i += 1
    if not heads:
        return [("全文", 0, len(lines))]
    secs = []
    if heads[0][0] > 0:
        secs.append(("文件头", 0, heads[0][0]))
    for k, (start, title) in enumerate(heads):
        end = heads[k + 1][0] if k + 1 < len(heads) else len(lines)
        secs.append((title, start, end))
    return secs


def emit_section_fragments(code_files):
    index = ["# 小节片段索引（fragment 名 → 文件 · 小节标题 · 行数）", ""]
    for fp in code_files:
        tag = fp.parts[-3].split("-")[0]
        stem = f"sec-{tag}-{fp.name.replace('.', '-')}"
        lines = fp.read_text(encoding="utf-8").splitlines()
        for n, (title, s, e) in enumerate(split_sections(lines)):
            frag = f"{stem}-{n}.tex"
            (OUT / frag).write_text(
                "\n".join(emit_code(lines[s:e], fp.suffix.lower())) + "\n",
                encoding="utf-8")
            index.append(f"{frag}  ·  {fp.name}  ·  {title}  ·  {e-s}行")
    (OUT / "INDEX.md").write_text("\n".join(index) + "\n", encoding="utf-8")
    print(f"  -> fragments/INDEX.md（小节片段 {len(index)-2} 个）")


# ---------------------------------------------------------------- 生成
def write(name: str, lines):
    (OUT / name).write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"  -> fragments/{name}")


def main():
    print("生成插图提示词片段：")
    sys.path.insert(0, str(HERE))
    try:
        from gen_figures import PROMPTS
        for fig_name, prompt in PROMPTS.items():
            write(f"prompt-{fig_name}.tex", [conv_text(prompt)])
    except ImportError:
        print("  （未找到 gen_figures.py，跳过）")

    print("生成 README 片段：")
    write("readme-main.tex", convert_md_file(ROOT / "README.md", starred=True))
    for ch in ["ch01-introduction", "ch02-ontology-foundations",
               "ch03-ontology-methodology", "ch04-ontology-languages",
               "ch05-reasoning", "ch06-applications", "ch07-knowledge-graph",
               "ch08-ontology-llm", "ch09-capstone-manufacturing"]:
        tag = ch.split("-")[0]
        write(f"readme-{tag}.tex", convert_md_file(ROOT / ch / "README.md"))
    write("readme-resources.tex",
          convert_md_file(ROOT / "resources" / "README.md", skip_sections=("目录",)))

    print("生成正文节选片段：")
    for snip_name, (relpath, start, end, ext_n) in SNIPPETS.items():
        write(f"{snip_name}.tex", emit_snippet(relpath, start, end, ext_n))

    print("生成代码片段：")
    code_files = (
        sorted((ROOT / "ch01-introduction" / "examples").glob("*"))
        + sorted((ROOT / "ch02-ontology-foundations" / "examples").glob("*"))
        + sorted((ROOT / "ch03-ontology-methodology" / "examples").glob("*"))
        + sorted((ROOT / "ch04-ontology-languages" / "examples").glob("*"))
        + sorted((ROOT / "ch05-reasoning" / "examples").glob("*"))
        + sorted((ROOT / "ch06-applications" / "examples").glob("*"))
        + sorted((ROOT / "ch07-knowledge-graph" / "examples").glob("*"))
        + sorted((ROOT / "ch08-ontology-llm" / "examples").glob("*"))
        + sorted((ROOT / "ch09-capstone-manufacturing" / "src").glob("*"))
    )
    for fp in code_files:
        tag = fp.parts[-3].split("-")[0]  # chXX
        name = f"code-{tag}-{fp.name.replace('.', '-')}.tex"
        lines = fp.read_text(encoding="utf-8").splitlines()
        write(name, emit_code(lines, fp.suffix.lower()))

    print("生成小节片段：")
    emit_section_fragments(code_files)

    print("完成。")


if __name__ == "__main__":
    main()
