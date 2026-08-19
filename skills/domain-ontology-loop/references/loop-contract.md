# Loop Contract：工作区、文件格式与 CAD 课程映射

## 工作区布局

```
<workspace>/
  ontology.json        当前版：{name, namespace, version, classes{}, properties{}}
  ontology.ttl         当前版 OWL 渲染（每次 commit 重生成）
  versions/vNNNN.json  快照：{version, parent(父版 checksum), attempt, recorded_at,
                             checksum, delta 摘要, verdicts}
  changelog.jsonl      一次 commit 一行
  prov.ttl             vN prov:wasDerivedFrom vN-1 派生链
  cq-bank/*.json       能力问题回归集（累积，不删除）
```

## delta.json（入口归一化层）

各实践来源的 schema 变体（cad-agent 就出现过 `classes` / `classes_added` /
`adds` / `additions` / `new_classes` / `candidate_concepts` / `adds_classes`
七种）**必须在进循环前归一化**为：

```json
{
  "classes":    [{"name": "IntentMode", "comment": "……"}],
  "properties": [{"name": "hasIntentMode", "comment": ""}],
  "removes":    ["ObsoleteClass"],
  "source":     {"attempt": "S07-attempt-0001", "note": "…"}
}
```

归一化代码可复用 `~/148-Semantica/fusion-i01-redo/fusion_all_lessons_semantica.py`
中的 `normalize()`（处理了全部七种变体）。新领域请从第一天就只用本格式，
不要重演 schema 碎片化。

## verdicts.json（冲突判决）

`propose` 报告冲突（同名不同义）或删除后，人写判决：

```json
{
  "IntentMode": {"action": "merge",   "reason": "新课补充了 Hybrid 语境，旧义仍成立"},
  "OldClass":   {"action": "keep_old", "reason": "新定义与 S03 证据段冲突，退回重审"},
  "ObsoleteClass": {"reason": "S14 后该概念并入 XxxClass，波及 CQ-S09-03 已改写"}
}
```

规则：`action` ∈ replace / keep_old / merge；`reason` 不得为空；
删除只需 reason（说明依据与波及）。无判决或空理由 → commit 拒绝。

## cq-bank/*.json（防遗忘回归集）

```json
{
  "id": "CQ-I01-01",
  "question": "三种意图驱动设计模式是什么？",
  "sparql": "PREFIX dom: <…#> SELECT ?c WHERE { ?c a owl:Class … }",
  "min_rows": 3
}
```

- `ask: true` 时按 ASK 判定；否则要求结果行数 ≥ `min_rows`（默认 1）。
- CQ 只增不删：每课内化完成后把新 CQ 放入库，它就成为下一轮的"旧知识"。
- 需要废止某 CQ 时，不删文件，而是随删除判决一并改写并在 reason 里注明——
  回归集自身的变化也要有账。

## 01-fusion-tutorial（CAD 课程）映射

| 课程侧 | 循环侧 |
|---|---|
| 每课 attempt 的 ontology-delta.json（7 种变体） | 归一化后的 delta.json |
| competency-questions.json | cq-bank/ 新增条目（附 SPARQL 化） |
| attempt 验收（lesson_accepted / checkpoint） | commit 的 --attempt 标识 |
| curriculum-ontology/L1 候选目录 | workspace 本身（版本谱系取代散落候选） |
| S17+ 检查收据 / practice-log | 暂不进本体；作为 PROV 来源引用 |

迁移路径：以现有 141 类合并本体做 `init` 基线（attempt=rebaseline），
之后每完成一课跑一圈循环；S15 之后"本体积累中断"的问题由 regress 强制暴露
——任何一课不产 delta、不加 CQ，谱系上就是一目了然的一版空转。

## 边界

- 本循环管"结构与谱系"，不管"内容对不对"——delta 里论断的真实性仍由
  实践证据（视频段、检查收据、评审）负责，进循环前先过各自领域的验收；
- 单调语义保证新增不撤销旧结论；需要撤回时走 removes + 判决，不走覆盖；
- workspace 是受控资产：不要手改 versions/ 与 changelog，改了 checksum 对不上。
