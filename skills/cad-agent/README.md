# CAD Agent core

本目录包含通用 CAD 证据合同、CAD／工艺交接合同、完整性转换器、Fusion 一次性受保护调用、可配置 NX/AutoCAD 远程桥及装配/机构有界筛查器。[证据驱动的重建方法](references/evidence-driven-reconstruction.md)用于从图片、视频、专利等资料提出形态和机构候选，并用有来源的物理/机构/数学推导收窄缺口；[装配与力学方法](references/assembly-and-physics-kernel.md)区分实例、界面、逐状态支撑、载荷路径、局部容量和证据边界。它们随 `ontology-engineering/` 根目录一同分发；不依赖作者工作站、旧 CAD checkout 或历史案例。

安装：`bash setup.sh`。离线通用证据、工艺交接与公开专项工具测试：`.venv/bin/python scripts/test_cad_evidence.py`、`.venv/bin/python scripts/test_cad_process_handoff.py`、`.venv/bin/python scripts/test_remote_assembly_mechanism.py`。本机 Fusion 预检：`bash doctor.sh --fusion`。远程桥的外部 CAD 依赖与实际验证范围见 [remote/README.md](remote/README.md)。使用前阅读 [SKILL.md](SKILL.md)。
