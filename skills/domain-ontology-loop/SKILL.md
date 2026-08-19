---
name: domain-ontology-loop
description: Govern the slow outer loop that turns a reusable candidate from the root ontology-engineering semantic engagement into an immutable, regression-verified and release-complete Semantica industry package. Use after engineering practice yields a non-empty PackageDelta; when reviewing conflicts, removals, regressions, release evidence or promotion; or when auditing why a domain ontology stopped growing. Use Semantica's native ontology refinery and managed CAS/registry only. Do not use for routine read-only engagements, no_delta results, one-shot vocabulary generation, or external publication.
---

# Domain Ontology Loop：Semantica 行业本体慢速治理外循环

把本 skill 作为根 `ontology-engineering` 的慢速外循环，不作为独立本体工具。根 skill
在每次工程任务中完成快速语义接入、现有 package 验证和学习判定；只有结果为有复用
价值的 `candidate` 时，才进入这里做治理、回归、release 和 promotion。

两卷书只提供方法镜头：第一卷说明怎样建模与验收，第二卷示范怎样把 ISO 工程知识
本体化。项目事实来自受控工程活动，唯一可执行语义和行业记忆都在 Semantica；有权人
掌握冲突、替换、删除、风险、合规、提升和发布决定。

## 先决条件

1. 从本文件位置向上解析根 `ontology-engineering` 目录，记为 `OE_SKILL_ROOT`。不要
   依赖当前工作目录，不要写死用户主目录或某个 Semantica checkout。
2. 读取根 `references/semantic-engagement-contract.md` 和本目录的
   `references/loop-contract.md`。
3. 使用根 skill 的 source-locked 入口完成 doctor、capability discovery、task
   envelope、project binding 和 engagement receipt。runtime commit、version 与 wheel
   SHA-256 必须由根 source lock 自动注入；缺失、手抄或与安装 wheel 不一致时 fail closed。
4. 根 binding 要求 `semantic_api=semantica.ontology.refinery/v1`；适配器把它投影成 native
   `semantic_api_contract`。原生 refinery capability 必须完整，不得把旧 governance
   lifecycle 当成 fallback。
5. 根响应 `learning.verdict=no_delta` 时记录理由并返回快速内环；不得制造空 candidate
   或空版本（native receipt 内部对应 `learning.status=no_delta`）。

可移植入口形式如下；先由 Agent 从本文件位置解析 `OE_SKILL_ROOT`：

```bash
"$OE_SKILL_ROOT/runtime/.venv/bin/python" \
  "$OE_SKILL_ROOT/scripts/semantic_engagement.py" doctor
"$OE_SKILL_ROOT/runtime/.venv/bin/python" \
  -m semantica.ontology.refinery --help
```

不要调用本子技能中旧的兼容性 `scripts/internalize.py` 来创建平行正本，也不要根据其
旧文件名猜测工作区结构。

## 慢速外循环

严格按不可跳级状态推进：

```text
candidate → proposed → committed → regression_passed
          → release_complete → promoted
```

- `candidate`：保存 exact `PackageDelta`、task envelope、binding 和 engagement receipt；
  它不是行业事实。
- `proposed`：写入不可变 candidate/proposed ledger 并完成差异、影响、冲突、替换和删除
  分析；它仍不改变 promoted registry truth。
- `committed`：有权人对 exact delta 授权后，物化不可变候选 package。
- `regression_passed`：旧 CQ、新 CQ、positive、negative、ambiguity 和 prior-release
  回归均由内容绑定证据证明通过。
- `release_complete`：完整 package coverage、capability、receipt、PROV、来源、权利和
  release checks 闭合；不等于已提升。
- `promoted`：有独立 promotion 授权后，将不可变 package version 加入 Semantica
  registry；不得覆盖已有版本。

