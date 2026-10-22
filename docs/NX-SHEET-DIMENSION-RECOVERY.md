# 图页尺寸协议上报错误的有限恢复

显式 `cad-agent.nx-dispatch-writer-release-review/v6` 仅接纳 `sheet_dimension_parameter_schema_guard_before_native_load`。原worker角色正确，但 `Parameters::parameter_schema` 漏掉新action，返回旧模型空间协议；实际拒绝是 `native_worker_failed:native_action_parameters_differ`，发生在SDK加载之前。原source/PE和完整任务必须由操作员实际审查，不能仅凭stderr推断原因。

该版本核原worker角色、MSVC目标、PE成员、create_sheet_reference_dimension动作、oev2.nx-sheet-dimension-params/1参数协议，以及原manifest/dispatch/source/PE完整身份。原stderr必须精确，stdout/out为空、没有native事件或结果；退出码必须为1，after-exit launch记录绑定原job和dispatch，原worker与supervisor必须有fresh OS不活观察。任何缺失、漂移、活进程或未知观察保持持有。

共享范围由 `NxSheetDimensionParameterGuardScope` 经真实ggen生成，逻辑仍由原 `oe-nx-protocol::writer_release` 承担。v1–5的原含义不变；v5圆柱错误角色不能解释本次协议错误，v6也不能解释旧事因。

这只允许同一根下不同新任务的writer准入，不修改原UNKNOWN_AFTER_DISPATCH，不补造native-result，不重跑原job，也不授予SDK、图纸、制造或物理通过。真实source/PE审查文件和原完整snapshot由当前操作员写定后，经原review set固定；纯测试、草稿模板和此说明不构成实际释放。

版本的当前安装、实际审查及原生试验以维护status和原件为准。创建动作的软件实现、原生成功、保存、独立新进程重开和图面可读性分别验收。
