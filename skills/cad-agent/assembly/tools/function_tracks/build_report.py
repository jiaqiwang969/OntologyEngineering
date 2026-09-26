# -*- coding: utf-8 -*-
"""Assemble report.tex for 《印刷机项目资料内容说明》 from report-inputs/*.

Inputs (all optional; missing parts render as 待补充 notes):
  ../report-inputs/inventory/archives.json          archive listings (main agent)
  frag_inventory.tex                                 generated overview tables
  ../report-inputs/nx/{baseji,sise}_summary.md       NX track summaries (markdown)
  ../report-inputs/nx/{baseji,sise}_tree.json        NX track structure data
  ../report-inputs/nx/*.png                          NX window captures
  ../report-inputs/drawings/summary.md, inventory.md, bom/bom_summary.md, coverage.md, dwg/dwg_summary.md
  ../report-inputs/drawings/img/manifest.json + png  drawing renders
Run:  python3 build_report.py && xelatex -interaction=nonstopmode report.tex (twice)
"""
from __future__ import annotations
import json, os, re, html
from pathlib import Path
try:
    import nx_sections, bom_sections
except ImportError:
    nx_sections = bom_sections = None

HERE = Path(__file__).resolve().parent
INP = HERE.parent / "report-inputs"
OUT = HERE / "report.tex"
DATE = "2026 年 9 月 4 日"


# ------------------------------------------------------------------ helpers
def tex(s: str) -> str:
    """Escape plain text for LaTeX (keeps CJK, full-width punctuation)."""
    if s is None:
        return ""
    s = str(s)
    rep = {"\\": r"\textbackslash{}", "&": r"\&", "%": r"\%", "$": r"\$", "#": r"\#", "_": r"\_",
           "{": r"\{", "}": r"\}", "~": r"\textasciitilde{}", "^": r"\textasciicircum{}",
           "″": r"$^{\prime\prime}$", "′": r"$^{\prime}$", "↔": r"$\leftrightarrow$", "→": r"$\rightarrow$", "←": r"$\leftarrow$", "≈": r"$\approx$",
           "≤": r"$\le$", "≥": r"$\ge$", "≠": r"$\ne$", "⏎": r"$\hookleftarrow$", "∝": r"$\propto$", "∞": r"$\infty$",
           "√": r"$\surd$", "∅": "Ø", "℃": "°C", "•": r"\textbullet{}", "✅": "[√]", "❌": "[×]", "⚠️": "[!]", "⚠": "[!]",
           "⬜": "[ ]", "🖥️": "[GUI]", "🖥": "[GUI]", "✔": "[√]", "✓": "[√]", "✗": "[×]", "→": r"$\rightarrow$", "⇒": r"$\Rightarrow$",
           "≈": r"$\approx$", "≡": r"$\equiv$", "⁰": r"$^{0}$", "¹": r"$^{1}$", "⁴": r"$^{4}$", "⁵": r"$^{5}$", "⁶": r"$^{6}$", "⁻": r"$^{-}$", "₂": r"$_{2}$", "∆": r"$\Delta$", "Δ": r"$\Delta$", "π": r"$\pi$", "μ": r"$\mu$", "µ": r"$\mu$"}
    return "".join(rep.get(c, c) for c in s)


def _url(u: str) -> str:
    u = u.strip().rstrip(".,;:)）]】")
    return r"\url{" + u.replace("\\", "/").replace("{", "%7B").replace("}", "%7D").replace(" ", "%20") + "}"


def md_inline(s: str) -> str:
    """Very small markdown inline converter: **bold**, `code`, [text](url), bare URLs; escapes the rest."""
    parts = re.split(r"(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\((?:https?|ftp)://[^)\s]+\)|(?:https?|ftp)://[^\s<>()\]\[（）]+)", s)
    out = []
    for p in parts:
        m = re.fullmatch(r"\[([^\]]+)\]\(((?:https?|ftp)://[^)\s]+)\)", p)
        if m:
            out.append(tex(m.group(1)) + "（" + _url(m.group(2)) + "）")
            continue
        if re.fullmatch(r"(?:https?|ftp)://[^\s]+", p):
            out.append(_url(p))
            continue
        if p.startswith("**") and p.endswith("**"):
            out.append(r"\textbf{" + tex(p[2:-2]) + "}")
        elif p.startswith("`") and p.endswith("`"):
            code = re.sub(r"(\\textbackslash\{\}|/|\\_|-|\.)", r"\1\\allowbreak{}", tex(p[1:-1]))
            out.append(r"\texttt{" + code + "}")
        else:
            out.append(tex(p).replace(", ", ",\\allowbreak{} ").replace("; ", ";\\allowbreak{} ").replace("、", "、\\allowbreak{}").replace("（", "\\allowbreak{}（").replace("-", "-\\allowbreak{}").replace("_", "_\\allowbreak{}"))
    return "".join(out)


