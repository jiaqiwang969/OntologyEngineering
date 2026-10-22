# 大上下文主模型直接调用

长资料恢复、跨条件关联或复杂工程推导需要额外模型辅助时，使用同一 Project 的 `project model-assistance prepare/run`。此可选通道经 Rust 直接 Responses HTTP，不启动 Codex 子进程；当前 Agent 可以不配置该服务，直接组织本地查源、专业工具与审阅。模型输出只提供待核工程线索。

Agent 从自然语言维护 `ModelAssistanceInput`：当前 Project 原件及 SHA、对象、当前 ProjectReviewContext、原目的、本次明确任务、问题文档与当前支持原件。用户无需手填 JSON。`original_requests` 的路径与摘要现在绑定 `oev2.advisory-questions/1` 文档；顶层仅接受 `schema/state/questions`，state 可选。questions 为 1–1024 项对象，每项有非空问题 ID、`instructions` 与 1–255 项字符串 `criteria`。例如：

```json
{
  "schema": "oev2.advisory-questions/1",
  "state": {"purpose": "核对候选计算的适用前提"},
  "questions": {
    "model-fit": {
      "instructions": "逐条检查当前来源支持的前提，保留未知。",
      "criteria": {
        "supported": "完整必要前提有当前证据支持",
        "unknown": "仍缺必要条件或专业验证"
      }
    }
  }
}
```

历史服务原始请求不自动成为该协议。需要借用旧问题时另建派生文档并保留原件、来源和摘要，不覆写旧请求。完整当前 Project 和当前上下文承担当前身份，旧快照不覆盖它们；明确问题的回答不能冒作完成机械设计或工程采用。

`ModelServiceProfile` 只放私有 `var/`：端点、精确请求模型、允许的精确返回模型列表、独立上下文/字节/输出/响应/超时预算、容量依据及凭据文件路径。`oe-model-credentials` 仅从明确指定、当前所有者的 0600 普通非软链接文件读取单个 HTTP bearer token；不读取 Codex 订阅登录令牌，不自动发现账号或修改全局模型配置，不将凭据值放进报告或分发包。支持 HTTPS 和明确 loopback HTTP，禁止重定向，不猜测协议，不自动改用其他模型。

```text
oev2 project model-assistance prepare --input var/.../input.json \
  --profile var/.../profile.json --output-dir var/.../prepare-01
oev2 project model-assistance run --input var/.../input.json \
  --profile var/.../profile.json --output-dir var/.../run-01 --live
```

没有 `--live` 时明确 not_run。每次 run 从原件重新准备，可编辑 plan 没有执行权。必要原件摘要、条件、对象、来源或上下文失配时阻断，保留原 kernel 检查及具体诊断。模型可以辅助调查已知工程违反及缺证据，没有工程采用权限。

运输使用 `store=false`、`truncation=disabled`、`stream=false`，没有工具调用。完整请求、原始输入档案、解析响应及终态回执在新目录追加保存；归档和 HTTP 发送复用同一序列化函数，记录实际发送字节和摘要。响应档案是解析 JSON，不声称 HTTP 原始字节。HTTP 观察分别保留客户端发送尝试、已观察状态和响应读取；第三方网关内部的上游路由及调用次数仍未知。`ModelAdvisory` 保留原问题选项、说明、来源、未知及验证要求；本次工程线索分为 fact、candidate、missing_condition、unsupported。事实须关联当前 evidence 身份；来源身份仍不证明内容支持或物理适用，agent 继续回读原页和核对专业证据。输出 JSON Schema 从真实 ggen 生成的字段和枚举映射推导，不另维护一份字段合同；未支持映射明确拒绝。

准入区分完整 HTTP 请求 UTF8 字节与 `utf8-model-content-conservative/1` 的模型正文计量：全部 input_text、instructions、输出格式字节，加明确包装余量及输出预留。它不是官方 tokenizer，也不是经证明的上界。容量声明、兼容服务实际行为、provider usage、结算费用及工程效果分别取证。历史超限、拒绝和未知用量保留原身份，不因新预算或入口关闭。

[OpenAI Responses 文档](https://developers.openai.com/api/docs/guides/text)说明输出可含 reasoning 等多项，不能只取第一项。parser逐项核 message/output_text/refusal、完成状态、精确模型、原问题覆盖及选项、来源和用量。incomplete、refusal、类型错误、缺失/无效 usage、失败或中途源变化保持对应状态，费用不填零。客户端 send 尝试不自动证明上游请求数；fixture 与实际服务分开登记。

单次派发，无重试或隐式回退。结果 `adopted=false`；结构及模型完成不替代 CAD、计算、制造、检验、最终计量和治理。首次接入服务/模型先冻结目标、输入、必要条件、预期及调用上限，失败保留并查机制。规则调整另建基线，不删未知、缩验收或改答案获得通过。

Responses 解析 `/1` 保留旧行为。`/2` 仅兼容已完成文本项中的 JSON 空白，仍要求唯一非空完整 JSON；额外非空输出、拒绝、未完成及无内容保持拒绝。当前上下文 `/2` 从 Project evidence 账本明列可引用身份，并给 ggen 派生的输出 schema 绑定同一名单。条件名称和 kernel 检查编号不因此变成证据来源。旧答复中的非法引用不映射、不删除、不改签，新上下文的模型效果另取证。

实际服务与模型由本次 profile 明确选定，并核对对应官方合同。兼容服务列出同名模型不证明相同容量、费用或全部接口能力。安装及本次执行须有各自身份；历史维护状态不替代新安装验收。工程及独立迁移未证明前仍重构中。
