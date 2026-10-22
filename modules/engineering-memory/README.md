# Engineering memory

工程本体项目内的正式记忆模块。由同一个 Cargo workspace、锁文件、安装过程和 `oev2 memory` 入口维护，直接复用根目录的 Semantica Rust 实现。私人原始会话、索引和经验库写在显式的 memory home；源码和安装目录不携带这些资料。

本模块负责保留来源、时间线、纠正、适用条件、因果不确定性和实际使用反馈。Project 负责当前工程事实与采用。召回结果始终是待核候选，整理审阅通过与工程采用分别记录。

## 安装与入口

先按仓库根 README 安装整个系统，再调用：

```sh
oev2 memory capabilities
oev2 memory status
oev2 memory recall '要查的工程问题' --condition '本次实际条件'
```

`oev2 memory` 使用统一安装中的 `engineering-memory` 程序。需要共享一个私有记忆库时，使用 `--home /absolute/private/memory`；单独调用程序时必须提供 `--home`。不同 profile 只有显式指向同一 home 才会共享，不能由 profile 或工作目录猜 Project。

普通经验记录、来源验证、检索、接续、回交与反馈无需外部模型服务。Agent 在当前任务中审阅原文并提交 `record` 或 `resolve`。`configure-extractor` 是显式选择本地程序的可选接口；未配置时 `run`、`tick` 只接入来源并保留待审包，不会自行提炼或调用模型。本模块不安装定时器或常驻服务。

## 从原件到可回查的经验

1. `evidence` 读取 Codex 或 Claude 的指定原文窗口，返回原字节范围与 SHA256；工具输出和隐藏推理不冒充用户或助手可见消息。
2. `register`、`sync` 登记并接续读取历史或新增消息。读取形成 pending packet，不能把“读过”记成“经验已整理”。
3. Agent 检查原文，保留失败、纠正和未知，用 `record --input experience.json` 保存经验；批量接续用 `resolve --input resolution.json`，逐条说明来源使用或省略原因。
4. `session prepare` 按当前任务和显式 Project 产生完整候选上下文；`session activate` 明确选择当前绑定。Project 或来源改变后重新 prepare，不能继续使用旧的有效性结论。
5. `session handoff`、`session export` 保留完整经验及来源映射。接收方按 Project 合同决定是否采用。
6. `session feedback` 记录候选究竟未用、拒绝还是实际使用，以及对应结果证据。曝光、使用、有益和可证明的改进分别处理。

```sh
oev2 memory evidence --provider claude --source /absolute/session.jsonl --start-byte 0 --end-byte 4096
oev2 memory record --input /absolute/experience.json
oev2 memory verify --id example --revision 1
oev2 memory session prepare --input /absolute/session-request.json
oev2 memory session activate --id task-context --context-sha256 RECEIPT_SHA256
oev2 memory session verify --id task-context
```

上面的窗口边界、ID 和摘要是示意；Agent 根据实际回执填写。用户只需说明目的、条件与取舍，结构化记录由 Agent 维护。原生合同以 `contracts/specs/engineering-memory.toml` 为正本，示例模板在 `skill/templates/`。占位模板必须补成真实来源后才能提交。

## 与同一 Project 的原生回交

`session v2-prepare` 校验明确的接收根与 CLI 摘要，调用真实 `knowledge intake`，输出完整事实与证据提案。Agent 在接收方审阅并更新 Project，然后重新绑定上下文；`session v2-verify` 运行真实 task-view 和 verify-task-view，复核原件到接收证据的两侧映射。

新安装通过 `var/install/INSTALLATION.json` 绑定程序身份。该文件存在却与根路径或摘要不符时，整体拒绝；不能回退到旧程序。只有没有新安装清单的显式历史安装才读取旧维护状态。来源字节一致只证明可追溯性，不证明结论真实或适用于工程。

## 历史检索与审阅组织

`history-index-build` 根据显式 `HistoryIndexBuildRequest` 建立可见消息锚点目录；`history-search` 执行有界词法检索，引用前通过 `evidence` 重读来源。索引覆盖与语义阅读覆盖分别报告。

`organization` 保存一个任务的内部方法、独立命题、依赖、证据和宿主审阅。角色为 `host`、`agent`、`program`；委派给 Agent 的方法需要已有 pilot 证据。程序不内置模型调度。纠正会使依赖旧结论的命题失效，未知可以保留并停止，所有必要命题未受支持时不能宣称 completed。关闭不会自动写经验，只有显式 `publish` 才发布可召回记录。

新组织写到 `organizations/`，不会自动加载旧实验任务或沿用它们的执行身份。已有 experience/2 与反馈合同继续保留来源和修订；迁移原经验时必须连同原文位置与实际访问条件一起核对。

内部方法、组织计划/结果/交接及组织发布请求使用 `/2` 合同，以区分新的通用 Agent 审阅与旧执行身份。旧组织原件不改写，也不能直接当作本版通过回执。

## 客户端接续

可选 `hook` 接收 Codex/Claude 生命周期事件，政策显式指定合法会话根目录、Skill 路径和上下文预算。它只提供历史候选、登记待整理来源；失败时报告警告并允许会话继续。不要仅因配置文件存在就宣称客户端已经调用。完整安装验证应在新的会话中观察实际触发与回执。

## 开发与验证

在仓库根目录运行：

```sh
rustup run 1.95.0 cargo test --locked -p engineering-memory-runtime -j2
rustup run 1.95.0 cargo clippy --locked -p engineering-memory-runtime --all-targets --no-deps -- -D warnings
rustup run 1.95.0 cargo fmt -p engineering-memory-runtime --check
python3 modules/engineering-memory/contracts/generate.py --cli /absolute/oev2 --ggen /absolute/ggen --check
```

生成器只编排真实 Rust 规范化与锁定官方 ggen，不自行渲染类型。`--cli` 必须与源码根匹配；单独安装的规范化程序可用 `--normalizer-root` 指向它自己的匹配根。生成后的 `contracts.rs` 和 `generated.rs` 不手改。

一般回归覆盖来源篡改、纠正链、队列接续、条件不适用、Project 漂移、客户端接续与组织审阅。原生接收测试需要明确的真实安装根，单独执行 `native_receiver_keeps_both_provenance_sides_current` 的 ignored 测试。合成回归不代表全量历史已提炼，也不证明跨项目方法迁移或 RRSI 已产生收益。

公开的来源审阅、候选回交、Project 登记、使用反馈与新会话召回示例见 [examples/README.md](examples/README.md)。请求与字节锚点由原生 Rust 读取器按实际路径生成，执行阶段只调用已核验的真实安装。

来源与许可见 [NOTICE.md](NOTICE.md)。
