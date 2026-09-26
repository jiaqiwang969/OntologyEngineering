# OLSK-Large-CNC-V3 装配主线工具台账

来源：`~/104-灵犀x2/开源机器人盲测/重建-20260829/tools/`（Poppy 重建通用工具箱，29 文件，原件哈希见 `SOURCE_SHA256.txt`）。
验收器：`validator/validate_acceptance.py` + `acceptance-contract.v1.json`（来自 `104-灵犀x2/工具/装配动画/AUDIT-X1-v015-vs-rejected-20260827-v001/`）。
运行环境：案例根 `.venv`（uv，Python 3.11.14：numpy 2.4.6 / scipy 1.17.1 / trimesh 5.1.0 / python-fcl 0.7.0.11 / cadquery-ocp 7.9.3.1 / pillow 12.3.0）；Blender 5.2.0 LTS（/opt/homebrew/bin/blender）。

| 工具 | 状态 | 改动 |
|---|---|---|
| parse_step_assembly_generic.py | 原样保留（未用于本案） | 无；对 ST-Developer AP242 文本会因 D1/D2/D3 失效（名称全 "?"、2380 边无变换） |
| parse_step_assembly_ap242.py | **OLSK 派生** | D1 实体名后无空格正则；D2 多行复合实例取 RRWT；D3 点/方向/A2P 语句级扫描（折行）；D4 累加器保留残片；D5 字面量内折行 LF 删除；D6 表示关系方向逐边判定（rep_1=父→M1·M2⁻¹；rep_2=父→M2·M1⁻¹），计数写入 adapter |
| s1_xcaf_channel_b.py | **OLSK 新增** | OCCT/XCAF 独立通道（零代码共享），输出实例路径/定义名/局部与世界矩阵 |
| s1_crosscheck_compare.py | **OLSK 新增** | 两通道按路径全矩阵比对 → evidence/s1-world-crosscheck.v1.json |
| validate_controlled_records_olsk.py | **OLSK 新增** | S0 受控记录结构级校验（BOM xlsx 两列/11 组/354 行/2379 件；SOP 31 列/114 步/78 章） |
| 其余 25 个通用工具 | 原样拷贝 | 尚未运行；进入各阶段时若需适配在此登记 |

## 记录
- 2026-09-02 通道 a 首跑（V001）：按原版 SolidWorks 约定 M1·M2⁻¹，得到 Spindle 原点≈(1,1,0)、叶原点跨度 12 m，明显反演；查 RR #21044：rep_1=#8370997（仅含子件原点 A2P）、rep_2=#8370996（父 SR，含全部组件位姿 A2P），即 Fusion/ST-Developer 为 rep_1=子、item_2=父系位姿。V001 输出封存为 `evidence/s1-channel-a-REJECTED-inverted-convention.v0.json`（回归反例），V002 用 D6 重跑。
- 2026-09-02 S1 交叉验证 V001：世界矩阵 1554 叶 + 822 子装配全部逐元素 0 差异（D6 方向判定成立）；结构 FAIL 三类：
  ① NAUO 名含 Part 21 撇号转义 `''`（"0,250'' NPT TEE v1 (2):1"）被通道 a 截断 → 两路径合并（D7 修）；
  ② Frame/Electrics 装配自带实体（XCAF 匿名 `=>[entry]` 组件），NAUO 树无身份 → D8 发 `=self` 叶；7 个零件被 OCCT 拆成匿名子标签（几何归父叶，比较规则 R2）；
  ③ 通道 a 用 PRODUCT.id（1735 唯一）、OCCT 标签用 PRODUCT.name（55 名共用、407 id≠name）→ D9 双字段；S2 工具增量 L1：`--label-map` 按 XCAF 标签 entry 取形（entry 只在同 STEP 字节 + 同 OCCT 版本下稳定，取形时核对标签名）。
  首轮 S2（按名取形）缺 142 定义，作废重跑。
