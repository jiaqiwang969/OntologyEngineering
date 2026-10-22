# 旧语义职责的实际替代范围

状态：重构中。现行入口都是本根 oev2，旧 wheel/包/合同作为历史数据。下表中的静态调用库存与行为验收不同；没有恢复 Python 备用执行。细节见 [语义 profile](SEMANTICS.md)。

| 旧职责与资产 | 当前 Rust 执行/入口 | 当前证据与未闭合范围 |
|---|---|---|
| RDF 读入、查询结果身份 | oe-kernel::graph；kernel query、books migration-review | RDF 1.1 项目查询；章级 XML/逐文档合并实测。通用 CLI 仍读 Turtle，未开放任意格式/远程图 |
| SPARQL、CQ oracle | 固定 Oxigraph 与本地 spargebra/spareval 补丁 | 三冻结包 452 场景、29 章原资产；20 项二卷查询互操作另列。初始绑定与 MINUS 修复按原 oracle 单独验收；十项 raw exact 差异保留。没有联邦/更新/查询总截止机制 |
| SHACL 范围、字段比较、日期和前缀 | oe-kernel::shacl profile/2 | 七项独立迁移测试含原 XML/质量场景；类型错误、超大整数、边界、缺失/未支持与冲突前缀保留。完整 SHACL 不由此推出 |
| 规则推导与来源 | oe-kernel::rules、kernel rules | 有界正 Horn 的原例与推导关系；DL/否定/任意多变量不支持 |
| 项目 evidence/review、Harness | oe-semantic::review 投影 + oe-kernel::forward/port | 当前 Rust 回执、新执行版本/摘要和目标映射；缺条件仍 unknown。直接 review 不独立读取当前 Project 原件，跨入口条件版本自动绑定还需补齐 |
| 包运输、版本与冻结 oracle | oe-semantic 的 Rust Bundle/manifest/运输检查 | 三包原 zip/oracle 原字节不动；旧回执不改签。历史 package gate 不代替项目采用 |
| 两卷章源、规范资产、章节读取 | books migration-map/historical-asset/chapter-asset-text/search/verify-sources | 校验来源与 29 章迁移入口实际执行；两卷不就绪报告、领域端到端和 agent 操作 oracle 保持未知 |
| 书源 generic asset-text/resolve-successor/engrave | books::execute → books::rust::execute；通用资产与后继解析使用 Rust Catalog，engrave 使用受控作者源 | 当前入口已走 Rust；历史 NativeRuntime 身份不能替代当前 Rust 来源证明。原完整 API、失败条件、作者产物及工程采用仍须分别验收 |
| registry、生命周期、书源与发布治理 | oe-kernel::governance 局部本地策略；旧 oe-semantic 编排保存合同 | 本地 guard/追加事件与原包重跑有验收；完整 registry、冲突治理、组织权限、公开发布及旧 lifecycle 接口未闭合 |
| 经验候选与方法更新 | oe-kernel::learning + Project/Harness/knowledge | 真实聊天方法接续、冻结/评估/拒绝/试用/恢复实际执行；只支持两种操作计划，不是任意代码或整版本自进化 |

图读入、原 oracle 比较、互操作比较、约束执行、项目审查和工程采用分别报告。不支持表达和未就绪 oracle 不删除；原数据、当前表示和适配规则保留各自身份。用户纠正可以恢复需求，不能充当机理证明；旧认证/标准条款转述不能变成当前产品认证。

本轮维护输入、首次红测、原预期、新 ggen 和新编译文件保存在私有维护记录中。普通 Rust tests 的通过数量不替代完整旧接口或原生平台验收。
