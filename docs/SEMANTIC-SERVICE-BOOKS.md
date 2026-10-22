# 书籍目录接入统一 Rust 服务

工程本体论规定对象、版本、条件、关系和证据的含义；ggen 从工程设计规格生成共享合同；Rust 版 Semantica 实现查询和执行；反馈进入现有 RRSI 实验与独立比较。目录查询属于这一执行体系，不能单独证明工程设计正确。

`oev2 services mcp-stdio --book-catalog` 把已经登记并钉住版本的两本方法书目录接入原 MCP。它复用 `ArchiveProvider`、中性 `Catalog` 和调用方的同一 `DatasetSession`，没有第二套书籍登记或工程项目状态。原 29 个章节与 1 个规范领域包保持原身份和顺序；原 manifest 内容以及 partial、absent、blocked 状态保留。

`SemanticBookCatalogRequest`、`SemanticBookCatalogResult` 与 `SemanticBookCatalogPort` 由真实 ggen 生成。上层仅运输已批准的书籍档案，并在读取前后检查档案和来源登记；中性服务不读取任意本机路径。查询实现返回原 `list_chapter_packages`、`get_chapter_package` 的公开 DTO，列出原 4 个固定资源和 30 个 manifest 资源。每个资源只按原精确 package ID 解析，不规范化路径、转义或重复前缀。

47 项独立原查询预期和 34 个原资源从 oe.7 原函数及钉住档案内容冻结；新增验证保留接口未连接时的真实失败，再验证 Rust 接入、来源漂移、预算和同一数据集身份。原函数只在私有历史 oracle 中选取执行，生产不调用 Python。原全部 13,494 项迁移范围继续保留，目录验证不能代替整库行为验证。

`--book-catalog` 现也连接同一实现的 `verify_book_sources`、`run_chapter_package`、`verify_chapter_package`。章场景实际查询、规则、SHACL 和 oracle 比较复用同一 DatasetSession；历史 RDFLib 摘要与当前 RDFC 摘要分开，可信编译来源未接通或原发布条件未满足时验证继续 blocked。完整五项书籍工具、空串默认场景、输入身份及剩余范围见 [MCP 工具](MCP-TOOLS.md)。原 47 项目录预期属于只读目录范围，不扩张为后来执行工具或整库的资格。图谱、推理和 pattern 抽取需显式 tool profile；HTTP、SSE、WebSocket、完整 Explorer/WASM 仍待迁移。四本机械教材、米思米和工程证据依旧由原工程知识与调用体系组织，目录资源不把它们冒充两本方法书的章节。
