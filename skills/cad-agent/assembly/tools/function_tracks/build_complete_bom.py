#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
OLSK Large CNC V3 —— 完整 BOM 与采购/加工清单 生成器
=====================================================

用法（在案例目录，如 资料解读工作文件 下）：
    .venv/bin/python 工艺文件/BOM与采购/build_bom.py

只读输入
--------
  功能属性/inputs/BOM_sheet1.csv          官方 BOM（Part Name, Quantity；无数量行=顶层分组）
  装配工艺/sop-olsk-workbook.v1.json      官方手册 114 步、581 个零件行
  本体/S1-occurrence清单-体展开.v2.json    STEP 实例（1697 叶 + 822 装配 occurrence）
  本体/S3-工序实例绑定.v1.json             手册名 ↔ STEP 名 的实例级绑定
  本体/S1-紧固件层.v1.json                 手册 GLB 中的 2487 个紧固件网格
  网格导出-体展开/manifest.json + *.stl    定义级几何（用于型材下料长度）
  功能属性/tracks/*/function.json          外购件家族解码（H 线 15 家族 + A–G 线）
  功能属性/inputs/OLSK_README.md           机器概况

输出（只写到 工艺文件/BOM与采购/）
--------------------------------
  OLSK_Large_CNC_V3_完整BOM_v1.xlsx       多 sheet 工作簿
  bom.v1.json                             机器可读的同一份数据
  BOM差异报告_v1.md                        方法、数字、发现、待确认问题
  _aabb_cache.v1.json                     STL 包围盒缓存（可删，删后自动重建）

判定口径
--------
  1. 行单位 = 手册零件名（404 个），这是唯一同时能连到 STEP 与官方 BOM 的键。
  2. STEP 实例数 = S3 绑定到该手册名的 **动件（mover）数**；一个 mover 是一个机器级刚体
     （叶 occurrence / 顶层装配自体的一个体 / 一个预装模块 solver_unit），不是叶体数。
     另给一列"STEP 全模型同名实例数"用于交叉核对。
  3. 外购件内部子体折叠：叶 occurrence 若位于某个 depth-2 装配（=预装模块）之下，
     一律计入该模块，不作为独立机器零件。
  4. 凡本案数据中没有的（材料牌号、等级、扭矩、气弹簧力值等），一律标"推断"或 UNKNOWN，
     不用公开资料的典型值顶替。
"""
from __future__ import annotations
import csv, json, os, re, struct, sys, math, collections, datetime, hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]           # .../assembly_film
OUT  = ROOT / "工艺文件" / "BOM与采购"
OUT.mkdir(parents=True, exist_ok=True)

P_BOM   = ROOT / "功能属性/inputs/BOM_sheet1.csv"
P_SOP   = ROOT / "装配工艺/sop-olsk-workbook.v1.json"
P_OCC   = ROOT / "本体/S1-occurrence清单-体展开.v2.json"
P_S3    = ROOT / "本体/S3-工序实例绑定.v1.json"
P_FAST  = ROOT / "本体/S1-紧固件层.v1.json"
P_MESH  = ROOT / "网格导出-体展开"
P_TRACKS= ROOT / "功能属性/tracks"
CACHE   = OUT / "_aabb_cache.v1.json"

VERSION   = "v1"
BUILT_AT  = datetime.datetime.now().astimezone().isoformat(timespec="seconds")

# ---------------------------------------------------------------- 工具函数
def base_name(s: str) -> str:
    """把 STEP 里的实例后缀（(1)/(2)/_1/vN/(Mirror)/_default）剥掉，得到定义级基名。"""
    s = (s or "").strip()
    s = re.sub(r"\(Mirror\)", "", s, flags=re.I)
    prev = None
    while prev != s:
        prev = s
        s = re.sub(r"\s*\(\d+\)\s*$", "", s)
        s = re.sub(r"_(\d+)$", "", s)
        s = re.sub(r"\s+v\d+\s*$", "", s, flags=re.I)
        s = re.sub(r"_default$", "", s)
        s = s.strip()
    s = re.sub(r"\s+", " ", s)
    s = re.sub(r"\((?:cnc milled|CNC milled|CNC Milled|cnc Milled)\)", "(CNC Milled)", s)
    s = re.sub(r"\((?:resin printed|Resin printed|Resin Printed|resin 3D printed|"
               r"Resin 3D printed|Resin 3D Printed|Resin 3d printed)\)", "(Resin printed)", s)
    return s.strip()

def norm_key(s: str) -> str:
    s = (s or "").lower()
    s = re.sub(r"\(mirror\)", "", s)
    s = re.sub(r"\((?:\d+|cnc milled|cnc|resin printed|resin 3d printed)\)", " ", s)
    s = re.sub(r"\bv\d+\b", " ", s)
    s = re.sub(r"[^a-z0-9äöüß]+", " ", s)
    return " ".join(s.split())

def stl_extents(path: Path):
    """返回 STL 的 [dx,dy,dz]（定义坐标系，mm），只读顶点极值。"""
    try:
        with open(path, "rb") as f:
            head = f.read(84)
            if head[:5] == b"solid" and b"facet" in head:
                f.seek(0); mn = [1e30]*3; mx = [-1e30]*3
                for line in f:
                    if line.strip().startswith(b"vertex"):
                        v = [float(x) for x in line.split()[1:4]]
                        for i in range(3):
                            mn[i] = min(mn[i], v[i]); mx[i] = max(mx[i], v[i])
                if mn[0] > 1e29: return None
                return [round(mx[i]-mn[i], 2) for i in range(3)]
            n = struct.unpack("<I", head[80:84])[0]
            if n == 0: return None
            buf = f.read(n*50)
            if len(buf) < n*50: return None
            import numpy as np
            d = np.frombuffer(buf, dtype=np.uint8).reshape(n, 50)
            v = d[:, 12:48].copy().view(np.float32).reshape(n*3, 3).astype(np.float64)
            return [round(float(v[:, i].max()-v[:, i].min()), 2) for i in range(3)]
    except Exception:
        return None

# ---------------------------------------------------------------- 载入输入
def load_bom():
    rows, groups, cur = [], [], None
    with open(P_BOM, newline="", encoding="utf-8-sig") as f:
        for r in csv.DictReader(f):
            n = (r.get("Part Name") or "").strip()
            q = (r.get("Quantity") or "").strip()
            if not n: continue
            if q == "":
                cur = n; groups.append(n)
            else:
                rows.append({"group": cur, "name": n, "qty": int(float(q))})
    return groups, rows

def load_sop():
    d = json.load(open(P_SOP, encoding="utf-8"))
    return d["operations"]

def load_occ():
    d = json.load(open(P_OCC, encoding="utf-8"))
    return d["occurrences"], d["assembly_occurrences"], d

def load_s3():
    return json.load(open(P_S3, encoding="utf-8"))

print("[1/9] 载入输入 …", file=sys.stderr)
BOM_GROUPS, BOM_ROWS = load_bom()
OPS_SOP   = load_sop()
LEAVES, ASMS, OCCDOC = load_occ()
S3        = load_s3()
FASTLAYER = json.load(open(P_FAST, encoding="utf-8"))
MANIFEST  = json.load(open(P_MESH / "manifest.json", encoding="utf-8"))["parts"]

# ---------------------------------------------------------------- 索引：STEP
ASM_BY_PATH = {a["nauo_path"]: a for a in ASMS}
def lvl2(path: str) -> str:
    seg = path.split("/")
    return "/".join(seg[:2]) if len(seg) >= 2 else path

# 机器级归属：每个叶 occurrence 归到 depth-2 节点
STEP_MACHINE = collections.Counter()      # 机器级基名 -> 件数（折叠后）
MODULE_INTERNALS = collections.Counter()  # 模块基名 -> 内部叶数
LEAF_LVL2 = {}                            # occ_uid -> ('LEAF'|'MODULE', base)
for o in LEAVES:
    p = lvl2(o["nauo_path"])
    if p == o["nauo_path"]:
        LEAF_LVL2[o["occ_uid"]] = ("LEAF", base_name(o["product"]))
    else:
        a = ASM_BY_PATH.get(p)
        if a:
            LEAF_LVL2[o["occ_uid"]] = ("MODULE", base_name(a["product"]))
            MODULE_INTERNALS[base_name(a["product"])] += 1
        else:
            LEAF_LVL2[o["occ_uid"]] = ("LEAF", base_name(o["product"]))
for kind, b in LEAF_LVL2.values():
    if kind == "LEAF":
        STEP_MACHINE[b] += 1
for a in ASMS:
    if a["depth"] == 2:
        STEP_MACHINE[base_name(a["product"])] += 1

# STEP 全模型同名叶数（不折叠）
STEP_LEAF_CNT = collections.Counter(base_name(o["product"]) for o in LEAVES)
STEP_ASM2_CNT = collections.Counter(base_name(a["product"]) for a in ASMS if a["depth"] >= 2)
STEP_TOP = {}                              # base -> 顶层装配
for o in LEAVES:
    STEP_TOP.setdefault(base_name(o["product"]), o["nauo_path"].split(":")[0])
for a in ASMS:
    if a["depth"] >= 2:
        STEP_TOP.setdefault(base_name(a["product"]), a["nauo_path"].split(":")[0])

def step_instances(b: str) -> int:
    """一个 STEP 基名在整机中的机器级件数。"""
    if STEP_LEAF_CNT.get(b):
        return STEP_LEAF_CNT[b]
    return STEP_ASM2_CNT.get(b, 0)

# ---------------------------------------------------------------- 索引：手册
MAN_QTY   = collections.Counter()
MAN_STEPS = collections.defaultdict(list)
MAN_ROWS  = []
for o in OPS_SOP:
    for p in o.get("parts", []):
        n = (p.get("name") or "").strip()
        if not n: continue
        q = p.get("qty") or 0
        MAN_QTY[n] += q
        MAN_STEPS[n].append(o["step"])
        MAN_ROWS.append({"op": o["op"], "step": o["step"], "chapter": o["chapter"],
                         "title": o["title"], "name": n, "qty": q})
MAN_NAMES = sorted(MAN_QTY)

# ---------------------------------------------------------------- 索引：S3 绑定
MOVERS_BY_MAN   = collections.Counter()                     # 手册名 -> 绑定动件数
STEPNAME_BY_MAN = collections.defaultdict(collections.Counter)
S3_BOUND_OCC    = collections.Counter()                     # 手册名 -> 绑定叶体数
PARTROW_BY_MAN  = collections.defaultdict(lambda: {"sop": 0, "glb": 0, "bound": 0})
HOLDS_BY_MAN    = collections.Counter()
OPS_BY_MAN      = collections.defaultdict(set)
for o in S3["operations"]:
    for m in o["movers"]:
        gs = set(m.get("via_glb_names") or [])
        for g in gs:
            MOVERS_BY_MAN[g] += 1
            STEPNAME_BY_MAN[g][base_name(m.get("product", ""))] += 1
            S3_BOUND_OCC[g] += len(m.get("member_occ_ids") or [])
            OPS_BY_MAN[g].add(o["step"])
    for pr in o.get("part_rows", []):
        d = PARTROW_BY_MAN[pr["sop_name"]]
        d["sop"] += pr.get("sop_qty") or 0
        d["glb"] += pr.get("glb_meshes") or 0
        d["bound"] += pr.get("bound_occurrences") or 0
    for h in o.get("holds", []):
        HOLDS_BY_MAN[h.get("sop_name")] += 1

# 未绑定叶（STEP 有、手册无）
UOCC = {o["occ_uid"]: o for o in LEAVES}
UNBOUND_MACHINE = collections.Counter()   # (class, base, top) -> n
UNBOUND_FOLDED  = collections.Counter()   # module base -> n
for u in S3["unbound_leaves"]:
    o = UOCC.get(u["occ_uid"])
    if not o:
        UNBOUND_MACHINE[(u.get("class", "?"), base_name(u.get("product", "")), "?")] += 1
        continue
    p = lvl2(o["nauo_path"])
    if p != o["nauo_path"] and p in ASM_BY_PATH:
        UNBOUND_FOLDED[base_name(ASM_BY_PATH[p]["product"])] += 1
    else:
        UNBOUND_MACHINE[(u.get("class", "?"), base_name(o["product"]),
                         o["nauo_path"].split(":")[0])] += 1

# ---------------------------------------------------------------- 手工对名表
# S3 未绑定、但按 H 线证据与命名可以高置信对上的 STEP 件。一律标"人工对名(推断)"。
ALIAS_MANUAL = {
    "Back Hinge":                 "Hinge GN 237-ZD-60-60-A-SW",
    "Roof Carriage":              "Linear Guide Cart HGH15C X4",
    "Y Endstop":                  "Inductive Sensor SN04",
    "Y Endstop Holder":           "Y Endstop Holder (Resin Printed)",
    "Valve 5 Way":                "4V210-08 5/2 12VDC Solenoid Valve",
    "Valve 2 Way 6-6":            "Solenoid Valve 2 Way",
    "Valve 2 Way 6-4":            "Solenoid Valve 2 Way",
    "Pressure Sensor":            "MLH100PGB01A",
    "Oil Pump":                   "CNC_OIL_PUMP",
    "Pressure Regulator":         "Pressure Regulator Single",
    "Pressure Regulator w/ Filter":"8002325 LFR-1/4-D-5M-MINI-A-MPA---",
    "Combo Pressure Regulator w/ Filter & De-Oiler": "162763 FRC-1/4-D-7-MINI-A",
    "Safety Relief Valve":        "Pressure Relese Valve",
    "Flow Regulator":             "Flow_Control",
    "X Chain":                    "Cable Chain CK18 18x60mm",
    "Front Chain":                "Cable Chain 18x37",
    "Power Supply 12V":           "Power supply NDR-480-3D",
    "Air Connector":              "Air Connector Press Fit 6-6mm",
    "Push Fit Connector T 6":     "Press Fit Connector 6mm T",
    "Push Fit Connector L 6":     "Press Fit Connector 90",
    "T-fitting":                  "0,250'' NPT TEE",
    "Tube 6":                     "Pipe 6mm",
    "X Motor Belt":               "Belt GT3 387-3MGT-15",
    "Z Front Cover":              "Z Cover Front (Resin printed)",
    "Electronic Panel":           "Electric Plate (CNC milled)",
    "Pneumatics Cable Duct Short":"Cable Duct 45X60",
    "PC/PCB Differential":        "Differential 5SV3312-6_G_I202_XX_22847V",
    "Exhaust Pipe Attacher":      "Guide Pipe Attacher (Resin printed)",
    "Roof Panel Front Left":      "Roof Front Left (CNC Milled)",
    "Roof Panel Front Right":     "Roof Front Right (CNC Milled)",
    "Roof Panel Back Right":      "Roof Back Right (CNC Milled)",
    "Spindle Relay":              "Relay 3RT2016-1BB41_G_NSA0_XX_00825V",
    "Relay":                      "Relay E290 16A",
}

# S3 已知的张冠李戴绑定（依据 功能属性/tracks/H/notes.md），写进主清单"备注/存疑"列
KNOWN_MISBIND = {
 "Spindle Circuit Breaker": "S3 把 `Power supply HDR-15 v2` 绑成本件——CAD 侧对象与手册件不是同一物（H 线第 12 节）",
 "PC/PCB Circuit Breaker":  "S3 让一只 Differential 同时消费 `PC/PCB Circuit Breaker` 与 `Drivers Circuit Breaker` 两个手册件（H 线第 12 节）",
 "Drivers Circuit Breaker": "同上：与 `PC/PCB Circuit Breaker` 共用同一 CAD 件",
 "Z Endstop":               "S3 把一个名为 `Z Cover` 的 CAD 件绑成本件——Z 归零传感器在 CAD 中未对上（H 线第 15 节）",
 "Piston Bottom Support Right": "S3 把 `Window Spacer 20mm (Resin printed) (6)` 绑成本件（H 线第 10 节）",
 "Tool Holder Support":     "CAD 与手册命名互换：CAD 的 `BT30 Tool Holder` 消费手册的 `Tool Holder Support`（H 线第 4 节），核对时极易看反",
 "Tool Holder Base":        "CAD 与手册命名互换：CAD 的 `Tool Holder Support (Resin printed)` 消费手册的 `Tool Holder Base`（H 线第 4 节）",
 "Z Motor":                 "S3 把电机与 `Z Motor Holder (Resin printed)` 合成同一刚体，电机本身没有独立 CAD 绑定（H 线第 3 节）",
 "Brush":                   "S3 把一把 `BT30 Tool Holder` 绑成本件，与清刀刷不是同一物",
 "Y Endstop":               "Y 的 SN04 与树脂 `Y Endstop Holder` 在 S3 中完全未绑定，本行为人工对名（H 线第 15 节）",
 "Back Hinge":              "6 只 GN 237 铰链在 CAD 中有 18 个 occurrence，S3 一个都没绑定，本行为人工对名（H 线第 14 节）",
}

# ---------------------------------------------------------------- 紧固件解码
FAST_PREFIX = ("B-screw", "C-screw", "HF-screw", "Lock Nut", "Washer",
               "Countersunk Screw", "Thermoplastic Screw", "Hex Screw",
               "Set Screw", "Standoff")

# 前缀 -> (头型, 建议标准号, BOM 中的写法, 材料/等级推断, 拧紧工具推断)
FAST_HEAD = {
    "B-screw":            ("内六角半圆头（Button head）", "ISO 7380-1", "ISO 3780",
                           "合金钢 10.9 发黑 或 A2-70 不锈钢（推断）", "内六角"),
    "C-screw":            ("内六角圆柱头（Cap head）",   "ISO 4762 / DIN 912", "DIN 912",
                           "合金钢 12.9 发黑 或 A2-70 不锈钢（推断）", "内六角"),
    "HF-screw":           ("未知（手册自造前缀，疑为法兰面/型材专用头）", "UNKNOWN —— 需向 InMachines 确认",
                           "（BOM 无此行）", "UNKNOWN", "UNKNOWN"),
    "Countersunk Screw":  ("内六角沉头（90°）",          "ISO 10642 / DIN 7991", "Countersunk Screw",
                           "合金钢 10.9 或 A2-70（推断）", "内六角"),
    "Thermoplastic Screw":("热塑自攻（塑料/复合板专用）", "无公制通用标准；按厂家型号（如 EJOT DELTA PT）采购（推断）",
                           "Thermoplastic Screw", "碳钢镀锌（推断）", "内六角/十字（推断）"),
    "Hex Screw":          ("外六角头栓",                 "ISO 4014 / DIN 931（部分螺纹）", "（BOM 无此行）",
                           "8.8 镀锌（推断）", "开口/套筒扳手"),
    "Lock Nut":           ("尼龙嵌件锁紧螺母",           "DIN 985 / ISO 10511", "DIN 985",
                           "8 级 镀锌（推断）", "开口/套筒扳手"),
    "Washer":             ("平垫圈",                     "ISO 7089 / DIN 125A", "（BOM 无此行）",
                           "200HV 镀锌（推断）", "—"),
    "Washer_LARGE":       ("大外径平垫圈",               "ISO 7093-1 / DIN 9021", "（BOM 无此行）",
                           "200HV 镀锌（推断）", "—"),
    "Set Screw":          ("内六角紧定螺钉（顶丝）",     "DIN 913（平端）/ DIN 916（凹端）", "（BOM 无此行）",
                           "45H 发黑（推断）", "内六角"),
    "Standoff":           ("双内螺纹六角隔离柱（铜柱）", "无标准号；按厂家规格（FF, L=8 mm）采购",
                           "（BOM 无此行）", "黄铜镀镍（推断）", "—"),
}
# 内六角对边（ISO 4762/7380/10642）与外六角对边，用于工具清单
HEX_KEY = {"M3": "2.5", "M4": "3", "M5": "4", "M6": "5", "M8": "6", "M10": "8", "M12": "10"}
HEX_KEY_BUTTON = {"M3": "2", "M4": "2.5", "M5": "3", "M6": "4", "M8": "5", "M10": "6", "M12": "8"}
HEX_KEY_CSK   = {"M3": "2", "M4": "2.5", "M5": "3", "M6": "4", "M8": "5", "M10": "6", "M12": "8"}
WRENCH_NUT    = {"M3": "5.5", "M5": "8", "M6": "10", "M8": "13(锁紧螺母常用 14)"}
# 建议包装数（贸易常规，推断）
PACK = {"M3": 200, "M4": 200, "M5": 100, "M6": 100, "M8": 50, "M10": 25, "M12": 25}

def parse_fastener(name: str):
    """返回 dict(prefix, thread, length, head, std, bom_style, material, tool) 或 None。"""
    for pre in sorted(FAST_PREFIX, key=len, reverse=True):
        if name.startswith(pre):
            rest = name[len(pre):].strip()
            large = "Large" in rest
            m = re.match(r"^(M\d+)(?:-(\d+))?", rest)
            thread = m.group(1) if m else ""
            length = int(m.group(2)) if (m and m.group(2)) else None
            key = "Washer_LARGE" if (pre == "Washer" and large) else pre
            head, std, bomstyle, mat, tool = FAST_HEAD[key]
            if pre == "B-screw":       t = f"内六角 {HEX_KEY_BUTTON.get(thread,'?')} mm"
            elif pre == "C-screw":     t = f"内六角 {HEX_KEY.get(thread,'?')} mm"
            elif pre == "Countersunk Screw": t = f"内六角 {HEX_KEY_CSK.get(thread,'?')} mm"
            elif pre == "Lock Nut":    t = f"扳手 {WRENCH_NUT.get(thread,'?')} mm"
            elif pre == "Hex Screw":   t = "扳手 13 mm"
            else:                      t = tool
            return {"prefix": pre, "thread": thread, "length": length,
                    "head": head, "std": std, "bom_style": bomstyle,
                    "material": mat, "tool": t, "large": large}
    return None

def bom_name_for_fastener(name: str) -> str:
    if name.startswith("B-screw "):  return "ISO 3780 " + name[8:]
    if name.startswith("C-screw "):  return "DIN 912 "  + name[8:]
    if name.startswith("Lock Nut "): return "DIN 985 "  + name[9:]
    return name

# ---------------------------------------------------------------- 外购件解码
# (正则, 家族, 厂家, 型号/订货号, 规格解码) —— 依据 功能属性/tracks/H
DECODE = [
 (r"HGH25CAZA",                  "直线导轨与滑块", "HIWIN", "HGH25CAZA",
  "25 系列四方型法兰滑块 48×84×40 mm；C=34.9 kN、C0=52.82 kN"),
 (r"HGH15C",                     "直线导轨与滑块", "HIWIN", "HGH15C",
  "15 系列滑块；C=14.7 kN、C0=23.47 kN"),
 (r"Y Linear Guide HGR25|HGR25 2945",       "直线导轨与滑块", "HIWIN", "HGR25 R C，L=2945 mm",
  "宽 23 / 高 22 mm，M6 沉孔，孔距 60 mm；每根 49 个固定点"),
 (r"HGR25 R C 1620|Linear Guide Cart HGR25 R C 1620", "直线导轨与滑块", "HIWIN", "HGR25 R C，L=1620 mm",
  "27 个固定点；本机用 M5 螺钉压装 —— HGR25 沉孔为 M6，疑为 HGR20，需确认"),
 (r"HGR25\s?520|Z Linear Gui",   "直线导轨与滑块", "HIWIN", "HGR25 R C，L=520 mm",
  "9 个固定点（520÷60≈9，与 HGR25 孔距吻合）"),
 (r"HGR15\s*2700|Linear Rail HGR15", "直线导轨与滑块", "HIWIN", "HGR15，L=2700 mm", "顶部排风吊架导轨"),
 (r"HGR15|Linear Guide Rail HGR15", "直线导轨与滑块", "HIWIN", "HGR15，L=820 mm",
  "升降窗/门导轨；WB-13 每根 18 点、WB-46.1 每根 17 点，两处不一致"),
 (r"SFU3210|Ball Screw SFU3210", "滚珠丝杠与支承", "—（CNC Profi 加工端）", "SFU3210，L=3000 mm",
  "Ø32 / 导程 10 mm，C7 轧制；Y 轴用作固定不转丝杠（旋转螺母方案）"),
 (r"SFU2505|Ball Screw SFU2505", "滚珠丝杠与支承", "—", "SFU2505，L=400 mm（Z）", "Ø25 / 导程 5 mm"),
 (r"X Ball Screw 2505|2505 1600","滚珠丝杠与支承", "—", "SFU2505，L=1600 mm（X）", "Ø25 / 导程 5 mm"),
 (r"Ball Screw Nut SFU 3210",    "滚珠丝杠与支承", "—", "SFU3210 滚珠螺母（法兰单螺母）", "配 Ø32/导程 10 丝杠"),
 (r"Ball Screw Nut 2510|Attachment Nut 2510|X Attachment Nut", "滚珠丝杠与支承", "—",
  "滚珠螺母 2510（CAD 名）", "命名与丝杠 2505 矛盾：按 $100=640 步/mm 反算导程应为 5 mm，疑笔误"),
 (r"Attachment Nut 2505",        "滚珠丝杠与支承", "—", "滚珠螺母 2505", "Ø25 / 导程 5 mm"),
 (r"Bearing 7210",               "滚珠丝杠与支承", "—", "7210 角接触球轴承", "Y 旋转螺母支承对之一；安装方向与预紧未记录"),
 (r"Bearing 7211",               "滚珠丝杠与支承", "—", "7211 角接触球轴承", "Y 旋转螺母支承对之一"),
 (r"Bearing 6904",               "滚珠丝杠与支承", "—", "6904 深沟球轴承 20×37×9", "X 丝杠游动端"),
 (r"Bearing 63004",              "滚珠丝杠与支承", "—", "63004 深沟球轴承 20×42×16", "X 丝杠固定端"),
 (r"Bearing 3804",               "滚珠丝杠与支承", "—", "3804 双列角接触球轴承", "Z 丝杠端"),
 (r"T6M80-1000",                 "伺服电机与驱动", "StepperOnline / Rtelligent", "T6M80-1000H2A3-M17S",
  "220 VAC 1 kW、额定 3.19 N·m / 峰值 9.56 N·m、3000/5000 rpm、17 位编码器、IP65"),
 (r"T6M60-400",                  "伺服电机与驱动", "StepperOnline / Rtelligent", "T6M60-400H2A3-M17S",
  "220 VAC 400 W、60 法兰；是否带抱闸未记录"),
 (r"Motor Driver T6-1000RS",     "伺服电机与驱动", "StepperOnline / Rtelligent", "T6-1000RS",
  "连续 7.0 Arms / 峰值 26.5 A，脉冲 + RS485"),
 (r"Motor Driver T6-400RS",      "伺服电机与驱动", "StepperOnline / Rtelligent", "T6-400RS", "Z 轴驱动器"),
 (r"ATC Spindle RATTMOTOR|RTM120x103", "主轴与刀库", "RATTMOTOR", "RTM120x103-30-24/4.5",
  "220 V、11 A、800 Hz、4.5 kW(S1)/5.4 kW(S6)、12000/24000 rpm、BT30 气动拉爪、内置 PTC140、风冷"),
 (r"VFD RTMD120X103",            "主轴与刀库", "（随主轴配套）", "BOM 记 RTMD120X103-30-24/4.5",
  "该编码是主轴型号；变频器本身型号/功率未记录 —— 需确认"),
 (r"BT30 Tool Holder",           "主轴与刀库", "—", "BT30 刀柄", "7/24 锥、法兰 Ø46；14 位刀库"),
 (r"Pulley Mädler 5M 72T",       "同步带与带轮", "Mädler", "HTD 5M 72T 15 mm（Art. 17237200）", "Y 轴旋转螺母带轮，减速比 3:1"),
 (r"Pulley HTD 48T 5M",          "同步带与带轮", "—", "HTD 5M 48T 15 mm", "X 丝杠端带轮，减速比 2:1"),
 (r"Pulley HTD 5M 24T",          "同步带与带轮", "—", "HTD 5M 24T", "X/Y 电机带轮"),
 (r"Pulley HTD 3M 72T",          "同步带与带轮", "—", "HTD 3M 72T 15 mm", "Z 丝杠端带轮，减速比 2.4:1"),
 (r"Pulley HTD 3M 30T",          "同步带与带轮", "—", "HTD 3M 30T 15 mm", "Z 电机带轮"),
 (r"Belt GT3 460-5MGT",          "同步带与带轮", "Gates", "PowerGrip GT3 460-5MGT-15", "节线长 460 mm、5M 齿距、宽 15 mm"),
 (r"Belt GT3 387-3MGT|387-3MGT", "同步带与带轮", "Gates", "PowerGrip GT3 387-3MGT-15", "BOM 名混用 GT3/3MGT，齿型存疑"),
 (r"Belt 294-3MGT",              "同步带与带轮", "Gates", "PowerGrip GT3 294-3MGT-15", "Z 轴同步带"),
 (r"Cable Chain 18x37",          "拖链线槽过线", "—", "Cable Chain 18×37，L=3000 mm", "Y 轴随动拖链"),
 (r"Cable Chain CK18|CK18 18x60","拖链线槽过线", "—", "CK18 18×60 mm", "X 轴随动拖链"),
 (r"Cable Duct 45X60",           "拖链线槽过线", "—", "线槽 45×60，L=1000 mm", "柜内布线槽，BOM 13 根"),
 (r"Cable Gland",                "拖链线槽过线", "—", "电缆密封接头（M8/M12×1/M16/M18/M20/M25）", "过线密封"),
 (r"Cable Tie",                  "拖链线槽过线", "—", "尼龙扎带", "HowTo H3；规格未记录"),
 (r"Piston 14-28 Hub 600",       "气弹簧",       "Gasfedershop（同系列）", "Gasdruckfeder 14-28 Hub 600",
  "杆 Ø14 / 缸 Ø28 / 行程 600 mm、全长约 1248 mm、两端 M10×1.5；**力值(N) 未记录**"),
 (r"Piston MAL16x150",           "气动元件",     "Airtac 同规格", "MAL16×150",
  "缸径 Ø16、行程 150 mm、M5 气口、双作用、0.1–1 MPa"),
 (r"4V210-08",                   "气动元件",     "Airtac", "4V210-08 5/2 12VDC",
  "P/A/B=G1/4、R/S=G1/8、有效通径 17 mm²、Cv≈1.0"),
 (r"Solenoid Valve 2 Way|2V025-08", "气动元件",  "Airtac", "2V025-08 2/2 常闭",
  "通径 Ø2.5 mm、黄铜阀体、氟胶密封、起动压差 <0.05 MPa"),
 (r"162763|FRC-1/4-D-7-MINI",    "气动元件",     "Festo", "FRC-1/4-D-7-MINI-A（订货号 162763）",
  "G1/4、40 µm、0.5–7 bar、1300 l/min、自动排水"),
 (r"8002325|LFR-1/4-D-5M-MINI",  "气动元件",     "Festo", "LFR-1/4-D-5M-MINI-A-MPA（订货号 8002325）",
  "G1/4、40 µm、0.5–12 bar、约 1400 l/min、带表与旋钮锁"),
 (r"MLH100PGB01A",               "传感器与限位", "Honeywell", "MLH100PGB01A", "压力传感器，换刀气路联锁"),
 (r"Pressure Regulator Single",  "气动元件",     "—", "单体调压阀", "各支路调压；设定压力未记录"),
 (r"Pressure Rel[ea]se|Pressure Release", "气动元件", "—", "安全泄压阀", "每条调压支路 1 只，共 7 只"),
 (r"Flow_Control|Flow Control",  "气动元件",     "—", "单向节流阀", "气缸速度调节"),
 (r"Coolant Unit YS-BPV-3000",   "气动元件",     "—", "YS-BPV-3000", "雾化冷却单元"),
 (r"^Coolant system$|^Coolant System$", "气动元件",     "—", "冷却总成（调压 + 泄压 + 2 只手动二通球阀 + 流量控制 + 储液杯）", "型号未记录"),
 (r"CNC_OIL_PUMP",               "气动元件",     "—", "电动定时集中润滑泵", "型号未记录"),
 (r"Push-To-Connect Connector 1/4 MNPT|Press Fit Connector 1-4 MNPT", "气动元件", "—",
  "1/4 MNPT × Ø6 mm 快插接头", "螺纹端按 HowTo H8 缠生料带"),
 (r"Press Fit Connector 90|90 quick fitting", "气动元件", "—", "Ø6 mm 90° 快插弯头", ""),
 (r"Press Fit Connector 6mm T|NPT TEE", "气动元件", "—", "Ø6 mm 三通 / 1/4\" NPT 三通", ""),
 (r"Fitting push lock silincer","气动元件",     "—", "带消声器的外螺纹快插排气接头", ""),
 (r"Pipe 6mm",                   "气动元件",     "—", "Ø6 mm 气管（BOM 按 20 m 卷）", "PU 气管；CAD 切成 32 段"),
 (r"Caster 500Kg",               "脚轮",         "—", "500 kg 重载脚轮", "8 只 = 4 t 承载；是否带刹车未记录"),
 (r"Hinge GN 237",               "门窗五金",     "Elesa+Ganter", "GN 237-ZD-60-60-A-SW",
  "锌合金压铸 60×60、A 型 2×2 沉孔、RAL 9005、AISI 303 销"),
 (r"Lock Keyed Cabinet",         "门窗五金",     "—", "柜锁（带钥匙）", "每扇后门 2 只"),
 (r"32x366mm Handle",            "门窗五金",     "—", "拉手 32×366 mm", "升降窗拉手，粘接 + 螺钉"),
 (r"Side Handle",                "门窗五金",     "—", "侧拉手", "后门用；粘接固定"),
 (r"Magnet 80x20",               "门窗五金",     "—", "块状磁铁 80×20×4 mm，约 18 kg 吸力", "机架侧 5 只螺钉、窗扇侧 5 只粘接"),
 (r"Window Angle Press Fit",     "型材连接件",   "—", "30×30 型材压入式角件", "橡皮锤敲入，不用螺钉"),
 (r"Inductive Sensor SN04",      "传感器与限位", "—", "SN04-N 电感式接近开关", "三轴归零/硬限位，$27=5.000 mm 脱离量"),
 (r"Emergency Button RND",       "电控柜电气件", "RND", "RND 210-00414", "24 V 急停回路"),
 (r"Breaker 5SL6510",            "电控柜电气件", "Siemens", "5SL6510-7", "MCB 1+N C10 6 kA，3 路"),
 (r"Differential 5SV3312",       "电控柜电气件", "Siemens", "5SV3312-6", "RCCB 25 A / 30 mA / A 型 / 2 极，3 路"),
 (r"3RT2016",                    "电控柜电气件", "Siemens", "3RT2016-1BB41（+3RH2911-1HA01 辅助触头）", "接触器"),
 (r"3RH2911",                    "电控柜电气件", "Siemens", "3RH2911-1HA01", "辅助触点块"),
 (r"Relay E290",                 "电控柜电气件", "ABB", "E290 16 A", "模数化安装继电器 ×8"),
 (r"Power supply NDR-480",       "电控柜电气件", "Mean Well", "NDR-480-24 / NDR-480-12", "24 V 20 A 480 W / 12 V，DIN 导轨式"),
 (r"Power supply HDR-15",        "电控柜电气件", "Mean Well", "HDR-15", "辅助小电源"),
 (r"DIN RAIL PERFORATED",        "电控柜电气件", "—", "35 mm 冲孔 DIN 导轨", "长/短各一根"),
 (r"USB-UHB-4U",                 "电控柜电气件", "Waveshare", "UHB-4U", "USB 集线器"),
 (r"Screen 13.3inch Waveshare",  "电控柜电气件", "Waveshare", "13.3″ IPS 电容触摸屏", "1920×1080，HDMI + USB 触控"),
 (r"Intel_Nuc",                  "电控柜电气件", "Intel", "NUC", "运行 OLOS 的工控机"),
 (r"PCB Controller",             "电控柜电气件", "InMachines（自研）", "PCB Controller",
  "端子：POWER/LASER CTL/PROBE/SP CTL/SP EN/Safety CTL/Door/Air/Coolant"),
 (r"Key Switch",                 "电控柜电气件", "—", "钥匙开关", ""),
]
DECODE = [(re.compile(p, re.I), fam, sup, mod, spec) for p, fam, sup, mod, spec in DECODE]

FAMILY_FALLBACK = [
 (r"^Bracket\s+\d|Connector Plate|Reinforcement Plate|Window (Corner|Middle) Connector|Window Angle", "型材连接件"),
 (r"\bBeam\b|\bProfile\b|Corner Profile|L-shape", "机架与窗框型材"),
 (r"Terminal|Bridge \d|Relay|Breaker|Differential|Power (Supply|Switch)|Button|Transistor|PCB|"
  r"DIN Rail|Driver|VFD|Display|Screen|USB|Emergency|Logic", "电控柜电气件"),
 (r"Cable|Wire|Tube|Pipe|Jumper|Mains|GND|Grommet|Tie|Duct|Chain|Gland", "拖链线槽过线"),
 (r"Valve|Regulator|Fitting|Push Fit|Coolant|Oil|Pneumat|Air |Piston|Brush|Sensor Tool", "气动元件"),
 (r"Window|Door|Handle|Lock|Hinge|Magnet|Key", "门窗五金"),
 (r"Ball Screw|Bearing|Nut Spacer|Attachment Nut", "滚珠丝杠与支承"),
 (r"Linear Guide|Carriage", "直线导轨与滑块"),
 (r"Pulley|Belt", "同步带与带轮"),
 (r"Spindle|Tool Holder|BT30", "主轴与刀库"),
 (r"Motor|Driver", "伺服电机与驱动"),
 (r"Bed |Shoulder|Gantry|Roof|Side |Back |Front |Base |Head |Box|Panel|Cover|Plate", "机架与外罩结构件"),
]
FAMILY_FALLBACK = [(re.compile(p, re.I), f) for p, f in FAMILY_FALLBACK]

def family_of(*names):
    if names and names[0] and parse_fastener(names[0]): return "紧固件"
    for n in names:
        if not n: continue
        for rx, f in FAMILY_FALLBACK:
            if rx.search(n): return f
    return ""

def decode_purchased(*names):
    for n in names:
        if not n: continue
        for rx, fam, sup, mod, spec in DECODE:
            if rx.search(n):
                return {"family": fam, "supplier": sup, "model": mod, "spec": spec}
    return None

# ---------------------------------------------------------------- 分类
CAT_STD   = "标准件"
CAT_BUY   = "外购件"
CAT_MOD   = "外购改制件"
CAT_MAKE  = "非标自制件"
CAT_CABLE = "线缆与耗材"
CAT_UNK   = "待定"

PROFILE_RX  = re.compile(r"(\d+)\s*x\s*(\d+)\s*x\s*(\d+)\s*mm", re.I)
PROFILE2_RX = re.compile(r"(\d+)\s*x\s*(\d+)\b", re.I)
RESIN_RX   = re.compile(r"\(Resin printed\)|Resin\s*3?D?\s*[Pp]rinted", re.I)
MILL_RX    = re.compile(r"\(CNC Milled\)|CNC\s*milled", re.I)
PROFI_RX   = re.compile(r"CNC\s*Profi", re.I)

# 手册名关键词 -> 分类（当 STEP 名不带加工标签时的回退判据）
KW_BUY = [
 "Linear Guide","Carriage","Bearing","Motor","Driver","Spindle","Pulley","Belt",
 "Chain","Duct","Gland","Tie","Piston","Valve","Regulator","Sensor","Fitting",
 "Caster","Hinge","Lock","Handle","Magnet","Display","Screen","Relay","Breaker",
 "Differential","Power Supply","Terminal","Transistor","Button","Switch","USB","DIN Rail",
 "Tool Holder","BT30","Oil Pump","Brush","Grommet","Standoff","Sticker","Endstop",
 "Clamp","Pump","VFD","Bridge","Push Fit Connector","T-fitting","Air Connector",
]
KW_CABLE = ["Cable","Wire ","Tube","Pipe","Jumper","Cord","Mains","GND","L-cable","N-cable",
            "V+ Cable","V- Cable","Tubes Bundle"]
# 手册名里表示"这是一件自制板/座/罩"的词（在外购件型号解码之后才使用）
KW_MADE = ["Plate","Cover","Panel","Holder","Support","Spacer","Attacher","Attachment",
           "Block","Fixer","Fillet","Mount","Backplate","Frame Left","Frame Right"]
BEAM_RX    = re.compile(r"\bBeam\b|Corner Profile|\bProfile\b", re.I)
BRACKET_RX = re.compile(r"^Bracket\s+\d+x\d+", re.I)
CONPLATE_RX= re.compile(r"Connector Plate|Reinforcement Plate", re.I)

def classify(man_name: str, step_names: list[str]) -> tuple[str, str, str]:
    """返回 (分类, 加工方式/材料, 判据)。判据链按可靠性从强到弱。"""
    f = parse_fastener(man_name)
    if f:
        return CAT_STD, "外购标准紧固件", f"手册名前缀 {f['prefix']}"
    joined = " | ".join(step_names)
    probe  = [man_name] + step_names
    # 1) 有标准代号的滚动轴承 / 滚珠螺母
    if re.search(r"\bBearing\s*(6904|63004|7210|7211|3804)\b", joined + " " + man_name, re.I):
        return CAT_STD, "外购标准滚动轴承", "名称含标准轴承代号"
    # 2) 铝型材角件（外购标准角码）
    if BRACKET_RX.search(man_name):
        return CAT_BUY, "外购铝型材角件（角码）", "手册名 Bracket AxB"
    # 3) 连接板 / 加强板：CNC 铣削自制（H 线未把它们列为 purchased_modules）
    if CONPLATE_RX.search(man_name):
        return CAT_MAKE, "CNC 铣削钢/铝板（梁端内置螺母板，材料未记录）", "手册名含 Connector Plate / Reinforcement Plate"
    # 4) 型材按长度下料（方管、角材、窗框型材、龙门梁）
    if any(PROFILE_RX.search(sn) and re.search(r"Profile|Beam|Corner Profile|L-shape", sn, re.I)
           for sn in probe):
        return CAT_MOD, "外购铝方管/型材按长度下料 + CNC 铣孔", "STEP 名含截面 AxBxC mm 且为 Profile/Beam"
    if (BEAM_RX.search(man_name) and
            not re.search(r"Cable|Wire|Plate|Cover|Attacher|Holder|Spacer", man_name, re.I)):
        return CAT_MOD, "外购铝方管/型材按长度下料 + CNC 铣孔", "手册名含 Beam / Profile"
    if re.search(r"^L-shape", man_name):
        return CAT_MOD, "外购角材按长度下料", "手册名 L-shape profile"
    if PROFI_RX.search(joined):
        return CAT_MOD, "外购丝杠毛坯 + CNC Profi 端部加工", "STEP 名含 CNC Profi"
    if re.search(r"DIN Rail|DIN RAIL", man_name + joined):
        return CAT_MOD, "外购 35 mm DIN 导轨按长度截断", "手册名 DIN Rail Long/Short"
    if re.search(r"Cable Duct \d", man_name):
        return CAT_MOD, "外购线槽按长度截断", "手册名 Cable Duct + 长度"
    if re.search(r"\bChannel\b", man_name) and any(PROFILE_RX.search(sn) for sn in step_names):
        return CAT_MOD, "外购走线槽按长度截断", "手册名含 Channel 且 STEP 名带截面"
    # 5) STEP 件名自带的加工标签（最可靠的自制判据）
    if RESIN_RX.search(joined):
        return CAT_MAKE, "光固化树脂 3D 打印（Resin printed）", "STEP 名含 Resin printed"
    if MILL_RX.search(joined):
        return CAT_MAKE, "CNC 铣削（板材/型材，材料未记录，推断为铝合金）", "STEP 名含 CNC milled"
    # 6) 命中外购件型号解码表
    if decode_purchased(*probe):
        return CAT_BUY, "整件外购", "命中 H 线外购件家族解码表"
    # 7) 线缆与管路耗材
    if any(k.lower() in man_name.lower() for k in KW_CABLE):
        return CAT_CABLE, "按线号/长度自制线束或按长度剪裁", "手册名含线缆/管路关键词"
    # 8) 板/座/罩类自制件（无 STEP 加工标签时的回退）
    if any(k.lower() in man_name.lower() for k in KW_MADE):
        return CAT_MAKE, "自制（加工方式未记录，按板/座件推断为 CNC 铣削）", "手册名为板/盖/座/垫类，STEP 无加工标签"
    # 9) 外购件关键词
    if any(k.lower() in man_name.lower() for k in KW_BUY):
        return CAT_BUY, "整件外购", "手册名含外购件关键词"
    return CAT_UNK, "", "无判据"

# ---------------------------------------------------------------- 几何（下料长度）
print("[2/9] 计算 STL 包围盒 …", file=sys.stderr)
NAME2STEM = {}
for stem, v in MANIFEST.items():
    NAME2STEM.setdefault((v.get("name") or "").strip(), stem)
if CACHE.exists():
    AABB = json.load(open(CACHE, encoding="utf-8"))
else:
    AABB = {}
    for p in sorted(P_MESH.glob("*.stl")):
        e = stl_extents(p)
        if e: AABB[p.stem] = e
    json.dump(AABB, open(CACHE, "w", encoding="utf-8"))

def extents_for(step_name: str):
    """按 STEP 定义名（含 `X [self]#bNNN` 体名）找 STL 包围盒 [dx,dy,dz]（mm）。"""
    for cand in (step_name, step_name + " ", " " + step_name):
        stem = NAME2STEM.get(cand)
        if stem and stem in AABB: return AABB[stem]
    for nm, stem in NAME2STEM.items():
        if base_name(nm) == step_name and stem in AABB:
            return AABB[stem]
    return None

# ---------------------------------------------------------------- 官方 BOM 对齐
print("[3/9] 对齐官方 BOM …", file=sys.stderr)
BOM_BY_NAME = collections.Counter()
BOM_GROUP_OF = {}
for r in BOM_ROWS:
    BOM_BY_NAME[r["name"]] += r["qty"]
    BOM_GROUP_OF.setdefault(r["name"], r["group"])
BOM_BY_BASE = collections.defaultdict(list)     # STEP 基名 -> [官方 BOM 行名]
for n in BOM_BY_NAME:
    BOM_BY_BASE[base_name(n)].append(n)
BOM_BY_NORM = collections.defaultdict(int)
BOM_NORM_SRC = {}
for n, q in BOM_BY_NAME.items():
    BOM_BY_NORM[norm_key(n)] += q
    BOM_NORM_SRC.setdefault(norm_key(n), n)

# 官方 BOM 行 -> STEP 基名（人工核定的近似行）
BOM_STEP_OVERRIDE = {
 "Cable Duct 45X60 1000mm":              "Cable Duct 45X60",
 "Power supply NDR-480-24":              "Power supply NDR-480-3D",
 "Power supply NDR-480-12":              "Power supply NDR-480-3D",
 "Differential 5SV3312-6":               "Differential 5SV3312-6_G_I202_XX_22847V",
 "Relay E290 16A":                       "Relay E290 16A",
 "Power supply HDR-15":                  "Power supply HDR-15",
 "USB-UHB-4U Waveshare":                 "USB-UHB-4U Waveshare",
 "Linear Guide HGR25 R C 1620mm":        "Linear Guide Cart HGR25 R C 1620mm",
 "Attachment Nut 2510":                  "X Attachment Nut 2510",
 "Cable Chain 18x37 3000mm":             "Cable Chain 18x37",
 "Y Linear Guide HGR25 2945mm":          "Y Linear Guide HGR25",
 "Pulley Mädler 5M 72T 15mm":            "Pulley Mädler 5M 72T 15mm Tight Fit",
 "Y Cable Channel 70x30x3mm":            "Y Cable Channel 70x30x3mm",
 "Y Linear Guide HGR25520mm":            "Z Linear Guie HGR25 520mm",
 "Gantry Wire Fixer":                    "Wire Fixer",
 "Pipe 6mm 20m":                         "Pipe 6mm",
 "Pressure Release Valve":               "Pressure Relese Valve",
 "6mm 90 Press Fit Connector":           "Press Fit Connector 90",
 "6mm T Press Fit Connector":            "Press Fit Connector 6mm T",
 "Press Fit Connector 1-4 MNPT x 6mm":   "Push-To-Connect Connector 1/4 MNPT x 6mm",
 "Solenoid Valve 5 Way":                 "4V210-08 5/2 12VDC Solenoid Valve",
 "Flow Control Valve":                   "Flow_Control",
 "Pressure Regulator 8002325 LFR-1_4-D-5M-MINI-A-MPA": "8002325 LFR-1/4-D-5M-MINI-A-MPA---",
 "Pressure Regulator with Filter 162763 FRC-1_4-D-7-MINI-A": "162763 FRC-1/4-D-7-MINI-A",
 "Pressure Sensor MLH100PGB01A":         "MLH100PGB01A",
 "Pneumatic fitting 6 to 4 mm tube":     "Фитинг переходник с 6 мм на 4 мм",
 "Linear Guide HGR15  820mm":            "Linear Guide Rail HGR15 X2",
 "Linear Guide HGR15  2700mm":           "Linear Rail HGR15",
 "Linear Guide Cart HGH15C":             "Linear Guide Cart HGH15C X4",
 "Handle 32x366mm":                      "32x366mm Handle",
 "Pneumatics Back Plate (CNC milled)":   "Pneumatics Back Plate",
 "Back Panel (CNC Milled)":              "Back Panel (CNC milled)",
 "Left Window (CNC Milled)":             "Left Window (CNC milled)",
 "Lock Keyed Cabinet":                   "Lock Keyed Cabinet",
 "Piston 14-28 Hub 600":                 "Piston 14-28 Hub 600",
 "Side Handle":                          "Side Handle",
 "Handle Attachment":                    "Handle Attachment",
 "Shoulder Flat Spacer Left (Resin printed)": "Shoulder Flat Spacer Left (Resin printed)",
 "Shoulder Cover (Resin Printed)":       "Shoulder Cover (Resin printed)",
 "2 Way Valve Mount":                    "2 Way Valve Mount",
 "Double Valve Mount":                   "Double Valve Mount",
}
def bom_qty_for_step(stepnames: list[str]) -> tuple[int, list[str]]:
    """按 STEP 基名回查官方 BOM 数量。"""
    tot, srcs = 0, []
    for b in stepnames:
        hits = []
        if b in BOM_BY_BASE:                       # 同基名的全部 BOM 行（含 (1)/(2) 变体）
            hits = list(BOM_BY_BASE[b])
        if not hits:
            for bn, sb in BOM_STEP_OVERRIDE.items():
                if base_name(sb) == b and bn in BOM_BY_NAME: hits = [bn]; break
        if not hits and norm_key(b) in BOM_BY_NORM:
            hits = [BOM_NORM_SRC[norm_key(b)]]
        for h in hits:
            if h not in srcs:
                tot += BOM_BY_NAME[h]; srcs.append(h)
    return tot, srcs

# ---------------------------------------------------------------- 主清单装配
print("[4/9] 装配主清单 …", file=sys.stderr)
ROWS = []
for man in MAN_NAMES:
    if man == "None":     # Workbook 的空零件行占位
        continue
    qty_man = MAN_QTY[man]
    s3 = STEPNAME_BY_MAN.get(man)
    if s3:
        stepnames = [n for n, _ in s3.most_common()]
        link = "S3绑定"
    elif man in ALIAS_MANUAL:
        stepnames = [base_name(ALIAS_MANUAL[man])]
        link = "人工对名(推断)"
    else:
        stepnames = []
        link = "无对应"
    body_only = [n for n in stepnames if "[self]#" in n]
    named     = [n for n in stepnames if "[self]#" not in n]
    show_step = ("；".join(named) if named else
                 (f"{len(body_only)} 个顶层装配自体体（{body_only[0].split('#')[0]}#…）" if body_only else ""))
    movers = MOVERS_BY_MAN.get(man, 0)
    if not movers and link == "人工对名(推断)":
        movers = sum(step_instances(n) for n in stepnames)
    step_all = sum(step_instances(n) for n in named) if named else \
               (len(body_only) if body_only else 0)
    bom_q, bom_src = 0, []
    f = parse_fastener(man)
    if f:
        bn = bom_name_for_fastener(man)
        if bn in BOM_BY_NAME:
            bom_q = BOM_BY_NAME[bn]; bom_src = [bn]
    else:
        if man in BOM_BY_BASE:
            bom_src = list(BOM_BY_BASE[man]); bom_q = sum(BOM_BY_NAME[x] for x in bom_src)
        else:
            bom_q, bom_src = bom_qty_for_step(named)
            if not bom_src and norm_key(man) in BOM_BY_NORM:
                bom_src = [BOM_NORM_SRC[norm_key(man)]]; bom_q = BOM_BY_NORM[norm_key(man)]
    cat, proc, basis = classify(man, stepnames)
    dec = decode_purchased(*(named + [man])) or {}
    # 规格解码
    if f:
        spec = (f"螺纹 {f['thread']}" + (f" × 长度 {f['length']} mm" if f['length'] else "") +
                f"；头型：{f['head']}")
        std, mat, tool = f["std"], f["material"], f["tool"]
    else:
        spec = dec.get("spec", "")
        std, mat, tool = "", "", ""
        if cat == CAT_STD: std = "见规格解码"
    # 外形尺寸与下料长度（取 STEP 定义体 STL 包围盒）
    geo_src = (named or body_only)
    e = extents_for(geo_src[0]) if geo_src else None
    dims = f"{e[0]:.0f}×{e[1]:.0f}×{e[2]:.0f} mm" if e else ""
    cut = ""
    if cat == CAT_MOD and e:
        cut = f"下料长 {max(e):.0f} mm"
    geom = "有" if (named or body_only) else "无"
    holds = HOLDS_BY_MAN.get(man, 0)
    if geom == "无" and holds:
        geom = f"无（手册 GLB 有 {holds} 个网格，STEP 无几何）"
    steps = sorted(set(MAN_STEPS[man]), key=lambda s: [int(x) for x in re.findall(r"\d+", s)])
    # 数量一致性
    flags = []
    if bom_q == 0:                      flags.append("官方BOM缺此行")
    elif bom_q != qty_man:              flags.append(f"BOM≠手册({bom_q}≠{qty_man})")
    if movers and movers != qty_man:    flags.append(f"STEP≠手册({movers}≠{qty_man})")
    if not movers and geom.startswith("无"): flags.append("STEP无几何")
    if not movers and geom == "有":     flags.append("STEP有几何但未绑定工序")
    ROWS.append({
        "手册名(英文原名)": man,
        "STEP名(英文原名)": show_step,
        "名称对应来源": link,
        "分类": cat,
        "家族/子类": dec.get("family") or family_of(man, *named),
        "手册数量": qty_man,
        "STEP实例数(工序绑定)": movers,
        "STEP实例数(全模型)": step_all,
        "官方BOM数量": bom_q,
        "官方BOM行名": "；".join(bom_src),
        "官方BOM分组": "；".join(sorted({BOM_GROUP_OF.get(s, "") for s in bom_src} - {""})) if bom_src else "",
        "数量一致性": "一致" if not flags else "；".join(flags),
        "规格解码": spec,
        "建议标准号": std,
        "材料/等级(推断)": mat,
        "拧紧工具(推断)": tool,
        "供应商": dec.get("supplier", ""),
        "型号/订货号": dec.get("model", ""),
        "加工方式/材料": proc,
        "外形尺寸(STEP包围盒)": dims,
        "下料尺寸": cut,
        "STEP几何": geom,
        "所在工序(步号)": "、".join(steps),
        "工序数": len(steps),
        "分类判据": basis,
        "备注/存疑": KNOWN_MISBIND.get(man, ""),
    })

# STEP 有、手册无的机器级件，追加到主清单（分类照常，手册数量 0）
for (cls, b, top), n in sorted(UNBOUND_MACHINE.items(), key=lambda x: (-x[1], x[0][1])):
    if cls == "SELF_BODIES_UNBOUND":
        continue
    dec = decode_purchased(b) or {}
    cat, proc, basis = classify(b, [b])
    e = extents_for(b)
    cut = f"下料长 {max(e):.0f} mm" if (cat == CAT_MOD and e) else ""
    bq, bsrc = bom_qty_for_step([b])
    ROWS.append({
        "手册名(英文原名)": "—（手册未列）",
        "STEP名(英文原名)": b, "名称对应来源": f"STEP独有({cls})",
        "分类": cat, "家族/子类": dec.get("family") or family_of(b),
        "手册数量": 0, "STEP实例数(工序绑定)": 0, "STEP实例数(全模型)": n,
        "官方BOM数量": bq, "官方BOM行名": "；".join(bsrc),
        "官方BOM分组": "；".join(sorted({BOM_GROUP_OF.get(s, "") for s in bsrc} - {""})) if bsrc else "",
        "数量一致性": "手册未列此件",
        "规格解码": dec.get("spec", ""), "建议标准号": "", "材料/等级(推断)": "", "拧紧工具(推断)": "",
        "供应商": dec.get("supplier", ""), "型号/订货号": dec.get("model", ""),
        "加工方式/材料": proc, "外形尺寸(STEP包围盒)": (f"{e[0]:.0f}×{e[1]:.0f}×{e[2]:.0f} mm" if e else ""),
        "下料尺寸": cut, "STEP几何": "有",
        "所在工序(步号)": "", "工序数": 0,
        "分类判据": f"{basis}；顶层装配 {top}",
        "备注/存疑": "手册 114 步未列此件",
    })

# 一条官方 BOM 行可能同时服务多个手册件（如 Linear Guide Cart HGH25CAZA 被 X/Y/Z Carriage 三行共用）——
# "官方BOM数量"列因此不可直接求和，这里给共用行打标记并另算去重合计。
BOM_ROW_USERS = collections.Counter()
for _r in ROWS:
    for _b in (_r["官方BOM行名"] or "").split("；"):
        if _b: BOM_ROW_USERS[_b] += 1
for _r in ROWS:
    shared = [b for b in (_r["官方BOM行名"] or "").split("；") if b and BOM_ROW_USERS[b] > 1]
    if shared:
        note = f"官方 BOM 行 `{'、'.join(shared)}` 由 {max(BOM_ROW_USERS[b] for b in shared)} 个手册件共用，本列不可求和"
        _r["备注/存疑"] = (_r["备注/存疑"] + "；" + note) if _r["备注/存疑"] else note
BOM_LINKED = set(BOM_ROW_USERS)

CAT_ORDER = {CAT_STD: 0, CAT_BUY: 1, CAT_MOD: 2, CAT_MAKE: 3, CAT_CABLE: 4, CAT_UNK: 5}
ROWS.sort(key=lambda r: (CAT_ORDER.get(r["分类"], 9), r["家族/子类"], r["手册名(英文原名)"]))
for i, r in enumerate(ROWS, 1):
    r["序号"] = i

# ---------------------------------------------------------------- 差异表
print("[5/9] 生成差异表 …", file=sys.stderr)
DIFF_BOM_MISSING, DIFF_QTY, DIFF_STEP_ONLY, DIFF_BOM_EXTRA = [], [], [], []

for r in ROWS:
    if r["手册数量"] > 0 and r["官方BOM数量"] == 0:
        DIFF_BOM_MISSING.append({
            "手册名": r["手册名(英文原名)"], "STEP名": r["STEP名(英文原名)"],
            "分类": r["分类"], "家族/子类": r["家族/子类"],
            "手册数量": r["手册数量"], "STEP实例数": r["STEP实例数(工序绑定)"],
            "建议标准号/型号": r["建议标准号"] or r["型号/订货号"],
            "所在工序": r["所在工序(步号)"],
            "说明": "官方 BOM_sheet1.csv 中查不到对应行（按手册名、STEP 名与规范化名三路回查）",
        })
    a, b, c = r["手册数量"], r["STEP实例数(工序绑定)"], r["官方BOM数量"]
    if r["手册数量"] > 0 and ((b and b != a) or (c and c != a)):
        DIFF_QTY.append({
            "手册名": r["手册名(英文原名)"], "STEP名": r["STEP名(英文原名)"],
            "分类": r["分类"], "手册数量": a, "STEP实例数(工序绑定)": b,
            "STEP实例数(全模型)": r["STEP实例数(全模型)"], "官方BOM数量": c,
            "手册-STEP差": (b - a) if b else "", "手册-BOM差": (c - a) if c else "",
            "名称对应来源": r["名称对应来源"], "所在工序": r["所在工序(步号)"],
        })

for (cls, b, top), n in sorted(UNBOUND_MACHINE.items(), key=lambda x: (-x[1], x[0][1])):
    dec = decode_purchased(b) or {}
    kind = ("真实缺失：CAD 有件，手册 114 步中没有任何一步装它"
            if cls == "NOT_IN_MANUAL_GLB" else
            "STEP 独有紧固件/螺母（手册零件栏未列）" if cls == "FASTENER_STEP_ONLY" else
            "顶层装配自体的未绑定体（Electrics 面板附件）")
    DIFF_STEP_ONLY.append({
        "STEP名": b, "class": cls, "顶层装配": top, "STEP实例数": n,
        "折叠判定": "机器级（不属任何 depth-2 预装模块）",
        "家族/子类": dec.get("family", ""), "型号": dec.get("model", ""),
        "性质": kind,
    })
for mod, n in UNBOUND_FOLDED.most_common():
    dec = decode_purchased(mod) or {}
    DIFF_STEP_ONLY.append({
        "STEP名": mod, "class": "FOLDED_INTO_PURCHASED_MODULE", "顶层装配": STEP_TOP.get(mod, ""),
        "STEP实例数": n, "折叠判定": f"外购件内部子体，已折叠计入模块（{n} 个内部叶）",
        "家族/子类": dec.get("family", ""), "型号": dec.get("model", ""),
        "性质": "非缺失：采购模块的内部零件，不进采购清单",
    })

MAN_KEYS = {norm_key(m) for m in MAN_NAMES}
MAN_KEYS |= {norm_key(bom_name_for_fastener(m)) for m in MAN_NAMES}
used_bom = set()
for r in ROWS:
    for s in (r["官方BOM行名"] or "").split("；"):
        if s: used_bom.add(s)
for n, q in sorted(BOM_BY_NAME.items()):
    if n in used_bom: continue
    DIFF_BOM_EXTRA.append({
        "官方BOM行名": n, "分组": BOM_GROUP_OF.get(n, ""), "BOM数量": q,
        "STEP同名实例数": step_instances(base_name(n)),
        "说明": "官方 BOM 有此行，但未能连到任何手册零件行（手册可能用了别名，或该件不进装配步）",
    })

# ---------------------------------------------------------------- 汇总表
print("[6/9] 生成汇总表 …", file=sys.stderr)
SUM_FAST = []
for man in MAN_NAMES:
    f = parse_fastener(man)
    if not f: continue
    q = MAN_QTY[man]
    bn = bom_name_for_fastener(man)
    bq = BOM_BY_NAME.get(bn, 0)
    pack = PACK.get(f["thread"], 100)
    need = math.ceil(q * 1.10 / pack) * pack        # 留 10% 损耗，按包装取整
    SUM_FAST.append({
        "手册规格名": man, "官方BOM行名": bn if bq else f"（缺行）{bn}",
        "螺纹": f["thread"], "长度mm": f["length"] or "",
        "头型": f["head"], "建议标准号": f["std"],
        "材料/等级(推断)": f["material"], "拧紧工具(推断)": f["tool"],
        "手册总用量": q, "官方BOM数量": bq, "差(手册-BOM)": q - bq,
        "建议包装数(推断)": pack, "建议采购量(含10%损耗,按包装取整)": need,
        "使用工序数": len(set(MAN_STEPS[man])),
    })
SUM_FAST.sort(key=lambda r: (r["建议标准号"], r["螺纹"], r["长度mm"] if r["长度mm"] != "" else 0))

SUM_BUY = collections.defaultdict(lambda: {"qty_man": 0, "qty_bodies": 0, "stepnames": set(),
                                            "bomnames": set(), "names": set(), "steps": set()})
for r in ROWS:
    if r["分类"] not in (CAT_BUY, CAT_STD): continue
    if r["分类"] == CAT_STD and parse_fastener(r["手册名(英文原名)"]): continue   # 紧固件另表
    mdl = r["型号/订货号"] or r["手册名(英文原名)"]
    if mdl == "—（手册未列）": mdl = r["STEP名(英文原名)"]
    key = (r["家族/子类"] or "未归类", r["供应商"] or "—", mdl)
    d = SUM_BUY[key]
    d["qty_man"] += r["手册数量"]
    if "顶层装配自体体" in (r["STEP名(英文原名)"] or ""):
        d["qty_bodies"] = d.get("qty_bodies", 0) + r["STEP实例数(工序绑定)"]
    for sn in (r["STEP名(英文原名)"] or "").split("；"):
        if sn and "[self]#" not in sn and "顶层装配自体体" not in sn: d["stepnames"].add(sn)
    for bn in (r["官方BOM行名"] or "").split("；"):
        if bn: d["bomnames"].add(bn)
    d["names"].add(r["手册名(英文原名)"])
    for st in (r["所在工序(步号)"] or "").split("、"):
        if st: d["steps"].add(st)
SUM_BUY_ROWS = []
for (fam, sup, mod), d in sorted(SUM_BUY.items()):
    SUM_BUY_ROWS.append({
        "家族": fam, "供应商": sup, "型号/订货号": mod,
        "手册数量": d["qty_man"],
        "STEP实例数": sum(step_instances(n) for n in d["stepnames"]) + d.get("qty_bodies", 0),
        "官方BOM数量": sum(BOM_BY_NAME.get(n, 0) for n in d["bomnames"]),
        "手册件名": "；".join(sorted(n for n in d["names"] if n != "—（手册未列）")) or "—（手册未列）",
        "STEP件名": "；".join(sorted(d["stepnames"])) or ("顶层装配自体体（Frame/Electrics [self]#…）" if d.get("qty_bodies") else ""),
        "官方BOM行名": "；".join(sorted(d["bomnames"])),
        "使用工序数": len(d["steps"]),
    })

SUM_MAKE = collections.defaultdict(lambda: {"kinds": 0, "qty_man": 0, "qty_step": 0, "names": []})
for r in ROWS:
    if r["分类"] != CAT_MAKE: continue
    proc = r["加工方式/材料"]
    d = SUM_MAKE[proc]
    d["kinds"] += 1; d["qty_man"] += r["手册数量"]; d["qty_step"] += r["STEP实例数(全模型)"]
    d["names"].append(r["手册名(英文原名)"] if r["手册名(英文原名)"] != "—（手册未列）" else r["STEP名(英文原名)"])
SUM_MAKE_ROWS = [{"加工方式": k, "品种数": v["kinds"], "手册总件数": v["qty_man"],
                  "STEP总实例数": v["qty_step"], "备注": "材料牌号手册与 BOM 均未记录（推断：机架件铝合金、树脂件光敏树脂）"}
                 for k, v in sorted(SUM_MAKE.items())]

CANON_SECTION = {
 "100x100": "100×100×5 mm 铝方管", "200x100": "200×100×5 mm 铝方管",
 "100x50": "100×50×5 mm 铝方管", "30x30": "30×30×2 mm 铝方管（窗框）",
 "20x7": "20×7 mm 铝角材", "60x40": "45×60 mm 线槽", "35x15": "35 mm 冲孔 DIN 导轨",
 "32x32": "Ø32 mm 滚珠丝杠（SFU3210）", "23x22": "HGR25 导轨", "90x30": "90×30×3 mm 走线槽",
}
SEC_ALIAS = {"100×100×5 mm": "100×100×5 mm 铝方管", "200×100×5 mm": "200×100×5 mm 铝方管",
             "100×50×5 mm": "100×50×5 mm 铝方管", "30×30×2 mm": "30×30×2 mm 铝方管（窗框）",
             "90×30×3 mm": "90×30×3 mm 走线槽"}
def canon_section(sec, dims):
    if sec and "推断" not in sec: return SEC_ALIAS.get(sec, sec)
    if dims:
        k = f"{int(round(dims[1]))}x{int(round(dims[0]))}"
        if k in CANON_SECTION: return CANON_SECTION[k]
        k2 = f"{int(round(dims[0]))}x{int(round(dims[1]))}"
        if k2 in CANON_SECTION: return CANON_SECTION[k2]
    return sec or "（名称与几何均未给截面）"

STEPNAME_USERS = collections.Counter()
for _r in ROWS:
    for _n in (_r["STEP名(英文原名)"] or "").split("；"):
        if _n and "[self]#" not in _n: STEPNAME_USERS[_n] += 1

SUM_PROFILE = []
for r in ROWS:
    if r["分类"] != CAT_MOD: continue
    names = [n for n in (r["STEP名(英文原名)"] or "").split("；") if n and "[self]#" not in n]
    sec = ""
    for cand in names + [r["手册名(英文原名)"]]:
        m = PROFILE_RX.search(cand)
        if m: sec = f"{m.group(1)}×{m.group(2)}×{m.group(3)} mm"; break
    if not sec:
        for cand in [r["手册名(英文原名)"]] + names:
            m = PROFILE2_RX.search(cand)
            if m: sec = f"{m.group(1)}×{m.group(2)} mm（名称给出）"; break
    L = ""; dims = None; src = "无几何"
    dm = r.get("外形尺寸(STEP包围盒)") or ""
    mm = re.findall(r"[\d.]+", dm)
    if len(mm) == 3:
        dims = sorted(float(x) for x in mm)
        L = round(dims[2], 1)
        src = "STEP 定义体包围盒"
        if names and STEPNAME_USERS.get(names[0], 0) > 1:
            src = "同名 STEP 定义（多个手册件共用，长度不可分辨）"
    sec = canon_section(sec, dims)
    SUM_PROFILE.append({
        "截面": sec or "（名称未给截面）", "手册名": r["手册名(英文原名)"],
        "STEP名": r["STEP名(英文原名)"], "下料长度mm": L, "长度来源": src,
        "手册数量": r["手册数量"], "STEP实例数": r["STEP实例数(全模型)"],
        "加工方式": r["加工方式/材料"], "所在工序": r["所在工序(步号)"],
    })
SUM_PROFILE.sort(key=lambda r: (r["截面"], -(r["下料长度mm"] if isinstance(r["下料长度mm"], (int, float)) else 0)))

SUM_PROFILE_AGG = collections.defaultdict(lambda: {"kinds": 0, "n": 0, "total_mm": 0.0})
for r in SUM_PROFILE:
    d = SUM_PROFILE_AGG[r["截面"]]
    d["kinds"] += 1
    n = r["手册数量"] or r["STEP实例数"] or 0
    d["n"] += n
    if isinstance(r["下料长度mm"], (int, float)): d["total_mm"] += r["下料长度mm"] * n
SUM_PROFILE_AGG_ROWS = [{"截面": k, "品种数": v["kinds"], "总根数": v["n"],
                         "总长度m(按包围盒估)": round(v["total_mm"] / 1000, 2),
                         "备注": ("该截面在 STEP 中没有几何，长度未知" if v["total_mm"] == 0 else
                                  "长度取 STEP 定义体包围盒最长边，含端部加工余量；采购应另加锯切余量（推断）")}
                        for k, v in sorted(SUM_PROFILE_AGG.items())]

# 工序×零件明细
OPS_SHEET = []
S3_OPS = {o["op"]: o for o in S3["operations"]}
for o in OPS_SOP:
    s3o = S3_OPS.get(o["op"], {})
    prmap = {p["sop_name"]: p for p in s3o.get("part_rows", [])}
    for p in o.get("parts", []):
        n = (p.get("name") or "").strip()
        pr = prmap.get(n, {})
        OPS_SHEET.append({
            "工序": o["op"], "步号": o["step"], "章": o["chapter"], "工序名": o["title"],
            "零件(手册名)": n, "数量": p.get("qty"),
            "手册GLB网格数": pr.get("glb_meshes", ""), "STEP绑定实例": pr.get("bound_occurrences", ""),
            "S3状态": pr.get("status", ""),
            "工具": o.get("tools_text") or "", "HowTo": "、".join(o.get("how_tos") or []),
        })

# ---------------------------------------------------------------- 待确认问题
QUESTIONS = [
 ("Q01", "紧固件标准号", "官方 BOM 写作 `ISO 3780`，该号并非螺钉标准。按头型（Button head）与逐规格 1:1 的数量对应，应为 `ISO 7380`。",
  "全部 28 行 B-screw（695 件）", "需 InMachines 确认是否笔误"),
 ("Q02", "HF-screw 标准", "`HF-screw` 全机 284 根（M12-20 ×268 + M12-60 ×16），是用量第一的紧固件；手册与 BOM 都没有给它标准号、头型和材料。",
  "HF-screw M12-20 ×268、M12-60 ×16", "无法采购，必须确认"),
 ("Q03", "BOM 紧固件缺行", "官方 BOM 的 Fasteners 分组只覆盖 ISO 3780 / DIN 912 / DIN 985 / 沉头 / 热塑五类的 68 行；"
  "手册用到的另外 13 个规格共 622 件在 BOM 中完全没有行。",
  "见 02_差异-BOM缺失 表", "照 BOM 采购会缺 622 件紧固件"),
 ("Q04", "结构接头缺行", "22 块 Connector Plate 与约 70 只 Bracket 角件在官方 BOM 中没有任何对应行（BOM 的 Frame 分组只有型材与脚轮）。",
  "Bracket 100x100/30x110/30x60/60x60、Frame/Base/Shoulder/Corner Connector Plate", "照 BOM 采购会缺 90+ 件结构接头"),
 (
  "Q05", "X 导轨规格", "BOM 记 `Linear Guide HGR25 R C 1620mm`，但手册用 50×C-screw M5-30 + 4×M5-40 压装；"
  "HGR25 沉孔为 M6，M5 对应 HGR20。27 个固定点在两种规格下都吻合 60 mm 孔距。",
  "X Linear Guide ×2", "选型不确定，影响滑块与螺钉采购"),
 ("Q06", "门导轨固定点数", "820 mm 门导轨在 WB-13 是每根 18 个 M4 固定点、在 WB-46.1 是 17 个；"
  "18 点对应约 45 mm 孔距，与 HGR15 标准 60 mm 不符。", "Door Linear Guide ×6", "需确认孔距与钻孔图"),
 ("Q07", "HGH15C 数量", "官方 BOM 19、手册 12（Door Carriage）+5（Roof Carriage）、CAD 18。三处不一致。",
  "Linear Guide Cart HGH15C", "需确认整机用量"),
 ("Q08", "滚珠螺母命名", "X 丝杠为 SFU2505（导程 5），螺母却名 `Ball Screw Nut 2510` / `X Attachment Nut 2510`（导程 10）。"
  "按 $100=640 步/mm 与 2:1 减速反算导程应为 5 mm。", "X Ball Screw Nut、X Attachment Nut", "疑为 CAD 命名笔误"),
 ("Q09", "支承轴承", "7210 ×2、7211 ×2、6904 ×2、63004 ×2、3804 共 9 个轴承只出现在官方 BOM 与 CAD，"
  "手册 114 步的零件栏一次都没有列；角接触轴承的安装方向（背对背/面对面）与预紧方式也没有任何说明。",
  "Bearing 7210/7211/6904/63004/3804", "装配工艺缺失，必须补"),
 ("Q10", "气弹簧力值", "6 支 `Piston 14-28 Hub 600` 只给了几何型号，没有力值（N）。三扇窗尺寸不同，是否共用同一力值不明。",
  "Window Piston ×6", "气弹簧选型最关键参数缺失"),
 ("Q11", "VFD 型号", "BOM 的 `VFD RTMD120X103-30-24/4.5` 用的是主轴型号编码，变频器本身的型号与功率没有记录。",
  "VFD Spindle Controller ×1", "无法采购"),
 ("Q12", "Z 轴抱闸", "Z 用 `T6M60-400H2A3-M17S`，型号中 A 通常表示不带抱闸；Z 轴无配重，断电时主轴头是否下坠没有说明。",
  "Z Motor ×1", "安全相关"),
 ("Q13", "扭矩与螺纹胶", "Workbook 全表没有任何扭矩字段与螺纹胶要求（HowTo H4 的内部批注写着 'Locktite should be mentioned or shown'）；"
  "树脂 3D 打印件（尤其位于 Y 向主传力路径的 Gantry Attacher Block）也没有拧紧扭矩上限。",
  "全部 2474 件紧固件、约 60 件树脂件", "工艺缺失"),
 ("Q14", "工具清单", "114 步中只有 WB-01.1 列了工具（Allen Key 6、Wrench 14），其余 113 步的工具全靠按紧固件规格推断。",
  "全表", "本清单已按螺纹规格推断内六角/扳手对边，需官方确认"),
 ("Q15", "铰链未进手册", "6 只 `Hinge GN 237-ZD-60-60-A-SW`（CAD 18 个 occurrence）在 S3 中一个都没有绑定；"
  "手册的 `Back Hinge ×6` 与之对不上名。", "Back Hinge ×6", "本清单按人工对名(推断)连接，需确认"),
 ("Q16", "材料牌号", "全部 CNC 铣削件与树脂打印件都没有材料牌号、板厚公差、表面处理记录。",
  "非标自制件全部", "无法下单加工"),
 ("Q17", "粘接件工艺", "把手、磁铁、`Front Screw Support`、`POM Fillet` 为粘接固定，胶种、表面处理与固化时间全部缺失。",
  "Window Handle ×3、Magnet 5 只、POM Fillet ×2 等", "工艺缺失"),
 ("Q18", "气压设定值", "各路工作压力设定（换刀 / 清刀 / 防尘罩）在手册与 BOM 中都没有；管路是否需要编号也未定。",
  "气动分组全部", "调试参数缺失"),
 ("Q19", "线缆规格", "全部电源线、驱动器线、跳线、多芯线只给了名称与长度代号（Length A/B/C），"
  "没有截面积、颜色、端子型号、实际长度。", "约 60 行线缆", "无法采购"),
 ("Q20", "T 型螺母不适用", "HowTo H1 'How To Insert T-nuts' 在本机 114 步中从未被引用，Workbook 与 BOM 中 'T-nut' 零命中；"
  "本机机架为 CNC 铣孔方管 + 内置连接板 + HF-screw，不使用 T 型槽型材。", "—", "建议从本机 HowTo 集合移除以免误导"),
 ("Q21", "BT30 刀柄 CAD 数量", "CAD 的 `Tool Changer` 下有 **29 个** depth-2 的 `BT30 Tool Holder (n)`，"
  "而刀库只有 14 个刀位、手册与 BOM 都写 14。其中 14 个被 S3 绑定、14 个完全未绑定、1 个被绑成手册的 `Brush`。"
  "疑为 CAD 中残留的重复/历史几何。", "BT30 Tool Holder", "CAD 数据待清理；采购按 14 把"),
]

# ---------------------------------------------------------------- 统计
def cnt(cat): return sum(1 for r in ROWS if r["分类"] == cat)
def qsum(cat, k="手册数量"): return sum(r[k] for r in ROWS if r["分类"] == cat)
STATS = {
    "生成时间": BUILT_AT,
    "官方BOM": {"分组数": len(BOM_GROUPS), "分组": BOM_GROUPS, "行数": len(BOM_ROWS),
                "件数合计": sum(r["qty"] for r in BOM_ROWS)},
    "官方手册": {"工序数": len(OPS_SOP), "零件行数": len(MAN_ROWS),
                 "不同零件名": len(MAN_NAMES), "件数合计": sum(MAN_QTY.values()),
                 "紧固件件数": sum(q for n, q in MAN_QTY.items() if parse_fastener(n)),
                 "非紧固件件数": sum(q for n, q in MAN_QTY.items() if not parse_fastener(n))},
    "手册GLB紧固件层": FASTLAYER["summary"],
    "STEP": {"叶occurrence": len(LEAVES), "装配occurrence": len(ASMS),
             "机器级件数(折叠后)": sum(STEP_MACHINE.values()),
             "机器级不同基名": len(STEP_MACHINE),
             "折叠进外购模块的内部叶数": sum(MODULE_INTERNALS.values()),
             "外购模块数(depth-2装配)": sum(1 for a in ASMS if a["depth"] == 2)},
    "S3绑定": {"动件数": sum(len(o["movers"]) for o in S3["operations"]),
               "绑定的手册零件名": len(MOVERS_BY_MAN),
               "未绑定叶": len(S3["unbound_leaves"]),
               "未绑定-折叠进外购模块": sum(UNBOUND_FOLDED.values()),
               "未绑定-机器级": sum(UNBOUND_MACHINE.values()),
               "手册有STEP无几何(holds)": sum(HOLDS_BY_MAN.values())},
    "本清单": {
        "总行数": len(ROWS),
        "标准件": {"行数": cnt(CAT_STD), "手册件数": qsum(CAT_STD)},
        "外购件": {"行数": cnt(CAT_BUY), "手册件数": qsum(CAT_BUY)},
        "外购改制件": {"行数": cnt(CAT_MOD), "手册件数": qsum(CAT_MOD)},
        "非标自制件": {"行数": cnt(CAT_MAKE), "手册件数": qsum(CAT_MAKE)},
        "线缆与耗材": {"行数": cnt(CAT_CABLE), "手册件数": qsum(CAT_CABLE)},
        "待定": {"行数": cnt(CAT_UNK), "手册件数": qsum(CAT_UNK)},
        "STEP独有追加行": sum(1 for r in ROWS if r["名称对应来源"].startswith("STEP独有")),
    },
    "差异": {
        "官方BOM缺失行数": len(DIFF_BOM_MISSING),
        "官方BOM缺失件数": sum(r["手册数量"] for r in DIFF_BOM_MISSING),
        "其中紧固件行数": sum(1 for r in DIFF_BOM_MISSING if r["分类"] == CAT_STD and parse_fastener(r["手册名"])),
        "其中紧固件件数": sum(r["手册数量"] for r in DIFF_BOM_MISSING if parse_fastener(r["手册名"])),
        "数量不一致行数": len(DIFF_QTY),
        "STEP独有-机器级行数": sum(1 for r in DIFF_STEP_ONLY if r["class"] != "FOLDED_INTO_PURCHASED_MODULE"),
        "STEP独有-机器级件数": sum(r["STEP实例数"] for r in DIFF_STEP_ONLY if r["class"] != "FOLDED_INTO_PURCHASED_MODULE"),
        "STEP独有-折叠进外购模块件数": sum(r["STEP实例数"] for r in DIFF_STEP_ONLY if r["class"] == "FOLDED_INTO_PURCHASED_MODULE"),
        "官方BOM未连上行数": len(DIFF_BOM_EXTRA),
        "官方BOM已回连行数(去重)": len(BOM_LINKED),
        "官方BOM已回连件数(去重)": sum(BOM_BY_NAME.get(b, 0) for b in BOM_LINKED),
        "官方BOM行被多个手册件共用的行数": sum(1 for b, n in BOM_ROW_USERS.items() if n > 1),
    },
}

# ---------------------------------------------------------------- 写 xlsx
print("[7/9] 写 xlsx …", file=sys.stderr)
from openpyxl import Workbook
from openpyxl.styles import Font, Alignment, PatternFill, Border, Side
from openpyxl.utils import get_column_letter

HDR_FILL = PatternFill("solid", fgColor="1F3864")
HDR_FONT = Font(color="FFFFFF", bold=True, size=10)
CELL_FONT = Font(size=10)
THIN = Side(style="thin", color="D9D9D9")
BORDER = Border(left=THIN, right=THIN, top=THIN, bottom=THIN)
CAT_FILL = {CAT_STD: "FFF2CC", CAT_BUY: "DEEBF7", CAT_MOD: "E2EFDA",
            CAT_MAKE: "FCE4D6", CAT_CABLE: "EDEDED", CAT_UNK: "F8CBAD"}

def add_sheet(wb, title, rows, cols=None, widths=None, freeze="A2", note=None):
    ws = wb.create_sheet(title[:31])
    if note:
        ws.append([note]); ws["A1"].font = Font(size=9, italic=True, color="666666")
        ws.append([])
        start = 3
    else:
        start = 1
    if not rows:
        ws.append(["（无记录）"]); return ws
    cols = cols or list(rows[0].keys())
    ws.append(cols)
    for c in range(1, len(cols)+1):
        cell = ws.cell(row=start, column=c)
        cell.fill = HDR_FILL; cell.font = HDR_FONT
        cell.alignment = Alignment(vertical="center", wrap_text=True)
    for r in rows:
        ws.append([r.get(c, "") for c in cols])
    for row in ws.iter_rows(min_row=start+1, max_row=ws.max_row, max_col=len(cols)):
        for cell in row:
            cell.font = CELL_FONT; cell.border = BORDER
            cell.alignment = Alignment(vertical="top", wrap_text=True)
        if "分类" in cols:
            v = row[cols.index("分类")].value
            if v in CAT_FILL:
                row[cols.index("分类")].fill = PatternFill("solid", fgColor=CAT_FILL[v])
    ws.freeze_panes = ws.cell(row=start+1, column=1).coordinate if freeze else None
    ws.auto_filter.ref = f"{get_column_letter(1)}{start}:{get_column_letter(len(cols))}{ws.max_row}"
    widths = widths or {}
    for i, c in enumerate(cols, 1):
        w = widths.get(c)
        if w is None:
            mx = max([len(str(c))] + [len(str(r.get(c, ""))) for r in rows[:400]])
            w = min(max(9, int(mx*1.15)), 52)
        ws.column_dimensions[get_column_letter(i)].width = w
    ws.row_dimensions[start].height = 30
    return ws

wb = Workbook(); wb.remove(wb.active)

# 00 说明
ws = wb.create_sheet("00_说明")
INTRO = [
 ("OLSK Large CNC V3 —— 完整 BOM 与采购/加工清单", ""),
 ("版本", VERSION), ("生成时间", BUILT_AT),
 ("生成脚本", "工艺文件/BOM与采购/build_bom.py"),
 ("机器", "开源大幅面 CNC 铣床，加工范围 2500×1250 mm；铝方管机架 + 树脂接头；三轴滚珠丝杠（Y 轴旋转螺母）；"
   "闭环交流伺服；4.5 kW BT30 主轴 + 14 位刀库；气动换刀/清刀/冷却/防尘罩；气弹簧升降窗；24 V 急停"),
 ("设计方", "InMachines Ingrassia GmbH（Daniele Ingrassia），CERN-OHL-W v2 / CC BY-SA 4.0"),
 ("", ""),
 ("【三个数量列的口径】", ""),
 ("手册数量", "装配工艺/sop-olsk-workbook.v1.json 中该零件名在 114 步里的用量之和（581 个零件行、3282 件）"),
 ("STEP实例数(工序绑定)", "本体/S3-工序实例绑定.v1.json 中绑定到该手册名的动件（mover）数。"
   "一个 mover = 一个机器级刚体：叶 occurrence / 顶层装配自体的一个体 / 一个预装模块 solver_unit"),
 ("STEP实例数(全模型)", "该 STEP 定义基名在整机 STEP 中的机器级实例数（不折叠也不去重工序），用于交叉核对"),
 ("官方BOM数量", "功能属性/inputs/BOM_sheet1.csv 中对应行的 Quantity"),
 ("", ""),
 ("【外购件内部子体的折叠规则】", ""),
 ("规则", "叶 occurrence 若位于某个 depth-2 装配（=预装采购模块，如 PCB Controller、2V025-08 电磁阀、"
   "Festo 三联件、Caster 500Kg）之下，一律计入该模块，不作为独立机器零件。"
   f"全模型 1697 个叶中有 {sum(MODULE_INTERNALS.values())} 个属于此类。"),
 ("", ""),
 ("【名称对应来源】", ""),
 ("S3绑定", "由 S3-工序实例绑定.v1.json 的 via_glb_names 给出，是手册 GLB 节点名 ↔ STEP product 的实例级对应"),
 ("人工对名(推断)", "S3 未绑定，但按 H 线证据与命名可高置信对上，本清单人工连接，一律标(推断)"),
 ("无对应", "STEP 中查不到几何（多为线缆、气管、端子、紧固件）"),
 ("STEP独有(...)", "STEP 有几何、手册 114 步未列，按 S3 的 unbound class 标注"),
 ("", ""),
 ("【推断的边界】", ""),
 ("标注规则", "凡本案数据（BOM/手册/STEP/GLB）中没有的数值——材料牌号、强度等级、扭矩、螺纹胶、"
   "气弹簧力值、导轨预紧、线缆截面——一律标(推断)或 UNKNOWN，不用公开资料的典型值顶替，并列入 09_待确认问题"),
 ("零件名", "全部照抄英文原名，不翻译"),
 ("官方BOM数量列不可求和", "一条官方 BOM 行可能同时服务多个手册件（例如 `Linear Guide Cart HGH25CAZA` 一行 12 只，"
   "被 X Carriage / Y Carriage / Z Carriage 三个手册件共用）。这类行已在'备注/存疑'列标出，"
   "求整机采购量请用 06/07 两张汇总表，不要对 01_总清单 的该列求和。"),
]
for k, v in INTRO:
    ws.append([k, v])
ws.column_dimensions["A"].width = 26; ws.column_dimensions["B"].width = 118
ws["A1"].font = Font(bold=True, size=14)
for r in ws.iter_rows(min_row=1, max_row=ws.max_row, max_col=2):
    r[1].alignment = Alignment(vertical="top", wrap_text=True)
    if r[0].value and str(r[0].value).startswith("【"):
        r[0].font = Font(bold=True, size=11)
ws.append([]); ws.append(["【关键数字】", ""])
ws.cell(row=ws.max_row, column=1).font = Font(bold=True, size=11)
def flat(d, pre=""):
    for k, v in d.items():
        if isinstance(v, dict): yield from flat(v, pre + k + " · ")
        elif isinstance(v, list): yield pre + k, "、".join(map(str, v))
        else: yield pre + k, v
for k, v in flat(STATS):
    ws.append([k, v])

MAIN_COLS = ["序号", "手册名(英文原名)", "STEP名(英文原名)", "名称对应来源", "分类", "家族/子类",
             "手册数量", "STEP实例数(工序绑定)", "STEP实例数(全模型)", "官方BOM数量", "官方BOM行名",
             "官方BOM分组", "数量一致性", "规格解码", "建议标准号", "材料/等级(推断)", "拧紧工具(推断)",
             "供应商", "型号/订货号", "加工方式/材料", "外形尺寸(STEP包围盒)", "下料尺寸", "STEP几何", "所在工序(步号)", "工序数",
             "分类判据", "备注/存疑"]
add_sheet(wb, "01_总清单", ROWS, MAIN_COLS,
          widths={"手册名(英文原名)": 32, "STEP名(英文原名)": 38, "规格解码": 42, "建议标准号": 26,
                  "材料/等级(推断)": 26, "加工方式/材料": 30, "外形尺寸(STEP包围盒)": 22, "所在工序(步号)": 30, "分类判据": 30, "备注/存疑": 46,
                  "型号/订货号": 30, "数量一致性": 24, "下料尺寸": 30, "官方BOM行名": 24},
          note="口径见 00_说明。分类：标准件 / 外购件 / 外购改制件 / 非标自制件 / 线缆与耗材。数量三列分别来自手册、STEP、官方 BOM。")

add_sheet(wb, "02_差异-BOM缺失", DIFF_BOM_MISSING,
          widths={"手册名": 30, "STEP名": 34, "建议标准号/型号": 30, "所在工序": 30, "说明": 46},
          note="手册 114 步里用到、但官方 BOM_sheet1.csv 中查不到对应行的零件。照 BOM 采购会缺这些件。")
add_sheet(wb, "03_差异-数量不一致", DIFF_QTY,
          widths={"手册名": 30, "STEP名": 34, "所在工序": 30},
          note="手册用量 / STEP 实例数 / 官方 BOM 数量三者中至少有一对不相等的零件。")
add_sheet(wb, "04_差异-STEP独有", DIFF_STEP_ONLY,
          widths={"STEP名": 44, "折叠判定": 40, "性质": 44, "型号": 30},
          note="STEP 有几何、手册未列。class=FOLDED_INTO_PURCHASED_MODULE 的是采购模块内部子体（已折叠，非缺失）；其余为真实缺失。")
add_sheet(wb, "05_差异-BOM未连上", DIFF_BOM_EXTRA,
          widths={"官方BOM行名": 44, "说明": 60},
          note="官方 BOM 有行、但连不到任何手册零件行的记录。")

add_sheet(wb, "06_汇总-紧固件", SUM_FAST,
          widths={"手册规格名": 26, "官方BOM行名": 26, "头型": 34, "建议标准号": 26,
                  "材料/等级(推断)": 30, "拧紧工具(推断)": 20},
          note="按规格×总数汇总。建议标准号与材料等级为推断（见 09_待确认问题 Q01/Q02）。"
               "建议采购量 = 手册用量 ×1.10 后按建议包装数向上取整；用量很小的规格由最小包装决定，合计会明显大于净用量。")
add_sheet(wb, "07_汇总-外购件", SUM_BUY_ROWS,
          widths={"家族": 20, "供应商": 22, "型号/订货号": 44, "手册件名": 40, "STEP件名": 40, "官方BOM行名": 34},
          note="按 H 线 15 个外购件家族 × 型号汇总。供应商/型号来自 功能属性/tracks/*/function.json 的译解。")
