# 第8章：本体与大语言模型

## 本章任务

本章把 LLM 放在受控语义回路中：自然语言请求 → 规范化提议 → 只读语义检查 →
有权限的执行器 → PROV/receipt。GraphRAG、Text2SPARQL 和 ontology-guided Agent
不是让模型获得事实权威或操作权限，而是让候选输出经过可复算约束并留下证据链。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch08`
- package status：`partial`
- release status：`blocked`
- 已迁入：幻觉控制规则、GraphRAG/Text2SPARQL 案例、legacy Agent 参考案例、CQ、
  合同与场景注册表
- 场景：`OE-V1-CH08-SCN-GUARDRAIL-001`，status `adapter`
- runner 状态：`blocked_unsupported_operation_contract`
- 缺口：受 schema/CQ 约束的 Text2SPARQL 与 Unknown 门禁；模型、prompt、ontology
  snapshot、工具动作和授权的端到端合同；内容绑定执行收据

包内 `ontology-guided-agent` 只是 `legacy_case_reference`。书中的三种预期 verdict 是
参考 oracle，不是当前 Semantica 已执行的动作路由结果；未绑定操作必须 fail closed。

## 四种融合模式

| 模式 | 模型负责 | Semantica/受控系统负责 |
|---|---|---|
| 生成前约束 | 理解上下文 | 提供允许的词汇、CQ、schema 与版本快照 |
| Text2SPARQL | 生成候选意图/查询 | 解析、allowlist、资源预算、只读执行与结果类型检查 |
| GraphRAG | 表达与归纳 | 按来源和图结构检索、裁剪并绑定证据 |
| Agent | 提出动作 | 语义校验、授权、执行、补偿与审计；四者不能合并为模型权限 |

## 统一语义介入入口

从 ontology-engineering skill 根运行。先只读发现，再按
[`semantic-engagement-contract.md`](../../semantic-engagement-contract.md) 建立精确
package/workspace binding 与 task envelope：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
runtime/.venv/bin/python scripts/semantic_engagement.py run \
  --binding /path/to/package-binding.json \
  --task /path/to/task-envelope.json \
  --scenario OE-V1-CH08-SCN-GUARDRAIL-001
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding /path/to/workspace-binding.json \
  --task /path/to/task-envelope.json \
  --workspace /path/to/semantica-managed-registry
```

不要直接运行书中历史 `ontology-guided-agent.py` 并把输出当成发布证据。只有绑定输入、
版本、oracle、工具权限和 receipt 的 Semantica 场景才能支撑可执行主张。当前 adapter
操作合同不受支持，因此 `run` 必须保留 blocker；`open` 不会把模型提议变成授权动作。
书提供 Agent 控制方法，受控工程记录提供事实，Semantica 是唯一可执行语义；工具动作、
风险、晋升和发布仍由有权人决定。原生 `semantica package ...` 只供底层
runner/manifest 诊断，不是本章主运行路径。
