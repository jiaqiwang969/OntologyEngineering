# 完整工程检查点与有界归档

工程条件、原件和当前检查是正本。当前 `harness step` 只接受显式无模型检查点，`harness consolidate` 只接受已验证的空候选归档；它们不调用模型，不选择工具路线，不授予工程采用。需要额外模型辅助时另用[主模型直接调用](direct-model-assistance.md)，普通模型传输与本页的本地归档预算分别负责。

## 完整当前检查点

Agent 根据自然语言任务整理 `oev2.harness-checkpoint-input/1`，顶层包含 `task_id/context/request/harness`，可选 `external_skills` 仅保存未调度声明；不接受 `questions`。`harness` 绑定同一 Project、对象、条件、来源、正式 review 及选定 previous，并声明：

```json
"checkpoint_policy": {
  "schema": "oev2.harness-checkpoint-policy/1",
  "mode": "verified_no_model",
  "byte_budget": 16777216,
  "reason": "保存当前完整检查和必要过程"
}
```

通过 `oev2 harness step --input ... --output ...` 写入新输出目录。未知协议、额外问题、模型 `context_policy`、凭据材料或越界资源拒绝；不会自动切换策略或裁减必要来源。

当前 Project 原字节另存 `current-project.json`，`semantic-state.json` 保存完整状态，输入、模型前提、反馈、工程检查与正式审查保留其实际结果。previous 绑定已验证检查点的原 iteration 和附件；历史内容不会被改签为当前通过。来源、条件、实现或资源基线变化须按合同说明，当前检查仍按当前身份执行。

终态 `oev2.harness-checkpoint/1` 的 `status=recorded` 仅表示检查点完整保存；`model_execution=not_run`、`model_calls=0`，没有路由候选。检查为 violated/unknown 时工程采用仍阻断。模型、CAD、制造和测量分别需要实际证据。

### 资源范围与完整性

- `byte_budget` 为正整数，最大 16,777,216 字节。计实际存储的每个附件一次，加终态 iteration 的 pretty JSON 与换行；同 SHA 原件可以只存一次，各来源引用仍保留。
- 最多 256 个附件。所有可解析 JSON 附件及 iteration 累计最多 200,000 个 Value 节点，每个 JSON 根深度为 0，最大深度 96；输入另限 1,048,576 有序 JSON 字节。
- 显式 `oev2.harness-checkpoint-policy/2` 仅将累计节点上限扩大到 524,288，其余限制不变。必须说明资源基线变化，不自动升级，不裁来源。
- JSON 结构计量不依赖文件后缀，包括摘要命名原件。首个非空白字符为 `{` 或 `[` 而严格解析失败的原件拒绝，不能以 opaque 标签绕过重复键、深度或损坏检查。其他二进制/文本按原字节保存与计量。

先核完整资源，再最后写 iteration；超限可能留下诊断性部分目录，没有完成回执就不算成功。源预检和终态实际计量分别报告，不能拿估算充作执行证据。

`load_step` 只接受当前检查点协议，重新核对附件摘要、终态、原输入、完整状态、当前检查和 previous 一致性，并拒绝混入路由或旧模型视图产物。旧模型 step 不因留有 JSON 或旧成功回执就成为当前可接续检查点；历史档案保留原身份，需要当前记录时从当前 Project 和真实原件另行建立。

## 空候选归档

已有表达足够、作者没有提出新增候选时，`harness consolidate` 使用 `oev2.consolidation-input/3`。填写 `checkpoint_id`、`problem_class`、`reason`、已验证的 `iterations`，`candidates` 必须为 `[]`，并声明：

```json
"archive_policy": {
  "schema": "oev2.consolidation-archive-policy/1",
  "mode": "verified_empty_checkpoint",
  "byte_budget": 16777216
}
```

不能混入 `context_policy`。当前入口不执行旧非空候选的模型评估协议；需要提出 PackageDelta 时另行组织 Agent 审阅、当前 Project 检查与独立验证，不能借此空归档免审或晋升。

归档预算计每个选定 iteration 原字节和每个附件引用位置的原字节之和，重复引用重复计数，最大 16 MiB。另限 16 条不同轨迹、累计 256 个附件、policy `/1` 累计 200,000 个 JSON Value 节点、每个 JSON 根深度 0 至 96、输入有序 JSON 1 MiB。显式 `oev2.consolidation-archive-policy/2` 仅将节点上限扩大到 524,288，其余范围不变；在本次 reason 说明来源与扩容理由。检查点按实际存储去重的字节口径与归档按引用位置计量的口径不能混用。

所有可解析 JSON 源均计结构，包括没有 `.json` 后缀的原件。解析失败的 JSON 候选，仅在精确 failed-review 路径及摘要绑定成立时作为 opaque 历史失败保留；合法 JSON 即使是失败材料仍计结构。凭据标记、篡改、重复轨迹、问题域不符或超限拒绝。

Rust 核对原检查点及所有原件，再逐字节复制到新私有目录并保存清单。`consolidation.json` 仅在全部写入和回读核验后生成；部分输出不算完成。原路径和原回执字节保留，不宣称移动目录会自动重映射历史绝对路径。

`no_delta` 表示 `author_declaration_only`，`execution_kind=not_run`、`model_calls=0`、`request_bytes=0`。没有模型判断、本体晋升或工程采用，也不重新执行历史工程审查。后续任务仍核当前 Project、条件和来源；不完整的 step 不能通过归档变成完整检查点。

## 后续改进的证据边界

完整记录、可逆表示和摘要匹配只证明相应记录合同。独特大原件不保证在任意预算内可容纳；超限应保留失败并明确组织任务，不能删必要条件、反例、失败或未知取得通过。

新领域概念或方法候选仍须说明可复用缺口、适用边界、反例和非空 PackageDelta，绑定当前 Project 与新任务证据，完成实际 Rust、回归与治理验证。完整 registry 晋升没有由此归档入口证明。评价真实方法收益时保留独立判据、失败、成本和曝光范围，按[RRSI](regularized-evolution.md)接续。
