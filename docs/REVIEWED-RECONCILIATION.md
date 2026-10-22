# 变更影响与当前复核的接续

`project reconcile`保留原合同：任何对象、条件、来源、依赖或采用状态变化都可能触发重审。原始`stale_adoptions`不会被后来的通过覆盖。已有`project adopt`和`verify-adoption`另行检查独立本地批准、当前策略及来源；本入口不替代它们。

显式`project reconcile-reviewed --input var/request.json --output var/new-result.json`接收生成的`ReviewedReconciliationRequest`：

- `schema`为`oev2.reviewed-reconciliation-request/1`。
- `before`和`current`为完整Project原件的`ArtifactReference`，保持同一项目身份。
- `sources`提供当前Project全部证据摘要对应的原件路径；按路径去重，同一摘要可对应多个证据ID。
- `reviews`是`DecisionReviewReplay`列表，每项指定受影响的`decision_id`、原生`binding`、RDF `evidence`和已保存`receipt`引用，以及原`bundle`、`query_ids`、`shape_ids`。

Agent维护这些字段；用户继续用自然语言描述目的和取舍。每项须匹配当前决定的对象、版本、条件及来源。绑定需声明全部当前工程检查、`current_project_context`、`current_condition_model`、`bound_focus_context`及实际查询/shape；至少一个shape、非空验收和断言。先用当前程序对准确输入生成新审查，随后才能在本入口验证重放；旧程序摘要的回执不借用新程序资格。

Rust重读来源，并通过原`RustSemanticPort`和`project::review_current`真正执行。完整原检查结果、覆盖、输入绑定和执行程序需与保存回执匹配；保存的`pass`或完整哈希本身不足。当前原件和规则包在结尾再核。未声明依赖仍是原Project的未知边界。

结果保留`legacy_impact`原报告，并列出`review_satisfied`与`still_requires_review`。部分缺少回执或失败时，已知检查保持各自结果，上游仍待复核会传播到下游；同时使用旧、新依赖和basis，删边不能抹掉旧影响。unknown/refuted决定不会被该入口改成supported。

`review_complete`只表示本次所有受影响决定的声明检查已完成；`review_partial`返回非零及可检查的部分结果。两者均为`engineering_adoption=not_granted`。程序不改Project、不写批准策略、不冒充独立审批者；工程采用继续沿原生adopt合同及真实权限办理。

资源保持有界：最多32个不同决定、256条不同来源路径、单原件16 MiB、当前来源累计64 MiB；规则执行复用原内核预算。所有输入路径遵守正式根的原件读取合同，失败不跳过来源，输出不覆盖历史回执。

此能力是本机有限软件接续；来源适用性、数值测量真值、原生CAD、物理验收、独立批准和RRSI收益各有自己的证据。实际构建、安装及本轮使用回执以`var/maintenance/status.json`为准。

## 显式大来源核验 /2

同一命令按请求 schema 明确选择 /1 或 /2，不自动迁移旧请求。/2保留before、current、sources、reviews，另需source_budget：max_files、max_file_bytes、max_total_bytes_per_pass三个正整数。硬上限分别4096、256MiB、2GiB；调用方可声明更低上限。累计量按每次完整核验计，实际必须完成审查前后两次，最坏读取量为两倍累计上限。来源数、字节数、文件系统性能与外层执行期限分别判断，未承诺硬实时完成。

沿原oe_core::fingerprint_file、普通文件及路径检查，用64KiB缓冲逐个读完并核SHA256；每个文件同时受剩余累计预算限制。来源程序、归档和CAD文件只核完整身份，不解析、运行或因此认证工程主张。Project、绑定、RDF和回执仍按原16MiB读取上限处理；32项当前复核、全部工程检查、依赖传播与独立采用保持。

/2结果具有独立版本，source_inventory报告实际路径数、不同摘要数、每遍字节数和完成的两遍核验。重复证据ID可引用同一摘要；不同路径的同内容仍重复计入文件数和字节。缺一项当前证据、来源漂移、重复路径、未登记摘要或预算耗尽均不产生完成结果。/1原来的256条、16MiB和64MiB限制不变。

该扩展回应完整历史中大量小证据及保留程序原件的读取需求。当前源绑定、各平台实际接收、完整Project原件可得性、J01/J02历史连续性、原生CAD、工程采用和方法收益分别验收。

### Closed CLI failure reasons

The generated port keeps its `FailureRecord` contract. The Project CLI extracts only an exact, allowlisted reason from the expected failed/unknown/nonretryable envelope. Source-count limits, source identity changes and incomplete inventories therefore remain distinguishable at the public boundary. Arbitrary error text, private file paths, unexpected envelopes and unlisted details retain the generic error. Rejections still produce failed/unknown/blocked; this diagnostic repair changes neither review acceptance nor adoption.
