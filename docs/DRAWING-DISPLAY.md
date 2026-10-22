# 有源图页显示调查

原 PDF/1 与 identified/1 的文本、来源、恢复及接收含义保持。显式新动作
`export_display_drawing_pdf` 使用 `oev2.nx-drawing-display-pdf-params/1`
和 `DrawingDisplayPdfIntent`，由原上下圆心父引用派生两行附注：
`UPPER/LOWER CENTERS` 与 `MODEL Z=<原名义坐标> mm`。它不是制造公差。

同一 Rust 几何、原尺寸关联、完整清单及失败恢复 owner 执行这个显示试验。
原生边界读取可见/裁切对象、对象类型与所属零件、显示状态、持久坐标系原点和
方向阵，以及当前 WCS。未知状态保留。UF_OBJ 的坐标系循环不包含未保存的 WCS、
临时或休眠对象，因此另外读取 WCS；清单不表示所有渲染元素或工程语义已知。

先以空 `hidden_csys` 调查，实际源文件中的坐标和方向阵形成后续选择依据。
显式选择必须唯一、alive、自有、非 WCS 且当前未 blank；最多16项，不复用历史
进程 tag，不删除、刷新、重建或保存模型。每项变化回读并在错误后仍尝试恢复。
两行文本、WCS和选择项恢复，以及源/副本字节和全部原几何/尺寸分别复核。
若原状态、恢复或身份未知，本次不得通过。

`cad drawing receive-pdf` 用显式接收/3复算这个动作；旧/1–2保留。
Mac Rust、同源 Windows worker 与安装 SDK、真实原生运行分别接收。
真实 PDF 的纸幅、文字、箭头、引线与三轴是否消失仍由独立解析/图面检查判定。
模型属性调用成功不能替代图面通过；源码、回执、正式选用状态分别保留。

规范正本为现有 `specs/modules/nx-sheet-dimensions.toml`，生成由锁定 ggen 完成；
实现为 `drawing_display`，执行与恢复复用 `drawing_identification`。
测试保留原反例并覆盖来源、歧义、WCS排除、部分失败、恢复及接收漂移。
原制造检验、保存重开、最终计量、63目标及完整Semantica范围不变。
