# 第1章：绪论——从哲学本体论到工程本体论

## 本章任务

本章回答三个问题：本体论从哪里来，工程本体论把什么变成可计算的共同约定，
以及 AI 时代为什么需要一个与统计生成分离的知识与约束层。Gruber 的“概念化的
显式规范”和 Studer 等人的“形式化、显式、共享”仍是理论起点；工程化则要求继续
给出范围、能力问题、可判定边界、执行证据和责任边界。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch01`
- package status：`partial`
- release status：`blocked`
- 已迁入：章合同、CQ 注册表、哲学—工程转向案例、AI 时代案例、全书路线图、场景注册表
- 未完成：可执行 conceptualization manifest、范围边界检查、正例与缺边界单因反例、
  内容绑定执行收据

本章的三份材料在书中作为阅读节选出现，其可执行/可检索正本是包内 asset
`philosophy-to-engineering`、`ontology-in-ai-era` 与 `book-roadmap`。
书目录不再保留迁移前的配套资产副本。

## 核心区分

| 维度 | 哲学本体论 | 工程本体论 |
|---|---|---|
| 研究对象 | 存在、范畴与世界的根本结构 | 某个共同体对领域概念、关系、规则和边界的承诺 |
| 主要方法 | 思辨、论证、范畴分析 | 需求化、形式化、实现、验证、版本化与治理 |
| 可检验产物 | 哲学论证体系 | 书中规格 + Semantica 包、场景、oracle 与 receipt |
| 权威边界 | 哲学立场 | 语义权威不替代事实权威和决策权威 |

“低熵”不是说世界本身没有歧义，而是说建模共同体把必要歧义显式化，并对机器可做
与不可做之事给出边界。当前 ch01 包尚不能自动判定一份范围声明是否完整，因此
本章练习仍是方法训练，不能声称已被 runner 验收。

## 统一语义介入入口

从 ontology-engineering skill 根运行。先只读发现 source-locked registry，再按
[`semantic-engagement-contract.md`](../../semantic-engagement-contract.md) 建立精确
package binding、workspace binding 与 task envelope：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
runtime/.venv/bin/python scripts/semantic_engagement.py run \
  --binding /path/to/package-binding.json \
  --task /path/to/task-envelope.json \
  --scenario OE-V1-CH01-SCN-001
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding /path/to/workspace-binding.json \
  --task /path/to/task-envelope.json \
  --workspace /path/to/semantica-managed-registry
```

此包当前没有可运行的完整场景；注册项 `OE-V1-CH01-SCN-001` 的状态明确为
`absent`，因此 `run` 必须返回可解释 blocker；`open` 只建立受管的行业本体学习回路，
不自动改动正式 package。书提供方法，受控工程记录提供事实，Semantica 是唯一可执行
语义；事实接受、风险、晋升与发布仍由有权人决定。原生 `semantica package ...` 只供
底层 runner/manifest 诊断，不是本章主运行路径。

## 学习建议

- 零基础读者：按第1→2→3→4章建立概念、逻辑、方法与语言的连续链。
- 有知识图谱经验者：可从第3章 CQ 与验收切入，再回补第2章开放世界语义。
- 关注 AI/Agent 者：先读本章与第8章，但必须回到第4、5、7章理解查询、推理和形状门禁。
