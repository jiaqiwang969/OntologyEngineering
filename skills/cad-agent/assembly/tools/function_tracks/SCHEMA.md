# OLSK Large CNC V3 组件功能属性 — 统一输出格式（所有分析线通用）

目标：把这台机器的每个功能单元和部套讲清楚四件事——**它是干什么的、怎么工作、怎么装（紧固件/方向/工具/调整）、装配顺序上它依赖谁、谁依赖它**。
本手册用于审查装配动画（v009）的顺序是否在功能与安装方式上成立，并为下一版工序 DAG（S3 v2）提供约束。参照 `~/163-曹隽-印刷/印刷机组件功能属性手册` 的做法：每条结论标明来源（我们的数据 / 官方资料 / 推断）与置信度。

每条线在 `功能属性/tracks/<track>/` 下写四个文件：

## 1. `function.json`（机器可读）

```json
{
  "track": "B",
  "title": "Y 轴与肩部",
  "units": [
    {
      "unit_id": "U-Y-AXIS",
      "name": "Y 轴（滚珠丝杠旋转螺母驱动的双侧长轴）",
      "unit_class": "MotionAxisUnit",          // FrameUnit / BaseUnit / MotionAxisUnit / SpindleHeadUnit / BedUnit / ToolChangerUnit / CoolantUnit / DustCoverUnit / ElectricalCabinetUnit / WiringUnit / PneumaticUnit / EnclosureUnit / DoorWindowUnit / ExhaustUnit / PurchasedFamily
      "function": "一句话：这个单元在整机里干什么",
      "principle": "工作原理，3~8 句：力/运动/信号/介质怎么走",
      "workbook_ops": ["WB-07", "WB-08", "WB-09", "WB-10", "WB-11", "WB-12", "WB-16", "WB-40", "WB-44"],
      "subassemblies": [
        {
          "code": "WB-07 Prepare Y axis - ball screws",
          "name": "Y 滚珠丝杠预装",
          "function": "…", "principle": "…",
          "key_parts": ["Ball Screw SFU3210 3000mm CNC Profi", "Y Ball Screw Holder (CNC Milled)", "…"],   // 用 Workbook/STEP/BOM 里的原名，照抄
          "purchased_modules": ["SFU3210 滚珠丝杠", "HGR25 直线导轨", "…"],
          "install_method": {
            "fasteners": "16×B-screw M8-25 + Washer + Lock Nut（照抄 Workbook）",
            "direction": "从上方垂直落座 / 沿轴向插入 / 从侧面推入（写清楚参照系：机器 X/Y/Z）",
            "tools": "Allen Key 6 / Wrench 14（Workbook 只在 01.1 写了工具，其余按紧固件规格推断并标‘推断’）",
            "how_tos": ["H2"],
            "adjustment_after_mount": "找平/预紧/张紧/对中/限位调整（没有就写‘无’，不确定写 UNKNOWN）",
            "notes": "Workbook Notes/Remarks/Problems 原文照抄 + 你的解读"
          },
          "sequence_constraints": [
            {"kind": "MUST_AFTER", "target": "WB-03 Fix Y linear guide beams", "why": "丝杠座要靠已固定的导轨梁定位（推断）", "source": "推断", "confidence": "中"},
            {"kind": "MUST_BEFORE", "target": "WB-44 Fix Y ball screw cover", "why": "盖板装上后丝杠不可再调（Workbook 步序 + 功能）", "source": "官方资料", "confidence": "高"},
            {"kind": "SAME_BENCH", "target": "WB-08 …", "why": "在台架上先成组再上机（手册 preparing 分组）", "source": "我们的数据", "confidence": "高"},
            {"kind": "ACCESS", "target": "WB-63 Fix shoulders' cover", "why": "盖板会挡住电机/丝杠的检修与接线口，接线必须在它之前", "source": "推断", "confidence": "中"}
          ],
          "functional_dependencies": ["Y 电机接线 → WB-69/70 之前不能通电试运行", "限位感应开关（homing）要在肩部盖板前装好并接好线"],
          "film_v009_observations": "对照 state-chain（章节顺序 = Workbook 步序）与 S3 绑定的 movers：这一步动画里出现的是哪些件、顺序是否与安装方式一致；有问题写清楚是‘顺序’、‘缺件（未绑定/未动画）’还是‘台架语义’"
        }
      ],
      "key_parameters": [
        {"name": "Y 行程", "value": "2500 mm（README 铣削区域）", "source": "官方资料", "note": "…"}
      ],
      "design_rules": [
        {"rule": "双侧丝杠必须在两肩部固定后再同步对中，否则龙门歪斜", "basis": "推断/行业惯例", "confidence": "中", "applies_to": ["BallScrew", "Shoulder"]}
      ],
      "coverage": {
        "bound_movers_in_film": 37, "unbound_or_not_animated": ["Belt 294-3MGT-15", "…"],
        "comment": "哪些功能上必需的件没有在影片里出现，或出现在错误的步骤"
      },
      "data_gaps": ["Workbook 无扭矩/胶粘/验收字段", "…"],
      "open_questions": ["需要向 InMachines/设计确认的问题"],
      "sources": [{"title": "…", "url_or_path": "…", "used_for": "…"}]
    }
  ],
  "role_definitions": [
    {"role": "LinearGuide", "definition_zh": "…", "typical_install_rule": "导轨先于滑块所载的板；两根平行导轨要以一根为基准找直"}
  ]
}
```

