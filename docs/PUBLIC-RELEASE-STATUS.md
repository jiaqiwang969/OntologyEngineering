# 公共发布状态

当前状态：**BLOCKED — 不应把 OntologyEngineering 本地工作树整体打包或声称为
已经完成权利清理的公开书包。**

该阻断不否定两卷书的教学价值，也不构成法律意见。它与“可以向经授权的 Semantica
fork 推送代码改动”是不同问题：代码仓推送权限不自动清理书稿、图像、规范派生表达
或受控来源的再发布权。

## 两个独立发布面

| 发布面 | 内容 | 当前要求 |
|---|---|---|
| OntologyEngineering | 两卷书、来源地图、检索/教学薄入口、构建锁 | 书稿/图/PDF/元数据逐项 rights + privacy allowlist |
| Semantica fork | 运行时、29 chapter packages、normative domain package、Python/CLI/MCP | 代码许可、package NOTICE/来源权利、测试与 receipt 门禁 |

两边通过稳定 package ID、书源锚点和 `runtime/semantica-source-lock.json` 连接。不要把
Semantica 迁入成功误解为 OE 内容权利自动放行，也不要把 OE 书稿可读误解为 package
可以不经独立权利审计公开。

## 当前阻断项

- 两卷书、图像、PDF 和高密度标准派生表达尚未形成逐项公开资产 manifest；
- 第二卷部分来源审计仍有 `pending`、`review_required` 或
  `not-cleared-for-republication` 状态；
- 涉及真实商业接触缘起的前言需作者明确批准，或合成化后同步重建成书；
- 书籍 PDF、图片及其他二进制资产仍需清理并白名单核对元数据；
- 代码、书稿、图片、PDF 与 Semantica package assets 的许可/NOTICE 边界必须分别登记；
- normative package 只能公开经审阅的转述与派生语义，不能夹带 ISO 原文或受限抽取件；
- 最终 source lock、可复现 wheel、package receipts 和 zero-exception backend gate 必须
  在候选 commit 上重新生成并通过，不能沿用迁移前的版本/哈希/测试声明。

## 架构侧已确定的发布纪律

- 外部书源只有两卷书；OE 不发布平行 ontology/CQ/SHACL/SPARQL/case/rule 副本；
- 29 章与 normative 的可执行正本位于 Semantica built-in packages；
- OE 只通过受控 bootstrap 调用 Semantica，无 fallback、无 backend bypass；
- unknown/missing/unsupported/partial/placeholder/hash mismatch 一律阻断；
- package 执行与 release verification 分开报告，场景 oracle 通过不等于可发布；
- 精确标准条款、表格与原文始终回用户合法持有的受控来源核对。

## 已完成的隐私改进

- 公共候选文本和脚本不再硬编码个人绝对路径；
- 新增 default deny + allowlist 发布政策与 ignored-worktree 扫描；
- 新书脚手架默认隔离标准原文、企业证据、会话、密钥和未清权利图；
- 自动门禁不得升级成认证、合规、风险接受或发布授权；
- 书源与 Semantica 执行资产的仓库边界已经明确。

## 解除阻断所需决定

1. 作者批准或合成化处理真实缘起叙事；
2. 对每个标准派生制品和 Semantica package asset 完成权利裁决；
3. 为代码、文字、图像、PDF 和 package data 分别登记许可/NOTICE；
4. 建立两个仓库的公开资产 manifests，所有放行项均为明确批准；
5. 清理 PDF/图像元数据，从干净 commit 按 allowlist 导出；
6. 在最终 Semantica commit 上重建可复现 wheel，更新 source lock 并生成 receipts；
7. 运行隐私、秘密、权利、技术、zero-exception、读者和领域门禁；
8. 由有权人分别批准 Semantica 代码/package 发布与 OE 书包发布。

在这些条件完成前，可以继续本地开发、向已授权开发 fork 推送合规的代码改动、使用
合成案例和评审方法；不得声称整个 OE 书包、规范派生语义或标准内容已经获得公开
再发布授权。
