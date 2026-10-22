# 登记来源 → 同一 Project 的有源模型辅助

工程本体规定对象、版本、条件、来源与采用边界；ggen 从规格生成 `/7` 共享数据与调用合同；Rust 接入实际来源并执行；有范围的真实反馈再进入原 RRSI 候选评估流程。此通道补来源阅读能力，不新增 Project、注册表、预算、模型派发器或凭据 owner。

Agent 根据自然语言目的维护同一 Project：登记原件的整个 PDF SHA 必须已经对应现有 evidence SourceRef；原 native `/2` 输入保留当前对象、条件、完整历史问题和单独原生页面正文。新 `/7` 只增加 `native_assistance` 引用和 `ocr_pages`：每项 `source_ref`、`request`（完整 KnowledgeRegisteredOcrRequest6）、`expected_selected_row_sha256`、`expected_raw_tsv_sha256`。首次未有独立 OCR 摘要时最后两项显式 null；提供值时精确核对。这些字段只选择/核验来源，不能填写 producer、report、passed 或采用权限。输入示意：

```json
{
  "schema": "oev2.model-registered-ocr-assistance-input/7",
  "native_assistance": {"path": "var/projects/example/native2-input.json", "sha256": "<原文件 SHA>"},
  "ocr_pages": [{"source_ref": "<同 Project 已登记整 PDF 的 evidence ID>", "request": "<完整登记 OCR6 对象，由 agent 维护>", "expected_selected_row_sha256": null, "expected_raw_tsv_sha256": null}]
}
```

示意中的占位须换成当前真实对象。实际 `request` 是对象，包含登记 document_id/page_count、原 /4 页请求的 index/registration/physical_page/native SHA/144dpi renderer，以及原 /5 受控 Tesseract/config/chi_sim+eng 来源。不能换成任意 PDF 路径或历史 audit。当前已登记六本书，各根可读性以本根原件 SHA/登记/index/Provider 检查为准。本次开发根冻结两份原件；其余来源缺失反例保留，正式库的六份原件不隐式授权本根读取。

`prepare-ocr` 实际运行来源 OCR并写离线背景，不调用模型。`run-ocr` 每次重新运行来源工厂；没有 `--live` 不读模型 credential 或发送。显式 `--live` 使用现有 HttpModelTransport，仍最多一 send、不自动重试/切换。Rust API 的 RegisteredOcrModelExecutor7 借当前真实 opaque6；共享 RunPort 是真正执行 facade，JSON/报告/Default/Clone 不生成来源能力。

请求保留完整原生正文与 OCR TSV 的全部原始行、空行、unknown confidence、父行诊断、来源 SHA、选中工具/配置/语言副本及实际 process 身份。原生页面字段 `ocr=not_run` 只描述原生提取通道；另列 `/6` 实际 OCR 观测。原始行顺序不等于视觉阅读顺序，置信度不等于识别准确度，选中语言文件不等于实际加载证明；数字/工程适用仍未知。不将图片伪称送给模型。

同一服务 profile 保留完整当前 Project、原问题、输出 schema 与精确 SourceRefs。输入文本、新增说明、输出 schema、包装余量、输出预留和 HTTP 字节分别计入原预算；放不下返回具体 blocked并保留完整内容。来源工厂与模型辅助复核借同一原 120 秒 caller Instant，不增加预算或重置 clock；HTTP 的服务总 deadline 沿原 owner。同步 IO/CPU、清理与原渲染硬抢占保持未知。

紧邻发送前（包括 transport configuration hook 后）和响应后重新核当前根、登记/index/PDF、选中来源、raw TSV 与原生/Project/profile。来源变化时保留实际响应、原始请求和来源 artifacts，但 current_support 为 not_established、adopted=false。新输出目录冲突拒绝覆写。模型答复继续经同一 SourceRefs 与正式内核检查，再结合计算、NX/制造/检验/现场证据形成有范围结论；不能因 source text 或模型 completed直接采用。

本轮有限验证使用一次 vol2/物理24的已知开发回放，原图/原生文本身份在实际输出前固定；没有逐字 OCR gold、未见迁移或工程收益结论。localhost synthetic credential 的单 POST 与 injected drift 是本地执行证据，真实 gateway、模型质量和全部 13494/19/19/22范围仍需接续。经验沿原流程分为项目事实、资料适用或方法候选；没有新增可验证收益记录 no_delta。

显式 `/7` 的结果另保存 `source_current_checks`：在原准入、发送前与终态检查时，记录同一次完整来源检查的 current/refused、有限静态错误码、未分类与截断状态，以及本次单调耗时。`terminal` 也会用于零次发送的结果，不表示响应已经发生。诊断不再次调用检查，不保存任意错误正文、路径或凭据，不替代最终 status、原内核检查或采用条件。未识别的错误保留 unknown；旧 `/1`、原生 `/2` 的结果字段不增改。

`read_source_content_in_root6` 将原 root 一致性 guard 和原 source read 组合，每次读取执行一次原完整 opaque6 核验。`/7` 原先在同一读取前紧邻执行相同完整核验两次，现在组合调用；旧 `/6` 的 verify/read 方法、全部原件/工具/图像/TSV/原始流核验及原绝对 deadline 保留。没有跨阶段缓存、旧 qualification 恢复或延后核验。实际控制试验能否在原 120 秒内完成须由新来源身份的实际回执证明，源码和测试不能证明耗时收益。

另一开发根的第一次完整本地试验实际 exit1/125269ms，虽然一次真实 OCR6 和来源当前的 HTTP200 答复已经取得，后续 after-raw-drift 控制在发送前拒绝、零 send、无响应，不能当作发送后变化验证。原 check 错误未保留，累计 deadline 耗尽只是有依据的推论。该失败保留原身份；新增诊断不改签旧结果，原 after-drift、预算、offline、目录冲突、最终来源/Project/120 秒断言继续执行。
