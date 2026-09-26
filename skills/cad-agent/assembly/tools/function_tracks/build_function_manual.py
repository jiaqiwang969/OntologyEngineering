# -*- coding: utf-8 -*-
"""Build 《OLSK Large CNC V3 组件功能属性手册》(manual.tex) from the function tracks A..H + order audit.

Inputs: 功能属性/tracks/<T>/function.json, order_constraints.json, notes.md, sources.md
        功能属性/order-audit.v1.json (tools/audit_sequence_constraints.py)
Run:    .venv/bin/python 功能属性/build/build_olsk_manual.py && cd 功能属性/build && xelatex manual.tex (twice)
"""
from __future__ import annotations
import json, re, datetime
from pathlib import Path
import build_report as br

HERE = Path(__file__).resolve().parent
CASE = HERE.parent.parent
TRACKS = CASE / "功能属性/tracks"
OUT = HERE / "manual.tex"
DATE = datetime.date.today().strftime("%Y 年 %m 月 %d 日")
tex = br.tex


def texb(s):
    return re.sub(r"(\\_|-|\.|（|\(|/)", r"\1\\allowbreak{}", tex(str(s)))


def fit_spec(spec, total_mm=158.0):
    pw = [float(x) for x in re.findall(r"p\{([0-9.]+)mm\}", spec)]
    rest = re.sub(r">\{[^}]*\}p\{[0-9.]+mm\}|p\{[0-9.]+mm\}", "", spec)
    n_lrc = len(re.findall(r"[lrc]", rest))
    ncol = len(pw) + n_lrc
    avail = total_mm - 2.2 * ncol - 13.0 * n_lrc
    s = sum(pw)
    if s > 0 and s > avail:
        f = avail / s
        spec = re.sub(r"p\{([0-9.]+)mm\}", lambda m: f"p{{{float(m.group(1)) * f:.1f}mm}}", spec)
    return spec


def longtable(spec, header, rows, caption=None):
    spec = fit_spec(spec)
    out = ["\\begin{longtable}{" + spec + "}"]
    if caption:
        out.append("\\caption{" + tex(caption) + "}\\\\")
    out.append("\\toprule " + " & ".join("\\textbf{" + tex(h) + "}" for h in header) + "\\\\ \\midrule \\endfirsthead")
    out.append("\\toprule " + " & ".join("\\textbf{" + tex(h) + "}" for h in header) + "\\\\ \\midrule \\endhead")
    out.append("\\bottomrule \\endfoot")
    for r in rows:
        out.append(" & ".join(texb(c) if c is not None else "" for c in r) + "\\\\")
    out.append("\\end{longtable}")
    return "\n".join(out)


def load_track(t):
    d = TRACKS / t
    fj = d / "function.json"
    data = None
    if fj.exists():
        try:
            data = json.loads(fj.read_text(encoding="utf-8"))
        except Exception as e:  # noqa
            data = {"_error": str(e)}
    notes = (d / "notes.md").read_text(encoding="utf-8") if (d / "notes.md").exists() else None
    sources = (d / "sources.md").read_text(encoding="utf-8") if (d / "sources.md").exists() else None
    return data, notes, sources


def safe_md(md: str) -> str:
    r"""把 notes/sources 里会炸 LaTeX 的字符先转义：行首以外的 # → \#，$ → \$（代码行内不动）。"""
    out = []
    for line in md.splitlines():
        if line.startswith("#"):
            k = len(line) - len(line.lstrip("#"))
            out.append(line[:k] + line[k:].replace("#", "\\#").replace("$", "\\$"))
        elif line.strip().startswith("```") or line.startswith("    "):
            out.append(line)
        else:
            out.append(line.replace("#", "\\#").replace("$", "\\$"))
    return "\n".join(out)


def lst(x):
    if x is None:
        return []
    if isinstance(x, list):
        return x
    return [x]