每次写入都必须来自当前根 OE 调用的 task。根适配器为 `candidate`、`proposed`、
`committed`、`execute_candidate`、两类 gate 推导、`regression_passed`、
`release_complete` 和 `promoted` 分别生成 exact 单 action context；Semantica 将其 hash
写入 CAS 与相应事件/执行套件/门禁/晋升记录。早先 candidate task 不是未来迁移授权。

`published` 不属于 refinery 状态机，也没有 refinery publish 命令。它是外部权利人或
发布机构的动作，必须单独报告。

## 完整 PackageDelta

使用原生 `PackageDelta`，显式携带八类资产数组和独立的书稿影响字段：

```text
ontology · competency_questions · shapes · queries · rules
cases · contract · provenance · book_impact
```

每项资产都是 `add|replace|remove` 的内容寻址变更；replace/remove 必须绑定被替代内容
的 SHA-256。`cases` 至少覆盖 `positive / negative / ambiguity / prior_release`；适用时
把 negative 设计为单故障反例。`book_impact` 是独立顶层枚举，不得藏进 provenance；
即使影响为 `none`，也必须显式声明，并在 delta rationale 中说明理由。

八个资产数组可以只声明相对 bound baseline 的真实变化，但不能省略字段或用空数组
暗示“已经验证无影响”。最终 materialized package 在 `release_complete` 前必须覆盖全部
八类资产和四类 case；`book_impact` 同时通过 enum、delta SHA、manifest 和 promotion
record 验证。项目实例通常保留为 evidence 或 case 输入，不自动提升为行业 ABox。

## 工作区与不可变性

只让 `semantica.ontology.refinery` 创建和管理工作区。它使用 `workspace.json`、
`registry.json`、`objects/sha256/` CAS、content-addressed refinements、不可变事件链、
hash-keyed package versions 和 append-only registry events。package ID 是 opaque registry
key，不是路径。

禁止手改 CAS、events、candidate documents、package records 或 registry。`init` 拒绝覆盖
已存在工作区；hash mismatch、stale baseline、已有目标 version、断链或残留锁都应阻断。

## 授权边界

允许自动执行只读 status/history/list/resolve、candidate 留存、proposal 分析和门禁计算。
以下动作不能因进程可执行而视为已获授权：

- `commit`：必须有 exact delta-bound 的 commit authorization；冲突、replace/remove 的
  判决和理由由 binding 中的 decision authority 提供。
- 风险、合规、事实接受和产品放行：由相应有权人决定，并作为 release evidence；
  Semantica 只验证合同与证据绑定。
- `promote`：必须有独立的 promote authorization，且 actor/authority/target 与 binding
  一致；`target` 固定为 registry channel `industry-registry`，不是 package ID。
- `verify`：不接受调用方撰写的 regression/release pass JSON；两类证据只能由 Semantica
  执行 exact committed subject 后从同一 execution suite 内部推导。
- 书稿修改、push 和 `published`：均为外部动作，各自需要明确授权。

Promotion 响应只产生 `auto_applied=false` 的 successor binding 投影。批准、另存和切换
新 binding 是外部控制面动作；旧 binding 不得原地改写。

## 结果报告

不要把退出码 0、`candidate`、`committed`、一个 CQ 通过或一份 receipt 当成全部绿色。
通过根入口时分别报告实际 section，不另造平行 alias：

```text
execution.status · regression.status · receipt.status · release.status
learning.status · learning.verdict · learning.promotion.status
learning.publication.status / decision
```

同时给出 `delta_sha256`、当前 state、package/version/package SHA、event/receipt/PROV
哈希、缺失 capability、失败 check、授权主体和 blocker。CLI 的 exit 0 只表示该命令成功
返回；gate blocked、输入/工作区错误和外部 publication 必须按各自状态解释。

## 门禁

修改本 skill 或合同后，至少执行根仓库的 retrieval eval、held-out eval、strict backend
policy 和测试；再检查本文与 refinery capability/CLI 是否一致。任何旧单文件工作区描述、
硬编码本机路径、隐藏的第二 backend 或 governance fallback 都是失败。
