
# EasyEDA Design Flow — 首席EDA工程师流程脊柱

你是**首席EDA工程师**。整板/非平凡设计**不允许**「边想边随手摆」——那正是覆盖、外围乱飞、线压元件的根因。
按下面的**分阶段 + 硬门禁**走:每个阶段有明确产出和**过门条件**,**门不过不进下一阶段**。

> 这是**编排层**,不重复规则或动作细节:
> - 具体动作(place/wire/modify/move/align/…) → [schematic.md](./schematic.md) / [pcb.md](./pcb.md)
> - 设计规则(分区/间距/朝向/选型) → `easyeda-agent` references
> - 本 skill 只负责**顺序、分组、门禁、自调闭环**。

## 阶段目录(TOC)

深在某阶段想回看别阶段的约束/顺序,先扫这张目录定位(💾 = 该阶段过门后 save 检查点):

- **原理图 S0–S6**:S0 设计方案书 · S1 图纸/分页 💾 · S2 模块编组 · S3 按组摆放 💾 · S4 通道布线 💾 · S5 校验门 · S6 调整闭环 💾 · 录制/演示模式
- **PCB P0–P10**:P0 新板 · P1 导器件 · P2 摆放 · P3 板框 · P4 禁布区(靠前)· P5 丝印对齐(靠前)· P6 可布性门 · **P7 布线**(三档阶梯 + **P7.0 关键网先行** + **自动布线对话框清单** + P7.9 beautify)· P8 叠层+电源+铺铜 · P9 引脚级丝印/极性 · P10 DRC+check 门 💾 · 反模式

## 核心原则

1. **先规划,后落子。** 没分页、没编组之前,一个元件都不放。
2. **先确认图纸,默认 A4。** 生产级布局必须先有可读 sheet primitive。若 `easyeda sch sheet-geometry` 找不到图纸/边界,立即停止:让用户在 EasyEDA 选择/创建默认 A4 图纸,或明确批准 debug/临时路径。**无图纸不允许摆放、布线、autolayout apply。**
3. **按图纸容量分页。** 先按 A4 可用区评估模块面积和布线通道;放不下就自动分页、按模块拆页,不要把单页坐标外扩当作解决方案。
4. **按组摆,不按件摆。** 芯片和它的外围(去耦/晶振/上下拉/接口)是**一个整体**,一起定位、一起移动,组与组之间留出布线通道。
5. **每步可验证。** 摆放后用 `easyeda sch layout-lint` 拿**机械真值**判覆盖/间距,而不是靠肉眼或截图(截图可能 stale)。
6. **门不过就回退。** layout-lint 有 ERROR、DRC 有 fatal → 立刻调整,不带病往下走。
7. **交互密度按模式分档,不再默认每阶段都停。** 全自动/里程碑确认/逐步确认三档怎么选、哪些坑永远不问用户——见下方「交互模式(Interaction Modes)」一节。
8. **过一阶段就存盘(硬规则)。** `place`/`wire`/`modify` 只改 EasyEDA **内存**,不 save 就**不落盘**——窗口重载、daemon 重启、EasyEDA 崩溃都会**丢未保存的工作**。daemon 默认开了**防抖 autosave(3s)** 兜底(变更停 3s 自动 `schematic.save`),但它是安全网不是替代:① 防抖窗口内进程挂掉仍丢最后几笔;② autosave 可能被 `--autosave-debounce 0` 关。所以**每个阶段门通过后仍显式 `easyeda sch save` 一次**(见各阶段 💾),即时落到已知良好点。本流程里 save 是既定步骤;若当前是逐步确认模式,save 前也报告并等待确认。

## 交互模式(Interaction Modes)

「先想清楚、再动手」不等于「每一步都等用户确认」——那会拖垮回归基准和无人值守场景。
按用户意图/场景在下面三档里选一档,而不是让逐步确认变成默认行为:

| 模式 | 确认点 | 场景 |
|---|---|---|
| **全自动(auto)** | 0(仅破坏性操作确认) | 回归基准、CI、ClawFlow operator、录制脚本 |
| **里程碑确认(milestone,真实用户默认)** | 3 处:① S0 设计方案书确认 ② 原理图完成、转 PCB 前 ③ 发板/交付前 | 正常客户设计 |
| **逐步确认(step)** | 每个 S/P 阶段都停,只做读回/报告/建议,等确认再继续 | 教学、演示、用户显式要求「每一步等我确认」 |

**问不问的判据只有一条**:用户的回答会不会改变实际做法——会改变才问,不会改变就不问。据此把坑分两类:
- **Guardrail(不论哪个模式,永远不问用户)**:有唯一正确答案的坑——save 纪律、mutation 后 `doc reload`、layout-lint/DRC 硬门、天线 keepout 必须覆盖全层等——继续以机械门禁形式内置在流程里,不进入交互。
- **Decision(真实权衡,进设计方案书由用户拍板)**:叠层与层数、地策略、接口取向、选型成本档位等——见 [`design-decisions.md`](./design-decisions.md),在 S0 一次性摊开选项+坑+推荐,让用户选,而不是在 S2/P4/P8 执行时才发现是权衡。

## 阶段流水线(原理图)

```
S0 设计方案书 → S1 图纸/分页💾 → S2 模块编组 → S3 按组摆放💾 → S4 通道布线💾 → S5 校验门 → S6 调整闭环💾
                                            ↑___________________________________|
```
> 💾 = 该阶段通过后 `easyeda sch save` 存盘检查点(见原则 5)。整板放置时,S3 每放完几组(或每 ~10 件)就 save 一次,别等全放完——崩一次就白干。

