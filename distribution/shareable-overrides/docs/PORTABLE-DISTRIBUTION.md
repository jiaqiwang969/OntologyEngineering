# 独立核心包的安装与边界

`manufacturing-v0.5.6` 是继公开制造方法 `0.5.5` 后，由仓库所有者批准发布的工程本体论独立核心包。ZIP 解压后得到单一 `ontology-engineering/` 根目录。所有随包文件由 `PORTABLE-MANIFEST.json` 逐项记录 SHA-256；没有指向作者目录的符号链接。公开资产的来源、许可依据和复核状态另见随 GitHub Release 发布的精确资产台账。本包不是公开书包或客户产品放行文件。

## 内容

- 根 [skill](../SKILL.md)、[制造工艺与成本模块](../skills/manufacturing-process-cost/SKILL.md)及其合成模板、报告组件；
- [CAD 模块](../skills/cad-agent/SKILL.md)、[通用 CAD 证据合同](../skills/cad-agent/contracts/cad-evidence.v1.schema.json)、[工艺交接合同](../skills/cad-agent/contracts/cad-process-handoff.v1.schema.json)、Fusion 受保护调用、[可配置远程桥](../skills/cad-agent/remote/README.md)、[装配与力学通用内核](../skills/cad-agent/references/assembly-and-physics-kernel.md)及有界装配/机构工具；
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
