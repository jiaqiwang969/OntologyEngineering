# 来源绑定的原生图页 PDF

`export_drawing_pdf` / `oev2.nx-drawing-pdf-params/1` 接受生成的
`DrawingPdfIntent`，复用原 NativeDraftingPlan、完整非 retained 尺寸清单和来源。
当前范围为毫米、单一图页及命名视图，至少一个已核尺寸；不是通用批量图册发布。

Rust 在新任务副本上核当前图页、完整标注与原字节后，调用 NXOpen
PrintPDFBuilder 导出指定图页。窄 C++ ABI 只传递 SDK 调用、选择、设置及
完成/清理观察；Rust 执行政策与检查。输出固定为 drawing.pdf，1:1 原尺寸、
黑白、标准线宽、文字模式，不追加、水印或栅格。已有输出拒绝覆盖。
不调用 CAD 保存或视图刷新；前后观测和副本/原件字节保持。

`oev2 cad drawing receive-pdf` 从完成任务、进程、工具、输入输出原件重算
来源检查。PDF 头尾检查仅为文件包络；独立解析器必须核页数、纸张与有效内容，
实际图面必须核清晰度、歧义与遮挡。几何关联有效不等于隐边可见或标注易懂。

有源受控反例、Mac Rust、Windows SDK/worker、实际原生导出、PDF 解析与
视觉检查分别记录。失败、部分输出、原CAD和旧图纸不删除；此能力不授予
制造/检验、最终计量、工程采用、正式选用或 RRSI 收益。