### S0 — 设计方案书(Design Proposal)
- **做什么**:读懂设计——器件清单、电源树、功能模块划分、目标幅面;**并在放置第一个元件之前**,把覆盖原理图 + PCB 全程的架构决策一次性定下来,而不是让它们在 S2/P4/P8 执行到才被想起(实测最贵的返工正是决策后置:天线 keepout 后置逼已布线的模块重绕,地策略选错要重铺)。
- **怎么做(轻量摸底)**:见 conventions 的 `design-pre-analysis.md`(器件清单、电源树、功能分组、幅面估算)。`easyeda health` 确认已连接。
- **怎么做(架构决策)**:查 [`design-decisions.md`](./design-decisions.md) 里的决策点清单——叠层与层数、地策略(单 GND PLANE vs 分区 pour + 桥地)、接口取向(如 USB 单/双取向)、器件选型成本档位等。**每一条都要把该文件里的选项、已知坑、推荐方案摊开给用户看,由用户拍板,不能替用户默认选**——这是设计方案书阶段的核心产出,不是可选步骤。RF/天线 keepout 范围**不在**这份决策清单里——它是唯一正确答案的 guardrail(必须覆盖全层),不摊开选项;S0 只需把 RF 器件位号和 `"all"` 层范围写进 spec 的 `rf` 字段供 P4 直接读取执行。
- **产出**:模块清单(如 MCU、电源、USB、传感器、调试口…)+ 每个模块的器件归属 + 一份**机器可消费的设计方案书 spec(JSON)**。后续阶段(S2 模块分区、P4 禁布区、P8 叠层/电源/铺铜)从这份 spec **读取执行,不重新决策**——方案书写下的决策与执行阶段的做法不一致视为 bug。

  **方案书 spec 形状**(与 `sch autolayout --spec` 同一路数——一份可回读、可复用的 JSON 文件,不是写完即弃的长文;字段可按项目取舍,关键是稳定、后续阶段能直接引用):

    {
      "modules": [
        {"name": "POWER",   "kind": "POWER", "parts": ["U2","L1","C1","C2","F1"], "page": "P2_POWER", "zone": "left"},
        {"name": "MCU",     "kind": "MCU",   "parts": ["U1","C18","C19","R6"],    "page": "P1_MCU_USB", "zone": "center"},
        {"name": "USB_HUB", "kind": "USB",   "parts": ["J2","U10","X1","C30","R15"], "page": "P1_MCU_USB", "zone": "left-top"},
        {"name": "ANT",     "kind": "ANT",   "parts": ["ANT1"], "page": "P1_MCU_USB", "zone": "right"}
      ],
      "flow": ["POWER", "MCU", "USB", "ANT"],
      "flowAxis": "auto",
      "pages": [
        {"name": "P1_MCU_USB", "sheet": "A4", "modules": ["MCU","USB_HUB","ANT"]}
      ],
      "stackup": {
        "layers": 4,
        "groundStrategy": "plane",
        "innerLayers": ["GND", "VCC_3V3"]
      },
      "assembly": {
        "profile": "hand-solder",
        "side": "top"
      },
      "rf": {
        "parts": ["U_WROOM1"],
        "keepoutLayers": "all"
      },
      "board": {
        "outline": "compact"
      },
      "interfaces": [
        {"name": "USB_C",      "ref": "J2", "edge": "bottom", "facing": "user-facing",
         "orientation": "dual", "plugWidthMm": 13.0},
        {"name": "备份电池座",  "ref": "J1", "internal": true},
        {"name": "IPEX 天线座", "ref": "E1", "edge": "any"}
      ],
      "costTier": "standard"
    }

  逐字段说明:
  - `modules[]` — `name`/**`kind`**/`parts`/`page`/`zone`;S2 模块编组直接读 `page` + `zone`,不重新分区。**标准外设模块可直接引用电路块**:给该 module 记 `block`(如 `"block.ch340c_usb_serial"`),S3 摆放时照抄该块拓扑、只重绑 ports——先 `easyeda blocks ls` 看有没有现成块,能少写一整个模块的选型+接线。
  - **`modules[].kind`(#167 新增)** — 受控功能域词汇,`flow` 里出现的就是这些值:`POWER` `MCU` `RF` `ANT` `IO` `ANALOG` `SENSOR` `STORAGE` `USB` `DEBUG` `PROTECTION` `POWER_MON` `OTHER`。为什么需要受控词汇:`name` 是自由文本,靠 name 匹配会在大小写和同义词上碎掉,而 P6 的 `flow-order` 维要把「模块质心沿轴的排序」与「spec 声明的顺序」对上。**没写 `kind` 时会拿 `name` 去词表碰一次**(老 spec 的 `name` 恰好常是 MCU/RF/IO),碰不上就报 WARN 且该模块不参与 flow-order。
  - **`flow` / `flowAxis`(#167 新增)** — `flow` 是**有序**功能域列表,表达「电源 → 数字 → RF → 天线」这种信号流向意图;`flowAxis` = `x` | `y` | `auto`(默认 `auto` = 取板框长边,长边通常就是信号流方向)。**方向不强制**:板上从右到左走 POWER→ANT 与从左到右同样好(打分时正反都算取绝对值大的)。**没写 `flow` 时 `pcb layout-score` 的 flow-order 维会被标 skipped 而不是给满分**——「没测」和「测了满分」必须可区分。`flow` 里重复或不在词表里的阶段是 ERROR;声明了但板上没有对应模块的阶段是 WARN。
  - `pages[]` — `name`/`sheet`(幅面,默认 `"A4"`)/`modules`;S1 分页直接读,不重新估算页数。
  - `stackup` — `layers`(层数)/`groundStrategy`(`"plane"` = 单 GND 内电层,或 `"signal-zones-with-pour"` = 分区 pour + 桥地)/`innerLayers`;P8 叠层+电源+铺铜直接读,不重新选地策略。**兼容旧写法** `inner1`/`inner2`(会被归一成 `innerLayers`)。
  - `assembly` — `profile`(`hand-solder` | `reflow`)/`side`(`top` | `both`);P2 的两个决策落盘处(仍需 `pcb stage set-assembly` 落进 workflow 状态,spec 只是意图正本)。
  - `rf` — `parts`(RF/天线器件位号列表)/`keepoutLayers`(如 `"all"` 或具体层号数组);P4 禁布区直接读作用范围,不重新判断该不该禁、禁哪些层。
  - `board` — 板框意图:客户给了尺寸/外形就原样记(如 `{"outline": "50x40mm"}`),**没给就写 `"compact"`(默认)**——紧凑是无信息时的正确目标(省板费+小体积),不要摊大饼;P1 落件与 P3 板框据此执行。**兼容旧写法**:直接写字符串 `"board": "compact"` 也能读。
  - `interfaces[]` — `name` + **`ref`/`edge`/`facing`/`internal`/`plugWidthMm`(#167/#168 新增)** + 既有的 `orientation`:
    - **`ref`** — 位号(`J1`/`USB1`…),把板级意图**钉到具体器件**上。没写 `ref` 的接口,#168 的连接器规则只能对它走启发式(报 INFO 而非 WARN),`pcb floorplan` 也不会为它钉边 —— 所以能写就写。
    - **`edge`** — `left`|`right`|`top`|`bottom`|`any`;`pcb floorplan` 只把**显式声明了 `ref`+`edge`** 的连接器钉到板边(边序是装配体验,工具不猜)。
    - **`facing`** — `user-facing`(用户插拔的对外口,外壳要开孔,理应占板外沿)| `internal`(只在箱内连线,如备份电池座/板间排针,占外沿就是浪费稀缺资源)| `any`(必须在某条边但哪条都行,如 RF 天线座)。**`"internal": true` 是 `facing:"internal"` 的简写**;两者矛盾(`internal:true` + `facing:"user-facing"`)是 ERROR。
    - **`plugWidthMm`** — 插头护套包络宽的人工覆盖(块库查找表查不到、或手上有真插头量过时用)。
    - **为什么这些非写不可**:「这个连接器对内还是对外」是**设计意图不是几何属性** —— 同一个 PH2.0-3P 座接箱内电芯是 internal、接箱外传感器就是 user-facing,铜箔再精确也推不出来。写了 spec → 判定升 WARN(板级决定可信);不写 → 只能靠启发式推定,只报 INFO。规范见 `pcb-design-rules.md` §3.5。
  - `costTier` — 选型成本档位(如 `"standard"`/`"premium"`),`parts-select.py` 选型时参考。

  **写完必须校验**(#167 起 spec 有 Go 类型 + 校验命令,写错不再静默):

  ```bash
  easyeda spec validate .easyeda/s0-<project>.json           # ERROR 才非零退出
  easyeda spec validate .easyeda/s0-<project>.json --strict   # 交付前用:WARN 也失败
  easyeda spec show     .easyeda/s0-<project>.json            # 看「工具实际读到的」归一化结果
  ```

  判定口径刻意宽松以兼容既有 spec:**ERROR** = 写了但写错(枚举外的 zone/kind/facing、flow 里重复或不存在的阶段、自相矛盾的 internal/facing);**WARN** = 缺了会让某维测不了;**INFO** = 能力降级。**既有 spec 全部继续能读**,缺新字段只报 WARN/INFO,不会一夜作废。
- **过门条件**:上面这份 spec 已经**过用户确认、落成了文件、并且 `easyeda spec validate` 无 ERROR**(与 `autolayout --spec` 文件同样对待——写到磁盘,可在后续阶段被引用,不是只停留在对话记录里);不再是「每个器件归到了某个模块」这么单薄。这正是「里程碑确认」模式的第一个确认点;若当前是「逐步确认」模式,同样在这里停住等确认(见「交互模式」一节);「全自动」模式下按已有 spec 或默认推荐值直接产出文件,不阻塞。

### S1 — 图纸 / 分页(先图纸,再分页!)
- **做什么**:确认当前页有图纸,默认 A4;再按模块/功能把设计**先分到几页**(电源一页、主控一页、接口一页…),别全堆一页。
- **怎么做**:`easyeda sch pages`(或 `doc ls`)读页结构 → `easyeda doc switch` 切目标页 → `easyeda sch sheet-geometry --json` 读 sheet/title-block。无 sheet 或 provenance 为 none 时停止,不要开始 place。
- **页面对齐模块计划(用户点名·必做)**:分页不是照单全收用户现有的页——**要主动把现有分页 reconcile 到模块计划**:①页名无意义(`P1`/`P2`/`Schematic1`/`page1`)或与模块不符 → `easyeda sch page-rename --page <uuid> --name <功能名>`(如 `P1_POWER`/`P2_MCU_ESP32`/`P3_IO`),命名风格跟参考板 `P1_POWER_PATH` 一致;②模块比页多 / 复杂模块要独立 → `easyeda sch page-new` 补页;③**多余空页 → `easyeda sch page-delete --page <uuid>` 删掉**(先确认该页无器件,`sch pages` + 逐页 `list` 核实;page-delete 无 undo,属破坏性,删前 inspect)。单页小板:把唯一那页也 `page-rename` 成有意义的名(别留 `Schematic1`),不必强行分多页。目标:**页集合 = 模块计划**,一页一功能域,跨页同名 `net_port` 接续。
- **💾 过门条件**:页集合与模块计划一致(该改名的已改、该补的已补、多余空页已删);每个目标页都有可读图纸(A4 默认)和明确职责;每页模块预计能落在可用区内,标题栏 keep-out 明确 → `easyeda sch save`。若用户要求逐步确认,保存/继续前停住。

### S2 — 模块编组
- **做什么**:在每页内,把「芯片 + 其外围电路」定义为一个**组**,并规划各组在页面上的**分区位置**(谁在左、谁在右、信号流向)。
- **怎么做**:分区/信号流向规则查 conventions 的 `schematic-layout-conventions.md`。此阶段只规划坐标分区,先不落子。布局 spec 里的 `sheet` 默认写 `"A4"`;zone 必须落在 S1 读到的 sheet 可用区内。**组的 page/zone 归属读 S0 方案书 spec 的 `modules[].page` / `modules[].zone`——这里只是把已定的分区落成具体矩形,不重新分配**。
- **分区落成可校验的认领(新)**:`easyeda sch zones set --spec <s0-spec.json>`(或 `--module NAME=ZONE:D1,D2`)把 modules[].zone 持久化进项目 workflow 状态——此后 **`sch layout-lint` 自动多一条 zone-violation WARN**(被认领的件 bbox 中心落在其分区矩形之外 = S0 拍板的分区没落实),`sch zones status` 随时看认领+活体违规。词汇同 autolayout/pcb zones(left/center/right × top/bottom;画布 y-UP,top=大 y=视觉上半区)。原理图和 PCB 的认领各自独立(同一模块在图纸和板上的分区可以不同)。
- **分区框可视化(新)**:`easyeda sch zone-draw` 把认领画成**虚线框+区名标签**(行业规范「先看区、再看线」;注释图元,非电气对象;真机验证过 `eda.sch_PrimitiveRectangle/Text` 全 CRUD)。框的几何与 zone-violation 用同一 `zoneRect`——**所见即所校验**。矩形 API 在 y-UP 画布以 `(MinX, MaxY)` 为左上锚点、按高度向 `-y` 延伸;固定九宫格与 partition 模式共用同一生成逻辑,禁止把 `MinY` 当左上 y（会整框下移一个高度并越过图纸/图签下边界）。固定九宫格标题默认 14pt，可用 `--font-size` 调整。`sch zones` 认领与所有绘框模式现在都按 **documentUuid 分页持久化**；多页 spec 的 `modules[].page` 会解析到各页，逐页用 `--doc <页>` 绘制，绝不把 MCU 页认领套到 Power/Peripheral 页。重跑先**验证删除**旧框：任何 rectangle/text survivor 都硬失败并保留恢复 ID；创建返回的框/文字 ID 数量必须一一匹配，部分失败在同一脚本内补偿删除。画/清完成均显式 `schematic.save(saved:true)`，不能用“内存里看见了”冒充持久化。
- **数据驱动整纸分区(#149)**:上面 `zone-draw`(默认 `--mode zones`)是**固定九宫格**;要「按整张纸合理切分功能区、给右下角图签留缺口」的行业版式,用 `easyeda sch zone-plan`(纯计算,`--json` 出方案+validation,不落笔)+ `easyeda sch zone-draw --mode partition --font-size 22`(画大字号分区框)。planner 从**活体几何**求解:usable sheet(减 margin)按**模块 bbox 之间的自然空隙**切列/行分割线(不是固定分数——高模块跨过中心分割线会跑出分区,故按边缘间隙且间隙≥gutter),每个分区**抬到标题栏 keep-out 之上**、顶部留 title band 放大字号区名。模块 bbox = 认领的各件 live bbox 并集(读当前页的 `sch zones` 认领)。validation 五项(sheetOverflow / partitionOverlap / titleBlockHits / moduleOutsideZone / labelCollisions)必须全 0；有违规时拒绝落笔。适合多页工程逐页分区。
- **过门条件**:每个组有明确的目标矩形区域(已 `sch zones set` 认领),组间预留了通道(不重叠的分区);若模块太多,已经拆到下一页而不是挤压本页。

### S3 — 按组摆放(芯片 + 外围一起)
- **做什么**:**逐组**放置——先放该组核心芯片,再把它的外围**就近**放在芯片周围(去耦贴电源脚、晶振贴时钟脚…),放完一组再下一组。
- **块优先(电路块库)**:摆放/接线任何**标准外设模块**前,**先查块库 `easyeda blocks ls`**(离线,无需 daemon/窗口,块库编在二进制里)——现有 **20 块 / 11 类目**(power/rf/usb/usb-serial/comms/storage/sensing/mcu/mcu-support/indicator/button),如 CH340 串口、ESP32 自动下载、buck/LDO/充电、RS-485、GNSS/433M、USB-hub、microSD…;`easyeda blocks search <关键词>`(按**功能**/**芯片**/**端口网**三维轮换搜,一词没中换维度别急着手接)、`blocks ls --category <cat>` 浏览整类、`blocks show <id>` 看完整拓扑。命中就照抄内部网表、只重绑 ports 到主控网络 + 重排位号(引脚用功能名,零改号),按 `schematic_notes` 落线。块的 `parts` 直接给出 `standard-parts.json` 的 role,选型这步都省了。**摆放几何也机械化了**:带 `schematic_layout` 模板的块,`sch block-apply` 按模板的相对偏移+朝向落件(信号流左入右出、去耦贴芯片的几何人审一次终身复用);无模板的块退回网格铺列。两条路**原点都会避开已有器件和 A4 标题栏图签**(不显式传 `--at` 时,按真实 bbox + 标题栏 keep-out 螺旋找最近空位,块落纸面右下也不会压到图签/明细表;显式 `--at` 尊重你的坐标但碰撞会警告)。落完以每次 place 返回的 `primitiveId` 标识本实例,并**回读真实 bbox + pins 作 wiring 前硬门**:读取/解析/几何不完整、涉及本实例的 bbox overlap 或异件引脚重合都会停止,绝不进入 autoconnect、绝不打印假绿;随后只按本次 ID 补偿删除并再次读回,可证明删净为 `failed-rolled-back`,否则明确 `failed-partial` + `PARTIAL STATE`(EasyEDA 各次变异独立 autosave,不是事务)。**块还带 PCB 多维约束 map**,各 P 阶段按 `target` 匹配读:`pcb_layout`(`*-adjacency`→P2 贴脚距离、`ep-*`→P8 EP 热过孔/接地缝合、`rf-*`/`balun-mirror`→P4 keepout+P7 RF 布线)、`placement`→P2 板边/朝向、`signals`→P7.0 差分/阻抗、`silk`→P9 逐脚标注。无命中才手接;手接并端到端验证过的新外设,按 [`standard-blocks-contributing.md`](./standard-blocks-contributing.md) 回馈入库(署名 + `validated` 门禁)。**块没带 `schematic_layout` 模板时,别再肉眼盯板手写 dx/dy**——在真板上把该外设摆漂亮一次,`easyeda sch extract-layout <block-id> --from <该实例的位号>`(或 `--role ROLE=DESIG` 精确指定)反向导出模板 JSON(role→{dx,dy,rotation},锚点=芯片/首个 role,自动吸 5 格),复审后粘进 `internal/blocks/data/<id>.json` 的 `schematic_layout` 段、跑 `go test ./internal/blocks/...` 过校验(#140)。这样「摆好一次→固化模板」是数据管线而非手工反推。
- **怎么做**:`easyeda sch place` + `sch modify`(设位号);坐标按 S2 的分区。库优先、选型规则见 schematic.md / references。
- **整组分区摆放优先用 `easyeda sch autolayout`**(模块级放置规划器,`--engine template` 默认):把 S2 的分区写成 `--spec`(每个 module 给 `zone`/`core`/`parts` 与规则),它按真实 bbox 把核心芯片放到分区中心、外围环绕核心、碰撞自动重试,并保留引脚 fanout 通道 + A4 标题栏 keep-out,**确定性产出可过 layout-lint 的坐标**。先 `--dry-run` 看方案,确认后用 `--doc <页>` + `--apply`;`--doc` 与 `spec.page` 同时存在时必须解析到同一 UUID。`--apply` 前必须有真实 sheet bbox;无 sheet 只能停在 dry-run/修图纸。**template v1 是 parts-only,必须在布线前运行**:规划前与首个 move 前各读取一次 fail-closed active-page inventory,只要存在 wire/bus/netflag/netport/netlabel/short_symbol 任一种就硬拒绝;同时要求完整有限 anchor/bbox、显式成功的 pins 读取,且两次快照的器件+sheet 几何完全一致,防止 stale plan。`--apply --all-pages` 同样 fail closed(`--all-pages` 仅 dry-run)。当前没有 force/override,`--rewire` 仅属于 official 引擎。落子后逐 primitiveId 回读目标锚点与完整 baseline,再验 sheet bounds/grid/spacing/overlap/pin coincidence/title-block,只有 `schematic.save` 明确 `saved:true` 才算完成;任一步失败都按规划快照逆序恢复,再次回读原锚点(只有确认值才算 restored)并证明 rollback 保存,不伪装成事务。**`--apply` 成功后自动画功能分区框(#142)**:按 `modules[].zone` 用 zone-draw 同几何画出虚线区域框+区名(电源区/MCU区/USB区…),认领和 frame IDs 都落到该目标 documentUuid，不再写项目级单记录；页面 UUID、ID 数量和保存同样核验。绘框失败不回滚已保存布局,但会补偿删除已创建注释并让命令非零退出,不会伪装完整成功。`--zone-draw=false` 关闭,`sch zone-draw --clear --doc <页>` 移除。**v1 只移动「已放置」的器件**,不创建缺件——所以先 `sch place` 把器件放上页,再用 autolayout 排布。手动 `sch place`/`modify` 仍是逐件微调的兜底。
- **兜底引擎 `--engine official`**:块/spec 都没覆盖的散乱页,可让**官方 `eda.sch_Document.autoLayout()`**(@beta,3.2.148 起可用)对整页粗排。⚠**它是破坏性的、只该布线前用**:实测三坑——① **只挪器件不挪导线** → 已连线页跑完连线全断(16件→59悬空);② **落点 off-grid** → 下游布线短桩落不到脚;③ **散布让短桩相撞短路**(--replace 也拆不开)。封装已加安全管线:**已连线页默认拒绝**(报 N 条线),`--rewire` 才允许(跑后自动:吸附5格→删断线→按跑前网表重连);**自检用 `sch check`(查断线/悬空)不止 layout-lint**;长操作 ~2min(300s 超时)、需目标页**前台**。**#143 兜底增强**:① `connect_pin` 网格判定容忍浮点残差(吸附后引脚坐标=anchor+旋转偏移会带入 649.9999999 之类 FP 噪声,旧严格判定误拒→现 round 到最近格再判并把桩起点吸到整格,官方 `--rewire` 可连干净);② 官方 `autoLayout` 喂 `designatorDeviceTypeMap`(按位号前缀分 resistor/capacitor/chip… 角色),布局更聚拢。即便如此,散布布局仍可能残留极少数短桩相撞的失败连接(命令会诚实报 floating/失败数,让你 `sch bridge-check` 核)。**优先级铁律:命中块→`block-apply` 模板;有 spec→`--engine template`;都没有才 `--engine official` 兜底,且成品/已连线页一律别用它。**
  - **official 的 P0 fail-closed 契约**:执行 `autoLayout` 前会再次读取同一页的「part id/位号/坐标/旋转 + sheet + wire/bus/net marker 全量计数」并与首份快照逐项比较，启动变异的同一个 JS action 内还会再次锁定 document/input；`--rewire` 另比较规范化网表，所以即使 wire 数没变、拓扑已换线也会拒绝。bus 无法可靠重建，始终拒绝。后续 snap/delete/rewire/save 全部钉同一 UUID。变异后的 bbox/pin/connectivity 回读或 `sch check` 只要不可用、解析不完整，或发现 overlap、异件 pin 重合、dangling/多网线/marker 等阻断 finding，或 save 未明确 `saved:true`，命令同样非零退出，不能再把 “check unavailable” 当 clean。注意官方 API 没有事务回滚：post-check 失败时页面已经变异，必须先人工修复/撤销，不能继续 S4。
- **摆完可自评质量**:`easyeda sch layout-score` 给布局可读性打五维分(标签折叠/标签反向/外围贴核心/长链挤压/版面整洁),低分维的归因**自带填好真实位号坐标的 fix 命令,照抄执行即可修**;它是诊断视角不是门(无 `--min-score` 永远 exit 0),门仍是下面的 layout-lint + check。
- **💾 过门条件**:进入 S4 前**必须先过 `easyeda sch layout-lint --strict`**——本组无覆盖/引脚重合，锚点全部在 5-unit 网格，模块不越认领 zone，组内/组间满足 `--min-gap`，且没有因缺 bbox/pin 几何而跳过的器件；已有 zone claim 时 sheet/zone check 也必须可用。默认非 strict 仅供诊断，不能再用 `0 overlap` 冒充布局已完成。**有 ERROR 先回 S3 调整**(单件 `sch modify`;成排对齐 `sch align --mode centerx/top…`;等距摊开 `sch distribute --axis x/y`,均默认 dry-run、`--apply` 落地并自检 overlap;整簇平移 `sch group-move`;实在挤不下 `sch autoplace-free`)。过了就 `easyeda sch save`(整板放置每 ~10 件存一次,别等全放完)。**有 `sch zones set` 认领的多页/多模块板,进 S4 前还需画出分区框+区名标注**——`sch autolayout --apply` 走的话已自动画(#142),手工 `sch place`/`modify` 路径不会自动触发,须显式补跑 `sch zone-draw`(整纸版式用 `--mode partition`);`sch zones status` 可查认领是否已可视化。**分区框只给模块命名——每个模块还要配 1~3 行电路说明**(`easyeda sch note --text "LDO: 5V→3V3 1A\n输入/输出各 100nF" --x … --y …`:作用+关键参数+设计要点,放模块框下/旁的空白处,字号默认 10 低于区名标签,不压电路,放完 `sch layout-lint` 核对无 overlap;`sch text-list` 枚举、`sch prim-delete` 清理)。这是**默认必做**(用户反馈:成品图没分区没说明=交付可读性差),不是可选装饰;分页(S1)+分区框(S2)+电路说明(S3)三件套齐了才算原理图组织完成。单页/单模块小板可免分页,但区名框+电路说明仍要画。

### S4 — 通道布线(留距离,别压元件)
- **做什么**:在组**摆放并通过 layout-lint 之后**再布线——信号走元件间的**空通道**,不要让导线压在元件或外围上。
- **怎么做**:布线/flag/去耦规则见 conventions 的 `auto-layout-sop.md`(模块内信号=短正交线，跨模块/跨页或长距离=命名 netport 短桩，flag 仅电源地，绝不穿引脚)。
- **电源/地/netport stub 用 `easyeda sch autoconnect`**(别再手猜 `connect --direction/--offset`):它按真实 bbox/引脚/已有 flag 几何打分,确定性选 direction+offset 再委托 `connect_pin` 落地,批量 `--spec` 还会自动错开标签。先 `--dry-run` 看计划,满意再落地。
- **💾 过门后**:`easyeda sch save` 存盘,再进入 S5。

### S5 — 校验门(机械真值,不是肉眼)

**逐页跑 `easyeda sch gate --strict --doc <page>`,再做一次设计意图对账。** 不要用
`--all-pages` 的浅数据冒充逐页证明(`--strict` 与 `--all-pages` 不兼容,多页就循环 `--doc`)。

1. **机械门(一条命令)** `easyeda sch gate --strict --doc <page>`

   gate 把四个检查器按**固定顺序**跑完并出一张报告 —— 顺序、阻塞判据、退出码都写在代码里,
   不再每次现场决定(此前四个命令各跑各的,agent 每次都要自己拼,拼法不一致就是不稳定):

   | # | stage | 阻塞判据 |
   |---|---|---|
   | 1 | `layout-lint` | `overlap` / `pin-coincidence`;strict 下 spacing、off-grid、zone-violation、缺失/畸形几何同样阻断 |
   | 2 | `check` | fatal / error 级 finding(悬空脚、导线交叉/穿脚、网络标识不一致、零长/悬挂线、`duplicate-net-marker`、`titleblock-overlap`、`marker-overlap`) |
   | 3 | `bridge-check` | `wire-bridge` 真短路(一棵 wire tree 带多个网名);orphan stub/flag 是告警,strict 下阻塞 |
   | 4 | `drc` | 官方 SDK fatal。**放最后**:最慢、需窗口前台,且聚合结果最不可行动 |

   **verdict 三态,`blocked` ≠ `fail`**:
   - `pass` — 全过
   - `fail` — **板子有阻塞问题**,照报告「下一步」修,回 S3/S4
   - `blocked` — **检查器没跑起来**(连接器断、页没打开、返回结构异常),原理图**从未被完整判定**。
     此时后续 stage 会被跳过而不是继续撞同一堵墙。先 `easyeda health`、`easyeda doc ls` /
     `doc switch <page>` 修环境,再重跑 gate。**别把它当成板子的问题去改电路。**

   `--json` 带每个 stage 的完整原生报告(`stages[].detail`),是四个单命令 JSON 的超集,不用重跑。
   局部复查仍可直接用 `sch layout-lint` / `sch check` / `sch bridge-check` / `sch drc` 单命令
   (它们原样保留),但**交付门走 gate**。窗口不在前台时 `--skip drc` 先过前三关。

2. **设计意图门** `easyeda sch read --doc <page>`
   - 新设计逐项对照 spec；整理已连线页面时对照修改前保存的 `DESIGNATOR.pin → net` 黄金表与显式 NC 集合。任何差异都先修复，不能把“布局变化”变成静默改网。
   - 这道门 gate **不管** —— 它是语义对账,机器判不了「接对了没有」,只能判「接得合不合法」。
   - 可再跑 `scripts/lint.sh <project>` 做数据 lint，但它不替代上面的活体门。
- ⚠️ **判状态看数据(`sch list` / `sch gate`),不看截图**(API 改动后画布可能不重绘 → 截图 stale)。

### S6 — 调整闭环(立刻调,再验)
- **先看 gate 报告的「下一步」** —— 每个失败 stage 自带规定的修法,别自己另发明一套。
- `layout-lint` 失败 → `sch modify`(单件)/`sch align`/`sch distribute`(成排)/`sch autoplace-free`(自动找空位)把冲突元件挪开。**几何先修**:重叠会连锁出一堆电气误报,先治几何再看电气,能省掉大半来回。
- `check` / `bridge-check` / `drc` 失败 → 补线、拆桥、清孤儿或补 NC → **重跑 gate 并重新 `sch read` 对账**。
- `blocked` 不是修电路的信号 → 按 S5 的三态说明先修环境(health / doc switch),再重跑。
- **💾 循环直到 `sch gate` verdict=pass 且设计意图对账无差异，再 `easyeda sch save --doc <page>` 收尾并确认 `saved:true`**。这就是“调整后立刻验证”的闭环。
- **收尾回流(块库共建)**:本板若含手工搭建且已 `sch check` + 网表核实通过的标准外围(库里没有的),按 [`standard-blocks-contributing.md`](./standard-blocks-contributing.md) 顺手回流一个块(署名 + `validated`)——验证刚过正是入库时机。

## 录制 / 演示模式(Recording / Demo Mode)

⚠️ **触发词**:当用户显式说「我要录制 / 做演示 / 做教程 / 要截图 / 要过程图 / 出分阶段图」等,进入本模式。**它不改变数据门,而是在数据门之外再加一道「可视化产物门」**——因为此时截图不再只是判据,而是**交付物**。

**双门规则**:

| 门 | 判什么 | 用什么 | 何时看 |
|----|--------|--------|--------|
| **数据门(始终生效)** | 设计对不对 | `pcb list` / `track-list` / `via-list` / `pour-list` / `drc` / `check` / `layout-lint`(原理图侧 `sch list` / `layout-lint`) | 每阶段判正确性 |
| **可视化产物门(仅录制/演示)** | 每阶段有没有**非 blank、非 stale、且是对的文档**的原生截图 | `easyeda pcb stage-snapshot --stage … --previous-sha256 <上帧sha>`(自动把关);单帧 PCB 用 `pcb snapshot`,原理图用 `sch export-image` | 每阶段留档交付 |

**核心纪律——原生截图 ≠ 数据渲染图**:

1. **优先原生 EasyEDA 截图。** 每个阶段 PCB 用 `easyeda pcb snapshot` 抓画布(带 `sha256`/`capturedAt`);原理图用 `sch export-image`(渲染文档数据,无 stale)。
2. **两种坏帧要分开:STALE(冻结)vs BLANK(空白)。**
   - **STALE** = 与上一帧字节相同(EasyEDA 不在 API 改动后自动重绘)。传 `--previous-sha256 <上帧sha>`,connector 检测到即重试一次并回报 `stale=true`。
   - **BLANK** = 画布根本没渲染出内容(窗口最小化 / 在别的 Space / 被别的窗口挡住时,`getCurrentRenderedAreaImage` 回读一张平坦帧)。`snapshot` / `stage-snapshot` 现在会**在 CLI 侧读 PNG 判空**并告警(`primitiveCount>0` 却是平坦单色图 = 窗口没渲染,不是设计错)。
   - ⚠️ **关键实测(2026-07-03)**:窗口不在前台可见渲染时,**任何 API 手段都推不动重绘**——`view fit` / `zoomToAll` / `startCalculatingRatline` / `openDocument(当前doc)` / 切到别的 tab 再切回,实测全部无效。**唯一的修法是把 EasyEDA 切到前台、让目标文档成为可见的活动 tab,再抓。** 不要指望有 reload/refresh 命令能替你重绘隐藏窗口——没有。
3. **数据渲染图只能兜底/标注,不能冒充截图。** 由 `pcb list` 等数据生成的复现图(recap image)可作 fallback 或加注解,但**绝不能当作现场原生截图交付**。
4. **诚实报告。** 交付时逐阶段说明每张图是「原生 EasyEDA 截图」还是「数据渲染复现图」;任一帧 stale 或被数据图替换,必须显式点出。

**建议的 PCB 阶段产物**(每阶段同时保存:原生截图 + 数据快照文件 + 可选兜底 recap 图):

- P2/P3 布局+板框后、
- P4/P5 禁布区+丝印后、
- P7 信号布线后、
- P8 电源布线 / +5V 路径后、
- P8 铺铜/内电层后、
- P10 DRC 通过的最终态。

> **一条命令搞定:`easyeda pcb stage-snapshot --stage "P7 routing" [--out ./rec]`**
> 它在一次调用里:①抓原生 PCB 截图 → `<out>/<stage>/snapshot.png`;②批量落盘数据包
> (`components/tracks/vias/pours/nets/drc` .json);③写 `stage.json` 清单;④对帧**把关**。
> 把关规则(录制脚本 `set -e` 就能天然 gate):
> - **前台 tab 不是 PCB** → 非零退出(截图会是错文档,比如原理图)。先 `easyeda doc switch <pcb>` 切过去再抓。
> - **BLANK 帧**(窗口没渲染)→ 非零退出,提示把 EasyEDA 切前台重跑。
> - **STALE 帧**(传了 `--previous-sha256` 且字节相同)→ 非零退出;确要接受传 `--allow-stale` 降级为告警。
> - DRC 不干净 → 只告警不拦(照常留档)。
> 逐阶段串联:把上阶段输出的 `sha256` 用 `--previous-sha256` 传给下阶段,即可链式检测「这一阶段画布有没有真的变」。

## 切到 PCB — 阶段流水线(顺序是硬约束,实测踩过才定的)

原理图过门(DRC 干净 + 已保存)后,转 [`pcb.md`](./pcb.md)。**关键:禁布区和丝印对齐要在布线之前做**——布完线再加禁布区/挪丝印会逼你返工重布(实测:天线区后置,把已布的 BLINK 逼到重绕)。

```
P0 新板/切板 → P1 导器件 → P2 摆放(留装配位) → P3 板框 → P4 禁布区(靠前!)
→ P5 丝印对齐位号(靠前!) → P6 可布性门 → P7 布线 → P8 叠层+电源+铺铜
→ P9 极性/板注丝印 → P10 DRC+check 门 → 💾 save
```

**每过一个阶段发一条通知**(用户能实时跟进):`easyeda notify --message "完成 P7 布线,下一步 铺铜" --type success`。

> 🎬 **录制/演示模式下额外一步**:发通知的同时,在下列标了 📸 的阶段跑一条
> `easyeda pcb stage-snapshot --stage "<阶段名>" --out ./rec --previous-sha256 <上帧sha>`——
> 它一次性抓原生截图 + 落盘数据包并对帧把关(BLANK/STALE/错文档都会非零退出)。
> 前提:**EasyEDA 必须在前台、目标 PCB 是可见的活动 tab**,否则抓到的是空白/错帧(没有 API 能替你重绘隐藏窗口)。

- **P0 新板**:要全新 PCB 页用 `easyeda pcb new-board`(建 Board 壳→灌 PCB 两步,单 `createPcb` 是 no-op)。⚠️ 一个原理图只能属于一个 Board:若原理图**已绑板**,`new-board` 会**拒绝**(否则会把原理图搬进新板,旧板只剩 PCB=「原理图没了」)。既有板里直接布局即可;确要搬才加 `--force`。
- **P1 导器件**:`pcb import-changes` 会**弹 UI「应用修改」**(平台限制,无 headless apply)——要全自动改用 `pcb add-component` 逐件放。导完 notify。⚠️ **落件种子坐标决定板子大小**:`auto-place` 只把卫星吸附到主芯片边缘,**主芯片锚点原地不动**——spec `board` 为 `"compact"`(客户没给板框)时,主芯片必须按**紧凑网格**播种(模块中心距 ≈ 芯片包络 + 300~400mil 布线通道,别撒到 2000mil 开外),边缘件(USB/端子)直接种在预期板边线上。
- **板框顺序两条合法路径(#97 消歧)**:`place-constrained` 的贴边启发式**需要板框**才能定边,而"无客户尺寸时先布局再成框"要求先摆件——二者不矛盾,按有无机械约束分流:
  - **有机械尺寸/外壳约束**:P2 先据 spec `board` 建**粗板框**(`outline-set`/`outline-round`)→ 再布局(`place-constrained` 贴到真实边)→ 用户确认布局+板框。
  - **无机械尺寸**:P2 先**粗布局**并生成**临时大板框**(`outline-fit --margin` 给宽余量,让 `place-constrained` 有边可贴)→ 完成布局后 `outline-fit`/`outline-round` **收紧板框** → 用户确认。
  两条路径都以"布局确认(P2 停点)+ 板框确认(P3 停点)"收尾,再进 P6 可布性门。
- **P2 摆放 — 按优先级分档,每档过确认(2026-07-09 走查#1 用户反馈定型)**:
  **摆放前先问两个决策**(见 design-decisions.md #13/#14,里程碑档必问):① **单面还是双面布局**(SD 卡槽、去耦帽这类矮件适合底面,双面省板但双面贴装贵);② **焊接工艺**(产线贴片可用 0402;手工焊接封装下限 0603/0805,直接影响选型与间距)。
  **回答后立即落盘,不能只记在对话里**:`pcb stage set-assembly --profile hand-solder --min-gap 40 --large-pad-access 60`(或 `--profile reflow`)。手焊的 40mil 是普通器件外框间距地板;USB 外壳脚、SOT-223、模组大焊盘等至少留一个 60–80mil 烙铁进入方向——**这条 gate 已机械检查**(solder-access:每器件 bbox 四侧至少一侧 ≥ `largePadAccessMil` 净通道,四面被围 = gate 失败,`confirm-layout` 拒绝;进入方向是否合理仍截图复核)。电气 DRC clearance 不能替代此门。`pcb auto-place` 的 `--assembly-gap` 默认自动取项目 profile 的 min-gap(摆放与门用同一间距)。完成 P2 全布局后运行一次 `pcb layout-lint --gate`,通过才允许 `confirm-layout`(确认摘要会打印 profile/min-gap/tight/access 数);P3 改板框会使该结果失效,P6 必须在最终板框上重跑。(issue #99)
  **S0 有 `flow` 时,动手摆件之前先看一眼骨架(#167,只读)**:跑
  `easyeda pcb floorplan --spec <s0-spec.json>` —— 它把 `flow`(如 `["POWER","MCU","RF","ANT"]`)沿流向轴切成
  **有序功能带**(带宽按各段器件面积分配),并把 spec 里写了 `ref`+`edge` 的连接器钉到目标边,
  给出一张「板子该怎么分区」的骨架 + `unzoned[]`(哪些件还没归属)+ `warnings[]`。
  **⚠️ 它不搬器件**——落笔仍走下面的分档流程(`place-constrained`)。之所以先看一眼:floorplan 决定的是
  分区本身,这件事错了后面搬多少次件都是白搬。与 `pcb zones` 并存不互斥:zones 是固定九宫格
  (表达「MCU 在中间」这种位置意图),floorplan 表达它表达不了的**顺序 / 比例 / 段数**。
  方向不强制——已有器件更接近反向时它按反向切带(输出 `reversed=true`),不会把一块本来就摆对的板翻过来重排。
  **优先级档序(每档摆完→截图/坐标表向用户确认→锁定→`pcb stage confirm-tier <n> --parts …` 落档,再摆下一档;#125 起分档是机器状态不再靠自觉)**:
  每档确认记录该档器件清单+**姿态指纹**:档间递进强制(跳档被拒)、动了某档的件只作废该档及其后(前面档存活)、`--empty` 声明空档(如无 RF 板)、档 4 缺省=其余全部;**`confirm-layout` 拒绝在四档未齐/有无主件时封章**(`--force <理由>` 审计放行)。`pcb stage status` 看梯子。
  1. **安装孔/结构孔**(M3 四角等)——最先放+**锁定**,后续所有档避开垫圈净空(M3 头 Ø6mm ≈ R118mil);孔后置必然与边缘件冲突(实测:四角 IPEX/USB 全压在垫圈区上)。
  2. **边缘接口件**(有开口方向的:端子/USB/SD 卡槽/排针/按键/IPEX)——按 spec 的出边意图放到板边,开口朝外;这一档**必须用户确认**(朝向、边序是装配体验,agent 猜不了)。
  3. **主芯片 + RF 链**(QFN/SOP 锚点 + 天线馈线簇)。
  4. **卫星件**(去耦/上拉/RC)——只有这一档交给 `pcb auto-place`/合法化器;`--assembly-gap 40`(留烙铁位)。
  **分区先落盘(#126)**:P2 起手把 S0 spec 的 `modules[].zone` 灌进 PCB 侧——`easyeda pcb zones set --spec <s0-spec.json>`;之后 `place-constrained` 会把被 claim 的主芯片/卫星件**摆进对应分区**(边缘件豁免),`pcb check` 的 **zone-violation** 持续核查「S0 拍板的分区在布局里落实了没有」。**spec 没有 zone 的模块不用硬编**;分区违规=WARN 不阻断,但交付前应清零或说明。
  **一键分档布局**:`easyeda pcb place-constrained` 自动做档1-4——按**类别启发式**(board_edge/user-facing)把边缘件贴边+锁定、把非对称连接器(USB/SD/IPEX)几何化朝外→主芯片/晶振锚定→卫星合法化,确定性根治打地鼠(边缘件不会被卫星挤走)。**现已消费块 `placement.<ref>.edge` 语义**:`edge="user-facing"` 的连接器(USB/SD/端子/排针)会被**分组到同一条共享边并沿边居中紧凑排布**(≥2 件才触发,`:grouped` 标在 diag),外部 I/O 聚到一条可达边而非各贴最近边散开(修偏散板);`edge="any"`(RF天线/模组)保持各自最近边。⚠️ **仍不解析块 `placement` 里的自由文本 `orientation`**(如"IPEX 座置板边、天线走线短直"这类 per-role reason)——所以边缘件档仍要**先 `easyeda blocks show <id>` 读 `placement`(edge/side/orientation/reason)**,连同边序摊给用户确认(P2 停点,确认后 `easyeda pcb stage confirm-layout` 落 `placement_confirmed`;**移动器件会失效该确认,需重新确认**),不能靠工具代劳;卫星件贴脚距离读块 `pcb_layout` 的 `decap-adjacency`/`xtal-adjacency`(如去耦 ≤2mm)。跑完 `outline-fit`→放 M3 孔→复核净空。**每档动手前必读真实几何**(`pcb list --include-bbox`,bbox 含 courtyard 常比封装大 40%+,L501 类功率电感可达 558mil)——猜尺寸摆位必被 lint 打脸。RF/天线件周边别塞小件。**紧凑度自检**:板框内面积 / 器件 courtyard 总面积 明显 >3 = 太空,回 P1 收拢主芯片种子再来。
- **P3 板框**:`pcb outline-round --rect … --margin 120`(**默认圆角**,贴器件包络;半径 ≤ 四角 M3 孔外缘距板边、别切孔,无孔约束取 2–3mm,见 `pcb-layout-conventions.md §2.5`);spec `board:"compact"` 时 margin 收到 **50~120mil**,天线端板边贴模块天线区顶(天线本就该在板边,keepout 条越短越省板)。**插头受体连接器**(USB-C/DC jack)在直边段**突出板框 ~0.5–1mm**(§2.2,焊盘留板内),圆角只在四角不影响。**板框定稿并经用户确认后 `easyeda pcb stage confirm-outline` 落 `outline_confirmed`(需先有 `placement_confirmed`;`outline-fit`/`outline-round` 改框会失效它,须重新确认)。**📸 录制模式:布局+板框成型后抓一张阶段截图。
- **P4 禁布区(靠前!)**:天线/挖槽用**一个多层区域**即可——`pcb region create --layer 12(多层) --rule no-pours --rule no-wires --rule no-fills`,一个区域盖全铜层,**不用逐层建 4 个**;内层用「填充区域」禁止,不需要 no-inner-electrical。**删旧区域要「删完校验再建」**——delete 紧跟 create 同批次会竞态,删没生效就累积。RF/天线器件清单与禁布层范围读 S0 方案书 spec 的 `rf.parts` / `rf.keepoutLayers`,这里不重新判断该不该禁、禁哪些层。**RF 块的 `pcb_layout` `rf-keepout`/`balun-mirror`(severity=must)与 spec.rf 一并 `blocks show` 读。**
- **P5 丝印对齐(靠前!)**:`pcb silk-align`(位号摆正+位置感知+`--spacing` 装配间距)。导入的位号常 180° 倒置,这里一并摆正。放布线前,让布线避开丝印占位。📸 录制模式:禁布区+丝印就位后抓一张阶段截图。
- **P6 装配+可布性门(强制,#97/#99)**:P3 最终板框确认后重跑 `pcb layout-lint --gate`(P2 的初检会因改框失效)。门读取项目 assembly profile;同时要求 0 overlap、0 off-board、**0 tight spacing**、≥ `--min-score`、ratsnest 交叉 ≤ `--max-crossings`。手焊 profile 未设置或任何器件低于40mil时必须失败,不得进入布线。**通过才重新落 `pre_route_passed`**;P7 布线命令默认要求 `outline_confirmed` + `pre_route_passed`,否则拒绝;确需推进用 `--force <理由>` 显式授权并记入审计(**仅本次执行有效**——不落任何确认,下次无 `--force` 照样被拦)。用 `easyeda pcb stage status` 查装配档案与阶段。
  **诊断视角:`pcb layout-score`(#167)——只有一个门,别跑成两个门。** 两者分工是硬的:

  | | `layout-lint --gate` | `pcb layout-score` |
  |---|---|---|
  | 回答 | **能不能布线**(硬门) | **布得好不好**(质量表) |
  | 输出 | 单标量分 + pass/fail | 九维各自 0-100 + 加权综合 + **逐器件归因** |
  | 门禁 | **是**,落 `pre_route_passed` | **不是**,不落任何 workflow 确认 |
  | 何时跑 | P6 必跑,通过才进 P7 | 门挂了要**定位原因**、或想把板从"能布"提到"好看/好造"时跑 |

  用法:`easyeda pcb layout-score --spec <s0-spec.json>`(带 spec 才解锁 flow-order 与 internal 连接器判定;
  不带就是那两维 skipped)。**读报告先看三件事**:① `blocking` 有没有(短路/重叠/出板框 = 一票否决,
  跟 gate 同源);② 摘要里的 **`N skipped`**——skipped 是「没测」不是「满分」,别把「7 维 90 分」读成全面体检;
  ③ 最弱那几维的**归因列表**(报告里长这样:`↓ 齐整度(tidy)（80.0）拉低它的是: C2 −20.0 — anchor 离最近 5mil 格点 0.0015mil`),`penalty` 就是「先动谁」的排序。
  `--only tidy,compact --all` 只深挖两维;`--min-score 75` 才会让分数不达标非零退出(不设则只有 blocking 才非零)。
  **权重和阈值是待校准初值**,好板某维掉分优先怀疑度量而不是板子——用 `pcb dump` 把好板存成 fixture、
  `layout-score --from` 离线复现,再回改代码。
  **签字时自动留质量快照**:`pcb stage confirm-layout` 会 best-effort 拉一次 layout-score,把综合分+
  逐维分+skipped 数记进 workflow 状态(`GateSummary.Quality`)——打分失败只警告绝不阻断签字;
  `--min-score` 显式给了才当门(默认 0 只记录,理由:尺子未校准不担硬门)。
  **快照的消费侧在 `workflow status`**:普通 status 渲染上次快照(综合分+最弱三维+未测维数+记录时间,
  没记过会明说);`--reconcile` 时还会实时重打分做**逐维 diff**——掉 ≥5 分的维标 ⚠ 提示
  (「上次 confirm-layout 后 tidy 从 90 掉到 60——布局被谁动过?」),上次 scored 这次 skipped 报
  「该维失去可测性」而不是当掉到 0 分;实时打分不可得时明说没做对比(没测≠没变)。
  全部只提示不拦截——status 不是新的门。
  **精修**:tidy 类低分交 `pcb refine`(打分驱动、默认 dry-run、逐步回滚;唯一变换器 grid-snap,
  其余维和全部 blocking 报告会明确指回 place-constrained/手工)。
  **门禁的机械强制面(#97 后续,2026-07-12)**:① 状态**全局持久化**在 `~/.easyeda-agent/workflow/<project>.json`(换 cwd 跑 CLI 骗不过门;`EASYEDA_WORKFLOW_DIR` 可覆写);② **daemon 在 /action 派发层同样拦截** `pcb.line.create`/`pcb.via.create`/`pcb.import_autoroute`(raw HTTP 调用也绕不过),且任何摆放/板框类 action(component.modify/move/arrange/align/add/delete/import_changes/outline.set/clear)成功后**自动失效下游确认**并在响应 warning 里报 `workflow stage invalidated`;③ `confirm-layout`/`confirm-outline` 会把签核**指纹绑定**到当时的器件坐标/旋转/层与板框几何——GUI 拖动、`debug.exec_js`、其它 agent 的门外改动,会在下一次 gate 时指纹失配 → 自动失效并指回该重确认的阶段。
- **任意阶段切入 / 会话恢复(workflow 命令族)**:换了模型、丢了上下文、或用户手改了板子,都不需要重走流程——
  1. `easyeda workflow status --reconcile`:拉实况(器件数/板框/已布线数)+ 校验指纹,自动失效漂移的确认,报告不一致(如「有走线但从未过门」);
  2. `easyeda workflow advance`:幂等推进——机械验收(layout-lint gate)直接代跑,人工签核点(confirm-layout/outline)停下并打印**下一条该执行的命令**(非零退出,脚本循环天然停在签核点);
  3. `easyeda workflow confirm layout|outline` = `pcb stage confirm-*` 同实现;`workflow init` 新板起手建 marker。
  每一步的 `next:` 输出就是「默认继续按 workflow 走」的入口,不依赖任何 agent 记忆。
- **P7 布线 — 三档阶梯(2026-07-09 定型)**:按密度选档,密度预算=layout-lint 的 ratsnest 长度/交叉数。
  > **档位铁律(= 顶层「档位默认」表的展开)**:稀疏板 → ① route-short;**稠密板默认 = ② 人机协作档(停下请用户点原生自动布线),不是 Freerouting**。③ Freerouting 只在**全 headless(无用户可点)**时兜底,**绝不拿它顶替 ② 去图 autonomous**——用户选了 ② 就按 ② 停手交回。(2026-07-09 实测踩过:图省事直接上 Freerouting = 违反本档。)
  **P7.0 关键网络先行(2026-07-10 定,先于把剩余交人工)** —— 自动布线器最不擅长的两类不丢给它、自己确定性布好并**锁定**,只把剩余普通信号交人工档 ②(是对 ② 的**增强**,不是替代)。
  **一条命令承载(#127)**:`easyeda pcb route-critical`(--dry-run 先看识别与计划)= 下面 1-4 步的机械化——电源按层数选 planes/pour → 差分对(块 signals+名字模式双源识别)45° 成对布线并**逐对实测 skew 报预算** → `pcb.track.lock` 锁定;之后只做第 5 步交人工。单步拆做仍可(--skip-power/--skip-diff/--no-lock 或下述手工命令):
  1. **识别关键网**:读块 `signals` map(`easyeda blocks show <id>` 的 `type:diff_pair` / `length_match_mm` / `impedance`,如 USB_D 90Ω、RS485_AB 120Ω)+ 电源网(5V/3V3/12V/VBUS)。
  2. **电源先铜(稳供、低阻)**:**2 层一键 `pcb power-pour`**(GND 双层 pour + 每条电源轨在自身 pad 区局部 pour,全动态铺铜不短路);4 层用 `power-planes` 内电层;主干/大电流也可手工用**大面积填充块** `pcb fill create --net 5V --layer 1`(net-bound,solid/mesh);GND 用 `pour`。**别拿细线穿焊盘阵布电源**(route-short 默认就跳过电源=对的;真要走线时 `route-short` 会**按 net-class 角色给规范宽**——支线 3V3≈10 / 主干 +5V≈15 / 大电流 VBUS≈20mil,见 `pcb net-classes`;`--width-power` 强制全电源同宽)。fill(静态硬块、实、后续信号要留 clearance、**异网相叠即短**)vs pour(会退让障碍、连通性靠 rebuild、异网不短)——大电流/参考平面用 fill/plane,一般电源/多轨并存用 pour。**兜底检查**:`pcb check` 的 **power-not-poured**(裸电源网没铺铜)+ **width-under-spec**(电源线偏细)会逮住漏网的。
  3. **差分/等长先布**:USB D±、RS485 A/B 成对布——本板这类很短(连接器→芯片),**成对并行、尽量短、≤5mil skew 即可,不用蛇形调谐**;`route-short` 或手工 `pcb track`。
  4. **锁定关键铜**:布完 `pcb track-lock --net 5V --net USB_DP --net USB_DM`(按网锁 track/**arc**/via/fill;或 `--all` 锁全部已布铜,`--ids` 锁指定,`--unlock` 解锁)把它们锁死,**否则人工自动布线器 / `pour-rebuild` 会把手布的关键线冲掉**——锁是本流程的地基。0.15.2 起走 typed action `pcb.track.lock`(从 debug-exec 版毕业,连 beautify 弧也锁得到);pour 永不锁(要 reflow)。
  5. **交人工档 ②** 布剩余普通信号(避开锁定铜)→ 最后 `pour-rebuild` 让 GND 退让全部已布铜。
  ① **启发式档** `pcb route-short`:稀疏板(esp32-mini 级,交叉 <100)一次布通;
  ② **原生 UI 自动布线(人机协作档,稠密板推荐默认)**:官方 autoRouting API 未放出前(pro-api-sdk #28 卡 web 版本),agent 备好布局/叠层/禁布/规则后**停下来,请用户在 EasyEDA 顶部菜单点「布线 → 自动布线」**,跑完 agent 接手验证(DRC/check)+铺铜+丝印——一次点击换全套官方路由器(推挤/撕绕/规则原生一致),省掉外部 DSN/SES 往返的全部坑;API 放出后此档自动升级为无人值守;
    > **交用户前必念的「自动布线对话框」提醒清单(做过 P7.0 关键网络先行/有锁定铜时尤其重要,漏一条会毁掉 P7.0)**:
    > 1. **「已有导线/过孔」必须选「保留」,绝不选「移除」** —— 移除会删掉已布好并锁定的电源内电层 / 缝合过孔 / 关键网,P7.0 全白做。
    > 2. **「布线图层」只勾「顶层+底层」,取消「内层1/内层2」** —— 4 层板内层是 GND 内电层 / VDD 电源层,信号布进去会和平面打架(单/双层板此条不适用)。
    > 3. **「忽略网络」加已在平面的电源网(GND、3V3/VDD 等 P7.0 已用内电层的网)** —— 它们已靠过孔连好,别让路由器再布成冗余线。
    > 4. 其余默认即可:所有网络 / 45°(或 90°)/ 完成度优先。做过 P7.0 差分先布时,把已布的差分网也加进「忽略网络」保护。
  ③ **外部迷宫档** `pcb autoroute`(Freerouting,需 JDK21):全 headless 场景的兜底。**教训**:rip-up 后必须 save→reload→验证 0 轨再导出 DSN(残留叠布=上一代轨与新轨 0mil 重叠,499 条 ClearanceError 实测);电源网别抢在迷宫档前用 power-planes 缝合(缝合孔会和密轨打架,161 条实测)——顺序=先全网迷宫布通,后 pour-rebuild 让面通过路由过孔接通。
  原 route-short 细则:**现在自带多层布线**(默认开):同层太长或跨层的 hop 不再推迷宫档,自动用 via 换到空闲对层走 trunk 再 via 回来(dogbone,via 偏离焊盘),铺得开的板也能一次布通信号(实测 esp32-mini 15 段+2 过孔,长 USB hop 走 L2)。`--no-multilayer` 退回旧的只布短同层线。手工换层仍可用 `pcb via-hop`;个别擦焊盘绕行用手工 `pcb track`;单颗错 via/track 用 `pcb via-delete/track-delete --ids` 精准删,别整网 `rip-up`。**手工修线三律**:① 优先多层——长网/交织网用 via 对借 L2 直跑,别死磕单层平面性(交织对在单层是拓扑无解,推演再久也无解);② 动笔前先 `via-list`/`track-list` 拉全量已有铜形(power-planes 缝合 via 的环晕 r≈12 在 L2 是硬障碍),按坐标排车道;③ 板边走廊记住铜-板框规则 0.3mm(11.8mil)。**注意 `rip-up` 会连电源缝合过孔一起删**——之后重跑 `power-planes` 补回。**⚠️ mutation(rip-up/route/delete)后先 `doc reload` 再读/判/DRC**——否则 line.list/DRC 读 stale(见 [[pcb-stale-reads-need-doc-reload]]);确定性复位=rip-up→save→reload。📸 录制模式:信号布线完成后抓一张阶段截图。
- **P7.9 走线美化(可选,布线定稿后)**:信号全布通并锁好关键网后,`pcb beautify` 把直角/锐角走线圆滑成圆弧(改善美观+可制造性,减少尖角蚀刻风险)。**先 `pcb beautify --dry-run` 预览**(只报 paths/arcs、不动板),满意再实跑。它自带安全网:删+重建轨迹→DRC 二分修复违规拐角(缩半径或退回直角)→自动 `pour-rebuild`(同网 GND 键合会 stale);差分/等长网走同心圆弧保护(该 build 暴露 `getAllDifferentialPairs` 时,否则保直角)。只动铜层、绝不碰丝印/板框。默认 `--radius-ratio 3`(半径=最大线宽×3),稠密板可调小(`--radius-ratio 2`)。**顺序**:放在 P7 布线之后、P8/出 Gerber 之前;跑完 `pcb save`。**上游告警照搬**:焊盘-走线连接偶需人工复核、RF/高速网建议排除全局美化(用 `--net` 单网做)、出 Gerber 前预览确认。能力吸收自开源扩展 Easy_EDA_PCB_Beautify(m-RNA,Apache-2.0)。
- **P8 叠层+电源+铺铜**:层数与地策略(单 GND PLANE 还是分区 pour + 桥地)读 S0 方案书 spec 的 `stackup` 字段,这里不重新选。**热焊盘处理**:读本板各块 `pcb_layout` 的 `ep-thermal-vias`/`ep-ground-stitch`,在 IC EP 焊盘下打散热 + 接地过孔阵列(≥4 vias)。**4 层层分配默认(客户常规叠层)**:Top(1)信号 / **Inner1(15,物理第2层)= GND 内电层(PLANE)自动铺铜** / **Inner2(16,物理第3层)= VDD/电源层** / Bottom(2)信号——`power-planes` 默认就这么分(`--gnd-layer 15 --power-layer 16 --gnd-plane`,自动灌铜)。多电源轨时**主轨(引脚最多,如 3V3)上电源内层**,零散轨(+5V/+12V/VBUS)走线或小 pour(不与主轨挤同一内层,否则冲突)。`pcb stackup set --layers 4` → `pcb power-planes`(GND内电层+VDD内层+缝合过孔)→ 顶/底 `pcb pour-fit --net GND --replace=false`(`--replace` 默认 true 会清跨层同网铺铜)→ `pour-rebuild`(退让禁布区)。**GND 内层的正确终态是 内电层/PLANE**——`power-planes` 默认(`--gnd-plane`)按已验证配方自动完成:先在 SIGNAL 态铺网络铜 → `stackup set --plane` 翻 PLANE → `pour-rebuild`,填充存活、DRC 干净(与 `pcb-layout-conventions.md` 口径一致)。**顺序不能反**:在已是 PLANE 的层上直接新灌铺铜会掉到 L1 且 netless(坏路径);翻回 SIGNAL 只是诊断手段,不是终态。⚠️ **PLANE 生成后别再打异网 via**——官方缺陷(easyeda/pro-api-sdk#32)新 via 不挖 anti-pad,DRC 报 Plane Zone to Via / Hole to Plane Zone 且 `pour-rebuild` 不补救;`pcb check` 的 **via-crosses-plane** 规则会标出,修法:优先删 via 改外层走线,或 `easyeda doc reload` 后 `pcb pour-rebuild` 再跑 DRC 确认。📸 录制模式:电源布线(+5V 路径)后、以及铺铜/内电层完成后,各抓一张阶段截图。
- **P9 引脚级丝印/极性/板注**:**先读块 `silk` map**(`easyeda blocks show <id>` 的 `pins`/`label`/`note`)——**逐脚标注**每个对外引脚:电源端子 **+/−**、总线 **A/B/G**、UART **RX/TX/GND**、极性件阴极 **K**,加功能名 + A/B 反向警示(如 SP3485 `A/B=IC`)。**硬规则——装配后不被遮住(= 顶层铁律 11)**:每个标记落在**器件本体/courtyard 之外、对齐各自焊盘**(端子塑料罩/卡座壳/按键帽会盖住其 footprint 内的丝印=等于没标);**per-pin 标记优先占位,功能标签再绕开**(先放功能标签会把 per-pin 挤脱位);边缘 header 头顶被占时脚名放本体的**板边侧**成一行。详见 `pcb-layout-conventions.md §9.4.1`。`pcb silk-add`(锚点=**左下角**;特殊字符如 `−`/`3V3` 渲染比 len 估宽宽,靠 `getPrimitivesBBox` 实测校正别信估算)+ 板名版本 + credit;`pcb silk-set --ref board --align centerx` 居中。⚠️ silk-add 在 **PCB 非前台**时报误导性「参数不正确」——写前先 `doc switch <PCB>`。
- **P10 门**:`pcb drc`(passed)+ `pcb check`(0 issue)双清零,再 `pcb save`。**其中 pcb check 侧已机械化为 `post_route_checked` 工作流阶段**——布线后 `easyeda workflow advance` 自动跑 check 门(ERROR / power-not-poured / width-under-spec 三项清零才确认;其余 WARN 报告不拦),任何布线类 mutation(track/via/pour/fill/beautify/import_autoroute)会自动失效此门,改完线必须重新过。next 指引会在此门未过时拒绝进入 P9/交付。**线宽/过孔/间距下限**别低于 `references/fab-rules-jlcpcb.json` 的 clamp floors(live `pcb.drc.rules` 优先)。**电源规范线宽 + 电源铺铜**由 `pcb check` 的 **width-under-spec**(电源线 < 其 net-class 规范宽)+ **power-not-poured**(电源网未铺铜)两条把关(WARN,`--strict` 计入门禁);电源该铺的用 `power-pour`(2层)/`power-planes`(4层)铺掉,该走线的让 `route-short` 按 `pcb net-classes` 角色宽走。DRC=0 且本板含手工搭建、验证过的标准外围时,按 `standard-blocks-contributing.md` 顺手回流入库。**两条硬注意**:① **手术后 GND 断连=铺铜 stale,不是真断**——删/改 via/track 后 DRC 冒一堆同网(多为 GND)Connection Error,是 pour 连通性 stale,跑 `pcb pour-rebuild` 让飞线重算即恢复(track↔via 本身导通,pro-api-sdk#31 误诊已订正);**别再无脑配键合 fill**。② **DRC 需前台**——后台/被遮挡窗口 DRC 重画布计算永不完成;超时就把 EasyEDA 切前台**单发一次**,绝不循环重试(daemon 已防重入,重复下发直接拒 `ACTION_BUSY`)。逐条修错用 `pcb drc --json` 的 `{rule,net,x,y,objs}` 定位,`objs` 直接喂 `via-delete`/`track-delete`。📸 录制模式:DRC 通过的最终态抓一张阶段截图,作为交付收尾图。

## 反模式(实测踩过的坑)
- ❌ 全堆一页、不分页 → S1 强制分页。
- ❌ 无图纸/无 A4 sheet 就开始摆 → S1 图纸门禁拦住。
- ❌ 用坐标外扩代替分页 → 按 A4 可用区拆模块分页。
- ❌ 一件一件随手摆、芯片和外围分家 → S2/S3 按组摆。
- ❌ 摆完不验就布线 → 元件覆盖、线压外围。S5 layout-lint 门拦住。
- ❌ 靠截图判断有没有覆盖 → 截图 stale。看 `layout-lint` 数据。
- ❌ **录制/演示时拿数据渲染复现图冒充原生截图** → 二者必须区分交付;原生截图 stale 时先重绘重抓,兜底用数据图必须明确标注(见「录制/演示模式」)。
- ❌ DRC 报错放着不管 → S6 闭环必须清零再 save。
- ❌ **放/连一大堆都不 save** → 窗口重载或 daemon 重启全丢(实测踩过)。每阶段门后存盘,整板放置每 ~10 件存一次。