def md_table(lines: list[str]) -> str:
    rows = [[c.strip() for c in ln.strip().strip("|").split("|")] for ln in lines]
    rows = [r for r in rows if not all(re.fullmatch(r":?-{2,}:?", c or "---") for c in r)]
    if not rows:
        return ""
    ncol = max(len(r) for r in rows)
    rows = [r + [""] * (ncol - len(r)) for r in rows]
    # column widths: distribute 150mm proportional to max text length, capped
    def w(cell):  # CJK characters are about twice as wide as Latin ones
        return sum(2 if ord(ch) > 0x2E80 else 1 for ch in cell)
    lens = [max(w(r[i]) for r in rows) for i in range(ncol)]
    total = sum(min(l, 60) for l in lens) or 1
    avail = 162 - 1.6 * ncol  # mm, after 2pt column separators
    widths = [max(10, avail * min(l, 60) / total) for l in lens]
    scale = min(1.0, (avail) / sum(widths))
    widths = [x * scale for x in widths]
    spec = "".join(f">{{\\RaggedRight\\arraybackslash}}p{{{w:.0f}mm}}" for w in widths)
    body = []
    for i, r in enumerate(rows):
        cells = " & ".join(md_inline(c) for c in r)
        body.append(cells + r" \\")
        if i == 0:
            body.append(r"\midrule")
    size = "\\scriptsize" if ncol >= 6 else "\\footnotesize"
    return ("\\begin{center}" + size + "\\setlength{\\tabcolsep}{2pt}\\begin{tabular}{@{}" + spec + "@{}}\\toprule\n"
            + "\n".join(body) + "\n\\bottomrule\\end{tabular}\\end{center}\n")


def md_to_tex(md: str, base_level: int = 2) -> str:
    """Convert the subset of markdown the tracks use (#, ##, ###, -, 1., |tables|, ```blocks```) to LaTeX.

    base_level 2 means '#' -> subsection, '##' -> subsubsection, '###' -> paragraph.
    """
    levels = {1: "subsection", 2: "subsubsection", 3: "paragraph", 4: "paragraph"}
    out, i = [], 0
    lines = md.splitlines()
    in_code = False
    code: list[str] = []
    list_stack: list[str] = []

    def close_lists():
        while list_stack:
            out.append("\\end{" + list_stack.pop() + "}")

    while i < len(lines):
        ln = lines[i]
        if ln.strip().startswith("```"):
            if in_code:
                out.append("\\begin{small}\\begin{verbatim}\n" + "\n".join(code) + "\n\\end{verbatim}\\end{small}")
                code, in_code = [], False
            else:
                close_lists(); in_code = True
            i += 1; continue
        if in_code:
            code.append(ln); i += 1; continue
        m = re.match(r"^(#{1,4})\s+(.*)$", ln)
        if m:
            close_lists()
            lvl = len(m.group(1)) + (base_level - 1)
            cmd = levels.get(lvl, "paragraph")
            title = md_inline(m.group(2).strip())
            out.append(f"\\{cmd}{{{title}}}" + ("\\mbox{}\\par" if cmd == "paragraph" else ""))
            i += 1; continue
        if ln.strip().startswith("|"):
            close_lists()
            tbl = []
            while i < len(lines) and lines[i].strip().startswith("|"):
                tbl.append(lines[i]); i += 1
            out.append(md_table(tbl)); continue
        m = re.match(r"^(\s*)([-*•]|\d+[.)])\s+(.*)$", ln)
        if m:
            kind = "enumerate" if m.group(2)[0].isdigit() else "itemize"
            if not list_stack or list_stack[-1] != kind:
                close_lists(); out.append("\\begin{" + kind + "}[nosep,leftmargin=2em]"); list_stack.append(kind)
            out.append("\\item " + md_inline(m.group(3).strip()))
            i += 1; continue
        if not ln.strip():
            close_lists(); out.append(""); i += 1; continue
        close_lists()
        out.append(md_inline(ln.strip()))
        i += 1
    close_lists()
    if in_code:
        out.append("\\begin{small}\\begin{verbatim}\n" + "\n".join(code) + "\n\\end{verbatim}\\end{small}")
    return "\n".join(out) + "\n"


def tex_break(s: str) -> str:
    """tex() plus discretionary break points after - _ . ( for long file names."""
    return re.sub(r"(\\_|-|\.|（|\()", r"\1\\allowbreak{}", tex(s))

