# 可执行佐证 demos（书 ↔ 代码相互佐证）

本目录把两卷书中的关键论断变成可执行验证。每个 demo 的输出结构固定为：

```
【书中论断】…（原话或忠实转述）
【锚点】…（书内文件/章节坐标）
（执行过程）
【佐证结论】成立 / 不成立
```

demo 以退出码表达佐证结果（0=成立，非 0=不成立），可当回归测试跑。

## 准备运行时（一次性）

```bash
bash runtime/setup_runtime.sh
```

## demo 清单

| demo | 佐证的书中论断 | 用到的引擎 |
|---|---|---|
| `ch04_shacl_open_vs_closed.py` | ch04/ch07：缺值在 OWL 开放世界下只是"未知"，在 SHACL 下是违规报告；且从概念模型自动派生的形状与手写形状拦截同一违例 | rdflib SPARQL、pyshacl、Semantica `OntologyEngine.to_shacl` |
| `ch05_forward_chaining.py` | ch05：swrl-rules.swrl 中作者手推的 Lathe_003 两步推理链，机器复算结论一致且可解释 | Semantica `Reasoner`（前向链）+ `ExplanationGenerator` |
| `vol2_iso_normative_query.py` | 第二卷：工程规范可本体化为带模态的可查询个体；刻录纪律可写成 SHACL 被机器强制 | rdflib SPARQL、pyshacl |
| `vol2_hara_asil_corroborate.py` | 第二卷 ch04/ch14：EPS HARA 工作表的 S×E×C→ASIL 判定链机器复算与作者手填一致；查表一致性门禁拒绝书中反例（S2E4C3 误标 B） | Semantica `Reasoner`、pyshacl（SPARQL constraint） |
| `vol2_claim_gate_corroborate.py` | 第二卷 ch03/ch11 主张本体：EPS 待完成论证结构放行；六绿僵局（证据未接受却标 ClaimAccepted）被拒；缺 context/validWindow 的主张被拦并列明缺件 | pyshacl（SPARQL constraint + minCount） |

运行单个 demo：

```bash
runtime/.venv/bin/python demos/ch04_shacl_open_vs_closed.py
```

## 书 ↔ 代码 诚实映射表

书讲的是完整理论，Semantica（0.6.5）只实现其中一部分。回答问题时不要把
两者混为一谈；以下映射为准：

| 书中概念 | 可执行对应 | 状态 |
|---|---|---|
| OWL/RDF 建模（ch04） | Semantica `OWLGenerator.generate_owl` | ✅ 可用 |
| SHACL 校验（ch04/ch07） | pyshacl（Semantica validator 同引擎）；`OntologyEngine.to_shacl` 自动派生 | ✅ 可用 |
| SPARQL 查询（ch04/ch07） | rdflib（**不要用** Semantica `SPARQLReasoner.execute_query`，0.6.5 是占位实现，返回空） | ✅ 用 rdflib |
| 规则推理/前向链（ch05） | Semantica `Reasoner.forward_chain` + `ExplanationGenerator` | ✅ 可用（不支持 SWRL 内建算术如 swrlb:greaterThan，数值判定需前置） |
| 描述逻辑推理器（ch05 DL/tableau） | 无对应（Semantica 无 DL 推理器） | ❌ 书讲原理，代码不覆盖 |
| SWRL / 时序 / 概率推理（ch05） | 无直接对应 | ❌ 同上 |
| OntoClean、方法论评估（ch03） | 无对应 | ❌ 同上 |
| 知识图谱构建（ch07） | Semantica `GraphBuilder` / `KGVisualizer` | ✅ 可用 |
| 溯源（第二卷证据本体一脉） | Semantica `ProvenanceManager`（W3C PROV 导出） | ✅ 可用 |
| 决策审计（第二卷保证/治理本体的运行时对应） | Semantica `ContextGraph`（决策+因果链+先例检索） | ✅ 可用，书中无同名概念，属工具侧扩展 |

依赖锁定在 `runtime/setup_runtime.sh`（semantica==0.6.5）。升级前先跑全部
demo 确认佐证仍成立。

## 已知实测参考

Semantica 在真实课程数据上的完整评估（三阶段、32 门课）见
`~/148-Semantica/fusion-i01-redo/`（工作区外部，不随 skill 分发）。
