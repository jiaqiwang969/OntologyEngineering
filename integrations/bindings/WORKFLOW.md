# 专业工具的可移植 Project 接续

这份流程保留当前六个 Skill 的本地接续行为。运行程序从固定上游安装；此目录的 Python 代码只核传输字节、绑定显式输入并监督进程。Project 语义审查、依赖传播、记忆检索与核验仍由同一安装中的 Rust 程序执行。工具完成、原件核验、决定与工程采用分别记录。

## 安装选择

先按根目录 AGENTS.md 安装本项目，并设置绝对路径 `ONTOLOGY_ENGINEERING_ROOT`。三个入口也接受 `--root /absolute/checkout`，不会从工作目录推断 Project。它们从该根的 `var/install/INSTALLATION.json` 选择程序，核根身份、唯一程序条目、实际可执行文件路径和 SHA256；不会读取另一份旧安装的配置或历史维护选择。进程监督目前支持 POSIX（macOS/Linux），不把它当作 Windows 进程树验收。

原件、Project、来源表和输出均由用户的当前任务产生。这里没有维护者的 Project、数据库、会话、凭据、旧机通过回执或运行环境。Python 3.8 及以上的标准库足够运行这些传输入口；程序本体仍是 Rust。

项目使用独立存储时，三个入口显式传 `--project-home /absolute/project/.engineering`。
该目录须已存在且不经过符号链接；不从 cwd 或环境变量猜测。`--root` 仍选软件安装，
新参数只选择业务文件和回执位置。Project、请求及相对 ArtifactReference 按该项目根解析，
缺件时不回退安装根或其他项目。`verify_candidate`/`reconcile_reviewed` 将回执写入项目的
`runs/`，并把同一 `--project-home` 传给 Rust；`resume_memory` 的 `--output-root` 也须在
该项目内。未传新参数时保留原安装根和 `var/` 路径语义，不自动改写旧引用或删除 `var/` 前缀。

本机安装 registry 的默认路径仍是安装根 `var/capabilities/registry.json`；需要固定项目内
副本时，原生 `tool-result`/`decide` 的 `--registry` 显式给该文件绝对路径。业务证据与工具
安装身份分别核查。共享记忆库继续用 `--home` / `--memory-home` 指定，项目根不改变记忆库。
搬迁项目或改版使旧 context 的路径/摘要失效时，重新显式准备并核验新绑定；本续接器不重写旧记录。

## 调用前与结果接收

用 `oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" project context` 取当前 Project。固定 project_id、revision、完整文件 SHA256、业务对象修订、条件、待决定事项、验收要求、未满足前提、原始输入与预算。工具版本不能代替业务对象版本，无当前 Project 时明确选择或建立，不从目录名猜。

| 工具 | 可提供的中间依据 | 必须保留的边界 |
|---|---|---|
| REA | 发布制品的限定结构、原生 Evidence 与授权运行观察 | 精确制品及工具身份；静态推断不写成动态事实，反编译表示不冒充原源码 |
| LeanToken | 当前源码实现、测试位置与有界范围 | 提交与脏文件摘要、索引 generation、实际返回字节；找到测试不表示已运行 |
| GitContribute | 来源、相关工作、贡献规范和覆盖 | 来源版本、时间、缺失与冲突；无命中不表示不存在，匿名访问也可能联网 |
| Jacobian | 明确数学前提下的关系 | 精确合同、输入映射、单位、标量类型和独立验算；数学成立不等于工程适用 |
| SmokingGun | 复杂度候选和已导入的性能比较 | 理论、实测、行为与采用分开；Rust 结构覆盖不等于完整语义分析 |
| Flameox | 明确工作负载的采集和运行原件分析 | 完整/部分状态、预算、生产者、重放与清退；一次采集不证明因果 |

1. 原件写成 `oev2.subject-manifest/1`，工具原生输出保存到当前 Project 私有区，再写 `oev2.tool-result/1`。实际工具身份与本机 `var/capabilities/registry.json` 相符；观察、推断、导入与未知分开。
2. 用 `project tool-result verify/register` 重读原件、核对象修订/条件/来源，登记为新的 Project 修订。结构化失败和部分结果保留，不能支持肯定决定。
3. 仅经 `project decide` 从已登记并重新核验的结果更新决定。来源或对象变化后，旧结果不得继续支持当前决定。性能比较先登记基线和候选，再用 `project tool-result compare`。
4. 交付前按原生 `project route-audit` 核可交付范围。`adopted=true` 只经 `project adopt` 的独立本地批准与 `verify-adoption`；结果接收器不授予采用。
5. 依赖必须来自实际关系，不为凑齐六个工具而杜撰。变化后使用 impact/reconcile；保留旧影响与支持历史，当前采用转待审。新的业务证据和当前审查才支持复核。

完整原生结果字段及命令见根目录 `references/professional-capabilities.md`。

## 检查 Agent 准备的 Project 修订

对象、条件或决定定义编辑后，`verify_candidate.py` 接收旧、新 Project 和 `{证据ID: 绝对原件路径}` 来源定位表。它用固定缓冲区核全部原件，保存输入快照，实际执行 Rust `project validate`（前后两份）和 `project reconcile`，再核原件与输入未变。它不解释工具输出，不允许以此绕过 `tool-result`/`decide`，不修改当前 Project。

