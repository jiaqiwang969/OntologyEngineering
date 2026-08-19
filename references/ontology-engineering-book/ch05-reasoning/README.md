# 第5章：知识表示与推理机制

## 本章任务

本章比较前向/后向规则、描述逻辑 tableau、SWRL、时态与概率推理，重点训练“先声明
推理 profile，再陈述结论”。书中可以完整解释这些理论；当前 Semantica 只对受限的
正向、单调规则给出原生执行承诺，其他 profile 不因资产存在而变成已支持。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch05`
- package status：`partial`
- release status：`blocked`
- payload：CQ/case `native`；rules `adapter`；ontology/shapes/SPARQL `absent`
- 可复算场景：`OE-V1-CH05-SCN-FORWARD-CHAIN-001`
- exact oracle：从预计算 `PowerAbove10(Lathe_003)` 等事实经受限规则推出
  `HighPowerEquipment(Lathe_003)` 与 `RequiresCooling(Lathe_003)`
- fail-closed 边界：一般 SWRL 与 built-ins、DL tableau、时态、概率、默认与非单调推理

`swrl-rules`、`description-logic-examples`、`temporal-reasoning`、
`probabilistic-reasoning` 均保留在包中作为教学/来源资产。保留不等于解析，更不等于执行；
manifest 中 role、format、scenario binding 与 capability 必须同时支持，结论才可复算。

## 推理类型对比

| 类型 | 书中讲解 | 当前包执行状态 |
|---|---|---|
| 受限正向链 | 从事实和正 Horn 式规则推出新事实 | adapter 场景可复算 |
| 描述逻辑 tableau | 可满足性、一致性、分类 | 理论/案例，runtime 不承诺完整 DL |
| SWRL | OWL 个体上的规则表达 | 源资产保留，一般 SWRL/builtin 不执行 |
| 时态推理 | 时间区间、状态演化 | 教学材料，未绑定 oracle |
| 概率推理 | 不确定性与概率本体 | 教学材料，未绑定 oracle |

## 复算

先由受控发布流程把实际 runtime commit 与精确 wheel/工件 SHA-256 分别写入
`SEMANTICA_RUNTIME_COMMIT`、`SEMANTICA_RUNTIME_SHA256`；缺失或错配必须失败关闭。

```bash
semantica package run semantica.chapter_packages.vol1.ch05 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH05-SCN-FORWARD-CHAIN-001 --json
```

不要把这个小型正向链场景表述为 Pellet/HermiT/SWRL 全兼容，也不要据此给整章发布绿灯。
