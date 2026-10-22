# 动态 provider 与 LiteLLM 包装

当前接续仍按工程本体规定含义 → ggen 合同 → 通用 Rust Semantica → 借鉴 RRSI 检验改进推进。用户给目的和条件，agent 整理来源、接口和验证。以下能力只属于模型调用和方法实现；模型答复不自动成为工程事实或采用结论。

通用 Factory 借用调用者注入的注册与实例缓存，提供 register、get、list_all、unregister、pool get、create_provider 和 clear。自定义注册优先于八个原内建名称；名称大小写与原逻辑相同。所有初始化参数参与缓存，保留原 Python 列表/元组以及字典/元组对的复用关系；原元组内容不再递归归一。缓存只保存摘要和实例绑定。注销注册后已有缓存仍存活；clear 只清实例复用。

工程适配通过同一 SharedModelCallContext 持有这个状态和模型调用预算。构造端口由调用者注入，没有新的全局模型、凭据、预算或图管理。`oe-model-credentials` 的显式 CredentialPool 负责持密、租用和检查输出。每次真实发送都先占用同一预算；发送失败仍计次，未报告 token 或费用保持未知。清缓存不能重置预算、凭据或 DatasetSession。

`providers factory` 读取 agent 整理的有限命令计划，实际执行动态注册、查找、构造绑定和缓存操作。默认构造适配只登记原默认模型与配置绑定，SDK 初始化和可用性仍为未观察；自定义构造由同一调用者端口实际提供。原 OpenAI/Groq 执行可按选中的 backend 和模型接入原 HTTP transport；其它 SDK 构造及执行保持独立缺口。不能把创建一条配置记录解释为某 SDK 或模型已经可用。

`providers litellm` 执行原 LiteLLM 包装的参数浅合并、文本或结构化输出处理。原 wrapper completion 请求保留完整模型名，实际有限 OpenAI/Groq 协议请求记录解析后的模型名。两份字节摘要分别取证，回执中的 wire 摘要只代表真实 HTTP body；同一 DatasetSession 来源记录同时保留两份请求。JSON 恢复遵循原包装的直接解析和贪心对象提取，不使用 BaseProvider 额外修复。

验证只使用本地 synthetic 凭据、注入端口与字面 loopback HTTP。原 15 个 Factory 观察、1 个原 pool 测试及 20 个包装边界保持独立冻结。包装端口不等于 LiteLLM SDK 的 62 个真实 dispatcher 运算；当前中性 JSON 有限数域、安全错误类别和明确字节/次数上限也不能冒充任意 Python 对象或非有限 JSON 原语义。原 typed/Pydantic、其余 provider SDK 和完整 13,494 项门禁继续保留，当前项目仍为重构中。

新增或纠正实现时先保留旧异常和失败记录，再冻结原目标/独立预期、隔离修正、重新生成与执行测试。原四处 provenance 导入异常没有在这里静默改成成功。