def unit_section(u):
    out = []
    out.append("\\subsection{" + tex(f"{u.get('unit_id', '')} {u.get('name', '')}") + "}")
    if u.get("function"):
        out.append("\\paragraph{功能}" + tex(u["function"]))
    if u.get("principle"):
        out.append("\\paragraph{工作原理}" + tex(u["principle"]))
    if u.get("workbook_ops"):
        out.append("\\paragraph{手册步骤}" + tex("、".join(map(str, u["workbook_ops"]))))
    subs = lst(u.get("subassemblies"))
    if subs:
        rows = []
        for s in subs:
            im = s.get("install_method") or {}
            im_s = "；".join(f"{k}：{v}" for k, v in (("紧固", im.get("fasteners")), ("方向", im.get("direction")), ("工具", im.get("tools")), ("装后调整", im.get("adjustment_after_mount"))) if v)
            rows.append([s.get("code", ""), s.get("name", ""), s.get("function", ""), im_s, "、".join(map(str, lst(s.get("key_parts"))[:6]))])
        out.append(longtable("p{22mm}p{22mm}p{40mm}p{48mm}p{34mm}", ["步骤", "部套", "功能", "安装方式", "关键件"], rows, "部套构成与安装方式"))
        crow = []
        for s in subs:
            for c in lst(s.get("sequence_constraints")):
                crow.append([s.get("code", "")[:12], c.get("kind", ""), c.get("target", ""), c.get("why", ""), c.get("source", ""), c.get("confidence", "")])
        if crow:
            out.append(longtable("p{20mm}p{22mm}p{34mm}p{56mm}p{14mm}p{10mm}", ["步骤", "约束", "对象", "理由", "来源", "置信"], crow, "顺序约束"))
        obs = [(s.get("code", ""), s.get("film_v009_observations")) for s in subs if s.get("film_v009_observations")]
        if obs:
            out.append("\\paragraph{影片 v009 对照}")
            out.append("\\begin{itemize}[leftmargin=1.6em,itemsep=1pt]")
            for code, o in obs:
                out.append("\\item " + tex(f"{code}：{o}"))
            out.append("\\end{itemize}")
    kp = lst(u.get("key_parameters"))
    if kp:
        out.append(longtable("p{34mm}p{50mm}p{30mm}p{40mm}", ["参数", "数值", "来源", "备注"],
                             [[p.get("name", ""), p.get("value", ""), p.get("source", ""), p.get("note", "")] for p in kp], "关键参数"))
    dr = lst(u.get("design_rules"))
    if dr:
        out.append(longtable("p{80mm}p{34mm}p{12mm}p{30mm}", ["规则", "依据", "置信", "适用"],
                             [[r.get("rule", ""), r.get("basis", ""), r.get("confidence", ""), "、".join(map(str, lst(r.get("applies_to"))))] for r in dr], "设计规则"))
    cov = u.get("coverage") or {}
    if cov:
        miss = lst(cov.get("unbound_or_not_animated"))
        out.append("\\paragraph{影片覆盖}" + tex(f"绑定进影片的动件 {cov.get('bound_movers_in_film', '?')}；未绑定/未动画 {len(miss)} 件" + (f"：{'、'.join(map(str, miss[:12]))}{'…' if len(miss) > 12 else ''}" if miss else "") + (f"。{cov.get('comment')}" if cov.get("comment") else "")))
    for key, title in (("functional_dependencies", "功能依赖"), ("data_gaps", "资料缺口"), ("open_questions", "待确认问题")):
        items = lst(u.get(key))
        if items:
            out.append("\\paragraph{" + title + "}")
            out.append("\\begin{itemize}[leftmargin=1.6em,itemsep=1pt]")
            for it in items:
                out.append("\\item " + tex(str(it)))
            out.append("\\end{itemize}")
    return "\n".join(out)


def track_section(t, data, notes, sources):
    title = (data or {}).get("title") or t
    out = ["\\clearpage", "\\section{" + tex(f"分析线 {t}：{title}") + "}"]
    if not data or data.get("_error"):
        out.append(br.pending(f"分析线 {t} 的 function.json 缺失或无法解析（{(data or {}).get('_error', '缺文件')}）"))
    else:
        for u in lst(data.get("units")):
            out.append(unit_section(u))
        roles = lst(data.get("role_definitions"))
        if roles:
            out.append(longtable("p{30mm}p{70mm}p{58mm}", ["角色", "定义", "典型安装规则"],
                                 [[r.get("role", ""), r.get("definition_zh", ""), r.get("typical_install_rule", "")] for r in roles], "功能角色定义"))
    if notes:
        out.append("\\subsection{叙述与疑点（分析线笔记）}")
        out.append(br.md_to_tex(br.md_sections(notes, None, drop_h1=True), base_level=3))
    return "\n".join(out)


def audit_section():
    p = CASE / "功能属性/order-audit.v1.json"
    if not p.exists():
        return "\\section{顺序约束审计}" + br.pending("order-audit.v1.json 未生成")
    d = json.loads(p.read_text(encoding="utf-8"))
    s = d["summary"]
    out = ["\\clearpage", "\\section{顺序约束审计（对照影片 " + tex(d.get("run", "")) + "）}",
           tex(f"约束 {s['constraints']} 条：违反 {s['violated']}（高置信 {s['violated_high_conf']}）、满足 {s['satisfied']}、未解析 {s['unresolved']}、仅记录 {s['info']}。判定只比较影片章节顺序（= 手册步序）与台架分组，不读几何。")]
    rows = [[r["track"], r["kind"], r.get("from") or "", r.get("to") or "", r["verdict"], r.get("confidence") or "", r.get("source") or "", (r.get("why") or "")[:160]]
            for r in d["rows"] if r["verdict"] in ("VIOLATED", "UNRESOLVED")]
    if rows:
        out.append(longtable("p{6mm}p{20mm}p{14mm}p{14mm}p{16mm}p{8mm}p{12mm}p{62mm}", ["线", "类型", "from", "to", "判定", "置信", "来源", "理由"], rows, "违反与未解析的约束"))
    return "\n".join(out)


