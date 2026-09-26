#!/usr/bin/env python3
"""S10·树一致性审计（BrickNet 借鉴 ①：装配序 = 连接图的生成树）。

BrickNet 的 Tree 表征把"怎么搭"定义为生成树：每个新件必须通过至少一条连接边
挂到**已建子图**上。本审计按成片播放序逐件核查入场件的挂接性：

  TIER_A  有边挂到已装动件            —— 强一致（树性质成立）
  TIER_B  只有边挂到未绑定/上下文件    —— 弱挂接（挂在"从未被安装"的脚手架上，
          正是 ReachyMini 镜头先置位那一类的图上表述），逐条列出
  NO_EDGE_ENTRY  一条已实现边都没有    —— **悬空入场，报警**（物理约束台账新类）
  FASTENER_BEFORE_CLAMPED  紧固件先于其全部被夹件入场 —— 报警（叶位违规）

用法：python3 tools/audit_tree_conformance.py [--run FILM_v013]
输出：本体/S10-树一致性审计.v1.json
"""
import argparse
import json
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
G8 = ROOT / "本体/S8-连接关系图.v1.json"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", default="FILM_v013")
    a = ap.parse_args()

    chain = json.loads((ROOT / "装配动画" / a.run / "state-chain.json")  # OLSK 增量：案例目录布局为 装配动画/<run>/（X1 为 工具/装配动画/）
                       .read_text(encoding="utf-8"))
    g8 = json.loads(G8.read_text(encoding="utf-8"))
    node = {n["occ"]: n for n in g8["nodes"]}
    unbound = {n["occ"] for n in g8["nodes"] if not n.get("op")}
    adj = {}
    for e in g8["edges"]:
        adj.setdefault(e["a"], set()).add(e["b"])
        adj.setdefault(e["b"], set()).add(e["a"])
    fastens = {f: set(cl) for f, cl in g8.get("fastens", {}).items()}

    installed = set()
    rows = []
    for ch in chain["chapters"]:
        if ch.get("finale"):
            continue
        installed |= set(ch.get("static_withdrawn", []))   # 静态呈现的退场件是可见锚
        # 上下文里的**绑定**退场件同样是稳定锚(它们以暗色就位态可见);
        # 未绑定上下文不入锚——TIER_B 的"挂在脚手架上"语义要保留。
        installed |= {o2 for o2 in ch.get("context", [])
                      if node.get(o2, {}).get("op")}
        for m in ch["movers"]:
            oid = m["occ"]
            nb = adj.get(oid, set())
            to_installed = sorted(nb & installed)
            to_unbound = sorted(nb & unbound)
            if to_installed:
                tier = "TIER_A"
            elif to_unbound:
                tier = "TIER_B"
            else:
                tier = "NO_EDGE_ENTRY"
            fast_alarm = False
            if node.get(oid, {}).get("is_fastener"):
                cl = fastens.get(oid, set())
                bound_cl = {c for c in cl if node.get(c, {}).get("op")}
                if bound_cl and not (bound_cl & installed):
                    fast_alarm = True
            rows.append({
                "op": ch["op"], "occ": oid,
                "product": node.get(oid, {}).get("product", "")[:36],
                "tier": tier,
                "edges_to_installed": to_installed[:6],
                "edges_to_unbound_only": to_unbound[:6] if tier == "TIER_B" else [],
                "fastener_before_clamped": fast_alarm,
            })
            installed.add(oid)

    alarms = [r for r in rows
              if r["tier"] == "NO_EDGE_ENTRY" or r["fastener_before_clamped"]]
    doc = {
        "$schema": "x1.tree-conformance-audit/v1",
        "purpose": "BrickNet Tree 表征借鉴：装配序应为连接图生成树；入场件必须有已实现边"
                   "挂到已建子图，紧固件必须后于被夹件（叶位）。悬空入场即报警。",
        "run": a.run,
        "summary": {
            "动件": len(rows),
            "按层": dict(Counter(r["tier"] for r in rows)),
            "紧固件叶位违规": sum(1 for r in rows if r["fastener_before_clamped"]),
            "报警数": len(alarms),
        },
        "alarms": alarms,
        "rows": rows,
    }
    (ROOT / "本体/S10-树一致性审计.v1.json").write_text(
        json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print("树一致性:", json.dumps(doc["summary"], ensure_ascii=False))
    for r in alarms[:10]:
        why = "无边入场" if r["tier"] == "NO_EDGE_ENTRY" else "紧固件先于被夹件"
        print(f"  ⚠ {r['op']} {r['occ']}({r['product'][:16]}) {why}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
