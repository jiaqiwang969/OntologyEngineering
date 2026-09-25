# 工程本体论：通用 CAD × 工艺独立核心版

**从[制造协作入口](docs/MANUFACTURING-COLLABORATION.md)开始。** 持续接收群里的试制、工艺、
设备、供应商和现场反馈，建立私有项目本体，比较和迭代方案；项目资料与本体不进入通用 skill。

日常调用及凭据准备见[使用说明](docs/USAGE.md)。触发根 skill 后默认通过 Jev 判断本轮所需能力，助手复核后组织执行。

当前版本 **0.5.9** 增加默认 Jev 情景路由、预写工程本体 instruct 和项目方法融合，见[更新说明](docs/releases/manufacturing-method-0.5.9.md)；
Semantica、CAD 适配器和冻结语义包各自保留独立组件版本。制造协作入口为 `0.1.0`。

这是可迁移的工程方法与执行核心，包含 Semantica 单一语义执行入口、制造工艺与成本模块、通用 CAD 证据及其工艺交接，图片/视频/专利到候选结构的重建与第一性原理推导记录，Fusion 受保护调用、可配置 NX/AutoCAD 桥，以及装配与力学的通用判断内核和有界专项筛查器。客户图纸、对话、模型、现场参数、历史 CAD 案例和两卷书不在包内。

从根目录运行：

```bash
python3 scripts/package_skill.py
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash skills/cad-agent/setup.sh
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/test_cad_evidence.py
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/test_cad_process_handoff.py
skills/cad-agent/.venv/bin/python skills/cad-agent/scripts/test_remote_assembly_mechanism.py
```

具体依赖、验证范围和授权边界见 [分发说明](docs/PORTABLE-DISTRIBUTION.md)。制造方法见 [manufacturing-process-cost](skills/manufacturing-process-cost/SKILL.md)，CAD 方法见 [cad-agent](skills/cad-agent/SKILL.md)、[重建方法](skills/cad-agent/references/evidence-driven-reconstruction.md)、[装配与力学内核](skills/cad-agent/references/assembly-and-physics-kernel.md)和[能力边界](docs/cad-agent-capability-map.md)。
