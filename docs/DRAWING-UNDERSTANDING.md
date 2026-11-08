# 读懂已有工程图纸（CAD 路线的反向一侧）

正向路线从零件、功能与要求生成图纸；本模块从一张已有图纸出发，只凭图纸字节把每个源对象读成可核对的工程表达，零件要素目标保持未知。它回答“图上画了什么、各笔属于哪个表达、哪些线是同一根结构线”，不回答“零件应该是什么”。结果是待复核的候选，不是工程采用。

## 四环承接

| 环节 | 位置 | 内容 |
|---|---|---|
| 工程本体论 | `specs/modules/drawing-understanding.toml` | 两项要求（完整工程表达、整条读图路线）、两个模块（`drawing_understanding` 与独立复核 `drawing_understanding_check`）、44 个类型、5 个接口、5 条约束及其验证案例 |
| ggen | `generated/contracts.rs` 等 | 由锁定 ggen 生成的类型、接口特征与校验；不手改 |
| Rust | `crates/oe-drawing`、`crates/oe-drawing-check`、`oev2 drawing …` | 分类、结构拓扑、Project 绑定的发现、路线、检查页；独立复核不依赖分类程序代码 |
| RRSI | 每次运行的 `route-receipt.json`、`graph-check.json`、浏览器 `checks.json` | 逐步输入/输出摘要、外部进程证据、首个失败与未运行步骤；资格验证另行冻结判据 |

## 读图的层次

1. **源对象守恒**：模型空间每个图形根对象连同原生成员（尺寸块、嵌套块）保留类型、图层、所属与句柄；DXF 2007 以前的文字按 `$DWGCODEPAGE` 解码。
2. **非排他用途**：同一源对象可同时属于多个表达，每个成员带 `[0,1]` 区间；只有合格 LINE 才能出现部分区间，源域必须被成员区间覆盖。
3. **完整表达**：在四个独立维度上分类——学科类别（尺寸、公差、粗糙度、视图标注、技术要求、轮廓等）、几何线型（由 LTYPE 实际划段推得，不由图层名决定）、源图元、视图。尺寸种类由显示文字与构造决定而非原生 DIMENSION 枚举；炸开的尺寸由终端、尺寸线、界线与文字重新组合；国标图层约定只在与本图线型一致时作证据。
4. **结构拓扑与结构线**：可见/不可见轮廓的每个自由端给出类型化原因（文字让位、局部剖视、绘图缝隙、未解释）；只在有依据的断口推断区间，推断区间永不成为源成员；同一视图同类轮廓中共线（或同圆）相连的片段与推断区间合成一根结构线，线型随结构。
5. **零件目标未知**：`engineering_target` 恒为空；把表达绑定到零件要素需要另外的有源工作。

## 学科知识：概念目录、识别卡与覆盖表

读图的判断写成知识，不散在代码里：

| 文件 | 内容 |
|---|---|
| `crates/oe-drawing/knowledge/concepts/G1…G9.toml` | 制图概念目录：987 个概念，整理自国家标准和教材；每个概念一张卡，写明含义、组成、常见画法、DXF 形式、判别方法、易混项和出处 |
| `crates/oe-drawing/knowledge/drawing-representation.toml` | 21 张识别知识卡：一个符号由哪些部分组成、有哪些画法变体、反例是什么、用什么参数判别。程序按卡中的词表和参数判断；每个变体和反例都对应一个测试 |
| `crates/oe-drawing/knowledge/recognizer-coverage.toml` | 覆盖表：每条规则和约定覆盖哪些概念，完整还是部分，缺什么 |

每个表达带 `concept_ids`，检查页显示概念名称。概念状态（卡识别、规则识别、部分、未识别）由覆盖表和卡推出，测试核对，不手填。当前：卡识别 88、规则识别 6、部分 42、未识别 851。没有规则的概念不会被猜出来，相应图元留在“未解释”。

纠正一处读错时，把它写成任何图都适用的知识（卡、变体、反例），用卡中知识写测试，不写具体图纸的句柄、坐标或标注；专家和用户的纠正只作评价对照，不进入识别输入。