HARMONIZE = [
    ("（应为“未完成”，估）", "（字面有歧义，见 BOM 表注）"),
    ("（疑为“未完成”）", "（字面有歧义，见 BOM 表注）"),
    ("（供报告作者使用）", ""),
]


def md_sections(md: str, keep: list[int] | None = None, drop_h1: bool = True) -> str:
    """Keep only the '## N.' sections whose number is in keep; drop the H1 line and '>' blockquotes."""
    for a, b_ in HARMONIZE:
        md = md.replace(a, b_)
    out, cur = [], None
    for ln in md.splitlines():
        if ln.startswith("# ") and drop_h1:
            continue
        if ln.lstrip().startswith(">"):
            continue
        m = re.match(r"^##\s+(\d+)[.．、]?\s*(.*)$", ln)
        if m:
            cur = int(m.group(1))
            ln = "## " + m.group(2)
        if keep is None or cur is None or cur in keep:
            out.append(ln)
    return "\n".join(out) + "\n"


def read(path: Path) -> str | None:
    return path.read_text(encoding="utf-8") if path.exists() else None


def figure(path: Path, caption: str, width: str = "0.92\\textwidth", label: str | None = None) -> str:
    if not path.exists():
        return f"\\par\\noindent\\textit{{（图片缺失：{tex(path.name)}）}}\\par\n"
    lab = f"\\label{{{label}}}" if label else ""
    return (f"\\begin{{figure}}[H]\\centering\\includegraphics[width={width},height=0.36\\textheight,keepaspectratio]"
            f"{{{path.as_posix()}}}\\caption{{{caption}}}{lab}\\end{{figure}}\n")


def pending(what: str) -> str:
    return f"\\par\\noindent\\textcolor{{red!70!black}}{{【待补充：{tex(what)}】}}\\par\n"


SUMMARY = r'''
\section*{摘要}
\addcontentsline{toc}{section}{摘要}
本批资料是上海晨隽金属制品有限公司两台印刷设备的设计与投产文件，共四个压缩包、1\,689 个文件：

\begin{itemize}[nosep,leftmargin=2em]
\item \textbf{八色机总装.rar}（NX 2412 装配，545 个部件文件）：八色\textbf{机组式柔印机}整机模型（各色印刷单元相互独立、沿纸路水平排列），顶层为 8 个 \texttt{SGH650-00} 印刷单元、1 个 \texttt{MQ-00} 模切单元、1 个 \texttt{QZL-00} 前张力总装和 1 个 \texttt{ZD-00} 底座组件；展开后 9\,829 个组件实例、544 个唯一部件，无缺失组件。部件未填写材料、重量等工程属性，重量与材料只能取自图纸。
\item \textbf{八色机投产清单.rar}（213 张 PDF + 12 份 xlsx 明细栏 + 1 个 STEP）：SGH650 印刷单元（91 张）、MQ 模切单元（83 张）、QZL 前张力总装（39 张）的三维出图工程图，A4 打印、带文字层，审定 瞿维国；12 份明细栏共 240 行（自制件 94 行，标准/外购件 146 行）。
\item \textbf{4色组.rar}（NX 2412 装配，533 个部件文件）：四色\textbf{层叠式柔印机} \texttt{4色组印刷机.prt}（色组上下层叠在共用框架/墙板上），25 个顶层组件（4 个色组单元、INFEED、OUTFEED、框架、浮动辊、纠偏、放大镜、安全防护、踏板、电柜、热风灯箱等），2\,354 个组件实例、469 个唯一部件，无缺失组件；219 个部件填有材料、352 个填有重量，属性完整度高。
\item \textbf{4色图纸清单.rar}（217 张 A3 NX 制图 PDF + 162 个 STEP + 5 个 DWG + 1 份 BOM）：YSJ01–YSJ13 十三个部件组的零件图，设计 张顺红，2026-04 至 2026-08；BOM 为 NX 属性导出，13 个分区、198 行。
\end{itemize}

主要发现：八色机是机组式结构，"母单元 SGH650 + 派生单元 MQ/QZL"按模块复用，MQ 与 QZL 直接引用 SGH650 的底辊部套、机架撑档、地脚等零件，加工与采购数量必须按整机汇总；4色组是层叠式的另一台独立机型，与此前的 SW370-576 图纸共用同一 NX 图框模板。需要向对方确认的缺口：SGH650-11-00 色标检测装置无图、网辊本体未出图、MQ/QZL 没有 xlsx 明细（只在装配图上）、11 组同图号多份 PDF（其中三组是同图号两个零件）、八色机整套只有 1 个 STEP；4色组 BOM 缺 YSJ13 热风灯箱分区、52 个图号有图无 BOM 行、55 个图号只有 PDF 没有 STEP。详见第~\ref{sec:cross} 节与第~\ref{sec:conclusion} 节。
\clearpage
'''

