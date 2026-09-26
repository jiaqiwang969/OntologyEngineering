#!/usr/bin/env python3
"""给装配动画帧叠字幕并拼片。

字幕**只搬运受控工艺卡的字段，不新增主张**（K-SCN-03）。至少四项：
工序名、工装/扭矩、技术要求、本工序作业条目。缺的如实写"SOP 未给"。

未通过认证的动件在片尾以标注卡如实列出（K-DEL-03），不伪造运动。

交付按 K-DEL-01 原子化：临时 .mp4（后缀仍是 .mp4）→ ffprobe 自检 → 原子换名。
字幕用 Pillow 叠加（本机 ffmpeg 无 drawtext）。

用法：uv run --with pillow python3 tools/compose_assembly_film.py [--run FILM_v001]
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
FONTS = ["/System/Library/Fonts/Hiragino Sans GB.ttc",
         "/System/Library/Fonts/STHeiti Medium.ttc"]


def font(sz):
    for f in FONTS:
        if os.path.exists(f):
            try:
                return ImageFont.truetype(f, sz)
            except OSError:
                pass
    return ImageFont.load_default()


def wrap(d, text, fnt, maxw):
    """逐字折行；拉丁词内不折（v010 审片：技术要求 "for l / ater"、"Do not tig / hten" 被拦腰截断）。"""
    out, line = [], ""
    for ch in text:
        if d.textlength(line + ch, font=fnt) > maxw and line:
            if ch.isascii() and ch.isalnum():
                cut = line.rfind(" ")
                if cut > 0 and line[cut + 1:].isascii():
                    out.append(line[:cut])
                    line = line[cut + 1:] + ch
                    continue
            out.append(line)
            line = ch
        else:
            line += ch
    if line:
        out.append(line)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--run", default="FILM_v001")
    ap.add_argument("--sop", default="装配工艺/sop-poppy-assemblyguide.v1.json")
    ap.add_argument("--title", default="逐零件装配动画")
    ap.add_argument("--legend-extra", default="", help="图例追加文字（v005：极淡=其他台架件，此刻不在场）")
    ap.add_argument("--encode-only", action="store_true", help="合成帧已齐全时只做编码（v007：编码阶段解码 ffmpeg stderr 出错后续跑）")
    a = ap.parse_args()
    global ROOT
    ROOT = a.case_dir
    cfg = json.loads((ROOT / "robot-config.v1.json").read_text(encoding="utf-8"))
    D = ROOT / "装配动画" / a.run
    chain = json.loads((D / "state-chain.json").read_text(encoding="utf-8"))
    sop = {o["op"]: o for o in json.loads(
        (ROOT / a.sop).read_text(encoding="utf-8"))["operations"]}
    raw = D / "frames_raw"
    outf = D / "frames"
    outf.mkdir(exist_ok=True)
    files = sorted(raw.glob("r_*.png"))
    if not files:
        print("没有渲染帧", file=sys.stderr)
        return 2

    # 帧 → 章
    spans = [(c["start"], c["end"], c) for c in chain["chapters"]]

    def chap(f):
        for s, e, c in spans:
            if s <= f < e:
                return c
        return spans[-1][2]

    f_big, f_mid, f_sm = font(40), font(24), font(19)

    # ---- 连接图面板（S8）：用户要求成片携带连接图,标签作为沟通坐标系 ----------
    # 全图 1558 节点是毛线球;面板画**逐章局部子图**:本章动件按播放序列出
    # (标签=occ 编号·件名,occ 编号是全部记录的主键,沟通直接引用),
    # 连线指向连接对象;颜色随帧同步,就位瞬间边点亮;终章显示全图实现率。
    import bisect
    RW, PW = 1280, 360                      # 渲染幅宽 / 面板宽
    g8 = json.loads((ROOT / "本体/S8-连接关系图.v1.json").read_text(encoding="utf-8"))
    gnode = {nd["occ"]: nd for nd in g8["nodes"]}
    gadj = {}
    for e in g8["edges"]:
        gadj.setdefault(e["a"], set()).add(e["b"])
        gadj.setdefault(e["b"], set()).add(e["a"])
    seat_f = {}                              # 视觉"就位"帧
    ctx_first = {}
    finale_start = None
    for c0 in chain["chapters"]:
        if c0.get("finale"):
            finale_start = c0["start"]
            continue
        for m0 in c0["movers"]:
            seat_f[m0["occ"]] = m0["f1"]
        # X1 审计点名的面板时钟缺陷修复：context 的 realized 帧必须消费
        # context_deferred_fasteners 的**真实出现帧**，而不是一律取章首。
        deferred = c0.get("context_deferred_fasteners", {}) or {}
        for oid in c0.get("context", []):
            ctx_first.setdefault(oid, int(deferred.get(oid, c0["start"])))
        # 静态退场件（卡死件）在本章动件全部就位后才出现
        wd_f = int(c0.get("static_withdrawn_appear_frame", c0["start"]))
        for oid in c0.get("static_withdrawn", []):
            ctx_first.setdefault(oid, wd_f)
    def place_frame(oid):
        if oid in seat_f:
            return seat_f[oid]
        if oid in ctx_first:
            return ctx_first[oid]
        return finale_start                  # 从未出场:终章才在场
    edge_rf = sorted((max(filter(None, (place_frame(e["a"]), place_frame(e["b"])) )
                      if place_frame(e["a"]) and place_frame(e["b"]) else 10**9)
                     for e in g8["edges"]))
    N_EDGES = len(g8["edges"])
    CODE6 = re.compile(r"^[0-9A-Z]{6}_")

    def lab(oid, width=11):
        nm = CODE6.sub("", gnode.get(oid, {}).get("product", oid))
        return f"{oid[-4:]}·{nm[:width]}"

    f_p1, f_p2 = font(17), font(14)

    def draw_panel(d, f, c, H=720):
        x0 = RW
        # v005：面板通高（v004 只画到 720，右下角露出一块未覆盖的三维画面）
        d.rectangle([x0, 0, x0 + PW, H], fill=(16, 18, 24))
        d.line([x0, 0, x0, H], fill=(60, 64, 74), width=2)
        realized = bisect.bisect_right(edge_rf, f)
        d.text((x0 + 16, 14), "连接图 S8", font=f_p1, fill=(150, 205, 170))
        d.text((x0 + 16, 40), f"全图实现 {realized}/{N_EDGES} 边", font=f_p2, fill=(150, 156, 166))
        if c.get("finale"):
            y = 88
            for ln, col in (
                ("整机装配态", (242, 244, 247)),
                (f"节点 {len(gnode)} · 边 {N_EDGES}", (200, 205, 212)),
                (f"本片实现 {realized} 边", (150, 205, 170)),
                ("未实现连接属于:", (150, 156, 166)),
                ("· 真实序卡死件(对开壳/球铰/插装,见台账)", (150, 156, 166)),
                ("· 工序归属未定件(暗色上下文)", (150, 156, 166)),
                ("台账: S8/S9/S10 + 最新交付说明", (110, 116, 126)),
            ):
                d.text((x0 + 16, y), ln, font=f_p2, fill=col)
                y += 26
            return
        if c.get("presence_only"):
            y = 88
            for ln, col in (
                ("单元结合章", (242, 244, 247)),
                (f"汇合成员 {len(c.get('closure_members', []))} 件", (200, 205, 212)),
                ("前驱台架产物按 S1 位姿汇合呈现", (150, 156, 166)),
                ("结合运动未认证（HOLD）：", (150, 156, 166)),
                ("不主张结合动作，仅陈述汇合后状态", (150, 156, 166)),
            ):
                d.text((x0 + 16, y), ln, font=f_p2, fill=col)
                y += 26
            return
        movers = c["movers"]
        # 伙伴列(非本章动件的连接对象,去重,按被连次数排序)
        mset = {m["occ"] for m in movers}
        pcount = {}
        for m in movers:
            for q in gadj.get(m["occ"], ()):
                if q not in mset:
                    pcount[q] = pcount.get(q, 0) + 1
        partners = sorted(pcount, key=lambda q: (-pcount[q], q))[:17]
        overflow = len(pcount) - len(partners)
        top, bot = 72, 700
        rh_m = min(26, max(15, (bot - top) // max(len(movers), 1)))
        rh_p = min(26, max(15, (bot - top) // max(len(partners) + (1 if overflow > 0 else 0), 1)))
        ym = {m["occ"]: top + i * rh_m + rh_m // 2 for i, m in enumerate(movers)}
        yp = {q: top + i * rh_p + rh_p // 2 for i, q in enumerate(partners)}
        xm_r, xp_l = x0 + 168, x0 + 196
        active = {m["occ"] for m in movers if m["f0"] <= f < m["f1"]}
        # 边:动件→伙伴
        for m in movers:
            oid = m["occ"]
            for q in gadj.get(oid, ()):
                if q in yp:
                    if oid in active:
                        col, wd = (255, 128, 26), 2
                    elif f >= m["f1"]:
                        col, wd = (96, 150, 116), 1
                    else:
                        col, wd = (58, 62, 72), 1
                    d.line([xm_r, ym[oid], xp_l, yp[q]], fill=col, width=wd)
        # 动件间的边(同章互连):左缘竖线
        for m in movers:
            for q in gadj.get(m["occ"], ()):
                if q in ym and ym[q] > ym[m["occ"]]:
                    col = (255, 128, 26) if (m["occ"] in active or q in active) else (58, 62, 72)
                    d.line([x0 + 8, ym[m["occ"]], x0 + 8, ym[q]], fill=col, width=1)
        # 动件行
        for m in movers:
            oid = m["occ"]
            y = ym[oid]
            if oid in active:
                d.rectangle([x0 + 12, y - rh_m // 2 + 1, xm_r, y + rh_m // 2 - 1],
                            fill=(52, 34, 16))
                col = (255, 148, 52)
            elif f >= m["f1"]:
                col = (198, 203, 210)
            else:
                col = (96, 101, 112)
            d.text((x0 + 14, y - 8), lab(oid), font=f_p2, fill=col)
        # 伙伴行
        act_partners = set()
        for m in movers:
            if m["occ"] in active:
                act_partners |= gadj.get(m["occ"], set())
        for q in partners:
            y = yp[q]
            col = (222, 196, 140) if q in act_partners else (120, 126, 138)
            d.text((xp_l + 4, y - 8), lab(q, 9), font=f_p2, fill=col)
        if overflow > 0:
            d.text((xp_l + 4, top + len(partners) * rh_p),
                   f"…+{overflow}(见S8)", font=f_p2, fill=(90, 96, 106))

    n = 0
    if a.encode_only:
        files = []
        n = len(sorted(outf.glob("f_*.png")))   # 已合成帧数作为预期时长依据
    for p in files:
        f = int(re.search(r"r_(\d+)", p.name).group(1))
        c = chap(f)
        o = sop.get(c["op"], {})
        # Poppy 指南字段映射：紧固件规格/视频/备注。缺的如实写"指南未给"。
        tools_s = o.get("fasteners") or "指南未给"
        # 中栏第二项：官方视频（Poppy 指南）或胶粘规范（Fourier SOP），都没有则如实
        tq = (("官方视频 " + o["video"]) if o.get("video")
              else ("胶 " + o["adhesive"]) if o.get("adhesive") else "指南未给")
        tech = (o.get("notes") or o.get("target") or "指南未给").replace("\n", " ")
        tech = tech.replace("\u2019", "'").replace("\u2018", "'").replace("\u201c", '"').replace("\u201d", '"')   # v005：字体缺弯引号字形（豆腐块）
        steps = []
        rim = Image.open(p).convert("RGB")
        # v005：渲染幅宽等于 RW 时不再被面板遮住右侧 320 px（v004 渲 1600 宽、面板从 1280 起盖住机器右端）
        if rim.size[0] != RW:
            rim = rim.resize((RW, round(rim.size[1] * RW / rim.size[0])), Image.LANCZOS)
        im = Image.new("RGB", (RW + PW, rim.size[1]), (16, 18, 24))
        im.paste(rim, (0, 0))
        W, H = RW, rim.size[1]            # 字幕带只覆盖三维画面区,面板区自管
        d = ImageDraw.Draw(im, "RGBA")
        d.rectangle([0, 0, W, 104], fill=(10, 12, 16, 200))
        d.rectangle([0, H - 118, W, H], fill=(10, 12, 16, 200))
        draw_panel(d, f, c, H)
        idx = [i for i, (_, _, cc) in enumerate(spans, 1) if cc is c][0]
        d.text((36, 20), f"{idx:02d}/{len(spans)}  {c['op']}", font=f_big, fill=(242, 244, 247))
        if c.get("finale"):
            # 终章是呈现章不是工序章：不显示"SOP 未给"噪声，直陈其主张边界
            d.text((36, 68), c.get("note", "整机 CAD 装配态"), font=f_mid, fill=(150, 205, 170))
            tech = "终章呈现 CAD 装配态（S1 位姿为定义真值），不表达运动，不主张未表达工序已完成"
        elif c.get("presence_only"):
            d.text((36, 68), "单元结合章：前驱台架产物汇合呈现（结合运动未认证 HOLD）",
                   font=f_mid, fill=(150, 205, 170))
            tech = c.get("note", "") or "结合运动未认证；本章不主张结合动作"
        else:
            # v005：副标题超宽时截断加省略号（v004 长紧固件清单跨过面板并被画面右缘裁掉）
            sub = f"紧固 {tools_s}   ·   {tq}   ·   本章动件 {len(c['movers'])}"
            while d.textlength(sub, font=f_mid) > W - 72 and len(tools_s) > 8:
                tools_s = tools_s[:-4].rstrip(" ;；,，") + "…"
                sub = f"紧固 {tools_s}   ·   {tq}   ·   本章动件 {len(c['movers'])}"
            d.text((36, 68), sub, font=f_mid, fill=(150, 205, 170))
        y = H - 108
        for ln in wrap(d, "技术要求：" + tech, f_sm, W - 72)[:2]:
            d.text((36, y), ln, font=f_sm, fill=(206, 210, 216))
            y += 25
        d.text((36, y), "件沿各自认证轴平移到位；一次只动一个件（S7 认证的正是这种运动）。"
                        "橙=运动中　灰=已就位　暗=装配对象/未表达运动件" + a.legend_extra, font=f_sm, fill=(160, 166, 176))
        im.save(outf / f"f_{f:05d}.png")
        n += 1
    print(f"叠字幕 {n} 帧")

    final = D / f"{cfg['case']}_{a.title}.mp4"
    tmp = D / f"tmp_{cfg['case']}_{a.title}.mp4"
    cmd = ["ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
           "-framerate", "30", "-pattern_type", "glob", "-i", str(outf / "f_*.png"),
           "-pix_fmt", "yuv420p", "-c:v", "libx264", "-crf", "20",
           "-movflags", "+faststart", str(tmp)]
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print("ffmpeg 失败:", r.stderr[-600:], file=sys.stderr)
        tmp.unlink(missing_ok=True)
        return 3
    pr = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration",
                         "-of", "json", str(tmp)], capture_output=True, text=True, errors="replace")
    try:
        dur = float(json.loads(pr.stdout)["format"]["duration"])
    except Exception as e:
        print("自检失败:", e, file=sys.stderr)
        tmp.unlink(missing_ok=True)
        return 4
    want = n / 30.0
    if abs(dur - want) > max(1.0, 0.08 * want):
        print(f"自检失败：时长 {dur:.1f}s vs 预期 {want:.1f}s，保留旧文件", file=sys.stderr)
        tmp.unlink(missing_ok=True)
        return 5
    tmp.replace(final)
    (D / "film-manifest.json").write_text(json.dumps({
        "$schema": "assembly-ontology.film-manifest/v2",
        "output": final.name, "frames": n, "duration_s": round(dur, 2), "fps": 30,
        "chapters": len(spans),
        "subtitle_fields": ["工序名", "紧固规格", "官方视频", "备注/目标", "动件数"],
        "subtitle_backend": "Pillow 叠加（本机 ffmpeg 无 drawtext 滤镜）",
        "delivery": "临时 .mp4 → ffprobe 自检 → 原子换名；+faststart",
        "not_animated": chain["not_animated"],
        "claim_boundary": chain["claim_boundary"] + [
            "字幕只搬运指南已有字段；没给的写『指南未给』，不补。",
            "字幕来源为官方装配指南搬运记录；指南未给的字段写『指南未给』，不补。",
        ],
    }, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {final}  {dur:.1f}s  {final.stat().st_size/1e6:.1f}MB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
