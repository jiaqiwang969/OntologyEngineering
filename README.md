# OntologyEngineering

## 让只有专家看得懂的标准，成为制造业看得懂、查得到、能验证的中文知识产品

OntologyEngineering 面向正在进入陌生专业领域的工程师、中小型制造企业和
**AI × 制造**团队。项目把合法获得的 ISO、IEC、GB 或行业标准及其工程知识，
在专业复核下重构为中文书、制造业案例、可检索知识、本体与约束，以及受来源、
流程和权限约束的 Agent Skill。

我们所说的“标准平民化”，不是删掉标准里的条件、例外和责任，也不是让 AI
替代专家，而是把专家脑中隐含的术语、判断路径、证据要求和适用边界讲明白，
让普通工程师能够学习、提问、复现和验证。

> 降低获取、理解、验证和使用行业知识的门槛；
> 不降低工程判断、合规审查、风险接受和授权责任的标准。

长期目标是形成一条可复用的“标准书厂”：只要来源与使用权合法、应用范围明确，
并有领域审阅者参与，一部标准就可以沿同一方法转化为一套**可读、可查、可运行、
可追溯**的知识产品。“任何 ISO 都可以变成一本书”描述的是可复用方法，不是上传
标准后一键生成，也不是绕过版权、专业判断或认证程序。

## 60 秒上手

### 只想看懂

直接从两卷书开始，不要求先懂 RDF、OWL 或 SHACL：

| 卷 | 书名 | 成书 PDF | 作用 |
|---|---|---|---|
| 第一卷 | 《工程本体论》 | [📕 工程本体论-全书.pdf](references/ontology-engineering-book/handbook/工程本体论-全书.pdf)（240 页图文版） | 理论与方法底座：怎样把对象、身份、关系、规则和证据说清楚 |
| 第二卷 | 《产品可信工程》 | [📘 产品可信工程-全书.pdf](references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf)（345 页图文版） | 完整参考实现：以 ISO 26262 为样板，演示一部工程规范怎样被讲懂并逐章本体化 |

### 想问一个问题

```bash
git clone https://github.com/jiaqiwang969/OntologyEngineering.git
cd OntologyEngineering
python3 scripts/search_ontology_sources.py "五张 PASS 为什么拼不成一次放行"
```

检索结果返回书中的章节、案例和上下文，而不是只给一个脱离场景的定义。

### 想确认当前 Skill 仍可运行

```bash
python3 scripts/eval_ontology_skill.py
```

作为 Agent Skill 使用时，将仓库放入或链接到 `~/.codex/skills/ontology-engineering/`
或 `~/.claude/skills/ontology-engineering/`。第一次阅读和检索不要求安装 Agent。

### 想把另一部标准做成书

先创建一个不含标准原文的私密友好型书包：

```bash
python3 skills/standard-to-book/scripts/init_book.py \
  --slug quality-management \
  --title "质量管理工程导读" \
  --standard "ISO 9001" \
  --output ./workbooks
```

随后按生成的 Book Charter 填写目标读者、真实问题、来源权利、能力问题和领域审阅者。
先运行结构检查；完成 Charter 后再进入更严格的 `charter` 与 `release` 阶段：

```bash
python3 skills/standard-to-book/scripts/validate_book.py \
  ./workbooks/quality-management --stage structure
```

### 想把自己的工程实践长成行业本体

用 `skills/domain-ontology-loop/` 的内化循环：每次实践产出 delta，
经差异分析、带理由的冲突判决、版本快照与 PROV 谱系合并进本体，
最后跑 CQ 防遗忘回归——**学新不忘旧，靠旧 CQ 全绿来证明，不靠感觉**：

```bash
python3 skills/domain-ontology-loop/scripts/internalize.py init \
  --workspace ./my-domain --name MyDomainOntology \
  --baseline lesson01-delta.json --attempt lesson01
python3 skills/domain-ontology-loop/scripts/internalize.py commit \
  --workspace ./my-domain --delta lesson02-delta.json --attempt lesson02
python3 skills/domain-ontology-loop/scripts/internalize.py regress \
  --workspace ./my-domain
```

完整规矩与 CAD 课程（01-fusion-tutorial）的映射见
`skills/domain-ontology-loop/references/loop-contract.md`；
可运行佐证见 `demos/internalization_loop.py`（CI 每次 push 重跑）。

正式候选发布前，完成命题账、插图、本体、书本 Skill、机器测试报告和公开白名单，再运行
`--stage release --write-lock` 冻结全部文件，随后用不带 `--write-lock` 的 release 检查复核。

完整接入方法见 [新增一本标准书](docs/ADDING-A-BOOK.md)，隐私与公开边界见
[隐私、来源与公开发布](docs/PRIVACY-AND-RIGHTS.md)。

## 一套框架，五种同源制品

```text
合法来源与制造业问题
  -> 面向普通工程师的中文书与图解
  -> 可检索、可追溯的结构化知识
  -> 可验证的 ontology / query / SHACL / fixtures
  -> 可复用的 Skill 与受控 Agent 工作流
  -> 来源、权利、版本和发布证据
```