CONCL = r'''
\section{结论与建议}\label{sec:conclusion}
\subsection{资料状态一览}
\begin{table}[H]\centering\small\begin{tabular}{@{}>{\RaggedRight\arraybackslash}p{34mm}>{\RaggedRight\arraybackslash}p{58mm}>{\RaggedRight\arraybackslash}p{66mm}@{}}\toprule
资料 & 可直接使用的部分 & 需补齐或确认的部分 \\\midrule
八色机组式柔印机（NX） & 整机结构、装配关系、数量统计、干涉/空间校核 & 部件无材料重量属性；1 个文件名乱码；13 个无几何体的哑元部件 \\
八色机投产清单 & SGH650 11 个部套的图纸与明细栏、MQ/QZL 装配图及零件图（有文字层，可机读） & SGH650-11-00 无图；网辊未出图；MQ/QZL 无 xlsx；11 组同图号多份 PDF；只有 1 个 STEP；A1/A2 图缩印为 A4 \\
4色组层叠式柔印机（NX） & 结构、属性（材料/重量/工艺/表面处理）、装配图纸页 & 6 个无几何体的外购件模型；纸路等辅助对象；整机包络需在 NX 中精确测量 \\
4色图纸清单 & 217 张零件图、162 个 STEP、5 个 DWG、BOM 198 行 & YSJ13 热风灯箱不在 BOM；52 个图号无 BOM 行；55 个图号无 STEP；BOM 签审列为空；无装配图 \\
\bottomrule\end{tabular}\end{table}

\subsection{建议}
\begin{enumerate}[nosep,leftmargin=2em]
\item 向晨隽索取：SGH650-11-00 色标检测装置图纸、网辊（SGH650-05-0003）图纸、MQ 与 QZL 两个单元的 xlsx 明细栏、YSJ1300000 热风灯箱的 BOM 分区，以及 A1/A2 幅面图纸的原始 PDF 或 CAD 源文件。
\item 逐组确认 11 组"同图号多份 PDF"以哪一版为准，尤其是 MQ-01-0001/0002 与 QZL-01-0002 这类同图号对应操作侧、发动侧两个不同零件的情况，避免投产时按同一图号只加工一件。
\item 八色机的加工与采购数量按整机汇总：8 个 SGH650 单元加 MQ、QZL 对 SGH650-02-00 底辊部套、SGH650-01-0006 撑档等共用件的引用要一并计入；本文第~\ref{sec:baseji} 节的实例数可作为核对基准。
\item 八色机 NX 部件缺少材料与重量属性，若需要整机重量或运输/基础载荷，应按图纸材料在 NX 中批量赋材料后重算，或以图纸标题栏重量为准。
\item 4色组投产范围目前只有框架、INFEED、OUTFEED 三个分区填了"投产数量"，其余分区是否投产、热风灯箱是否随机交付，需与对方确认。
\item 后续可用同一套解读流程（附录 B）对更新版资料做增量核对：重新遍历 NX 装配得到实例数，重新解析 BOM 与文件清单，比对差异即可。
\end{enumerate}
'''

