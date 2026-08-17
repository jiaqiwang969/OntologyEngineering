# 第8章：ASIL 分解与相关失效分析 / Chapter 8: ASIL Decomposition and DFA

本章追踪一场 EPS 联合评审：`C(D)+A(D)` 已命中受控基准，硬件与软件查询也各有绿灯，
为什么评审仍拒绝把分解从 Draft 改成 Approved？正文用同一个冲突依次固定分解对象、检验
两条需求的功能冗余、解释等级与活动义务、区分共存/FFI/技术独立性，再让一条共享时间戳疑点
走完安全分析、DFA、措施与验证。章节最后只签与证据状态等宽的窄结论。

> 案例边界：EPS 分解、候选通道、DFA 记录及 Approved fixture 均为合成教学数据。EPS 主
> ABox 明确停在 `Draft / Planned / EvidenceCandidate / Undetermined`；正向 fixture 只证明
> 当前机器合同的路径可达，不证明任何现实项目已获得充分独立结论。

## 受控例外怎样表达

第5章的 `TraceASILInheritanceShape` 继续守“派生安全需求不得任意降级”的一般规则。第8章
没有删除它，而是要求降级主张绑定一次完整分解：一条初始需求、两条不同产出、受控基准、
双向链接、不同候选元素、精确括号标记和 DFA 槽位。半张分解、错误上游、单条自降或自建
policy 均不能取得豁免。

§5.4.9 明列八个基准，并许可产生更高 ASIL 的方案；ISO 没有给出通用偏序算法。本仓库采用
“无序配对、逐支达到某个基准”作为可执行项目策略。它是本书的机器合同，不是 ASIL 加法，
也不得冒充标准原文。QM(x) 分支仍是安全需求；QM 是分类而非 ASIL，不能参加 A–D 的自由
rank 比较。括号沿追溯链保留安全目标 ASIL，二次分解时不会把根部 D 改写成中间层 C。

## 交付物与证据边界

| 产物 | 当前作用与边界 |
|---|---|
| `chapter.md` | 九问连续正文；从联合评审冲突走到 `EPS-S08` 候选交接 |
| `../../ontology/asil-decomposition-schemas.ttl` | 八个规范基准及本书逐支比较策略，二者明确分层 |
| `../../ontology/abox-eps-decomposition.ttl` | EPS 候选分解、DFA 活动/工作产物和四条状态轴 |
| `../../ontology/source-anchors-part9.ttl` | 已对象化 Clause 5、选定 Clause 6/7/8、Figure 2/3 与资料性 Annex B/C 的精确坐标；它不是 Part 9 全量模型 |
| `../../ontology/fsafety-tbox.ttl` | 分解、QM(x)、DFA 活动/产物/主题/发现/措施/结论词汇 |
| `../../ontology/shapes.shacl.ttl` | Candidate 结构门、精确标记门及 Approved 闭世界证据门 |
| `../../eval/eval-cases.yaml` | 基准登记查询，以及并排返回分解、DFA 执行、工作产物和结论状态的 EPS 查询 |
| `../../eval/shape-fixtures.yaml` | higher/QM/Approved 正例与单输出、错组合、同元素、缺槽位、错标记、过早批准等反例 |

Candidate 门只证明已编码的最低结构存在，不判断两条需求在语义上各自符合初始需求，也不证明
物理独立。Approved 门在其上要求 DFA 完成、主题处置、允许的结论分支、可信原因措施及验证、
DFA 验证报告和确认评审；它仍不能验证理由真实性、试验可信度、评审人胜任性或产品可放行。
`ApprovedDecompositionEvidenceShape` 是本书把多条义务投影为发布条件的 house policy。

尚未结构化或仍需人工判断的主要对象包括：逐分支能力卡、§5.4.12 多层活动配置、Clause 6/8
完整模型、DFA 分析深度和物理独立性，以及 BMS/AEB 的 DFA ABox。来源、机器门禁与教学
推演任何一项绿色，都不能把这些开放项自动改名为完成。

## 快速开始

```bash
.venv/bin/python -m unittest \
  eval.test_run_eval.ASILDecompositionSourceAndPolicyTests
.venv/bin/python eval/run_eval.py
```

第一条验证第8章的策略和放行语义，第二条运行开发态全量门禁。最终报告必须来自全部写入停止
后的新运行及其真实退出码，不能复用输入漂移前启动的旧进程。
