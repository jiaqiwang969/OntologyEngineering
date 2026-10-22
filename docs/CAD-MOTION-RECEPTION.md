# 离散运动原件接收

用途是把已执行的离散运动及其失败交回当前工程判断。接收本身不改变 Project 的决定、采用、要求、依赖或检验义务。

- 规范正本：`specs/modules/nx-assembly-motion.toml` 与 `cad-engineering-binding.toml`；共享合同由锁定 ggen 生成。
- 原生捕获 owner：`oe-nx-worker::native` 及 `geometry::motion_task`。仅 `/2` 记录实际参数绑定、ABI 原件、逐实体 UF 查询的实参/返回、干涉原值、清理及保存原始返回。未进入调用、仅写 pending、已返回未成功持久化分别保留。
- 流与算法 owner：`oe-nx-worker::motion_evidence::receiving2`。复用原 evidence 有界逐行读取、PairPlan、位置/变换、body binding、persistence 检查。完整 success 要有完整覆盖、封口、保存原件；实际失败与未知前缀保留已核局部结果。
- 运输 owner：`oe-cad::motion::native_receiving` 复用 `nx::client::verify_received_tree`、NX protocol 和 event-index。原始 job/params/state/dispatch/process/worker/runtime 与完整输入/输出必须相符，不能从结果自行构造“预期”。服务记录证明其有界观察范围，不补出缺失的原生调用。
- Project owner：生产 CLI 调用原 `oe-kernel::project::cad_focus`，在接收前后核同一 Project 原件、对象版本和条件。当前采用仍经原 work/Harness/审查；engineering-memory 不构成必需依赖。

`oev2 cad motion receive --input REQUEST --output REPORT` 接收 `CadMotionReceiveRequest1`。`receive-reopen` 接收 `CadMotionReopenRequest1`，重读原 motion 请求，核新只读 job 的输入就是旧保存字节，再核独立 process 的 host/pid/birth 和有限位置几何。瞬时 tag 和枚举顺序不作跨进程身份。

CLI 退出 0 只表示本次有限接收完成；1 表示 partial 或 unknown，报告继续区分；硬合同错误沿原 CLI 错误返回。旧 `/1` 只能接为 `legacy_limited`，不会升级为完整 UF 身份检查。更宽主张单列 unresolved：连续扫掠、全 BRep、动力/强度、制造图、现场检验和工程采用均不能由离散检查推得。

跨平台：流与语义接收为 Rust；NX 原生捕获需 Windows MSVC、对应 SDK/运行库与新 worker 的独立接收。当前 Mac 软件测试不能替代该 Windows/native 证据；既有 Windows2002源码回执不能沿用到新源码。正式安装继续以实际分发接收记录为准。

独立输入保留原70类别及保存边界子例；来源包括原 SDK、原NX/位置/持久化合同和既有失败原件。合成任务属于 development_replay，不作为物理工程通过或 RRSI 净收益。