- 通道 b V002：增加 ref_entry/component_entry/occt_version。比较器：结构对齐规则 R1-R3 落盘 + 输出 `evidence/s1-definition-labels.v1.json`。
- 新增 `build_glb_step_bridge.py`：手册 GLB 网格 ↔ STEP occurrence 几何桥（S3 前置）；`build_olsk_sop_record.py`：SOP 逐字搬运；`manual-bench-groups`：手册 Model.jsx 台架分组抽取。
- 2026-09-02 S2：按 entry 取形 919/919；共轴 870 对、平面 1546 对 + 锥 42 对；STL 901/919（18 空包围盒）。
- 几何桥迭代：v1（叶候选、包含度打分）432 叶绑定、质心中位差 47 mm；v2（+子装配候选、平移众数精修）反而 340；
  实测帧拟合本身到 mm 级（主轴/丝杠框完全重合），问题在打分——手册网格把"螺杆+螺母"画成一件、把外购模块画成一件；
  v3 改 AABB IoU 对称打分 → 427 UNIQUE/39 AMBIGUOUS，UNIQUE 质心中位差 0.9 mm；2849 UNMATCHED 中 2480 命中紧固件词表（STEP 无此几何）。
  剩余大件漏配根因：STEP 把机架型材建成 Frame/Electrics 组件自带复合体（[self] 一个定义多实体），SOP 逐根装配 →
  新增 `split_definition_bodies.py` 按 OCCT SOLID 遍历序发体级身份 `<id>#b<k>`，桥 v4 加体候选 + 子装配候选 IoU≥0.5 门槛。
- `build_olsk_s3_binding.py`：动件三类 leaf / body / asm（PURCHASED_MODULE 单元，成员按 S1 子树），唯一消耗按叶或体计。
- 桥 v5：只用 IoU×质心接近（包含度通道把螺钉配到包住它的垫块/主轴，作废）；子装配 IoU 门槛 0.3；
  谱系折叠（近等分候选互为祖先/后代时取祖先最高子装配，体 vs 整叶取整叶）→ 504 UNIQUE / 18 AMBIGUOUS；
  仅 Frame [self] 与 Electrics [self] 需体级身份；脚轮→ Caster 子装配单元、电源→子装配单元、驱动器→整叶。
- S3 绑定 v2：冲突裁决 R-C1（SOP 行点名）/R-C2（最早工序），其余记 re_shown；S5 折叠：深度≥2 子装配若未被 SOP 分解
  （叶消耗工序 ≤1）整体折为求解单元归入该工序，成员=S1 子树全部叶；未绑定叶分类 FASTENER_STEP_ONLY / SELF_BODIES_UNBOUND / NOT_IN_MANUAL_GLB。
- 体展开视图：`derive_body_view.py` 生成 S7/S8 消费用 S1 视图（occ_id 重编号、#b 后缀）与网格目录；robot-config 消费路径切换，权威路径留 paths_authority；
  `extract_body_geometry.py` 逐 SOLID 提解析面并入 S2 parts；共轴/平面图在体展开视图上重建（-体展开 后缀）。
- 移植 X1 `audit_tree_conformance.py`（K-PATH-15 树一致性审计：TIER_A/TIER_B/NO_EDGE_ENTRY/FASTENER_BEFORE_CLAMPED）与 `order_by_tree.py`（树引导工序内排序）；
  唯一改动：state-chain 路径 `工具/装配动画/<run>` → `装配动画/<run>`。原件哈希追加到 SOURCE_SHA256.txt。
- 新增 `本体/S4-机构识别.v1.json`（K-KIN-01）：词表+共轴证据分类（丝杠 12/螺母 8/导轨滑块 45/气缸 24/铰链 18/轴承 14/带 10/带轮 4/管线拖链 135/主轴电机 11），
  路线裁决：有受控 SOP 步序 → 不走论文派生规划；机构作刚体或 S5 单元安装；螺纹/压装界面按 K-PHY-11/K-PATH-09 不认证。
