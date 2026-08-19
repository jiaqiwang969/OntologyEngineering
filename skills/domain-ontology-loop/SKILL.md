---
name: domain-ontology-loop
description: Grow a domain/industry ontology from ongoing engineering practice through a governed internalization loop — baseline snapshot, delta proposal, conflict verdicts with reasons, versioned commit with provenance, and competency-question regression so new lessons never silently erase old knowledge. Use when a team (e.g. a CAD curriculum, a factory QC program, a design-review practice) wants to repeatedly convert practice artifacts into an accumulating ontology instead of one-off deltas; when someone asks how to iterate an ontology without forgetting; or when auditing why an ontology stopped growing. Do not use for one-shot ontology generation without regression.
---

# Domain Ontology Loop（工程实践 → 行业本体的迭代内化）

一句话：**学新不忘旧，靠的不是感觉，是版本谱系 + 冲突判决 + 旧 CQ 回归。**

本模板把第二卷的治理方法用在本体自己身上：本体版本是发布快照（ch20），
每次实践产出的 delta 是变更（ch17），能力问题库是防遗忘回归集（第一卷 ch03
「CQ 即验收测试」）。任何领域——CAD 课程、产线质检、评审实践——都可以照此
把自己的工程实践不断内化成行业本体。

## 循环（每次实践一圈）

```
实践产出 delta（归一化为 delta.json）
  → propose   差异分析：新增了什么、和旧知识撞了什么、想删什么
  → 人判决    冲突与删除必须带理由（ch17：保留/作废是判断，不是默认）
  → commit    合并 → 新版本快照（checksum + PROV 派生链 + changelog）
  → regress   ★ 旧 CQ 全部重跑：答不上 = 真的忘了，门禁拦下不许发版
  → 新 CQ 入库，成为下一轮的"旧知识"
```

RDF 的单调语义保证"新增不撤销旧结论"（第一卷 ch02，有 runnable 佐证）；
会造成遗忘的只有两件事：**无版本的覆盖**和**无判决的删除**——循环的门禁
正是拦这两件事的。

## 上手

```bash
# 1. 用第一课的产出建基线（拒绝覆盖已有 workspace）
python3 scripts/internalize.py init \
  --workspace ./my-domain --name MyDomainOntology \
  --baseline lesson01-delta.json --attempt lesson01

# 2. 每完成一次实践：先看差异
python3 scripts/internalize.py propose --workspace ./my-domain --delta lesson02-delta.json
#    有冲突/删除时退出码 2，写 verdicts.json（每条带 action + 非空 reason）再提交

# 3. 受控合并出新版
python3 scripts/internalize.py commit --workspace ./my-domain \
  --delta lesson02-delta.json --verdicts verdicts.json --attempt lesson02

# 4. 防遗忘回归（旧 CQ + 新 CQ，全绿才算内化完成）
python3 scripts/internalize.py regress --workspace ./my-domain

# 5. 随时查谱系
python3 scripts/internalize.py history --workspace ./my-domain
```

工作区结构、delta/verdict/CQ 文件格式、以及 `01-fusion-tutorial`（CAD 课程）
如何映射到本循环，见 `references/loop-contract.md`。

## 门禁规矩（与书对应）

| 规矩 | 出处 | 工具行为 |
|---|---|---|
| 同名不同义必须判决，判决必须带理由 | ch17「保留是判断，不是默认」 | `commit` 拒绝无判决/无理由的冲突 |
| 删除必须说明依据与波及 | ch17「宣布作废却说不出作废了谁，是挥手」 | `commit` 拒绝无理由的 removes |
| 版本不可静默重建 | ch20 发布快照 | `init` 拒绝覆盖已有 workspace；每版带 checksum + parent |
| 派生可追 | ch20 / PROV | `prov.ttl` 记录 vN wasDerivedFrom vN-1 |
| 旧知识以旧 CQ 度量 | 第一卷 ch03 CQ 即验收 | `regress` 任一旧 CQ 失败即非零退出 |

## 佐证

`demos/internalization_loop.py`（仓库根 demos/）用书内 I01/S01 数据完整走一圈，
并证明：冲突无理由被拒、判决后合并、三个版本后 v1 时代的 CQ 依然全绿。
CI（corroboration workflow）每次 push 重跑。
