# 第5章：知识表示与推理机制 / Chapter 5: Knowledge Representation and Reasoning

## 本章内容

本章深入介绍本体推理机制，包括：
- 前向推理与后向推理策略
- 描述逻辑 Tableau 算法
- SWRL 规则（Semantic Web Rule Language）
- 时态推理与具名图（Named Graph）
- 概率本体推理
- 推理性能优化

## 文件说明

| 文件 | 内容 |
|------|------|
| `swrl-rules.swrl` | SWRL 规则示例（设备调度、工艺推荐） |
| `description-logic-examples.txt` | 描述逻辑推理示例（Tableau算法） |
| `temporal-reasoning.txt` | 时态推理与具名图示例 |
| `probabilistic-reasoning.txt` | 概率本体推理示例 |

## 推理类型对比

| 推理类型 | 机制 | 适用场景 |
|----------|------|----------|
| 前向推理 | 从已知事实推导新结论 | 实时推理、数据流 |
| 后向推理 | 从目标反向查找证据 | 诊断、问题求解 |
| Tableau | 基于描述逻辑的可满足性检测 | 一致性检查 |
| SWRL | 基于规则的推导 | 业务规则表达 |

## SWRL 语法说明

```
前提(Antecedent) → 结论(Consequent)
多个原子用 ∧ (AND) 连接
变量用 ?variableName 表示
```