# ------------------------------------------------------------------ preamble
PREAMBLE = r"""% !TeX program = xelatex
\documentclass[UTF8,zihao=-4]{ctexart}
\usepackage[a4paper,top=24mm,bottom=22mm,left=22mm,right=22mm,headsep=8mm,footskip=12mm]{geometry}
\usepackage{fontspec}
\setmainfont{texgyretermes-regular.otf}[BoldFont=texgyretermes-bold.otf,ItalicFont=texgyretermes-italic.otf,BoldItalicFont=texgyretermes-bolditalic.otf]
\setsansfont{texgyreheros-regular.otf}[BoldFont=texgyreheros-bold.otf]
\setmonofont{texgyrecursor-regular.otf}[Scale=0.88]
\setCJKmainfont{Songti SC}[FakeStretch=0.94]
\setCJKsansfont{Heiti SC}[FakeStretch=0.94]
\setCJKmonofont{Heiti SC}[FakeStretch=0.94]
\usepackage{array,longtable,tabularx,booktabs,multirow,makecell,multicol}
\usepackage{graphicx,float,caption,xcolor,enumitem,fancyhdr,titlesec,indentfirst,ragged2e}
\PassOptionsToPackage{hyphens}{url}\usepackage[hidelinks]{hyperref}\urlstyle{tt}\def\UrlBreaks{\do\/\do-\do.\do_\do=\do?\do\&\do\%\do\#\do\~\do\a\do\b\do\c\do\d\do\e\do\f\do\g\do\h\do\i\do\j\do\k\do\l\do\m\do\n\do\o\do\p\do\q\do\r\do\s\do\t\do\u\do\v\do\w\do\x\do\y\do\z\do\0\do\1\do\2\do\3\do\4\do\5\do\6\do\7\do\8\do\9}\Urlmuskip=0mu plus 1mu
\setlength{\parindent}{2em}\setlength{\parskip}{2pt}\setlength{\headheight}{15pt}\setlength{\emergencystretch}{2.5em}\hyphenpenalty=50
\setlength{\tabcolsep}{4pt}\renewcommand{\arraystretch}{1.2}
\captionsetup{font=small,labelfont=bf,skip=4pt}
\pagestyle{fancy}\fancyhf{}
\fancyhead[L]{\zihao{-5}印刷机项目资料内容说明}\fancyhead[R]{\zihao{-5}163-曹隽-印刷}
\fancyfoot[C]{\zihao{-5}第 \thepage\ 页}
\renewcommand{\headrulewidth}{0.4pt}\renewcommand{\footrulewidth}{0pt}
\titleformat{\section}{\centering\songti\bfseries\zihao{3}}{\thesection}{0.6em}{}
\titleformat{\subsection}{\songti\bfseries\zihao{4}}{\thesubsection}{0.6em}{}
\titleformat{\subsubsection}{\songti\bfseries\zihao{-4}}{\thesubsubsection}{0.6em}{}
\titleformat{\paragraph}[hang]{\heiti\zihao{-4}}{}{0pt}{}
\titlespacing*{\section}{0pt}{1.4\baselineskip}{1.0\baselineskip}
\titlespacing*{\subsection}{0pt}{0.9\baselineskip}{0.5\baselineskip}
\titlespacing*{\subsubsection}{0pt}{0.7\baselineskip}{0.35\baselineskip}
\titlespacing*{\paragraph}{0pt}{0.6\baselineskip}{0.2\baselineskip}
\setcounter{secnumdepth}{3}\setcounter{tocdepth}{2}
\begin{document}
"""

COVER = r"""
\begin{titlepage}
\centering
\vspace*{28mm}
{\heiti\zihao{1} 印刷机项目资料内容说明\par}
\vspace{6mm}
{\songti\zihao{3} 八色机总装 · 八色机投产清单 · 4色组 · 4色图纸清单\par}
\vspace{14mm}
{\zihao{-3} 基于 Siemens NX 2412 与 AutoCAD 2024 的资料解读\par}
\vspace{40mm}
\begin{tabular}{r@{\hspace{1em}}p{112mm}}
\zihao{-4}项目目录 & \zihao{-4}\texttt{163-曹隽-印刷}\\[3pt]
\zihao{-4}资料来源 & \zihao{-4}曹隽 提供（2026 年 9 月 4 日收到的四个 RAR 压缩包及两张说明截图）\\[3pt]
\zihao{-4}解读工具 & \zihao{-4}NX 2412（NX Open 装配遍历）、AutoCAD 2024（COM 读图）、poppler、openpyxl\\[3pt]
\zihao{-4}整理日期 & \zihao{-4}""" + DATE + r"""\\[3pt]
\zihao{-4}文档性质 & \zihao{-4}资料内容介绍与完整性核对，不构成设计评审结论\\
\end{tabular}
\vfill
{\zihao{5} 本文所有数量、尺寸、材料均取自文件本身或 CAD 软件读数；\\ 无法验证的项目均在文中注明。\par}
\vspace{10mm}
\end{titlepage}
\tableofcontents
\clearpage
""" + SUMMARY

