# NX 改型、图纸维护与审核交付

> Execution update 2026-09-25: retain the native API findings, geometry/drawing methods and their historical evidence scope below. Use references/nx-execution.md and direct NXOpen journals. nx_call.py/nx_bridge.sh and MCP paths mentioned below are retired, not fallback routes.

适用：长度/幅宽变化后孔列偏移、孔数不变、配合孔不对应、现有图纸过期、尺寸失关联、
注释重叠、视图越框，以及“导出最终目录里的全部图纸”。入口按任务含义检索，零件名称只用于
绑定具体对象。供应商现成件获取仍走 [supplier-cad-acquisition.md](supplier-cad-acquisition.md)。

本指南将 2026-09-07 印刷机项目的经验吸收为可复用工作方法。原生执行证据限于 NX 2412、
两块指定刮刀零件的修复和该项目的全目录导出。它没有把旧包装器全部认证，也没有把未经工程师
审核的零件变成标准件。历史证据摘要与文件哈希见
[`assets/nx-drawing-review/printing-20260907.evidence.json`](../assets/nx-drawing-review/printing-20260907.evidence.json)。

## 从需求走到交付

```text
确认参数含义、版本及改动范围
  → 盘点模型、装配实例、已有图纸与来源
  → 明确长度、孔列、配合和图纸的依赖关系
  → 在授权副本中改型并完成增大/减小/恢复验证
  → 维护尺寸关联、刷新视图和断开视图
  → 检查图纸内容与排版，保留需设计判定的问题
  → 保存并重开核查，或明确采用只读审核导出
  → 全目录逐页导出、对照、清单、合订本、ZIP 核验
```

开始时检查现有脚本已经维护了哪些依赖。本例原流程主要覆盖实体端面与组件位置变化，
未把孔数/孔列相位、配合孔、制图刷新和图面排版作为完整的后续检查。这是“模型能变长，
图纸仍有问题”的流程缺口。导入体和移动面历史本身不能说明孔距、对称规则已参数化。

按具体任务保留四个独立结果：模型几何、尺寸/视图状态、图面可读性、交付覆盖率。
工程师的尺寸、公差、材料及制造要求审核另行记录；一个结果通过不能代替其余结果。

## 参数化孔列与配合

先确定控制长度 L、孔距 p、最小端距 e、中心 c、允许孔数以及配合组。
这里 e 是端面到**孔心**的距离；孔壁材料余量须另结合孔半径/工艺约束计算。
仅当设计确认为“固定孔距、两端对称、在允许范围内尽量排孔”时，使用：

```text
N = floor((L - 2e) / p) + 1
x[i] = c + (i - (N - 1)/2) p, i = 0 … N-1
实际两端距 = (L - (N-1)p)/2
```

若 L < 2e，则该规则无可行孔心位置。偶数孔也能对称；是否必须有中心孔、固定端距、
奇数孔或禁止区属于设计约束。不要默认挪一端、拉伸孔距，或把本例 80 mm 孔距套给其他件。
配合孔列共用同一控制规则，再按各自坐标系与装配变换核对孔轴；不要由两个零件各自长度
独立算 N，导致配合组孔数不同。

本例实测回归（单位 mm；控制值不是纸带净宽，目录 `W650` 也不是参数真值）：

| 控制值 | 压板长度 | 垫板长度 | 配合孔列数 |
|---|---:|---:|---:|
| 850 | 924 | 1000 | 11 |
| 650 | 724 | 800 | 9 |
| 450 | 524 | 600 | 6 |
| 恢复 850 | 924 | 1000 | 11 |

控制压板采用 p=80、e=42。核查真实孔心坐标、孔径、孔轴和装配实例，不只看表达式 N。
本例垫板的“深10螺纹孔”标注与配合孔列不是同一个计数：孔列为 11 时该标注为 14，
需要各自绑定正确表达式。加长、缩短、孔数跳变附近、偶数孔及恢复原值都是有意义的检查。

纯数学辅助工具（不写 NX）：

```bash
python3 ~/.codex/skills/ontology-engineering/skills/cad-agent/scripts/nx_review_helpers.py plan-row \
  --length 924 --pitch 80 --minimum-end-margin 42
```

它计算规则，不替用户选择规则，也不验证实体、孔壁余量或装配配合。

## 原生图纸刷新与关联

改型流程应在模型/跨部件更新后维护已有图纸。对于原文件已有的图纸页，先保存清单和状态，
再在明确的工作/显示部件上下文中执行。此次 NX 2412 使用：

