# NX 几何质量属性的原始诊断

本增量接续实际复制失败中的体积末位差异，取得能够区分原因的原始观测。原copy/1严格检查、保存和独立回读判据继续保留。源码、软件、当前Windows编译、实际测量、原因判断和工程采用分别接收。

`probe_assembly_mass_properties` 使用显式 `oev2.nx-assembly-mass-probe-params/1`、原生PRT及装配budget，在没有复制或保存的流程中重复测量。`probe_component_copy_mass_properties` 使用显式 `oev2.nx-copy-mass-probe-params/1`、原复制意图与budget，在复制前、复制后及到达保存后的阶段采样；原复制检查未通过时仍失败且不保存。重复次数必须2–4，每阶段最多16样本，超限在复制前拒绝。

共享 `NxGeometricalMassObservation1` 经specs与锁定真实ggen生成。Rust native核当前prototype实体、所属part、类别及非occurrence，再用原UF接口调用 `UF_MODL_ask_mass_props_3d`。请求为实体analysis1、units4（kg/metres）、density0、accuracy模式1与11个原输入（首项0.999999）。只读取几何属性，没有材料质量假设。

47项结果与13项误差统计逐项保存为16位小写十六进制binary64位模式；体积在raw47索引1。统计含义按原SDK，模式1为相应 +/-范围估计，不能替代完整工程测量不确定度。原返回码、body/owner、进程、阶段、重复序号与来源分别绑定。非有限值以原位模式保留，不被JSON空值替代；错误和不合格观测先记录再拒绝，不用缓存、舍入或归一化制造匹配。

不变状态控制中出现变化，只支持相应计算非重复性的判断；复制后才出现变化时仍需区分数值与几何原因。真实改型控制、覆盖及保存/独立重开另验。诊断动作不替代旧copy/1谱系接收，不继承工程采用。

Project/work/Harness保存假设、冻结动作、实际观测与有范围的决定。已有方法足够时记录实例/no_delta。开发反例不证明 RRSI 净收益，原63目标、CAD16、完整Semantica及制造计量继续。

责任与平台：`geometry::mass_probe` 检查请求、预算和记录，`native::mass_probe` 负责Windows UF采集；原 `component_copy` 管复制检查与保存；Project/reconcile/work/Harness与正式接收沿原owner。macOS软件和运输不代替当前Windows测量。见[原复制范围](NX-COMPONENT-COPY.md)。