add_sheet(wb, "08_汇总-非标件", SUM_MAKE_ROWS,
          widths={"加工方式": 46, "备注": 60},
          note="非标自制件按加工方式汇总。加工方式判据来自 STEP 件名的 (CNC Milled) / (Resin printed) 标签。")
add_sheet(wb, "09_汇总-型材下料", SUM_PROFILE_AGG_ROWS,
          widths={"截面": 20, "备注": 70},
          note="外购改制件（型材/丝杠/导轨/线槽按长度下料）按截面汇总；明细见 10_型材下料明细。")
add_sheet(wb, "10_型材下料明细", SUM_PROFILE,
          widths={"截面": 26, "手册名": 34, "STEP名": 40, "加工方式": 40, "所在工序": 24, "长度来源": 30},
          note="下料长度 = STEP 定义体 STL 包围盒最长边（mm），含端部加工特征，不含锯切余量。")
add_sheet(wb, "11_工序零件明细", OPS_SHEET,
          widths={"工序名": 34, "零件(手册名)": 30, "工具": 24},
          note="官方手册 114 步 × 581 个零件行；S3状态 = COUNT_MATCH / COUNT_MISMATCH / NO_GLB。")
add_sheet(wb, "12_待确认问题", [{"编号": a, "主题": b, "问题": c, "涉及零件": d, "影响": e}
                               for a, b, c, d, e in QUESTIONS],
          widths={"编号": 8, "主题": 16, "问题": 74, "涉及零件": 34, "影响": 26},
          note="需向 InMachines Ingrassia GmbH（daniele@inmachines.net）确认的 20 个问题。")

