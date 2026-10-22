# 运行修复与源码映射

此模块保留电子 Skill 2.1.4 实际引用的本地运行修复。指南资源的上游基线为
0.25.1，运行程序补丁的上游基线为 0.23.0；不能把指南的 `.version` 当作已安装
CLI/连接器身份。补丁是当前保留的本地候选 `0.23.0-local.4`，不是上游发行。

完整补丁为 [easyeda-agent-runtime-0.23.0-local.4.patch](../patches/easyeda-agent-runtime-0.23.0-local.4.patch)，
基于官方 commit `f1aab8d2b03ab6a0bd78ce83e7961d838f4d850b`。以下路径均相对于应用补丁后的上游源码根。
本次仅保持已有修复，未把它们移植到 0.25.1；后续上游升级必须重新比较、移植和验收。

| Skill 使用的行为 | 保留的实现 | 独立离线检查与限制 |
|---|---|---|
| 修改 part 时保留未请求的 Supplier Part、OtherProperty；不从库猜实例属性 | `extension/src/component-modify.ts`、`actions.ts` 的 typed 修改路径 | `component-modify.test.ts`：省略字段、合并与非法状态；不证明任意宿主字段完整 |
| SDK 修改失败后保留原错误，最多一次 fresh 回读；完整身份缺失/漂移/超时为 unknown，不自动重试 | `component-modify-diagnostic.ts`、`actions.ts` | `component-modify-diagnostic.test.ts`：未变、部分变、异常、延迟与身份漂移；回读不是事务锁或回滚 |
| 多段 wire 解码；group-move 在写入前检查全部部件/线形，拒绝 marker 等不支持类型 | `wire-decoder.ts`、`actions.ts` | `wire-decoder.test.ts`、`actions.test.ts`；仅覆盖声明的线段形状与场景 |
| EN/CN 导入确认识别目标弹窗，一次点击；关闭不冒充工程完成 | `import-confirm.ts`、`actions.ts` | `import-confirm.test.ts`、`import-changes.test.ts`：错误/隐藏/禁用/歧义目标、时间预算、晚响应；`applied_unverified` 仍要双侧回读 |
| 导入的 action/批量步骤有明确超时预算；超时不取消 SDK 工作 | `internal/app/app.go`、`cmd_apply.go` | `call_timeout_test.go`：范围、默认/显式优先级与转发 |
| 原生 epro2、Gerber 和 PCB netlist 导出；不以 SCH/DSN 或重新序列化替代原件 | `extension/src/native-exports.ts`、`internal/app/cmd_native_exports.go`、`cmd_project.go`、`cmd_pcb.go`、`internal/protocol/actions.go` | `native-exports.test.ts`、`pcb-netlist-export.test.ts`、`cmd_native_exports_test.go`：身份漂移、SDK 错误/空文件、原字节；宿主格式、铜连通与便携文件重开未由单元测试证明 |
| `pcb list --include-properties` 保留 getter 的 undefined/空字典/有值差别与标量类型 | `pcb-component-properties.ts`、`actions.ts`、`cmd_pcb.go`、`internal/protocol/actions.go` | `pcb-component-properties.test.ts`、`cmd_pcb_inspection_test.go`：异常/非法字典、类型、未请求时不调用；不补库值，也不声明全部私有字段齐全 |
| 安装和 activate 不连接；Connect 幂等，Force Reconnect 明确；Stop 清理本 owner，pending 动作排空期间阻止新连接 | `extension/extension.json`、`src/index.ts`、`src/transport.ts` | `index-menu.test.ts`、`transport-register-error.test.ts` 及既有 transport 测试：重新求值 owner、晚响应、注册失败、清理和排空；不证明长期原生稳定性 |
| daemon 默认不替换占端口进程；显式 PID 文件与期限，退出只删本次 PID 记录 | `internal/app/cmd_daemon.go` | `cmd_daemon_test.go` 及 daemon 包：占端口拒绝/参数/记录归属；任务启动仍须显式禁止 Skill 自动更新和自动保存 |

补丁保留上述实现、协议/命令接线和全部关联新增测试，共 35 个上游文件差异。
`extension/package-lock.json` 仅恢复上游两空格格式以缩小差异；其 JSON 依赖图与本地修订一致，
没有升级依赖。包版本和连接器元数据保持 local.4；Go CLI 通过明确 ldflags 设置同版本。

[build-runtime.sh](../scripts/build-runtime.sh) 对固定 Git 提交执行 `git apply --check`，
实际应用补丁后运行 Go app/protocol/daemon 测试、全部 connector 测试、类型检查及构建。
它保留本次日志和 CLI/连接器/entry/动作目录摘要，`BUILD.json` 明确原生安装和工程操作未运行。
官方原版与补丁均不含私人项目输入；新增测试使用合成对象、模拟 SDK 和本机有限测试端点。

旧版本的安装、RC 原生回归和历史验收不随发布转移。新环境必须确认自己实际加载的 CLI、
连接器、宿主和目标文档，重新验证所需的属性、网表、导入、保存重开和服务清退。
没有该结果时保留 unknown；当前包存在、离线测试通过和菜单文案均不能补齐原生证据。
