#!/usr/bin/env python3
"""基线路径检查的共享判据（audit_path_penetration 与 validate_axis_choice 同口径）。

K-IF-04 区域+体积级实现，两阶段：
  阶段1（快，逐步）：非基线件接触 → 直接判穿模；
                   基线件上 t=0 接触区(膨胀邻域)之外的新区接触 → 记为嫌疑。
  阶段2（慢，仅嫌疑步）：动件采样点做射线含入测试——采样点进入基线件**材料内部**
                   数量超过 t=0 本底 → 真穿模；否则是接触斑沿配合面移动（球面
                   配合、深孔滑动、掠过卡槽边缘），合法。

为什么不用表面启发式：法向判据在球面配合上必然失效（球面法向覆盖所有方向），
邻域判据在深孔滑动上必然误报（接触斑随动件移动）。材料含入是唯一如实的问法。
fcl 的 mesh-mesh contact depth 不可用（0.1mm 重叠报整件尺寸），故用射线奇偶。
前提：零件级 STL 由 CAD 实体网格化而来，几何上封闭；不封闭的件（如 OmniPicker
游离面）含入测试不可信，调用方须按 no_solid 走保守代理口径。
"""
from __future__ import annotations

import numpy as np
import trimesh


class PathChecker:
    def __init__(self, mgr, mgr_meshes, world_T, dilate=2.0,
                 samples=260, inside_margin=3, step_quant=1e-6, inward_eps=0.3):
        # OLSK 增量（2026-09-03，v005）：含入采样点不再取网格顶点，而取表面采样点并沿法向**向材料内**
        # 偏移 inward_eps（用自身绕数校验方向，两侧都不在自身材料内的薄件样本丢弃）。
        # 根因：贴合面上的顶点恰在对方表面上，绕数 ≈0.5，t=0 本底被算成 25%（实测 OLSK 机架衬板
        # b019 对横梁 b012），随后衬板沿轴滑进横梁壁 27.8% 仍低于本底+5% 而放行——v004 成片
        # 画面口径扫描抓出的 30 条路径穿模里，机架衬板/角板 8 条皆此因。
        self.inward_eps = float(inward_eps)
        self.mgr = mgr
        self.meshes = mgr_meshes          # 管理器名 -> 局部系网格
        self.world_T = world_T            # 管理器名 -> 4x4 世界变换
        self.dilate = float(dilate)
        self.n_samples = int(samples)
        # 裕度取 max(绝对, 5% 采样点)：贴合曲面滑动时显示级网格（挠度≈对角线/400）
        # 会有 1–2% 采样点以亚毫米深度"蹭"进对方网格，这是网格化噪声不是隧道；
        # 真隧道的含入量级是 10–30%+。绝对值 3 在球面配合上恰好病态临界（实测
        # 脚踝连杆 4 点 vs 裕度 3,一步之差判定翻转）。
        self.inside_margin = max(int(inside_margin), int(0.05 * self.n_samples))
        self._local_pts = None
        self._T0 = None
        self._fcl_self = None
        self._psamp = {}
        self._rev_inside0 = {}
        self._allowed = {}
        self._base = set()
        self._inside0 = {}

    # ---- 每个动件调用一次 -------------------------------------------------
    def prepare(self, mesh, T0):
        self._T0 = np.array(T0, dtype=np.float64)
        self._fcl_self = None
        # 采样点：表面采样 + 向内偏移（见 __init__ 注释）；旧法（顶点去重）保留为 inward_eps<=0 时的行为
        self._local_pts = self._inward_samples(mesh, self.n_samples)
        self._mesh = mesh

        per = self._contacts(np.array([0.0, 0.0, 1.0]), 1, 0.0)
        self._base = set(per)
        self._allowed = {}
        for n, tids in per.items():
            cen = self.meshes[n].triangles_center
            ref = cen[np.array(sorted(tids))]
            dmin = np.full(len(cen), np.inf)
            for j in range(0, len(ref), 32):
                d = np.linalg.norm(cen[:, None, :] - ref[None, j:j + 32, :], axis=2).min(axis=1)
                dmin = np.minimum(dmin, d)
            self._allowed[n] = set(np.nonzero(dmin <= self.dilate)[0].tolist())
        # t=0 含入本底（配合网格互穿的噪声水平）：绕数不便宜，延迟到该件首次出现嫌疑再算
        self._inside0 = {}
        self._rev_inside0 = {}
        return self._base

    # ---- 逐步判定 ---------------------------------------------------------
    def bad_at(self, axis, sense, t):
        """返回 (bad, kind, 违规名集合)。kind ∈ {None, NON_BASELINE, BASELINE_MATERIAL}"""
        per = self._contacts(axis, sense, t)
        extra = set(per) - self._base
        if extra:
            # 贴触补录：就位态"恰好接触"（零深度）的界面在不同 BVH 构建下时有时无
            # ——实测关节05后壳的自家螺钉在 trimesh 端 t=0 报不撞、逐对 fcl 报 288 触。
            # 判据原文"t=0 的接触即许可界面"涵盖贴触：距离 ≤0.05mm 者补入基线，
            # 以首触三角形为许可区种子；真非基线（就位有间隙）不受此赦。
            import fcl as _fcl
            still = set()
            for n in extra:
                if self._seat_distance(n) <= 0.05:
                    self._base.add(n)
                    tids = per[n]
                    cen = self.meshes[n].triangles_center
                    ref = cen[np.array(sorted(tids))]
                    dmin = np.full(len(cen), np.inf)
                    for j in range(0, len(ref), 32):
                        d2 = np.linalg.norm(cen[:, None, :] - ref[None, j:j + 32, :],
                                            axis=2).min(axis=1)
                        dmin = np.minimum(dmin, d2)
                    self._allowed[n] = set(np.nonzero(dmin <= self.dilate)[0].tolist())
                else:
                    still.add(n)
            if still:
                return True, "NON_BASELINE", still
        suspects = {n for n, tids in per.items()
                    if n in self._base and not tids <= self._allowed[n]}
        zax = np.array([0.0, 0.0, 1.0])
        for n in suspects:
            if n not in self._inside0:
                self._inside0[n] = self._inside_count(n, 0.0, zax, 1)
            if n not in self._rev_inside0:
                self._rev_inside0[n] = self._rev_inside_count(n, 0.0, zax, 1)
        # 含入必须**双向**（用户第三次抓出的盲区）：大件吞小件时，落入小件体内的
        # 动件采样点寥寥（相对动件是 0.x%），但小件整颗被吞——反向数它的点进动件
        # 材料，裕度按**它自己**的采样数计。X1 实例：摆臂扫过夹它的螺钉群全绿。
        confirmed = {n for n in suspects
                     if (self._inside_count(n, t, axis, sense)
                         > self._inside0[n] + self.inside_margin)
                     or (self._rev_inside_count(n, t, axis, sense)
                         > self._rev_inside0[n] + self._rev_margin(n))}
        if confirmed:
            return True, "BASELINE_MATERIAL", confirmed
        return False, None, set()

    def _partner_samples(self, n):
        if n not in self._psamp:
            self._psamp[n] = self._inward_samples(self.meshes[n], self.n_samples)
        return self._psamp[n]

    def _inward_samples(self, mesh, n):
        """材料内部采样点（局部系）。inward_eps<=0 时退回顶点去重法。"""
        if self.inward_eps <= 0 or len(mesh.faces) == 0:
            v = np.unique(np.round(mesh.vertices, 2), axis=0)
            if len(v) > n:
                v = v[np.linspace(0, len(v) - 1, n).astype(int)]
            return v
        try:
            pts, fid = trimesh.sample.sample_surface(mesh, n, seed=0)
        except TypeError:
            np.random.seed(0)
            pts, fid = trimesh.sample.sample_surface(mesh, n)
        pts = np.asarray(pts, dtype=np.float64)
        nrm = np.asarray(mesh.face_normals[np.asarray(fid)], dtype=np.float64)
        cand = pts - nrm * self.inward_eps
        ok = np.abs(_winding(mesh, cand)) > 0.5
        flip = ~ok
        if flip.any():
            cand2 = pts[flip] + nrm[flip] * self.inward_eps
            ok2 = np.abs(_winding(mesh, cand2)) > 0.5
            cand[flip] = np.where(ok2[:, None], cand2, cand[flip])
            ok[flip] = ok2
        out = cand[ok]
        if len(out) < max(8, n // 4):      # 开壳/极薄件：内部样本不足，退回顶点法（保守代理口径由调用方决定）
            v = np.unique(np.round(mesh.vertices, 2), axis=0)
            if len(v) > n:
                v = v[np.linspace(0, len(v) - 1, n).astype(int)]
            return v
        return out

    def _rev_margin(self, n):
        return max(3, int(0.05 * len(self._partner_samples(n))))

    def _rev_inside_count(self, n, t, axis, sense):
        """反向含入：对方采样点进**动件**材料的数量（动件位于位移 dt 处）"""
        Tp = self.world_T[n]
        pw = self._partner_samples(n) @ Tp[:3, :3].T + Tp[:3, 3]
        M = self._T0.copy()
        M[:3, 3] = self._T0[:3, 3] + axis * (sense * t)
        pl = (pw - M[:3, 3]) @ M[:3, :3]
        w = _winding(self._mesh, pl)
        return int(np.count_nonzero(np.abs(w) > 0.5))

    def _seat_distance(self, n):
        """动件在就位态与对象 n 的最小距离（mm）"""
        import fcl as _fcl
        if self._fcl_self is None:
            self._fcl_self = self.mgr._get_fcl_obj(self._mesh)
        o1 = _fcl.CollisionObject(self._fcl_self,
                                  _fcl.Transform(self._T0[:3, :3], self._T0[:3, 3]))
        res = _fcl.DistanceResult()
        _fcl.distance(o1, self.mgr._objs[n]["obj"], _fcl.DistanceRequest(), res)
        return max(0.0, float(res.min_distance))

    # ---- 内部 -------------------------------------------------------------
    def _contacts(self, axis, sense, t):
        """名字广查 + 逐对细查。单次 manager 查询的接触枚举有全局上限，
        大壳件在就位态可有数千接触——截断会**漏记部分基线伙伴**，
        它们随后被误判为非基线（实测：关节05后壳漏掉自家两颗未绑定螺钉→假卡死）。
        逐对枚举每对独享上限，伙伴不会互相挤掉。"""
        import fcl as _fcl
        M = self._T0.copy()
        M[:3, 3] = self._T0[:3, 3] + axis * (sense * t)
        # 广查用缓存的动件 BVH（in_collision_single 每步重建 BVH，大合并网格上是
        # 主开销；此处与其语义等价：同一 manager.collide + 同 contacts 名字提取）
        if self._fcl_self is None:
            self._fcl_self = self.mgr._get_fcl_obj(self._mesh)
        R, tv = M[:3, :3], M[:3, 3]
        o1 = _fcl.CollisionObject(self._fcl_self, _fcl.Transform(R, tv))
        cdata = _fcl.CollisionData(
            request=_fcl.CollisionRequest(num_max_contacts=100000, enable_contact=False))
        self.mgr._manager.collide(o1, cdata, _fcl.defaultCollisionCallback)
        if not cdata.result.is_collision:
            return {}
        nms = set()
        for contact in cdata.result.contacts:
            cg = contact.o1
            if cg == self._fcl_self:
                cg = contact.o2
            nm = self.mgr._extract_name(cg)
            if nm is not None:
                nms.add(nm)
        if not nms:
            return {}
        req = _fcl.CollisionRequest(num_max_contacts=100000, enable_contact=True)
        per = {}
        for n in nms:
            o2 = self.mgr._objs[n]["obj"]
            res = _fcl.CollisionResult()
            _fcl.collide(o1, o2, req, res)
            if res.contacts:
                per[n] = {int(c.b2) for c in res.contacts}
        return per

    def _inside_count(self, n, t, axis, sense):
        M = self._T0.copy()
        M[:3, 3] = self._T0[:3, 3] + axis * (sense * t)
        pw = self._local_pts @ M[:3, :3].T + M[:3, 3]
        Tp = self.world_T[n]
        pl = (pw - Tp[:3, 3]) @ Tp[:3, :3]      # 世界 -> 基线件局部
        w = _winding(self.meshes[n], pl)
        return int(np.count_nonzero(np.abs(w) > 0.5))


# 广义绕数（Barill et al. 的精确立体角式，纯 numpy）。
# 为什么不用 trimesh.contains：它走射线奇偶——依赖 rtree（缺了直接抛），
# 且在 CAD 导出的开壳/自交网格上双向撒谎（实测把螺钉杆判成"全在"薄壁轴承内环
# 材料里、把明明分离的兄弟件判成 260/260 全含）。绕数对开壳退化成分数值，
# |w|>0.5 阈值依然稳健，且永不抛异常。
def _winding(mesh, pts, chunk=2_000_000):
    tri = mesh.triangles          # (m,3,3)
    m = len(tri)
    npt = len(pts)
    out = np.zeros(npt, dtype=np.float64)
    # 分块控制内存：每块 点×三角形 组合不超过 chunk
    rows = max(1, int(chunk // max(m, 1)))
    for i0 in range(0, npt, rows):
        p = pts[i0:i0 + rows]                       # (k,3)
        a = tri[None, :, 0, :] - p[:, None, :]      # (k,m,3)
        b = tri[None, :, 1, :] - p[:, None, :]
        c = tri[None, :, 2, :] - p[:, None, :]
        la = np.linalg.norm(a, axis=2)
        lb = np.linalg.norm(b, axis=2)
        lc = np.linalg.norm(c, axis=2)
        num = np.einsum("kmi,kmi->km", a, np.cross(b, c))
        den = (la * lb * lc + np.einsum("kmi,kmi->km", a, b) * lc
               + np.einsum("kmi,kmi->km", b, c) * la
               + np.einsum("kmi,kmi->km", a, c) * lb)
        out[i0:i0 + rows] = np.arctan2(num, den).sum(axis=1) / (2.0 * np.pi)
    return out
