# CAD Agent core

本目录包含 CAD／工艺交接合同、完整性转换器及 Fusion 一次性受保护调用。它随 `ontology-engineering/` 根目录一同分发；不依赖作者工作站、旧 CAD checkout 或历史案例。

安装：`bash setup.sh`。离线几何交接测试：`.venv/bin/python scripts/test_cad_process_handoff.py`。本机 Fusion 预检：`bash doctor.sh --fusion`。使用前阅读 [SKILL.md](SKILL.md)。