- S7 首遍（114 工序，FCL）：485 动件/112 脱离/158 NO_RELEASE/215 E 类；粗端判据分域验证三类均 <0.90 → 全部禁用（K-DIR-02/K-GOV-11），17 件降级；交叉验证 MISSING → HOLD。
- 单元动件 kind 统一为工具箱约定 `solver_unit`（此前写作 asm 使 S8 连接图 KeyError）；含单元的 14 道工序首遍重跑。
- `sweep_certify_bvh_generic.py` 增量：`--out-suffix` 分片输出（本机 4 个 Blender 进程并行），合并后供 merge_s7 交叉验证（K-GOV-02）。
- `build_assembly_animation_generic.py` 增量：上轴符号优先取 robot-config coordinates.up_axis_sign（S3 几何桥实测 +Y），foot/head 关键词法仅作交叉校验（非人形装配会默认 -1 渲倒，F-SCN-08 同类）。
- `build_assembly_animation_generic.py` 增量：求解单元成员若定义本身无几何（manifest EMPTY_BBOX，如 Fusion 'Cut001'、空接头定义），记入 compiled_summary.empty_geometry_members 且不显示，不再整片拒绝；动件本身无几何仍拒绝。
- `reopen_readback_generic.py` 增量：跳过 chain.compiled_summary.empty_geometry_members 中的成员（定义无几何，构建阶段已列账），其余成员仍逐个核对象与 f0/f1 位置。
- 2026-09-03 S7 行走 114/114：485 动件 474 可播/11 STUCK；56.1 分片并行（6 片 + 单实体 5 片）合并。FILM_v001：冻结 474+11=485 闭合；构建 6394 帧/97 机位；A9 回读 474 件 0.00 mm PASS。
- 树一致性审计 v001：TIER_A 421 / TIER_B 43 / NO_EDGE_ENTRY 10（多为台架首件：机架首根型材、皮带、带轮，需分"台架锚"与解析面覆盖缺口）/ 紧固件叶位违规 1。
- **词表缺陷（待 v002 修）**：fastener_lexicon `(?i)\bscrew` 命中 "Ball Screw"、`\bnut\b` 命中 "Ball Screw Nut"/"Driven Nut"，把丝杠与丝母误判为紧固件（影响工序内顺序、S8 fasten 边、树审计"紧固件先于被夹件"1 例）。修正：加负向前瞻/后顾排除 ball screw / screw nut / driven nut，随后重跑 merge_s7 → S8 → 播放序 → 冻结 v002。
- 配乐（narrate-and-score 路线，仅背景音乐，无旁白）：`配乐/cues.json` 三候选（MiniMax-Music3 @ dell-7920，确定性 seed），measure_cue + 自相关节拍估计后选 A-machine-room-steady（记录 `配乐/decision.v1.json`）；
  新增 `tools/build_score_bed.py`：按 state-chain 章节分 8 段落，每段从乐段起点重启、边界 2 s 等功率交叉淡化 → 全片乐床；衰减/地板/响度/闪避由 skill 的 assemble.py 施加并复测。
