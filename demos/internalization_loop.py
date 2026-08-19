"""佐证 demo · domain-ontology-loop —— 行业本体的迭代内化：学新不忘旧。

模板论断（skills/domain-ontology-loop/SKILL.md，规矩皆源自两卷书）：

  1) 冲突与删除必须带理由的判决，否则 commit 被拒（ch17：保留是判断不是默认）；
  2) 每版是带 checksum 与 PROV 派生链的快照，init 拒绝覆盖谱系（ch20）；
  3) ★ 旧 CQ 是防遗忘回归集：连学三课后，第一课的 CQ 依然全绿=没忘；
     而当一次（程序上合法的）删除真的抹掉旧知识时，regress 必须当场抓住
     ——"忘没忘"不靠感觉，靠回归（第一卷 ch03 CQ 即验收）。

执行：用书内 I01/S01 数据在临时工作区完整走三圈循环 + 一次故意的知识删除。
"""

import _common  # noqa: F401 — 静默 Semantica 进度输出

import json
import subprocess
import sys
import tempfile
from pathlib import Path

SKILL_ROOT = Path(__file__).resolve().parents[1]
TOOL = SKILL_ROOT / "skills/domain-ontology-loop/scripts/internalize.py"
NS = "https://w3id.org/domain-ontology-loop#"


def run(*args, expect=0):
    r = subprocess.run([sys.executable, str(TOOL), *args],
                       capture_output=True, text=True)
    return r


ws = Path(tempfile.mkdtemp(prefix="dol-")) / "fusion-curriculum"
tmp = ws.parent

# 三课 delta（内容取自书内 I01 与 S01 的真实类）
d1 = {"classes": [{"name": n, "comment": c} for n, c in [
    ("IntentMode", "决定建模环境与可用工具的设计意图模式"),
    ("PartMode", "仅零件创建工具"), ("AssemblyMode", "仅装配命令"),
    ("HybridMode", "零件与装配命令并存"), ("WorkflowTransition", "模式间受支持的转换")]],
    "properties": [{"name": "hasIntentMode", "comment": ""}]}
d2 = {"classes": [{"name": n, "comment": c} for n, c in [
    ("CourseOrientationLesson", "以路线图为产出的课程"),
    ("RoadmapStage", "有目标与证据锚的下游学习单元"),
    ("CurrentCapabilityContact", "非破坏性的当前工具能力检查")]]}
d3_conflict = {"classes": [
    {"name": "IntentMode", "comment": "（S06 补充）意图模式还约束保存与协作行为"},
    {"name": "TSplineBody", "comment": "T-Spline 造型体"}]}
d4_remove = {"classes": [], "removes": ["PartMode"]}

for name, obj in [("d1", d1), ("d2", d2), ("d3", d3_conflict), ("d4", d4_remove)]:
    (tmp / f"{name}.json").write_text(json.dumps(obj, ensure_ascii=False))

print("【模板论断】无判决的冲突被拒；三版后旧 CQ 全绿；删除造成的真遗忘被回归当场抓住")
print("【锚点】skills/domain-ontology-loop/SKILL.md · 书 ch17/ch20 · 第一卷 ch03\n")

results = []

# v1：I01 基线 + 第一课 CQ
run("init", "--workspace", str(ws), "--name", "FusionCurriculum",
    "--baseline", str(tmp / "d1.json"), "--attempt", "I01")
(ws / "cq-bank/CQ-I01-modes.json").write_text(json.dumps({
    "id": "CQ-I01", "question": "三种意图模式在本体中吗？",
    "sparql": f"PREFIX dom: <{NS}> PREFIX owl: <http://www.w3.org/2002/07/owl#> "
              "SELECT ?c WHERE { VALUES ?c { dom:PartMode dom:AssemblyMode dom:HybridMode } "
              "?c a owl:Class }", "min_rows": 3}, ensure_ascii=False))
reinit = run("init", "--workspace", str(ws), "--name", "X")
results.append(("init 拒绝覆盖已有谱系（ch20）", reinit.returncode != 0))

# v2：S01 顺利合并 + 新 CQ 入库
run("commit", "--workspace", str(ws), "--delta", str(tmp / "d2.json"), "--attempt", "S01")
(ws / "cq-bank/CQ-S01-orientation.json").write_text(json.dumps({
    "id": "CQ-S01", "question": "路线图课程概念在本体中吗？", "ask": True,
    "sparql": f"PREFIX dom: <{NS}> PREFIX owl: <http://www.w3.org/2002/07/owl#> "
              "ASK {{ dom:CourseOrientationLesson a owl:Class }}"}, ensure_ascii=False))

# v3：冲突——无判决被拒，判决（带理由）后合并
r_noverdict = run("commit", "--workspace", str(ws), "--delta", str(tmp / "d3.json"),
                  "--attempt", "S06")
results.append(("同名不同义、无判决 -> commit 拒绝", r_noverdict.returncode != 0))
print("门禁输出：", [l for l in r_noverdict.stdout.splitlines() if "✗" in l])
(tmp / "verdicts.json").write_text(json.dumps({
    "IntentMode": {"action": "merge", "reason": "S06 补充协作语境，I01 原义仍成立，二者并存"}},
    ensure_ascii=False))
r_v3 = run("commit", "--workspace", str(ws), "--delta", str(tmp / "d3.json"),
           "--verdicts", str(tmp / "verdicts.json"), "--attempt", "S06")
results.append(("判决带理由后合并为 v3", r_v3.returncode == 0))

# ★ 学了三课，第一课的 CQ 还答得上吗
r_reg = run("regress", "--workspace", str(ws))
print("\n三版之后的防遗忘回归：")
print("\n".join("  " + l for l in r_reg.stdout.splitlines()))
results.append(("v3 上旧 CQ（I01/S01）全绿=没忘", r_reg.returncode == 0))

# v4：程序合法的删除抹掉旧知识——回归必须抓住
(tmp / "verdicts4.json").write_text(json.dumps({
    "PartMode": {"reason": "（故意的教学反例）假设并入 IntentMode，未改写受波及 CQ"}},
    ensure_ascii=False))
run("commit", "--workspace", str(ws), "--delta", str(tmp / "d4.json"),
    "--verdicts", str(tmp / "verdicts4.json"), "--attempt", "bad-merge")
r_reg2 = run("regress", "--workspace", str(ws))
print("\n删除 PartMode 后的回归（应当失败——这正是要点）：")
print("\n".join("  " + l for l in r_reg2.stdout.splitlines()))
results.append(("真遗忘被 regress 当场抓住（非零退出）", r_reg2.returncode != 0))

hist = run("history", "--workspace", str(ws))
print("\n版本谱系：")
print(hist.stdout)

ok = all(r for _, r in results)
for name, r in results:
    print(f"  [{'✓' if r else '✗'}] {name}")
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：内化循环学新不忘旧——"
      f"冲突要判决、版本有谱系、旧 CQ 是防遗忘的硬门禁，连'合规的删除'造成的"
      f"知识丢失也逃不过回归。")
sys.exit(0 if ok else 1)