## 先查经验

读图任务开始前，用任务原话查工程记忆中的读图经验（整条路线、转换保真、断线与结构线、评价参照等）：

```
oev2 --root ROOT memory session prepare --input request.json
```

请求按 [`session-request.json` 模板](../modules/engineering-memory/skill/templates/session-request.json)填写：`task` 与 `query` 用用户原话；`provider` 与 `session_id` 取当前客户端（Claude Code 的 `CLAUDE_CODE_SESSION_ID`，Codex 的 `CODEX_THREAD_ID`）；`project` 与 `conditions` 按实际任务填写，不从目录或 profile 推断。返回的候选只含摘要与来源引用，不含原文；需要时再用 `memory get --id …` 读全文、`memory verify` 核来源。按候选的 `conditions` 与 `unknowns` 判断是否适用：经验是待核候选，不代替本次的冻结判据与独立评价。用 `session activate` 绑定所用上下文，结束时用 `session feedback` 记录实际用、拒绝或未用。

不加 `--home` 时用本安装根的私有库 `var/memory`（不进 Git），同一安装根下的各客户端共用；需要别的私有库时显式给 `--home`。经验原文可能含开发图纸的数值与专家纠错：在已用于开发的同一张图上做评价时，被评价的 Agent 不读这些来源原文，否则结果按受污染计。

## 固定路线

```
oev2 --root ROOT --project-home HOME drawing route \
  --project project.json --object object.json \
  --dwg drawing.dwg --cadcraft-runtime /path/cadcraft-runtime.json \
  --output-directory runs/drawing-r1 --title "图名" --chrome "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
```

步骤依次为：核运行身份 → 读源 → 沙箱探针 → CADCraft 转 2018 DXF → CADCraft 渲染中性 SVG 与预览 → 当前 Project 上下文 → 发现请求 → 隔离 worker 分类 → 独立复核 → 检查页 → 七种显示模式的离线浏览器检查 → 制图规定检查 → 核源未变。首个失败即停，其后步骤记为 `not_run`；不给 Chrome 时页面不检查，路线状态为 `page_not_checked`。输入固定：不能传入参照答案、区域、反馈、配对模型或截图。

已有 DXF 及其渲染时用 `--dxf` 与 `--svg` 代替 `--dwg`，转换与渲染记为 `not_applicable`。单独的命令：`drawing discover`（在当前 Project 下发现）、`drawing check`（独立复核）、`drawing page`（检查页）、`drawing check-page`（单一模式的浏览器检查）、`drawing rules`（制图规定检查）。路径相对项目目录，或为绝对路径；输出目录必须是项目目录内的新目录。

发现要求项目目录是已锚定的 workspace，且给出的 Project 就是其当前 Project（`native_current_project_changed` 拒绝其他修订）。workspace 经同一管理程序打开，支持 `oev2.workspace-commit/1`–`/6`（见[项目文件体系](PROJECT-WORKSPACE.md)）；在大型 Project 上读图前，图纸须先登记为该 Project 的证据。

## 转换器锁定

只接受 [`integrations/runtime-locks/cadcraft-source.json`](../integrations/runtime-locks/cadcraft-source.json) 锁定的 CADCraft：fork `jiaqiwang969/cadcraft`（上游 `storytold/cadcraft`）的 `fix/dxf-route-fidelity` 提交 `bfa06d4`，版本 `cadcraft-cli 0.15.1-aixcam.5 (dev)`。锁编译进 `oev2`。本机构建另写运行记录 `oev2.cadcraft-runtime/1`：

```json
{"schema":"oev2.cadcraft-runtime/1","program_path":"/abs/cadcraft-cli","program_sha256":"…",
 "repository":"https://github.com/jiaqiwang969/cadcraft","source_commit":"bfa06d49eaf1deb5a4c4098d29fa7d1849f7ab86",
 "fonts_directory":null,"fonts_manifest_path":null,"fonts_manifest_sha256":null}
```

