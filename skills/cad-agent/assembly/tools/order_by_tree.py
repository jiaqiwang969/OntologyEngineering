#!/usr/bin/env python3
"""树引导排序（K-PATH-15 落地：装配序 = 连接图的生成树遍历）。

每工序内按分层 BFS 产序：
  第 0 层 = 有边挂到**锚集**(已装工序动件 ∪ 未绑定件)的结构件,按边数降序;
  第 n 层 = 有边挂到已排件的结构件;
  紧固件殿后,且各自尽量排在其全部被夹件之后;
  耦合对钉在工序末位(认证场景);无边件(S2 覆盖缺口/真悬空)保持原相对位置并标记。
几何不产序,只验证——树给候选序,行走认证,审计终门。

用法：python3 tools/order_by_tree.py
输入：S8 连接图 + S7 + 当前播放序(耦合对/参照) ；输出：原地重写 播放序 + 打印差异
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    g8 = json.loads((ROOT / "本体/S8-连接关系图.v1.json").read_text(encoding="utf-8"))
    s7 = json.loads((ROOT / "本体/S7-扫掠认证.v2.json").read_text(encoding="utf-8"))
    odp = ROOT / "本体/S7-工序内播放序.v1.json"
    od = json.loads(odp.read_text(encoding="utf-8"))

    node = {n["occ"]: n for n in g8["nodes"]}
    unbound = {n["occ"] for n in g8["nodes"] if not n.get("op")}
    adj = {}
    for e in g8["edges"]:
        adj.setdefault(e["a"], set()).add(e["b"])
        adj.setdefault(e["b"], set()).add(e["a"])
    fastens = {f: set(cl) for f, cl in g8.get("fastens", {}).items()}

    installed = set(unbound)          # 锚集:未绑定件常在;已装动件逐工序累加
    n_changed_ops = n_moved = 0
    for op in s7["operations"]:
        opn = op["op"]
        spec = od["ops"].get(opn)
        if not spec or len(spec["order"]) < 3:
            if spec:
                installed |= {e["occ"] for e in spec["order"]}
            continue
        order = spec["order"]
        by_occ = {e["occ"]: e for e in order}
        pairs = [e for e in order if e.get("pair_id")]
        core = [e for e in order if not e.get("pair_id")]
        stru = [e for e in core if not node.get(e["occ"], {}).get("is_fastener")]
        fast = [e for e in core if node.get(e["occ"], {}).get("is_fastener")]

        # 结构件分层 BFS
        placed, layers = [], []
        remaining = {e["occ"] for e in stru}
        anchor = set(installed)
        while remaining:
            layer = [o for o in remaining if adj.get(o, set()) & anchor]
            if not layer:
                layer = sorted(remaining, key=lambda o: [x["occ"] for x in stru].index(o))
                layers.append(("NO_EDGE", list(layer)))
                placed += layer
                break
            layer.sort(key=lambda o: (-len(adj.get(o, set()) & anchor),
                                      [x["occ"] for x in stru].index(o)))
            layers.append(("L", layer))
            placed += layer
            anchor |= set(layer)
            remaining -= set(layer)

        # 紧固件:被夹件全部已排者优先,其余按原序
        fast_order = sorted(
            (e["occ"] for e in fast),
            key=lambda o: (0 if fastens.get(o, set()) & set(placed) else 1,
                           [x["occ"] for x in fast].index(o)))
        newseq = ([by_occ[o] for o in placed] + [by_occ[o] for o in fast_order] + pairs)
        moved = sum(1 for i, e in enumerate(newseq) if e["occ"] != order[i]["occ"])
        if moved:
            n_changed_ops += 1
            n_moved += moved
            spec["order"] = newseq
            spec["tree_ordered"] = True
        installed |= {e["occ"] for e in order}
    od["tree_guidance"] = {"rule": "K-PATH-15:分层 BFS 生成树遍历产序;几何只验证",
                           "changed_ops": n_changed_ops, "moved_slots": n_moved}
    odp.write_text(json.dumps(od, ensure_ascii=False), encoding="utf-8")
    print(f"树引导排序:{n_changed_ops} 工序变更 · {n_moved} 席位移动")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
