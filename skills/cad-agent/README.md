# CAD Agent core

本目录包含通用 CAD 证据合同、CAD／工艺交接合同、完整性转换器及 Fusion 一次性受保护调用。可记录零件、图纸/BOM、装配和机构证据，再把与制造有关的对象交接给工艺模块。它随 `ontology-engineering/` 根目录一同分发；不依赖作者工作站、旧 CAD checkout 或历史案例。

安装：`bash setup.sh`。离线通用证据与工艺交接测试：`.venv/bin/python scripts/test_cad_evidence.py`、`.venv/bin/python scripts/test_cad_process_handoff.py`。本机 Fusion 预检：`bash doctor.sh --fusion`。使用前阅读 [SKILL.md](SKILL.md)。