运行前依次核：记录的仓库与提交等于锁；程序字节摘要等于记录；字体清单未变；程序自报版本等于锁。任一不符即拒绝，且被拒的程序不运行（版本不符时只运行过 `--version`）。构建不假定跨机器可复现，每个本机构建记录自己的摘要。

转换在固定环境中运行，**不给 SHX 字体**：字体存在时 CADCraft 会按字体度量重算对齐 TEXT 的基线起点（组码 10），DXF 字节就会随本机字体而变；定义对齐点（组码 11）不变。渲染可用本机字体，字体文件不随本仓库分发。注意：CADCraft 的 `reviewplot` 渲染需要程序旁的 `Fonts/cadcraft-fonts.json` 字体清单，缺少时路线在 render 步失败；没有字体包时可先用 `cadcraft-cli run <dxf> --export <svg>` 导出不依赖字体包的 SVG，再以 `--dxf` 与 `--svg` 运行路线，此时少数堆叠公差文字在页面中可能显示为方框，分类不受影响。

## 隔离 worker

分类在 `oev2 --drawing-source-worker` 中进行：它只从 stdin 收 DXF 字节，不打开根目录、Project、请求或任何路径，多余参数即拒绝。父进程以 deny-default 的 `sandbox-exec` 配置运行它，配置只允许执行它自己、读系统库与 `/dev/null`。路线每次先做四个探针：合成字节得到关系图、读文件被拒、网络被拒、多余参数被拒。worker 依赖 macOS；其他平台明确拒绝。复核记录实际 worker 的字节摘要与配置文本。

## 独立复核

`oe-drawing-check` 用自己的 DXF 读入与代码页解码，从源字节重新推导并逐项核对：源根对象完整保留及类型/图层/所属；原生成员存在且互异；LTYPE 划段推得的线型节奏；合格 LINE 的精确端点；成员区间留在源域内且无缝覆盖；表达族随种类、成员表完整、视图存在、无臆造零件目标；尺寸带数值来源、形位框共享被标尺寸的两个终端；结构端点落在源上、推断区间端点精确且与两侧共线或同圆、结构线片段同视图同类且恰好出现一次；渲染身份都是源根对象；worker 身份与沙箱配置；固定文件未变。任何一项不符即停并报出该项名称（命令行失败行只给检查种类，不含句柄与路径）。报告给出总数、源推导数与分项计数。

## 制图规定的本体化约束

概念目录和识别卡回答"图上这是什么"，制图规定回答"图上应当怎样"。规定不写在代码里，而是本体的约束：[`crates/oe-drawing-check/knowledge/drafting-ontology.toml`](../crates/oe-drawing-check/knowledge/drafting-ontology.toml) 装着两份 Turtle 文档。`ontology` 声明图样对象的类（可见轮廓线、中心线、波浪线、剖面线、尺寸线、尺寸界线、指引线、尺寸数字、角度数字……）和对象之间可由几何计算的关系属性（被图线穿过、与图线重合、端点落在轮廓线上、方向角、文字方向、文字内容）；每个类链接概念目录编号。`shapes` 为每条规定写一个 SHACL 形状：目标类、属性路径与约束、规定编号、概念编号、标准条款、中文消息和严重度。第一批 14 条规定来自图线（G1）、视图与剖视（G2）、尺寸（G4）三组，例如"尺寸数字不得被任何图线穿过"（GB/T 4458.4-2003 4.3.4）、"波浪线应画到轮廓线为止、不得与轮廓线相穿"、"中心线与可见轮廓线重合时只画轮廓线"（GB/T 14665-2012 4.3）、"角度数字须注写单位、一律水平书写"、"剖面区域内注写处剖面线断开"。