XLSX = OUT / f"OLSK_Large_CNC_V3_完整BOM_{VERSION}.xlsx"
wb.save(XLSX)
print(f"      -> {XLSX}", file=sys.stderr)

# ---------------------------------------------------------------- 写 json
print("[8/9] 写 json …", file=sys.stderr)
DOC = {
    "$schema": "olsk.bom-and-procurement/v1",
    "record_id": f"OLSK-V3-BOM-{VERSION.upper()}",
    "built_at": BUILT_AT,
    "purpose": "OLSK Large CNC V3 完整 BOM 与采购/加工清单：手册 × STEP × 官方 BOM 三源对账，"
               "按标准件/外购件/外购改制件/非标自制件分类，含规格解码、供应商型号、差异表与汇总表。",
    "sources": {
        "official_bom": "功能属性/inputs/BOM_sheet1.csv",
        "manual": "装配工艺/sop-olsk-workbook.v1.json",
        "step_occurrences": "本体/S1-occurrence清单-体展开.v2.json",
        "name_binding": "本体/S3-工序实例绑定.v1.json",
        "fastener_layer": "本体/S1-紧固件层.v1.json",
        "geometry": "网格导出-体展开/manifest.json + *.stl",
        "purchased_decode": "功能属性/tracks/{A..H}/function.json、tracks/H/notes.md、tracks/H/sources.md",
        "machine_readme": "功能属性/inputs/OLSK_README.md",
    },
    "claim_boundary": [
        "行单位是手册零件名；官方 BOM 与 STEP 通过 S3 的 via_glb_names 与人工对名表连接，"
        "人工对名一律标(推断)，不作为官方对应关系。",
        "STEP 实例数以 S3 动件数为准；外购模块的内部子体按 depth-2 装配折叠，不计为独立零件。",
        "型材下料长度取 STEP 定义体 STL 包围盒最长边，含端部加工特征、不含锯切余量，仅供估算。",
        "材料牌号、强度等级、扭矩、螺纹胶、气弹簧力值、导轨预紧、线缆截面在本案数据中均不存在，"
        "本清单中相关字段一律为推断或 UNKNOWN，并列入待确认问题。",
        "紧固件的标准号对应（B-screw↔ISO 7380、C-screw↔DIN 912、Lock Nut↔DIN 985）依据逐规格 1:1 的数量匹配，"
        "官方 BOM 写作 ISO 3780 疑为笔误，需 InMachines 确认。",
    ],
    "stats": STATS,
    "rows": ROWS,
    "diff": {
        "bom_missing": DIFF_BOM_MISSING,
        "qty_mismatch": DIFF_QTY,
        "step_only": DIFF_STEP_ONLY,
        "bom_unlinked": DIFF_BOM_EXTRA,
    },
    "summary": {
        "fasteners": SUM_FAST,
        "purchased": SUM_BUY_ROWS,
        "made": SUM_MAKE_ROWS,
        "profiles_by_section": SUM_PROFILE_AGG_ROWS,
        "profiles_detail": SUM_PROFILE,
    },
    "operations": OPS_SHEET,
    "open_questions": [{"id": a, "topic": b, "question": c, "parts": d, "impact": e}
                       for a, b, c, d, e in QUESTIONS],
}
JSONP = OUT / "bom.v1.json"
json.dump(DOC, open(JSONP, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print(f"      -> {JSONP}", file=sys.stderr)

# ---------------------------------------------------------------- 写 md
print("[9/9] 写差异报告 …", file=sys.stderr)
S = STATS
def md_table(rows, cols, limit=None, keymap=None):
    out = ["| " + " | ".join(cols) + " |", "|" + "|".join(["---"]*len(cols)) + "|"]
    for r in (rows[:limit] if limit else rows):
        out.append("| " + " | ".join(
            str(r.get(keymap.get(c, c) if keymap else c, "")).replace("|", "\\|").replace("\n", " ")
            for c in cols) + " |")
    return "\n".join(out)

fast_missing = [r for r in DIFF_BOM_MISSING if parse_fastener(r["手册名"])]
other_missing = [r for r in DIFF_BOM_MISSING if not parse_fastener(r["手册名"])]
plate_missing = [r for r in other_missing if re.search(r"Bracket|Connector Plate|Plate$", r["手册名"])]
n_fast_pcs = sum(r["手册数量"] for r in fast_missing)
n_plate_pcs = sum(r["手册数量"] for r in plate_missing)

md = f"""# OLSK Large CNC V3 —— BOM 差异报告 {VERSION}

> 生成时间 {BUILT_AT} ｜ 生成脚本 `工艺文件/BOM与采购/build_bom.py` ｜ 数据文件 `bom.v1.json` ｜ 工作簿 `OLSK_Large_CNC_V3_完整BOM_{VERSION}.xlsx`
>
> 机器：开源大幅面 CNC 铣床，加工范围 2500×1250 mm，设计方 InMachines Ingrassia GmbH。
> 本报告只做**三源对账**：官方 BOM（采购视角）× 官方装配手册（工艺视角）× STEP 装配树（几何视角）。

---

## 一、方法

### 1.1 三个数据源与它们各自的口径

| 源 | 文件 | 规模 | 口径 |
|---|---|---|---|
| 官方 BOM | `功能属性/inputs/BOM_sheet1.csv` | {S['官方BOM']['分组数']} 个分组、{S['官方BOM']['行数']} 行、合计 {S['官方BOM']['件数合计']} 件 | 按 CAD 顶层装配分组（{ '、'.join(BOM_GROUPS) }），件名基本等于 STEP product 名 |
| 官方装配手册 | `装配工艺/sop-olsk-workbook.v1.json` | {S['官方手册']['工序数']} 步、{S['官方手册']['零件行数']} 个零件行、{S['官方手册']['不同零件名']} 个不同零件名、合计 {S['官方手册']['件数合计']} 件 | 用"友好名"（Base Inner Bottom Beam、X Carriage…），含全部紧固件 {S['官方手册']['紧固件件数']} 件 |
| STEP 装配树 | `本体/S1-occurrence清单-体展开.v2.json` | {S['STEP']['叶occurrence']} 个叶 occurrence + {S['STEP']['装配occurrence']} 个装配 occurrence | 含采购模块的全部内部零件，需要折叠后才能当零件清单用 |

三者的连接键是 `本体/S3-工序实例绑定.v1.json` 的 `via_glb_names`（手册 GLB 节点名 ↔ STEP product 的实例级对应），
共 {S['S3绑定']['动件数']} 个动件、覆盖 {S['S3绑定']['绑定的手册零件名']} 个手册零件名。S3 未绑定但可高置信对上的，
本清单用一张 {len(ALIAS_MANUAL)} 条的人工对名表连接，全部标 `人工对名(推断)`。

### 1.2 外购件内部子体的折叠规则

STEP 的 {S['STEP']['叶occurrence']} 个叶里有 **{S['STEP']['折叠进外购模块的内部叶数']} 个**位于某个 depth-2 装配之下——
那是采购来的预装模块（PCB Controller 247 个内部叶、5 只 Airtac 2V025-08 电磁阀 220 个、
Push-To-Connect 快插接头 44 个、CNC_OIL_PUMP 37 个、Festo 三联件、Mean Well 电源、Caster 500Kg…）。
本清单一律把它们折叠进模块，不作为独立采购件。折叠后整机机器级件数 **{S['STEP']['机器级件数(折叠后)']}**（{S['STEP']['机器级不同基名']} 个不同基名），
其中 {S['STEP']['外购模块数(depth-2装配)']} 个是预装模块。

### 1.3 三个数量列

- **手册数量**：该零件名在 114 步中的用量之和。
- **STEP 实例数（工序绑定）**：绑定到该手册名的**动件数**。一个动件 = 一个机器级刚体（叶 occurrence / 顶层装配自体的一个体 / 一个 solver_unit 预装模块）。
  用动件数而不是叶体数，是因为 8 只 `Caster 500Kg` 在 STEP 里展开成 40 个叶体——按叶体数会得出"8 只脚轮 = 40 件"的错误。
- **官方 BOM 数量**：BOM 对应行的 Quantity。

### 1.4 推断的边界

材料牌号、强度等级、扭矩、螺纹胶、气弹簧力值、导轨预紧等级、线缆截面积，在 BOM、手册、STEP、GLB 四份数据里**一个都没有**。
本清单中这些字段一律标 `(推断)` 或 `UNKNOWN`，不用公开资料的典型值顶替，并全部列进第五节的待确认问题。零件名一律照抄英文原名。

---

## 二、数字

### 2.1 本清单的构成

| 分类 | 行数 | 手册件数 |
|---|---|---|
| 标准件（紧固件 + 有标准号的轴承） | {S['本清单']['标准件']['行数']} | {S['本清单']['标准件']['手册件数']} |
| 外购件（导轨/丝杠/伺服/主轴/气动/电气/脚轮/气弹簧/五金…） | {S['本清单']['外购件']['行数']} | {S['本清单']['外购件']['手册件数']} |
| 外购改制件（型材/丝杠/导轨/线槽按长度下料） | {S['本清单']['外购改制件']['行数']} | {S['本清单']['外购改制件']['手册件数']} |
| 非标自制件（CNC milled / Resin printed / CNC Profi） | {S['本清单']['非标自制件']['行数']} | {S['本清单']['非标自制件']['手册件数']} |
| 线缆与耗材 | {S['本清单']['线缆与耗材']['行数']} | {S['本清单']['线缆与耗材']['手册件数']} |
| 待定 | {S['本清单']['待定']['行数']} | {S['本清单']['待定']['手册件数']} |
| **合计** | **{S['本清单']['总行数']}** | **{S['官方手册']['件数合计']}** |

其中 {S['本清单']['STEP独有追加行']} 行是"STEP 有几何、手册未列"追加进来的（手册数量记 0）。

### 2.2 差异总览

| 差异类型 | 行数 | 件数 |
|---|---|---|
| 官方 BOM 缺失（手册有、BOM 无） | {S['差异']['官方BOM缺失行数']} | {S['差异']['官方BOM缺失件数']} |
| ├ 其中紧固件 | {S['差异']['其中紧固件行数']} | {S['差异']['其中紧固件件数']} |
| └ 其中连接板 / 角件 | {len(plate_missing)} | {n_plate_pcs} |
| 数量不一致（手册 / STEP / BOM 三者对不上） | {S['差异']['数量不一致行数']} | — |
| STEP 独有 · 机器级（真实缺失） | {S['差异']['STEP独有-机器级行数']} | {S['差异']['STEP独有-机器级件数']} |
| STEP 独有 · 折叠进外购模块（非缺失） | {len([r for r in DIFF_STEP_ONLY if r['class']=='FOLDED_INTO_PURCHASED_MODULE'])} | {S['差异']['STEP独有-折叠进外购模块件数']} |
| 官方 BOM 有行、连不上手册 | {S['差异']['官方BOM未连上行数']} | — |
| 官方 BOM 已回连（去重） | {S['差异']['官方BOM已回连行数(去重)']} | {S['差异']['官方BOM已回连件数(去重)']}（占 BOM 总件数 {S['差异']['官方BOM已回连件数(去重)']*100//S['官方BOM']['件数合计']}%） |
| 手册有件、STEP 无几何（holds） | — | {S['S3绑定']['手册有STEP无几何(holds)']} |

---

## 三、三个最重要的发现

### 发现 1 —— 照官方 BOM 采购，会缺 {S['差异']['官方BOM缺失件数']} 件，其中 {S['差异']['其中紧固件件数']} 件是紧固件

官方 BOM 的 `Fasteners` 分组有 68 行、1852 件，与手册用量**逐行 1:1 完全相等**——说明它是从手册统计出来的，
但**只统计了五类**：`ISO 3780`（B-screw）、`DIN 912`（C-screw）、`DIN 985`（Lock Nut）、`Countersunk Screw`、`Thermoplastic Screw`。
手册实际用到的另外 13 个规格一行都没有：

{md_table(sorted(fast_missing, key=lambda r: -r['手册数量']), ['手册名','手册数量','建议标准号/型号'], keymap={'建议标准号/型号':'建议标准号/型号'})}

合计 {n_fast_pcs} 件。里面最扎眼的是 `HF-screw M12-20` **268 根——它是全机用量第一的紧固件**，
而手册与 BOM 都没有给它标准号、头型和材料（见 Q02）；`C-screw M12-30` 148 根、`Lock Nut M8` 44 只、全部 75 只垫圈同样缺行。

非紧固件那一侧，缺的主要是**结构接头**：{len(plate_missing)} 行、{n_plate_pcs} 件的 `Bracket 100x100 / 30x110 / 30x60 / 60x60`
铝角件与 `Frame / Base / Shoulder / Corner Connector Plate` 连接板。BOM 的 `Frame` 分组只有型材与脚轮，
把整套梁端内置螺母板与外侧加强角件全漏了——而这台机器的机架承载正是靠它们。

### 发现 2 —— 官方 BOM 写的 `ISO 3780` 不是螺钉标准，应为 `ISO 7380`

BOM 用 `ISO 3780 M5-8` 这样的行名表示手册里的 `B-screw M5-8`，28 个规格逐条对应、数量分毫不差，
所以对应关系是确定的。但 **ISO 3780 是"道路车辆—拖拉机与农林机械连接尺寸"标准，不是螺钉标准**。
按头型（手册前缀 B = Button head）与用途判断，应为 **ISO 7380-1 内六角半圆头螺钉**。
这一项影响 695 件螺钉的采购，本清单在"建议标准号"列一律填 `ISO 7380-1` 并标注疑点（Q01）。

同组的另外两个映射是可靠的：`C-screw` ↔ `DIN 912`（= ISO 4762 内六角圆柱头）、`Lock Nut` ↔ `DIN 985`（尼龙锁紧螺母）。

### 发现 3 —— STEP 里有 {S['差异']['STEP独有-机器级行数']} 类件（{S['差异']['STEP独有-机器级件数']} 件）手册一步都没装，其中包含整套后门铰链与全部 9 个支承轴承

把采购模块的内部子体折叠掉之后，剩下 {S['差异']['STEP独有-机器级件数']} 个机器级实例在手册 114 步里找不到任何装配步骤。按数量排前几位：

{md_table([r for r in DIFF_STEP_ONLY if r['class'] != 'FOLDED_INTO_PURCHASED_MODULE'][:14], ['STEP名','STEP实例数','顶层装配','性质'])}

其中三项是真正的工艺缺口：

1. **6 只 `Hinge GN 237-ZD-60-60-A-SW` 后门铰链（CAD 18 个 occurrence）** ——
   手册有 `Back Hinge ×6` 这一行，但 S3 一个实例都没绑上，两侧对不上名。铰链是后门唯一的转动副，不能没有装配步骤（Q15）。
2. **9 个支承轴承 `7210 ×2 / 7211 ×2 / 6904 ×2 / 63004 ×2 / 3804`** ——
   它们只出现在官方 BOM 与 CAD 里，**手册 114 步的零件栏一次都没有列**。7210+7211 这对角接触轴承正是 Y 轴"旋转螺母"方案的核心支承，
   连安装方向（背对背/面对面）和预紧方式都没有任何说明（Q09）。
3. **8 只 `Linear Guide Cart HGH15C`、32 段 `Pipe 6mm`、14 把 `BT30 Tool Holder`** —— 数量口径三方不一致（Q07）。

作为对照，另有 {S['差异']['STEP独有-折叠进外购模块件数']} 个叶体是采购模块的内部零件（`PCB Controller` 247、`2V025-08` 电磁阀 220、
快插接头 44、油泵 37 …），它们**不是缺失**，已按规则折叠，不进采购清单。

---

## 四、其它值得注意的对账结果

### 4.1 数量不一致（{S['差异']['数量不一致行数']} 行）

按"手册 − STEP"差值排序的前 15 行：

{md_table(sorted([r for r in DIFF_QTY if r['STEP实例数(工序绑定)']], key=lambda r: -abs(r['手册数量']-r['STEP实例数(工序绑定)']))[:15], ['手册名','STEP名','手册数量','STEP实例数(工序绑定)','官方BOM数量','名称对应来源'])}

读这张表要注意三件事：

- 行里出现 `B-screw M6-25 → Z Endstop Plate Holder (Resin printed)` 这类"螺钉绑到树脂件"的组合，
  **不是 BOM 错**，而是手册 GLB 把少数螺钉合并进了别的零件的节点，S3 因此把整组绑成一个动件。这类行的"STEP 实例数"没有采购意义。
- `人工对名(推断)` 的行，差值同时包含"对名不准"的可能，需连同 STEP 名一起复核。
- 差值为正（手册 > STEP）多半是 CAD 缺件；差值为负（STEP > 手册）多半是手册漏步或一个手册名对应多个 CAD 件
  （如 `Tube 6` 一行对应 CAD 的 32 段 `Pipe 6mm`）。

### 4.2 手册有件、STEP 无几何：{S['S3绑定']['手册有STEP无几何(holds)']} 处

其中 {S['官方手册']['紧固件件数']} 件是紧固件——**这台机器的 CAD 里几乎不存在手册要求的螺钉**
（`本体/S1-紧固件层.v1.json` 从手册 GLB 里另行提取出 {FASTLAYER['summary']['fasteners']} 个紧固件网格、{FASTLAYER['summary']['specs']} 种规格，
分布在 {FASTLAYER['summary']['ops']} 个工步，用于表达与画面审计，不作为 STEP 身份）。
其余是线缆、气管、端子、标签这些手册有名、CAD 无实体的项。

### 4.3 官方 BOM 的回连情况

官方 BOM 的 {len(BOM_BY_NAME)} 个不同行名里，{S['差异']['官方BOM已回连行数(去重)']} 个连上了手册零件（{S['差异']['官方BOM已回连件数(去重)']} 件，
占 BOM 总件数 {S['差异']['官方BOM已回连件数(去重)']*100//S['官方BOM']['件数合计']}%），{S['差异']['官方BOM未连上行数']} 个连不上。
另有 **{S['差异']['官方BOM行被多个手册件共用的行数']} 条 BOM 行被多个手册件共用**——
例如 `Linear Guide Cart HGH25CAZA` 一行 12 只，被 `X Carriage` / `Y Carriage` / `Z Carriage` 三个手册件共用。
这类行已在总清单的"备注/存疑"列逐条标出；**求整机采购量请用 06/07 两张汇总表，不要对总清单的"官方BOM数量"列求和**。

连不上的 {S['差异']['官方BOM未连上行数']} 行**几乎都是"BOM 与 CAD 里有、装配手册里没有"的件**，不是命名问题：

{md_table(sorted(DIFF_BOM_EXTRA, key=lambda r: -r['BOM数量']), ['官方BOM行名','分组','BOM数量','STEP同名实例数'])}

其中 `Press Fit Connector 1-4 MNPT x 6mm ×23`（气路全部快插接头）、`CHINA_Coolant-hose flex_segment ×18`（Z 头冷却软管）、
`PCB Controller`、`USB-UHB-4U Waveshare`、`Motor T6M60-400H2A3-M17S`（Z 伺服，因为手册的 `Z Motor` 在 S3 里被并进了 `Z Motor Holder`）、
`Bearing 3804`、`Ball Screw Nut SFU2505` 都是功能上必需的件——它们能买到，但手册没有告诉装配者在哪一步、怎么装。

### 4.4 型材下料

外购改制件按截面汇总（长度取 STEP 定义体包围盒最长边，含端部加工特征、不含锯切余量）：

{md_table(SUM_PROFILE_AGG_ROWS, ['截面','品种数','总根数','总长度m(按包围盒估)'])}

---

## 五、需要向 InMachines 确认的问题

{md_table([{'编号': a, '主题': b, '问题': c, '影响': e} for a, b, c, d, e in QUESTIONS], ['编号','主题','问题','影响'])}

---

## 六、输出清单

| 文件 | 内容 |
|---|---|
| `build_bom.py` | 生成脚本（`.venv/bin/python 工艺文件/BOM与采购/build_bom.py`），只读输入、只写本目录 |
| `OLSK_Large_CNC_V3_完整BOM_{VERSION}.xlsx` | 13 个 sheet：说明 / 总清单 / 4 张差异表 / 5 张汇总表 / 工序零件明细 / 待确认问题 |
| `bom.v1.json` | 同一份数据的机器可读版本（含 claim_boundary 与全部统计） |
| `BOM差异报告_{VERSION}.md` | 本文件 |
| `_aabb_cache.v1.json` | STL 包围盒缓存（可删，删后自动重建） |

**主张边界**：本报告的全部结论只依据本案的四份只读数据（官方 BOM、官方手册、STEP 装配树、手册 GLB）。
外购件的厂家与型号译解来自 `功能属性/tracks/*/function.json`；凡数据中不存在的数值一律标推断或列入第五节，不用公开资料的典型值顶替。
"""
MDP = OUT / f"BOM差异报告_{VERSION}.md"
open(MDP, "w", encoding="utf-8").write(md)
print(f"      -> {MDP}", file=sys.stderr)

print(f"\n完成：{S['本清单']['总行数']} 行主清单；"
      f"BOM 缺失 {S['差异']['官方BOM缺失行数']} 行 / {S['差异']['官方BOM缺失件数']} 件；"
      f"数量不一致 {S['差异']['数量不一致行数']} 行；"
      f"STEP 独有机器级 {S['差异']['STEP独有-机器级行数']} 类 / {S['差异']['STEP独有-机器级件数']} 件。",
      file=sys.stderr)
