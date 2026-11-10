# 图样版本候选与改正文件写出

`MdEngineeringExpressionGraph.revisions` 保存同文件内的图样身份、版本候选和源图元差别。`revisions.rs` 只读取源字节及本次分类所得标题栏与图框；同图号、同名称、不同日期且较晚图纸有可归属的修改说明，才建立版本关系。日期排序给出的是现行候选，不是发布批准。无候选时为 `null`，既有表达、成员、视图和结构线不改变。

知识卡 `drawing_revision` 对应 DC-G9-018、019、023。图号、名称、更改区的含义据 GB/T 10609.1、GB/T 14689 及《机械制图》第7版 §1-1、图1-2/1-3。按日期推断候选、几何配准及对应搜索属于有限算法，不把它们写成国标的发布规定。对齐文字的原生点按 [Autodesk TEXT group codes](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-62E5383D-8A14-47B4-BFC4-35824CAE8363.htm) 解释；该文档只决定 DXF 编码语义。

先以图框对齐，再由重复几何支持局部平移，产生相同、移动、样式改、几何改、文字改、新增、删除七类对应。重复对象保留重数；分段对应可一对多。句柄是原件定位，不是编辑历史的证明。页面默认选择现行候选，可切换版本、展开依据及每条差别，定位到任一来源。

`correction.rs` 接收生成的 `MdDrawingCorrectionPlan`，仅执行与 `certain` 条目绑定的操作；它不决定制图规则是否被违反。计划绑定源 SHA-256；编辑既有实体还需 `record_sha256` 给出的原生记录摘要。成功返回改正字节及 `MdDrawingCorrectionReceipt`，包括原件/改正件摘要、操作到条目的对应、未改条目、新句柄和可重放的字节区间。

- 加 TEXT：指定内容、位置、字高、方向和已有图层；Unicode 用 DXF 转义写入。
- 样式：限定已声明样式组码，图层和线型引用须存在或为内建值。
- 移动：LINE、TEXT、MTEXT、CIRCLE、ARC、POINT、INSERT、SOLID 的平面坐标；不做隐式坐标系或单位转换。
- 删除：拒绝仍被引用的对象及未支持的复合记录；不删除块定义。
- 嵌套 INSERT：按根句柄及块内句柄路径编辑，沿路径复制共享定义及 BLOCK_RECORD，分配新句柄并改写内部 owner。其他实例和原定义字节保留。两项操作不能争用同一叶记录，父子编辑不能混用。

外部引用、附带扩展对象/数据的块、属性序列、尺寸缓存直接编辑及其他未实现的几何变换明确拒绝。接口是有限的改正写出器，不是通用 CAD 编辑器。块所有权与句柄分别按 [BLOCK](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-66D32572-005A-4E23-8B8B-8726E8C14302.htm)、[BLOCK_RECORD](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-A1FD1934-7EF5-4D35-A4B0-F8AE54A9A20A.htm) 和 [HEADER HANDSEED](https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-DXF/files/GUID-A85E8E67-27CD-4C59-BE61-4DC9FADBE74A.htm) 的编码定义维护。

`write_new` 先在内存完成并核对，创建自己的临时文件、同步，再以不覆盖目标的文件链接发布。原文件和已有目标不写；失败不留下半个目标。`verify` 重新解析两份字节，核摘要、区间重放、允许的修改范围、未选记录和克隆兄弟的逐字节相等、全部新句柄、选定出现路径及操作后置条件。

`route::route_with_correction(root, request, plan_path)` 在原路线的图检查和页面之间增加 `write-corrected`、`read-corrected`、`render-corrected`、`check-corrected` 四步。字节 worker 只收到改正件字节，不接收计划、旧图或回执；渲染器必须已通过固定来源核验。新增文字必须出现在渲染来源中。没有计划的原 `route` 入口保持原步骤。

当前提供版本页面与写出执行能力。由制图规则本体产出问题清单、自动生成改正计划、默认路线接入和页面“改正后”模式，是后续规则生产者的接入工作。合成测试 `synthetic_correction_steps_write_read_render_and_verify` 可直接验证四步，不创建 Project；需显式给出已构建 worker、固定渲染器记录及新证据目录。执行检查不等于工程验收。