第二批 16 条扩到图线、视图与剖视、尺寸、几何公差、表面结构五组，仍只取能用平面几何或文字判定的规定：中心线两端超出轮廓约 2~5 mm（GB/T 4457.4-2002 表1 04.1.2）、比例取自标准系列（GB/T 14690）、同一零件各剖面区域的剖面线方向与间距一致（GB/T 4457.5-2013 4.7）、视图名称与剖切符号／投射箭头／放大部位圈旁的字母对应、局部放大图注比例（GB/T 4458.1-2002 5.7.2）、尺寸界线超出尺寸线约 2 mm 且与之垂直（GB/T 4458.4-2003 4.1.1）、半径尺寸线通过圆心（4.2.3）、尺寸线不画在轮廓线延长线上（4.2.2）、线性尺寸数字随尺寸线方向并避开竖直左侧 30° 区（4.3.2）、公差值前的 Ø 只用于圆柱形公差带（GB/T 1182-2018 8.2.2）、基准字母水平书写（GB/T 17851-2022 7.2.1）、Ra／Rz 数值取自系列（GB/T 1031-2009）。为此本体多了线性／半径尺寸线、线性尺寸数字、公差值、粗糙度值、比例文字、视图名称、索引字母、基准字母等细分类，和一组由通用几何算出的事实：文字高度、剖面线间距、中心线超出轮廓的长度、尺寸界线超出尺寸线的长度与夹角、径向指向圆弧、位于延长线上、与图线同向、同字母、剖面线方向／间距不一致、链式相邻的尺寸线，以及来源记录的 DXF 图层与颜色。例外写成知识而不是放宽谓词：箭头外置的相邻小尺寸（GB/T 4458.4-2003 4.2.5）成链时，尺寸线相叠不算"尺寸线与其他图线重合"。叠写公差按主文字框加叠写框建模，多段线的圆弧段按圆心半径取样。

规范与本体互相绑定：`MdDraftingObject.class` 与 `MdDraftingRelation.predicate` 的允许值就是本体声明的类表和属性表，测试双向核对；形状只能引用本体里的类和属性；每条形状的概念编号必须存在于目录。本体文件是编译输入，改动即重新绑定程序。

检查在独立复核 crate 中进行，不调用分类程序：对象的类来自冻结关系图的成员角色（识别给出的身份），几何来自源 DXF 字节（含尺寸显示块的文字框及其块偏移），关系由通用几何谓词计算，然后分三层导出事实并用框架的 SHACL 引擎（`semantica-runtime`）对每层评估同一套形状：

| 层 | 事实 | 一条命中说明 |
|---|---|---|
| source 源图 | 实体如画：整条线、原文 | 源文件本身偏离规定（要与纸质原图和画图习惯对照后才算图纸错误） |
| recognition 识别 | 成员区间内的几何、关系图给出的标签（如补了单位的角度） | 识别结果仍偏离规定 |
| presentation 呈现 | 合并后的结构线与推断补全的线段，文字按渲染即原文 | 页面所画偏离规定；推断段命中时是补全规则越界 |

`oev2 drawing rules --request 发现请求 --output 报告` 写出报告（`oev2.drawing-drafting-rules/1`：规定表、每层的对象数、关系数、三元组数与命中数、每条命中的层、规定、焦点对象、源句柄与消息）和每层的事实文件 `drafting-facts-<层>.ttl`。路线的 `check-rules` 步运行同一检查并把计数写进回执。命中是待复核的偏离，不改变关系图，也不使路线失败；复核时把每条命中归为原图偏离、识别偏离、呈现偏离、规定需例外或误报。

## 检查页与浏览器检查

检查页嵌入冻结的关系图与中性 SVG，不做分类；含脚本、外部对象、图片、事件属性或链接的 SVG 被拒。页面默认显示完整结构（每根结构线画一次，推断区间在线内），可切换学科类别/线型/图元/视图筛选、原图对照、断口端点与重复线。页面明确写出未嵌入独立参照：参照答案只用于独立评价，不进入页面或输入。

浏览器检查在无头 Chrome（新用户目录、时限）中运行，每项期望都由页面内嵌的关系图计算，不注入答案；需要本图没有的元素的检查记入 `facts.not_applicable`，不算失败。七种模式（kind、line、view、geometry、focus、complete、asdrawn）各运行一次全部断言并留截图。

## 公开案例

