# Native semantic service: finite development slice

工程本体规定含义 → ggen 生成合同 → Rust Semantica 执行 → RRSI 根据实际反馈检验改进。该服务复用同一 DatasetSession 和生成合同，不建立 Project、采用规则、版本登记、模型凭据或经验历史的第二权威。

`oev2 services mcp-stdio` 执行真实 binary UTF8/newline framing。显式 `--context-graph` 从当前 root 展开原源，严格 JSON 读取后由 host 载入同一 Dataset 的 `semantica.context-record-projection/1` named graph。节点 id/type、完整原属性及边 id/source/target/familyId 保存在显式记录中；重复 ID、悬空端点和部分无效输入先于发布拒绝。任意 RDF subject/triple 不计为 ContextGraph node；声明记录的语义不自动成为工程事实。此投影只是有限兼容开发 profile，未证明原 ContextGraph 全部持久化/算法/API 等价。

原 canonical 17 tools 的 name/description/inputSchema 全部保留，wire `_meta.semantica` 和生成 tool DTO 显示实际 availability。默认 `--tool-profile read-only` 保留有限 summary 入口；显式 `--tool-profile native` 接入同一 DatasetSession 的决策、实体、关系、推理、图分析、导出和 pattern 抽取，`original` profile 保留已冻结的原异常。书籍工具另需 `--book-catalog` 注入已登记的 ArchiveProvider：目录、manifest、来源检查及章场景执行走同一书籍实现，验证仍保留 blocked 条件。30 个动态 manifest 资源仅在该目录实际连接时可读；原固定资源仍按实际可用范围报告。各工具的条件、兼容边界和未覆盖行为见 [MCP 工具](MCP-TOOLS.md)。HTTP/SSE/WS、学习型 NER、完整 Explorer/WASM 与原类的全部公开方法继续开放；工具接线、目录返回及局部执行均不等于全库验收。

只读 `oev2 services query` 以显式RDF原文件和SHA读取，从生成 SemanticServiceQueryRequest 校验当前revision及RDFC dataset identity，真正调用同一DatasetSession.query/cache/typed RDF实现。unsupported/SERVICE/update/malformed/stale/source mismatch失败，无Python生产fallback。该调用不采纳Project事实、不自动选择版本或产生学习事件。

native host 提供同一个生成 SemanticServiceBudgetPort，request/query共享累计call/input/output预算；stdio把空白、换行、batch封装计为真实传输字节。超限在输出前拒绝，不启动socket或子服务。本片无任何模型调用，模型能力须注入既有 SharedModelCallContext 与 oe-model-credentials 的显式 CredentialPool，不另建model ledger。现query本体预算不代表已证明任意SPARQL wall-time/cancel/CPU预算，复杂query资源仍需单列验收。

协议固定2024-11-05沿原legacy dispatcher；当前 serverInfo 显示新Rust真实版本，不冒用旧0.6.5+oe.7。严格 lifecycle/version negotiation尚未接通。canonical旧batch无响应并只向stderr写说明；显式compatibility-batch按旧兼容入口逐项dispatch。非法params/shape受控返回JSON-RPC错误、非法UTF8受控parse error是明确的新finite profile，未声称旧错误文本/全部非法形状等价。未支持工具返回isError/not_run，而不是假成功。原endpoint启动参数不一致和worker占位未被此stdio路径自动修复。

合同正本为 specs/engineering.toml 的 semantic_service、semantic_runtime 及关联类型与端口，由锁定实际 ggen 生成。公开 fixtures 保存原目录、dispatcher、完整初始化条件及实际业务对照；每组预期只证明其冻结输入和公开的兼容范围，不等于原整个 ContextGraph 引擎。source/build 输入集合含 fixtures 及中性 crate，CLI entry 复用既有 compiled-source、generated engineering/neutral binding。全库 13,494 scope 及 80 前端责任保持不变，有限测试 pass 不能宣称完整迁移、完整协议合规或工程采用。

RRSI候选仍经同一source/spec → actualggen → build/test → 固定baseline/candidate对照；whole-code资格、选择、真实使用、反馈和恢复仍走既有唯一registry/adoption/history。此服务没有制造资格通过或新的Project/registry。两本方法书、四教材、MISUMI 资料与工程审阅职责保留在上层体系。

wire/catalog/context记录的JSON读取复用中性providers::parse_protocol_json，保留用户字面`$serde_json::private::Number`对象；先由原json.loads+确切dispatcher独立冻结，再取得Rust RED并修复。MCP协议parser保持原json.loads的重复key last-key-wins行为；工程CLI的显式context/query输入另走既有strict_json，重复key拒绝规则属于该输入边界。

共享生成DTO的JSON身份由[同一中性JSON owner](JSON-IDENTITY.md)维持；仅先解析为Value再derive DTO仍会在锁定serde的内部Number标记边界重解释对象。字段hook经规范/RDF/真实ggen生成，raw及preserved-Value分别取证，Option缺失/null和required Vec原行为保持。该修复不改变本页完整服务未接通范围。
