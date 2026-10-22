# 有源原生组件复制：显式候选合同

`copy_assembly_component` 使用 v2 自有 Rust worker 和原生 SDK，接续当前装配、来源闭包与事件 owner。共享合同为 `NxComponentCopyIntent1` / `NxComponentCopyOutcome1`，从 `specs/modules/cad-engineering-binding.toml` 经锁定真实 ggen 生成。现有构造 `/1`、`/2` 和对应关系合同不静默改变。

本候选以原孔板/定位销 P04 为首个接收场景：同一任务内新复制的完整来源闭包中，明确选一个直接叶实例，实际调用 `NXOpen::Assemblies::ComponentAssembly::CopyComponents`，再用 `UF_ASSEM_rename_instance` 设置新定义实例名。该 API 不更改 part occurrence 名或通用对象名；当前清单从 `UF_ASSEM_ask_component_data(instance)` 读取实例名。完整父级、原型、当前进程编号和两种参考集分别核验，不按名称推断原生同一性。

SDK 明确允许正常复制返回数量少于请求数量。因此进入调用、正常返回、结果总数、有界实际返回项、异常、实例命名和后续保存各自记录。此 profile 必须恰好一个新结果，具有新身份、原父级与原型以及相同当前位姿；原实例与全部原型几何保持。零项、多项、错误父级/原型、复用编号或名称、改动原对象、命名失败均保留失败，不重试、不用 AddComponent 补结果。嵌套或子装配复制超出本首个直接叶 profile，明确拒绝，不扁平化为支持。

复制前、复制后和保存后的完整观察分别记录；即使 SDK 返回失败，也尽可能读回并保留当前状态，无法读回时保持具体失败。两个 UF 参考集都要完整覆盖，原成员及新成员必须与来源一致。原来源文件持续核字节，所有加载对象与保存目标限定在任务副本中。保存原始返回码和逐文件错误另查；当前进程完成不代替独立新进程重开。

Rust `NativeApi` 和独立 POD shim 边界保持原 ABI；新结构与 SDK 方法签名要在 Windows 实际编译核对。旧 worker/shim 不具备新符号时拒绝，不借用原 cad-agent。调用参数格式为 `oev2.nx-component-copy-params/1`，`intent` 指向上述有源意图，`budget` 复用原 `NxAssemblyBudget`；明确描述该动作且通过新版本接收的 worker 才可执行。

来源清单、软件检查、当前平台编译、实际 NX、独立保存重开、Project 依赖重审和工程采用分别验收。软件控制和例子属于 `development_replay`；本文及代码存在不表示原生接收或正式启用。最新原件从当前维护登记定位。复制不继承原对象的工程通过状态；复制的几何、工程特征引用和采用仍由原 Project、evidence、实例对应与治理 owner 审查。

原 P05 目标孔特征删除、其余 CAD16、完整 Semantica 和制造/计量义务继续保留。删除组件不能代替删除目标孔；后续必须建立当前特征的独立选择、实际 `UF_MODL_delete_feature`、完整几何缺失、独立重开及原依赖重审证据。

显式证据接收使用 `CadAssemblyCopyRequest1`（`oev2.cad-assembly-copy/1`）及 `oev2 cad evidence verify-copy --input ... --output ...`。`previous_project` / `before` 绑定原版本，`project` / `assembly` 绑定新版本；`copy_job` 和 `copy_native_result_ref` 绑定真实复制任务。三个进程分别检查，原版本的完整PRT闭包须等于复制输入，复制原始事件按实际先后重核，全部保存产物须等于独立重开输入。两种UF参考集都要求完整原值。旧构造接收入口继续只接创建，声明型对应也不会因本类型存在自动升级。

生产caller复用原Project的`cad_focus`、`reconcile`和`impact`，重核两个版本的Project来源及当前条件，汇总旧新依赖要求重审的决定。接收命令不改写Project或授予采用；后续work/Harness仍要根据原工程验收核实结果、保留失败和未知。保存报告不能反序列化恢复verified资格；公开代码、软件fixture、实际平台编译、真实NX、分发安装与工程采用分别登记。
