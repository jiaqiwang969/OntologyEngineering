# 数值要求、孔列与图纸交付

用户用自然语言讨论；agent 在同一 Project 的 EngineeringBasis 中维护以下合同。`specs/engineering.toml` 是共享类型与约束正本，经 Rust 规范化 RDF 和锁定真实 ggen 生成。`generated/` 不手改；算法在 `oe-kernel`、`oe-cad` 和原生 worker。内部文件不是新的项目状态系统。

## 数值验收

| 注释/字段 | 填写要求 | Rust 与实际检查 |
|---|---|---|
| `@required requirement_id / requirement_property` | 稳定要求身份；facts 中保存完整当前要求，文字摘要不能代替 | `NumericAcceptance`；当前要求快照逐字段一致 |
| `@required object / state / datum` | 对象、版本、条件、原来源摘要；取得结果的过程状态与空间/测量基准 | 与 Project、当前 facts、计算/观测身份分别核对 |
| `@format Quantity upper / lower / target / tolerance` | maximum、minimum、range 或 target_tolerance；单位一致，容差非负；禁止隐式换算 | 有限数值、界限次序及量纲检查；精度说明不是不确定度 |
| `@required metric` | scalar、absolute、radial_2d；二维分量有独立名称且同单位 | 标量、绝对值或欧氏合成；两个局部轴都符合不等于合成符合 |
| `@required definition_status / coverage_status` | confirmed/draft；declared_complete/partial/unknown，原依据未确认不得填 confirmed | 草案或部分覆盖即使条件比较符合，正式数值结论仍 unknown |
| `@required evidence_role` | calculated_model、measurement、simulation、native_geometry、analytic_proxy | 要求与实际输出角色必须一致；生成 DTO 拒绝未支持用词 |
| `@required calculation XOR observation` | 使用已有模型重新计算，或绑定原观测；两者恰有一种 | 缺结果与选择歧义有独立诊断；旧算术在条件过期时仍可作诊断 |
| `@required expanded_uncertainty / uncertainty_basis` | 观测的同单位、非负不确定度及取得依据；缺失不填零 | 整个区间在界内 pass、整个区间在界外 violated、跨界 unknown；不认证物理校准 |
| `@required condition_values / source_versions` | 输出取得时全部事实属性/值及所有来源版本 | 核对当前内容；同名 scope、重新写哈希或旧计算可运行都不能恢复支持 |

生产检查 `oe-kernel::numeric::check` 返回模型适用、算术/观测、数值符合、工程采用及受影响决定。一个声明完整的模型数值 pass 只支持其限定模型；现场条件、校准、制造能力与最终采用仍须对应证据。单位错误、旧状态/基准、旧来源、未支持模型、缺输入、结果超限都保留具体原因。

例如独立算术 `20×100³/(3×200000×1000)=1/30 mm` 正确，但超出 `0.02 mm`；`10 N` 时为 `1/60 mm`。这是人工模型反例，不是设备性能。两个轴各 `0.025 mm` 的欧氏合成为约 `0.035355 mm`；最终采用哪种口径由受控验收要求确定。`0.028±0.004 mm` 对 `0.03 mm` 上限不能明确判定。

将 `kind=numeric_acceptance` 的 assertion 放入当前 Project，沿 `project work`、`kernel review --project`、Harness 和经验评估复用同一检查。不另造脚本判定最终合格。原包查询/形状、焦点、当前条件检查照常执行；新增数值检查不能掩盖原失败。

显式 `oev2.numeric-assessment/2` 与 `oev2.numeric-acceptance/2` 用独立生成的 `Numeric*V2` 类型表达逐对象的严格上限 `strict_maximum`，并在要求、观测中分别绑定参考端点 `reference_object`。原 `/1` DTO、wire 与 maximum 包含上限的行为保持。两个端点在同一 Project 中核对象版本、条件和来源；实际参考对象的实现与标定仍需独立计量证据。

`/2` 只支持距离口径 absolute 或 radial_2d；整个区间的上端 `< upper` 才有条件数值支持，下端 `>= upper` 为不符合，其余 unknown，不加 epsilon 放行相等值。每个十字用自己的对象与观测检查，不能平均多个十字抵消个别超差。草案、覆盖不足、缺不确定度继续 unknown；计算模型的支持也不变成实物合格。实际项目的用户确认与最新项目版本是私有来源，合成边界反例不代表其机器能力。

