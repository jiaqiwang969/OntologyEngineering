# 独立 ProjectStore：跨会话接续的公开例子

本例用公开的合成支架资料演示：安装目录与项目目录分离；两次独立进程从文件恢复同一目标；用户纠正追加到原请求后并保留未解决状态；根目标变化后，旧 RRSI goal 不能继续冻结。没有调用模型、专业工具或设备；`ready` 表示接续材料完整，支架验收仍是 `open`，工程决定仍是 `unknown`，不产生工程采用。

先按仓库 README 安装**包含 ProjectStore 命令的当前源码版本**。本例需要 Python 3，仅用它封装已有 JSON、计算文件摘要和核对输出；所有 Project 检查、状态迁移、冲突及 RRSI 绑定都由 Rust CLI 执行。已有旧二进制不会因为源码更新而自动支持新命令。先核：

```sh
OE_ROOT="/absolute/real/path/OntologyEngineering"
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" project workspace --help
```

使用一个尚不存在、父目录已存在的真实绝对路径运行。脚本拒绝复用已有目录，也拒绝把示例放进安装目录；不会调用默认共享 memory，不会修改已有项目：

```sh
OE_PROJECT_HOME="/absolute/real/path/bracket-continuation-demo"
sh "$OE_ROOT/examples/project-workspace/demo.sh" "$OE_ROOT" "$OE_PROJECT_HOME"
```

开发验收可显式设置 `OEV2_BIN=/absolute/path/to/oev2` 选择与当前源码绑定的新构建，不能拿旧安装或其他源码的程序替代。脚本保留全部资料、HEAD、不可变 commit、摘要归档、派生 Markdown 和预期拒绝日志，不删除测试目录。

逐步查看其原生命令如下。`demo.sh` 已执行这些步骤；不要在同一目录重复执行写入命令。第一条 `init` 要求已创建真实目录，复制 `fixtures/` 后即可提交。

```sh
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace init --workspace-id bracket-demo --project-id bracket-continuation-demo
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace commit --input initial-commit.json
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace resume --output views/session-one.md
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace learning-goal --output learning/old-goal.json
```

`demo.sh` 把 `HEAD.json` 中的 `commit` 原样写成 `expected-head.json`，也填入新提交的 `expected_parent`。这是现有文件的传输，不由脚本决定当前性。随后原生 `begin` 重新恢复并比对 HEAD；它只检查阶段入口，**不执行阶段**：

```sh
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace begin --expected-head expected-head.json --stage S1
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace commit --input corrected-commit.json
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace resume --output views/after-correction.md
```

纠正后的恢复记录应显示完整 U1、U2 原文，`C-U2.status=acknowledged`、验收 A1 为 `open`、`automatic_dispatch=false`。acknowledged 允许继续调查，并不表示纠正已经解决。提交的是完整状态与追加历史，不能通过替换原请求、缩减验收或更换当前摘要隐藏前情。

脚本接着用旧 goal 引用运行以下命令，预期非零退出并包含 `workspace_learning_root_changed`。这是一个专门的拒绝样例；缺少旧 RRSI 的评估协议字段是有意的：根目标绑定必须先拒绝，不能进入候选冻结。

```sh
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace learning-freeze --input stale-goal-freeze.json --output learning/stale-candidate
"$OE_ROOT/var/install/bin/oev2" --root "$OE_ROOT" --project-home "$OE_PROJECT_HOME" project workspace learning-goal --output learning/current-goal.json
```

导出新 goal 不等于 RRSI 方法有效。实际学习仍须准备真实 baseline/candidate、受控评估与原件，按既有 `learning-freeze` 协议冻结、评估和反馈。本例只验证旧根目标拒绝，没有声称完成真实方法改进。

以后在新会话只需显式选择同一个安装根和 ProjectStore，再调用只读 `project workspace resume`。`views/*.md` 是派生阅读材料，不能代替原生恢复；源文件发生变化、丢失、父提交冲突或动作已派发但未回读时，必须保留失败或未知，不自动重发动作。

安装本机的 `var/capabilities/registry.json` 与共享 memory 不属于本例 ProjectStore。专业工具证据、ToolResult、项目 memory candidate 应保存到项目根下；memory 服务另用显式 `--home`。移动整个 ProjectStore 后，先重新明确绑定新真实路径并运行恢复；含绝对来源路径的 memory 接续需重新核验，不继承旧机器安装身份或工具验收。

`--project-home` 支持 Project、Harness、Kernel、memory 和 `knowledge experience`；其他 Knowledge 共享资料库命令仍使用安装的 `sources/`，暂拒绝此参数。Kernel 的业务输入相对 ProjectStore；软件 bundle 以及显式 `--query`、`--shapes`、`--rules` 文件默认相对安装根，也可用明确绝对路径选择算法文件。软件规则不会被项目内同名文件自动替代。
