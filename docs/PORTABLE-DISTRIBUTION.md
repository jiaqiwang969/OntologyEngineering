# 独立分发与新目录验证

持续跟进生产问题先看[制造协作入口](MANUFACTURING-COLLABORATION.md)；以下安装和复验由部署者处理。
日常调用与 Jev 凭据准备见[使用说明](USAGE.md)。v0.5.9 的普通完整包和核心包均不附凭据；
已有可选体验包装的绑定规则见[历史说明](JEV-TRIAL.md)。
当前分发版本为 `0.5.9`，两种包均包含默认 Jev 情景路由、预写 instruct、项目方法融合及制造协作入口 `0.1.0`，见[更新说明](releases/manufacturing-method-0.5.9.md)。独立模型质量、双模型升级对照和真实总成本评测留待后续项目实践，
不以这些未完成的评测阻止候选分发，也不宣称准确率或降本已经得到独立验证。

有两种不同的本地制品，不能混用其放行范围：

作者侧 `distribution/shareable-overrides` 中的入口模板保存为 `SKILL.md.in`，避免被本机
当成同名活动 skill。核心构建器只在 staging 中恢复白名单的 `SKILL.md` 目标名，并继续
校验原批准字节摘要；重命名不批准新内容，也不把本地工作版当成冻结公开版。

