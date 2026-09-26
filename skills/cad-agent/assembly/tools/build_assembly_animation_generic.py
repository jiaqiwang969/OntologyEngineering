#!/usr/bin/env python3
"""S10：把冻结的已认证运动状态编译成装配动画。

**只消费已认证的状态链，动画层不发明任何运动**（K-GOV-01 的推论）。

冻结状态认证的是什么，动画就只能播什么：
  · 每个动件沿**它自己的轴**做单自由度平移，从 `+认证接近距离` 走到就位（0）
  · 在场集、轴、方向、行程和顺序必须来自同一份版本化状态
  · 普通时间窗一次只动一个件；仅冻结状态明示给出同一 `pair_id`
    的相邻两件共用时间窗同步合拢。不推断任何其他并发运动。
  · 状态模式精确消费冻结 `approach_mm`；仅 legacy/probe 模式使用全局上限。

在场时机（K-SCN-01）：件从它自己的运动开始时出场；已就位的常显。
序列感知认证与 film 审计消费真实剩余集；结论只覆盖状态中冻结的场景与区间。

未通过认证的动件（NO_RELEASE / 无共轴界面 / 未绑定）**不播运动**，用标注卡如实列出（K-DEL-03）。

用法：blender -b --factory-startup -P tools/build_assembly_animation.py -- [--out FILM_v001] [--state <pipeline-state.json>] [--fpm 12] [--cap 20]
"""
import copy
import hashlib
import json
import math
import os
import struct
import sys
import time
from pathlib import Path

import bpy
import mathutils

# 参数化移植（重建-20260829）：--case-dir 指向机器人目录（robot-config.v1.json 提供路径）；
# up 轴由配置/脚-头关键词推断；新增 unit_members 成组动件（S5 solver-unit 集成工序）。
argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


def arg(n, d):
    return argv[argv.index(n) + 1] if n in argv else d


CASE = Path(arg("--case-dir", "")).resolve()
if not CASE.is_dir():
    raise RuntimeError("--case-dir 必须指向机器人重建目录")
_cfg = json.loads((CASE / "robot-config.v1.json").read_text(encoding="utf-8"))
_P = _cfg["paths"]
ROOT = CASE
MESH = CASE / _P["mesh_dir"]
OCC = CASE / _P["s1_manifest"]
S7 = CASE / "本体/S7-扫掠认证.v2.json"
GRAPH = CASE / _P["s2_coax_graph"]
# OLSK 增量（2026-09-03，v005）：--playable 可指向延长行程的可播表（v2），默认仍是 v1（v004 行为不变）
PLAY = CASE / arg("--playable", "本体/S7-可播接近距离.v1.json")

OUTDIR = CASE / "装配动画" / arg("--out", "FILM_v001")
STATE_TEXT = arg("--state", "")
STATE_PATH = ((ROOT / STATE_TEXT).resolve() if STATE_TEXT and not Path(STATE_TEXT).is_absolute()
              else Path(STATE_TEXT).resolve() if STATE_TEXT else None)
FPM = int(arg("--fpm", 12))          # 每个动件的帧数（行程 ≤ FPM_REF_MM 时）
HOLD = int(arg("--hold", 6))         # 工序间停顿帧
# OLSK 增量（2026-09-03，v005）：帧数随行程线性加长到 FPM_MAX（行程 ≥ FPM_MAX_MM），让长行程以近似恒定速度可辨。
# 默认 FPM_MAX=FPM 即关闭（v004 行为不变）。只改表达层时间窗，不改任何认证距离/轴。
FPM_MAX = int(arg("--fpm-max", FPM))
# v010（2026-09-05 十段审片）：就位停顿、动件最小画面占比、视线不得平行插入轴、剖切回退改为隐藏、终章环绕
SEAT_HOLD = int(arg("--seat-hold", 0))               # 每个动件就位后停顿帧（就位姿态要看得见）
MIN_MOVER_FRAC = float(arg("--min-mover-frac", 0.0))  # 动件半径至少占可见画面半高的比例（推近镜头，K-SCN-10 门禁的构图侧）
AXIS_VIEW_MAX = float(arg("--axis-view-max", 1.0))    # |插入轴·视线| 上限（1.0 关闭）：沿视线的运动读作弹入
CUTAWAY_HIDE = int(arg("--cutaway-hide", 0))          # 剖切回退：挡在动件前的兄弟/配合件本组内直接隐藏而非 0.25 虚化
FINALE_ORBIT = float(arg("--finale-orbit-deg", 0.0))  # 终章相机环绕角度（0 关闭）
FPM_REF_MM = float(arg("--fpm-ref-mm", 20.0))
FPM_MAX_MM = float(arg("--fpm-max-mm", 150.0))


GHOST_ALPHA = float(arg("--ghost-alpha", 0.12))   # v005 用 0.05：0.12 的 DITHERED 半透明在大盖板上成"毛玻璃"
GHOST_MAX = int(arg("--ghost-max", 100000))
SHOTS_MAX = int(arg("--shots-max", 1))
VIS_SELECT = int(arg("--vis-select", 0))            # v005 用 4：几何评分前 N 个候选视向各渲一张低清 Cryptomatte，按实测可见占比定机位（K-SCN-10）
VIS_PCT = int(arg("--vis-pct", 12))                 # 可见性测量分辨率百分比              # v005 用 4：每章最多 4 个机位（按动件空间分组硬切）        # v005 用 8：每章只虚化遮挡最重的几件（v004 单章虚化 33–85 件→整机"X 光"）
MATE_GHOST = float(arg("--mate-ghost-alpha", 0.0))  # v005 用 0.3：动件整体在配合件包围盒内部（管内衬板）时，配合件在其运动窗内虚化
BENCH_GHOST = float(arg("--bench-ghost", 0.0))
BENCH_GHOST_R = float(arg("--bench-ghost-radius", 0.0))   # v008：极淡台架件只在动件足迹的 N 倍半径内显示，其余本组内不显示（近景"雾"）
FRAME_MARGIN = float(arg("--frame-margin", 1.10))         # v008 用 1.22：构图余量（审片：件贴画面下缘）
VIS_PROBES = int(arg("--vis-probes", 1))                  # v008 用 3：实测可见性探针数（最小件+首件+末件）      # v005 用 0.04：非本台架闭包的已装件以极淡在场作空间参照（台架语义不变：它们"不在场"）
METAL_CAP = float(arg("--metal-cap", 1.0))          # v005 用 0.55：Metallic 1.0/Roughness 0.15 的抛光铝在暗色世界里映成近黑（10 段审片一致报"就位即消失"）
ROUGH_FLOOR = float(arg("--rough-floor", 0.0))      # v005 用 0.40
WORLD_GREY = float(arg("--world", 0.10))            # v005 用 0.30：环境光提亮，金属有东西可反射
ALBEDO_CAP = float(arg("--albedo-cap", 1.0))
EMIT_GAIN = float(arg("--emit-gain", 2.5))            # v008 用 1.2：(R-1)×2.5 的橙色自发光把小件糊成无明暗的平色块（审片：继电器/断路器"扁平剪影"）         # v008 用 0.78：抛光铝 0.91 在 0.30 世界里成"白雾"（终章审片），封顶后仍是浅灰而有明暗
OVERLAY_TOP = int(arg("--overlay-top", 0))          # 字幕带高度（px）：构图只用带间的可见区
OVERLAY_BOTTOM = int(arg("--overlay-bottom", 0))


def fpm_for(approach_mm):
    if FPM_MAX <= FPM or FPM_MAX_MM <= FPM_REF_MM:
        return FPM
    u = (float(approach_mm) - FPM_REF_MM) / (FPM_MAX_MM - FPM_REF_MM)
    return int(round(FPM + (FPM_MAX - FPM) * min(1.0, max(0.0, u))))
CAP = float(arg("--cap", 20.0))      # 全局接近上限 mm
RES = (int(arg("--rx", 1600)), int(arg("--ry", 900)))
CHLIMIT = int(arg("--chapters", 0))
PROBE = "--probe" in argv   # 探针轮：不消费可播表，直接用 min(认证接近, 上限)，供审计取首次非基线接触
FPS = 30


def log(m):
    print(f"[anim] {m}", flush=True)


def interp(kind):
    """插值类型在插入时定。Blender 5.x 的 Action 改成 slotted，没有 action.fcurves，
    事后遍历 fcurve 改插值的老写法直接报 AttributeError。"""
    bpy.context.preferences.edit.keyframe_new_interpolation_type = kind


def read_stl(p):
    with open(p, "rb") as f:
        f.read(80)
        n = struct.unpack("<I", f.read(4))[0]
        buf = f.read(n * 50)
    v, fa = [], []
    for i in range(n):
        o = i * 50 + 12
        t = struct.unpack_from("<9f", buf, o)
        b = len(v)
        v += [t[0:3], t[3:6], t[6:9]]
        fa.append((b, b + 1, b + 2))
    return v, fa


def mkmat(name, rgba, rough=0.45):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = rgba
    b.inputs["Roughness"].default_value = rough
    if rgba[3] < 1.0:
        m.blend_method = "BLEND"
        b.inputs["Alpha"].default_value = rgba[3]
    return m