INTRO = r"""
\section{编制说明}
\subsection{目的与范围}
本文介绍 2026 年 9 月 4 日收到的四个压缩包的内容：两套 Siemens NX 2412 三维装配（\textbf{八色机总装}、\textbf{4色组}）和两套二维图纸包（\textbf{八色机投产清单}、\textbf{4色图纸清单}），说明每个包里有什么、如何组织、彼此如何对应，并核对图纸、三维模型与 BOM 之间的覆盖关系。目标读者是需要快速了解这批资料的工程、采购和项目管理人员。

\subsection{资料来源与说明截图}
资料由曹隽通过微信发来，随附两张截图，分别标注了两套 NX 装配的总装文件：\texttt{八色机文件/\allowbreak 八色机总装/\allowbreak 八色机总装.prt}（"装配总文件"，1.89 MB，2026-08-31）和 \texttt{E:/\allowbreak 晨隽/\allowbreak YSJ/\allowbreak 4色组/\allowbreak 4色组印刷机.prt}（"装配总文件"，56 996 KB，2026-08-21）。截图中的路径 \texttt{E:/晨隽/YSJ/} 说明 4色组 的原始工作目录位于晨隽的 YSJ（印刷机）项目下，这与文件编号前缀 YSJ 一致。此前一批资料（精美达670底座、SW370-576 图纸）已另行整理，本文不再重复。

\subsection{解读方法}
\begin{itemize}[nosep,leftmargin=2em]
\item 三维装配：在 Windows 工作站上用 NX 2412 打开总装文件，通过 NX Open 遍历组件树，读取每个部件的属性（件号、名称、材料、重量、供应商等）、实例数、体数量、包围盒，并记录缺失组件；同时在 NX 图形窗口中截取整机视图。
\item 二维图纸：用 poppler 读取 PDF 页数、纸张与元数据并渲染代表性图纸；DWG 文件在 AutoCAD 2024 中打开（只读），读取图层、文字、标注和块，并截取视图。
\item BOM：用 openpyxl 解析全部 xlsx，统一件号后与 PDF、STEP、DWG 的文件名交叉核对。
\item 所有文件均以只读方式处理，未修改、保存或另存任何原始文件。
\end{itemize}

\subsection{约定}
文中"部套"指 NX 中的子装配或 BOM 中的 \texttt{-00} 级分组；"自制件"指有工程件号并配有零件图的加工件，"外购件"指以规格型号或供应商件号命名的采购件。数量单位为件，重量单位为 kg，尺寸单位为 mm。
"""


def section_overview() -> str:
    frag = read(HERE / "frag_inventory.tex") or pending("资料总览表格")
    body = r"""
\section{资料总览}
四个压缩包共 1 689 个文件，解压后约 633 MB。两套 NX 装配各约 540 个部件文件；两套图纸包分别以 PDF 为主，其中 4色图纸清单 还附带 162 个 STEP 三维文件和 5 个 DWG。表~\ref{tab:archives} 至表~\ref{tab:largest} 给出规模、类型、日期与编号系列的统计。
""" + frag + r"""
\subsection{两种机型}
按用户给出的机型术语：八色机是\textbf{机组式柔印机}，各色印刷单元（SGH650-00）相互独立、沿纸路水平依次排列，每个单元自带墙板，由底座组件串联，色数按需求增减单元，模切、张力等功能同样是可插拔的单元；4色组印刷机是\textbf{层叠式柔印机}，四个色组单元上下层叠安装在共用的框架和墙板（YSJ0100000 框架，2000×500×50 墙板）上，进料、出料、烘干等布置在框架两端。这一差别决定了改幅宽时的影响范围：机组式主要改每个单元内部的辊、横向撑档和单元自身墙板间距，再复制单元；层叠式则共用框架的墙板间距、所有横向件和层叠传动一起变。

\subsection{从文件名看到的编号体系}
\begin{itemize}[nosep,leftmargin=2em]
\item \textbf{SGH650-xx-xxxx}：印刷单元（八色机的单色印刷部套），\texttt{SGH650-00} 为印刷单元总装，\texttt{-01} 至 \texttt{-10} 为机架、底辊、版辊、压力调整、网辊传动、刮刀、墨斗等部套，\texttt{-xxxx} 为部套内零件。
\item \textbf{MQ-xx-xxxx}：模切单元（MQ = 模切），\texttt{MQ-00} 为模切单元总装，\texttt{MQ-01} 至 \texttt{MQ-12} 为机架、主电机、上辊、副墙板、底辊、加压、冷却辊、刮刀等部套。
\item \textbf{QZL-xx-xxxx}：前张力总装（\texttt{QZL-00}，部套 01 机架、02 压辊、03 张力辊、04 浮动辊、05 联纸辊）；\textbf{ZD-xx-xxxx}：八色机底座组件（\texttt{ZD-00}/\texttt{ZD-01}，底座 1–3、地脚板、M20 调节螺杆）；\textbf{SGH650-20-xxxx}：钳制器件（本包中仅 1 张图并附唯一的 STEP）。
\item \textbf{YSJ gg ss nnn}：4色组印刷机（YSJ = 印刷机拼音首字母），gg 为部件组（01 框架、02 色组单元、03 INFEED、04 OUTFEED、05 浮动辊单元、06 纠偏装置、07/08 放大镜装置、09/10 安全防护装置、11 踏板、12 电柜装置、13 热风灯箱），ss 为子部件，nnn 为件号，\texttt{000} 结尾为装配。
\item \textbf{8 位数字件号}（如 00138934、22061336\_AF0）和以规格数字开头的文件（如 6205ZZ、63 外管）为外购件或从供应商模型导入的部件，\texttt{\_AF0/\_AF1} 后缀是同一外购件在导入时生成的多个实例副本。
\end{itemize}
"""
    return body


