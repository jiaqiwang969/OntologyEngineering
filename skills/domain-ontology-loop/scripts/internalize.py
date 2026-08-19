#!/usr/bin/env python3
"""domain-ontology-loop 内化工具：把工程实践的本体 delta 受控地长进行业本体。

设计对应第二卷的治理规矩：
  - ch17 变化本体：改动必须过差异分析；无理由的覆盖/删除被拒（保留是判断，不是默认）；
  - ch20 发布保证：每个版本是带校验和的快照，派生链（PROV）可追；
  - 第一卷 ch03：CQ 库是防遗忘回归集——新版必须仍能答对全部旧 CQ。

工作区布局（由 init 创建）：
  <workspace>/
    ontology.json        当前版（classes/properties: name -> {comment, source}）
    ontology.ttl         当前版 OWL 渲染（rdflib）
    versions/vNNNN.json  历史快照（含 checksum、parent、attempt、delta 摘要）
    changelog.jsonl      每次 commit 一行
    prov.ttl             版本派生链（W3C PROV）
    cq-bank/*.json       能力问题回归集（SPARQL + 期望）

delta 文件格式（把各来源的 schema 变体归一化到此再进循环）：
  {"classes": [{"name": "...", "comment": "..."}],
   "properties": [{"name": "...", "comment": ""}],
   "removes": ["ClassName", ...],            # 可选，删除必须带 verdict
   "source": {"attempt": "...", "note": "..."}}

verdict 文件格式（propose 报告冲突后，人写判决再 commit）：
  {"ClassName": {"action": "replace|keep_old|merge", "reason": "非空理由"}}
"""

import argparse
import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

NS = "https://w3id.org/domain-ontology-loop#"


# ---------------------------------------------------------------- 基础
def load(p: Path):
    return json.loads(p.read_text()) if p.exists() else None


def dump(p: Path, obj):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + "\n")


def checksum(obj) -> str:
    return hashlib.sha256(json.dumps(obj, ensure_ascii=False, sort_keys=True).encode()).hexdigest()


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def render_ttl(ontology: dict, ns: str) -> str:
    """把 ontology.json 渲染为 OWL Turtle（rdflib）。"""
    from rdflib import Graph, Namespace, RDF, RDFS, OWL, Literal
    g = Graph()
    n = Namespace(ns)
    g.bind("dom", n)
    for name, meta in ontology.get("classes", {}).items():
        c = n[name]
        g.add((c, RDF.type, OWL.Class))
        g.add((c, RDFS.label, Literal(name)))
        if meta.get("comment"):
            g.add((c, RDFS.comment, Literal(meta["comment"])))
    for name, meta in ontology.get("properties", {}).items():
        p = n[name]
        g.add((p, RDF.type, OWL.ObjectProperty))
        g.add((p, RDFS.label, Literal(name)))
        if meta.get("comment"):
            g.add((p, RDFS.comment, Literal(meta["comment"])))
    return g.serialize(format="turtle")


def write_state(ws: Path, ontology: dict):
    dump(ws / "ontology.json", ontology)
    (ws / "ontology.ttl").write_text(render_ttl(ontology, ontology.get("namespace", NS)))


# ---------------------------------------------------------------- 差异与冲突
def diff_delta(ontology: dict, delta: dict) -> dict:
    """差异分析：新增 / 冲突（同名不同义）/ 删除。"""
    adds_c, conflicts = [], []
    for c in delta.get("classes", []):
        name, new_comment = c["name"], c.get("comment", "")
        old = ontology["classes"].get(name)
        if old is None:
            adds_c.append(name)
        elif new_comment and old.get("comment") and new_comment != old["comment"]:
            conflicts.append({"name": name, "old": old["comment"], "new": new_comment})
    adds_p = [p["name"] for p in delta.get("properties", [])
              if p["name"] not in ontology["properties"]]
    removes = delta.get("removes", [])
    return {"adds_classes": adds_c, "adds_properties": adds_p,
            "conflicts": conflicts, "removes": removes}


