# OntologyEngineering 0.6.0 — 独立核心包

从根 [工程本体 skill](SKILL.md) 描述当前问题、已有材料和本轮目标即可。
它组织 CAD、制造、证据与语义协作；Jev 提供候选选择，主 agent 复核并接续执行。

- CAD 默认 Siemens NX，经 NXOpen/Journal 直接执行；标准件优先米思米中国。
- 按工程用途选择模型、二维图纸、规格或采购资料，分别验证文件、原生几何与适用性。
- 成功、失败和用户纠正进入有证据的经验复盘，再验证下一任务是否采用相关指导。
- Semantica 是唯一正式语义后端。Fusion 执行器已退役；没有自动启用夜间常驻任务。

参见 [使用说明](docs/USAGE.md)、[发布说明](docs/releases/manufacturing-method-0.6.0.md)、
[安装与复验](docs/PORTABLE-DISTRIBUTION.md)、[CAD 入口](skills/cad-agent/SKILL.md)
和 [经验整理](references/practice-consolidation.md)。

This portable core includes source-locked semantics, direct NX orchestration, MISUMI
resource acquisition, Jev browser tooling and evidence-bounded practice consolidation.
CAD licenses/hosts, browser connections and credentials are supplied by the recipient.
Downloaded bytes do not prove geometry or mechanical fitness. Independent model accuracy,
unattended reauthentication and ten-product cold-restart reliability are not established.

核心包不含两卷书、客户资料、浏览器账号或私有历史回执。已有书源可在
[公开仓库](https://github.com/jiaqiwang969/OntologyEngineering) 查阅；权利及组件边界见
[组件声明](docs/COMPONENT-NOTICE.md)。普通包不包含 Jev 凭据。