def nx_section(key: str, title: str, label: str, pretty: str) -> str:
    md = read(INP / "nx" / f"{key}_summary.md")
    out = [f"\\subsection{{NX 总装结构解读}}\\label{{{label}}}\n"]
    img = INP / "nx" / f"{key}_overview.png"
    out.append(figure(img, f"{pretty}：NX 2412 中打开的总装视图（在用户桌面 NX 会话中截取）", label=f"fig:{key}"))
    try:
        out.append(nx_sections.nx_block(key, pretty))
    except Exception as e:  # fall back to the track's markdown
        out.append(pending(f"NX 表格生成失败：{e}"))
        if md:
            out.append(md_to_tex(md, base_level=3))
    if md:
        sec8 = md_sections(md, keep=[8])
        sec8 = "\n".join(ln for ln in sec8.splitlines() if not ln.startswith("- ") or ln.startswith("- 未加载") or "##" in ln or True)
        # keep only the section body (drop the leading bullets that repeat the headline numbers)
        body = sec8.split("## ", 1)
        if len(body) == 2:
            out.append(md_to_tex("## " + body[1], base_level=3))
    return "".join(out)


def drawings_block(files, base_level: int = 3) -> str:
    out = []
    for spec in files:
        fname, heading = spec[0], spec[1]
        keep = spec[2] if len(spec) > 2 else None
        cut_at = spec[3] if len(spec) > 3 else None
        md = read(INP / "drawings" / fname)
        if md and cut_at and cut_at in md:
            md = md.split(cut_at, 1)[0]
        if heading:
            out.append(f"\\subsection{{{heading}}}\n")
        if md:
            out.append(md_to_tex(md_sections(md, keep), base_level=base_level))
        else:
            out.append(pending(f"{fname}（图纸取证线尚未返回）"))
    return "".join(out)


def image_gallery(prefix_filter) -> str:
    man = INP / "drawings" / "img" / "manifest.json"
    if not man.exists():
        return pending("图纸缩略图（manifest.json 缺失）")
    items = json.loads(man.read_text(encoding="utf-8"))
    if isinstance(items, dict):
        items = items.get("images", list(items.values()))
    out = []
    for it in items:
        f = it.get("file") or it.get("png") or ""
        if not prefix_filter(f, it):
            continue
        p = INP / "drawings" / "img" / os.path.basename(f) if not os.path.isabs(f) else Path(f)
        cap = it.get("caption") or os.path.basename(f)
        facts = it.get("title_block_facts")
        if isinstance(facts, dict):
            facts = "；".join(f"{k}：{v}" for k, v in facts.items() if v)
        cap_full = tex(cap) + (f"。标题栏：{tex(facts)}" if facts else "")
        out.append(figure(p, cap_full, width="0.9\\textwidth"))
    return "".join(out) or pending("该系列没有渲染图")


def dwg_figures() -> str:
    out = []
    for js in sorted((INP / "drawings" / "dwg").glob("YSJ*.json")):
        if js.name.count(".") > 1:
            continue
        try:
            d = json.loads(js.read_text(encoding="utf-8"))
        except Exception:
            continue
        code = d.get("code", js.stem)
        png = INP / "drawings" / "dwg" / f"{code}.png"
        di = d.get("drawing_info") or {}
        name = (d.get("opened") or {}).get("name") or (di.get("document") or {}).get("name")
        if not name:
            refs = list((INP / "drawings" / "dwg" / "pdf-ref").glob(f"{code}*.png"))
            name = (refs[0].stem + ".dwg") if refs else code
        ents = d.get("model_entity_count") or di.get("model_entity_count") or d.get("entity_total") or "?"
        dims = d.get("dimension_count") or len(d.get("dimension_samples") or []) or "?"
        blocks = d.get("block_count") or di.get("block_definition_count") or len(d.get("blocks") or []) or "?"
        out.append(figure(png, f"AutoCAD 2024 中只读打开的 {tex(name)}：模型空间为 1:1 的 A3 图框，实体 {ents} 个、标注 {dims} 个、块定义 {blocks} 个", width="0.9\\textwidth"))
    return "".join(out)


def appendix_filelists() -> str:
    inv_p = INP / "inventory" / "archives.json"
    if not inv_p.exists():
        return pending("文件清单")
    inv = json.loads(inv_p.read_text(encoding="utf-8"))
    out = ["\\section{附录 A：文件清单}\n本附录按压缩包列出全部文件名（去掉目录前缀），供检索。\n"]
    for rar in ["八色机总装.rar", "八色机投产清单.rar", "4色组.rar", "4色图纸清单.rar"]:
        names = sorted(os.path.basename(f["name"]) for f in inv[rar]["list"])
        out.append(f"\\subsection{{{tex(rar)}（{len(names)} 个文件）}}\n")
        out.append("\\begin{multicols}{3}\\scriptsize\\RaggedRight\\sloppy\\setlength{\\parindent}{0pt}\\setlength{\\emergencystretch}{3em}\n")
        out.append("\n\n".join(tex_break(n) for n in names))
        out.append("\n\\end{multicols}\\normalsize\n")
    return "".join(out)


