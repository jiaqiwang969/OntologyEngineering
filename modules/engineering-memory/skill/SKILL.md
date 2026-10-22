---
name: engineering-memory
description: Use the integrated engineering memory module to recall, verify, curate and hand off source-bound Codex and Claude experience within the current engineering Project. Preserve chronology, corrections, conditions, causal uncertainty and actual-use feedback.
metadata:
  version: "2.0.0-rc.6"
---

# 工程记忆模块

通过统一入口 `oev2 memory` 使用本模块。完整安装、来源和接收方式见 [模块说明](../README.md)。本文件属于工程本体系统，不依赖另一份全局 memory Skill 或历史工作区。

用户用自然语言表达目标；Agent 维护合同和来源，不要求用户填写结构化记录。

1. 明确任务、条件和当前 Project。Project 从实际任务取得，不能从 cwd、profile 或历史候选推断。无项目任务可以不绑定，但不能据此写工程采用。
2. 使用 `session prepare` 与 `session activate` 选定当前任务上下文。读取候选时保留完整时间线、纠正、失败、未知与适用条件；候选曝光不是使用。
3. 关键决定前用 `session verify` 检查当前 Project、最新修订与来源。用 `get` 查看的是历史记录，不代表它仍适用；被排除的候选或未读窗口不能补成已读。
4. 缺少经验时使用 `history-search` 查锚点，使用 `evidence` 读指定原窗口。需要建立索引时明确原件根与预算。不能把搜索命中数当作全历史阅读数量。
5. 使用 `record` 或 `resolve` 整理经验。保留作者声称与实际观察的区别；纠正追加新修订，不改写原文和旧回执。因果关系证据不足时写为未知或假设。来源必须能逐字节复核。
6. 需要工程采用时，通过 `session export` 或原生 `session v2-prepare` 回交同一 Project。工程采用由当前 Project 的证据和审查决定。Project 变化后重新 prepare/activate，再执行 `session v2-verify`。
7. 阶段结束使用 `session feedback` 记录实际使用、拒绝或未用及其依据。改进收益需要独立对照，不能把模型答复或来源哈希一致当作收益证明。

`organization` 可组织显式的 Agent 审阅与确定性程序工作。内部方法版本、独立命题和验收依据要先固定；保留未知和失败，不放宽判据取得通过。模块不派发模型，不安装后台定时器；需要的专业工具由宿主按任务调用。

私人 session、记忆库、hook 政策和项目记录放在私有数据目录，不提交到 GitHub。配置、路径和原件可访问性以当前安装为准。
