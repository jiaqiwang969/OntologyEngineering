# ch10 来源审计：Part 8 横切过程与工具/复用边界

## 审计结论

当前 ch10 已按同一 `CR-0412` 放行冲突教材化吸收 ISO 26262-8:2018 的选定 Clause 5–14 内容：
DIA、需求规格、配置/变更、验证、文档、工具置信、软件组件资格、硬件元素评价和 PiU。Part 10
§9、§10、§13 仅作 SEooC、PiU 和工具变化的资料性解释。

“教材化吸收”只说明正文已经准确消费并标注这些来源，不表示相应项目活动已执行，也不表示所有
对象都已进入 RDF。当前本体的实质可执行范围仍主要是 Clause 11 工具子域：Table 3 六格、两个
TeachingExample usage 评价、Table 4/5 的 2 表/8 方法/32 推荐单元及其局部 Shape/CQ/fixture。

选定来源经过本地 PDF、MinerU JSON/Markdown 和 `pdftotext` 交叉回读。独立来源冷审发现的正文
P1 已逐项修正；最终冻结结论仍不替代专家审查、文本哈希、权利处置或出版批准。

## 证据路径

- 原始 PDF：`ISO 26262-2018/ISO 26262-8-2018.pdf`、`ISO 26262-2018/ISO 26262-10-2018.pdf`
- Part 8 MinerU 报告：`structured/mineru/ISO-26262-2018/part-08-supporting-processes/evidence_report.md`
- Part 8 结构化源：`structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json`
- Part 10 结构化源：`structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json`
- 逐源回读矩阵：`notes/ch10-iso-reread-matrix.md`
- 机器来源锚：`ontology/source-anchors-part8.ttl`、`ontology/source-anchors-part10.ttl`
- 覆盖账：`coverage/source-units.csv`

PDF 带有 `Not for Resale` 标识。本仓库只保留短标题、坐标、语义重构和必要教学计算，不复制原表
图像或长段原文。

## 关键坐标

| 主题 | 关键来源 | PDF 页 / block | 当前用途 |
|---|---|---|---|
| DIA 适用与例外 | §5.4.1.1 | p15/b6 | 硬件例外保留质量程序资格与 Clause 13 评价的并列条件 |
| DIA 内容 | §5.4.3.1 | p16/b8 起 | 十一项组织为决定/执行、边界交换、评价/检查权 |
| 供应商 FSA | §5.4.5.1–.4 | p18/b1 起 | C/D 要求、B 推荐；报告评价元素需求符合与过程准则；Note 不升格 |
| 需求十特性 | §6.4.2.4 | p21/b12 起 | 单条需求质量与时间/测量反例 |
| 需求集合与追溯 | §6.4.3.1–.3 | p23/b5、b19；p24/b4 | 集合质量、三向追溯、Table 2 |
| 表达/验证方法 | Table 1 / Table 2 | p21/b5；p24/b5 | 两次选择独立给理由，无自动配对规则 |
| 配置管理 | §7.4.1–.5 | p25/b9–19 | 基线、复现与生命周期策略 |
| 变更主链 | §8.4.1.2–.4 | p26/b10 起；p27/b0 | 计划对象/时点→请求→分析→决定→实施/验证→文档 |
| 影响与发布前更新 | §8.4.3.1、§8.4.5.2 | p27/b16；p28/b8 | 不按改动行数定范围，不把触发窄化为整车 |
| 验证计划/用例/报告 | §9.4.1.1、§9.4.2.2、§9.4.3.3 | p30/b2、b26；p31/b19 | 对象绑定和首次 FAIL→修复→重验时间语法 |
| 验证独立性 | §9.4.2.4、§9.4.3.2 | p31/b15、b18 | 参照被验证工作产物作者 |
| 文档载体与身份 | §10.4.1–.6 | p33/b8 起；p34/b3 | 策划、工作产物结果载体、作者/批准/修订/历史/状态与当前版本 |
| Clause 11 入口与信息 | §11.4.1、§11.4.4.2 | p37/b2；p38/b12 起 | 适用性关系及正确评估/使用前须可得的六类信息 |
| TI/TD/TCL | §11.4.5.1–.4、Table 3 | p38/b16；p39/b0、b15–17 | usage、should 保守估计、shall 查表 |
| 资格方法 | §11.4.6.1、Table 4/5 | p40/b1–3 | TCL 路由和 §4.3 组合理由 |
| 软件组件资格/变化 | §12.4.2.1–.4、§12.4.3 | p43–45 | 规格/手册、Part 6 覆盖、实现改变旧验证失效、用途改变复核有效性 |
| 硬件元素评价 | §13.1、§13.4.1–.4 | p45/b15；p47–50 | 对象范围、共同需求/失效分析底座与 Class I/II/III 层次 |
| PiU 目标 | §14.4.5.1–.2.4、Table 6 | p53/b16；p54/b2–7 | 配置连续性、计算理由、specimen 求和、逐安全目标和 70% 置信 |
| 零事件示例 | Table 7 | p54/b9 | Note 3 信息性示例；已从 normative_table 纠正 |
| 临时 credit 与事件 | §14.4.5.2.5/.6、§14.4.5.3、Table 8 | p55/b5–7、b13 | Table 8 前提、特定根因小时重置和全运行期事件可检索 |
| SEooC | Part 10 §9、§9.2.4.5 | p57/b0；p65/b7 | 资料性假设确认与三类差异处置 |
| PiU / 工具说明 | Part 10 §10、§13 | p65/b14；p82/b0 | 资料性理解，不生成额外 SHALL |