```sh
python3 "$ONTOLOGY_ENGINEERING_ROOT/integrations/bindings/verify_candidate.py" \
  --before /absolute/old-project.json --candidate /absolute/new-project.json \
  --sources /absolute/source-map.json
```

每次调用生成新的私有回执目录；失败也保存已取得的输入、原生输出和原因。

## 明确当前对象的复核调用

按根目录 `docs/REVIEWED-RECONCILIATION.md` 准备已有 /1 或 /2 请求。当前调用必须给出当前 Project 路径；请求中的 current 快照必须与它逐字节相同。历史回放只能明确选择 `--historical-snapshot`，其结果标记 `historical_only`。

```sh
python3 "$ONTOLOGY_ENGINEERING_ROOT/integrations/bindings/reconcile_reviewed.py" \
  --request /absolute/review-request.json --project /absolute/current-project.json
```

可加 `--memory-context ID --memory-context-sha256 SHA256 [--memory-home /absolute/store]`。包装器通过同一安装的原生 memory get/verify 核同一 Project 身份，审查前后均验证；该绑定不能用于历史模式。memory 默认在本根 `var/memory`。

Rust 真正重放当前内核审查，保留原始 `legacy_impact`，分别列出 `review_satisfied` 与 `still_requires_review`。未复核上游仍阻止下游，unknown/refuted 不被包装器改成 supported。复核通过不授予采用。

三个包装器均保留 90 秒和每个子进程合计 8 MiB 输出上限，可通过 `--max-seconds`、`--max-output-bytes` 降低。期限用于子进程执行，文件哈希与系统调用仍受实际存储性能影响，不承诺整个命令硬实时终止。超时或超限时保留输出前缀、原因、退出码及本任务进程组清退回执；没有核实清退不报告完成。仅清理本次启动的进程组。调用完成后复核 Project、请求及已选可执行程序身份，拒绝期间漂移。

## 显式记忆续接

原生记忆流程保持 `session prepare/handoff` → `project memory-candidate verify/register` → 实际使用 → `project memory-feedback` → `session feedback`。记忆候选不能支持肯定/否定决定、不能关闭验收，不替代工具结果。记录 no_delta、无帮助或有依据的效果，不把曝光或召回次数记作收益。

同一任务需要从准确旧上下文重新准备时：

```sh
python3 "$ONTOLOGY_ENGINEERING_ROOT/integrations/bindings/resume_memory.py" \
  --project /absolute/current-project.json --context-id ID --context-sha256 SHA256 \
  --provider codex --session-id SESSION --output-root /absolute/private-runs
```

也可显式传 `--home /absolute/store`，不省略原来的共享存储选择。入口核原 context SHA、provider、session、完整当前 Project 绑定，执行原生 get→verify→prepare→verify。只改变新 context 的 ID 和 phase，保留原 task/query、对象、条件、跨项目策略与预算；不 activate、不改 Project、不授予采用。新任务或否定文字不被改写。每一步均核安装选择、真实程序与 Project 未变。旧背景失效则拒绝，不偷偷更新哈希使其继续生效。

原生短句 hook 与这个显式入口职责不同：hook 使用激活任务和 hook policy；显式续接保留被选原请求的策略和预算。六种“继续优化”短句及否定/对象/预算变更边界已在本项目 memory 源码中保留。

## REA 启动环境

当前本地 REA 有一个环境启动器，程序算法仍为固定 `rea-agents@6.5.0`。可移植对应物为 `rea-launcher.cjs`。新用户选择自己的 Node（原运行环境为 24.18.0）、固定 npm 包目录、Ghidra（原运行环境为 12.1.4）和 JDK ≥21，不复制旧机路径：

```sh
export ONTOLOGY_REA_PACKAGE_ROOT='/absolute/installed/node_modules/rea-agents'
export GHIDRA_INSTALL_DIR='/absolute/ghidra-installation'
export ONTOLOGY_REA_JAVA_HOME='/absolute/jdk-home'
/absolute/node-24/bin/node "$ONTOLOGY_ENGINEERING_ROOT/integrations/bindings/rea-launcher.cjs" --version
```

保留已满足 ≥21 的 JAVA_HOME；否则使用明确给定的 ONTOLOGY_REA_JAVA_HOME。PATH 优先当前选定的 Node 目录。启动器不查旧安装、不自动安装后端，错误不能用静态小样例成功遮蔽。它的配置测试不代替真实 Ghidra/目标格式验收。直接使用上游 CLI 时也需要等价的已配置环境。

## 新安装检查

在自己的新 checkout 完成安装后执行：

```sh
python3 -m unittest discover -s "$ONTOLOGY_ENGINEERING_ROOT/integrations/bindings" -p 'test_*.py'
node "$ONTOLOGY_ENGINEERING_ROOT/integrations/bindings/test_rea_launcher.cjs"
```

测试使用该根的真实 Rust 程序、隔离存储及自有合成输入，不需要维护者的历史案例。受控假进程测试只验证预算和清退；原生测试验证其明确范围。外部专业工具的功能验收仍按 `docs/TOOL-INTEGRATIONS.md` 分别执行。没有安装原生程序时测试应失败，不把跳过记为通过。