书负责帮助人理解；本体负责稳定语义；查询和约束负责暴露不一致；Skill 负责复用
工作方法；Agent 只在权限范围内执行。任何一层都不能自行制造工程事实、合规结论或
发布授权。

## 一部标准怎样变成一本书

```text
合法来源与适用目标
  -> 普通工程师真正会问的问题（CQ）
  -> 术语、对象、身份和关系的概念化
  -> 制造现场故事与自然误判
  -> 要求、例外、证据和适用边界
  -> 查询、约束、正例与单因反例
  -> 图文书、Skill、评测和版本化发布
```

所有书复用的是问题方法、生产工序、文件合同和验证门禁。每本新书都必须重新建立
领域语义、来源与权利、教学案例、适用边界、图像和审阅结论。第二卷的“前十章传统
实践＋后十章本体化回答”是 ISO 26262 的教学设计，不是所有新书必须套用的目录模板。

## 项目总纲

长期保持稳定的原则：

- 用能力问题划定范围，不建立包罗万象的万能本体；
- 区分语义权威、事实权威与决定权威；
- 让 Claim、Evidence、Counterevidence 和未决项同时可见；
- 用正例、单因反例、查询和约束验证声明过的能力；
- 保留来源、版本、身份、变更、复核与发布链；
- 让 LLM 翻译和组织知识，但不让模型输出冒充来源或授权；
- 把真实企业资料、标准原文和个人会话留在私有受控层；
- 任何公开包都采用默认拒绝、白名单放行。

一句话原则：

> 复用问法、工序、契约、文件形态和门禁；
> 重做语义、事实、来源、权利、结论、视觉和授权。

## 当前两卷的关系

第一卷教“怎样建”；第二卷证明“怎样在一个严苛领域完整走一遍”。第二卷前十章
按 ISO 26262 生命周期讲传统功能安全实践，后十章把同一批困难逐项转成主张、身份、
治理、情境危害、需求追溯、测量证据、版本变化、依赖独立、制造现场和发布保证十个
独立本体。

两卷共享同一条思想主线：

> 工程本体是语义之根，受控活动是事实之源，有权的人是决定之源。

第二卷中的 EPS-RC17、ENV-01、人物、事故和数字均为合成教学材料，不对应真实企业、
产品或个人，不得作为真实产品结论引用。

## 隐私与权利默认值

公共仓库不接收：标准原始 PDF 及受限抽取物、企业内部资料、Claude/Codex 会话、
rollout、附件临时路径、密钥、令牌、个人绝对路径、未清权利的参考图或带身份信息的
元数据。公共包只接收经过审阅的原创讲解、合成案例、可公开脚本、本体和明确可发布
的图文资产。

提交或制作公开包前运行：

```bash
python3 scripts/check_public_privacy.py --root . --include-ignored
```

该检查只能发现已经编码的泄漏模式，不能替代版权审查、技术评审或人工隐私复核。
当前两卷的再次公开发布状态见 [发布阻断清单](docs/PUBLIC-RELEASE-STATUS.md)。

## 目录结构

```text
SKILL.md                              # 两卷阅读、检索和来源锚定入口
agents/                               # Skill UI 元数据
skills/standard-to-book/              # “标准 -> 书”复用流程与书包脚手架
docs/                                 # 新书接入、隐私和公开发布合同
references/
├── ontology-engineering-book/        # 第一卷
├── product-trustworthiness-book/     # 第二卷
├── iso-normative-ontology/           # 当前分发的转述与本体化卡片；仍受发布权利审查
├── source-map.md                     # 第一卷来源地图
└── product-trustworthiness-source-map.md
scripts/
├── search_ontology_sources.py        # 全文检索
├── eval_ontology_skill.py            # 现有两卷评测
├── engrave_iso.py                    # 从用户本地受控来源生成候选结构
└── check_public_privacy.py           # 公开前隐私预检
```

## 当前评测边界

现有两卷 Skill 的固定题库评测结果为：事实正确率 998/1000、陷阱诚实率 39/39；
当前仓库语料上的 Mode A 检索可答率为 884/961（92.0%）。题库、方法和存档见
`references/benchmark/`。

这些数字只描述当前冻结书包在该题库上的表现，不代表能够正确回答所有 ISO 问题，
也不证明任何真实产品安全、合规、通过认证或可以发布。

## 最后一句

> 标准平民化，不是把严谨性删掉；
> 而是把严谨性讲明白，并做成普通工程师能够验证的步骤。

## 许可

本仓库以 [MIT License](LICENSE) 发布，覆盖代码、脚本、demo、本体与随仓库
分发的两卷书稿及配套资产。MIT 允许自由使用、修改与再分发，但请注意：

- 本仓库不含任何 ISO/IEC/GB 标准原文；条款均为作者转述，引用时以
  「条款号 + 转述」报告，原文核对回你自己合法获得的标准文本；
- 第二卷全部人物、事故与 EPS 数据为合成教学材料，不得作为真实产品结论、
  合规意见或认证依据引用（详见 `docs/PRIVACY-AND-RIGHTS.md`）。
