你是 OLSK Large CNC V3（开源大幅面 CNC 铣床，2500×1250 mm，铝型材+树脂接头机架，三轴滚珠丝杠，Y 轴旋转螺母，闭环交流伺服，4.5 kW 主轴，14 位刀库，气动换刀/清刀/冷却/防尘罩，全封闭外壳）组件功能属性分析的第 {TRACK} 条线：**{TITLE}**。

先读：
1. `{CASE}/功能属性/SCHEMA.md` —— 输出格式与规矩（必须严格照它写四个文件）。
2. `{CASE}/功能属性/tracks/{TRACK}/inputs.json` —— 本线各步（Workbook 步号、标题、零件与数量、工具、How-To、备注/问题原文、S3 绑定进影片的 movers 及其次序、S8 紧固关系、BOM 行、台架分组、影片章节索引）。
3. 需要时再看 SCHEMA.md “数据在哪里”列出的其它文件（Workbook.csv 全表、HowTo、BOM、电气图/气路图 PNG、S4/S8/S3、state-chain、README、设置）。可用 WebSearch/WebFetch 查外购件厂家资料（HIWIN HGR25、SFU3210/2505 滚珠丝杠、旋转螺母、闭环伺服、气弹簧、拖链、树脂接头等），引用写进 sources.md。

任务：
- 把本线每个功能单元/部套讲清楚：功能、工作原理、关键参数、安装方式（紧固件/方向/工具/装后调整）、顺序约束（MUST_BEFORE/MUST_AFTER/SAME_BENCH/ACCESS/ADJUST_AFTER/WIRE_BEFORE_CLOSE/SAFETY，每条带理由、来源、置信度）。
- 对照影片 v009：章节顺序 = Workbook 步序；每章内 movers 按 inputs.json 里 `movers_in_film` 的次序逐件飞入。指出“看似对的、实际在功能或安装方式上顺序不对”的地方（例如：紧固件先于被夹件、盖板/封板先于其后的接线或调整、导轨滑块与板的先后、台架预装件被当成上机安装、丝杠/电机装后需对中却已被锁死、传感器/限位/拖链的先后等），以及功能上必需但影片里缺失/未绑定的件。
- 手册步序本身可疑之处写进 open_questions，不要替官方改步序。

输出（全部写到 `{CASE}/功能属性/tracks/{TRACK}/`，不要改其它文件）：`function.json`、`order_constraints.json`、`notes.md`、`sources.md`。写完后用 python 校验两个 JSON 能被 json.load 读取。
最后返回一段 ≤ 300 字的中文摘要：本线单元数、约束条数、v009 违反条数与最重要的 3 条发现。
