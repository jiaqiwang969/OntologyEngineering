# Fusion 多实例与多 Agent 建模

用于同一台 Mac 上有多个 Fusion 窗口、用户希望多个 Agent 分工建模的情况。
先确认独立进程，再绑定工具调用；窗口数量、相同应用名称和不同 Python 会话都不能
单独证明隔离。本页补充 [Fusion 执行规则](fusion-execution.md)，不改变保存、超时、
quarantine、owner lease 或共用 safety state root 的要求。

## 每个任务绑定一个实例和文档

每个 worker 持有一份冻结的选择：

```text
FUSION_CAD_UPSTREAM=http://127.0.0.1:<实际监听端口>/mcp
FUSION_CAD_EXPECTED_PID=<刚核验的 Fusion PID>
FUSION_CAD_EXPECTED_START_TOKEN=<该 PID 的精确 libproc 启动令牌>
FUSION_CAD_PEEKABOO_MODE=local
```

前三项必须完整且一致。地址只允许精确的 IPv4 loopback `/mcp` URL；不能用主机名
别名、查询参数、转发端口或轮询候选端口来选择实例。首选项的“首选端口”可能与当前
实际监听端口不同，以 OS listener 属主和进程身份验真为准；PID、令牌和端口在重开后
都需重新发现，不能把某次会话的值写成技能常量。

默认未选择实例的路线仍要求单一 Fusion PID 和默认端口。只设置 `--fusion-pid`
不启用多实例路线。完整选择存在时，外部 preflight 只针对该 PID：

1. 核对 Fusion 进程列表、监听器属主、可执行文件和高分辨率启动令牌。
2. Peekaboo `list windows --pid ... --include-details off_screen,bounds,ids`
   使用 `local`，核对返回的 `targetApplication.processIdentifier`。
3. 每个返回的 CGWindow ID 都必须由该 PID 所有；CG 普查前后再核验完整实例身份。
   旧实例的窗口、缺失属主、令牌漂移或监听器移交都拒绝就绪，不能退到其它模式。
4. 继续检查同一 canonical safety state root 的 marker 和原有保存义务。

仍显示的首选项面板 `Preferences`／`首选项` 是调用阻挡条件，即使同一 PID 仍有
建模窗口也不能视为就绪。Fusion 可能保留已关闭窗口的 CG 记录；oe.8 起仅对这两个
确切原始标题，重新证明完整窗口属主、进程启动身份、三次 CG 元数据、两次 ordered-out
状态，以及唯一主画布的两次 ordered-in 状态后，才忽略该保留记录。窗口在后台或其它
Space 本身不是放行依据。私有只读 macOS 排序接口缺失或返回不确定值仍会拒绝。

oe.9 对充分证明身份、标题、尺寸和排序状态稳定的首选项／主画布共同坐标平移，
在原有最多 3 次和共享期限内重新获取完整 Peekaboo 普查；最后所有坐标必须精确相符。
它不接受带偏移的证据、不缓存豁免、不新增持久状态。其它异常仍立即拒绝。
外部预检遇到这种漂移仍保守返回未就绪。后台按钮无法操作时保持该实例待命，
不靠健康检查或建模请求试探面板是否挡住主线程。

这些都是 MCP 外部证据，不是文档或模型回读。真正调用仍经过一次性 guarded cell，
由其重新核验动态身份、窗口、owner lease 和保存状态。不能直连任何实例的 MCP 端点。

任务还要绑定独立文档：精确 name、`dataFile.id` lineage、版本以及该步的原生断言。
新文档使用 F1 的无覆盖创建、首次保存与新请求回读；已有用户文档不能仅凭活动窗口
标题当成测试文件。脚本进入后核对目标身份再改几何，修改与保存放在相应受控步骤内，
另起请求回读保存状态和关键特征。两个进程操作同一个云文档也可能发生版本冲突，
不要把进程隔离当成文档隔离。

## 调度边界

| 情形 | 当前执行方式 |
|---|---|
| 同一个 Fusion PID，多个 Agent | 保持一个持有 owner lease 的 cell；任务串行，不能靠改 state root 绕过互斥。 |
| 不同 PID、不同端点、独立文档 | 各 worker 固定完整选择，通过普通 `fusion_ops.py` 调用分工；一次调用一个回执。 |
| `fusion_document_f1.py` 与基于 `_common.GuardedCaller` 的工具 | 仍有全局 busy 检查；先依次完成预检、创建与保存，再安排不同 PID 的普通建模调用。 |
| 同 PID 的多个文档窗口或标签页 | 不能当作独立 worker；仍受同 PID 的互斥、活动文档与窗口证明约束。 |

