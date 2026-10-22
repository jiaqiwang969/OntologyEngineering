# Rust 书籍来源证据门禁

旧门禁要求 native_execution 的 RECORD/source_lock/result，而当前 Rust API 返回真实 engine、archive 和 semantic_execution。原生产流程在缺批准源时得到 null report；批准源可读时，原238检查又失去执行状态。两个断点已分别实际复现，原 blocked 期待保留。

当前书籍来源检查使用独立 `ontology-engineering.book-source-rust-response/v1` 与生成的 `BookSourceExecution`。历史 Semantica native 回执保持原身份，只能作为历史，不能继承为当前 Rust 通过。通用 Catalog 算法仍在 semantica-runtime；工作室来源、执行文件和候选证据由 oe-semantic 适配，通用库不依赖工程合同。

新版绑定已批准的历史数据 archive、continuity、编译实现与两组实际生成合同、当前执行文件 SHA/字节数、作者根选择及全部已读/尝试但缺失的来源。缺失输入保存 None，操作末尾重新核对。批准 archive 或其登记不可用是 not_run；原checker实际执行但作者条件不满足是 completed+blocked。无执行或检查失败均不授予事实、治理或发布资格。

`release_evidence::observe_book_bindings` 保留完整 Rust 结果和独立执行身份。复验重新调用同一个 Catalog checker，并逐项比较整个回执和输入清单；删检查并重算 raw/result SHA 也不能通过。符合结论仍要求批准 archive、一致源码/执行文件/输入、无验证错误、全部非空检查 passed，与原门禁强度相同。

公开 `books::rust::source_execution::IMPLEMENTATION_SOURCES` 由 Cargo 复用 `build_support/provenance.rs` 的同一编译输入清单构造，覆盖拥有的 Rust/TOML、两个本地 parser/evaluator、规格模板、实际两组生成合同与生成回执。该表是编译来源元数据，合同仍由真实 ggen 生成；修改源码不能继承旧执行文件身份。表不含作者资料或凭据，不能代替实际来源、计算或工程通过。外部 Cargo 源缓存、原生依赖与未登记编译环境仍不在这一绑定范围。合成缺作者输入 fixture 与正式登记材料分别验收；正式现有294作者文件存在，不能把隔离副本缺材料写成正式原稿丢失。

本次只接续具体书籍来源门禁。其他 release evidence 的旧外部身份/回归协议、release_sources 运输承诺、PDF/视觉审查及工程采用仍须分别迁移或取证。来源一致性不证明教材科学内容正确、设计可制造或整体书籍发布已通过。拒绝/未执行/blocked/未知均保留，不能靠删除原负例或忽略失败结束全Semantica迁移目标。

构建与执行使用同一个 `build_support/book_source.rs` 输入集合函数。执行时核对已编译成员的字节，再核对当前成员集合；新增拥有的 Rust 源文件或模板同样需要新的编译身份。当前测试分别保留字节漂移和新增成员反例，不把仅有旧成员一致性称为全部来源闭合。