- `build_assembly_animation_generic.py` 增量：机位 view 向量的 UP 分量取负（相机高于中心俯视）；v001 终章相机低于床身看到底面。表达层改动，v002 生效。
- `media_qc_generic.py` 增量：framemd5 加 `-map 0:v:0`，带音轨的成片（配乐版）行数才等于视频帧数；faststart 由 assemble 后再 remux 一次 `-movflags +faststart` 保证。
- 2026-09-03 FILM_v002：472 已动画 + 13 未动画 = 485 闭合；回读 PASS；两机分半渲染 6370 帧；配乐版媒体 QC PASS；成片口径穿模 470 CLEAR / 2 PENETRATES（07 带轮、25 Z 轴承挡圈，两审计一致）→ v003 回灌收短；树审计 NO_EDGE 8（7 台架首件 + 1 皮带）、紧固件叶位违规 0。
- 验证器 v002：REJECTED 10 项——A2.1/A3.1/A4.1/A4.2 因验收包只按 485 个动件实体计数而非 1697 个 occurrence（S5 单元成员与未绑定叶未逐一列账）；A5.4 把 S8 孤立节点 142 当无边入场报警（其中柔性 16 / 无几何 48 / 手册未表达 51 / 仅 STEP 紧固件 18 / 台架首件 8）；A6.4 穿模 2；A8.7/A10.5-7 人工复看缺席。修法：验收包增量——occurrence 级处置台账（每个 occurrence 恰一条处置：动画/单元成员/未动画原因），S8 节点按单元展开计 occurrence 覆盖，无边入场按树审计分类（台架根、柔性件）后计报警；穿模走 v003。
- `build_acceptance_packet.py` 增量：occurrence 级处置台账（ANIMATED / ANIMATED_AS_UNIT_MEMBER / NOT_ANIMATED:<原因>，含 EMPTY_DEFINITION 与 S3 未绑定类别），S8 节点按单元展开计 occurrence 覆盖（raw_nodes 保留），无边入场取树审计并分类（BENCH_ROOT / FLEXIBLE_EXEMPT / ALARM），原 S8 孤立节点数保留为 isolated_graph_nodes_raw。
- 2026-09-03 FILM_v003：穿模审计 407 CLEAR / 4 PENETRATES——v001 收短过的 4 件（3 皮带 + 龙门连接块）回潮，根因是 `build_playable_from_seqwalk` 只取最新 run 的审计，导致逐版振荡。
- `build_playable_from_seqwalk.py` 增量：审计回灌取全部历史 run 的并集（同件取最短，保守方向）；S4 柔性类（同步带/管线/拖链）不作刚体滑入动画，why_not=FLEXIBLE_CLASS_NOT_RIGID_ANIMATED（K-ID-04/K-DEL-03）。v004 可播 431 / 未动画 54。
- `build_assembly_animation_generic.py` 增量：材质 = STEP 外观（`evidence/step-appearance-map.v1.json`：Opaque(r,g,b) 转线性、命名外观词表、中性灰按铝合金）× Object Info 状态色；状态语义不变（白=真实外观已就位，暗=未装上下文，暖色乘子+橙色自发光=运动中）；每种 (颜色,金属度,粗糙度) 一份材质，Alpha 仍走 Object Info；世界光略提亮供金属反射。
- 链脚本增量：assemble 后先 `-movflags +faststart` 再媒体 QC（v002/v003 都曾在此停链）。
- 2026-09-03 FILM_v004：验证器 engineering-film 机器门全部 PASS（A0-A7、A9、A11），穿模 0/431、兄弟件 0/344、树审计报警 0；仅剩 A8.7 + A10.5-7 人工复看回执 → 当前决定 REJECTED，成片以评审候选身份交付。回执后：`build_acceptance_packet.py --case-dir . --run FILM_v004 --review-receipt <receipt.json>` → `validate_acceptance.py`。
- `build_assembly_animation_generic.py` 增量：透明外观（Polycarbonate (Clear)/Glass，10 个产品）基础 Alpha 0.28 × Object Info Alpha，Transmission 0.6；appearance-map 增加 alpha 字段。
- 新增 `tools/visual_overlap_scan.py`：画面口径穿模扫描，不豁免基线接触，双向绕数含入占比 ≥2% 即记录，分 SEATED_OVERLAP（t=0 已重叠：CAD 装配态/绑定级）与 PATH_OVERLAP（路径穿模）。
- 2026-09-03 v004 十段并行审片（10 个子智能体逐帧看 v004 成片，每段 9 章，共 ~900 帧 + 局部放大）归纳的系统性缺陷及根因：
  (1) "弹入无位移"——行走上限 `--cap 20`，20 mm 行程在 2–3 m 画面里不可辨；且 Blender 5.2 下 `preferences.edit.keyframe_new_interpolation_type` 对 Python `keyframe_insert` 不生效，位移/颜色/α 全是贝塞尔（前半窗几乎不动、就位色渐变、遮挡者 α 渐变贯穿整章即"X 光在章末才收敛"）。
  (2) "就位即消失/近黑"——STEP 外观 Aluminum-Polished（metallic 1.0, roughness 0.15）在 0.10 的暗世界里映成近黑。
  (3) 机位：全片一个固定视向 + 一章一机位（1.9 m 横梁与 30 mm 衬板同框）+ 上下文半径 2.2×rad 把整机框进来（镜头 7.5 m）。
  (4) 渲染 1600 宽而面板从 1280 起覆盖 → 机器右端被面板盖住；面板只画到 720 → 右下角露出三维画面。
  (5) 遮挡者虚化按整章半径挑选，单章 33–85 件 α=0.12 → 整机"毛玻璃"；玻璃件未按透明外观渲染（v004 前已修：alpha 0.28）。
  (6) HGR25 导轨/滑块的供应商信号绿外观（CAD 原值）被一致误读为调试材质。
  (7) 台架章只显示闭包 → 零件悬浮在空画面里，看不出装到哪。