- **独立核心版**：`python3 scripts/build_shareable_core.py --output /path/to/new.zip`。作者侧的精确资产白名单 `distribution/shareable-core-assets.json` 逐文件锁定来源和 SHA-256，只导出 Semantica、制造方法、通用 CAD 证据、形态/机构重建、CAD／工艺交接、可移植 Fusion 执行层、NX/AutoCAD 可配置远程桥及有界装配/机构筛查器。两卷书、未审历史 CAD 案例、工作站配置和真实项目均不进入该 ZIP。`manufacturing-v0.5.7` 承接 [0.5.6](https://github.com/jiaqiwang969/OntologyEngineering/releases/tag/manufacturing-v0.5.6)，增加来源绑定的第一性原理推导记录与逆向重建方法；该历史版本以 `manufacturing-v0.5.7` 标签固定。当前 `0.5.9` 使用 `manufacturing-v0.5.9` 标签，补齐情景路由、项目方法与治理入口。新目录安装、组件权利和工程边界以 ZIP 内的 `docs/PORTABLE-DISTRIBUTION.md`、`docs/COMPONENT-NOTICE.md` 及随 Release 上传的公开资产台账为准。
- **完整仓库快照**：`python3 scripts/package_skill.py --output ...` 按当前仓库文件清单打包两卷书、公开 CAD 核心和其余仓库资产，包含 `distribution/` 构建清单及不参与 skill 发现的入口模板，便于复验和重建核心包。GitHub 的完整 Release ZIP 是固定标签的仓库快照；不包含仓外的历史私有 CAD 执行器、案例或客户证据。[两卷书整体权利状态](PUBLIC-RELEASE-STATUS.md)与核心包的公开资产台账分开记录。文件检查通过不能当作权利放行。

两种制品都以单一 `ontology-engineering/` 根目录交付，接收方在新目录复验；正式语义 release 与具体客户产品放行另行判断。

## 随目录携带什么

| 内容 | 根目录内的位置 | 作用 |
| --- | --- | --- |
| 情景路由与工程本体 instruct | [路由说明](../references/context-routing.md)、[固定指令](../references/context-routing-instructions.json)、`scripts/route_engineering_task.py` | 从当前情景判断能力需求，由 agent 复核并组织实际工作；两种包携带相同实现 |
| 总入口和制造方法 | [SKILL.md](../SKILL.md)、[制造模块](../skills/manufacturing-process-cost/SKILL.md) | 资料筛选、沟通、方案与反馈迭代 |
| 制造协作入口与任务卡 | [制造协作入口](MANUFACTURING-COLLABORATION.md)、[现场任务卡](../skills/manufacturing-process-cost/assets/templates/shop-floor-action-card.md) | 持续讨论、私有项目本体、方案修订及现场反馈；任务卡按需生成 |
| CAD 模块及交接合同 | [CAD 模块](../skills/cad-agent/SKILL.md)、[通用 CAD 证据](../skills/cad-agent/references/cad-engineering-workflow.md)、[工艺交接](../skills/cad-agent/references/cad-process-integration.md) | 零件、图纸/BOM、装配、机构对象的来源校验、项目 ABox 与双向工艺问题；不自行执行语义规则 |
| 理论、案例和记录模板 | `references/`、`skills/manufacturing-process-cost/` | 书源锚点、脱敏演变案例、八类记录 |
| 报告组件与合成演变输入 | [报告设计](../skills/manufacturing-process-cost/references/report-design.md)、[生成入口](../scripts/manufacturing_report.py)、`skills/manufacturing-process-cost/assets/report-template/` | 四类内容视图、原生 XeLaTeX、冻结与输出对应检查 |
| 工艺图模板 | [使用说明](../skills/manufacturing-process-cost/references/process-flow-templates.md)、`skills/manufacturing-process-cost/assets/report-template/flowcharts/` | 顺序、汇合、返工、条件分支及资源成本关系；原生 TikZ 图源 |
| 固定版本的 Semantica | [运行时锁](../runtime/semantica-source-lock.json)、`runtime/vendor/` | 唯一可执行语义实现及安装校验 |
| 冻结的制造案例 | [案例传输锁](../runtime/semantic-bundles.json)、`runtime/vendor/` | 141 项 Semantica 原生资产、68 个可重跑场景 |
| 运行、分发及回归检查 | [案例入口](../scripts/run_manufacturing_cases.py)、[分发工具](../scripts/package_skill.py)、`tests/` | 校验资产、执行案例、发现断链和打包遗漏 |
| CAD 非语义运行源锁 | [CAD 源锁](../runtime/cad-operational-source-lock.json) | 逐文件约束动态加载和子进程调用，修改即重审；不豁免第二语义后端 |
| 远程 CAD 桥、逆向重建与装配/机构工具 | [CAD 模块](../skills/cad-agent/SKILL.md)、[能力图](cad-agent-capability-map.md) | 图片/视频/专利到候选结构的通用方法与证据绑定、NX MCP SSH 传输、AutoCAD COM/relay、装配与力学通用内核、AABB 装入/顺序和四连杆位置筛查；媒体自动识别、外部 CAD 软件与真实主机验收不随包提供 |
| Fusion 专用 wheel | [运行身份锁](../skills/cad-agent/dist/fusion-runtime-lock.json)、[检验器](../skills/cad-agent/scripts/verify_fusion_runtime_wheel.py) | 仅安装 Fusion 执行代理；打包和安装后检查 wheel 摘要、成员、入口及旧语义模块缺席 |

冻结案例 ZIP 是 Semantica 已验证候选的不可变传输副本。它包含数据、查询、约束、规则和 oracle，没有作者工作区、个人路径、权限绑定或替代引擎。传输锁检查每个成员、manifest、资产摘要和运行时身份；实际语义解释和验证全部交给 Semantica。修改案例须在受控 Semantica 流程中形成后继，再更新传输包和锁。

客户原件、真实对话、报价、模型、私有映射和实际运行日志不进入这个目录的通用分发包。旧 CAD 本体、历史查询文件与含语义 MCP 的原 CAD wheel 留在作者侧迁移来源，不进入通用包。真实企业的项目目录、输入事实和新授权由接收方提供；不会自动继承源组织的采用状态。

## 怎样检查和打包

在根目录运行，无需安装语义运行时即可做文件检查：

```bash
python3 scripts/package_skill.py
python3 scripts/package_skill.py --output ../ontology-engineering.zip
```

工具按明确的根目录清单收集文件，排除 `.git`、`.venv`、缓存及构建中间件；检查本地文档链接是否闭合、符号链接、直接个人标识，以及冻结包展开后的文件清单与文本。失败不输出分发 ZIP。成功时附带 `PORTABLE-MANIFEST.json`，其中每个摘要对应实际放入 ZIP 的字节。已有输出不覆盖，选择后继文件名。

检查解压后的目录时，也会核对其 `PORTABLE-MANIFEST.json`，发现交付后缺失、增加或变更的受管文件。新版本从受控作者目录重新打包；保留收到的旧包及清单，不在原交付快照里消除变更痕迹。

关键词扫描不替代新案例的重识别审阅；增加客户衍生内容时还需确认少见参数组合不会暴露来源。作者侧打包命令只生成本地制品；是否发布及发布范围由独立的资产台账、权利说明和实际 GitHub Release 记录确定。

## 接收方最小验证

解压到任意新位置，从新目录运行：

```bash
python3 scripts/package_skill.py
python3 scripts/search_ontology_sources.py --scope book "对象身份 证据 依赖"
python3 scripts/run_manufacturing_cases.py --list
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
bash skills/cad-agent/setup.sh
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/verify_fusion_runtime_wheel.py --installed
bash skills/cad-agent/doctor.sh
runtime/.venv/bin/python scripts/run_manufacturing_cases.py --run \
  --output ../work/manufacturing-replay-001
```

`--list` 是文件完整性检查；`--run` 是新环境的原生案例执行，分别保存结果。CAD doctor 是本机能力预检。远程 CAD profile 由接收方填写其 SSH 别名与侧车路径；工程软件、许可和远程主机验收仍由接收方负责。公开桥和有界求解器可先离线运行 `python3 skills/cad-agent/scripts/test_remote_assembly_mechanism.py`。可用重复的 `--scenario MFG-01-PN` 参数选择部分场景，必须报告实际运行范围。全部场景通过也只证明所列合成输入与 oracle，不能替代客户方案快照、现场能力、真实成本或制造放行验证。

Python 和 Python 依赖属于安装环境；首次安装可能访问包源，并非完整离线安装包。普通阅读、书源检索和冻结制造案例回放不需要 Semantica 源码 checkout。重新构建 Semantica、维护书稿或治理原生候选另按各自工作流提供受控输入，这些可选维护任务不作为制造方法的首次使用前提。

### 报告组件的独立使用

报告组件需要 Python 3.10 或以上。生成 PDF 另需 XeLaTeX（含 `ctex`、Fandol、TeX Gyre 等发行版字体与包）和 Poppler 的 `pdftotext`、`pdffonts`、`pdfinfo`；这些是公开的系统依赖，不是其他 skill 或作者机器上的素材。未加 `--compile` 时只生成原生 TeX 与冻结资料。输出放在分发目录外的新目录：

```bash
python3 scripts/manufacturing_report.py generate \
  --input skills/manufacturing-process-cost/assets/report-template/example/03-resource-cost.json \
  --output ../work/report-001 --compile
python3 scripts/manufacturing_report.py verify ../work/report-001
```

再按 [报告核验说明](../skills/manufacturing-process-cost/references/report-design.md) 渲染并逐页看图。组件保存输入、模板、内容映射、生成器摘要及实际 PDF 摘要，历史复验使用对应的整根分发版本。四轮合成报告、独立询价试用和 68 个原生语义场景各有验证范围，不能相互替代；组件当前不接入本次报告快照的 Semantica 回执。

## 后续更新的约定

- 方法版本、记录模板版本、报告组件版本、Semantica 包版本分别记录；不为改说明制造空语义版本。
- 改动后重做文件闭合、脱敏检查、根 skill 门禁及新目录试用。维护传输层时加入篡改、遗漏、重复路径和越界路径反例。
- 旧版本、旧回执保留。分发新版本时说明发生的变化和需要重新验证的范围，不以旧验证覆盖新产物。