def load_frozen_motion_state(path):
    """把冻结状态编译为渲染时间线，并在进入 Blender 前闭合结构不变式。

    这里只做状态合同校验，不重算顺序、轴、方向或行程。
    """
    frozen_bytes = path.read_bytes()
    frozen = json.loads(frozen_bytes.decode("utf-8"))
    if frozen.get("$schema") not in {
        "x1.assembly-film-pipeline-state/v1",
        "x1.assembly-film-motion-plan/v1",
        "assembly-ontology.film-pipeline-state/v2",
        "assembly-ontology.film-motion-plan/v2",
    }:
        raise RuntimeError(f"不支持的冻结状态 schema: {frozen.get('$schema')}")
    if frozen.get("$schema") == "x1.assembly-film-motion-plan/v1" \
            and frozen.get("status") != "READY_FOR_CREATE_ONLY_EXECUTION":
        raise RuntimeError(f"motion plan 未 READY: {frozen.get('gates')}")
    if frozen.get("$schema") == "x1.assembly-film-pipeline-state/v1" \
            and frozen.get("status") == "FAIL":
        raise RuntimeError("拒绝消费 FAIL pipeline state")
    assembly = frozen.get("assembly")
    if not isinstance(assembly, dict):
        raise RuntimeError("冻结状态缺 assembly")
    if assembly.get("$schema") not in {"x1.frozen-assembly-motion-state/v1",
                                       "assembly-ontology.frozen-assembly-motion-state/v2"}:
        raise RuntimeError(f"不支持的 assembly schema: {assembly.get('$schema')}")

    timeline = []
    seen_ops = set()
    seen_movers = set()
    for operation in assembly.get("operations", []):
        op_name = operation.get("op")
        if not isinstance(op_name, str) or not op_name:
            raise RuntimeError("冻结状态含无效工序名")
        if op_name in seen_ops:
            raise RuntimeError(f"冻结状态含重复工序: {op_name}")
        seen_ops.add(op_name)
        movers = []
        for item in operation.get("movers", []):
            occ = item.get("occ")
            if not isinstance(occ, str) or not occ:
                raise RuntimeError(f"{op_name}: 冻结动件缺 occ")
            if occ in seen_movers:
                raise RuntimeError(f"冻结状态含重复动件: {occ}")
            axis = item.get("insertion_axis_world")
            if not isinstance(axis, list) or len(axis) != 3:
                raise RuntimeError(f"{op_name}/{occ}: 冻结轴必须是 3 维")
            try:
                axis = [float(value) for value in axis]
                approach = float(item["approach_mm"])
                sense = int(item["withdraw_sense"])
            except (KeyError, TypeError, ValueError) as exc:
                raise RuntimeError(f"{op_name}/{occ}: 冻结运动字段无效") from exc
            if not all(math.isfinite(value) for value in axis) or sum(value * value for value in axis) <= 0:
                raise RuntimeError(f"{op_name}/{occ}: 冻结轴非有限非零向量")
            if not math.isfinite(approach) or approach < 0:
                raise RuntimeError(f"{op_name}/{occ}: 冻结行程必须是非负有限数")
            if sense not in (-1, 1):
                raise RuntimeError(f"{op_name}/{occ}: withdraw_sense 必须为 -1 或 1")
            if not item.get("approach_source"):
                raise RuntimeError(f"{op_name}/{occ}: 冻结状态缺 approach_source")
            mover = dict(item)
            mover["insertion_axis_world"] = axis
            mover["withdraw_sense"] = sense
            mover["playable_approach_mm"] = approach
            mover.pop("approach_mm", None)
            movers.append(mover)
            seen_movers.add(occ)
        if not movers and not operation.get("presence_only"):
            raise RuntimeError(f"{op_name}: 冻结工序不得为空")
        entry = {"op": op_name, "movers": movers}
        if operation.get("presence_only"):
            entry["presence_only"] = True
            entry["note"] = operation.get("note", "")
        timeline.append(entry)

    skipped = copy.deepcopy(assembly.get("not_animated", []))
    skipped_ids = set()
    skipped_count = 0
    for item in skipped:
        occs = item.get("occs", [])
        if not isinstance(occs, list) or item.get("n") != len(occs):
            raise RuntimeError(f"{item.get('op', '?')}: not_animated 计数与 occs 不一致")
        duplicate = skipped_ids.intersection(occs)
        if duplicate:
            raise RuntimeError(f"not_animated 含重复件: {sorted(duplicate)}")
        skipped_ids.update(occs)
        skipped_count += len(occs)
    overlap = seen_movers & skipped_ids
    if overlap:
        raise RuntimeError(f"动件同时被列为 not_animated: {sorted(overlap)}")

    summary = assembly.get("summary", {})
    actual = {
        "operations": sum(1 for t in timeline if not t.get("presence_only")),
        "movers": len(seen_movers),
        "not_animated": skipped_count,
    }
    if any(summary.get(key) != value for key, value in actual.items()):
        raise RuntimeError(f"冻结状态 summary 不一致: declared={summary}, actual={actual}")
    return frozen, assembly, timeline, skipped, hashlib.sha256(frozen_bytes).hexdigest()


