#!/usr/bin/env python3
"""Poppy S3：把官方指南工序绑定到具体 occurrence（实例级）。

X1 的 S3 靠 SOP 物料行 + 侧别 + 共配合 + 唯一消耗解绑定；Poppy 的官方指南是
模块化章节而不是逐物料行，且装配树本身就是按官方子装配划分的（29 个根单元）。
因此绑定用『NAUO 子树前缀 → 工序』的显式映射完成，规则：

  ① 每个叶 occurrence 恰好被一道工序消耗一次（唯一消耗，Orbita C2 同构）——
     脚本硬校验，闭合失败即退出非零。
  ② 左右歧义单元已用世界坐标裁定（+X=左，+Y=上；见 s3-side-evidence 段）。
  ③ 集成工序移动的是已完成的刚性子装配（S5 solver unit 语义），
     成员逐一列出，不是任意成组。
  ④ 证据级逐工序落盘：DOC_EXPLICIT（指南明说）/ DOC_IMPLIED / GEOMETRIC_ONLY。

用法：python3 build_poppy_s3_binding.py --occ S1清单 --sop sop-poppy --out OUT
"""
import argparse
import json
import sys
from pathlib import Path

# 工序装配 DAG：每道工序的**前驱工序**（其产物是本工序的输入）。
# 官方指南是模块化装配（分组并行制作）：子装配在台架上单独组装，
# 行走/审计的在场集 = 前驱闭包的动件，不是"全部更早章的件"——
# 否则脊柱会被"已在最终位姿的电机单元"假卡死（2026-08-29 v2 行走实测 35 STUCK 的主因）。
PREDECESSORS = {
    "LEG-PELVIS": [], "LEG-L-HIP": [], "LEG-L-THIGH": [], "LEG-L-SHIN": [],
    "LEG-L-ASSEMBLY": ["LEG-L-HIP", "LEG-L-THIGH", "LEG-L-SHIN"],
    "LEG-R-HIP": [], "LEG-R-THIGH": [], "LEG-R-SHIN": [],
    "LEG-R-ASSEMBLY": ["LEG-R-HIP", "LEG-R-THIGH", "LEG-R-SHIN"],
    "LEGS-PELVIS-ASSEMBLY": ["LEG-L-ASSEMBLY", "LEG-R-ASSEMBLY", "LEG-PELVIS"],
    "TRUNK-DOUBLE-MX64": [], "TRUNK-DOUBLE-MX28": [], "TRUNK-SPINE": [], "TRUNK-CHEST": [],
    "TRUNK-ASSEMBLY": ["TRUNK-DOUBLE-MX64", "TRUNK-DOUBLE-MX28", "TRUNK-SPINE", "TRUNK-CHEST"],
    "ARM-L-FOREARM": [], "ARM-L-UPPERARM": [], "ARM-L-SHOULDER": [],
    "ARM-L-ASSEMBLY": ["ARM-L-SHOULDER", "ARM-L-UPPERARM", "ARM-L-FOREARM"],
    "ARM-R-FOREARM": [], "ARM-R-UPPERARM": [], "ARM-R-SHOULDER": [],
    "ARM-R-ASSEMBLY": ["ARM-R-SHOULDER", "ARM-R-UPPERARM", "ARM-R-FOREARM"],
    "TRUNK-ARMS-ASSEMBLY": ["TRUNK-ASSEMBLY", "ARM-L-ASSEMBLY", "ARM-R-ASSEMBLY"],
    "LEGS-TORSO-ASSEMBLY": ["LEGS-PELVIS-ASSEMBLY", "TRUNK-ARMS-ASSEMBLY"],
    # 头部：面壳+相机+屏是独立台架单元（指南：camera support 装到 head_front）；
    # 电子件装进**开着的后壳**（HEAD-NECK 台架产物）；HEAD-CLOSE 才把两半合上。
    # 曾把 HEAD-CAMERA-SCREEN 排为 HEAD-NECK 的后继——Odroid 在封闭腔里进场，假卡死。
    "HEAD-NECK": [], "HEAD-CAMERA-SCREEN": [],
    "HEAD-ELECTRONICS": ["HEAD-NECK"],
    "HEAD-CLOSE": ["HEAD-ELECTRONICS", "HEAD-CAMERA-SCREEN"],
    "HEAD-TORSO-ASSEMBLY": ["LEGS-TORSO-ASSEMBLY", "HEAD-CLOSE"],
}