`sequence_constraints.kind` 取值：`MUST_BEFORE` / `MUST_AFTER` / `SAME_BENCH`（应在台架成组）/ `ACCESS`（后装件会挡住前装件的操作/接线口）/ `ADJUST_AFTER`（装后需要调整，调整前不能被后续件锁死）/ `WIRE_BEFORE_CLOSE`（接线要在封板/盖板前）/ `SAFETY`（安全或损伤风险）。`target` 用 Workbook 步号 + 标题。

## 2. `order_constraints.json`（本线全部顺序约束的扁平表，供程序核对）

```json
[
  {"from": "WB-07", "to": "WB-44", "kind": "MUST_BEFORE", "why": "…", "source": "官方资料|我们的数据|推断", "confidence": "高|中|低",
   "parts": ["Y Ball Screw Cover", "…"], "violated_in_v009": false, "evidence": "state-chain 章序 WB-07(7) < WB-44(46)"}
]
```
`violated_in_v009` 由你按 `本体/S3-工序实例绑定.v1.json` 的 `order_index` 和 `装配动画/FILM_v009/state-chain.json` 的章节顺序判定；工序内的 mover 顺序按 S3 `operations[].movers` 的次序（影片按这个次序逐件飞入）。

## 3. `notes.md`（给人读）

每个单元一节：功能、原理、组成与关键参数、安装方式、顺序约束、影片 v009 对照（哪些看似对的其实顺序不对、为什么）、疑点。中文；件号/零件名照抄；数字带单位。

## 4. `sources.md`

所有参考资料（标题、路径或 URL、一句话说明用在哪里）。公开资料可用 WebSearch/WebFetch（HIWIN HGR25、SFU3210、伺服、气弹簧、拖链等厂家手册）；引用时注明；没看过的不写。

## 数据在哪里（只读）

- 官方手册步骤：`功能属性/inputs/Workbook.csv`（114 步，零件/数量/工具/备注/How-To）与 `装配工艺/sop-olsk-workbook.v1.json`（同内容的结构化版）；How-To：`功能属性/inputs/HowTo_Blad1.csv`
- 手册台架分组（哪些步是 preparing 预装组）：`装配工艺/manual-bench-groups.v1.json`
- 本线的切片输入（本线各步的零件、S3 绑定的 movers、S8 紧固关系、BOM 行、S4 机构类）：`功能属性/tracks/<track>/inputs.json`
- 全机 BOM：`功能属性/inputs/BOM_sheet1.csv`（按顶层装配分组：Bed / Frame / Gantry / Z Axis / Electrics / Housing / Pneumatics …）
- 电气图 / 气路图：`功能属性/inputs/schematics/*.png`（用 Read 看图；需要放大可用 `pdftoppm -r 200 -f 1 -l 1 -png <pdf> out` 自己再渲染到你的 scratch 目录）与 `*.txt`
- 机器概况：`功能属性/inputs/OLSK_README.md`；控制设置：`功能属性/inputs/OLSK_Large_CNC_V3_Settings.txt`；刀位表：`功能属性/inputs/ToolsPositions_Sheet1.csv`
- 机构识别（名称词表 + 共轴）：`本体/S4-机构识别.v1.json`；连接关系图（COAX/PLANE/FASTENS）：`本体/S8-连接关系图.v1.json`；工序绑定与台架组：`本体/S3-工序实例绑定.v1.json`
- 影片章节顺序与每章 movers：`装配动画/FILM_v009/state-chain.json`；影片本身：`装配动画/FILM_v009/*_scored.mp4`（可用 ffmpeg 抽帧看某一步的画面）
- 官方视频（介绍/演示，非装配过程）：`/Users/<user>/161-OLSK-Large-CNC-V3/media/official/`

## 规矩

- 只读上述数据，不改它们；输出只写到 `功能属性/tracks/<track>/`。
- 每个结论区分三种来源：我们的数据、官方资料、推断；推断要写"推断"。
- 顺序约束宁缺毋滥：每条都要有理由；把"手册步序本身就有问题"的地方单独列出（`open_questions`），不要替官方改步序。
- 中文写作；件号和零件名照抄英文原名；数字带单位。
