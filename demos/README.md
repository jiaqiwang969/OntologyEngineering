# 可执行佐证 demos（书 ↔ 代码相互佐证）

本目录把两卷书中的关键论断变成可执行验证。每个 demo 的输出结构固定为：

```
【书中论断】…（原话或忠实转述）
【锚点】…（书内文件/章节坐标）
（执行过程）
【佐证结论】成立 / 不成立
```

demo 以退出码表达佐证结果（0=成立，非 0=不成立），可当回归测试跑；
`.github/workflows/corroboration.yml` 在每次 push 时自动全量回归。

结构约定：数据与断言分离——场景图放在 `fixtures/*.ttl`（可单独审阅、复用），
demo 代码只含门禁形状、查询与断言；`_common.py` 是公共入口（静默 Semantica
进度输出、提供 `load_fixture()`），每个 demo 的第一个 import 都是它。

## 准备运行时（一次性）

```bash
bash runtime/setup_runtime.sh
runtime/.venv/bin/python demos/<demo>.py
```

## 第一卷《工程本体论》

| demo | 佐证的书中论断 | 引擎 |
|---|---|---|
| `vol1_ch02_reasoning_modes.py` | ch02：单调推论不被新事实撤销（故障事实推不翻 Available）；同一缺失在 OWA 是未知、在 CWA 是假 | Semantica `Reasoner`、rdflib |
| `vol1_ch03_cq_acceptance.py` | ch03/ch04：「CQ 即验收测试」——书中 CQ1 查询原文直接执行，合格本体通过、缺能力本体被挡 | rdflib SPARQL |
| `ch04_shacl_open_vs_closed.py` | ch04/ch07：缺值在 OWL 开放世界下只是"未知"，在 SHACL 下是违规报告；自动派生形状与手写形状拦截同一违例 | pyshacl、Semantica `to_shacl` |
| `ch05_forward_chaining.py` | ch05：swrl-rules.swrl 作者手推的 Lathe_003 两步链，机器复算一致且可解释 | Semantica `Reasoner` + `ExplanationGenerator` |

## 第二卷《产品可信工程》

| demo | 佐证的书中论断 | 引擎 |
|---|---|---|
| `vol2_iso_normative_query.py` | 规范可本体化为带模态的可查询个体（240 个）；刻录纪律可写成 SHACL 被机器强制 | rdflib SPARQL、pyshacl |
| `vol2_hara_asil_corroborate.py` | ch04/ch14：EPS HARA 工作表 S×E×C→ASIL 复算与作者手填一致；查表门禁拒绝 S2E4C3 误标 B | Semantica `Reasoner`、pyshacl |
| `vol2_claim_gate_corroborate.py` | ch03/ch11 主张本体：待完成论证结构放行；六绿≠接受被拒；缺件即拒并列明缺件 | pyshacl |
| `vol2_ch12_identity_bridge.py` | ch12 身份：族检查接住"把型号当单件"；桥接查询把"这就是同一台"兑现成证据链 | pyshacl、rdflib SPARQL |
| `vol2_ch13_governance_distance.py` | ch13 治理：太近查询两行、清晨亮灯两行与书一致；授权生效必须本人，代签被弹回 | rdflib SPARQL、pyshacl |
| `vol2_ch15_reopen_list.py` | ch15 需求追溯：理由锚在参数上，重开清单一次查询取出，无关项不入列 | rdflib SPARQL |
| `vol2_metrics_recompute.py` | ch06/ch16 度量：400→403 FIT 七项数值全程复算与书打印值相符；got≥need 门禁只标 SPFM 未达标 | 算式复算、pyshacl |
| `vol2_ch17_change_verdict_gate.py` | ch17 变化：旧值牵出三张"通过"；空波及面的作废与无理由的保留被拒，写明理由放行 | rdflib SPARQL、pyshacl |
| `vol2_ch18_independence_meet.py` | ch18 依赖独立：feedsFrom+ 闭包上溯在第二级找到 Reg5V_U3（只追一步查不到）；押过期事实的排除自动标疑 | rdflib SPARQL（属性路径） |
| `vol2_ch19_substitution_gate.py` | ch19 现场：缺设计评估的代换被拒（对照表不是评估）；推断不得写成事实 | pyshacl |
| `vol2_release_binding_gate.py` | ch20 发布保证：违例查询恰好一行（r12 绑错快照）；装包门禁拒收并出处置开单，修订回包放行 | rdflib SPARQL、pyshacl |

覆盖：第一卷 ch02/03/04/05/07；第二卷 ch03/04/06 与后十章的 ch11–ch20 全部十个本体
（ch14 情境危害并入 HARA demo，ch16 测量并入度量 demo）。

## 书 ↔ 代码 诚实映射表

书讲的是完整理论，Semantica（0.6.5）只实现其中一部分。回答问题时不要把
两者混为一谈；以下映射为准：

| 书中概念 | 可执行对应 | 状态 |
|---|---|---|
| OWL/RDF 建模（ch04） | Semantica `OWLGenerator.generate_owl` | ✅ 可用 |
| SHACL 校验（ch04/ch07） | pyshacl（Semantica validator 同引擎）；`OntologyEngine.to_shacl` 自动派生 | ✅ 可用（0.6.5 有 base_uri 追加 "/" 的 quirk，见 ch04 demo 注释） |
| SPARQL 查询（ch04/ch07） | rdflib（**不要用** Semantica `SPARQLReasoner.execute_query`，0.6.5 是占位实现，返回空） | ✅ 用 rdflib |
| 规则推理/前向链（ch05） | Semantica `Reasoner.forward_chain` + `ExplanationGenerator` | ✅ 可用（不支持 SWRL 内建算术，数值判定需前置；单调，无结论撤销——ch02 demo 正以此佐证书中论断） |
| 描述逻辑推理器（ch05 DL/tableau） | 无对应（Semantica 无 DL 推理器） | ❌ 书讲原理，代码不覆盖 |
| SWRL / 时序 / 概率 / 默认逻辑推理（ch02/ch05） | 无直接对应 | ❌ 同上 |
| OntoClean、方法论评估（ch03） | 无对应；CQ→SPARQL 验收部分可执行（vol1_ch03 demo） | ⚠️ 部分 |
| 知识图谱构建（ch07） | Semantica `GraphBuilder` / `KGVisualizer` | ✅ 可用 |
| 溯源（第二卷证据本体一脉） | Semantica `ProvenanceManager`（W3C PROV 导出） | ✅ 可用 |
| 决策审计（第二卷保证/治理本体的运行时对应） | Semantica `ContextGraph`（决策+因果链+先例检索） | ✅ 可用，书中无同名概念，属工具侧扩展 |

依赖锁定在 `runtime/setup_runtime.sh`（semantica==0.6.5）。升级前先跑全部
demo 确认佐证仍成立。

## 教学素材边界

- 第二卷全部人物、事故、EPS 数据均为合成教学材料；demo 按书中示意记法
  构造最小图，不是真实产品数据。
- 完整 36 格 ASIL Table 4 TTL 与精确 ISO 条款坐标在另行受控的工程正本
  来源账中，不随本 skill 分发；HARA demo 使用书内公开的代表性映射单元。
