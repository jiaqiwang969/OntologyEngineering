# 圆柱错误角色的有限恢复

`cad nx worker verify-current-role --worker-directory ...` 在本地核文件完整性和当前 native worker 的声明角色、执行路径及支持的 Windows target。它复用 worker 身份 owner，不调用 NX 或远端。原 `worker verify` 继续只报告 `package_complete`；文件完备、声明角色匹配、PE 实际编译身份、SDK、几何和工程采用分别验收。圆柱 prepare/stage 在运输前核声明角色，Windows worker 仍核自己的实际编译 target。

圆柱包装曾把 `worker_id` 写成 `oe-nx-rotating-part`，固定 worker 合同要求 `oe-nx-native-acceptance`。原程序在 `Native::load` 前正确拒绝，原 job 的 `UNKNOWN_AFTER_DISPATCH`、失败 PE、输出、进程和源码原件保留。这不属于几何失败或原生通过。

独立审查显式使用 `cad-agent.nx-dispatch-writer-release-review/v5` 与 `cylinder_worker_role_identity_guard_before_native_load`。仅覆盖原 MSVC `worker/oe-nx-worker.exe`、`create_cylinder_solid`、`oev2.nx-rotating-part-params/1`、本次精确错误角色。v1–4 的旧事因和行为不变，不能借旧 assembly review 接受新圆柱失败；v5 不能重释旧事因。

当前 controller 要重新核原完整 job/dispatch/source/PE、精确 stderr、空 stdout/out、退出后 launch diagnostic、实际 exit 1、完整目录与文件 snapshot、原 worker/supervisor 创建时间及 fresh OS 不活观察。原 review 的 source/DLL 字段名保留：本次来源说明必须证明尚未进入 SDK 加载，不把新的 controller 或 worker PE 当成原失败程序。任何输出/native 事件、漂移、仍活或未知进程都拒绝。

这只允许同一 root 中不同新 job 的 writer 准入。它不修改原状态、不补造 result、不重跑原 job，也不授予 SDK、圆柱 create/save/reopen、实际零件设计、材料或制造资格。当前新源码与最终同源 Windows controller/worker、四个新原生任务和独立新进程重开仍分别取证。
