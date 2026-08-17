# 第 4 章：先决定担心什么

副标题：HARA 纵向样板。

这一章不从风险表或措施清单开场，而从一个工程团队很自然的动作开场：问题尚未说清，
双传感器、关断和告警已经画在白板上。正文用同一辆车、同一 EPS 非预期助力和两个不同
使用情境，迫使读者先分开分析对象、情境、场景、担心、后果、目标与假设，再在适用的
安全 concern 下走完一条 ISO 26262 HARA 纵线。

HARA 在本章中是严格样板，不是全书的通用上位框架。随后换用 ENV-01 湿度测量漂移，
保留同一提问骨架，却明确丢弃 S/E/C、ASIL 和 SafetyGoal。由此证明：可迁移的是工程问题
的区分方式，不是某个行业的等级语言。

## 本章交付

| 工件 | 作用 |
|---|---|
| `chapter.md` | 4.1—4.8 完整问题链：自然判断、情境反例、概念区分、EPS HARA、S/E/C、Table 4、安全目标、ENV-01 迁移和问题冻结 |
| `problem-contract.yaml` | `ProblemContract PTW-PC-04` 工作件；冻结第 14 章必须回答的问题，不预选本体实现 |
| `examples/item-definition-eps.md` | 历史 EPS 相关项素材；只作来源与迁移输入，不自动继承为新稿结论 |
| `examples/hara-worksheet.csv` | 历史 HARA 行素材；真实项目仍须重做场景、证据与评审 |
| `examples/asil-determination.txt` | Table 4 与边界出口素材；发布时以新稿来源账本为准 |
| `examples/hara-casebook.md` | 历史判例素材；资料性示例不得替代规范判断 |
| `examples/fsc-derivation.md` | FSC 素材，已移交第 5 章重写使用；不再由第 4 章展开教学 |
| `notes/ch04-ch14-pair-rewrite-blueprint.md` | 第 4/14 章配对施工、独立本体与图文同步合同 |

## 读者必须跨过的认知断点

1. 失常名称不能脱离 context 自带后果、等级或目标。
2. concern 不是 consequence，objective 也不是 implementation。
3. S、E、C 回答三种不同的证据问题，先论证再查有限映射。
4. Safety Goal 属于 ISO HARA 适配器；generic Objective 可以服务其他 concern。
5. 假设是会使判断重开的工程对象，不是藏在脚注里的默认事实。

## 两个出口

本章有两个刻意分开的出口：

- 向第 5 章交付 `FSC-InputBoundary`，由第 5/15 章处理功能概念、需求派生、分配与接口；
- 向第 14 章交付 `PTW-PC-04`，由一个独立的场景—目标本体回答查询、约束和单因反例问题。

第 14 章不是第 4 章完成的前置条件，也不 import 第 4 章的业务图。两章通过冻结合同配对，
不通过共享 TBox、ABox 或运行时图耦合。

## 图文合同

本章最终概念图为 `ch04-fig01-context-changes-consequence`：同一车、同一 EPS 偏差，
左侧高速道路，右侧静止维修工位。正文必须在图前让读者寻找不变量，在图后推出
“失常名称不足以身份化场景”。最终图只允许由 built-in ImageGen 生成并完成语义、印刷、
权利和读者验收；PUML 或旧 PDF 只能作草模，不能进入出版包。

## 诚实边界

EPS 与 ENV-01 全部为 `TeachingExample / Synthetic`。S3/E4/C3、ASIL D、S0 边界、
目标和假设均不是现实产品结论。门禁通过只能证明本书工作件满足已编码合同，不能替代
HARA 验证、整车确认、独立评审、符合性判断或放行授权。
