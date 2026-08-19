# 产品可信工程：从规范、本体到证据闭环

> 贯穿样板：ISO 26262:2018 汽车功能安全
> 架构基线：本目录只保存可读书稿；Semantica 是全书唯一机器可执行语义。

本书讨论怎样把“产品值得相信”写成有主语、有版本、有情境、有证据射程、
有反证入口、有决定者、也有重开条件的工程判断。前十章从功能安全现场提出问题，
后十章用工程本体逐一回答；术语、方法、合成案例、反例故事与工程规则仍是书的
内容。它们不会因为执行架构迁移而被删去。

## 书是规范，Semantica 是唯一执行语义

本书与 Semantica 不是两套实现，也不是“主实现 + 备用实现”。边界固定如下：

| 层 | 唯一权威 | 保存什么 |
|---|---|---|
| 可读规范层 | 本目录 | 两卷书的论述、术语解释、方法、案例叙事、图、来源说明与能力问题的自然语言意图 |
| 可执行语义层 | Semantica built-in packages | 本体、CQ 注册表、SPARQL、SHACL、正反案例、工程规则、exact oracle、manifest、版本、PROV、receipt 与 release verdict |
| 标准事实层 | 用户合法持有的受控 ISO 来源 | 原始条文、表、图及精确坐标；不随公共书稿或 Semantica 包分发 |
| 决定层 | 有权的工程人员和组织 | 技术充分性、风险接受、合规、发布、制造或现场行动授权 |

因此，本目录不再保留可执行 RDF/OWL、查询、Shape、fixture、runner 或后备
runtime。旧书内路径只作为迁移来源写入 Semantica 的哈希账本；不能再被调用。
Semantica 不可用、包未知、资产哈希不符或所需能力不受支持时，执行必须阻断，
不得退回书内副本。

Semantica 当前登记 20 个本卷 chapter packages，另有一个规范转述 domain package。
所有章包 manifest 当前均声明 `status=partial`、`release_status=blocked`。这意味着：
声明的场景 oracle 即使通过，也只能证明那个受限机器合同；它与发布准备度是两个
独立状态。不得把 `partial`、`blocked`、占位内容、未运行检查或 unsupported 推理
解释成绿色。

## 20 章的一一绑定

| 书章 | 主题 | Semantica package id |
|---|---|---|
| ch01 | 为什么全绿不等于产品可信 | `semantica.chapter_packages.vol2.ch01` |
| ch02 | 概念与术语 | `semantica.chapter_packages.vol2.ch02` |
| ch03 | 安全管理与决定权 | `semantica.chapter_packages.vol2.ch03` |
| ch04 | 概念阶段与 HARA | `semantica.chapter_packages.vol2.ch04` |
| ch05 | 系统级产品开发 | `semantica.chapter_packages.vol2.ch05` |
| ch06 | 硬件级产品开发 | `semantica.chapter_packages.vol2.ch06` |
| ch07 | 软件级产品开发 | `semantica.chapter_packages.vol2.ch07` |
| ch08 | ASIL 分解与 DFA | `semantica.chapter_packages.vol2.ch08` |
| ch09 | 生产、运行、服务与退役 | `semantica.chapter_packages.vol2.ch09` |
| ch10 | 支持过程与发布保证 | `semantica.chapter_packages.vol2.ch10` |
| ch11 | 可信主张本体 | `semantica.chapter_packages.vol2.ch11` |
| ch12 | 工程对象与同一本体 | `semantica.chapter_packages.vol2.ch12` |
| ch13 | 工程治理本体 | `semantica.chapter_packages.vol2.ch13` |
| ch14 | 情境与目标本体 | `semantica.chapter_packages.vol2.ch14` |
| ch15 | 需求与架构本体 | `semantica.chapter_packages.vol2.ch15` |
| ch16 | 测量证据本体 | `semantica.chapter_packages.vol2.ch16` |
| ch17 | 配置与变化本体 | `semantica.chapter_packages.vol2.ch17` |
| ch18 | 依赖与韧性本体 | `semantica.chapter_packages.vol2.ch18` |
| ch19 | 产品数字线程本体 | `semantica.chapter_packages.vol2.ch19` |
| ch20 | 持续保证决定本体 | `semantica.chapter_packages.vol2.ch20` |

前十章与后十章仍保持 10+10 镜像关系：ch01↔ch11、ch02↔ch12，依此类推。
镜像说明问题与回答，不允许跨包偷用业务图。每章执行只从其 allowlisted package
读取资产；跨章综合必须显式记录输入包、版本与映射，不能把兄弟章图静默合并。

## 怎样复核一章

读者先读本章正文，再从源锁定的 Semantica build 查看、运行和验证对应包：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
runtime/.venv/bin/python scripts/semantic_engagement.py run \
  --binding /path/to/package-binding.json \
  --task /path/to/task-envelope.json \
  --scenario semantica.vol2.ch14.scenario.primary
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding /path/to/workspace-binding.json \
  --task /path/to/task-envelope.json \
  --workspace /path/to/semantica-managed-registry
```

binding 与 task fixture 先按
[`semantic-engagement-contract.md`](../semantic-engagement-contract.md) 绑定 `discover`
返回的精确 package/version/digest；统一入口自动核验并注入 source identity。
`propose/commit/verify/history/promote` 引用同一合同定义的 delta、candidate 与 gate-evidence
fixtures，并以各子命令 `--help` 为准。原生 `semantica package show/run/verify` 仅供底层
runner/manifest 诊断，不是主运行路径。

报告时至少分开四件事：书中依据；package/scenario id；场景 oracle 状态；独立
release verdict。Python、CLI 与 MCP 都只能是同一个 `SemanticPackageRunner` 的
适配面，不得各自实现另一套执行逻辑。

当前 Semantica 支持 RDF Dataset、SPARQL query/update、SHACL、受限正向单调规则、
snapshot/diff、PROV 与 receipt/release 绑定。这不等于完整 DL/tableau 推理、任意
SWRL built-in、非单调/默认、时态或概率推理已经支持；超出声明能力的请求必须
fail closed。

## ISO 26262 的权威边界

`semantica.chapter_packages.vol2.normative` 保存本书**已登记范围内**的工程释义、
术语/模态映射、来源坐标和教学案例；当前资产主要来自 Part 1 与 Part 3，不覆盖
Part 11/12 或 28 张方法表的全部语义。它的主场景是
`semantica.vol2.normative.scenario.modality-fidelity`。该包同样是 `partial/blocked`，
而且不是 ISO 原文副本、官方解释、合规意见或认证证据。精确条文与表格只能回到
用户合法持有的受控来源核对。书中 EPS、ENV-01、人物、事故和数值均为合成教学
材料，机器通过不能把它们升级为现实产品事实。

## AI、事实与决定

全书的人机顺序不变，但执行落点已经统一：

```text
自然语言问题 → 书章中的概念与方法 → Semantica chapter package
  → 归一化输入 → CQ / query / SHACL / supported rules → oracle
  → snapshot / PROV / receipt → 独立 release verdict
  → 原生事实回读与人工工程评审 → 有权主体决定
```

LLM 可以翻译语言与意图，不能补写 Unknown；Agent 可以提出候选动作，不能因
本体一致就获得写权限；`conforms` 不授予发布、制造或现场变更权限。本体负责语义
权威，受控设计、仪器和工程系统负责事实权威，人和组织负责决定权威。

本书最终训练的不是“把所有检查做绿”，而是让每句可信主张都能回答：相信什么，
依据什么，相信到哪里，谁承担决定，以及哪些变化会迫使它重新接受质疑。
