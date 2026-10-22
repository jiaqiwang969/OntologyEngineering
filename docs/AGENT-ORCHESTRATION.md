# 当前会话、工程执行与经验回流

用户说明目标、对象、条件和验收要求，当前 Codex／Claude 等 Agent 负责理解要求、维护同一 Project、选择工具并组织审阅。Rust 执行来源、合同与工程检查；内置 engineering-memory 保存和召回有来源的经验。核心流程不需要专用判断服务，也不需要另装一份工程记忆 Skill。空记忆库可以直接开始，是否召回历史经验按当前任务决定。

工程本体论规定含义，ggen 生成合同，Rust Semantica 与工程模块执行，RRSI 根据真实反馈检验改进。Agent 的解释、模型回答、工具成功和工程采用分别记录；任何一项都不能代替完整工程验收。

## 同一 Project 内推进任务

1. 依据自然语言要求整理目标、对象修订、条件、来源、决定和完整验收，明确当前证据缺口。用户无需代填内部 JSON 或规范文件。
2. 需要历史经验时使用 `oev2 memory session prepare/handoff`，经 `project memory-candidate verify/register` 接续。经验只是待核候选，不能直接支持工程决定。
3. `knowledge query` 和 `project work` 在当前版本执行本地来源查证与已声明的计算。返回片段、计算或缺项后，Agent 回读原件，审阅内容支持与适用性；本地检索命中不意味着来源适用或工程通过。
4. 按需要调用[专业工具](TOOL-INTEGRATIONS.md)，保留输入、对象与工具版本、原始输出、执行状态、覆盖和未知，经 `project tool-result` 回到原 Project。需要专业原生执行时沿对应工具合同，不能用另一个工具的成功替代。
5. 根据新证据更新决定，执行当前 Project 的审查与采用合同。条件或对象变化时重审受影响决定，保留旧记录和失败身份。
6. 经 `project memory-feedback` 与 `oev2 memory session feedback` 保存实际使用反馈。曝光、未使用、无帮助和返工照实记录；需要改进方法时进入[有证据的改进](../references/regularized-evolution.md)。

## 并行与依赖

Agent 可以拆分并行的来源调查、源码定位和独立验证。每项子任务必须带有当前目标、对象版本、条件、缺口、必要来源与验收；有依赖的动作等待前置结果。失败、未读原件和未知条件不能作为已满足的前提。

汇总时保留每组来源、候选、矛盾、未知和检查义务。多个 Agent 复述同一来源不会增加独立证据，多数意见不能替代验证。并行任务的副作用仍受原授权范围约束，不允许各自独立覆盖 Project。新一轮按实际回读结果组织，明确预算与停止条件，不自动建立无限后台批次。

## 检查点与可选模型辅助

`oev2 harness step` 当前接受显式无模型检查点输入，记录完整当前 Project、检查与来源；`harness consolidate` 只接受空候选的完整归档。记录完成和 `no_delta` 均不授予工程采用。具体协议、资源范围与失败处理见[完整检查点与归档](../references/harness-context.md)。旧模型路由、文字/图片判断及后台判断命令不属于当前入口，历史回执不能为新版本补签通过。

普通 chat/embedding 能力仍使用显式选定的 `oe-model-credentials` 传输与预算合同。长资料或复杂推导确需额外模型时，可使用[主模型直接调用](../references/direct-model-assistance.md)的 `project model-assistance prepare/run`；仅显式 `--live` 才派发。它是有来源的辅助材料，Agent 继续核对原件、专业证据和必要工程检查。没有该服务或凭据时，当前会话仍可组织 Project、工具和经验回流。

评估方法时冻结输入、判据与可比基线，开发、训练和独立验证资料分开。保留失败、未知用量、总成本和未执行范围；吞吐、结构完整、保存经验或安装成功不证明工程质量或迁移收益。
