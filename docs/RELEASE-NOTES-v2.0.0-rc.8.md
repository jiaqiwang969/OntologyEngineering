# OntologyEngineering v2.0.0-rc.8 — 读懂已有工程图纸（几何制图）

**给定一张 DWG，按照学科工艺分解元素：每个图元归到哪类工程表达、哪些线是同一根结构线、断口为什么断，都能回查到图纸的源字节。**

这是 v2.0.0 的第 8 个整合候选（预发布，重构中）。相对 rc.7，这一版的主线是工程图纸的**反向一侧**：此前工程本体的 CAD 路线只有从零件生成图纸的正向（NX/SolidCraft），读懂一张已有图纸基本空白。rc.8 把读图做成本体里的正式能力，并配上方法正本、可执行的检查义务和随 skill 召回的经验。

## 这一版有什么

| 内容 | 位置 |
|---|---|
| 读图命令：`oev2 drawing route` / `discover` / `check` / `page` / `check-page` | `crates/oe-drawing`、`crates/oe-drawing-check` |
| 读图的规范模块：两项要求、两个模块、28 个类型、5 个接口、5 条约束及验证案例，另有 8 条义务 | `specs/modules/drawing-understanding.toml` |
| 转换器锁定：只接受固定 fork 提交的 CADCraft | `integrations/runtime-locks/cadcraft-source.json` |
| 规范语言的“义务”与按运行判定的 `oev2 design evidence` | `specs/modules/design-obligations.toml`、`crates/oe-design` |
| CAD 路线的方法正本（正向与反向、工具保真、只看源字节、冻结判据、四环与义务） | `references/cad-route.md` |
| 读图前先查经验、结束写反馈的 skill 步骤；skill 简介写明读图能力 | `SKILL.md`、`docs/DRAWING-UNDERSTANDING.md` |
| 原创的公开例子：三段阶梯轴 | `examples/drawing-understanding/i1-stepped-shaft` |

完整说明见 [读懂已有工程图纸](DRAWING-UNDERSTANDING.md) 与 [CAD 路线](../references/cad-route.md)。

## 几何制图功能说明

### 它做什么，不做什么

输入一张已有的二维工程图（DWG，或已有的 DXF 与其渲染），只凭图纸字节把每个源对象读成可核对的**工程表达**：轮廓、中心线、剖面线、断裂与局部剖边界、各类尺寸、螺纹、几何公差与基准、表面粗糙度、视图标注、技术要求、图框与标题栏等；同时给出结构拓扑——哪些片段是同一根结构线、每个断口的原因。

它回答“图上画了什么、各笔属于哪个表达、哪些线是同一根结构线”，**不回答**“零件应该是什么”：表达到零件要素与工艺的绑定保持为未知（`engineering_target` 恒为空），结果是待复核的候选，不是工程采用。

### 读图的五个层次

1. **源对象守恒**：模型空间每个图形根对象，连同尺寸块、嵌套块等原生成员，保留类型、图层、所属与句柄；DXF 2007 以前的文字按 `$DWGCODEPAGE` 解码。
2. **非排他用途**：同一源对象可以同时属于多个表达（例如一条轮廓线同时被借作尺寸界线），每个成员带 `[0,1]` 区间；只有合格的直线可以只被部分使用，源对象必须被成员区间覆盖。
3. **完整表达**：在四个互相独立的维度上分类——学科类别、几何线型（由 LTYPE 实际划段推得，不由图层名决定）、源图元、视图。尺寸种类由显示文字与构造决定，不信原生 DIMENSION 枚举与测量值；被炸开的尺寸由终端、尺寸线、界线与文字重新组合；国标图层约定只在与本图线型一致时作证据。
4. **结构拓扑与结构线**：可见与不可见轮廓的每个自由端都给出类型化原因（文字让位、局部剖视、绘图缝隙、未解释）；只在有依据的断口推断补全区间，推断区间永不成为源成员；同一视图同类轮廓中共线（或同圆）相连的片段与推断区间合成**一根结构线**，线型随结构本身（可见轮廓粗实线、不可见轮廓虚线）。真实开口、不同几何或不同视图才分开。
5. **零件目标未知**：把表达绑定到零件要素需要另外的有源工作。

### 识别的表达类别与依据

| 族 | 类别 |
|---|---|
| 几何 | 可见轮廓、不可见轮廓、中心线（轴线）、假想线、断裂与局部边界、剖面线、螺纹画法 |
| 尺寸 | 线性、直径、半径、角度、倒角、螺纹标注、引线；尺寸定义点归作构造参考 |
| 公差 | 位置度、方向、垂直度、平行度、跳动、对称度等几何公差，基准符号 |
| 表面 | 表面粗糙度 |
| 视图 | 视图名称、向视与放大标注 |
| 注释 | 一般技术要求、材料与热处理、工艺要求、防护要求 |
| 图纸 | 图框、标题栏 |

