# NX装配运动与位置重开

工程本体声明对象、版本、条件、完整装配来源和绝对目标位姿；ggen生成共享运动/位置/证据合同；v2 Rust及显式NX SDK执行；实际失败与下一次使用结果进入经验候选。当前安装与实机通过范围以本根maintenance/status.json指向的原件为准，接口存在不表示所有模型已通过。

`run_assembly_motion` 使用 `oev2.nx-assembly-motion-params/1`：参数绑定运动意图ArtifactReference、库存预算及证据预算。意图重读完整源闭包，全部输入先核SHA；NX只操作任务新建的私有装配。所有loaded part均须位于该namespace且属于声明，包含非placement依赖。全样本与实体对预算预检后，按当前位姿计算父先子后的绝对目标增量，检查实际完整组件树和全部solid实体对。

每帧事件持久化并绑定样本；query后必须执行SDK cleanup/update，保留原始返回码。干涉、无干涉和接触分开；缺返回、漏样本、漏实体对、预算或写入失败均不能返回成功。分段证据封口后重读原件；封口只证明记录完整。保存后的四类身份分别是工程对象、原来源、当前native会话和输出文件，不能互相替代。

`inspect_assembly_positions` 使用 `oev2.nx-assembly-position-reopen-params/1`：绑定保存后的新源闭包、此前保存位置观察的ArtifactReference和库存预算。以新job/新worker进程重开新私有副本，不请求运动、干涉查询或保存；重开前后核私有副本SHA。位置观察保持显式 `/2`。

跨进程按完整组件名称路径及原型匹配，不比较native tag或枚举序号；同父同名导致路径歧义时拒绝。位置覆盖父级映射为root或完整组件路径，检查原始覆盖类型与definition位姿；单位、实体/片体数量、包围角点及体积按几何多重集作有界比较。结果不代表全B-rep拓扑身份。独立进程资格另核controller的job、PID、创建时间和程序摘要，不能从动作名或一份expected文件推断。

旧 `/1` 库存和角色枚举不放宽，事件phase不作source_role。所有执行来自v2自己的worker/controller，不调用旧cad-agent。旧v2总文件100MiB限制仍适用，分段不绕过；显式v3按任务容量准入，见[发布与取回](NX-PUBLISHED-EXECUTION.md)。动作内部原来源/事件边界不静默扩大；长任务还需核native诊断累积及实际时间。离散帧检查不证明连续扫掠、载荷/动力学、制造合格或项目专属精度的实测。

## 无物理时间的有序场景候选

显式 `oev2.nx-assembly-ordered-scene/2` 与 `scene::plan_ordered` 以从零连续ordinal表示几何采样顺序，没有time_s字段，不推导速度/加速度。与原/1共用完整实例、工具进入、父子和姿态检查。它尚未成为原生motion动作；原/1仍要求严格递增秒数，不静默改义。

## 原生命名空间文件检查

成员检查保留canonical包含关系、普通文件/祖先句柄固定、大小上限、非空条件和file identity/size/write-time/attributes/link-count/final-path复核。非空判断直接使用固定句柄size，不再读取全PRT计算随后丢弃的摘要；来源、副本、输入和输出的完整字节摘要仍由原边界owner执行。该改动不缓存跨操作的路径或文件身份，不构成性能或工程验收。


超限诊断候选：v3任务readback超过原2MiB限制时，使用显式`oev2.nx-native-indexed-readback/1`，全部事件引用按256项分页到`event_reference_index`回执。顶层索引绑定完整事件数和每页摘要；原`load_status_records`与失败`partial_observations`从同一完整索引按phase读取。接收必须核全部页、连续序列和原事件文件摘要，不支持该版本的消费者不得按旧字段判空或通过。旧任务/小结果格式与原大小限制不变。索引包含操作结束前完整事件集合；后续operation_finished和termination保留独立生命周期记录。当前候选尚未取得Windows原生失败持久化验收。

大字段接续候选：indexed-readback/1 的 `external_fields` 精确保留原JSON字段的名称、UTF-8字节数、完整SHA256与有序块事件引用；每块最多128KiB原文本，单字段及每次readback合计最多64MiB。完整接收验证每个原事件的schema、任务、序号、路径、摘要与长度，以及每块的字段、连续ordinal/offset、最终完整字节身份，拒绝缺块、重复、字段覆盖和伪造；不得把缺字段当作空装配。`event_index::receive_readback` 先恢复字段，再验证全体原事件并恢复诊断引用，NativeResult身份/结果与工程适用仍由原接收层核验。

原readback限制为2MiB，单事件/结果记录限制为4MiB，均保持。显式v3遇到单条观察接近4MiB时使用 `oev2.nx-native-external-observation/1` 分块；它不改变原操作或失败结果。1514组件的约2.5MiB原观察本来可落盘，外置readback修复的是结果内嵌字段超2MiB问题，不把原事件写入误报为失败。小记录与旧任务继续原格式及限制；独立接收必须支持这些显式版本。当前尚待Windows构建和真实全量保存/独立重开。

选择indexed前预留16KiB实际生命周期空间，结束时重新验证该字段及最终2MiB读回上限；字段按当前合计大小从大到小完整外置，多个不足256KiB的字段累计超限也必须处理。重组由oe-wire统一严格JSON解析，真实`$serde_json::private::Number`对象保留对象含义，重复键拒绝。
