# ch11 来源审计：收口合同与 Safety Case 生命周期边界

## 审计结论

第11章不是再吸收一遍前十章的 ISO 条款，而是只读消费其已登记教学对象，用一份待签报告检查
“对象—检查器—当次结果—未决—效力边界”能否同时成立。正文新增的直接外部来源只有
ISO 26262-10:2018 §5.3.2；其余 HARA、系统、硬件、软件、分解、生产和支撑过程内容均作为上游
章节与本仓库受控教学资产回读，不在本章重新制造规范性主张。

§5.3.2 属 Part 10 资料性指南。本章准确消费其增量开发说明、Safety Plan 的增量策划 Note、三个
示例里程碑、确认评审引用和修改影响说明；没有把示例版本名提升成所有项目的强制进入条件，也没有
把“增量”解释成可以提前关闭未执行活动。

当前来源状态仍为 `machine_extracted_review_pending`，权利状态仍为 `review_required`。本审计证明
正文与选定本地结构化提取坐标一致，不替代专家复核、文本哈希、出版授权或 ISO 合规判断。

## 直接来源坐标

结构化源：
`structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json`

| 物理页 / block | bbox | 本章用途 | 模态边界 |
|---|---|---|---|
| p23/b3 | `[114,325,467,342]` | §5.3.2 标题与坐标入口 | 只作单元索引 |
| p23/b4 | `[112,351,941,382]` | Safety Case 可作为增量活动并与生命周期阶段集成 | 资料性解释 |
| p23/b5 | `[112,391,941,420]` | Safety Plan 可策划增量步骤和初步版本 | Note，不升格为 shall |
| p23/b6 | `[112,429,942,504]` | preliminary/interim/final 的示例里程碑顺序 | `For example`，不是版本名硬定义 |
| p23/b7 | `[112,512,806,527]` | 确认评审回指 Part 2 §6.4.9 | 不由 ch11 执行或宣称完成 |
| p23/b8 | `[112,537,941,568]` | 相关项修改时评价影响并按需更新 Safety Case | 不等于每次变化可静默吸收 |

正文 §11.5.3 对这五段内容作语义重构，没有复制长段原文。`ontology/source-anchors-part10.ttl`
以 `Clause_10_5_3_2` 和五个 `SourceFragment` 保存 b3–b8 的受控坐标；outline 同步保存逐块坐标。
`coverage/source-units.csv` 的 `SU-10-5.3.2` 已登记 `chapter_ids=ch11`、
`disposition=anchor_only`。锚只对象化来源身份与 informative 边界，不对象化项目执行，因此不能写成
`modeled`。

## 上游资产与本章责任边界

| 本章消费对象 | 上游责任 | ch11 的权限 |
|---|---|---|
| HARA、FSR/TSR、硬件度量、软件需求、分解、生产与工具对象 | ch04–ch10 | 只读查询、比较结构深度与报告边界 |
| Clause 8 安全确认对象 | ch05 对象责任边界 | 读取 Draft/Planned/NotRun/NotPerformed，不升级状态 |
| Claim、Argument 与 EvidenceCandidate 角色 | ch03 CAE 治理边界及对象责任章 | 读取七件候选角色，不创建或接受证据 |
| CQ/oracle/fixture 与收口报告合同 | ch11 / eval 基础设施 | 可修订检查合同，但必须记录验证器变化影响 |
| `bundle-manifest.yaml` | 历史冻结记录 | 保留并核验，不因当前漂移自动重冻 |

七件候选证据的数量、Clause 8 状态链和 `ClaimOpen` 都来自仓库当前上游教学对象。它们是书稿模型
事实，不是 ISO 26262 自带案例或真实项目证据。BMS/AEB 的最小链与 EPS 的深链同样属于合成教学
数据；机器查询可证明已登记关系，不能证明工程活动、评审或产品实现存在。

## 历史报告与当前诊断的证据边界

`reports/acceptance/20260813-r4-final.json` 是一次真实仓库检查记录。它支持当次
`capstone-three-items=0`、`knowledge-model=1`、overall failed、九项 gate 记录及输入快照事实；
`release_mode=false` 表示没有运行出版 `--release` 门禁集。JSON 未保存子进程完整诊断，因此不能
从后来诊断快照的失败签名倒推历史 `knowledge-model` 红灯原因。

重写期的 manifest 漂移、capstone 退出 2 和契约测试结果必须绑定单独的 dated diagnostic snapshot，
不能以“当前”二字永久写死。该诊断只证明旧 manifest 对所记录工作树字节失败关闭，以及检查器的
已编码局部合同；它既不是新冻结候选，也不是历史报告的改写。

## 未关闭项

- Part 10 §5.3.2 仍待来源专家复核、文本哈希与权利处置；
- Part 2 §6.4.9 只通过 Part 10 回指进入本章叙述，本章未独立吸收其完整要求；
- 当前旧 manifest 已与开发态输入漂移，不能运行成新的冻结包结论；
- CQ、Shape、fixture 和契约测试只覆盖已注册缺陷模型，不能证明图外无遗漏；
- 三案例均为 `TeachingExample`，任何机器 PASS 都不构成验证、确认措施、功能安全评估、认证或产品放行。