规则依据国内机械制图的通用规定，例如 GB/T 4457.4（图线）、GB/T 4458.1（视图）、GB/T 4458.4（尺寸注法，“尺寸数字不得被任何图线通过”）、GB/T 4459.1（螺纹画法）、GB/T 14665（CAD 制图）与 GB/T 1182（几何公差）。每类表达带有机械制图知识单元编号（`knowledge_ids`，K01–K67）；知识单元目录与书籍、标准原件属于私有资料库，不随本仓库分发。规则不含任何具体图纸的句柄或内容。

### 一条命令的读图路线

```sh
oev2 --root ROOT --project-home HOME drawing route \
  --project project.json --object object.json \
  --dwg drawing.dwg --cadcraft-runtime /path/cadcraft-runtime.json \
  --output-directory runs/drawing-r1 --title "图名" \
  --chrome "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
```

步骤依次为：核运行身份 → 读源 → 沙箱探针 → CADCraft 转 2018 版 DXF → CADCraft 渲染中性 SVG 与预览 → 当前 Project 上下文 → 发现请求 → 隔离 worker 分类 → 独立复核 → 检查页 → 七种显示模式的离线浏览器检查 → 核源未变。首个失败即停，其后步骤记为 `not_run`，每步写入回执 `route-receipt.json`。输入是固定的：不能传入参照答案、手工区域、反馈、配对模型或截图。

已有 DXF 与其渲染时用 `--dxf`、`--svg` 代替 `--dwg`。单独的命令：`drawing discover`（在当前 Project 下发现）、`drawing check`（独立复核）、`drawing page`（检查页）、`drawing check-page`（单一模式的浏览器检查）。读图绑定一个已锚定的 Project 工作区，项目目录、对象与来源都由 Project 承接。

### 转换器锁定

DWG 只经锁定的 CADCraft 读入：fork `jiaqiwang969/cadcraft`（上游 `storytold/cadcraft`）的 `fix/dxf-route-fidelity` 提交 `bfa06d4`，版本 `cadcraft-cli 0.15.1-aixcam.5 (dev)`，许可 MIT OR Apache-2.0。锁编译进 `oev2`；本机构建另写运行记录 `oev2.cadcraft-runtime/1`，记下程序路径与摘要。运行前核对仓库与提交、程序摘要、字体清单和自报版本，任一不符即拒绝且不运行该程序。

这个 fork 修正了上游在 DXF 输出上丢失的信息：样条拟合与曲线拟合多段线的类型、代码页。转换在固定环境中进行且**不给 SHX 字体**，否则 CADCraft 会按字体度量重算对齐文字的基线起点，DXF 字节会随本机字体而变。分类本身也不信尺寸的原生测量值，改读尺寸显示的文字。

### 只看源字节

分类在 `oev2 --drawing-source-worker` 中进行：它只从 stdin 收 DXF 字节，不打开根目录、Project 或任何路径。父进程以 deny-default 的 `sandbox-exec` 配置运行它；每次路线先做四个探针（合成字节得到关系图、读文件被拒、网络被拒、多余参数被拒）。

### 独立复核

`oe-drawing-check` 用自己的 DXF 读入与代码页解码，从源字节重新推导并逐项核对：源根对象完整保留、原生成员、线型节奏、直线端点、成员区间覆盖、表达与成员表、视图、尺寸数值来源、形位框与被标尺寸的共享终端、结构端点与推断区间、结构线片段同视图同类且恰好出现一次、渲染身份、worker 身份与沙箱配置。它不调用分类程序的代码，任何一项不符即停并报出该项名称。

### 检查页与浏览器检查

检查页是单个离线 HTML，嵌入冻结的关系图与中性 SVG，不做分类。默认显示**完整结构**（每根结构线画一次，补全区间在线内），可按学科类别、线型、图元、视图筛选，可对照原图、查看断口端点与重复线。页面明确写出未嵌入独立参照。浏览器检查在无头 Chrome 中按七种模式（kind、line、view、geometry、focus、complete、asdrawn）各运行一次全部断言并留截图，每项期望都由页面内嵌的关系图计算。

### 义务：谁执行、在哪个入口、凭什么成立

规范为读图声明 8 条义务：源对象守恒、worker 只收源字节、部分区间只在合格直线上、不编造零件目标、结构线自洽、转换器身份、页面如实显示、运行可复核。每条写明执行者（生成校验、Rust 检查、独立复核、浏览器检查、运行回执、资格验证等）、入口，以及一次运行中哪份文档的哪个字段满足什么条件。

```sh
oev2 --root ROOT design evidence --run runs/drawing-r1
```

按这些义务判定一次运行，给出成立、失败、缺证据或属于运行之外。

