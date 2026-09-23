# Fusion 受保护调用

Fusion 本机 MCP 端点由 Fusion 提供。本模块只经 `scripts/fusion_call.py` 启动一次性守卫进程调用；不要直接连端点，不要并发驱动同一个 Fusion PID。接收方通过 `FUSION_CAD_UPSTREAM` 和 `FUSION_CAD_PEEKABOO` 指定本机配置。先运行 `bash doctor.sh --fusion` 读取预检，不发业务请求。

每次调用只执行一个明确动作。保存前确认文档身份、文件名、项目和活动状态；写入后原生回读并记录来源。退出码 0 仅表示请求已送达，须检查 `classification == "delivered_ok"`。业务错误修正输入；`guard_transient` 只能按守卫允许的条件重试；`upstream_timeout` 和其他歧义结果须冻结证据、隔离当前 PID，并用 `scripts/fusion_recovery.py evidence` 诊断，不能直接向同一 PID 再发请求。

CAD 原生回读是项目 ABox 的证据来源。它不能替代焊缝截面、检漏、承压、热或结构实测，也不能从名义几何自动推断实物密封。
