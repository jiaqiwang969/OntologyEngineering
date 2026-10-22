# 阶梯轴（隔离候选）

`oe-nx-worker::geometry::stepped_profile` 计算连续同轴圆柱段的体积、外表面积、包围盒与各段轴向区间。交换段位可能保留所有总量，检查必须同时核每段的位置。

共享意图 `NxSteppedShaftIntent`、段尺寸 `NxSteppedShaftSegment` 和创建参数由规范正本 `specs/modules/nx-stepped-profile.toml` 经实际 ggen 生成。`cad nx worker stepped-shaft-plan --intent PATH --input-member intent.json` 读取本根原始意图，生成 `create_stepped_shaft` 参数与摘要；计划不执行 CAD。独立重开动作是 `inspect_stepped_shaft`，显式参数版本 `oev2.nx-stepped-shaft-params/1`，另含输入 `.prt` 的 `part` 成员。旧圆柱协议保持。

范围为绝对坐标 mm/+Z、2–32 段，长度/直径 1e-4–1e5 mm，总长不超过 1e5 mm，原点和派生坐标绝对值不超过 1e6 mm。相邻半径差必须大于 2e-6 mm，不自动合并输入段。1e-6 mm 是软件匹配阈值，不是加工公差。

worker 在任何原生调用前核源字节和意图。NX 按每段创建新实体，从特征取得实体后明确合并 target/tool；再读完整单实体、面清单、每面的所属实体及边、全部圆边、精确包围盒、原生体积和面积。每段圆柱的两条端圆边、两端圆面、每个环形轴肩必须各自一一对应；检查不凭外观或总体积。

模型保存后重新检查；独立重开不得保存输入，须由控制器关联原保存文件字节和不同进程身份。未通过当前 Windows 构建、真实 NX 和独立重开的候选不能算原生验收完成。软件夹具及实际原生资格分别在私有本轮记录中报告，工程采用另验。全程使用 v2 自有 worker，不调用旧 cad-agent。