def check_verdicts(report: dict, verdicts: dict) -> list:
    """ch17 规矩：冲突与删除必须有带理由的判决。返回违规清单。"""
    violations = []
    for c in report["conflicts"]:
        v = verdicts.get(c["name"])
        if not v or v.get("action") not in ("replace", "keep_old", "merge"):
            violations.append(f"冲突未判决：{c['name']}（同名不同义，需 replace/keep_old/merge）")
        elif not str(v.get("reason", "")).strip():
            violations.append(f"判决缺理由：{c['name']}——保留/替换是判断，不是默认")
    for name in report["removes"]:
        v = verdicts.get(name)
        if not v or not str(v.get("reason", "")).strip():
            violations.append(f"删除缺理由：{name}——宣布作废须说明波及与依据")
    return violations


# ---------------------------------------------------------------- 命令
def cmd_init(a):
    ws = Path(a.workspace)
    if (ws / "ontology.json").exists():
        sys.exit("workspace 已存在，拒绝覆盖（发布保证：不得静默重建谱系）")
    baseline = load(Path(a.baseline)) if a.baseline else {"classes": [], "properties": []}
    ontology = {
        "name": a.name, "namespace": a.namespace or NS, "version": 1,
        "classes": {c["name"]: {"comment": c.get("comment", "")}
                    for c in baseline.get("classes", [])},
        "properties": {p["name"]: {"comment": p.get("comment", "")}
                       for p in baseline.get("properties", [])},
    }
    write_state(ws, ontology)
    snap = {"version": 1, "parent": None, "attempt": a.attempt or "init",
            "recorded_at": now(), "checksum": checksum(ontology),
            "delta": {"adds_classes": sorted(ontology["classes"]),
                      "adds_properties": sorted(ontology["properties"]),
                      "conflicts": [], "removes": []}}
    dump(ws / "versions" / "v0001.json", snap)
    (ws / "cq-bank").mkdir(exist_ok=True)
    with open(ws / "changelog.jsonl", "a") as f:
        f.write(json.dumps({"v": 1, "attempt": snap["attempt"], "at": snap["recorded_at"]},
                           ensure_ascii=False) + "\n")
    _append_prov(ws, 1, None, snap)
    print(f"init: v1，classes={len(ontology['classes'])}，properties={len(ontology['properties'])}")


def cmd_propose(a):
    ws = Path(a.workspace)
    ontology = load(ws / "ontology.json") or sys.exit("workspace 未初始化")
    report = diff_delta(ontology, load(Path(a.delta)))
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if report["conflicts"] or report["removes"]:
        print("\n-> 存在冲突/删除：commit 前需提供带理由的 verdicts 文件（ch17：无理由不合并）",
              file=sys.stderr)
        sys.exit(2)


def cmd_commit(a):
    ws = Path(a.workspace)
    ontology = load(ws / "ontology.json") or sys.exit("workspace 未初始化")
    delta = load(Path(a.delta))
    verdicts = load(Path(a.verdicts)) if a.verdicts else {}
    report = diff_delta(ontology, delta)
    violations = check_verdicts(report, verdicts or {})
    if violations:
        print("门禁拒绝本次内化：")
        for v in violations:
            print(f"  ✗ {v}")
        sys.exit(1)
    # 应用：新增 + 判决过的替换/合并 + 判决过的删除
    for c in delta.get("classes", []):
        name = c["name"]
        v = (verdicts or {}).get(name, {})
        if name not in ontology["classes"] or v.get("action") in ("replace", "merge"):
            entry = {"comment": c.get("comment", "")}
            if v.get("action") == "merge" and ontology["classes"].get(name, {}).get("comment"):
                entry["comment"] = ontology["classes"][name]["comment"] + "；" + entry["comment"]
            if v:
                entry["verdict"] = {"action": v["action"], "reason": v["reason"]}
            ontology["classes"][name] = entry
    for p in delta.get("properties", []):
        ontology["properties"].setdefault(p["name"], {"comment": p.get("comment", "")})
    for name in report["removes"]:
        ontology["classes"].pop(name, None)
    ontology["version"] += 1
    vn = ontology["version"]
    write_state(ws, ontology)
    parent = load(ws / "versions" / f"v{vn-1:04d}.json")
    snap = {"version": vn, "parent": parent["checksum"], "attempt": a.attempt or "unnamed",
            "recorded_at": now(), "checksum": checksum(ontology), "delta": report,
            "verdicts": verdicts or {}}
    dump(ws / "versions" / f"v{vn:04d}.json", snap)
    with open(ws / "changelog.jsonl", "a") as f:
        f.write(json.dumps({"v": vn, "attempt": snap["attempt"], "at": snap["recorded_at"],
                            "adds": len(report["adds_classes"]),
                            "conflicts": len(report["conflicts"])}, ensure_ascii=False) + "\n")
    _append_prov(ws, vn, vn - 1, snap)
    print(f"commit: v{vn}，+{len(report['adds_classes'])} 类，"
          f"{len(report['conflicts'])} 项冲突已判决，classes={len(ontology['classes'])}")