def main():
    t0 = time.time()
    if STATE_PATH and PROBE:
        raise RuntimeError("--state 与 --probe 互斥；冻结状态不得切回探针运动")
    if CHLIMIT < 0:
        raise RuntimeError("--chapters 不得为负数")
    if STATE_PATH and OUTDIR.exists() and any(OUTDIR.iterdir()) and "--allow-existing-output" not in argv:
        raise RuntimeError(f"状态模式拒绝复用非空输出目录: {OUTDIR}")
    OUTDIR.mkdir(parents=True, exist_ok=True)
    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads(OCC.read_text(encoding="utf-8"))["occurrences"]
    s7 = None if STATE_PATH else json.loads(S7.read_text(encoding="utf-8"))
    graph = json.loads(GRAPH.read_text(encoding="utf-8"))
    mates = {}
    for pr in graph["occurrence_pairs"]:
        mates.setdefault(pr["a_occ"], set()).add(pr["b_occ"])
        mates.setdefault(pr["b_occ"], set()).add(pr["a_occ"])
    occ_index = {o["occ_id"]: i for i, o in enumerate(occ)}
    idx2id = {i: o["occ_id"] for i, o in enumerate(occ)}
    id2occ = {o["occ_id"]: o for o in occ}

    # S5 折叠单元：实体动件 = Blender 对象组（成员同轴同窗刚体平移）；
    # 全叶绑定（Poppy/X1）时 ent_members 恒等，行为与原版一致。
    _bind = json.loads((CASE / _P["s3_binding"]).read_text(encoding="utf-8"))
    ent_members, member_of = {}, {}
    for _op0 in _bind["operations"]:
        for _m0 in _op0["movers"]:
            mem = _m0.get("member_occ_ids") or [_m0["occ_id"]]
            ent_members[_m0["occ_id"]] = list(mem)
            for _mid in mem:
                member_of[_mid] = _m0["occ_id"]

    def members_of(oid):
        return ent_members.get(oid, [oid])

    import re
    import unicodedata
    CODE = re.compile(r"^[0-9A-Z]{6}_")

    def nrm(x):
        return unicodedata.normalize("NFKC", str(x or "").strip()).replace("╱", "/")

    lut = {}
    for stem, v in man.items():
        if "error" in v and "sha256" not in v:
            continue
        nm = nrm(v.get("name") or stem)
        for k in (nm, CODE.sub("", nm)):
            lut.setdefault(k, stem)

    def stem_for(prod):
        s0 = nrm(prod)
        cands = [s0, CODE.sub("", s0)]
        if "_" in s0:
            b, suf = s0.rsplit("_", 1)
            if re.match(r"^(?:M|ST|Ø|φ)?\s*\d+(?:\.\d+)?\s*(?:[×xX*]\s*\d+(?:\.\d+)?)?\s*$", suf):
                cands += [b, CODE.sub("", b)]
        for k in cands:
            if k in lut:
                return lut[k]
        return None

    # S8 连接图:未绑定紧固件的被夹集(上下文出场时序用)
    G8_F = ROOT / "本体/S8-连接关系图.v1.json"
    fastens_g8 = {}
    if G8_F.exists():
        _g8 = json.loads(G8_F.read_text(encoding="utf-8"))
        _unbound = {n["occ"] for n in _g8["nodes"] if not n.get("op")}
        for _f, _cl in _g8.get("fastens", {}).items():
            if _f in _unbound:
                fastens_g8[_f] = set(_cl)
    if STATE_PATH:
        frozen, assembly, timeline, skipped, authority_sha256 = load_frozen_motion_state(STATE_PATH)
        log(f"冻结状态 {STATE_PATH.name}: {assembly['summary']['movers']} 动件")
        if "--cap" in argv:
            log("--state 模式精确消费冻结 approach_mm；忽略 --cap，不重裁行程")
    else:
        # 兼容迁移入口。新运行应先由 assembly_film.py 冻结状态，再走上面的只读分支。
        play = {} if PROBE else {r["occ"]: r for r in json.loads(PLAY.read_text(encoding="utf-8"))["rows"]}
        ORDER_F = ROOT / "本体/S7-工序内播放序.v1.json"
        within = ({} if PROBE or not ORDER_F.exists()
                  else json.loads(ORDER_F.read_text(encoding="utf-8"))["ops"])
        timeline, skipped = [], []
        for op in s7["operations"]:
            good = []
            for m in op["per_mover"]:
                pr = play.get(m["occ"])
                axis_repicked = bool(pr and pr.get("playable")
                                     and str(pr.get("source", "")).startswith(
                                         ("AXIS_RE", "SEQUENCE_AWARE",
                                          "PAIR_CLOSURE", "SLOT_SEARCH")))
                if not axis_repicked and (m.get("verdict") != "RELEASES"
                                          or m.get("certified_approach_mm") is None):
                    continue
                if PROBE:
                    if m.get("verdict") != "RELEASES" or m.get("certified_approach_mm") is None:
                        continue
                    pr = {"playable_approach_mm": min(float(m["certified_approach_mm"]), CAP),
                          "source": "PROBE_CAPPED", "playable": True}
                if pr is None or not pr["playable"]:
                    continue
                m = dict(m)
                m["playable_approach_mm"] = pr["playable_approach_mm"]
                m["approach_source"] = pr["source"]
                m["pair_id"] = pr.get("pair_id")
                if pr.get("insertion_axis_world"):
                    m["insertion_axis_world"] = pr["insertion_axis_world"]
                    m["withdraw_sense"] = pr["withdraw_sense"]
                good.append(m)
            sib_blocked = set()
            w = within.get(op["op"])
            if w and good:
                gm = {m["occ"]: m for m in good}
                new_good = []
                for entry in w["order"]:
                    m2 = gm.pop(entry["occ"], None)
                    if m2 is None:
                        continue
                    m2["playable_approach_mm"] = entry["approach_mm"]
                    if entry.get("trimmed_for_siblings") and \
                            "TRIMMED_FOR_SIBLINGS" not in m2["approach_source"].split("+"):
                        m2["approach_source"] += "+TRIMMED_FOR_SIBLINGS"
                    new_good.append(m2)
                sib_blocked = set(gm)
                if sib_blocked:
                    log(f"{op['op']}: 播放序未列而弃 {sorted(sib_blocked)}")
                good = new_good
            gset = {m["occ"] for m in good}
            bad = [m for m in op["per_mover"] if m["occ"] not in gset]
            if bad:
                rs = set()
                for m in bad:
                    pr = play.get(m["occ"])
                    if m["occ"] in sib_blocked:
                        rs.add("SIBLING_BLOCKED_IN_FILM_SCENE")
                    elif m.get("verdict") != "RELEASES":
                        rs.add(m.get("verdict") or m.get("klass"))
                    elif pr is None:
                        rs.add("NOT_AUDITED")
                    elif not pr["playable"]:
                        rs.add("NO_PLAYABLE_APPROACH_NON_BASELINE_CONTACT")
                skipped.append({"op": op["op"], "n": len(bad), "reasons": sorted(rs),
                                "occs": [m2["occ"] for m2 in bad]})
            if good:
                timeline.append({"op": op["op"], "movers": good})
    if CHLIMIT:
        timeline = timeline[:CHLIMIT]
        included_ops = {item["op"] for item in timeline}
        skipped = [item for item in skipped if item.get("op") in included_ops]
        log(f"章节限制: 仅编译前 {len(timeline)} 道可播工序（诊断子集）")
    n_mov = sum(len(c["movers"]) for c in timeline)
    log(f"可播工序 {len(timeline)}  可播动件 {n_mov}  不可播动件 {sum(s['n'] for s in skipped)}")
    moving_ids = {m["occ"] for c in timeline for m in c["movers"]}
    moving_member_ids = {mid for oid in moving_ids for mid in members_of(oid)}
    if STATE_PATH:
        missing_occ = sorted(oid for oid in moving_ids
                             if oid not in id2occ and oid not in ent_members)
        if missing_occ:
            raise RuntimeError(f"冻结动件在当前 S1/绑定不存在: {missing_occ}")
        missing_mesh = []
        empty_geometry_members = []   # OLSK 增量：STEP 定义本身无几何（manifest error=EMPTY_BBOX）的单元成员，如实列账、不显示、不发明
        _err_by_name = {v.get("name"): v.get("error") for v in man.values() if isinstance(v, dict) and "error" in v}
        for oid in sorted(moving_ids):
            for mid in members_of(oid):
                stem = stem_for(id2occ[mid]["product"])
                if stem is None or not (MESH / f"{stem}.stl").is_file():
                    if _err_by_name.get(id2occ[mid]["product"]) == "EMPTY_BBOX" and mid != oid:
                        empty_geometry_members.append({"member": mid, "unit": oid, "product": id2occ[mid]["product"]})
                    else:
                        missing_mesh.append(mid)
        if missing_mesh:
            raise RuntimeError(f"冻结动件缺可显示真几何: {missing_mesh}")
        if empty_geometry_members:
            log(f"单元成员无几何（EMPTY_BBOX，按 EMPTY_DEFINITION 列账不显示）: {len(empty_geometry_members)} 个")

    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    engines = [i.identifier for i in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items]
    sc.render.engine = "BLENDER_EEVEE_NEXT" if "BLENDER_EEVEE_NEXT" in engines else "BLENDER_EEVEE"
    sc.render.resolution_x, sc.render.resolution_y = RES
    sc.render.fps = FPS
    sc.world = bpy.data.worlds.new("W")
    sc.world.use_nodes = True
    sc.world.node_tree.nodes["Background"].inputs[0].default_value = (WORLD_GREY, WORLD_GREY * 1.05, WORLD_GREY * 1.2, 1)   # OLSK：略提亮，让金属外观有环境可反射（v005 --world 0.30）

    # 一套材质，颜色与透明度都从 Object Info 取——这样"未装/运动中/已就位"可以靠
    # ob.color 逐帧关键帧化。给每个件挂一套材质再逐帧换 slot 是做不到关键帧的。
    # OLSK 增量（2026-09-03）：材质 = STEP 外观颜色 × 状态色（Object Info Color），状态语义不变：
    #   ob.color 白 (1,1,1) = 已就位显示真实外观；暗 = 未装上下文；运动中用 >1 的暖色乘子 + 由 R-1 驱动的橙色自发光。
    #   每种 (颜色, 金属度, 粗糙度) 一份材质，Alpha 仍走 Object Info（遮挡者虚化），DITHERED 写深度。
    _colors_p = CASE / "evidence/step-appearance-map.v1.json"
    _cmap = json.loads(_colors_p.read_text(encoding="utf-8"))["products"] if _colors_p.exists() else {}
    _mat_cache = {}

    def material_for(product):
        key = _cmap.get(product) or _cmap.get(product.split("#")[0]) or _cmap.get(product.split("#")[0].replace(" [self]", "")) \
              or {"rgb": [0.62, 0.64, 0.67], "metallic": 0.0, "roughness": 0.45, "name": "default"}
        k = (tuple(round(v, 3) for v in key["rgb"]), round(key.get("metallic", 0.0), 2), round(key.get("roughness", 0.45), 2), round(key.get("alpha", 1.0), 2))
        if k in _mat_cache:
            return _mat_cache[k]
        M = bpy.data.materials.new(f"step_{len(_mat_cache):03d}")
        M.use_nodes = True
        nt = M.node_tree
        bsdf = nt.nodes["Principled BSDF"]
        oi = nt.nodes.new("ShaderNodeObjectInfo"); oi.location = (-700, 0)
        base = nt.nodes.new("ShaderNodeRGB"); base.location = (-700, -250)
        base.outputs[0].default_value = (min(k[0][0], ALBEDO_CAP), min(k[0][1], ALBEDO_CAP), min(k[0][2], ALBEDO_CAP), 1.0)
        mix = nt.nodes.new("ShaderNodeMix"); mix.location = (-400, -100)
        mix.data_type = 'RGBA'; mix.blend_type = 'MULTIPLY'; mix.inputs["Factor"].default_value = 1.0
        nt.links.new(base.outputs[0], mix.inputs[6]); nt.links.new(oi.outputs["Color"], mix.inputs[7])
        nt.links.new(mix.outputs[2], bsdf.inputs["Base Color"])
        sep = nt.nodes.new("ShaderNodeSeparateColor"); sep.location = (-700, 250)
        nt.links.new(oi.outputs["Color"], sep.inputs[0])
        sub = nt.nodes.new("ShaderNodeMath"); sub.operation = 'SUBTRACT'; sub.inputs[1].default_value = 1.0; sub.location = (-500, 250)
        nt.links.new(sep.outputs[0], sub.inputs[0])
        mul = nt.nodes.new("ShaderNodeMath"); mul.operation = 'MULTIPLY'; mul.inputs[1].default_value = EMIT_GAIN; mul.use_clamp = True; mul.location = (-300, 250)
        nt.links.new(sub.outputs[0], mul.inputs[0])
        bsdf.inputs["Emission Color"].default_value = (1.0, 0.45, 0.12, 1.0)
        nt.links.new(mul.outputs[0], bsdf.inputs["Emission Strength"])
        if k[3] < 1.0:   # 透明件（Polycarbonate Clear）：材质 α = Object Info α × 定值，虚化语义保留
            am = nt.nodes.new("ShaderNodeMath"); am.operation = 'MULTIPLY'; am.inputs[1].default_value = k[3]; am.location = (-300, -400)
            nt.links.new(oi.outputs["Alpha"], am.inputs[0]); nt.links.new(am.outputs[0], bsdf.inputs["Alpha"])
            # v008：透明件运动中不加自发光——发光面在 EEVEE 里看起来是实心奶白（审片：前窗作为动件时"完全不透明"），
            # 橙色运动语义仍由基色乘子表达
            mul.inputs[1].default_value = 0.0
        else:
            nt.links.new(oi.outputs["Alpha"], bsdf.inputs["Alpha"])
        bsdf.inputs["Metallic"].default_value = min(k[1], METAL_CAP)
        bsdf.inputs["Roughness"].default_value = max(k[2], ROUGH_FLOOR)
        if hasattr(M, "surface_render_method"):
            M.surface_render_method = 'DITHERED'
        else:
            M.blend_method = 'HASHED'
        _mat_cache[k] = M
        return M

    M = material_for("__default__")
    bsdf = M.node_tree.nodes["Principled BSDF"]
    # **全部不透明**。第一版把未装件做成半透明（BLENDED），1558 个件层层叠加把画面糊成
    # 一片均匀的雾，就位的灰件和运动中的橙件全被压没了——EEVEE 的混合面不写深度。
    # 而且未装件**不出场**（K-SCN-01：件从它自己的工步起出场）。
    # 让它们以暗色在场试过一版：橙/灰的动件都埋在壳体内部，从外面根本看不见。
    # 尚无动作的件在工序段只能作为静态上下文出场；终章再统一显示
    # 当前成功载入的全部 CAD 实例。这两种呈现都不表示它们的安装动作已被认证。
    C_PEND = (0.16, 0.17, 0.19, 1.0)      # 未装上下文：真实外观 × 0.16（暗）
    C_MOVE = (1.55, 1.0, 0.55, 1.0)       # 运动中：暖色乘子，R>1 触发橙色自发光（材质内 (R-1)×2.5）
    C_INST = (1.0, 1.0, 1.0, 1.0)         # 已就位：真实 STEP 外观

    meshes = {}
    objs = {}
    for o in occ:
        st = stem_for(o["product"])
        if st is None:
            continue
        # **显示一律用真几何，不用凸包代理。**
        # 代理是 S7 碰撞用的器具（保守包住真件），不是显示用的：
        # OmniPicker 的凸包把该章构图撑到 2423mm，画面里只剩一个大блоб。
        # 碰撞结论已由 S7 出完，动画阶段不需要代理。
        use = st
        if use not in meshes:
            v, f = read_stl(MESH / f"{use}.stl")
            me = bpy.data.meshes.new(use)
            me.from_pydata(v, [], f)
            me.validate()
            meshes[use] = me
        ob = bpy.data.objects.new(o["occ_id"], meshes[use])
        W = o["world"]
        ob.matrix_world = mathutils.Matrix([W[0], W[1], W[2], [0, 0, 0, 1]])
        sc.collection.objects.link(ob)
        if not ob.data.materials:
            ob.data.materials.append(material_for(o["product"]))
        ob.color = C_PEND
        ob.hide_viewport = ob.hide_render = True      # 默认不出场
        objs[o["occ_id"]] = ob
    log(f"对象 {len(objs)}  网格 {len(meshes)}  {time.time()-t0:.0f}s")

    def objs_of(oid):
        return [objs[mid] for mid in members_of(oid) if mid in objs]

    # ---- 时间线：单件窗，只允许冻结 pair_id 的两件同步 -------------------------
    # ---- 台架可见性语义（模块化装配）----------------------------------------
    # 每章可见集 = 前驱闭包成员 + 本章逐步出场的成员；台架章结束后其件隐藏
    # （"装完放一边"），到含它的结合章/终章重现。否则其他台架的件在最终位姿常显，
    # 动件飞入会穿过画面上可见的它们（film 审计实测 39 例假象/真象混杂的穿模）。
    preds_map = (assembly.get("predecessors", {}) if STATE_PATH else {})
    op_members = {}
    for operation in (assembly.get("operations", []) if STATE_PATH else []):
        op_members[operation["op"]] = {m["occ"] for m in operation.get("movers", [])}
    for item in (assembly.get("not_animated", []) if STATE_PATH else []):
        op_members.setdefault(item["op"], set()).update(item.get("occs", []))

    def bench_closure(opn):
        acc, stack = set(), [opn]
        while stack:
            cur = stack.pop()
            for p2 in preds_map.get(cur, []):
                if p2 not in acc:
                    acc.add(p2)
                    stack.append(p2)
        out = set()
        for p2 in acc:
            out |= op_members.get(p2, set())
        return out

    bench_ghosted = set()

    def set_alpha(ob, fr, a_prev, a_new):
        """对象色 α 关键帧：fr-1 保持 a_prev，fr 起 a_new（常量插值）。RGB 用就位色（真实外观）。"""
        interp("CONSTANT")
        ob.color = (1.0, 1.0, 1.0, a_prev)
        ob.keyframe_insert("color", frame=max(1, fr - 1))
        ob.color = (1.0, 1.0, 1.0, a_new)
        ob.keyframe_insert("color", frame=fr)
        interp("BEZIER")

    def set_vis(ob, fr, visible):
        interp("CONSTANT")
        ob.hide_viewport = ob.hide_render = visible      # 前一帧保持旧状态
        ob.keyframe_insert("hide_viewport", frame=max(1, fr - 1))
        ob.keyframe_insert("hide_render", frame=max(1, fr - 1))
        ob.hide_viewport = ob.hide_render = not visible
        ob.keyframe_insert("hide_viewport", frame=fr)
        ob.keyframe_insert("hide_render", frame=fr)
        interp("BEZIER")

    visible_now = set()
    frame = 1
    chapters = []
    ctx_shown = set()
    for c in timeline:
        closure_ids = bench_closure(c["op"]) if STATE_PATH else set()
        own_ids = op_members.get(c["op"], set()) if STATE_PATH else {m["occ"] for m in c["movers"]}
        # 章首可见性重置：前驱闭包出场，非闭包非本章件退场
        if STATE_PATH:
            for oid in sorted(closure_ids - visible_now):
                for ob2 in objs_of(oid):
                    if BENCH_GHOST > 0 and oid in bench_ghosted:
                        set_alpha(ob2, frame, BENCH_GHOST, 1.0)
                    else:
                        set_vis(ob2, frame, True)
                bench_ghosted.discard(oid)
            for oid in sorted(visible_now - closure_ids - own_ids):
                for ob2 in objs_of(oid):
                    if BENCH_GHOST > 0:
                        # v005：非本台架件不隐藏而是极淡在场（空间参照）；语义仍是"不在场"，审计口径不变
                        set_alpha(ob2, frame, 1.0, BENCH_GHOST)
                    else:
                        set_vis(ob2, frame, False)
                if BENCH_GHOST > 0:
                    bench_ghosted.add(oid)
            visible_now = (visible_now & own_ids) | {o2 for o2 in closure_ids if objs_of(o2)}
        chapter_visible_ids = set(visible_now)
        if c.get("presence_only"):
            ch = {"op": c["op"], "start": frame, "movers": [], "presence_only": True,
                  "note": c.get("note", ""), "context": [], "context_deferred_fasteners": {},
                  "static_withdrawn": [], "closure_members": sorted(closure_ids)}
            frame += 2 * HOLD + FPM
            ch["end"] = frame
            chapters.append(ch)
            continue
        ch = {"op": c["op"], "start": frame, "movers": []}
        if STATE_PATH:
            ch["visible_ids"] = sorted(chapter_visible_ids)
        prev_pair = None
        prev_f0 = frame
        for m in c["movers"]:
            group = objs_of(m["occ"])
            if not group:
                log(f"!! 场景缺对象 {c['op']} {m['occ']}")
                continue
            ax = mathutils.Vector(m["insertion_axis_world"]).normalized()
            approach = float(m["playable_approach_mm"])
            off = ax * (m["withdraw_sense"] * approach)
            # 耦合对（PAIR_CLOSURE）：两半共用同一时间窗，同步合拢
            fpm_m = fpm_for(approach)
            if m.get("pair_id") and m["pair_id"] == prev_pair:
                f0, f1 = prev_f0, frame           # 复用上一件的窗口
            else:
                f0, f1 = frame, frame + fpm_m
            prev_f0 = f0
            prev_pair = m.get("pair_id")
            # 出现：从 f0 起可见（K-SCN-01：件从它自己的运动开始时出场）。
            # 折叠单元 = 对象组：全体成员同轴同窗刚体平移（各自 home + 公共 off）
            for ob in group:
                home = ob.matrix_world.translation.copy()
                interp("CONSTANT")
                ob.hide_viewport = ob.hide_render = True
                ob.keyframe_insert("hide_viewport", frame=max(1, f0 - 1))
                ob.keyframe_insert("hide_render", frame=max(1, f0 - 1))
                ob.hide_viewport = ob.hide_render = False
                ob.keyframe_insert("hide_viewport", frame=f0)
                ob.keyframe_insert("hide_render", frame=f0)
                interp("LINEAR")   # v005：匀速平移（贝塞尔缓入缓出让 12 帧窗前半段几乎不动，审片报"弹入"）
                ob.location = home + off
                ob.keyframe_insert("location", frame=f0)
                ob.location = home
                ob.keyframe_insert("location", frame=f1)
                interp("BEZIER")
            ch["movers"].append({"occ": m["occ"],
                                 "member_count": len(group),
                                 "member_occs": (members_of(m["occ"])
                                                 if len(group) > 1 else None),
                                 "product": m.get("product") or occ_prod(id2occ, m["occ"]),
                                 # 轴与方向必须与偏移**同组落盘**：下游审计只能整组消费这一份，
                                 # 不能一半从这里取、一半回上游 S7 取——换轴的动件会被拼成旧轴配新偏移。
                                 "insertion_axis_world": [round(float(v), 9) for v in ax],
                                 "withdraw_sense": int(m["withdraw_sense"]),
                                 "approach_mm": round(approach, 3),
                                 "certified_approach_mm": m.get("certified_approach_mm"),
                                 "approach_source": m["approach_source"],
                                 "pair_id": m.get("pair_id"),
                                 "trimmed_for_penetration":
                                     m["approach_source"] == "TRIMMED_TO_FIRST_NON_BASELINE",
                                 "rule": m.get("direction_rule"), "f0": f0, "f1": f1})
            frame = f1 + SEAT_HOLD
            visible_now.add(m["occ"])
        # 上下文：本章动件的**共轴配合对手**。它们是这些件真正装到的对象，
        # 身份由 S2 认证过，但**工序归属未定**——所以只作为静态上下文出现，
        # 从本章起常显、不动、暗色，不主张"它们此时才被装上"。
        own_skipped = {o2 for s2 in skipped if s2["op"] == c["op"] for o2 in s2.get("occs", [])}
        ctx = set()
        for m in ch["movers"]:
            memset = set(members_of(m["occ"]))
            for mid in memset:
                i = occ_index.get(mid)
                for j in mates.get(i, ()):
                    oid = idx2id[j]
                    if oid in memset:
                        continue                      # 单元内部界面不作上下文
                    if member_of.get(oid, oid) in moving_ids:
                        continue                      # 动件（或其单元成员）不作上下文
                    # 台架语义：上下文只允许闭包内或本章成员——跨台架的配合对手此刻不在场。
                    # 本章 not_animated 件不走 context（章首出场会被随后动件穿过）；
                    # 它们统一按 static_withdrawn 在全部动件就位后出现。
                    if STATE_PATH and oid not in closure_ids and oid not in own_ids:
                        continue
                    if oid in own_skipped:
                        continue
                    ctx.add(oid)
        # 图拓扑在场时机（用户抓出的第三个盲区之后的修正）：夹本章动件的**未绑定紧固件**
        # 按连接拓扑必然晚于被夹件安装——它们的上下文出场推迟到被夹件就位（f1）之后，
        # 否则画面上是"螺钉先站好、摆臂再从中间挤进来"。其余上下文仍从章首出场。
        seat_of = {m["occ"]: m["f1"] for m in ch["movers"]}
        interp("CONSTANT")
        ctx_appear = {}
        for oid in sorted(ctx):
            ob = objs.get(oid)
            if ob is None or oid in ctx_shown:
                continue
            ctx_shown.add(oid)
            appear = ch["start"]
            clamped_here = [c for c in fastens_g8.get(oid, ()) if c in seat_of]
            if clamped_here:
                appear = max(seat_of[c] for c in clamped_here)
            ctx_appear[oid] = appear
            ob.hide_viewport = ob.hide_render = True
            ob.keyframe_insert("hide_viewport", frame=max(1, appear - 1))
            ob.keyframe_insert("hide_render", frame=max(1, appear - 1))
            ob.hide_viewport = ob.hide_render = False
            ob.keyframe_insert("hide_viewport", frame=appear)
            ob.keyframe_insert("hide_render", frame=appear)
            visible_now.add(oid)
        interp("BEZIER")
        ch["context"] = sorted(ctx)
        ch["context_deferred_fasteners"] = {k: v for k, v in ctx_appear.items()
                                            if v > ch["start"]}
        # 退场绑定件静态呈现：以暗色就位态出现，标注"未表达运动"。
        # 出现时机改为**本章全部动件就位之后**（章首出现会让随后的支架/盖板动件
        # 穿过画面上已站好的它——v002 film 审计 8 处穿模中 6 处即此因）。
        wd = [o2 for s2 in skipped if s2["op"] == c["op"] for o2 in s2.get("occs", [])]
        shown_w = []
        wd_frame = max([m["f1"] for m in ch["movers"]], default=ch["start"])
        interp("CONSTANT")
        for oid in wd:
            group_w = objs_of(oid)
            if not group_w or oid in ctx_shown:
                continue
            ctx_shown.add(oid)
            for ob in group_w:
                ob.hide_viewport = ob.hide_render = True
                ob.keyframe_insert("hide_viewport", frame=max(1, wd_frame - 1))
                ob.keyframe_insert("hide_render", frame=max(1, wd_frame - 1))
                ob.hide_viewport = ob.hide_render = False
                ob.keyframe_insert("hide_viewport", frame=wd_frame)
                ob.keyframe_insert("hide_render", frame=wd_frame)
            shown_w.append(oid)
            visible_now.add(oid)
        interp("BEZIER")
        ch["static_withdrawn"] = shown_w
        ch["static_withdrawn_appear_frame"] = wd_frame
        frame += HOLD
        ch["end"] = frame
        chapters.append(ch)

    # ---- 终章：整机 CAD 装配态 ---------------------------------------------
    # 用户要求"最终要看到完整装配"。S1 位姿是定义真值：终章让**全部件**在场、
    # 统一就位色，呈现完整装配态。它不表达任何运动，也不主张未表达工序被完成——
    # 只陈述"装配完成后的整机长这样"（claim_boundary 记入链）。
    interp("CONSTANT")
    fin = {"op": "整机装配态", "start": frame, "movers": [], "finale": True,
           "note": "整机 CAD 装配态（含未在本片工序中表达的件）；无运动主张"}
    already_visible = (set(visible_now) if STATE_PATH
                       else {m["occ"] for c in chapters for m in c["movers"]} | set(ctx_shown))
    already_visible = {mid for oid in already_visible for mid in members_of(oid)}
    for oid, ob in objs.items():
        if oid not in already_visible:
            # 只对从未出场的件打显隐键；已在场件由既有关键帧保持，避免终章前一帧闪隐
            ob.hide_viewport = ob.hide_render = True
            ob.keyframe_insert("hide_viewport", frame=frame - 1)
            ob.keyframe_insert("hide_render", frame=frame - 1)
            ob.hide_viewport = ob.hide_render = False
            ob.keyframe_insert("hide_viewport", frame=frame)
            ob.keyframe_insert("hide_render", frame=frame)
        ob.color = C_INST
        ob.keyframe_insert("color", frame=frame)
    frame += 3 * HOLD + 75
    fin["end"] = frame
    chapters.append(fin)
    interp("BEZIER")

    last = frame
    sc.frame_start, sc.frame_end = 1, last
    log(f"时间线 1..{last} 帧 = {last/FPS:.1f}s（含终章 整机装配态）")

    # 颜色关键帧：未装(半透明) → 运动中(橙) → 就位(灰)。常量插值，避免中间过渡色。
    interp("CONSTANT")
    for c in chapters:
        for m in c["movers"]:
            f0, f1 = m["f0"], m["f1"]
            for ob in objs_of(m["occ"]):
                ob.color = C_PEND
                ob.keyframe_insert("color", frame=max(1, f0 - 1))
                ob.color = C_MOVE
                ob.keyframe_insert("color", frame=f0)
                ob.color = C_INST
                ob.keyframe_insert("color", frame=f1)
    interp("BEZIER")

    # ---- 相机：逐章构图（K-SCN-04）------------------------------------------
    # 整机一个机位试过：1.27m 的人形里一个臂部法兰组只占几个像素，看不见任何东西。
    # 改成每章按"本章动件的接近位与就位 + 本章之前已装件"反解一次，硬切不平移
    # （平移是表达层发明的运动，不做）。
    def frustum(pts, cd, up, view, q=1.0):
        """反解构图，取**最大值**覆盖全部点（K-SCN-04）。

        取 95 分位试过一版：确实压住了 OmniPicker 那种含游离面的离群件，
        但代价是把正常零件也裁掉——腿的下缘掉出画面，正是 Orbita 失效目录里
        『三条臂掉出画面下边缘』那一条。统计裁剪治不了个别件的几何缺陷。
        正确做法是把**已记录几何不可靠的件**（8 个 no_solid 采购模块）排除在构图之外，
        其余一律用最大值全覆盖。"""
        right = view.cross(up).normalized()
        up2 = right.cross(view).normalized()
        c = mathutils.Vector((sum(p[0] for p in pts) / len(pts),
                              sum(p[1] for p in pts) / len(pts),
                              sum(p[2] for p in pts) / len(pts)))

        def pct(vals):
            v = sorted(vals)
            return v[min(len(v) - 1, int(q * len(v)))]

        hw = pct([abs((p - c).dot(right)) for p in pts])
        hh = pct([abs((p - c).dot(up2)) for p in pts])
        hd = pct([abs((p - c).dot(view)) for p in pts])
        fx = 2 * math.atan(0.5 * cd.sensor_width / cd.lens)
        fy = 2 * math.atan(0.5 * cd.sensor_width * RES[1] / RES[0] / cd.lens)
        # 字幕带遮住的上下条不算可见区：竖向视场按可见带比例收窄（v005，--overlay-top/bottom）
        fy_eff = 2 * math.atan(math.tan(fy / 2) * max(0.2, (RES[1] - OVERLAY_TOP - OVERLAY_BOTTOM) / RES[1]))
        need = max(hw / math.tan(fx / 2), hh / math.tan(fy_eff / 2)) * FRAME_MARGIN + hd
        return c, need, right, up2

    allpts = []
    for oid, ob in objs.items():
        for cn in ob.bound_box:
            allpts.append(ob.matrix_world @ mathutils.Vector(cn))
    lo = [min(p[k] for p in allpts) for k in range(3)]
    hi = [max(p[k] for p in allpts) for k in range(3)]
    spans = [hi[k] - lo[k] for k in range(3)]
    # up 轴优先取配置（T 形展臂人形的"最大跨度轴"启发式会选到臂展轴——X1 未踩因其非 T-pose）
    _sem = _cfg.get("coordinate_semantics") or (
        json.loads((CASE / _P["s3_binding"]).read_text(encoding="utf-8"))
        .get("coordinate_semantics", {}))
    _up_cfg = next((k[1] for k, v in _sem.items() if isinstance(v, str) and v == "上"
                    and len(k) == 2 and k[0] in "+-" and k[1] in "XYZxyz"), None) \
        if isinstance(_sem, dict) else None
    if _up_cfg is not None:
        vax = "xyz".index(_up_cfg.lower())
    else:
        vax = spans.index(max(spans))
    # up 轴符号：由 S1 产品名关键词（foot/head）的世界位置中位数推断；
    # 配置的 coordinate_semantics 已知 +Y=上时结果应一致，两者不符即报错。
    def kw_median(kws, axis):
        v = sorted(o["world"][axis][3] for o in occ
                   if any(k in o["product"].lower() for k in kws))
        return None if not v else v[len(v) // 2]

    foot = kw_median(("foot", "脚掌"), vax)
    head = kw_median(("head", "头部"), vax)
    # OLSK 增量（2026-09-02）：非人形装配没有 foot/head 锚点，关键词法会默认 -1 把机器渲倒（F-SCN-08 同类）。
    # 配置 coordinates.up_axis_sign（由 S3 几何桥实测落盘）优先；两者都有且不符则报错，不静默。
    _sgn_cfg = (_cfg.get("coordinates") or {}).get("up_axis_sign")
    if _sgn_cfg is not None:
        sgn = float(_sgn_cfg)
        if head is not None and foot is not None and (1.0 if head > foot else -1.0) != sgn:
            raise SystemExit("up_axis_sign 配置与 foot/head 关键词推断不符，需人工裁决")
    else:
        sgn = 1.0 if (head is not None and foot is not None and head > foot) else -1.0
    UP = mathutils.Vector([1.0 if k == vax else 0.0 for k in range(3)]) * sgn
    lat = mathutils.Vector([1.0 if k == 0 else 0.0 for k in range(3)])
    fwd = mathutils.Vector([1.0 if (k != vax and k != 0) else 0.0 for k in range(3)])
    # OLSK 增量（2026-09-03）：机位 = 中心 − view·距离，原式 +UP 分量使相机落在中心之下（2 m 高的机床终章看到床身底面）；
    # 改为 −UP 分量让相机高于中心约 15° 俯视。仅表达层机位，不触及任何认证字段。
    view = (fwd * -0.80 + lat * 0.52 - UP * 0.26).normalized()

    cd = bpy.data.cameras.new("cam")
    cd.lens = 50
    cd.clip_start = 1.0
    cd.clip_end = max(spans) * 60
    cam = bpy.data.objects.new("cam", cd)
    sc.collection.objects.link(cam)
    sc.camera = cam
    installed = []
    installed_by_id = {}
    installed_ids, ctx_cum = set(), set()
    prev_view = None
    ghost_until = {}

    # ---- v005：实测可见性选机位（K-SCN-10）------------------------------------------
    # 包围盒中心判遮挡太粗：横梁端头/型材槽把动件整个挡住而中心测试看不出来（v005 首轮实测 2 个滑块、
    # 4 个丝杠座可见像素为 0）。做法：几何评分前 VIS_SELECT 个候选视向各渲一张低清 Cryptomatte matte，
    # 取动件可见像素/投影凸包面积作可见占比，最终分 = 几何分 × (0.1 + 可见占比)。渲染用临时合成器节点组，存盘前拆除。
    _vis_dir = OUTDIR / "vis_select_tmp"
    _vis_state = {}
    if VIS_SELECT > 0:
        import sys as _sys
        _sys.path.insert(0, str(Path(__file__).resolve().parent))
        from exr_reader_min import read_channels as _read_exr
        from bpy_extras.object_utils import world_to_camera_view as _w2cv
        _vis_dir.mkdir(exist_ok=True)
        _vl = sc.view_layers[0]
        _vis_state["pass"] = _vl.use_pass_cryptomatte_object
        _vl.use_pass_cryptomatte_object = True
        _vl.pass_cryptomatte_depth = 2
        _tree = bpy.data.node_groups.new("vis_select_comp", "CompositorNodeTree")
        sc.compositing_node_group = _tree
        sc.use_nodes = True
        _tree.interface.new_socket("Image", in_out="OUTPUT", socket_type="NodeSocketColor")
        _rl = _tree.nodes.new("CompositorNodeRLayers")
        _cm = _tree.nodes.new("CompositorNodeCryptomatteV2")
        _cm.source = "RENDER"
        _cm.layer_name = f"{_vl.name}.CryptoObject"
        _out = _tree.nodes.new("CompositorNodeOutputFile")
        _out.directory = str(_vis_dir)
        _out.file_output_items.new(socket_type="FLOAT", name="matte")
        try:
            _out.format.exr_codec = "NONE"
        except Exception:
            pass
        _sock = next(s_ for s_ in _out.inputs if s_.name == "matte")
        _tree.links.new(_rl.outputs["Image"], _cm.inputs["Image"])
        _tree.links.new(_cm.outputs["Matte"], _sock)
        _gout = _tree.nodes.new("NodeGroupOutput")
        _tree.links.new(_rl.outputs["Image"], _gout.inputs[0])
        # 测量用独立相机：主相机已有前几机位的常量关键帧，frame_set/渲染求值会把它拉回旧机位（K-SCN-06 同类陷阱）
        _vis_cam = bpy.data.objects.new("vis_cam", cd)
        sc.collection.objects.link(_vis_cam)
        _vis_state.update({"tree": _tree, "cm": _cm, "pct": sc.render.resolution_percentage,
                           "samples": sc.eevee.taa_render_samples, "filepath": sc.render.filepath, "vis_cam": _vis_cam})
        sc.render.filepath = str(_vis_dir / "dummy")
        _finale_frame = max(ch_["start"] for ch_ in chapters)

        def _hull_area(pts2):
            pts2 = sorted(set(pts2))
            if len(pts2) < 3:
                return 0.0
            def cr(o, a_, b_): return (a_[0] - o[0]) * (b_[1] - o[1]) - (a_[1] - o[1]) * (b_[0] - o[0])
            lo_, hi_ = [], []
            for q in pts2:
                while len(lo_) >= 2 and cr(lo_[-2], lo_[-1], q) <= 0: lo_.pop()
                lo_.append(q)
            for q in reversed(pts2):
                while len(hi_) >= 2 and cr(hi_[-2], hi_[-1], q) <= 0: hi_.pop()
                hi_.append(q)
            hull = lo_[:-1] + hi_[:-1]
            return abs(sum(hull[k][0] * hull[(k + 1) % len(hull)][1] - hull[(k + 1) % len(hull)][0] * hull[k][1]
                           for k in range(len(hull)))) / 2.0

        def measure_visible_frac(loc_c, R_c, names, frame):
            """把相机放到候选机位，在 frame 渲低清 Cryptomatte，返回 动件可见像素 / 投影凸包像素。"""
            vcam = _vis_state["vis_cam"]
            sc.camera = vcam
            sc.render.resolution_percentage = VIS_PCT
            sc.eevee.taa_render_samples = 1
            _vis_state["cm"].matte_id = ", ".join(names)
            sc.frame_set(frame)
            vcam.matrix_world = mathutils.Matrix.Translation(loc_c) @ R_c.to_4x4()
            for f_ in _vis_dir.glob("*.exr"):
                f_.unlink()
            bpy.ops.render.render(write_still=False)
            exrs = sorted(_vis_dir.glob("*.exr"))
            vis_px = 0
            if exrs:
                chans, _h = _read_exr(str(exrs[-1]))
                key = next((k for k in chans if "matte" in k.lower()), list(chans)[0])
                vis_px = int((chans[key] > 0.5).sum())
            Wm, Hm = int(RES[0] * VIS_PCT / 100), int(RES[1] * VIS_PCT / 100)
            pts2 = []
            for nm in names:
                ob_ = objs.get(nm)
                if ob_ is None:
                    continue
                for cn in ob_.bound_box:
                    cv = _w2cv(sc, vcam, ob_.matrix_world @ mathutils.Vector(cn))
                    if cv.z > 0:
                        pts2.append((min(Wm, max(0.0, cv.x * Wm)), min(Hm, max(0.0, cv.y * Hm))))
            hull = _hull_area(pts2)
            sc.camera = cam
            sc.frame_set(_finale_frame)      # 复位到终章（全员就位、可见），后续包围盒读数才是就位态
            sc.render.resolution_percentage = _vis_state["pct"]
            sc.eevee.taa_render_samples = _vis_state["samples"]
            return (vis_px / hull) if hull > 0 else 0.0, vis_px

    for ci, c in enumerate(chapters):
        ctx_cum |= set(c.get("context", []))
        if c.get("finale"):
            # 终章：整机构图（几何不可靠件不参与反解，防镜头拉飞）
            pts_f = []
            for oid, ob in objs.items():
                st_f = stem_for(occ_prod(id2occ, oid))
                if st_f and not man.get(st_f, {}).get("has_solid", True):
                    continue
                for cn in ob.bound_box:
                    pts_f.append(ob.matrix_world @ mathutils.Vector(cn))
            ctr, need, right, up2 = frustum(pts_f, cd, UP, view)
            loc = ctr - view * need
            R = mathutils.Matrix(((right.x, up2.x, (-view).x),
                                  (right.y, up2.y, (-view).y),
                                  (right.z, up2.z, (-view).z)))
            cam.matrix_world = mathutils.Matrix.Translation(loc) @ R.to_4x4()
            if FINALE_ORBIT > 0:
                # v010：终章缓慢环绕（审片：3 s 定帧像一张静态图）；相邻键角差小，Euler 用上一键做连续化
                _K = 8; _prev_e = None
                for _i in range(_K + 1):
                    _t = _i / _K
                    _rot = mathutils.Matrix.Rotation(math.radians(FINALE_ORBIT) * (_t - 0.5), 4, UP)
                    _v = (_rot @ view.to_4d()).to_3d().normalized()
                    _ctr, _need, _right, _up2 = frustum(pts_f, cd, UP, _v)
                    _R = mathutils.Matrix(((_right.x, _up2.x, (-_v).x), (_right.y, _up2.y, (-_v).y), (_right.z, _up2.z, (-_v).z)))
                    cam.matrix_world = mathutils.Matrix.Translation(_ctr - _v * _need) @ _R.to_4x4()
                    _e = cam.matrix_world.to_euler("XYZ", _prev_e) if _prev_e is not None else cam.matrix_world.to_euler("XYZ")
                    cam.rotation_euler = _e; _prev_e = _e
                    _fr = int(round(c["start"] + _t * max(1, c["end"] - 1 - c["start"])))
                    cam.keyframe_insert("location", frame=_fr)
                    cam.keyframe_insert("rotation_euler", frame=_fr)
            else:
                for fr in (c["start"], max(c["start"], c["end"] - 1)):
                    cam.keyframe_insert("location", frame=fr)
                    cam.keyframe_insert("rotation_euler", frame=fr)
            c["camera"] = {"distance_mm": round(need, 1), "finale_full_machine": True, "orbit_deg": FINALE_ORBIT}
            continue
        if c.get("presence_only"):
            # 结合章：构图 = 闭包成员全体（单元汇合呈现）
            pts_p = []
            for oid in c.get("closure_members", []):
                for mid in members_of(oid):
                    pob = objs.get(mid)
                    if pob is None:
                        continue
                    st_p = stem_for(occ_prod(id2occ, mid))
                    if st_p and not man.get(st_p, {}).get("has_solid", True):
                        continue
                    for cn in pob.bound_box:
                        pts_p.append(pob.matrix_world @ mathutils.Vector(cn))
            if pts_p:
                ctr, need, right, up2 = frustum(pts_p, cd, UP, view)
                loc = ctr - view * need
                R = mathutils.Matrix(((right.x, up2.x, (-view).x),
                                      (right.y, up2.y, (-view).y),
                                      (right.z, up2.z, (-view).z)))
                cam.matrix_world = mathutils.Matrix.Translation(loc) @ R.to_4x4()
                for fr in (c["start"], max(c["start"], c["end"] - 1)):
                    cam.keyframe_insert("location", frame=fr)
                    cam.keyframe_insert("rotation_euler", frame=fr)
                c["camera"] = {"distance_mm": round(need, 1), "presence_union": True}
            continue
        # ---- v005：按动件空间分组逐组构图（硬切）----------------------------------------
        # v004 一章一机位：章内 1.9 m 横梁与 30 mm 衬板同框，衬板成一个像素——十段审片一致报"动件不可见/弹入"。
        # 分组 = 播放序上相邻、且合并足迹半径 ≤ max(1.8×成员最大半径, 400 mm) 的动件共一机位；每组一次硬切，
        # 每章最多 SHOTS_MAX 组（超出则合并相邻最近组）。SHOTS_MAX=1 即 v004 行为（整章一机位）。
        def mover_pts(m):
            ax = mathutils.Vector(m["insertion_axis_world"]).normalized()
            off = ax * (m["withdraw_sense"] * m["approach_mm"])
            pts_, excl = [], 0
            for mid in members_of(m["occ"]):
                ob = objs.get(mid)
                if ob is None:
                    continue
                # 几何不可靠的件不参与构图：它们的包围盒会把镜头拉飞
                st_m = stem_for(occ_prod(id2occ, mid))
                if st_m and not man.get(st_m, {}).get("has_solid", True):
                    excl += 1
                    continue
                for cn in ob.bound_box:
                    w = ob.matrix_world @ mathutils.Vector(cn)
                    pts_.append(w)
                    pts_.append(w + off)
            if not pts_:      # 整件不可靠，只好退回用它构图
                for ob in objs_of(m["occ"]):
                    for cn in ob.bound_box:
                        pts_.append(ob.matrix_world @ mathutils.Vector(cn))
            return pts_, excl

        def radius_of(pts_):
            cc = sum(pts_, mathutils.Vector()) / len(pts_)
            return cc, max((p - cc).length for p in pts_)

        groups = []
        for m in c["movers"]:
            pm, ex = mover_pts(m)
            if not pm:
                continue
            r_m = radius_of(pm)[1]
            if groups:
                g = groups[-1]
                same_pair = bool(m.get("pair_id")) and m["pair_id"] == g["movers"][-1].get("pair_id")
                r_union = radius_of(g["pts"] + pm)[1]
                r_max = max(g["r_max"], r_m)
                # 尺度相容：小件不并入大件的机位（3 m 丝杠旁的丝杠座在 7 m 机位里只有 11 px，K-SCN-10 实测）
                compatible = min(r_m, g["r_max"]) >= 0.15 * max(r_m, g["r_max"]) or r_union <= 400.0
                if same_pair or (compatible and r_union <= max(1.8 * r_max, 400.0)):
                    g["movers"].append(m)
                    g["pts"] = g["pts"] + pm
                    g["excl"] += ex
                    g["r_max"] = r_max
                    continue
            groups.append({"movers": [m], "pts": pm, "excl": ex, "r_max": r_m})
        if not groups:
            continue
        while len(groups) > SHOTS_MAX:
            i_min = min(range(len(groups) - 1),
                        key=lambda i: radius_of(groups[i]["pts"] + groups[i + 1]["pts"])[1])
            a_, b_ = groups[i_min], groups[i_min + 1]
            groups[i_min:i_min + 2] = [{"movers": a_["movers"] + b_["movers"], "pts": a_["pts"] + b_["pts"],
                                        "excl": a_["excl"] + b_["excl"], "r_max": max(a_["r_max"], b_["r_max"])}]

        shots, ghosts, mate_ghosts = [], [], []
        for gi, g in enumerate(groups):
            own = list(g["pts"])
            own_excluded = g["excl"]
            g_start = c["start"] if gi == 0 else g["movers"][0]["f0"]
            g_end = c["end"] if gi == len(groups) - 1 else groups[gi + 1]["movers"][0]["f0"]
            # 构图只框**本组动件**加它周围的局部上下文。
            # 把所有已装件都框进去试过：越到后面镜头越远，最后退成整机全景，
            # 什么细节都看不见（这正是 K-SCN-04 说的"别用经验系数、要按内容反解"的反面）。
            oc, rad = radius_of(own)
            own_r = rad
            # 上下文半径 1.3×rad（下限 200 mm）：v004 的 2.2×rad 把 3 m 丝杠单元的整机框进来，镜头退到 7.5 m
            ctx_r = max(rad * 1.3, 200.0)
            for oid in c.get("context", []):
                cob = objs.get(oid)
                if cob is None:
                    continue
                st_c = stem_for(occ_prod(id2occ, oid))
                if st_c and not man.get(st_c, {}).get("has_solid", True):
                    continue
                for cn in cob.bound_box:
                    w = cob.matrix_world @ mathutils.Vector(cn)
                    if (w - oc).length <= ctx_r:
                        own.append(w)
            if c.get("visible_ids") is not None:
                # v010：台架章只框可见件（其余机器件已隐藏，不能让镜头去框一片空）
                _vis_mids = {mid for oid in c["visible_ids"] for mid in members_of(oid)}
                pts = list(own) + [p for mid, pl in installed_by_id.items() if mid in _vis_mids for p in pl if (p - oc).length <= ctx_r]
            else:
                pts = list(own) + [p for p in installed if (p - oc).length <= ctx_r]
            # 逐组选机位。候选 = 8 个方位角 × 2 个俯角；评分 = 画面占比(1/need) × 运动可辨度(插入轴垂直于视线)
            # / (1+0.08×遮挡者数)，与上一机位相同视向加 8% 保持连贯。只改表达层机位，不触及任何认证字段。
            g_members = {mid for m in g["movers"] for mid in members_of(m["occ"])}
            axes_ch = [mathutils.Vector(m["insertion_axis_world"]).normalized() for m in g["movers"]]
            cands = []
            for az_i in range(8):
                az = math.radians(45.0 * az_i + 22.5)
                for el in (0.08, 0.26, 0.50):   # v009：加近水平俯角，床下/腔内件才有可见候选
                    vc = ((fwd * math.cos(az) + lat * math.sin(az)) * (1.0 - el * el) ** 0.5 - UP * el).normalized()
                    cands.append(vc)
            # 配合件（solid）默认不算遮挡者；但比动件大 5 倍以上的配合件能把动件整个挡住
            # （滑块在横梁下方、导轨在型材槽内：v005 可读性实测 2 个滑块可见像素 0），这类大配合件按遮挡者计并以 MATE_GHOST 虚化。
            mates_g = set()
            for mid in g_members:
                i2 = occ_index.get(mid)
                for j in mates.get(i2, ()):
                    mates_g.add(idx2id[j])
            occ_bbs = []
            for oid in (installed_ids | ctx_cum) - bench_ghosted - g_members:
                gob = objs.get(oid)
                if gob is None:
                    continue
                bb = [gob.matrix_world @ mathutils.Vector(cn) for cn in gob.bound_box]
                pc = sum(bb, mathutils.Vector()) / 8.0
                pr = max((p - pc).length for p in bb)
                if pr < 0.08 * own_r:
                    continue
                if oid in mates_g and pr < 2.0 * own_r:
                    continue
                occ_bbs.append((oid, pc, pr))
            # 本组先装的兄弟件对后装的兄弟件同样是遮挡者（Z 中板先装、滑块在板后装：v005 实测机位对着板正面）
            # ——只参与视向评分，不虚化（刚装上的件不该被虚掉）
            sib_bbs = []
            first_f0 = min(m_["f0"] for m_ in g["movers"])
            for m_ in g["movers"]:
                if m_["f0"] == first_f0:
                    continue
                for mid in members_of(m_["occ"]):
                    pass
            for m_ in g["movers"][:-1]:
                for mid in members_of(m_["occ"]):
                    gob = objs.get(mid)
                    if gob is None:
                        continue
                    bb = [gob.matrix_world @ mathutils.Vector(cn) for cn in gob.bound_box]
                    pc = sum(bb, mathutils.Vector()) / 8.0
                    pr = max((p - pc).length for p in bb)
                    if pr >= 0.5 * own_r:
                        sib_bbs.append((mid, pc, pr))
            if AXIS_VIEW_MAX < 1.0 and axes_ch:
                _ok = [vc for vc in cands if max(abs(ax_.dot(vc)) for ax_ in axes_ch) <= AXIS_VIEW_MAX]
                if _ok:
                    cands = _ok
            _fy_eff = 2 * math.atan(math.tan(2 * math.atan(0.5 * cd.sensor_width * RES[1] / RES[0] / cd.lens) / 2)
                                    * max(0.2, (RES[1] - OVERLAY_TOP - OVERLAY_BOTTOM) / RES[1]))
            _need_cap = (own_r / (math.tan(_fy_eff / 2) * MIN_MOVER_FRAC)) if MIN_MOVER_FRAC > 0 else None
            scored = []
            for vc in cands:
                ctr_c, need_c, right_c, up_c = frustum(pts, cd, UP, vc)
                if _need_cap is not None and need_c > _need_cap:
                    # 推近到动件至少占画面半高的 MIN_MOVER_FRAC，构图中心改到动件（上下文被裁掉也接受）
                    need_c = max(_need_cap, 1.6 * own_r)
                    ctr_c = oc
                loc_c = ctr_c - vc * need_c
                perp = sum(1.0 - abs(ax_.dot(vc)) for ax_ in axes_ch) / max(1, len(axes_ch))
                n_occ = 0
                for oid, pc, pr in occ_bbs + sib_bbs:
                    rel = pc - loc_c
                    d_along = rel.dot(vc)
                    if 1.0 < d_along < need_c - 0.3 * rad and (rel - vc * d_along).length < own_r + pr:
                        n_occ += 1
                sc_ = (1.0 / max(need_c, 1.0)) * (0.35 + 0.65 * perp) / (1.0 + 0.08 * n_occ)
                if prev_view is not None and (vc - prev_view).length < 1e-6:
                    sc_ *= 1.08
                scored.append((sc_, vc, (ctr_c, need_c, right_c, up_c, n_occ, perp)))
            scored.sort(key=lambda t: -t[0])
            vis_note = None
            if VIS_SELECT > 0 and scored:
                # 最小的动件最容易被挡：用它的中程帧实测
                m_small = min(g["movers"], key=lambda m_: radius_of(mover_pts(m_)[0])[1] if mover_pts(m_)[0] else 1e9)
                probes = [m_small]
                if VIS_PROBES >= 2 and g["movers"][0] is not m_small:
                    probes.append(g["movers"][0])
                if VIS_PROBES >= 3 and g["movers"][-1] is not m_small and g["movers"][-1] is not g["movers"][0]:
                    probes.append(g["movers"][-1])
                rescored = []
                for sc_, vc, fr_ in scored[:VIS_SELECT]:
                    ctr_c, need_c, right_c, up_c, n_occ, perp = fr_
                    need_v = max(need_c, 200.0 / math.tan(0.5 * 2 * math.atan(0.5 * cd.sensor_width / cd.lens)))
                    loc_c = ctr_c - vc * need_v
                    R_c = mathutils.Matrix(((right_c.x, up_c.x, (-vc).x), (right_c.y, up_c.y, (-vc).y), (right_c.z, up_c.z, (-vc).z)))
                    vfs, vpx = [], 0
                    for m_p in probes:
                        names_v = [x for x in members_of(m_p["occ"]) if x in objs]
                        if not names_v:
                            continue
                        vf_, vp_ = measure_visible_frac(loc_c, R_c, names_v, (m_p["f0"] + m_p["f1"]) // 2)
                        vfs.append(vf_); vpx += vp_
                    vfrac = (sum(vfs) / len(vfs)) if vfs else 0.0   # 多探针取平均：一侧全挡（Z 中板背面 6 个滑块）会被压分
                    rescored.append((sc_ * (0.1 + vfrac), vc, fr_, vfrac, vpx))
                rescored.sort(key=lambda t: -t[0])
                best_s, best_v, best_fr, vis_note = rescored[0][0], rescored[0][1], rescored[0][2], {"visible_frac": round(rescored[0][3], 3), "visible_px_measure_res": rescored[0][4], "candidates_measured": len(rescored), "probe_mover": m_small["occ"]}
                # v009 剖切回退：最佳视向下探针仍大半被挡（<0.35）→ 本组先装的大兄弟件按遮挡者虚化（v008 审片：Z 中板先装，
                # 6 个滑块在板背面全程不可见；WB-77 远侧盖板）。刚装上的件被虚掉是次优，但比整章看不见动件好。
                cutaway_siblings = rescored[0][3] < 0.35
            else:
                best_s, best_v, best_fr = scored[0]
            view_ch = best_v
            prev_view = view_ch
            ctr, need, right, up2, n_occ_best, perp_best = best_fr
            # 最小取景：半宽不小于 200 mm（小螺钉组也要有可辨的局部上下文）
            need = max(need, 200.0 / math.tan(0.5 * 2 * math.atan(0.5 * cd.sensor_width / cd.lens)))
            loc = ctr - view_ch * need
            # v009：机位不得落在任何已在场件的包围盒内（v008 审片：相机钻进横梁里，画面被近裁面切掉）——
            # 落入即沿视线后退 20%，最多 4 次
            for _try in range(4):
                inside_any = False
                for oid_c in (installed_ids | ctx_cum | g_members):
                    gob_c = objs.get(oid_c)
                    if gob_c is None:
                        continue
                    bb_c = [gob_c.matrix_world @ mathutils.Vector(cn) for cn in gob_c.bound_box]
                    lo_c = [min(p[k] for p in bb_c) - 60.0 for k in range(3)]
                    hi_c = [max(p[k] for p in bb_c) + 60.0 for k in range(3)]
                    if all(lo_c[k] <= loc[k] <= hi_c[k] for k in range(3)):
                        inside_any = True
                        break
                if not inside_any:
                    break
                need *= 1.2
                loc = ctr - view_ch * need
            R = mathutils.Matrix(((right.x, up2.x, (-view_ch).x),
                                  (right.y, up2.y, (-view_ch).y),
                                  (right.z, up2.z, (-view_ch).z)))
            cam.matrix_world = mathutils.Matrix.Translation(loc) @ R.to_4x4()
            # 每组两个同姿态关键帧（组首 + 组末），相机 F 曲线在存盘前统一设为常量插值（硬切）
            for fr in (g_start, max(g_start, g_end - 1)):
                cam.keyframe_insert("location", frame=fr)
                cam.keyframe_insert("rotation_euler", frame=fr)
            shots.append({"start": g_start, "end": g_end, "movers": [m["occ"] for m in g["movers"]],
                          "distance_mm": round(need, 1), "own_pts": len(own),
                          "excluded_unreliable_movers": own_excluded,
                          "context_pts": len(pts) - len(own), "context_radius_mm": round(ctx_r, 1),
                          "view": [round(float(x), 4) for x in view_ch], "view_score": round(best_s, 6),
                          "occluders_in_view": n_occ_best, "axis_perp": round(perp_best, 3),
                          "rad_mm": round(rad, 1), "vis_select": vis_note})

            # ---- 遮挡者虚化（用户：有些区域被挡住看不到）------------------------
            # 该透明化谁由**连接图邻域**定义：本组动件与其 S2 配合对象保持实体，
            # 其余已在场件若挡在相机与动件足迹之间，本组内 α=GHOST_ALPHA（DITHERED 写深度不成雾）。
            # v005：只虚化遮挡最重（视角占比最大）的 GHOST_MAX 件；判据用动件足迹 own_r 而非整章半径。
            solid = set(g_members)
            cand_g = []
            cutaway_ids = set()
            if VIS_SELECT > 0 and vis_note is not None and cutaway_siblings:
                # 剖切回退：把压在动件前方的一切在场件（兄弟、配合件、上下文）都列为候选，优先级最高
                extra = list(sib_bbs)
                for oid_m in sorted(mates_g):
                    gob_m = objs.get(oid_m)
                    if gob_m is None or oid_m in g_members:
                        continue
                    bb_m = [gob_m.matrix_world @ mathutils.Vector(cn) for cn in gob_m.bound_box]
                    pc_m = sum(bb_m, mathutils.Vector()) / 8.0
                    extra.append((oid_m, pc_m, max((p - pc_m).length for p in bb_m)))
                for oid_e, pc, pr in extra:
                    rel = pc - loc
                    d_along = rel.dot(view_ch)
                    if 1.0 < d_along < need - 0.3 * rad and (rel - view_ch * d_along).length < own_r + pr:
                        cand_g.append((pr / max(d_along, 1.0) + 10.0, oid_e))
                        cutaway_ids.add(oid_e)
                vis_note["cutaway_siblings"] = len(cand_g)
            for oid, pc, pr in occ_bbs:
                if oid in solid:
                    continue
                rel = pc - loc
                d_along = rel.dot(view_ch)
                if not (1.0 < d_along < need - 0.3 * rad):
                    continue                      # 不在相机与装配区之间
                if (rel - view_ch * d_along).length < own_r + pr:
                    cand_g.append((pr / max(d_along, 1.0), oid))
            cand_g.sort(reverse=True)
            interp("CONSTANT")
            # v005：极淡在场的非本台架件若压在动件足迹前方，本组内干脆不显示（它们本就"不在场"），
            # 否则近景里一根 1.9 m 横梁的极淡影子糊满整幅画面（实测 WB-07 带轮近景）。
            for oid in sorted(bench_ghosted):
                gob = objs.get(oid)
                if gob is None:
                    continue
                bb = [gob.matrix_world @ mathutils.Vector(cn) for cn in gob.bound_box]
                pc = sum(bb, mathutils.Vector()) / 8.0
                pr = max((p - pc).length for p in bb)
                rel = pc - loc
                d_along = rel.dot(view_ch)
                far_away = BENCH_GHOST_R > 0 and (pc - oc).length - pr > BENCH_GHOST_R * max(own_r, 100.0)
                if far_away or (1.0 < d_along < need - 0.3 * rad and (rel - view_ch * d_along).length < own_r + pr):
                    gob.color = (1.0, 1.0, 1.0, BENCH_GHOST)
                    gob.keyframe_insert("color", frame=max(1, g_start - 1))
                    gob.color = (1.0, 1.0, 1.0, 0.0)
                    gob.keyframe_insert("color", frame=g_start)
                    gob.color = (1.0, 1.0, 1.0, BENCH_GHOST)
                    gob.keyframe_insert("color", frame=max(g_start, g_end - 1))
            for _score_g, oid in cand_g[:GHOST_MAX]:
                gob = objs.get(oid)
                rgb = C_INST if oid in installed_ids else C_PEND
                alpha_g = max(GHOST_ALPHA, MATE_GHOST) if oid in mates_g else GHOST_ALPHA   # 大配合件留 0.3，仍能看出动件装在它上面
                if CUTAWAY_HIDE and oid in cutaway_ids:
                    alpha_g = 0.0                      # v010：剖切=隐藏（审片：0.25 的“玻璃”把动件冲成半透明）
                # v008：虚化贯穿到组末（含就位帧），恢复键放在组末帧；若上一组已虚化到本组首帧则不再插"前一帧=1.0"的预键
                # （v007 审片：遮挡者恰在就位帧恢复不透明，把落点挡住）。终章首帧另有统一就位色键在后写入。
                if ghost_until.get(oid, -1) < g_start:
                    gob.color = (rgb[0], rgb[1], rgb[2], 1.0)
                    gob.keyframe_insert("color", frame=max(1, g_start - 1))
                gob.color = (rgb[0], rgb[1], rgb[2], alpha_g)
                gob.keyframe_insert("color", frame=g_start)
                gob.color = (rgb[0], rgb[1], rgb[2], 1.0)
                # v010：恢复键放到章末而非组末——组末=末件就位帧，遮挡者恰在就位帧变回不透明把落点挡住（十段审片“动件只在高亮时可见”）
                gob.keyframe_insert("color", frame=c["end"])
                ghost_until[oid] = c["end"]
                ghosts.append(oid)
            interp("BEZIER")

            # v005：动件整体在某配合件的包围盒内部（管内衬板、盒内单元）→ 该配合件在动件运动窗内虚化，
            # 否则动件全程不可见（审片：Frame 衬板"面板高亮但画面无任何橙色"）。
            if MATE_GHOST > 0:
                interp("CONSTANT")
                for m in g["movers"]:
                    mobs = [objs[mid] for mid in members_of(m["occ"]) if mid in objs]
                    if not mobs:
                        continue
                    mpts = [ob_.matrix_world @ mathutils.Vector(cn) for ob_ in mobs for cn in ob_.bound_box]
                    mlo = [min(p[k] for p in mpts) for k in range(3)]
                    mhi = [max(p[k] for p in mpts) for k in range(3)]
                    mvol = max(1.0, (mhi[0] - mlo[0]) * (mhi[1] - mlo[1]) * (mhi[2] - mlo[2]))
                    mc = [(mlo[k] + mhi[k]) / 2 for k in range(3)]
                    # 包壳判定不限于配合件：任何已在场件的包围盒整体包住动件包围盒（Z 轴滑块在导轨壳内、门锁在门板内）都算包壳
                    # （v005 可读性实测：12 个动件在 4 个候选视向下可见像素皆为 0，全是被非配合件包住）
                    enclosers = set()
                    for mid in members_of(m["occ"]):
                        i2 = occ_index.get(mid)
                        for j in mates.get(i2, ()):
                            enclosers.add(idx2id[j])
                    enclosers |= (installed_ids | ctx_cum) - bench_ghosted
                    for oid in sorted(enclosers):
                        if True:
                            gob = objs.get(oid)
                            if gob is None or oid in mate_ghosts or oid in g_members:
                                continue
                            bb = [gob.matrix_world @ mathutils.Vector(cn) for cn in gob.bound_box]
                            lo_ = [min(p[k] for p in bb) for k in range(3)]
                            hi_ = [max(p[k] for p in bb) for k in range(3)]
                            vol = (hi_[0] - lo_[0]) * (hi_[1] - lo_[1]) * (hi_[2] - lo_[2])
                            # 整件包围盒落在配合件包围盒内才算"内部"（只查中心会把 3 m 丝杠当成带轮的外壳而虚化）
                            ov = 1.0
                            for k in range(3):
                                ov *= max(0.0, min(mhi[k], hi_[k] + 2.0) - max(mlo[k], lo_[k] - 2.0)) / max(1e-6, mhi[k] - mlo[k])
                            inside = ov >= 0.6      # 动件包围盒体积 ≥60% 落在对方包围盒内即算被包（导轨大半在型材槽内）
                            if inside and vol > 3.0 * mvol:
                                rgb = C_INST if oid in installed_ids else C_PEND
                                if ghost_until.get(oid, -1) < m["f0"]:
                                    gob.color = (rgb[0], rgb[1], rgb[2], 1.0)
                                    gob.keyframe_insert("color", frame=max(1, m["f0"] - 1))
                                gob.color = (rgb[0], rgb[1], rgb[2], MATE_GHOST)
                                gob.keyframe_insert("color", frame=m["f0"])
                                gob.color = (rgb[0], rgb[1], rgb[2], 1.0)
                                gob.keyframe_insert("color", frame=c["end"])   # v010：包壳虚化持续到章末（就位帧不再被壳体盖住）
                                ghost_until[oid] = c["end"]
                                mate_ghosts.append(oid)
                interp("BEZIER")
        c["camera"] = dict(shots[0], shots=shots, n_shots=len(shots))
        c["ghosted_occluders"] = ghosts
        c["mate_ghosts"] = mate_ghosts

        for m in c["movers"]:
            for mid in members_of(m["occ"]):
                ob = objs.get(mid)
                if ob is None:
                    continue
                installed_ids.add(mid)
                for cn in ob.bound_box:
                    installed.append(ob.matrix_world @ mathutils.Vector(cn))
                    installed_by_id.setdefault(mid, []).append(ob.matrix_world @ mathutils.Vector(cn))
    log(f"竖直轴 {'xyz'[vax]}{'+' if sgn>0 else '-'}  逐章机位 {len(chapters)} 个")

    for (a, b, c_), e in (((0.45, 0.55, -1.0), 4.2), ((-0.7, 0.25, -0.7), 2.1), ((0.1, -0.45, 1.0), 1.3)):
        ld = bpy.data.lights.new(f"L{e}", "SUN")
        ld.energy = e
        lo_ = bpy.data.objects.new(f"L{e}", ld)
        sc.collection.objects.link(lo_)
        rr = view.cross(UP).normalized()
        uu = rr.cross(view).normalized()
        dv = (rr * a + uu * b + view * c_).normalized()
        Rl = mathutils.Matrix(((rr.x, uu.x, (-dv).x), (rr.y, uu.y, (-dv).y), (rr.z, uu.z, (-dv).z)))
        lo_.matrix_world = Rl.to_4x4()

    # OLSK 增量（2026-09-03，v005）：显式设置 F 曲线插值。实测 Blender 5.2 的
    # preferences.edit.keyframe_new_interpolation_type 对 Python keyframe_insert **不生效**：
    # v004 的对象色/α 关键帧全是贝塞尔——就位色从橙渐变到灰、遮挡者 α 从 0.12 渐变回 1.0 贯穿整章
    # （10 段审片报的"X 光在章末才收敛""就位后颜色慢慢变"皆此因）。状态量必须阶跃，位移匀速。
    n_fc = 0
    for act in bpy.data.actions:
        fcs = []
        if hasattr(act, "fcurves"):
            fcs = list(act.fcurves)
        else:
            for layer in getattr(act, "layers", []):
                for strip in layer.strips:
                    for cb in strip.channelbags:
                        fcs.extend(cb.fcurves)
        is_cam = "cam" in act.name.lower() and not act.name.startswith("occ")
        _fin_start = chapters[-1]["start"] if (FINALE_ORBIT > 0 and chapters and chapters[-1].get("finale")) else None
        for fc in fcs:
            kind = "CONSTANT" if (is_cam or fc.data_path in ("color", "hide_render", "hide_viewport")) else \
                   "LINEAR" if fc.data_path == "location" else None
            if kind is None:
                continue
            for kp in fc.keyframe_points:
                kp.interpolation = "LINEAR" if (is_cam and _fin_start is not None and kp.co.x >= _fin_start) else kind
            n_fc += 1
    log(f"F 曲线插值显式设置 {n_fc} 条（色/可见=常量，位移=线性）")

    if VIS_SELECT > 0 and _vis_state.get("tree") is not None:
        sc.compositing_node_group = None
        bpy.data.node_groups.remove(_vis_state["tree"])
        sc.view_layers[0].use_pass_cryptomatte_object = _vis_state["pass"]
        sc.render.resolution_percentage = _vis_state["pct"]
        sc.eevee.taa_render_samples = _vis_state["samples"]
        sc.render.filepath = _vis_state["filepath"]
        sc.camera = cam
        bpy.data.objects.remove(_vis_state["vis_cam"])
        for f_ in _vis_dir.glob("*"):
            f_.unlink()
        _vis_dir.rmdir()
        sc.frame_set(1)
        log("可见性选机位：临时合成器已拆除")

    blend = OUTDIR / "assembly_source.blend"
    bpy.ops.wm.save_as_mainfile(filepath=str(blend))
    log(f"存盘 {blend.name}")

    static_context_ids = {oid for chapter in chapters for oid in chapter.get("context", [])}
    static_withdrawn_ids = {
        oid for chapter in chapters for oid in chapter.get("static_withdrawn", [])
    }
    pair_ids = {
        mover["pair_id"] for chapter in chapters for mover in chapter.get("movers", [])
        if mover.get("pair_id")
    }
    not_animated_count = sum(item["n"] for item in skipped)
    if STATE_PATH:
        try:
            state_ref = str(STATE_PATH.relative_to(ROOT))
        except ValueError:
            state_ref = str(STATE_PATH)
        authority_is_plan = frozen["$schema"] == "x1.assembly-film-motion-plan/v1"
        purpose = "冻结装配运动 authority 编译成的时间线；动画层不回读上游表格发明运动。"
        motion_authority = {
            "mode": "frozen_plan" if authority_is_plan else "frozen_state",
            "state_file": state_ref,
            "state_sha256": authority_sha256,
            "state_schema": frozen["$schema"],
            "state_version": frozen.get("plan_version") if authority_is_plan
            else frozen.get("state_version"),
            "authority_id": frozen.get("plan_id") if authority_is_plan
            else frozen.get("run_id"),
            "assembly_schema": assembly["$schema"],
            "assembly_summary": copy.deepcopy(assembly["summary"]),
            "consumed_fields": [
                "operation/mover order", "insertion_axis_world", "withdraw_sense",
                "approach_mm", "approach_source", "pair_id", "not_animated",
            ],
        }
        first_claim = (
            f"本时间线精确消费冻结状态中的 {n_mov} 个动件"
            f"（{len(timeline)} 道运动工序）；顺序、轴、方向和行程未从"
            "PLAY/ORDER/S7 重算或拼接。"
        )
        approach_claim = (
            "行程精确消费冻结 approach_mm；构建器不再应用 --cap 或其他收短。"
        )
    else:
        purpose = "兼容模式：直接把 S7/PLAY/ORDER 编译成时间线。"
        motion_authority = {
            "mode": "legacy_s7_play_order",
            "sources": [str(S7.relative_to(ROOT)), str(PLAY.relative_to(ROOT)),
                        "本体/S7-工序内播放序.v1.json"],
            "probe": PROBE,
        }
        first_claim = (
            f"本兼容时间线表达 {n_mov} 个动件（{len(timeline)} 道运动工序）；"
            "新运行应先冻结版本化状态再渲染。"
        )
        approach_claim = (
            "兼容模式的起始偏移受 PLAY/ORDER 与全局接近上限约束；"
            "具体记录见各 mover 行。"
        )

    (OUTDIR / "state-chain.json").write_text(json.dumps({
        "$schema": "x1.assembly-state-chain/v1",
        "purpose": purpose,
        "motion_authority": motion_authority,
        "params": {
            "frames_per_mover": FPM,
            "hold_frames": HOLD,
            "chapter_limit": CHLIMIT or None,
            "partial_build": bool(CHLIMIT),
            "approach_cap_mm": None if STATE_PATH else CAP,
            "frozen_approach_consumed_verbatim": bool(STATE_PATH),
            "fps": FPS,
        },
        "frames": [1, last], "seconds": round(last / FPS, 2),
        "compiled_summary": {
            "motion_operations": len(timeline),
            "movers": n_mov,
            "not_animated": not_animated_count,
            "static_context": len(static_context_ids),
            "static_withdrawn": len(static_withdrawn_ids),
            "loaded_cad_instances": len(objs),
            "empty_geometry_members": empty_geometry_members if STATE_PATH else [],
            "explicit_pair_ids": len(pair_ids),
        },
        "chapters": chapters,
        "not_animated": skipped,
        "claim_boundary": [
            first_claim,
            f"普通时间窗只动一个冻结动件；仅 {len(pair_ids)} 个显式 pair_id "
            "的相邻两件共用时间窗同步合拢，不推断其他并发运动。",
            approach_claim,
            "运动只有沿轴平移。拧入、边转边落、相位对准都不在覆盖内——螺钉只是沿轴推到位，不表示拧紧动作。",
            f"not_animated 列出 {not_animated_count} 个未表达运动的件；"
            "即使它们以静态退场件或终章实例出现，也不表示运动已被补齐。",
            f"工序段中 {len(static_context_ids)} 个共轴配合对手和 "
            f"{len(static_withdrawn_ids)} 个退场件只作静态上下文；"
            "静态出现不主张它们在该时刻被安装、移动或认证。",
            f"终章只把当前成功载入的 {len(objs)} 个 CAD 实例静态显示在 S1 就位位姿；"
            "它是整机外观参考，不主张未表达工序已完成。",
            "相机逐章反解取最大值覆盖；标为 no_solid 的几何不可靠采购模块"
            "不参与构图，因此这些件可能出框。",
            "只框本章动件加半径 2.2 倍范围内的局部上下文；"
            "每章两个同姿态关键帧使该段镜头静止，章与章之间会有一次插值过渡。",
        ],
    }, ensure_ascii=False, indent=1), encoding="utf-8")
    log("完成")
    return 0


def occ_prod(id2occ, oid):
    o = id2occ.get(oid)
    return o["product"] if o else "?"


def s7_mover(s7, opname, oid):
    for op in s7["operations"]:
        if op["op"] == opname:
            return [m for m in op["per_mover"] if m["occ"] == oid]
    return []


def median_centroid(sd, kw, axis):
    v = sorted(r["world_centroid"][axis] for r in sd if kw in r["product"])
    return None if not v else v[len(v) // 2]


main()
