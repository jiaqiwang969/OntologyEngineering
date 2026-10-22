# 登记物理页的 OCR 来源内容

工程本体论区分资料原件、登记页、原生文字、渲染图像、OCR 转写和工程主张。规范新增 `KnowledgeRegisteredOcrRequest6` 与 `KnowledgePreparedRegisteredOcrPage6`，由真实 ggen 生成合同，Rust 在现有知识所有者中执行并保存反馈。此项补资料正文入口；方法收益及工程采用仍依 RRSI 固定目标、反例和独立验证流程判定。

调用 `registered_ocr6::prepare_registered_page_ocr6` 时，使用同一 `SkillRoot` 登记/索引，显式选择 document_id、登记 page_count 和物理页；不能传任意 PDF 路径。原件、登记、索引、行及工具、config、语言数据的字节/SHA 前后分别核验。`ocr_page5` 内部只有一份实际 OCR 执行，旧 `/5` 的 interchangeability/188/144dpi 拒绝边界继续保留；另版 `/6` 一般登记页复用原 `/4` 的完整144dpi渲染及 `/5` 的 chi_sim+eng/config 工具域。这不是全部 DPI、语言或任意 PDF 的兼容性声明。

工厂借用进入资料准备前建立的同一绝对 `Instant`；不增加资源预算。每页四次原 Poppler 调用，Tesseract 一次版本观测、一次 OCR；无自动重试。同步读文件、计算与原渲染器的硬期限、实际语言加载和动态库闭包保持未知。完整原始 argv、stdout/stderr 与失败产物保留，不能凭 exit0 推断文字准确。

`read_source_content6(&prepared)` 先重验当前同调用来源，返回借用的中性 TSV 内容与登记页出处。它是后续同 Project 资料背景准备的输入入口，本模块没有模型派发、凭据、Project 更新或工程采用权限。JSON 审计或 TSV 解析结果不能构造 opaque producer；实际模型读取与工程主张需分别取证。原生正文缺失仍只指原生文字通道，不能断言物理页为空；OCR 文字与原生正文保留独立摘要、阅读顺序与准确度状态。

当前私有接入只有两份原 PDF；登记六本不代表六本原件都可读。vol2/24 的真实有限验证属于已知开发回放，输入和独立 PNG/RGB 预期在候选输出前冻结，没有逐字 OCR 准确度 gold，也不是未见任务收益。旧严格 TSV `/3`、`/5` 反例及原 same-Project native-only 模型请求保持。全13494项、19模块、19协议、22所有者和平台/SDK/UI/server及真正工程交付继续未闭合。
