# 第10章：支撑过程——五张绿灯为什么拼不成一次放行

本章不再按 Part 8 条款目录讲六个互不相干的过程。正文用同一个合成变更
`CR-0412` 贯穿 DIA、需求、验证、配置/变更、工具和复用：需求
`TSR_TorquePlausibility` 从 v1.7 变为 v1.8，发布候选从 `B-2026-03` 走向
`B-2026-04`，五份真实的旧绿材料却没有共同指向同一责任、对象、版本、usage 和决定时点。

`SUP-S10` 只是书稿层的候选状态摘要和章节接口记号，不是当前 RDF 实体、配置库基线或项目批准。
本章所有项目名、版本、参数、运行历史和裁决均为合成教学数据。

## 当前范围

- Part 8 Clause 5–10 已按本章问题链教材化吸收：分布式开发接口、安全需求规格、配置管理、变更
  管理、验证和文档管理。教材化吸收不等于这些项目过程已经执行，也不等于相应对象已经进入 RDF。
- Clause 11 从适用性、usage、TI/TD/TCL 走到 Table 4/5 方法选择；现有机器资产只覆盖其中的
  usage/评估结构、Table 3 和 Table 4/5 受控矩阵，不覆盖资格计划、执行和报告。
- Clause 12–14 及 SEooC 只用于区分复用对象和证据路线。`FixedPointMath 3.1.2` 没有任何已完成
  的组件资格、SEooC 集成或 PiU 工程主张。
- Part 10 §9、§10、§13 只作资料性解释，不产生新的规范义务。规范结论回到 Part 8。
- 选定来源已重读并同步到 `SOURCE-AUDIT.md` 与来源账；专家复核、文本哈希、权利处置和出版批准
  仍未完成。

## 本章交付物

| 产物 | 当前职责 |
|---|---|
| `chapter.md` | 九问连续正文；同一 `CR-0412` 从放行冲突走到窄裁决和 BMS 冷迁移 |
| `SOURCE-AUDIT.md` | Part 8 Clause 5–14、Part 10 §9/§10/§13 的来源坐标、强度和开放边界 |
| `examples/tool-tcl-decision.txt` | TI/TD/TCL 的离线判定示例 |
| `examples/tool-qualification.md` | Table 4/5 方法画像、§4.3 组合语义和资格记录字段 |
| `../../ontology/tcl-table3.ttl` | Table 3 的六个 TI×TD→TCL 单元 |
| `../../ontology/tool-qualification-tables.ttl` | 2 表、8 个方法条目、32 个 ASIL 推荐单元 |
| `../../ontology/abox-eps-tools.ttl` | 两个明确标为 TeachingExample 的 usage 级工具评价 |
| `../../ontology/source-anchors-part8.ttl` | 当前 Part 8 受控来源锚；不能代表全章所有 prose-only 来源都已对象化 |
| `../../ontology/shapes.shacl.ttl` | usage、工具评价、TCL 映射和方法表的局部闭世界门禁 |
| `../../eval/eval-cases.yaml` | `CQ-CH10-01..03` 与 `GATE-CH10-01..04` 的精确 oracle |
| `../../eval/fixtures/` | Table 3/4/5 与工具评价的单因正负反例 |

## 机器合同的精确宽度

当前可执行资产能够核对：

1. Table 3 是否精确恢复六个 TI×TD→TCL 单元；
2. 两个教学工具评价是否各自绑定具体 usage、TI、TD 和 TCL；
3. TI2×TD3 是否被错标成非 TCL3，或映射表是否缺失、重复、越界；
4. Table 4/5 是否保持 2 张 TCL 专属表、8 个方法条目和 32 个推荐单元；
5. `CQ-CH10-03` 所登记的窄问题：TCL3/TCL2 两张表在 ASIL D 下哪些方法为 `++`。

底层 32 个推荐单元可以支持其他查询，不表示任意 TCL×ASIL 查询都已有注册 oracle。所有 ch10
CQ 的 `expert_review_status` 仍为 `pending`。

当前机器不能证明：

- §11.4.1 的工具清单完整或适用性筛选正确；
- rationale 的工程真实性；
- §11.4.2–.4 的有效性、使用符合性和策划已经完成；
- 资格方法已经选择、执行并形成 §11.4.6.2 记录；
- DIA、需求质量、Clause 9 计划—规格—执行—报告、基线/变更链或文档身份完整；
- Clause 12/13/14、SEooC 或五张材料共同绑定 `B-2026-04`；
- 任何现实工具、组件、产品或发布符合 ISO 26262。

`SoftwareToolUsageDescriptionShape` 当前只强制工具、目的、输入和输出。§11.4.5.1 c) 的步骤、
环境约束和功能约束带“适用时”边界；未来若采用“有值或显式 N/A+理由”门禁，那是本书的局部闭
世界策略，不能改写成 ISO 无条件字段要求。

## Table 3 快速参照

| 工具影响 | TD1 | TD2 | TD3 |
|---|---|---|---|
| TI1 | TCL1 | TCL1 | TCL1 |
| TI2 | TCL1 | TCL2 | TCL3 |

顺序不可倒置：先固定 usage 并判断 Clause 11 是否适用；仍适用时评 TI/TD；选择不明时的保守估计
是 §11.4.5.3 的 should；确定取值后按 Table 3 得到 TCL 是 §11.4.5.4 的 shall。TCL1 只表示
§11.4.6.1 不要求 qualification methods，不是 Clause 11 其他义务的豁免，也不是工具品质等级。

## 本体化实践入口

正文 10.8 用一条完整链解释当前门禁：

```text
source anchor → TBox → ABox → Shape → SPARQL → exact oracle → single-factor fixture
```

尚未落盘的全章最小关系设计是：

```text
CR-0412 → old/new baseline → requirement version → verification execution/report
        → tool usage/evaluation/qualification status → reuse candidate/route → DIA responsibility
```

这只是后续对象化设计目标。不要用虚构三元组补齐它，也不要先建立一个会把未建模现实当成缺失的
`ReleaseReadyShape`。

## 开发检查

```bash
.venv/bin/python eval/check_coverage.py
python3 eval/check_outline_contract.py
.venv/bin/python eval/run_eval.py
python3 -m unittest eval.test_chapter_maturity
```

`Table_9_C_1` 已补齐 Part 9 唯一所有权并通过定向回归；230 项单元测试已在分段扫描与两项定向
复跑中全部通过，其中 pySHACL 嵌套查询耗时 106.8 s。完整 `run_eval.py` 仍没有同一快照的通过
或发布报告，因此不能把单元测试结果表述为发布门禁全绿。
发布成熟度以 `publication/chapter-maturity.yaml` 为准；ch10 仍应保持 `unassessed`，直至专家、
权利、来源文本哈希、最终 PDF 和用户验收均真正关闭。
