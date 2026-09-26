#!/usr/bin/env python3
"""A9 保存/重开全量回读：独立新进程打开已存 .blend，对账 state-chain。

检查（全量，不抽样——X1 审计指出三样本回读不等于全量回读）：
  ① 场景对象数 = S1 叶 occurrence 中成功载入的实例数（chain.compiled_summary.loaded_cad_instances）
  ② 每个动件在 f0 帧的位置 = home + axis*sense*approach，在 f1 帧 = home（误差 ≤1e-4 mm）
  ③ 帧范围与 chain.frames 一致
输出：<run>/reopen-readback.v1.json（PASS/FAIL + 逐件最大误差）

用法：blender -b --factory-startup -P reopen_readback_generic.py -- --case-dir <dir> --run FILM_v001
"""
import json
import sys
from pathlib import Path

import bpy
import mathutils

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


def arg(n, d):
    return argv[argv.index(n) + 1] if n in argv else d


CASE = Path(arg("--case-dir", "")).resolve()
RUN = CASE / "装配动画" / arg("--run", "FILM_v001")
chain = json.loads((RUN / "state-chain.json").read_text(encoding="utf-8"))
_cfgp = json.loads((CASE / "robot-config.v1.json").read_text())["paths"]
occ = json.loads((CASE / _cfgp["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
home_of = {o["occ_id"]: [o["world"][i][3] for i in range(3)] for o in occ}
# S5 折叠单元：动件=对象组，每个成员都要独立对账（同轴同窗，各自 home）
_bind = json.loads((CASE / _cfgp["s3_binding"]).read_text(encoding="utf-8"))
ent_members = {}
for _op0 in _bind["operations"]:
    for _m0 in _op0["movers"]:
        ent_members[_m0["occ_id"]] = _m0.get("member_occ_ids") or [_m0["occ_id"]]

bpy.ops.wm.open_mainfile(filepath=str(RUN / "assembly_source.blend"))
sc = bpy.context.scene

fails, checked, max_err = [], 0, 0.0
_empty_members = {e["member"] for e in chain.get("compiled_summary", {}).get("empty_geometry_members", []) or []}
n_obj = sum(1 for ob in sc.collection.objects if ob.name.startswith("occ_"))
if n_obj != chain["compiled_summary"]["loaded_cad_instances"]:
    fails.append({"kind": "OBJECT_COUNT", "expected": chain["compiled_summary"]["loaded_cad_instances"],
                  "actual": n_obj})
if [sc.frame_start, sc.frame_end] != chain["frames"]:
    fails.append({"kind": "FRAME_RANGE", "expected": chain["frames"],
                  "actual": [sc.frame_start, sc.frame_end]})

for c in chain["chapters"]:
    if c.get("finale"):
        continue
    for m in c["movers"]:
        ax = mathutils.Vector(m["insertion_axis_world"]).normalized()
        off = ax * (m["withdraw_sense"] * m["approach_mm"])
        group = [mid for mid in ent_members.get(m["occ"], [m["occ"]]) if mid in home_of]
        if not group:
            fails.append({"kind": "MISSING_OBJECT", "occ": m["occ"]})
            continue
        obs = []
        for mid in group:
            if mid in _empty_members:   # OLSK 增量：构建阶段列账的无几何成员（EMPTY_BBOX）按定义无对象，不计 MISSING_OBJECT
                continue
            ob = sc.objects.get(mid)
            if ob is None:
                fails.append({"kind": "MISSING_OBJECT", "occ": m["occ"], "member": mid})
            else:
                obs.append((mid, ob))
        if not obs:
            continue
        worst0 = worst1 = 0.0
        sc.frame_set(m["f0"])
        for mid, ob in obs:
            start = mathutils.Vector(home_of[mid]) + off
            worst0 = max(worst0, (ob.matrix_world.translation - start).length)
        sc.frame_set(m["f1"])
        for mid, ob in obs:
            home = mathutils.Vector(home_of[mid])
            worst1 = max(worst1, (ob.matrix_world.translation - home).length)
        err = max(worst0, worst1)
        max_err = max(max_err, err)
        checked += 1
        if err > 1e-4:
            fails.append({"kind": "MOTION_TUPLE", "occ": m["occ"],
                          "members": len(group),
                          "f0_err_mm": round(worst0, 6), "f1_err_mm": round(worst1, 6)})

doc = {"$schema": "assembly-ontology.reopen-readback/v1",
       "run": arg("--run", "FILM_v001"),
       "movers_checked": checked,
       "max_error_mm": round(max_err, 9),
       "failures": fails,
       "status": "PASS" if not fails else "FAIL",
       "claim_boundary": ["独立进程重开回读；只验位置关键帧与对象数，不验颜色/相机/面板。"]}
(RUN / "reopen-readback.v1.json").write_text(json.dumps(doc, ensure_ascii=False, indent=1),
                                             encoding="utf-8")
print(f"[readback] movers {checked} max_err {max_err:.2e} mm  {doc['status']}")
sys.exit(0 if not fails else 1)