### 与工程记忆：读图前先查经验

`SKILL.md` 与读图说明写明：读图任务开始前，用任务原话经 `oev2 memory session prepare` 查读图经验（整条路线、转换保真、断线与结构线、评价参照等），先看不含原文的候选摘要，按其适用条件与未知判断是否使用；结束时用 `session feedback` 记录实际用、拒绝或未用。不加 `--home` 时用本安装根的私有库 `var/memory`，同一安装根下的各客户端共用；经验库内容不随仓库分发。

### 公开例子

`examples/drawing-understanding/i1-stepped-shaft` 是原创的三段阶梯轴 DWG，以及锁定转换器得到的 DXF 与渲染。测试在其上运行完整路线（DXF 方式）、发现、独立复核和单项篡改反例。

## 验证

- **移植忠实（开发图纸上的资格验证）**：在一张开发用零件图（不随仓库分发）上，经锁定转换器得到的 DXF 与参照字节相同；关系图除声明的格式改名和一句中性陈述外与参照逐字节相同；独立复核的分项计数与七种模式浏览器检查（每种 3151 项）的计数和事实一致。这说明移植忠实，不说明分类正确。
- **部署复核**：部署后的程序在公开例子上走完整路线，关系图与资格运行逐字节相同，七种模式的浏览器检查（每种 211 项）一致，8 条义务全部成立。rc.8 的发布复核见 [验证记录](UNIFIED-VERIFICATION.md)。
- **对照测试**：同一张开发用零件图、同一提示，在全新会话中让 Claude Code（Opus 5.5）与 Codex（GPT-6-Astra，reasoning max）各分“带本 skill 与经验 / 不带”两组，每组 2 次，按冻结评分器与参照比对：

| 客户端 / 分组 | 类别一致率 | 严格一致率 | 专家纠错探针 |
|---|---|---|---|
| Claude Code，带 | 1.0 | 0.885 | 8/8 |
| Claude Code，不带 | 0.99 | 0.834 | 8/8 |
| Codex，带 | 1.0 | 0.917 | 8/8 |
| Codex，不带 | 0.996 | 0.847 | 6/8 |

8 次都完成了任务。不带经验的 Codex 两次都把被专家纠正过的尺寸界线误判为轮廓，带经验后均正确；Claude 不带经验时在这一粗类口径下已接近满分，带经验后改走可复现、带回执与独立复核的路线。局限：测试图就是开发图，参照由同一套规则产生，粗类口径测不出成员区间与结构线等细节，每组只有 2 次。对其他图纸的效果需要另一张图上的盲测。

## 已知限制

- 规则阈值与视图启发式只在一张开发图纸上校准；对另一张图的结果需要独立评价，尚未盲测。
- 分解停在图样表达层；零件要素、工艺与检验的绑定仍为未知。
- **渲染需要字体清单**：CADCraft 的 `reviewplot` 渲染需要程序旁的 `Fonts/cadcraft-fonts.json`，缺少时路线在渲染步失败（读图说明此前写作“不给字体也可渲染”，本版已更正）。没有字体包时，可先用 `cadcraft-cli run <dxf> --export <svg>` 导出不依赖字体包的 SVG，再以 `--dxf` 与 `--svg` 运行路线；此时少数堆叠公差文字在页面里可能显示为方框，分类不受影响。
- 只支持 ASCII DXF 的有限剖面与 macOS 上的隔离 worker；DWG 只经锁定的 CADCraft 读入。
- 读图只认 `oev2.workspace-commit/1` 格式的 Project 工作区；更新格式的工作区需先由对应 owner 迁移。
- `design inspect` 对 9 个关系图键名（`bbox`、`coordinates`、`evidence_ids`、`line_type`、`pattern`、`points`、`primitive`、`raw_text`、`rule`）报字段含义冲突，留待建立明确的语义身份。
- 规则词表含国内机械制图的通用词汇（如图框登记栏、标题栏、“技术要求”、“热处理”等）与 ☆/★ 关键特性记号；别的图框样式或企业惯例需另行评价。
- 检查通过只说明源守恒与输出合同一致；工程正确、制造与检验另行验收。

## 安装与升级

准备 Git、Rust/rustup（仓库锁定 1.95.0）与平台 C/C++ 编译工具：

```sh
git clone https://github.com/jiaqiwang969/OntologyEngineering.git "$HOME/OntologyEngineering"
cd "$HOME/OntologyEngineering"
git checkout v2.0.0-rc.8
sh scripts/install.sh

export ONTOLOGY_ENGINEERING_ROOT="$HOME/OntologyEngineering"
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" --version
```

已有 rc.7 安装时，在原仓库 `git fetch --tags && git checkout v2.0.0-rc.8` 后重新运行 `sh scripts/install.sh`；安装器保留上一安装为 `var/install-previous.*`，私有数据目录 `var/` 与 `sources/` 不受影响。