- `sweep_sequence_reverse_generic.py` 增量：`--fine-until/--coarse-step/--out-dir`（细步 0.25 到 20 mm，之后 1 mm 粗步到 `--cap 150`；另存目录不覆盖 v004 记录）。默认参数下与 v004 行为完全一致。dell-7920 农场：386 作业（>6 实体的工序按单实体分片），launcher.py 补位（xargs -P 96 实测只并发 ~25）。
- `build_playable_from_seqwalk.py` 增量：`--seq-dir/--out`；`freeze_film_state_generic.py` 与 `build_assembly_animation_generic.py` 增量：`--playable` 指向 v2 可播表。
- `build_assembly_animation_generic.py` 增量（v005 表达层，均有开关、默认=v004 行为）：`--fpm-max/--fpm-ref-mm/--fpm-max-mm` 帧数随行程线性加长；位移 LINEAR、颜色/可见/相机 CONSTANT 在存盘前显式设置到 F 曲线（Blender 5.2 分层动作 API）；`--shots-max` 按动件空间分组逐组机位硬切（播放序相邻且合并半径 ≤ max(1.8×最大成员半径, 400 mm)）；逐组 8 方位×2 俯角选视向（画面占比 × 插入轴垂直度 / 遮挡数，连贯加成 8%）；上下文半径 1.3×rad（下限 200 mm）；最小取景半宽 200 mm；`--overlay-top/bottom` 构图只用字幕带之间的可见区；`--ghost-alpha/--ghost-max` 遮挡者按动件足迹判定且只取最重的 N 件；`--mate-ghost-alpha` 动件整件包围盒落在配合件包围盒内时配合件在运动窗内虚化（管内衬板可见）；`--bench-ghost` 非本台架已装件以极淡在场作空间参照（压在动件前方时本组内不显示；语义仍"不在场"，审计口径不变）；`--metal-cap/--rough-floor/--world` 金属反射与环境亮度。
- `compose_assembly_film_generic.py` 增量：面板通高；渲染幅宽≠1280 时缩放而非被面板覆盖；副标题超宽截断加省略号；弯引号替换（字体缺字形）；`--legend-extra`。
- `render_film_generic.py` 增量：`--samples`（EEVEE TAA 采样，DITHERED 半透明去颗粒）。
- `evidence/step-appearance-map.v1.json` 表达层覆盖：8 个 HGR25 导轨/滑块的信号绿外观改钢色，原值保留在 `rgb_cad`，`override` 字段写明理由（K-GOV：声明而非静默）。
- `visual_overlap_scan.py` 修正：记录 `seat_frac`（t=0 含入本底），PATH_OVERLAP 仅当沿路含入比就位本底高出 `--path_margin`（默认 5%），否则归 SEATED_OVERLAP（首轮 71 条 PATH 多为就位本已重叠的 CAD 态 + 接触噪声）。
- `baseline_region.PathChecker` 修正（v005，量具级）：含入采样改为表面采样 + 沿法向向材料内偏移 0.3 mm（自身绕数校验方向；薄件样本不足退回顶点法）。根因：贴合面顶点恰在对方表面上，绕数≈0.5 被计为"内"，t=0 本底虚高（实测衬板 b019 对横梁 b012 本底 25%），沿轴滑进横梁壁 27.8% 低于本底+5% 而放行。修正后本底 0.000、5 mm 处 30.8% → 判穿。dell 行走农场以修正量具重跑（旧量具结果存 `-oldchecker` 目录，不进入任何链）。
- v004 画面口径扫描（修正分类后）：402 对重叠 = 372 SEATED（CAD 装配态本已重叠：磁铁嵌型材、管内衬板、压配角件等，成片不改 CAD）+ 30 PATH（沿路含入超本底 ≥5%：机架衬板/角板 8、磁铁 5、垫块/隔板 7、门锁把手 5、丝杠板 2 等）。
- 修正量具本机复核（WB-01.2，cap 150）：衬板 b019/b006/b033 的最优方向翻转（v004 量具 +X 穿横梁壁 20 mm "CLEAR"；修正后 −X 离开横梁 150 mm FREE），其余 15 件轴向不变、行程 20→150 mm FREE。证明修正在**行走层**改变了认证路径，不只是审计层多报。
- 2026-09-03 工具链正本迁入 `~/.codex/skills/cad-agent/assembly/tools/`（K-GOV-13/K-GOV-16 治理）：通用工具 40 个 + adapters 5 + validator + farm + regress。项目目录（OLSK `engineering_project/assembly_film/tools`）自 FILM_v005 链跑完后改为只读镜像；后续修改先改正本、跑 `regress/regress.py --compile`、再同步到项目。
- 2026-09-04 K-GOV-16 补充：农场并发必须受内存约束。dell-7920 上 90 个审计进程（每个加载全部 1034 个 STL）把 503 GB 内存吃满，负载 130+，sshd 停止响应。`farm/farm_launcher.py` 加 `--mem-reserve-frac`（MemAvailable 低于 15% 不再起新作业）；审计类作业每进程 RSS 先量后定并发。
- 2026-09-04 审计农场备用节点 jx-001（aarch64 DGX Spark）：`python3 -m venv` + `pip install trimesh python-fcl numpy scipy rtree` 直接可用（fcl 有 aarch64 轮子）；内存受限（推理服务占 90 GB）故并发 6 并带内存守卫；作业表按工序逆序与本机正序相向推进，结果每 5 分钟 rsync 回本机。
- 2026-09-04 v005→v007 表达层与门禁工具：
  - `measure_readability.py`（K-SCN-10 测量）：Cryptomatte matte 数每个动件中程/就位（f1-1）帧可见像素、投影凸包面积、f0→f1 屏幕位移，换算成片分辨率给 READABLE / UNREADABLE:SIZE|SHIFT|VISIBLE；Blender 5.2 后台的 File Output 节点只写多层 EXR，用 `exr_reader_min.py` 解无压缩 EXR。
  - `build_assembly_animation_generic.py`：`--vis-select N` 几何评分前 N 个候选视向各渲一张低清 Cryptomatte，按实测可见占比定机位（独立测量相机，主相机动画不干扰）；分组尺度相容（小件不并入大件机位）；先装兄弟件与大配合件（≥5×动件半径）计入遮挡评分；包壳虚化按包围盒体积 60% 落入即算；`--shots-max 8`。OLSK 实测：可读 239→382/423。
  - `carry_forward_audits.py`：章节输入哈希（动件序列/轴/向/行程/成员 + 上下文 + 含静态件的前驱闭包）相同则承接上一版审计记录（写 carried_from），v006→v007 只重审 1 个工序。
  - `visual_overlap_scan.py` 记 `clean_prefix_mm`；`build_playable_from_seqwalk.py --min-visible 5` 消费画面口径回灌（K-PATH-18）。
  - `aggregate_review_findings.py` + `film_review_segments.py`：K-DEL-04 审片分段与汇总 → `review-findings.v1.json`（sev3 计数进验收包 frame_review）。
  - `validator/acceptance-contract.v1.1.json`：新增 A12 表达层门禁（画面口径 PATH=0、可读性已测、审片 segments>0 且 sev3=0；缺失即 UNKNOWN→HOLD）。
  - 结果：FILM_v007 穿模审计 0 / 兄弟件 0 / 画面口径 PATH 0（v004 为 30），420 动件、65 静态（其中 8 件按 K-PATH-18 低于 5 mm 改静态）。
