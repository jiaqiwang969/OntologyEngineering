# 有源 typed 关系抽取 /8

工程本体论规定含义 → ggen 生成合同 → Rust Semantica 执行 → RRSI 以实际反馈检验改进。资料正文与模型关系候选用于下一步查证，不直接成为 Project 事实。

`oev2 providers typed-extract --input <input.json> --output var/<new-directory> --credential-file <explicit-600-file> [--local-synthetic-endpoint http://127.0.0.1:<port>/v1/chat/completions | --custom-openai-endpoint <explicit-full-url>] --max-calls 3` 是本候选的实际入口。输入采用生成 `KnowledgeTypedExtractionInput8`；agent 从本根 `knowledge` 原页读窗整理同一登记页、SHA、完整所选 UTF8 字节窗口、原实体和当前模型配置。用户仍用自然语言，不需手填这些字段。

输入绑定 `source_ref/page_id/document_sha256/text_sha256/raw_text_sha256/physical_page/page_count/channel/selected_text_sha256/start_byte/end_byte`。本 CLI 只实际读取登记的 native 正文；native 缺失不能当空白物理页，也不自动调用 OCR。库的 `prepare_borrowed_ocr8` 只接受同根实际 `PreparedRegisteredOcrPage6`，完整观察 JSON 保留原行序、来源与未知；它不从历史回执 JSON 恢复能力，也不新增 OCR、模型或来源所有者。OCR 原绝对时间仍由真实6能力检查。

`generation` 是原 `SemanticProviderRequest`：schema、provider、interface_profile、operation、model、custom_endpoint、prompt、options。此入口 operation=`generate`，prompt 必须为空，由原 AST 冻结模板构造。明确 `options.max_tokens`、`maximum_request_bytes`、`context_window_tokens` 和 `reserved_output_tokens`；实际 wire 全字节与保守 UTF8 输入估算加输出预留分别检查。typed 修复和空结果后的 fallback 也在真实 post 前逐次检查。估算不是精确 tokenizer，费用与未报告 usage 不填零。物理调用沿既有 SharedModelCallContext、RustTypedDispatchHarness、HTTP transport 和凭据 guard。

当前业务子域是原 `RelationsResponse` 和 `RelationsWithTemporalResponse`、最多80给定实体、原单个64000字符以内完整窗口、exact 原实体或 ASCII 不区分大小写匹配。原 prompt、时间字段和 `llm_typed` 转换保留；未迁移 fuzzy match、synthetic entity、>80过滤、分块/cache、任意 Python 对象及 Instructor 分别拒绝或保持 unknown。空 typed 关系通过同 dispatcher 调用结构化 fallback，本 profile 继续核原 schema 与真实 response/request 绑定。原完整结构化 fallback 的宽松对象域仍未签等价。

来源通过当前 Provider/索引/登记/原文档字节读取，并在发送前后重核；发送后漂移会保留 safe raw response 和失败，不能采用。caller 在准备前创建一次120秒绝对时钟；每次来源、发送和终态检查借同一 Instant。现 HTTP 本地10秒/外部30秒以内且初始不超过 remaining；同步读取和阻塞调用的硬抢占仍未证明，超时返回不能冒成功。

回执分别记录完整来源、native/OCR、选窗/省略范围、原 typed/validation/fallback、真实 wire/body SHA、safe raw response、共享调用计数和未知。历史 indexed-native 提取进程未被重新执行或证明；当前文件身份不证明文字准确。没有实体偏移、关系因果、时间置信度校准、中文质量、工程适用或采用证书。

本片验收为独立冻结原 prompt/schema/有限转换与 literal localhost HTTP、预算/schema/source/failure 控制；真实教材 native 访问和 CLI 接线另以各当前原件记录。synthetic response 证明协议执行，不证明外部推理质量。原 PatternExtraction `/1` 原样有效，全部13494/19模块/19协议/22所有者、所有平台/providers/UI、实际项目制造与计量、完整代码RRSI资格/使用/反馈/恢复继续 required/unknown；已闭合项目不重开。接收根须自行真实ggen、构建、原回归与实际检查，不继承本候选通过。