## 参数与孔列意图

`CenteredHoleRow` 写稳定 row_id、当前 object、local_frame、mm 单位、精确十进制的 length/pitch/minimum_center_to_end/center/diameter 及 source_refs。当前事实属性是 `cad_row:<row_id>`，保存同一完整类型并绑定原来源。缺 typed fact 保留数学计划，当前使用 unknown；字段冲突明确指出 length、frame、pitch 等，拒绝当前采用。十进制等值写法不制造冲突。

居中周期列按长度、孔距及最低孔心端距取可容纳孔数，再对称布置；偶数孔保持半孔距相位。余量不是自造的统一端距。模型每个配合成员写自己的边界、孔径、坐标与来源；指定 `count_controller` 时，成员服从控制件孔数并检查是否容纳。较长配合板不能自行多生一个孔。定位孔、沉孔、盲孔、螺纹、种子与非周期特征不由这条有限规则推断。

`cad variant plan-row/plan-mating` 通过同一 Project/context 核对当前意图，每个成员得到自己的 context，原调用 context 单列保留。计划包含孔中心和 mating、dimension_association、view_update、layout、delivery 的依赖起点。改变参数后沿 Project 重开相关运动、CAD、图纸/BOM、制造、检验、成本和交期；未声明依赖仍未知。

`cad variant check-native` 接受原生圆柱面回读、明确的设计意图选择及正确刚体装配变换，核对孔心轴线、直径、数量和一一对应。圆柱面不自动都是孔；检查调用方必须限定周期列/配合组。增大、缩小、孔数跳变、偶数孔、恢复原值和重复执行分别验证。实例的孔距/端距只能通过其来源填写，没有通用默认尺寸。

## 原生与图纸范围

当前 Rust/NX 路径包括 `create_hole_plate`（有界新样件生成）、`inspect_detailed`（回读几何、图纸、视图及尺寸）和 `maintain_drawings`（受控复制、设 work part、刷新视图、保存与回读）。`cad nx prepare/stage/run/status/fetch` 绑定输入字节、worker/controller、SDK 运行库、目录和进程身份。pending/unknown 不自动再次 dispatch；同一个远程根的写操作顺序执行，失败原件保留。

新样件生成不是任意已有特征树改型。导入旧件的盲孔/倒角/螺纹维护、尺寸父对象重关联、自动排版和新设计完整制图仍需独立实现与验收。不能用 fresh regeneration 的成功覆盖这些缺口。

保存后在新进程独立重开：核对实际几何、视图 stale 状态和尺寸 query 返回，留输入/输出及工具身份。尺寸 `retained=false` 只说明本次原生状态，不能证明它关联了设计要求的正确孔、边或装配父对象。文字框位置、角度（rad）、宽高只作图面诊断；未绑定 sheet/view frame 时不判断裁切或重叠。

`cad drawing audit-packet` 校验实际文件清单、摘要、PDF、模型/图号及审核表关联。隐藏杂项、缺文件、摘要漂移和同版对应错误均拒绝。严格清单通过不代替原生重开、独立视觉阅读、尺寸关联、BOM 完整性或制造放行。视觉裁切、空图框、缺注释、无工艺/公差/检验、版本错配分别安排必要验收。

## 因果经验与下一任务

恢复原目的、条件、动作、结果、用户纠正、原因/假设、修正和验证，再提炼方法。只改外形造成孔列或图纸遗漏，是有源的软件/覆盖缺陷；它支持“参数变化需核对功能和下游依赖”的有条件方法，不支持通用物理定律。历史成功、原中间失败和当前新执行各有身份。

下一任务将方法保存为 Project fact/支持关系并实际调用检查，记录因此重开的决定和需要补的证据。已有对象、条件、主张和依赖表达足够时记实例或 no_delta。工作方法的收益、行业知识候选、工具修复和工程效果分别取证；当前候选只有有限操作计划，不宣称完整自进化。RRSI 冻结和恢复按 [合同](regularized-evolution.md) 执行。
