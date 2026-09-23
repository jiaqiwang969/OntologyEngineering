# CAD Agent core

本目录包含通用 CAD 证据合同、CAD／工艺交接合同、完整性转换器、Fusion 一次性受保护调用、可配置 NX/AutoCAD 远程桥及装配/机构有界筛查器。装配与力学的核心是[跨产品判断方法](references/assembly-and-physics-kernel.md)：实例身份、界面、逐状态支撑、载荷路径、局部容量和证据边界，不是某个产品的搭建流程或某个仿真算法。它随 `ontology-engineering/` 根目录一同分发；不依赖作者工作站、旧 CAD checkout 或历史案例。

安装：`bash setup.sh`。离线通用证据、工艺交接与公开专项工具测试：`.venv/bin/python scripts/test_cad_evidence.py`、`.venv/bin/python scripts/test_cad_process_handoff.py`、`.venv/bin/python scripts/test_remote_assembly_mechanism.py`。本机 Fusion 预检：`bash doctor.sh --fusion`。远程桥的外部 CAD 依赖与实际验证范围见 [remote/README.md](remote/README.md)。使用前阅读 [SKILL.md](SKILL.md)。