- 2026-09-04 OLSK FILM_v007 审片结果：三级 21（全为机位/虚化/语义类，无穿模三级）；v008 表达层修复：极淡台架件只在动件 3×半径内显示、遮挡者 α 0.25 且贯穿就位帧、多探针实测可见性选机位、每章 ≤10 机位、铝板反照率封顶 0.78、自发光增益 2.5→1.2、透明件运动中不发光、采样 96。两条语义待办（准备件台架章空场、跨台架配合的前驱缺失）记入 open-items 候选。
- 2026-09-05 OLSK v008 审片：三级 28（无穿模三级）；v009 增量：候选俯角加 0.08、剖切回退（最佳视向探针可见 <0.35 时把压在动件前方的兄弟件/配合件/上下文按遮挡者透视）、配合件遮挡阈值 5×→2×、相机落入任何在场件包围盒（+60 mm）即沿视线后退 20%（≤4 次）。审片反复指出的"台架空场/跨台架悬空"属 O-25。
- 2026-09-05 舰队渲染三机分帧：Mac（~1 s/帧）+ dell-7920 `/data`（~1.5 s/帧）+ dell-nb Windows Blender 5.2 portable（~2 s/帧，PowerShell + scp）。DGX Spark 仅有发行版 Blender 4.0.2（aarch64），不能开 5.x 场景，只作审计节点。
- 2026-09-05 DGX 升 Blender 5.2 可行性：无 arm64 二进制；源码编译卡在 GCC 14（已装，清华镜像）/Python 3.13/OIIO 3/OCIO 2.4（noble 无包）+ 节点无外网；GB10 无头 EEVEE 已验证可用。记入剧本五、并列为非关键路径事项。
- 2026-09-05 farm 新增 `three_node_render_template.sh`（三机分帧 + 尾段再平衡 + 逐帧核数后才合成）、`windows_node_render_template.sh`（Windows 节点：Win32_Process 脱会话启动、scp 通配回传、done 标记）、`master_local_template.sh`（本机审计版主控）；`remote_render_template.sh` 远端根改用数据盘 /data；`compose_assembly_film_generic.py` 同步 `--encode-only` 期望时长按已合成帧数计算的修正。
- 2026-09-05 功能/安装语义（用户："看似对的、实际顺序不对"）：新增 `function_tracks/`（SCHEMA.md、AGENT_PROMPT_TEMPLATE.md、build_function_manual.py+build_report.py：八条并行分析线 → 组件功能属性手册 PDF）、`build_function_track_inputs.py`（按线切片 Workbook/S3/S8/BOM/S4）、`audit_sequence_constraints.py`（顺序约束对照 state-chain 章序与冻结播放序，含工序内约束按零件名匹配）、`build_play_order_functional.py`（工序内播放序 v2：种子→接触支撑贪心→紧固件在被夹件后→耦合对末尾）、`walk_unit_joins.py`+`materialize_unit_joins.py`（台架组整体上机六轴认证并落成 S3 v2/S7 v3/可播表/播放序）、`reclassify_s4_flexible.py`（S4 v2：柔性词表误判 67 件）。冻结器新增 `--play-order/--s4/--s7`；可播表 `--s4`；验收包与穿模审计按冻结态 tracked_inputs 读同版本。
- 2026-09-05 表达层 v010（十段审片 264/48 三级的系统性原因）：构建器新增 `--seat-hold`（就位停顿）、`--min-mover-frac`（动件最小画面占比，推近并以动件为中心）、`--axis-view-max`（视线不平行插入轴）、`--cutaway-hide`（剖切=隐藏）、`--finale-orbit-deg`（终章环绕）；遮挡/包壳虚化恢复键移到章末（就位帧不再被挡）；台架章 `--bench-ghost 0` 隐藏非本组件；构图只框可见件；合成器 HUD 拉丁词不拦腰折行；诊断脚本 `diag_ghost_state.py`/`diag_vanish.py`/`render_frames_list.py`。农场：`chain_template_v010.sh`、`v010_prep_template.sh`。
- 2026-09-06 功能属性的下游交付（不依赖动画）：`function_tracks/build_sop_regulation.py`（114 步工艺卡：功能序、零件/紧固件清单、扳手/内六角尺寸、参考扭矩（ISO 898-1 8.8 参考，铝型材/树脂折减，标来源）、装后调整/检验点、前置步、官方备注、疑点 → PDF+xlsx+json）；`function_tracks/build_complete_bom.py`（手册/BOM/STEP 三源对照：标准件/外购件/改制件/非标件分类、差异表、紧固件规格汇总、外购件家族、非标件加工方式）；上游问题清单（open_questions 去重归并成 GitHub issue 草稿，中英）。OLSK 结果：BOM 缺 120 行/978 件、`ISO 3780`→ISO 7380-1、9 个支承轴承手册未列、换刀由不在 BOM/CAD 的 RP2040 板执行、固件引脚冲突。
- 2026-09-06 紧固件层（O-26）：`extract_glb_fasteners.py` 从手册 GLB 抽出 2487 颗螺钉网格；但手册 GLB 与 STEP 之间无一致刚体变换（唯一名尺寸一致对拟合只有 41% 在 10 mm 内，逐步平移也不收敛：现有绑定对按包围盒相似度配，几何错配），放置需做形状配准或按 STEP 螺栓孔生成，列为 v011。`fallback_walk_chunk.py`：新序行走超时（>2.5 h）时用旧序同实体行回退并标注。
- 2026-09-06 v010 交付（10 分 09 秒，97 章，18264 帧，本机独渲）：穿模 0、兄弟件 0、画面口径 6（小件顺宿主滑到位）、顺序审计违反 27（v009 48）、审片三级 29（v009 48）；验证器 REJECTED（A8.7/A10.7/A12）。审片根因与 v011 清单见 lessons-inbox/20260905-olsk-functional-order-and-unit-joins.md 补记。