def cmd_regress(a):
    """CQ 防遗忘回归：当前版必须答对 cq-bank 全部问题（旧 CQ 即旧知识）。"""
    from rdflib import Graph
    ws = Path(a.workspace)
    g = Graph()
    g.parse(ws / "ontology.ttl", format="turtle")
    cqs = sorted((ws / "cq-bank").glob("*.json"))
    failed = 0
    for f in cqs:
        cq = load(f)
        res = g.query(cq["sparql"])
        if cq.get("ask"):
            ok = bool(res)
        else:
            ok = len(list(res)) >= cq.get("min_rows", 1)
        failed += 0 if ok else 1
        print(f"  [{'✓' if ok else '✗'}] {cq['id']} {cq['question'][:50]}"
              + ("" if ok else "  <- 旧知识回归失败（真的忘了）"))
    print(f"regress: {len(cqs) - failed}/{len(cqs)} 通过")
    sys.exit(0 if failed == 0 else 1)


def cmd_history(a):
    ws = Path(a.workspace)
    for f in sorted((ws / "versions").glob("v*.json")):
        s = load(f)
        print(f"  v{s['version']:>3} {s['recorded_at']} attempt={s['attempt']} "
              f"+{len(s['delta']['adds_classes'])}类 冲突{len(s['delta']['conflicts'])} "
              f"sha={s['checksum'][:12]}")


def _append_prov(ws: Path, vn: int, parent_vn, snap: dict):
    lines = []
    uri = f"<{NS}version/v{vn}>"
    lines.append(f"{uri} a <http://www.w3.org/ns/prov#Entity> ;")
    lines.append(f'    <http://www.w3.org/ns/prov#generatedAtTime> "{snap["recorded_at"]}" ;')
    lines.append(f'    <{NS}checksum> "{snap["checksum"]}" ;')
    lines.append(f'    <{NS}attempt> "{snap["attempt"]}" .')
    if parent_vn:
        lines.append(f"{uri} <http://www.w3.org/ns/prov#wasDerivedFrom> <{NS}version/v{parent_vn}> .")
    with open(ws / "prov.ttl", "a") as f:
        f.write("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("init"); p.add_argument("--workspace", required=True)
    p.add_argument("--name", required=True); p.add_argument("--baseline")
    p.add_argument("--namespace"); p.add_argument("--attempt"); p.set_defaults(fn=cmd_init)
    p = sub.add_parser("propose"); p.add_argument("--workspace", required=True)
    p.add_argument("--delta", required=True); p.set_defaults(fn=cmd_propose)
    p = sub.add_parser("commit"); p.add_argument("--workspace", required=True)
    p.add_argument("--delta", required=True); p.add_argument("--verdicts")
    p.add_argument("--attempt"); p.set_defaults(fn=cmd_commit)
    p = sub.add_parser("regress"); p.add_argument("--workspace", required=True)
    p.set_defaults(fn=cmd_regress)
    p = sub.add_parser("history"); p.add_argument("--workspace", required=True)
    p.set_defaults(fn=cmd_history)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