将环境变量放入各 worker 的独立子进程环境，不在共享 shell 中来回切换一个全局
“当前实例”。F1 会把同一选择冻结到所有阶段的 child environment。输出目录和 receipt
stem 也要按 worker 区分，但 safety state root 必须共用，不能按任务或实例另建根目录。

并行有效性要由本机结果证明：请求执行时间确有重叠，各 worker 的保存与独立回读
仍匹配自己的文档 lineage、版本和几何断言，对方文档没有混入特征。仅 HTTP 成功、
两张截图或各自返回 `success=true` 不能支持这项结论。小测试通过也不证明任意大型
装配、插件或 UI 命令都能并行；按涉及的工作继续验证。

### 本机双 Agent 验收（2026-09-24，R2）

两个实际 Agent 分别控制 Fusion 2705.1.15 和 2705.1.25 的独立进程与独立文档，
通过 canonical `fusion_ops.py` 完成两轮。首轮各建 96 个台阶实体，次轮各新增 48 个，
最终各 144 个实体／288 个草图／288 次拉伸。A 为台阶方块，B 为台阶圆柱。

原生脚本内部建模区间分别重叠 55.708 秒、69.064 秒，两边都有在交叠区间实际完成
特征的进度记录。四次建模、四次独立回读均首次成功并确认 ACK；两轮分别读回保存
且 clean 的 v2、v3。逐体名称、位置、包围盒、解析体积和特征集合通过；A 还核对
台阶顶点。第二轮后两侧原有 96 个实体保持不变，原有项目文件的保存版本和原生几何
摘要也与测试前一致。前台采样期间只有用户终端，没有 Fusion 前台样本。

证据包标识：`fusion-parallel-acceptance-20260924-r2`，机器判定为
`PASS_TWO_ROUNDS`。本次可支持独立零件的并行分工；没有串行基准，不报告加速倍数，
也不代表同一文档并发编辑、长期运行、复杂装配、任意 UI 或 CAM 已验收。

## 优先后台观察，不打断前台

用户同时使用电脑时优先用 Peekaboo 按 PID 的只读 `list`／精确窗口 `image`，并核对
实际返回的窗口及 OS 属主。截图请求中的 PID/window ID 不是返回内容的身份证明。
`local` 指 CLI 的执行方式，不等于任意 UI 动作都有可靠的后台语义。

默认禁止 `activate`、`NSRunningApplication.activate`、bring-to-front、焦点抢占和
Space 切换；不要为让检查通过而先把 Fusion 拉到前台。后台读取或控制失败时保留
拒绝原因，不能自动降级为前台操作。需要前台交互的步骤只有在用户明确允许后再执行。
模型操作优先走已绑定文档的 Fusion API；屏幕点击、快捷键和应用名称匹配不能替代它。

当前工具边界：

- **Peekaboo 3.2.1**：当前守卫使用已经核验的签名与只读窗口证据路径。
  不由此推断所有后台点击、模态控件或多实例 UI 控制均已可用。
- **Peekaboo 4.5.0**：已单独验证精确后台截图及官方构建专属 producer。首次 Fusion 首选项
  AX 控件未出现，动作没有成功；后续只读观察取得可操作的命名按钮，在绑定 PID、
  启动身份、窗口和新 snapshot 后，一次 `actionOnly` 后台操作成功关闭面板，并用新
  AX 与窗口排序证据确认。动作的 `dispatched_unverified` 回执本身不算完成证明。
  这只验证了该控件，不能泛化为任意 UI 命令。v4.5 的后台点击默认不带焦点参数；
  旧版 `--no-auto-focus` 被其解析器视为 foreground-only 选项，不能混用。

某个 PID 发生上游超时或歧义后，对该 PID 的 read/save/close/健康探针等限制全部保持。
使用外部 Peekaboo/OS/AppLog 证据按原 runbook 分诊；换端口、换 worker 或创建新 state
root 都不能解除该 PID 的 marker。其它独立实例是否继续，仍按各自身份和原有守卫检查。

## 源码借鉴与本地实现分开记录

本轮社区源码中，[frankhommers 的请求调度与取消状态机](https://github.com/frankhommers/autodesk-fusion-mcp/blob/ff5f43afb8cc150ba59f54f2d0e91d1187e67683/fusion_bridge/dispatch.py#L129-L325)
和[跨会话取消测试](https://github.com/frankhommers/autodesk-fusion-mcp/blob/ff5f43afb8cc150ba59f54f2d0e91d1187e67683/tests/test_session_cancellation.py#L97-L155)
可供队列和取消隔离设计参考。它们属于固定 commit 的上游实现；本技能本轮采用的是
自己的进程／监听器／窗口／文档绑定，不能据此声称已经移植该状态机，或把已开始的
原生建模操作报告为已取消。具体版本的本机验证状态见
[Fusion release compatibility](fusion-release-compatibility.md)。