读 DWG 还需要按锁构建 CADCraft 并写运行记录：

```sh
git clone https://github.com/jiaqiwang969/cadcraft.git && cd cadcraft
git checkout bfa06d49eaf1deb5a4c4098d29fa7d1849f7ab86
rustup run 1.95.0 cargo build --locked --release -p cadcraft-cli
# 运行记录（oev2.cadcraft-runtime/1）写入 program_path 与 program_sha256，格式见 docs/DRAWING-UNDERSTANDING.md
```

页面检查需要本机 Chrome。

## 资料包下载

书籍、标准与资料图谱不进入仓库，按资料包另行下载。仓库此前只给出 Google Drive 上的旧资料包；为免资料不全，本版起同时给出两份百度网盘资料包，三份可以并存：

| 资料包 | 内容 | 下载 |
|---|---|---|
| 旧资料包 | 书籍与米思米标准品目录（以 `upload-manifest.json` 清点） | [Google Drive](https://drive.google.com/drive/folders/1QoNTu0gy9aPwQ-Sk6y5IWSBcVUCDVdXK) |
| 资料包一：`OntologyEngineering-sources-20261023.zip` | 此前整理的完整资料：7 本书原件（登记 6 本）、米思米两套标准品目录、612 个 inCAD 机构案例（BOM 与 CAD 格式归档分片）及其资料图谱；交付清单 `source-delivery.json` 列 1398 个文件 | [百度网盘](https://pan.baidu.com/s/17f-v1qZitDMdIcnhCUSdmQ)，提取码 `i7qa` |
| 资料包二：`ontology-sources-mechanical-drawing-20261105.zip` | rc.8 读图规则依据的机械制图资料：7 本书、144 个标准版本（登记 137 个）、机械制图知识体系 v2（12 个领域、67 个知识单元）与资料阅读导航 v1；不含真实图纸、会话或项目原件 | [百度网盘](https://pan.baidu.com/s/1J4xUZXpXCXXrdBUqRmbisg)，提取码 `u8vm` |

资料包二的压缩包为 1,479,005,868 字节，SHA-256 `53e8eb8de2d8f1d85a99360303ffcfe9ab48a67b9d276b5d726c797ce605b150`，包内交付清单 `delivery.json` 的 SHA-256 为 `970c0b20eb620bfb5407ed017bd9d2b5bc6f34230452e7c62477649df5820a72`；网盘分享页显示的文件名可能与上表不同，以摘要为准。解压后得到与安装根相同结构的 `sources/`，按包内 `sources/collections/mechanical-drawing/README.md` 导入：

```sh
ROOT="$ONTOLOGY_ENGINEERING_ROOT"   # 安装根；PKG 为资料包二的解压目录
mkdir -p "$ROOT/sources/collections/mechanical-drawing"
cp "$PKG/sources/collections/mechanical-drawing/delivery.json" "$ROOT/sources/collections/mechanical-drawing/"
"$ROOT/var/install/bin/oev2" --root "$ROOT" knowledge library source-delivery import \
  --manifest sources/collections/mechanical-drawing/delivery.json --from "$PKG/sources"
"$ROOT/var/install/bin/oev2" --root "$ROOT" knowledge library source-delivery verify \
  --manifest sources/collections/mechanical-drawing/delivery.json
```

导入逐个核对字节数与 SHA-256，不覆盖已有不同内容；`verify` 应全部为 `verified`。登记到独立索引 `sources/.indexes/mechanical-drawing` 与检索的做法见包内说明；《机械制图》第 7 版、《互换性与技术测量基础》和全部标准是扫描件，登记后只能按标准号、题名、章节名与别名检索，正文检索需另行 OCR。读图程序写入的知识单元编号（`knowledge_ids`）经包内 `knowledge-links.json` 连到单元正文、书页锚点与标准记录；这是导航层的连接，不是条款级核对。

资料包一解压后得到 `OntologyEngineering-sources-20261023/`，按其中 `README.md` 以 `source-delivery.json` 导入并核验，再恢复 inCAD 分片、登记书籍与目录。

包内书籍与标准多数受版权保护，取得方式与许可逐项记录在包内；仅供授权范围内使用。

## 其他变更（自 rc.7）

- 工程记忆：macOS 重新挂载卷后设备号变化不再使已固定的快照失效。
- 派生设计模型的资源档位按输入版本记录（`oev2.derived-design-model-resource/3`）。
- 规范片段格式 `oev2.design-fragment/4`：在扩展根下声明义务。
- 读图 CLI 测试：并行运行时夹具持锁与子进程启动互斥，消除偶发的 `resource_busy`（只改测试）。

本版不授予工程采用，项目的全部根验收仍按各自证据判断。
