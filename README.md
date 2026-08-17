# OntologyEngineering —— 工程本体论 · 两卷一体 Skill

一个可检索、带评测门禁的 Agent Skill，打包两卷互为表里的书：

| 卷 | 书名 | 成书 PDF | 定位 |
|---|---|---|---|
| 第一卷 | 《工程本体论》 | [📕 工程本体论-全书.pdf](references/ontology-engineering-book/handbook/工程本体论-全书.pdf)（图文导读版，12MB） | 理论卷：本体论基础、方法论、描述语言、推理、知识图谱、本体×LLM，九章 + 制造业综合实战 |
| 第二卷 | 《产品可信工程》 | [📘 产品可信工程-全书.pdf](references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf)（345 页图文版，含封面与章首艺术图） | 实战卷：以 ISO 26262 汽车功能安全为贯穿样板。前十章讲透 AI 之前的传统最佳实践（术语/管理/HARA/系统/硬件/软件/分解 DFA/生产运行/支撑过程）；后十章把同一套工程逐章本体化（主张/身份/治理/情境危害/需求追溯/测量证据/版本变化/依赖独立/制造现场/发布保证十个独立本体）——"如何把一部工程规范本体化"的完整示范 |

第二卷是第一卷的实战续篇：前十章是"AI 之前的世界"，后十章是"AI 之后的世界"。
两卷共享同一套写作方法（场景先行、自然误判完整发生、命题后置、问题链）与
同一条思想主线：**工程本体是语义之根，受控活动是事实之源，有权的人是决定之源。**

## 目录结构

```
SKILL.md                                    # skill 入口：双卷路由、来源锚定规则、评测门禁
references/
├── ontology-engineering-book/              # 第一卷全书（章节 + examples + 排版工程）
├── product-trustworthiness-book/           # 第二卷全书（前言 + 20 章 + 附录 A–D）
│   └── handbook/                           #   321 页成书 PDF + 全书图谱计划
├── iso-normative-ontology/                 # ISO 26262 本体化刻录层（条款 TTL + 卡片视图）
├── source-map.md                           # 第一卷章节地图
├── product-trustworthiness-source-map.md   # 第二卷章节地图（含人物与事故索引）
└── eval-cases.json                         # 22 条能力问题式检索评测（成书质量门禁）
scripts/
├── search_ontology_sources.py              # 双卷 + 刻录层全文检索
├── engrave_iso.py                          # 标准刻录器（需本地受控提取件）
└── eval_ontology_skill.py                  # 评测门禁运行器
```

## 用法

```bash
# 检索（双卷）
python3 scripts/search_ontology_sources.py "证据射程 主张 部件"

# 跑评测门禁（当前 22/22 用例）
python3 scripts/eval_ontology_skill.py
```

作为 Agent Skill 使用时，将本目录放入（或符号链接到）`~/.codex/skills/` 或
`~/.claude/skills/`，Agent 依 `SKILL.md` 的来源锚定规则先检索后作答，
拒绝无来源的空谈。

## 千题闭卷评测（benchmark）

对本 skill 的 1000 题闭卷测评（题库、评价标准、判分工具与答卷存档见
`references/benchmark/`）。答题方唯一知识来源为本 skill；金标准与答题全程
隔离并经赛后审计（详见 benchmark/README.md）：

| 指标 | 成绩 |
|---|---|
| 事实正确率 | **998/1000 = 99.8%** |
| 陷阱诚实率（未覆盖内容拒绝臆答） | **39/39 = 100%** |
| 教学类讲例率（用书中案例讲出答案） | **75.1%**（核心教学题 94%） |
| 检索可答率 | **97.2%** |

评价标准是双金标准：**事实分**（ISO 实质正确）+ **讲例分**（能用书中案例
讲出来——这正是本书区别于干查表的价值）。方法表 347 题的金标准来自
仓库侧 RDF（不在 skill 内），与 skill 附录 D 构成独立交叉验证。

## 案例与边界纪律

- 第二卷贯穿案例 EPS-RC17（电动助力转向候选）与 ENV-01（环境监测单元）及全部
  人物、事故、数字均为**合成教学材料**，不对应任何真实企业、产品或个人，
  不得作为真实产品结论引用。
- 正文对 ISO 26262 为自然语言转述；本仓库不包含、不再分发标准原文。
- 第二卷插图已按 `handbook/book-figure-plan.yaml` 批量生成并排版入书；
  个别在文编号图仍为提示词占位，随图管线迭代更新。

## 一句话

> 一件产品值得被相信，从来不是因为谁说了一句"放心"——
> 而是因为有一群人，肯把自己的名字一行一行留在它活着的记忆里。
