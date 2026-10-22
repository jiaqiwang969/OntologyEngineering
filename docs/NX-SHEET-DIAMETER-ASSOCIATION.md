# 单圆直径的整圆关联合同

当前NX2412在有源单圆直径调用中回读UF_DRF_NO_POINT_TYPE=0，原父圆、成员视图和ComputedSize均可独立核对。输入UF_DRF_arc_center与输出UF_DRF_object_assoc_point_type不是同一概念；不能从线性圆心标注推断径向尺寸必回读ARC_CENTER_POINT_TYPE=2。SDK的uf_drf_types.h保留两类枚举及绝对assoc_point定义，实际安装版本和观察绑定于维护回执。

显式intent/params/check/reopen `/2`按整个圆对象处理0，仍要求唯一完整圆边、未缩短视图、26/10类型、原名义直径/单位、真实父圆及所属视图。追加检查关联位置有效、坐标基准为UF_DRF assoc_point absolute coordinates，并在独立中心容差内与原圆心一致。缺位置、错坐标、其他关联类型、外来或歧义父圆均拒绝。

原 `/1`始终要求圆心点类型2，原失败继续拒绝。`/2`是有来源的语义扩展，不改写旧回执，不归一化原生类型，不通过扩大数值容差取得通过。创建与保存仍在副本，原几何和旧尺寸保持，独立进程重开按新字节重新选择父圆和尺寸。图面、制造与采用另验。

复用原共享SheetDiameterIntent，specs正本、真实ggen、手写Rust、Windows SDK和Mac接收分别核验。软件测试使用独立合成样例和失败回放；原生有效性须新任务接收。开发曝光不构成独立泛化或RRSI净收益。原63项、完整Semantica/CAD/制造/计量范围保持。
