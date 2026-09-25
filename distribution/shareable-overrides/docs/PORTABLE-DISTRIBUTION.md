# 独立核心包的安装与边界

日常调用和 Jev 凭据准备见[使用说明](USAGE.md)。普通核心包和源码不含凭据；
既有可选体验包装的规则另见[历史说明](JEV-TRIAL.md)。

当前分发版本为 `0.5.9`，包含默认 Jev 情景路由、预写 instruct 与项目方法融合，见[更新说明](releases/manufacturing-method-0.5.9.md)。
ZIP 解压后得到单一 `ontology-engineering/` 根目录。所有随包文件由 `PORTABLE-MANIFEST.json`
逐项记录 SHA-256；没有指向作者目录的符号链接。资产来源、许可依据与公开授权保存在作者侧
精确资产台账，正式发布时另随 Release 提供。本核心包不含两卷书或客户产品放行文件。

员工持续参与项目先看[制造协作入口](MANUFACTURING-COLLABORATION.md)，以下安装由部署者处理。
入口 `0.1.0` 连接讨论、私有项目本体和检查，让方案随现场反馈不断修订；按需提供操作与交接指导。
独立模型质量、双模型对照及实际总成本评测留待后续项目实践，不作为本候选分发的前置条件，
也不被标记为已通过。

## 内容

- [默认情景路由](../references/context-routing.md)、[预写 instruct](../references/context-routing-instructions.json)、能力说明及调用代码；
- [行业本体治理](../skills/domain-ontology-loop/SKILL.md)、[新书编写](../skills/standard-to-book/SKILL.md)及配套合同；
- 根 [skill](../SKILL.md)、[制造工艺与成本模块](../skills/manufacturing-process-cost/SKILL.md)及其合成模板、报告组件；
- [CAD 模块](../skills/cad-agent/SKILL.md)、[证据驱动的重建方法](../skills/cad-agent/references/evidence-driven-reconstruction.md)、[第一性原理推导记录合同](../skills/cad-agent/contracts/analysis-record.v1.schema.json)、[通用 CAD 证据合同](../skills/cad-agent/contracts/cad-evidence.v1.schema.json)、[工艺交接合同](../skills/cad-agent/contracts/cad-process-handoff.v1.schema.json)、Fusion 受保护调用、[可配置远程桥](../skills/cad-agent/remote/README.md)、[装配与力学通用内核](../skills/cad-agent/references/assembly-and-physics-kernel.md)及有界装配/机构工具；
- 锁定的 Semantica wheel、合成制造案例传输包、执行脚本和验证工具；
- [隐私与权利政策](PRIVACY-AND-RIGHTS.md)、[组件权利说明](COMPONENT-NOTICE.md)及文件清单。

两卷书、未清权图/PDF、CAD 历史装配案例、工作站配置、真实客户项目及会话没有打包。Fusion、NX、AutoCAD 软件由接收方提供；NX 外部 sidecar 与真实主机配置也不随包提供。语义 wheel 的上游 MIT 许可证和内部章节包 NOTICE、制造合成包的独立 NOTICE、Fusion 适配器的所有者公开授权见[组件说明](COMPONENT-NOTICE.md)；公开打包不改变候选语义包的工程状态，也不替接收方取得外部软件许可。

## 接收方验证

在新目录中运行：

```bash
python3 scripts/package_skill.py
python3 scripts/check_semantica_backend_policy.py --mode strict
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
python3 scripts/run_manufacturing_cases.py --list
bash skills/cad-agent/setup.sh
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/verify_fusion_runtime_wheel.py --installed
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/test_cad_evidence.py
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/test_cad_process_handoff.py
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/test_remote_assembly_mechanism.py
bash skills/cad-agent/doctor.sh
```

第一次安装依赖 Python 3.11+ 和可访问的 Python 包源。CAD 交接与 Semantica 回放不需要 Fusion 在线；调用 Fusion 前须另行安装并开启其本机 MCP、Peekaboo，且通过 `doctor.sh --fusion` 预检。`doctor.sh --fusion` 不向 Fusion 发送业务请求。NX/AutoCAD 远程桥需要接收方自己的 SSH 配置、CAD 软件及 sidecar/COM 依赖；离线协议测试不等于远程主机验收。

合成制造案例可用 `runtime/.venv/bin/python scripts/run_manufacturing_cases.py --run --output ../manufacturing-replay` 重放。输出须放在包外的新目录。回放通过只证明冻结合成输入和 oracle，不证明客户模型、装配/机构、具体接头或制造方案合格。

包内 `scripts/package_skill.py` 会在解包后核对清单、文件字节、链接、隐私直接标识和冻结资产；新增、缺失或变更均使检查失败。作者侧使用 `scripts/build_shareable_core.py` 依据精确 SHA 白名单重新生成后继 ZIP，不从接收方修改过的包重打。
