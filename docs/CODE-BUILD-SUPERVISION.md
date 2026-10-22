# 同一版本管理中的构建来源

工程本体论规定含义 → ggen 生成合同 → Rust Semantica 实现执行 → RRSI 根据反馈检验改进。

本项将“产物从哪份源码构建”纳入现有受控版本管理。设计正本为 specs/engineering.toml，共享类型与端口由锁定真实 ggen 产生；算法复用 oe-evolution 的独立 builder、五阶段实际构建和 oe-core 的有界进程。

新入口 `oev2 versions supervise-build --input var/...` 的请求只引用已有 binding、修订、范围和 actor。显式 `controlled-version-authority/3` 在调用前绑定 `code_build_supervision` 私有上下文，其中选择 frozen_export 源码条件、builder、cargo/rustc/ggen 原字节身份及预算。旧 /1、/2 不自动获得监督构建权限；actor 还须有 `supervise_build` scope。

现有 selection.lock 监督执行，记录保存在同一 binding 的 build-observations 中，不加入原只允许 activate/restore 的 selection 事件。真实 builder 必须完成 normalizer、ggen、runner、ggen-check、identity；核验原始输出与完整 descriptor/capture，并重新核 policy、context、source 和 tool。失败保留实际进程及未知。

`owner_observed` 表示本地已有 owner 实际监督了这次构建；不认证敌对代码的OS隔离，也不让公开结果 JSON 自行授予资格。原 runtime probe 对调用者选择的 capture 仍报告 untrusted/origin unknown。本轮是一次实际 debug 构建的来源能力，两个独立构建的字节复现、全13494原行为、原19门槛、跨平台、版本启用/使用/Project反馈/恢复及工程采用仍各自需要证据。

后续 RRSI 比较须先冻结目标、语义、独立预期和范围，再沿同一链提出候选和重新验证；不能因为构建成功修改目标或放宽原验收。

## 显式 /2 构建预算

`oev2.code-build-supervision-context/1` 保留原 `build` 请求与分派预算语义。已有 /1 context 的原工具与来源身份不自动成为 /2 的批准。新 /2 context 仍须由同一 `controlled-version-authority/3` 预先绑定；它选择 `build-budgeted` 命令，接受由真实 ggen 生成的 `CodeBudgetedBuildRequest`，沿同一 builder、`build_with_deadline` 和 `ProcessTree` 传递剩余时间与每个输出流上限。

/2 时间从 `supervise` 函数进入开始，包含规范、既有 authority/context、来源与工具核验、非阻塞 `selection.lock`、分派与产物核验。CLI 请求读取、编译身份守卫不在该入口计时内。分派前从同一剩余时间保留 250ms 作为内层本地清理和阶段回执余量；内层每个构建阶段只能取得剩余的 body 时间，不能逐阶段重置总预算。外层分派使用 owner 的剩余时间作为最后取消边界，包含这段余量，避免先于内层期限终止 builder 并丢失原始子阶段捕获。预算耗尽或清理无法核验都保留失败与未知，不返回 owner_observed。最终私有证据落盘及不可中断的文件/OS 操作不构成硬实时或总资源成本保证；250ms 是余量，不是所有平台都已经证明的清理时限。

macOS 用 libproc、Linux 用 /proc 记录本地已观察到的 PID、出生身份与父子关系，在原有 ProcessTree 内终止已经观察到的嵌套子进程，继续使用现有 Unix 进程组；Windows 复用原 Job Object。进程回执分别保存原执行终态 `observed_status` 与清理状态，清理核验失败不能抹掉 `timed_out`、`output_limit` 等原因，也不能记作成功。`local_cleanup_completed` 仅描述该后端对已观察集合的本次清理，不能证明所有可能后代：25ms 观察前快速退出并重归属的节点、其它 Unix 的独立进程组和敌对脱离仍未覆盖。新实际反例冻结正常的嵌套 run_command 行为与原预期，不构造编译来源或授权绕过；Linux、Windows 与其它 Unix 的本次新增路径仍需各自实际验收。