# 工序 → 消耗成员（prefix: NAUO 子树；uid: 单件）。顺序即成片章节候选序。
MAPPING = [
    # 官方 Legs 章步序 1-5；先腿后躯干的全序是编导选择（官方允许并行制作）
    ("LEG-PELVIS",           {"prefixes": ["NAUO1/"], "uids": ["nauo:NAUO12"]},
     "DOC_EXPLICIT", "pelvis 单元 + BIOLOID 3P 扩展 PCB（PCB 位于骨盆背侧 z=-21，几何裁定）"),
    ("LEG-L-HIP",            {"prefixes": ["NAUO3/", "NAUO5/"], "uids": []},
     "DOC_EXPLICIT", "hip_left + hip_main_motor_left（+X=左，centroid x=+39.9）"),
    ("LEG-L-THIGH",          {"prefixes": ["NAUO7/"], "uids": []}, "DOC_EXPLICIT", None),
    ("LEG-L-SHIN",           {"prefixes": ["NAUO8/"], "uids": ["nauo:NAUO11"]},
     "DOC_EXPLICIT", "shin_left + simple_foot_left"),
    ("LEG-L-ASSEMBLY",       {"prefixes": [], "uids": [],
                              "unit_joins": ["NAUO3+NAUO5", "NAUO7", "NAUO8+NAUO11"]},
     "DOC_EXPLICIT", "左腿合装：hip/thigh/shin 三个 solver unit 相继结合"),
    ("LEG-R-HIP",            {"prefixes": ["NAUO2/", "NAUO4/"], "uids": []},
     "DOC_EXPLICIT", "hip_right（centroid x=-66.5）"),
    ("LEG-R-THIGH",          {"prefixes": ["NAUO6/"], "uids": []}, "DOC_EXPLICIT", None),
    ("LEG-R-SHIN",           {"prefixes": ["NAUO9/"], "uids": ["nauo:NAUO10"]},
     "DOC_EXPLICIT", "shin_right + simple_foot_right"),
    ("LEG-R-ASSEMBLY",       {"prefixes": [], "uids": [],
                              "unit_joins": ["NAUO2+NAUO4", "NAUO6", "NAUO9+NAUO10"]},
     "DOC_EXPLICIT", None),
    ("LEGS-PELVIS-ASSEMBLY", {"prefixes": [], "uids": [],
                              "unit_joins": ["left_leg", "right_leg", "onto:NAUO1"]},
     "DOC_EXPLICIT", "指南 Legs 步 4"),
    ("TRUNK-DOUBLE-MX64",    {"prefixes": ["NAUO13/"], "uids": []}, "DOC_EXPLICIT", None),
    ("TRUNK-DOUBLE-MX28",    {"prefixes": ["NAUO16/"], "uids": []}, "DOC_EXPLICIT", None),
    ("TRUNK-SPINE",          {"prefixes": ["NAUO15/"], "uids": []}, "DOC_EXPLICIT", None),
    ("TRUNK-CHEST",          {"prefixes": ["NAUO17/"], "uids": []}, "DOC_EXPLICIT", None),
    ("TRUNK-ASSEMBLY",       {"prefixes": [], "uids": ["nauo:NAUO14", "nauo:NAUO19", "nauo:NAUO18"],
                              "unit_joins": ["NAUO13", "NAUO15", "NAUO16", "NAUO17", "onto:NAUO14"]},
     "DOC_EXPLICIT", "abdomen 为基体；SMPS×2 的工序归属为 DOC_IMPLIED（NAUO19 y=91 近 abdomen 螺孔证据；"
                     "NAUO18 y=215 胸位，指南未明说安装时机，标 AMBIGUOUS_EVIDENCE）"),
    ("ARM-L-FOREARM",        {"prefixes": ["NAUO23/"], "uids": []}, "DOC_EXPLICIT", None),
    ("ARM-L-UPPERARM",       {"prefixes": ["NAUO22/"], "uids": []},
     "DOC_EXPLICIT", "upper_arm_double_MX-28（centroid x=+223=左）"),
    ("ARM-L-SHOULDER",       {"prefixes": ["NAUO21/"], "uids": []},
     "DOC_EXPLICIT", "shoulder_x 左（centroid x=+110，几何裁定）"),
    ("ARM-L-ASSEMBLY",       {"prefixes": [], "uids": [],
                              "unit_joins": ["NAUO21", "NAUO22", "NAUO23"]},
     "DOC_EXPLICIT", None),
    ("ARM-R-FOREARM",        {"prefixes": ["NAUO27/"], "uids": []}, "DOC_EXPLICIT", None),
    ("ARM-R-UPPERARM",       {"prefixes": ["NAUO26/"], "uids": []}, "DOC_EXPLICIT", None),
    ("ARM-R-SHOULDER",       {"prefixes": ["NAUO25/"], "uids": []},
     "DOC_EXPLICIT", "shoulder_x 右（centroid x=-110）"),
    ("ARM-R-ASSEMBLY",       {"prefixes": [], "uids": [],
                              "unit_joins": ["NAUO25", "NAUO26", "NAUO27"]},
     "DOC_EXPLICIT", None),
    ("TRUNK-ARMS-ASSEMBLY",  {"prefixes": [], "uids": ["nauo:NAUO20", "nauo:NAUO24"],
                              "unit_joins": ["left_arm", "right_arm", "onto:trunk"]},
     "DOC_EXPLICIT", "48x M2x3（STEP 无此螺钉件，HOLD 台账呈现）；肩板三点标记定向"),
    ("LEGS-TORSO-ASSEMBLY",  {"prefixes": [], "uids": [],
                              "unit_joins": ["legs_pelvis", "onto:torso"]},
     "DOC_EXPLICIT", "16x M2.5x4（STEP 无此螺钉件）"),
    ("HEAD-NECK",            {"prefixes": ["NAUO28/NAUO31/"], "uids": ["nauo:NAUO29", "nauo:NAUO28/NAUO30"]},
     "DOC_EXPLICIT", "head_back 为基体；AX-12 舵机组（含 horn 与 BHS_M2_6X8 螺钉件）+ neck"),
    ("HEAD-CAMERA-SCREEN",   {"prefixes": ["NAUO28/NAUO37/"],
                              "uids": ["nauo:NAUO28/NAUO34", "nauo:NAUO28/NAUO38",
                                       "nauo:NAUO28/NAUO33", "nauo:NAUO28/NAUO35", "nauo:NAUO28/NAUO36"]},
     "DOC_IMPLIED", "head_face(=head_front) 引入时机指南未明说；相机支架安装其上，故归本工序"),
    ("HEAD-ELECTRONICS",     {"prefixes": [], "uids": ["nauo:NAUO28/NAUO32", "nauo:NAUO28/NAUO39",
                                                        "nauo:NAUO28/NAUO40", "nauo:NAUO28/NAUO41",
                                                        "nauo:NAUO28/NAUO42", "nauo:NAUO28/NAUO43"]},
     "DOC_EXPLICIT", "扬声器×2+装饰板×2+Odroid+9DOF Razor（Razor 指南列为可选，几何在场故绑定）"),
    ("HEAD-CLOSE",           {"prefixes": [], "uids": [], "unit_joins": ["head_front_group", "onto:head_back_group"]},
     "DOC_EXPLICIT", "3x M2x8 合壳（STEP 无此螺钉件）"),
    ("HEAD-TORSO-ASSEMBLY",  {"prefixes": [], "uids": [], "unit_joins": ["head", "onto:neck/chest"]},
     "DOC_IMPLIED", "指南头章以布线暗示头晚于躯干；作为终段集成工序"),
]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--occ", required=True, type=Path)
    ap.add_argument("--sop", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    a = ap.parse_args()

    occd = json.loads(a.occ.read_text(encoding="utf-8"))
    occs = occd["occurrences"]
    sop = json.loads(a.sop.read_text(encoding="utf-8"))
    sop_ops = {o["op"] for o in sop["operations"]}

    consumed = {}
    ops_out = []
    for op, spec, ev, note in MAPPING:
        members = []
        for o in occs:
            uid = o["occ_uid"]
            path = o["nauo_path"]
            hit = uid in spec.get("uids", ()) or any(
                path.startswith(p) for p in spec.get("prefixes", ()))
            if hit:
                if uid in consumed:
                    print(f"!! 双重消耗: {uid} 同时在 {consumed[uid]} 与 {op}", file=sys.stderr)
                    return 2
                consumed[uid] = op
                members.append({"occ_id": o["occ_id"], "occ_uid": uid,
                                "product": o["product"], "depth": o["depth"]})
        ops_out.append({
            "op": op,
            "in_official_sop": op in sop_ops or op == "HEAD-TORSO-ASSEMBLY",
            "evidence_class": ev,
            "note": note,
            "predecessors": PREDECESSORS[op],
            "consumes_leaves": len(members),
            "unit_joins": spec.get("unit_joins", []),
            "movers": members,
        })

    missing = [o for o in occs if o["occ_uid"] not in consumed]
    total = sum(len(x["movers"]) for x in ops_out)
    doc = {
        "$schema": "assembly-ontology.s3-op-binding/v2",
        "record_id": "POPPY-S3-BINDING-REBUILD-V001",
        "purpose": "官方指南工序 → occurrence 实例绑定（唯一消耗，闭合校验）。",
        "sop_source": str(a.sop),
        "coordinate_semantics": {"basis": "已知左右单元（hip/forearm L/R）世界质心裁定",
                                 "+X": "左", "+Y": "上", "+Z": "前后（未再细判）"},
        "closure": {"leaf_occurrences": len(occs), "consumed": total,
                    "unconsumed": [{"occ_uid": o["occ_uid"], "product": o["product"]} for o in missing],
                    "status": "PASS" if not missing and total == len(occs) else "FAIL"},
        "claim_boundary": [
            "绑定不是顺序认证：工序间全序中 DOC 未定的部分由后续连接图约束+S7 共同决定并逐章标注。",
            "unit_joins 是 S5 solver-unit 语义的集成动作声明，其运动五元组在 S6/S7 求出，本文件不含。",
            "指南提到而 STEP 没有的紧固件（48x M2x3、16x M2.5x4、3x M2x8 等）不在 movers 内，"
            "进 HOLD 台账，不得凭想象补件（灵犀平价合同第 3 条）。",
            "SMPS×2 与 head_face 的工序归属证据级低（DOC_IMPLIED/AMBIGUOUS），成片前须人工复核。",
        ],
        "operations": ops_out,
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {a.out}")
    print(f"  工序 {len(ops_out)}  消耗叶 {total}/{len(occs)}  闭合 {doc['closure']['status']}")
    if missing:
        for o in missing:
            print(f"  未消耗: {o['occ_uid']}  {o['product']}")
    return 0 if not missing else 1


if __name__ == "__main__":
    sys.exit(main())