## 已修正的来源风险

1. Clause 5 例外不再写成“Clause 13 评价即可豁免 DIA”；现成硬件保留另一并列前提。
2. DIA 的 RASIC Note 不再升格；“恰好一个 A”明确是本书局部闭世界策略。
3. ASIL(B) 的供应商 FSA 保持 recommendation，C/D 保持 requirement。
4. 派生需求的 ASIL 继承恢复 shall 强度；Table 1/2 不再伪造一一配对。
5. Clause 9 独立性不再笼统写“非作者”，而是相对于被验证工作产物作者。
6. Clause 8 直接链止于实施、验证和文档；新基线是项目配置策略，不是假造的逐变更 SHALL。
7. §8.4.5.2 不再被误窄为整车安全功能或性质。
8. use case/输出检查变化先重做 Clause 11 入口筛选，仍适用时才更新 TI/TD。
9. Clause 12 不再承诺实现改变后“只重建受影响资格”；旧验证失效后先重判证据路线。
10. Clause 13 的共同需求/失效分析底座、Class I 条件、对象范围、Class II 综合论证和 Class III
    附加论据已恢复精确层次。
11. SEooC 不再写成“整个 ISO 原封不动套到 element”；递延活动和真实上下文假设仍需完成。
12. PiU 统一使用“可观察事件”，Table 7 保持 Note 示例，Table 8 前提和特定根因小时重置已写全。
13. 供应商 FSA 报告的两条评价轴、现场问题回到既定监控过程、文档管理策划、工具六类可得信息
    和 PiU 未来用途 ASIL 均已从“提到主题”补成带主语与模态边界的判断。

## 规范事实与本书投影

| 主张 | 归因与边界 |
|---|---|
| DIA、需求、配置/变更、验证、文档、工具和复用条款 | Part 8 规范性来源；适用性和具体模态词逐段保留 |
| Part 10 §9/§10/§13 | 资料性指南；所有 RDF 锚显式标 `InformativeStatement` / `ISOInformativeGuidance` |
| `SUP-S10`、六坐标主张卡、恰好一个 A | 本书教学/治理构造，不是 ISO 原生工作产物或 RDF 项目实体 |
| Satisfied/Violation/Unknown/Not modelled | 本书机器界面策略，不是 ISO 状态枚举 |
| 40/10/60 ms、0.80 Nm、1.25×10⁹ h | 合成教学数据；不指代现实 EPS 参数、运行历史或 PiU 结论 |
| Table 3 与 Table 4/5 可执行转录 | 已编码的受控规范结构；不证明工具清单完整、理由真实或资格活动完成 |

## 来源账同步

- 正文实际消费的 136 个选定 Part 8 单元已登记 `chapter_ids=ch10`：47 个为 `anchor_only`、
  86 个为 `prose_only`、Table 3/4/5 三个表单元为 `modeled`。这些处置描述机器表达深度，
  不是 136 条项目符合性结论。
- 9 个 Part 10 资料性单元已登记 ch10：6 个为 `anchor_only`、3 个为 `prose_only`，且不改变
  其 informative 状态。
- Part 10 §9.2.4.1–.4 的新开发软件组件示例链和 §10.2–.4 的 ASIL C ECU 硬件 PiU 示例并未
  被本章逐单元消费，七行已退回 `not_covered`，没有用父级主题相近制造覆盖。
- 37 个本轮新增的 Part 8/10 代表性锚已在覆盖账登记；它们只对象化坐标，不对象化项目执行。
- `SU-8-TABLE-7` 已由 `normative_table` 改为 `informative_table`。
- 未被正文消费的大量单元继续保持 `not_covered`；rights 继续 `review_required`。

## 机器证据与未关闭项

当前可执行：Table 3 六格、两个教学 usage 评价、Table 4/5 的 2/8/32、7 个 ch10 CQ/GATE 和聚焦
Shape/fixture。当前不可执行：§11.4.1 工具清单/筛选、§11.4.2–.4 策划与有效性、资格计划/执行/
报告、DIA、Clause 9 全链、基线/CR、Clause 12–14 以及五张材料共同快照。

所有 ch10 CQ 的专家状态仍为 `pending`。`Table_9_C_1` 已补齐 Part 9 唯一所有权并通过定向回归；
230 项单元测试已在分段扫描与两项定向复跑中全部通过，其中 pySHACL 嵌套查询耗时 106.8 s；
完整 `run_eval.py` 仍没有同一快照的通过或发布报告。
来源权利处置、专家技术复核、原文文本哈希、图稿来源/权利、最终 PDF 和用户验收均未关闭。
