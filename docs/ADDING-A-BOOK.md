# 新增一本标准书

本文定义新书接入合同。目标不是“把标准翻译一遍”，而是把陌生领域变成普通工程师
能学习、查询、验证和受控使用的知识产品，同时维持唯一语义运行时：

```text
新书正文/图/来源地图 → OntologyEngineering 书侧
本体/CQ/shape/query/case/rule/contract/lifecycle → Semantica governed package
```

书是新加入的“石头”。可执行语义不能在 OE 或书包里形成第二套发布正本。

## 进入条件

开始前必须具备：

- 明确的目标读者和制造业使用场景；
- 合法获得的标准或技术资料及可说明的使用权限；
- 至少一名能够复核实质内容的领域审阅者；
- 私有来源区、作者工作区与公共发布区的隔离；
- 不把书、package、Skill、测试或 AI 输出冒充官方标准、认证或合规结论；
- 为新书在 Semantica 中建立稳定 package namespace 的计划。

条件不满足时，可以研究公开材料和起草 Book Charter，但不得导入受限原文、生成
条款级结论或发布“完整标准书”。

## 1. 创建作者工作区

```bash
python3 skills/standard-to-book/scripts/init_book.py \
  --slug welding-quality \
  --title "焊接质量工程导读" \
  --standard "目标标准族" \
  --output ./workbooks
python3 skills/standard-to-book/scripts/validate_book.py \
  ./workbooks/welding-quality --stage structure
```

脚本只创建合同、台账和作者目录，不读取标准、不调用模型、不覆盖现有目录。标准原文
保存在工作区之外的私有受控位置，仅以逻辑 ID 和哈希关联。`.gitignore` 是误操作防线，
不是允许把私有证据放进书包的理由。

作者工作区里的 ontology/query/shape/case 草稿只是候选材料。候选发布前必须通过
`semantica.ontology.refinery/v1` 进入受控 package registry；OE 不接受这些草稿成为
平行运行资产。

## 2. 冻结 Book Charter

在 `book-charter.md` 确认：

1. 谁会读，已有知识是什么；
2. 要解决哪些现场问题；
3. 读完允许作出哪些决定，哪些决定仍禁止；
4. 标准版本、适用/排除范围与领域审阅者；
5. 书、Semantica package、Skill 和 Agent 各自职责；
6. 公开范围、私有证据范围与 package namespace。

Charter 未冻结，不进入批量写作、生图或 package 发布。

## 3. 建立来源与权利账

在 `sources/source-register.csv` 登记逻辑 ID、版本、哈希、权利基础、可公开性与复核
状态。公共账禁止出现本机绝对路径、下载令牌、真实企业名称或受限原文。

来源账回答“依据来自哪里、能否使用”；它不自动证明转述正确。决定性内容仍由领域
审阅者回到合法持有的原始来源核对。精确条款和表格不得从模型记忆生成。

## 4. 从真实问题建立 CQ

先写 10–30 个普通工程师会提出的问题，例如：

- 这个术语在现场究竟指什么对象？
- 哪个版本、批次、配置或人员身份才是同一个对象？
- 什么活动产生事实，什么材料只能是候选证据？
- 哪些条件必须同时成立，哪些例外改变结论？
- 什么时候必须停止，让专家、审核人或责任人决定？

每个 CQ 声明读者、所需证据、预期回答、能力 profile 与 exact oracle。CQ 是范围和
验收合同，不是章节装饰。候选 CQ 可在作者工作区迭代，晋升正本进入 Semantica package。

## 5. 建立命题账与书源锚点

为每个公开命题登记章节、CQ、来源、原创摘要、类别、权限边界、证据判据和审阅状态。
区分标准转述、作者解释、最佳实践与教学假设。每章和每个 CQ 至少由一个已审阅命题
覆盖，并给出稳定书源锚点，供 package provenance 回指。

## 6. 概念化并建立 Semantica package

按领域建立术语、分类、关系、身份、状态、版本、权限与未决项。在 Semantica 中为
每章或明确的 domain unit 建立 governed package，至少包含：