PREAMBLE = br.PREAMBLE.replace("印刷机项目资料内容说明", "OLSK Large CNC V3 组件功能属性手册").replace("163-曹隽-印刷", "161-OLSK-Large-CNC-V3")

COVER = r"""
\begin{titlepage}
\centering
\vspace*{28mm}
{\heiti\zihao{1} OLSK Large CNC V3 组件功能属性手册\par}
\vspace{6mm}
{\songti\zihao{3} 功能 · 工作原理 · 安装方式 · 装配顺序约束 · 影片对照\par}
\vspace{14mm}
{\zihao{-3} 开源大幅面 CNC 铣床（Open Lab Starter Kit，InMachines Ingrassia GmbH 设计）\par}
\vspace{40mm}
\begin{tabular}{r@{\hspace{1em}}p{112mm}}
\zihao{-4}资料基础 & \zihao{-4}官方装配手册 Workbook（114 步）与 How-To、V3 BOM、电气/气路图、STEP 装配及本体 S1–S8 记录、装配动画 v009 状态链\\[3pt]
\zihao{-4}分析方式 & \zihao{-4}八条并行分析线（A 机架与底座 / B Y 轴与肩部 / C X 轴、Z 轴与主轴头 / D 床身与刀具辅助 / E 电气与控制 / F 气动 / G 外壳与门窗 / H 外购件家族），每条结论标明来源（我们的数据 / 官方资料 / 推断）与置信度\\[3pt]
\zihao{-4}关联制品 & \zihao{-4}功能属性/tracks/*（function.json、order\_constraints.json、notes.md、sources.md）、功能属性/order-audit.v1.json、cad-agent assembly 本体\\[3pt]
\zihao{-4}整理日期 & \zihao{-4}""" + DATE + r"""\\
\end{tabular}
\vfill
{\zihao{5} 本手册用于审查装配顺序是否在功能与安装方式上成立；所有“顺序约束”在设计确认前均为分析结论，不是放行依据。\par}
\vspace{10mm}
\end{titlepage}
\tableofcontents
\clearpage
"""

INTRO = r"""
\section{使用说明}
\subsection{目的}
把这台机器的每个功能单元和部套讲清楚四件事：它是干什么的、怎么工作、怎么装（紧固件、方向、工具、装后调整）、装配顺序上它依赖谁、谁依赖它。装配动画 v009 通过了全部几何门（穿模 0、画面口径重叠 0、可读性、媒体 QC），但“看似对的、实际顺序不对”的问题几何门发现不了：它来自功能与安装方式。本手册把这两类知识显式写下来，并逐条对照影片。
\subsection{机型原理}
OLSK Large CNC V3 是铣削区域 2500×1250 mm 的开源大幅面 CNC 铣床：铝方管与树脂接头组成机架，三轴均为滚珠丝杠（Y 轴双侧丝杠用旋转螺母驱动），闭环交流伺服，感应开关回零；4.5 kW 主轴，14 位气动换刀，自动对刀、清刀、冷却与防尘罩；全封闭外壳，三面可开启（气弹簧升窗）；工业级电气（接触器、漏电保护、断路器）与 24 V 急停；控制软件 OLOS。
\subsection{结论的可信度}
每条内容区分三类来源：\textbf{我们的数据}（Workbook、BOM、STEP、本体记录、影片状态链）、\textbf{官方资料}（README、How-To、电气/气路图、厂家手册）、\textbf{推断}（由前两者推理）。顺序约束带置信度（高/中/低）；“手册步序本身可疑”的地方列在各单元的待确认问题，不替官方改步序。
"""


def main():
    parts = [PREAMBLE, COVER, INTRO]
    for t in "ABCDEFGH":
        data, notes, sources = load_track(t)
        parts.append(track_section(t, data, notes, sources))
    parts.append(audit_section())
    parts.append("\\clearpage\\section{附录：参考资料}")
    for t in "ABCDEFGH":
        _, _, sources = load_track(t)
        if sources:
            parts.append("\\subsection{分析线 " + t + "}")
            parts.append(br.md_to_tex(br.md_sections(sources, None, drop_h1=True), base_level=3))
    parts.append("\\end{document}\n")
    OUT.write_text("\n".join(parts), encoding="utf-8")
    print("wrote", OUT)


if __name__ == "__main__":
    main()