```python
part.Preferences.Drafting.DelayViewUpdate = False
part.Preferences.Drafting.DelayUpdateOnCreation = False
for sheet in part.DrawingSheets:
    sheet.Open()
    part.DraftingViews.UpdateViews(
        NXOpen.Drawings.DraftingViewCollectionViewUpdateOption.OutOfDate, sheet)
```

仍有过期视图时，按现场依赖情况调用 `UpdateViewBreaks(view)` 和
`Session.UpdateManager.DoUpdate(mark)`，检查其错误返回，再更新视图。
本例 `UpdateSheetsAndViews([DrawingSheet])` 会产生无效视图错误，不能按名称猜参数类型。

对比更新前后图纸页清单、尺寸数量、尺寸标识、`IsRetained` 和过期视图；保存的交付副本需
关闭重开后再检查。仅关掉延迟更新偏好，不足以证明以后任何编辑路径都能自动维护图纸。
当前参数改型入口要显式调用维护流程；其他 NX 手工编辑或事件触发路径需另测。

孔数量变化可能重建拓扑，使原尺寸引用断开。已明确设计意图的孔位尺寸可通过
`UF.Draw.AskDraftingCurveParents` 将模型边映射到所属视图的制图曲线，再设置关联；
数量文字可使用原生关联表达式，并读取其实际求值结果。无法唯一确认原尺寸意图时保留问题
和刷新前对照，不通过手填显示值宣称关联已修复。

同名图号、镜像件、独立文件和同零件多图纸页分别保留。零件级 retained 数量不能当作每页
独立计数。本例部分历史视图与当前模型不一致，刷新无法替代设计意图判定。

## 排版优化有接口，也有适用边界

本例的有效顺序是：按纸张可用区域安排视图 → 原生注释分布 → 重叠公差框堆叠 →
检查实际文字轮廓并做有限位置修正 → 更新/保存/重开复核。

| 已使用接口或读数 | 用途与关键限制 |
|---|---|
| `UF.Draw.AskViewScale` / `SetViewScale` | 按可用区域选择视图比例；标准比例、文字可读性和图幅选择由模板约束 |
| `UF.View.MapModelToDrawing`、`GetDrawingReferencePoint`、`MoveView` | 用模型点实际投影误差计算移动量；图纸参考点不保证是模型中心 |
| `Drafting.AutomationManager.CreatePreferencesBuilder`、`CreateDistributeAnnotationsBuilder` | 设置间隔偏好，添加指定视图并验证/提交；不是全图无碰撞保证 |
| `DraftingFcf.RemoveFromStack` / `InsertIntoStack` | 调整重叠公差框；保留其关联对象及公差语义 |
| `UF.Drf.AskDraftAidTextInfo` | 获取文字块真实角度和轮廓，处理竖排尺寸/引线文字 |
| `AnnotationOrigin` | 有界微调后必须重读实际轮廓，NX 可能自动重新居中 |

`LetteringPreferences.Angle == 0` 不代表文字未旋转。此次用真实文字块组成旋转矩形，再做
分离轴碰撞检测。尺寸、公差框、注释、几何、视图区域与标题栏是不同障碍对象；不能仅看
未旋转外框，也不能因表格接口不适用就默认表格区域可放文字。

修正前后保持尺寸值、单位、公差、关联对象和注释清单；只在当前模板准许的区域和对象中
移动。移动后重读，无法找到合适位置就报告未解决。该排版策略只在两块指定刮刀的 A3 图纸
和上述参数回归中验证；本次全目录另发现 15 张越框/裁切等排版问题，因此不能称为全机
自动排版已经完成。

其他 NX 2412 教训：`TaggedObjectManager.GetTaggedObject(tag)` 才是此次验证的解析方法；
体/边范围用 `UF.ModlGeneral.AskBoundingBox(tag)`；不要在批量扫描里试调用已触发原生崩溃
的 `Annotation.GetViews()`。通过所属视图和关联信息确定上下文。版本变化先读本地 SDK、
做小范围试验，再放大范围。

## 批量导出与审核包

“全部图纸”要求盘点最终目录全部 PRT，而不只是打开总装后加载的部件。本例总装可达
264 页，全目录有 320 页；另有 56 页来自未被总装引用的文件。八色机扫描 546 个 PRT
未发现内嵌制图页，应说明缺少图纸，不以空白 PDF 补数。