- manifest、contract、命题/CQ 与书源锚点；
- TBox/ABox、positive、single-fault negative、ambiguity、prior-release cases；
- named SPARQL、SHACL、受支持规则与 exact oracle；
- required capabilities 与诚实的 unsupported/partial/blocked 状态；
- asset hashes、snapshot/diff、version、PROV、execution report 与 release receipt；
- Python/CLI/MCP 通过同一个 `SemanticPackageRunner` 的可发现性。

一次炼化输入必须是完整 `PackageDelta`，顶层覆盖：

```text
ontology · competency_questions · shapes · queries · rules · cases
contract · provenance · book_impact
```

`book_impact` 必须明确为 `none`、`vol1-method`、`vol2-iso-exemplar` 或合同声明的组合；
普通行业实例默认为 `none`。书稿影响不能埋在自由文本 provenance 中，也不能因为
package 变化就自动重写书。

不要让所有书 import 一个万能总 TBox。跨书对齐要独立登记；移除映射后，每本书仍能
说明自己的范围。不要在 OE 增加 backend adapter、package loader 或 fallback。

## 7. 用制造现场故事讲明白

正文采用“场景先行、误判完整发生、失败暴露、命题后置”：

1. 用足够真实但不泄漏企业信息的合成现场；
2. 让常见误判自然发生；
3. 说明它为何在对象、版本、证据或权限上失效；
4. 给出标准转述、本体表达和 package 检查；
5. 明确仍需专家或有权人判断的部分。

平民化是讲清完整判断链，不是删掉条件、例外与责任。

## 8. 图文协同

每幅图先登记 visual question、semantic baseline、输入权利、生成方式、hash、caption
与 alt text。图可以解释概念，不能补造事故、试验、产品或合规证据。使用生成工具时，
只提交已授权输入；公共仓只保存项目自己的提示合同、审核结果和发布台账。

## 9. 分层放行

候选必须按 `candidate → proposed → committed → regression_passed →
release_complete → promoted` 逐级推进；`published` 是外部有权人的独立动作。候选发布
至少经过：

1. 来源与权利检查；
2. 章节/CQ/命题覆盖检查；
3. Semantica registry、assets 与 exact-oracle 检查；
4. positive/negative/ambiguity/prior-release 回归；
5. source commit、wheel SHA-256、package/input/output 与 PROV receipt 绑定；
6. 章节问题链、图文一致性与普通工程师冷读；
7. 领域专家复核；
8. 隐私扫描和公开资产 allowlist；
9. PDF、书本 Skill、Semantica package version 与 package lock 冻结；
10. 有权人分别作出 promotion 与对外发布决定。

验证工作区：

```bash
python3 skills/standard-to-book/scripts/validate_book.py \
  ./workbooks/welding-quality --stage charter
python3 skills/standard-to-book/scripts/validate_book.py \
  ./workbooks/welding-quality --stage release --write-lock
python3 skills/standard-to-book/scripts/validate_book.py \
  ./workbooks/welding-quality --stage release
```

`release/package-lock.csv` 冻结书侧；Semantica receipt 冻结执行侧。二者必须通过稳定
package ID、书源锚点和 hash 指向同一候选版本。任一侧字节变化都应开启新候选发布。

进入正式问答检索的正文与 reader/usage guides 也必须有内容锁，不能依赖代码中的裸路径
allowlist。锁条目使用无绝对路径、无 `..`、无非规范分隔的 POSIX 相对路径，并保存
`(SHA-256, path)`；检索器必须先对同一批待读取字节核验摘要，再解码、评分或引用。
正文锁与 guide 锁可以按书的作者工作流分开，但缺任一正式锁或任一字节漂移都应失败关闭。

## 完成定义

一本新书只有在以下内容指向同一冻结版本时，才是候选发布：

- 人可读的书与图；
- 可查询的来源地图与命题地图；
- Semantica 中可发现、可执行、可验证的 built-in packages；
- source/wheel/package/input/output/PROV 绑定的 receipts；
- 来源、权利、隐私、技术、冷读与领域审阅记录；
- zero-exception gate 证明 OE 没有平行语义资产或 backend bypass。

任何自动绿色都只证明当前输入满足已编码合同，不授予出版、认证、制造、风险接受或
对外承诺权限。新书注册时增加的是新书源与 Semantica packages，不改变“两卷/多卷书
是石头、Semantica 是唯一可执行语义层”的总架构。