[`examples/drawing-understanding/i1-stepped-shaft`](../examples/drawing-understanding/i1-stepped-shaft) 是原创合成三段阶梯轴的 DWG，以及锁定转换器得到的 DXF 与内置字体渲染；测试在其上运行完整路线（DXF 方式）、发现、独立复核及单项篡改反例。

## 义务：谁执行、在哪个入口、凭什么成立

规范 `specs/modules/drawing-understanding.toml` 为本模块声明九条义务（格式见[设计方法](../references/design-method.md#在四环之间保持含义和依据)）。`oev2 --root ROOT design evidence --run 运行目录` 按这些义务判定一次运行；不给 Chrome 时，三条页面义务缺证据而不是成立。

| 义务 | 执行者与入口 | 运行证据 | 运行之外 |
|---|---|---|---|
| 源对象守恒 | 生成合同校验；分类程序 `check`（worker 输出）；独立复核 `check`（check-graph） | 复核通过且“源根对象完整”“类型/图层/所属”“原生成员存在”“源域覆盖到端”各有计数；review 状态为待复核候选 | — |
| worker 只收源字节 | `review`（沙箱、只有 stdin）；路线 probe-sandbox 四个探针；独立复核 | review 记有沙箱配置与 worker 摘要；回执 probe-sandbox 通过；“实际 worker 是固定分类程序”计数 | — |
| 部分区间只在合格直线上 | 分类程序 `check`；独立复核；检查页 `page` | 对应复核计数；回执 page 步通过 | — |
| 不编造零件目标 | 独立复核；浏览器检查（7 种模式） | “无臆造零件目标”计数；7 份页面检查均通过 | — |
| 结构线自洽 | 独立复核；浏览器检查 | 结构线身份与“恰好一根结构线”计数；7 份页面检查中每根线只画一次、推断区间端点在线上 | — |
| 转换器身份 | `verify_cadcraft`（verify-runtime） | 回执 verify-runtime 通过、编译进程序的锁在回执中 | 转换保真由开发图纸上的资格验证确立 |
| 页面如实显示 | `render`；浏览器检查 | 回执 page 步通过；7 份页面检查通过且各有断言 | — |
| 运行可复核 | 路线回执 | 回执记有程序摘要；read-source、verify-source 通过 | 跨运行的逐字节复现由重复资格运行确立 |
| 制图规定出自本体 | 生成合同校验；独立复核 `check_rules`（`drawing rules`、check-rules） | 报告状态 checked、规定不少于 12 条、三层依次为 source/recognition/presentation；回执 check-rules 步通过 | 两批共 30 条，只覆盖可由平面几何或文字判定的规定；需要零件、投影或空腔知识的规定未形式化 |

## 资格与限制

- 规则阈值、视图启发式和知识卡在两张开发图纸上校准；对另一张图的结果需要独立评价，不能由这里的检查通过推出。
- 规则词表含国内机械制图的通用词汇与记号：图框的“借（通）用件登记栏”、标题栏、图号、比例、“技术要求”“热处理”“调质”“口部倒角”等工艺词，以及 ☆/★ 关键特性记号。它们只用于识别，规则不读取也不复制任何具体图纸的内容；别的图框样式或企业惯例需另行评价。
- 开发图纸上的资格验证：经锁定转换器得到的 DXF 与参照字节相同，关系图在声明的格式改名和一句中性陈述替换之外与参照逐字节相同，独立复核与七种模式的浏览器检查计数和事实与参照一致。这说明移植忠实，不说明分类正确。
- 检查通过只说明源守恒与输出合同一致；工程正确、零件要素绑定、制造与检验均另行验收。
- 只支持 ASCII DXF 有限剖面与 macOS worker；DWG 只经锁定的 CADCraft 读入。
- `design inspect` 对 `bbox`、`coordinates`、`evidence_ids`、`line_type`、`pattern`、`points`、`primitive`、`raw_text`、`rule` 报字段含义冲突：这些是既有关系图格式的键名，在别的领域类型中另有含义（`line_type` 在源对象上是源线型名，在样式、成员和结构线上是推得的几何线型）。改名会改变关系图格式，留待四环阶段建立明确的语义身份。
