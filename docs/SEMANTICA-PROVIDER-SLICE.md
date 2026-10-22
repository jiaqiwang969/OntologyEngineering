# 原 provider 协议的 Rust 增量

工程本体将模型调用视为有条件、可追溯的计算活动。模型答复、HTTP 完成和 JSON 解析成功不建立工程事实。请求、实际传输字节、响应、执行身份和实际用量分别记录；方法升级仍走已有工程采用与验证流程。

本候选将 OpenAI/Groq 的 generate、generate_structured 协议迁移到唯一 `semantica-runtime::providers`。请求、wire/result/usage/failure、注入端口和共享调用状态从同一 `specs/engineering.toml` 由锁定官方 ggen 实际生成。通用层不依赖 oe-*、HTTP SDK、凭据、环境变量、重试预算或 Project。`oe-semantic::providers` 仅导出同一算法和已有 DatasetSession/provenance。

保留原标准 OpenAI 的 json_object 模式，自定义 OpenAI 省略 response_format；Groq 在提示词不包含大小写无关的 json 时补 JSON 指令。保留原白名单、非 None 参数、单条 user 消息、首 choice 返回，以及 semantic_extract/llms 两个 Groq 入口的不同默认模型。结构化返回支持 scalar、false、null、列表、对象和原 JSON 修复顺序，不将原失败扩大为推断成功。未知 kwargs 被原白名单忽略。

`oe-model-credentials::provider_transport::ProviderHttpTransport` 接收上层显式构造的 `CredentialPool`，复用其 lease、retire、contains_in 与 generation。`read_single_http_bearer` 支持单个 RFC6750 ASCII token，仅可从明确指定的 owner/0600/regular/no-symlink 文件读入；不自动发现账号或恢复旧凭据格式。传输一次只发一次 HTTP 请求，禁自动重试和重定向；授权头为敏感值，失败不回显任意正文或路径，响应外层和解析后 structured 结果都接受同 pool 检查。401/402 只退役原 lease，不隐藏轮换调用。

Harness 模块中的 `SharedModelCallContext` 是普通模型调用的共享预算 owner。它由显式 policy 构造，chat、embedding 和注入的其他普通 provider 共用同一实例，dispatch 前计次；当前本地 checkpoint/consolidate 不调用模型。失败也计 attempt，耗尽后不再调用 transport。已报告 token 使用 BigUint 精确累计，缺项/坏值不估为零。独立进程不会自动共享此内存上下文；本切片不声称已有持久跨会话预算账本。

成功模型活动委托已有 `provenance::load_records` 原子写入同一 DatasetSession。输入为已检查 wire body 的原字节，输出为已检查结果的原字节，保留软件主体、活动开始/结束、使用和生成关系、200 Unicode 字符 preview、用量与 source hash。不另设 provenance registry，不根据来源活动自动采用输出。价格/费用估计、失败活动、流式活动仍无原功能等价证明。

`oev2 providers generate` 只读取明确输入与显式凭据文件，使用 Harness context 后调用唯一通用库。可选 `--local-synthetic-endpoint` 必须是 http literal loopback，并将结果永久标为 synthetic_loopback；测试不读外部现有密钥，不作真实供应商或付费调用。`--custom-openai-endpoint` 表示完整 chat/completions URL，原 Python base_url 配置/SDK初始化和全部 client availability 语义还未迁移，本切片不以完整 URL 入口冒认其完成。

原完整范围锁和 13,494 门禁不变，migration mapping 只声明实际协议来源，不能自签 verified。原18个相关源码和22个直接相关测试的逐字节审计保存在候选私有调查证据中。Base generate_typed/instructor/schema修复、通用 Base structured 多轮反馈、ProviderPool/custom registry/cache、Gemini/Anthropic/Ollama/DeepSeek/Novita、HuggingFace 加载/推理/NER/REBEL、LiteLLM、多轮/批量/流式、所有 embeddings 与原 provenance wrapper 完整方法仍待迁移；Python NaN/Infinity JSON、原 URL/SDK全部初始化行为也未等价。本页不会缩减完整目标。