def appendix_method() -> str:
    return r"""
\section{附录 B：解读工具与可复现步骤}
\begin{itemize}[nosep,leftmargin=2em]
\item NX 装配遍历：NX 2412（Windows 11 工作站 dell-nb），通过 NX Open Python 在 run\_journal 无界面会话中打开总装文件，遍历 \texttt{ComponentAssembly.RootComponent} 的全部子组件，读取部件属性、体与包围盒；调用入口为 cad-agent 技能的 \texttt{scripts/\allowbreak nx\_call.py}（DreamEnding/NX\_MCP 桥）。整机视图在用户桌面的交互式 NX 中以 \texttt{ugs\_router.exe -ug -use\_file\_dir} 打开后截取窗口。
\item AutoCAD 读图：AutoCAD 2024 简体中文版，通过 COM（cad-agent 技能 \texttt{scripts/\allowbreak autocad\_call.py}，acad\_mcp\_server）以只读方式打开 DWG，读取 \texttt{drawing\_info}、\texttt{list\_layers}、\texttt{read\_texts}、\texttt{list\_dimensions}、\texttt{list\_blocks} 并 \texttt{capture\_view} 截图。
\item PDF：poppler 的 pdfinfo/pdftotext/pdftoppm；xlsx：openpyxl；压缩包：macOS bsdtar 与 Windows 7-Zip 22.01（macOS 上的 p7zip 17 不支持这些 RAR5 文件的压缩方法）。
\item 所有中间数据（组件树 JSON、BOM CSV、覆盖核对表、渲染图）保存在整理工作目录 \texttt{report-inputs/} 下，可按需提供。
\end{itemize}
"""


def main() -> None:
    doc = [PREAMBLE, COVER, INTRO, section_overview()]
    # ---- 八色机
    doc.append("\\section{八色机组式柔印机（SGH650 印刷单元 + MQ 模切单元 + QZL 前张力）}\\label{sec:baseji}\n")
    doc.append(nx_section("baseji", "八色机总装", "sec:baseji-nx", "八色机总装"))
    doc.append("\\subsection{投产清单图纸与部套 BOM（八色机投产清单.rar）}\n")
    doc.append(drawings_block([("summary_baseji.md", None, [1, 2, 3, 5, 6])]))
    doc.append(bom_sections.sgh_bom_block())
    doc.append("\\subsection{代表性图纸}\n")
    doc.append(image_gallery(lambda f, it: (it.get("set") or "").startswith("八色") or "SGH650" in f or "MQ-" in f or "QZL" in f))
    # ---- 4色组
    doc.append("\\section{四色层叠式柔印机：4色组印刷机（YSJ 系列）}\\label{sec:sise}\n")
    doc.append(nx_section("sise", "4色组印刷机", "sec:sise-nx", "4色组印刷机"))
    doc.append("\\subsection{零件图、STEP 与 BOM（4色图纸清单.rar）}\n")
    doc.append(drawings_block([("summary_sise.md", None, [1, 2, 4, 5, 6])]))
    doc.append(bom_sections.ysj_bom_block())
    doc.append(drawings_block([("dwg/dwg_summary.md", "5 个 DWG 的 AutoCAD 解读", None, "## 逐份说明")]))
    doc.append(dwg_figures())
    doc.append("\\subsection{代表性图纸}\n")
    doc.append(image_gallery(lambda f, it: (it.get("set") or "").startswith("4色") or "YSJ" in f))
    # ---- cross-cutting
    doc.append("\\section{两套资料的关系、覆盖核对与问题}\\label{sec:cross}\n")
    doc.append("\\subsection{图纸 / 三维 / BOM 覆盖核对}\n")
    doc.append(bom_sections.coverage_block())
    doc.append(drawings_block([("summary.md", "两套图纸包的对照与异常点", [1, 2, 5, 6])]))
    nx_notes = read(INP / "nx" / "notes.md")
    doc.append("\\subsection{NX 取证线的方法与保留意见}\n" + (md_to_tex(nx_notes, 3) if nx_notes else pending("NX notes.md")))
    doc.append(CONCL)
    doc.append(appendix_filelists())
    doc.append(appendix_method())
    doc.append("\\end{document}\n")
    OUT.write_text("".join(doc), encoding="utf-8")
    print("wrote", OUT, OUT.stat().st_size, "bytes")


if __name__ == "__main__":
    main()
