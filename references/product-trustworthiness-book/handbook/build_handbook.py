#!/usr/bin/env python3
"""把《产品可信工程》的 Markdown 内容正本转换为 XeLaTeX 出版快照。

单一数据源原则：20 个正式章节必须各自提供 chapter.md，不允许 README
或其他摘要回退。前言取 front-matter/preface.md（frontmatter 无编号排版），
附录取 appendices/appendix-*.md（`\\appendix` 后按 A–D 自动编号）。本脚本负责
Markdown→LaTeX 的机械转换；handbook 只做排版装配。

用法：python3 handbook/build_handbook.py
输出：handbook/fragments/*.tex + fragments/INDEX.md
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BOOK = HERE.parent
ROOT = BOOK.parent
OUT = HERE / "fragments"
FIGDIR = HERE / "figures-rendered"

# 出版装配只消费这一份显式清单。仓库仍保留旧的
# ``ch11-capstone-three-items`` 作为迁移/审计输入，但它不能再通过
# ``glob("ch*")`` 与现行第 11 章争用同一个 ``readme-ch11.tex``。
CHAPTER_DIRS = (
    "ch01-introduction",
    "ch02-concepts-terminology",
    "ch03-safety-management",
    "ch04-concept-hara",
    "ch05-system-development",
    "ch06-hardware-development",
    "ch07-software-development",
    "ch08-asil-decomposition-dfa",
    "ch09-production-operation",
    "ch10-supporting-processes",
    "ch11-claim-ontology",
    "ch12-identity-ontology",
    "ch13-governance-ontology",
    "ch14-context-hazard-ontology",
    "ch15-requirements-ontology",
    "ch16-measurement-ontology",
    "ch17-change-ontology",
    "ch18-dependency-ontology",
    "ch19-field-ontology",
    "ch20-assurance-ontology",
)

# 章首页用人工语义断行避免把复合词拆开，或在第二行留下单个汉字。
# 键仍是目录、书签与运行页眉使用的纯文本短标题；值只影响章首页显示。
CHAPTER_DISPLAY_BREAKS = {
    "先把词说清：从同名之争到术语体系": ("先把词说清：", "从同名之争到术语体系"),
    "谁有权说“可以相信”：安全管理与安全生命周期": (
        "谁有权说“可以相信”：", "安全管理与安全生命周期",
    ),
    "先决定担心什么：从使用情境到安全目标": ("先决定担心什么：", "从使用情境到安全目标"),
    "把目标接住：技术安全概念与系统层开发": ("把目标接住：", "技术安全概念与系统层开发"),
    "数字要能作证：硬件层开发与硬件度量": ("数字要能作证：", "硬件层开发与硬件度量"),
    "多一条通道，不等于多一分独立：ASIL 分解与相关失效分析": (
        "多一条通道，不等于多一分", "独立：ASIL 分解与相关失效分析",
    ),
    "设计走进工厂之后：生产、运行、服务与报废": (
        "设计走进工厂之后：", "生产、运行、服务与报废",
    ),
    "回到放行桌：支撑过程与安全案例": ("回到放行桌：", "支撑过程与安全案例"),
    "把“可以交付”写成机器能查的一句话：可信主张本体": (
        "把“可以交付”写成", "机器能查的一句话：可信主张本体",
    ),
    "名字背后的身份判据：对象与同一本体": ("名字背后的", "身份判据：对象与同一本体"),
    "把“谁说了算”画进图里：治理与责任本体": (
        "把“谁说了算”", "画进图里：治理与责任本体",
    ),
    "让担心可以被推理：情境与危害本体": ("让担心可以被推理：", "情境与危害本体"),
    "承诺不再丢失：需求与追溯本体": ("承诺不再丢失：", "需求与追溯本体"),
    "带着出处的数字：测量与证据本体": ("带着出处的数字：", "测量与证据本体"),
    "变化有了形状：版本与变化本体": ("变化有了形状：", "版本与变化本体"),
    "看见方框之间的缝：依赖与独立性本体": (
        "看见方框之间的缝：", "依赖与独立性本体",
    ),
    "每一颗件都有自己的历史：制造与现场本体": (
        "每一颗件都有自己的历史：", "制造与现场本体",
    ),
    "活的安全案例：发布与保证本体": ("活的安全案例：", "发布与保证本体"),
}

TEX_SPECIALS = {
    "\\": r"\textbackslash{}", "{": r"\{", "}": r"\}", "$": r"\$",
    "&": r"\&", "#": r"\#", "^": r"\^{}", "_": r"\_", "%": r"\%",
    "~": r"\textasciitilde{}",
}

SYMBOL_MAP = [
    ("p̂", r"\(\hat{p}\)"),
    ("×", r"\(\times\)"), ("→", r"\(\rightarrow\)"), ("←", r"\(\leftarrow\)"),
    ("↔", r"\(\leftrightarrow\)"), ("⇒", r"\(\Rightarrow\)"),
    ("↓", r"\(\downarrow\)"), ("↑", r"\(\uparrow\)"),
    ("≥", r"\(\ge\)"), ("≤", r"\(\le\)"), ("≠", r"\(\ne\)"),
    ("≈", r"\(\approx\)"), ("σ", r"\(\sigma\)"), ("λ", r"\(\lambda\)"),
    ("Σ", r"\(\sum\)"), ("∈", r"\(\in\)"), ("∉", r"\(\notin\)"),
    ("⊇", r"\(\supseteq\)"), ("±", r"\(\pm\)"), ("−", "-"),
    ("÷", r"\(\div\)"), ("√", r"\(\surd\)"), ("°", r"\(^{\circ}\)"),
    ("π", r"\(\pi\)"), ("τ", r"\(\tau\)"), ("α", r"\(\alpha\)"),
    ("β", r"\(\beta\)"), ("γ", r"\(\gamma\)"), ("χ", r"\(\chi\)"),
    ("Δ", r"\(\Delta\)"), ("∨", r"\(\lor\)"), ("∧", r"\(\land\)"),
    ("✓", r"\checkmark{}"), ("✔", r"\checkmark{}"),
    ("△", r"\(\triangle\)"), ("▼", r"\(\blacktriangledown\)"),
    ("▷", r"\(\triangleright\)"), ("…", r"\ldots{}"),
    ("※", r"\(\divideontimes\)"), ("®", r"\textregistered{}"),
    ("─", "-"),
]
SYMBOL_MAP += [
    (chr(0x2460 + i), rf"\textcircled{{\scriptsize {i + 1}}}") for i in range(9)
]

SUPERSCRIPT_CHARS = {
    "⁰": "0", "¹": "1", "²": "2", "³": "3", "⁴": "4", "⁵": "5",
    "⁶": "6", "⁷": "7", "⁸": "8", "⁹": "9", "⁻": "-", "⁺": "+",
}
SUBSCRIPT_CHARS = {
    "₀": "0", "₁": "1", "₂": "2", "₃": "3", "₄": "4", "₅": "5",
    "₆": "6", "₇": "7", "₈": "8", "₉": "9",
}

MATH_IDENTIFIER_RE = re.compile(
    r"Σλ|λ(?:_[A-Za-z]+(?:,[A-Za-z]+)?)?|σ|Σ"
    r"|[⁰¹²³⁴⁵⁶⁷⁸⁹⁻⁺]+|[₀₁₂₃₄₅₆₇₈₉]+"
)


def render_math_identifier(token: str) -> str:
    if token == "Σλ":
        return r"\(\sum \lambda\)"
    if token.startswith("λ_"):
        suffix = token[2:]
        return r"\(\lambda_{\mathrm{" + suffix + r"}}\)"
    if token[0] in SUPERSCRIPT_CHARS:
        return r"\(^{" + "".join(SUPERSCRIPT_CHARS[c] for c in token) + r"}\)"
    if token[0] in SUBSCRIPT_CHARS:
        return r"\(_{" + "".join(SUBSCRIPT_CHARS[c] for c in token) + r"}\)"
    return {"λ": r"\(\lambda\)", "σ": r"\(\sigma\)", "Σ": r"\(\sum\)"}[token]


def esc(s: str) -> str:
    def escape_plain(part: str) -> str:
        escaped = "".join(TEX_SPECIALS.get(c, c) for c in part)
        for a, b in SYMBOL_MAP:
            escaped = escaped.replace(a, b)
        return escaped

    out: list[str] = []
    cursor = 0
    for match in MATH_IDENTIFIER_RE.finditer(s):
        out.append(escape_plain(s[cursor:match.start()]))
        out.append(render_math_identifier(match.group(0)))
        cursor = match.end()
    out.append(escape_plain(s[cursor:]))
    return "".join(out)


def typographic_quotes(s: str) -> str:
    """将正文中成对 ASCII 双引号转为中文出版引号。"""
    return re.sub(r'"([^"\n]+)"', r'“\1”', s)


def breakable_code(code: str) -> str:
    """排版行内代码，并为路径、复合词和长标识符提供断行点。"""
    if (
        code.isascii()
        and not any(char.isspace() for char in code)
        and "{" not in code
        and "}" not in code
    ):
        # xurl 为路径、CamelCase 和长标识符提供稳定断行；
        # nolinkurl 仅排版原文，不生成虚假超链接。
        return r"\nolinkurl{" + code + "}"

    chunks: list[str] = []
    for index, char in enumerate(code):
        if index and char.isupper() and (code[index - 1].islower() or code[index - 1].isdigit()):
            chunks.append(r"\allowbreak{}")
        chunks.append(esc(char))
        if char in "/._-,:;=+\\ ":
            chunks.append(r"\allowbreak{}")
    return r"\allowbreak{}\texttt{" + "".join(chunks) + r"}\allowbreak{}"


def inline(s: str) -> str:
    """行内 Markdown：`code`、**bold**、链接降级为文字。"""
    parts = re.split(r"(`[^`]*`)", s)
    buf = []
    for p in parts:
        if p.startswith("`") and p.endswith("`") and len(p) >= 2:
            code = p[1:-1]
            buf.append(breakable_code(code))
        else:
            p = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", p)
            p = typographic_quotes(p)
            p = esc(p)
            p = re.sub(r"\\\*\\\*(.+?)\\\*\\\*", r"\\textbf{\1}", p)  # 转义后的 **
            p = p.replace(r"\*\*", "")  # 兜底
            buf.append(p)
    # esc 会把 ** 原样保留（* 非特殊字符），直接处理粗体
    s2 = "".join(buf)
    s2 = re.sub(r"\*\*(.+?)\*\*", r"\\textbf{\1}", s2)
    return s2


def verbatim_line(s: str) -> str:
    """Keep code literal while replacing glyphs unavailable in the mono font."""
    return s.replace("▷", ">").replace("※", "*")


LONG_CAMEL_RE = re.compile(r"[A-Za-z][A-Za-z\[\]\d]{13,}")


def camel_breaks(text: str) -> str:
    """给窄列里的超长 CamelCase 词补大小写交界断行点。"""
    def brk(m: re.Match[str]) -> str:
        return re.sub(r"(?<=[a-z])(?=[A-Z])", r"\\allowbreak{}", m.group(0))

    return LONG_CAMEL_RE.sub(brk, text)


def cell_inline(s: str) -> str:
    """表格单元格排版：在数学记号连写、下划线与长驼峰词处补断行点。"""
    t = inline(s)
    t = t.replace(r"\)/\(", r"\)/\allowbreak\(")
    t = t.replace(r"\)+\(", r"\)+\allowbreak\(")
    t = t.replace(r"\)(", r"\)\allowbreak(")
    t = t.replace(r"\_", r"\_\allowbreak{}")
    # 只在花括号深度 0 处理，避免改写 \nolinkurl{...} 等命令参数。
    out: list[str] = []
    plain: list[str] = []
    depth = 0
    for ch in t:
        if ch == "{":
            if depth == 0:
                out.append(camel_breaks("".join(plain)))
                plain = []
            depth += 1
            out.append(ch)
        elif ch == "}":
            depth -= 1
            out.append(ch)
        elif depth == 0:
            plain.append(ch)
        else:
            out.append(ch)
    out.append(camel_breaks("".join(plain)))
    return "".join(out)


def md_table_to_tex(lines: list[str]) -> str:
    rows = []
    for ln in lines:
        cells = [c.strip() for c in ln.strip().strip("|").split("|")]
        rows.append(cells)
    if len(rows) >= 2 and all(re.fullmatch(r":?-{2,}:?", c) for c in rows[1]):
        header, body = rows[0], rows[2:]
    else:
        header, body = rows[0], rows[1:]
    ncol = max(len(r) for r in rows)
    tail_cells = [re.sub(r"[`*]", "", cell) for row in [header, *body] for cell in row[1:]]
    compact_tail = (
        ncol >= 5
        and tail_cells
        and max(map(len, tail_cells)) <= 12
        and all(not re.search(r"[\u3400-\u9fff]", cell) for cell in tail_cells)
    )
    if compact_tail:
        first_factor = 2.5
        other_factor = (ncol - first_factor) / (ncol - 1)
        first = rf">{{\raggedright\arraybackslash\hsize={first_factor:g}\hsize\linewidth=\hsize}}X"
        other = rf">{{\centering\arraybackslash\hsize={other_factor:g}\hsize\linewidth=\hsize}}X"
        colspec = first + other * (ncol - 1)
    else:
        colspec = "X" * ncol
    # 超长表用 xltabular（tabularx 列宽 + longtable 跨页），避免整表超出版心。
    # 不能只按“行数”判断：附录 A 的 DFI 表只有六行，但每格含多条机制，
    # 旧逻辑把整表塞进一个不可分页的 tabularx，曾把末行裁出页面。
    cell_lengths = [len(cell) for row in body for cell in row]
    long_table = (
        len(body) >= 10
        # Four wide prose columns become taller than a page much sooner than
        # their raw character count suggests.  The DFI synopsis is the
        # canary: six rows / 912 characters already exceed A4 in tabularx.
        or sum(cell_lengths) >= 700
        or (cell_lengths and max(cell_lengths) >= 180)
    )
    env = "xltabular" if long_table else "tabularx"
    out = [] if long_table else [r"\begin{center}"]
    out += [
        r"\begingroup\small",
        r"\setlength{\tabcolsep}{4pt}",
        r"\begin{" + env + r"}{\linewidth}{@{}" + colspec + r"@{}}",
        r"\toprule",
    ]
    out.append(" & ".join(cell_inline(c) for c in header + [""] * (ncol - len(header))) + r" \\")
    out.append(r"\midrule")
    if long_table:
        out.append(r"\endhead")
    for r in body:
        out.append(" & ".join(cell_inline(c) for c in r + [""] * (ncol - len(r))) + r" \\")
    out += [r"\bottomrule", r"\end{" + env + r"}", r"\endgroup"]
    if not long_table:
        out.append(r"\end{center}")
    return "\n".join(out)


# 正文图槽两行标记：`<!-- FIG: <id> <slug> -->` + `> 图 X-M（占位）：图题`。
FIG_SLOT_RE = re.compile(r"^<!--\s*FIG:\s*(\S+)\s+(\S+)\s*-->$")
FIG_PLACEHOLDER_RE = re.compile(
    r"^>\s*图\s*\S+\s*[（(]\s*占位\s*[）)]\s*[：:]\s*(?P<caption>.+)$"
)
MARKDOWN_IMAGE_RE = re.compile(
    r'^!\[(?P<alt>[^\]]*)\]\('
    r'(?P<target><[^>]+>|[^\s)]+)'
    r'(?:\s+"(?P<title>[^"]*)")?\s*\)$'
)
# 极宽图横排阈值：缩进版心（约 449pt 宽）后文字将小于原大的 34%（约 3.6pt），
# 且宽高比足够大、横排能被整页利用时，改用 sidewaysfigure 收纳。
SIDEWAYS_MIN_WIDTH_PT = 1320.0
SIDEWAYS_MIN_ASPECT = 2.0
# 成书版心约 449pt 宽、图内最小字通常为 11--14px。宽图缩小后若低于
# 约 7.5pt，未裁切也已失去教材可读性；精确门限由下方的最小字号计算补充。
PORTRAIT_TEXT_WIDTH_PT = 449.0
PORTRAIT_TEXT_HEIGHT_PT = 650.0
MIN_EFFECTIVE_FONT_PT = 7.5


def load_figure_dims(fig_dir: Path) -> dict[str, tuple[float, float, float | None]]:
    """读取渲染清单里的物理尺寸（pt）；缺失或损坏时返回空表（不横排）。"""
    manifest = fig_dir / ".render-manifest.json"
    if not manifest.exists():
        return {}
    try:
        data = json.loads(manifest.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}
    dims: dict[str, tuple[float, float, float | None]] = {}
    if isinstance(data, dict):
        for stem, entry in data.items():
            if isinstance(entry, dict) and "width_pt" in entry and "height_pt" in entry:
                font = entry.get("min_font_pt")
                dims[stem] = (
                    float(entry["width_pt"]),
                    float(entry["height_pt"]),
                    float(font) if font is not None else None,
                )
    return dims


def figure_slot_tex(fig_id: str, stem: str, caption: str,
                    dims: tuple[float, float, float | None] | None) -> str:
    """图槽 -> LaTeX figure 环境；宽度自适应、\\linewidth 封顶，极宽图横排。"""
    portrait_scale = (
        min(
            1.0,
            PORTRAIT_TEXT_WIDTH_PT / dims[0],
            0.78 * PORTRAIT_TEXT_HEIGHT_PT / dims[1],
        )
        if dims is not None else 1.0
    )
    unreadable_portrait = (
        dims is not None
        and dims[2] is not None
        and dims[2] * portrait_scale < MIN_EFFECTIVE_FONT_PT
        and dims[0] > dims[1]
    )
    sideways = (
        dims is not None
        and (
            (
                dims[0] >= SIDEWAYS_MIN_WIDTH_PT
                and dims[0] >= SIDEWAYS_MIN_ASPECT * dims[1]
            )
            or unreadable_portrait
        )
    )
    env = "sidewaysfigure" if sideways else "figure"
    # 竖排高度上限 0.78\textheight：为最长图题（约 9 行 small）留足同页余量，
    # 实测 0.82 时最高图 ch03-fig1 溢出 8.3pt（Float too large）。
    size = (
        r"max size={0.92\textheight}{0.85\textwidth}" if sideways
        else r"max size={\linewidth}{0.78\textheight}"
    )
    lines = [
        rf"\begin{{{env}}}" + ("" if sideways else "[htbp]"),
        r"\centering",
        rf"\adjustbox{{{size}}}{{\includegraphics{{figures-rendered/{stem}.pdf}}}}",
    ]
    number = re.search(r"fig(\d+)$", fig_id)
    if number:
        # 图号锚定在槽位 id 上（ch04-fig3 -> 图 4-3），与正文占位编号一致，
        # 即便个别图渲染缺失回退占位，后续图号也不漂移。
        lines.append(rf"\setcounter{{figure}}{{{int(number.group(1)) - 1}}}")
    lines.append(r"\caption{" + inline(caption) + "}")
    lines.append(rf"\end{{{env}}}")
    return "\n".join(lines)


def markdown_image_tex(path: str, caption: str) -> str:
    """Render a project-local Markdown image in place, with an inseparable caption."""
    return "\n".join([
        r"\begin{center}",
        r"\begin{minipage}{\linewidth}",
        r"\centering",
        r"\adjustbox{max size={\linewidth}{0.78\textheight}}"
        rf"{{\includegraphics{{{path}}}}}",
        r"\captionof{figure}{" + inline(caption) + "}",
        r"\end{minipage}",
        r"\end{center}",
    ])


def chapter_art_tex(path: str) -> str:
    """Render an unnumbered chapter-opening illustration.

    Chapter art is editorial navigation, not a numbered teaching figure.  Its
    physical size stays at the established book setting; the layout macro only
    feathers the four edges into paper white and does not consume the chapter's
    ``figure`` counter.
    """

    return "\n".join([
        r"\begin{center}",
        r"\vspace{0.2em}",
        r"\chapterartfade{%",
        r"\adjustbox{max size={0.88\linewidth}{0.27\textheight}}"
        rf"{{\includegraphics{{{path}}}}}%",
        r"}",
        r"\vspace{0.35em}",
        r"\end{center}",
    ])


def resolve_markdown_image(target: str, source_path: Path | None) -> tuple[Path, str] | None:
    """Resolve an image against its Markdown source and return a handbook-relative path."""
    raw = target[1:-1] if target.startswith("<") and target.endswith(">") else target
    if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*://", raw):
        return None
    base = source_path.parent if source_path is not None else ROOT
    resolved = (base / raw).resolve() if not Path(raw).is_absolute() else Path(raw).resolve()
    if not resolved.is_file() or resolved.suffix.casefold() not in {".png", ".jpg", ".jpeg", ".pdf"}:
        return None
    include_path = Path(os.path.relpath(resolved, HERE.resolve())).as_posix()
    return resolved, include_path


def md_to_tex(md: str, *, chapter_from_h1: bool, starred_sections: bool = False,
              figures_dir: Path | None = None, source_path: Path | None = None) -> str:
    fig_dir = FIGDIR if figures_dir is None else figures_dir
    fig_dims = load_figure_dims(fig_dir)
    # 构建层剥离 HTML 注释。只保留旧式 PlantUML 图槽；新版 OBSERVE/CONSUME
    # 标记属于源文语义合同，不应泄漏到 LaTeX/PDF。
    def keep_legacy_slot(match: re.Match[str]) -> str:
        comment = match.group(0).strip()
        return comment if FIG_SLOT_RE.fullmatch(comment) else ""

    md = re.sub(r"<!--.*?-->", keep_legacy_slot, md, flags=re.S)
    lines = md.splitlines()
    out: list[str] = []
    i = 0
    in_code = False
    in_quote = False
    in_list: str | None = None

    def close_list():
        nonlocal in_list
        if in_list:
            out.append(r"\end{" + in_list + "}")
            in_list = None

    while i < len(lines):
        ln = lines[i]
        if ln.strip().startswith("```"):
            close_list()
            if not in_code:
                out.append(r"\nopagebreak[4]")
                out.append(r"\begin{codebox}")
                out.append(
                    r"\begin{Verbatim}[fontsize=\small,breaklines=true,"
                    r"breakanywhere=true,breaksymbolleft={},breaksymbolright={}]"
                )
            else:
                out.append(r"\end{Verbatim}")
                out.append(r"\end{codebox}")
            in_code = not in_code
            i += 1
            continue
        if in_code:
            out.append(verbatim_line(ln))
            i += 1
            continue
        image_match = MARKDOWN_IMAGE_RE.match(ln.strip())
        if image_match:
            close_list()
            resolved = resolve_markdown_image(image_match.group("target"), source_path)
            caption = image_match.group("title") or image_match.group("alt")
            if resolved is None:
                print(
                    f"警告：Markdown 图片无法解析，保留替代文字：{image_match.group('target')}",
                    file=sys.stderr,
                )
                out.append(r"\begin{noteblock}" + inline(caption) + r"\end{noteblock}")
            else:
                _, include_path = resolved
                if image_match.group("title") == "chapter-art":
                    out.append(chapter_art_tex(include_path))
                else:
                    out.append(markdown_image_tex(include_path, caption))
            i += 1
            continue
        slot = FIG_SLOT_RE.match(ln.strip())
        if slot:
            fig_id, slug = slot.group(1), slot.group(2)
            stem = f"{fig_id}-{slug}"
            placeholder = (
                FIG_PLACEHOLDER_RE.match(lines[i + 1].strip())
                if i + 1 < len(lines) else None
            )
            if placeholder and (fig_dir / f"{stem}.pdf").exists():
                close_list()
                out.append(figure_slot_tex(
                    fig_id, stem, placeholder.group("caption"), fig_dims.get(stem),
                ))
                i += 2
                continue
            if placeholder:
                print(f"警告：图槽 {stem} 渲染文件缺失，保留占位行", file=sys.stderr)
            else:
                print(f"警告：图槽 {stem} 占位行缺失，跳过标记", file=sys.stderr)
            i += 1  # 丢弃标记行；占位行（如有）走常规引用块排版
            continue
        if ln.startswith("|") and i + 1 < len(lines) and lines[i + 1].lstrip().startswith("|"):
            close_list()
            tbl = []
            while i < len(lines) and lines[i].lstrip().startswith("|"):
                tbl.append(lines[i]); i += 1
            out.append(md_table_to_tex(tbl))
            continue
        if ln == ">" or ln.startswith("> "):
            close_list()
            if not in_quote:
                out.append(r"\begin{noteblock}")
                in_quote = True
            # CommonMark 允许用单独的 `>` 作为引用块内的空行。
            # 这种结构不应在成书中泄漏成一枚孤立的大于号。
            content = ln[1:].lstrip()
            out.append(inline(content) if content else "")
            i += 1
            if i >= len(lines) or not lines[i].startswith(">"):
                out.append(r"\end{noteblock}")
                in_quote = False
            continue
        if ln.strip() in {"---", "***", "___"}:
            close_list()
            i += 1
            continue
        m = re.match(r"^(#{1,4})\s+(.*)$", ln)
        if m:
            close_list()
            level, title = len(m.group(1)), m.group(2)
            title = re.sub(r"\s*/\s*Chapter.*$", "", title)  # 去英文副题
            star = "*" if starred_sections else ""
            if level >= 2:
                # Markdown 独立阅读时保留显式节号（含附录的 A.1/C.0 式）；
                # LaTeX 会自动编号，生成时去重。
                title = re.sub(r"^(?:[A-Za-z]\.)?\d+(?:\.\d+)*[.)、]?\s+", "", title)
            if level == 1 and chapter_from_h1:
                title = re.sub(r"^第\s*\d+\s*章(?:[：:]\s*|\s+)", "", title)
                title = re.sub(r"^附录\s*[A-Za-z](?:[：:]\s*|\s+)", "", title)
                if title in CHAPTER_DISPLAY_BREAKS:
                    first, second = CHAPTER_DISPLAY_BREAKS[title]
                    out.append(
                        r"\chapter[" + inline(title) + "]{"
                        + inline(first) + r"\\" + inline(second) + "}"
                    )
                else:
                    out.append(r"\chapter{" + inline(title) + "}")
            elif level <= 2:
                # 节标题与至少数行正文留在同一页，避免标题落在页底、
                # 下方只剩一行的阅读断裂。
                out.append(r"\Needspace{6\baselineskip}")
                out.append(r"\section" + star + "{" + inline(title) + "}")
            else:
                out.append(r"\Needspace{5\baselineskip}")
                out.append(r"\subsection" + star + "{" + inline(title) + "}")
            i += 1
            continue
        if re.match(r"^\s*[-*]\s+", ln):
            if in_list != "itemize":
                close_list()
                out.append(r"\begin{itemize}")
                in_list = "itemize"
            out.append(r"\item " + inline(re.sub(r"^\s*[-*]\s+", "", ln)))
            i += 1
            continue
        if re.match(r"^\s*\d+\.\s+", ln):
            if in_list != "enumerate":
                close_list()
                out.append(r"\begin{enumerate}")
                in_list = "enumerate"
            out.append(r"\item " + inline(re.sub(r"^\s*\d+\.\s+", "", ln)))
            i += 1
            continue
        if not ln.strip():
            close_list()
            out.append("")
            i += 1
            continue
        # 把代码块的空间需求放在它的引导句之前。这样引导句不会孤零零留在
        # 页底；无论中间有无空行，都只生成一次 Needspace。
        next_content = i + 1
        while next_content < len(lines) and not lines[next_content].strip():
            next_content += 1
        if next_content < len(lines) and lines[next_content].strip().startswith("```"):
            out.append(r"\Needspace{8\baselineskip}")
        out.append(inline(ln))
        i += 1
    close_list()
    return "\n".join(out)


def chapter_source(chdir: Path) -> Path:
    """Return the sole chapter prose source, failing closed when it is absent."""
    chapter = chdir / "chapter.md"
    if not chapter.is_file():
        raise FileNotFoundError(f"正式章节缺少唯一内容正本 chapter.md：{chdir}")
    return chapter


def appendix_tex(source: Path) -> str:
    """附录片段：`\\appendix` 后由 LaTeX 自动编号为 A/B/C/…。"""
    md = source.read_text(encoding="utf-8")
    tex = md_to_tex(md, chapter_from_h1=True, source_path=source)
    first_numbered = re.search(r"^##\s+[A-Za-z]\.(\d+)", md, flags=re.M)
    if first_numbered and first_numbered.group(1) == "0":
        # 源文首节编号 X.0（如术语表 C.0）：让 LaTeX 编号与源文对齐。
        tex = re.sub(
            r"(\\chapter\{[^\n]*\})\n",
            r"\1\n\\setcounter{section}{-1}\n",
            tex,
            count=1,
        )
    return tex


def expected_outputs() -> dict[str, str]:
    """Render every generated fragment without mutating the workspace."""
    index = ["# fragments index（自动生成，勿手改）", ""]
    outputs: dict[str, str] = {}
    preface = BOOK / "front-matter" / "preface.md"
    if preface.exists():
        tex = md_to_tex(
            preface.read_text(encoding="utf-8"),
            chapter_from_h1=True,
            starred_sections=True,  # frontmatter 章无编号，节亦不编号
            source_path=preface,
        )
        outputs["preface.tex"] = tex + "\n"
        index.append(f"- preface.tex <- {preface.relative_to(ROOT)}")
    for dirname in CHAPTER_DIRS:
        chdir = BOOK / dirname
        source = chapter_source(chdir)
        tex = md_to_tex(
            source.read_text(encoding="utf-8"),
            chapter_from_h1=True,
            source_path=source,
        )
        name = f"readme-{chdir.name.split('-')[0]}.tex"
        outputs[name] = tex + "\n"
        index.append(f"- {name} <- {source.relative_to(ROOT)}")
    for source in sorted((BOOK / "appendices").glob("appendix-*.md")):
        letter = source.stem.split("-")[1]
        name = f"appendix-{letter}.tex"
        outputs[name] = appendix_tex(source) + "\n"
        index.append(f"- {name} <- {source.relative_to(ROOT)}")
    outputs["INDEX.md"] = "\n".join(index) + "\n"
    return outputs


def write_outputs(out_dir: Path = OUT) -> None:
    """Write a deterministic TeX snapshot to *out_dir*."""
    outputs = expected_outputs()
    out_dir.mkdir(parents=True, exist_ok=True)
    for name, content in outputs.items():
        (out_dir / name).write_text(content, encoding="utf-8")
    print(f"fragments 已生成：{len(outputs) - 1} 个片段（前言/章/附录）-> {out_dir}")


def main() -> None:
    write_outputs()


if __name__ == "__main__":
    main()
