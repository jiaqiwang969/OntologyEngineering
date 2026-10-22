# 当前 Rust 生命周期切片

工程本体论规定对象、条件、来源和采用含义 → ggen 生成共享合同 → Rust Semantica 执行控制与有范围的验证 → RRSI 用实际使用反馈检验改进。中性库承担原始数据语义和算法，工程 Project、授权、版本选择和历史权威仍由既有 owner 承担。

`semantics` 的默认 runtime 已从退役 Python 执行切到同一 Rust 库。旧 `NativeRuntime`/wheel/probe 仅保留历史身份，生产不恢复 Python，也不把当前 Rust 源摘要冒充旧40位 Git 或旧wheel证明。新观察明确报告实际编译输入与当前可执行文件的字节身份；编译来源、来源真实性、工程采用仍为 unknown。

当前实际 `semantics open` 可以在调用者选定的同一行业 workspace 记录 blocked/no_delta 观察。它会核 binding/task 内容与来源前后身份，记录并保存不等于四阶段验证通过，行业事实保持原状态。

`semantics propose` 的 `--engagement` 在当前 Rust 观察 profile 下接受生成合同 `SemanticRefineryObservationRequest`，字段是 schema=`semantica.refinery-observation-request/1`、私有 manifest_path、scenario_ids。agent 根据自然语言整理输入，用户无需填写内部规范。请求只描述要执行什么，不能传入自报 passed 的旧 receipt。实际执行复用 Catalog 的临时内容视图、同一 DatasetSession 和 BookRunnerState，保存正反例、receipt、数据集和 unknown 结果。非空 PackageDelta 与 manifest 的全部资产 ID/角色/字节内容和版本必须在执行前后绑定。

有条件的经验目前保存在既有 `engagements/` 的带摘要记录中；记录包含非空 delta、原因、实际检查和缺失门禁。返回 `candidate_recorded=true`、`candidate_state=not_registered`、`state=null`，不能读成原 candidate/proposed 已注册。source/actor/完整四阶段未闭合时不产生正式候选状态。

中性原控制 profile 独立保留原22 dataclass 公共合同及有限 candidate→proposed→committed 的原字节/CAS/事件语义。caller提供的 synthetic complete receipt 是原数据 oracle，不能作为生产执行证据。原行业注册表仍未晋升；verify/promote/现有非零基线、完整 subject suites、12项有序门禁、真实 provenance/rights/采用与恢复待进一步实现。旧19/22和全部13494范围不缩减。

存储复用同一 workspace 与 `.refinery.lock`，现有 Project、版本 registry、调用预算和凭据池不新增。写前校验时间及输入，文件/CAS/事件严格核字节与链，参与写者锁下发布；失败保留原因、CAS及未知副作用，未发表 stage 受控清理。历史门禁不能靠修改 pointer 或重签 passed 字段跳过。当前文件适配仅本机实际验收；Windows、敌对文件替换和任意进程崩溃恢复未宣称通过。

两本方法书、四本机械教材与米思米继续通过本地知识与专业工具入口支撑工程判断，Agent 回读来源并核适用性。已完成的工程项目保留闭环历史；从专利/视频到施工图、工艺检验和真正落地的实际设备项目仍是后续工程验收目标。当前观察不重开其交付，也不能用书籍查询通过替代现场工程合格。

Harness 当前交接由独立同基线分支维护；它只指向当前 oev2，仍是 not_invoked/current_registry_not_verified。自动从聊天构造有条件 PackageDelta、完整阶段验证、同owner选择/实际使用/学习反馈/恢复并未因本切片自动接通。组合后必须重新生成、构建和验证，不能继承分支资格。