Windows 身份比较统一规范化路径和大小写；原名用于展示。本例曾因 `normcase()` 后与
混合大小写原名比较而误标“独立文件”。用原生加载清单重新关联，并保留原始记录和修正说明。

无界面 NX 已能原生导出 PDF。此次每次 `PrintPDFBuilder.Commit()` 带来约 30 秒转换器
启动开销，批量时先逐页导出 CGM，再一次调用安装版 `cgm2pdf.exe`，可减少重复启动：

```python
b = part.PlotManager.CreateCgmBuilder()
try:
    b.Scale = 1.0
    b.Size = NXOpen.CGMBuilder.SizeOption.ScaleFactor
    b.Colors = NXOpen.CGMBuilder.Color.AsDisplayed
    b.OutputText = NXOpen.CGMBuilder.OutputTextOption.Polylines
    b.VdcCoordinates = NXOpen.CGMBuilder.Vdc.Real
    b.SetFilenames([output_path])
    b.SourceBuilder.SetSheets([sheet])
    b.Commit()
finally:
    b.Destroy()
```

安装版转换器实测用法为 `cgm2pdf.exe output.pdf s0001.cgm s0002.cgm ...`。
短 ASCII 临时名减少编码和命令长度问题；页序清单必须绑定原文件/图纸页/版本及 CGM 哈希。
分批大小由命令行长度和实测容量决定，不能把任意数量塞进一个进程。

`OutputText=Text` 的试验曾丢失中文，即使文字数量仍相同。`Polylines` 在两个对照页上
通过同尺度栅格比较及中文、公差符号查看；这是样本验证。轮廓方式保留外观，文字通常不再
具备普通 PDF 文本的检索能力。新字体/图幅/版本仍需抽样比对原生直接导出结果。

如果要导出历史视图与刷新后视图，分别保存状态；只读审核导出不保存 PRT，记录源文件前后
哈希。用户要求持久化改型时，另在交付副本保存并重开，不能用审核导出替代这一步。
独立 journal 持有本任务配置和 PID，串行访问 NX；连接中断先查收据，避免重放尚在执行的写操作。

PDF 可打开、有线条、像素非白，都不保证有工程图内容。此次 59 页只有图框/模板内容，
在刷新前后均为空，须作为“需确认或补图”保留。中间区域像素统计只用于筛选，整页查看
才能排除偏置视图等误判。还要检查长件断开视图被裁切、比例、文字重叠及中文符号。

审核包保留逐页 PDF、可填写且带相对链接的清单、异常刷新前对照、带书签的合订本及哈希清单。
引用、独立、空白、关联异常和排版异常可以分别统计；类别重合应去重后算总问题页数。
本例最终为 320 页（261 页有图面内容、59 页空白模板），28 张关联问题与 15 张排版问题
重合 1 张，故 42 张对照图、101 个重点页。工程审核状态保持待审核。

对采用本次标准导出目录结构的解压包，可运行只读核对：

```bash
python3 ~/.codex/skills/ontology-engineering/skills/cad-agent/scripts/nx_review_helpers.py audit-packet /path/to/review-package
```

它核对文件哈希、全目录图纸清单、Windows 引用身份、原生导出与 PDF 的绑定以及已知问题
是否被标明；不重开 NX、不重新渲染 PDF、不判断未被记录的设计问题。缺少记录或错配退出 2，
退出 0 只代表这些包内检查通过。XLSX 链接、页数、书签、合订页与单页的渲染一致性仍由
导出工作流实际核对。跨平台解 ZIP 时规范化反斜杠并拒绝越界路径；合成 PDF 缩略总览时
用栅格缩略图或内存流，避免保留大量源文档文件句柄。

## 怎样积累成内部可复用模板

每个模板逐步绑定：部件身份/用途、规格及适用范围、源模型版本、参数关系、配合对象、
图纸模板、执行方法、回归证据和工程师审核状态。这样下一次可以按用途和规格检索，再在
已验证范围内复用。McMaster 式选型可以作为使用体验参考；当前整合增加的是流程知识、
NX 技巧和离线辅助工具，尚未建立新的选型界面、完整零件库或 NX 自动生成服务。

失败案例也参与检索：一端移动导致孔列偏置、尺寸链写死、原图过期、断开视图越框、
文字导出丢中文、装配盘点漏独立件、图框被误当成完整图纸。工程师返回的结论应绑定具体
文件版本，更新模板及复核用例；一次成功只增加对应范围的证据。
