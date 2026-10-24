# 电子设计专业模块

唯一电子入口为 [SKILL.md](SKILL.md)。本模块保留 electronics-design 2.1.4 的五个内部 GUIDE，与统一工程本体和内置 memory 使用同一 Project；不把五个内部目录重新安装成五个同级 Skill。

| 内部分工 | 指南与资源 |
|---|---|
| 原理图/PCB typed 操作、连接、检查与工作阶段 | [easyeda-agent](modules/easyeda-agent/GUIDE.md) |
| 按需查官方 API、文档格式及已授权 debug | [easyeda-api](modules/easyeda-api/GUIDE.md) |
| 器件引脚、真实短线与网络端口扇出 | [easyeda-schematic-net-fanout](modules/easyeda-schematic-net-fanout/GUIDE.md) |
| 器件 OBJ/STEP 来源、派生与对齐证据 | [lceda-jlc-3d-model-extraction](modules/lceda-jlc-3d-model-extraction/GUIDE.md) |
| 实物证据、连接矩阵与工人接线卡 | [make-worker-wiring-card](modules/make-worker-wiring-card/GUIDE.md) |

安装步骤和版本摘要见 [INSTALL.md](INSTALL.md)，上游与自有内容分界见 [NOTICE.md](NOTICE.md) 和 [SOURCES.json](SOURCES.json)。外部程序由用户独立安装；本模块不依赖旧 Skill 目录、开发者脚本、私人资料或历史通过回执。它没有新增 PCB Rust 语义适配器；结果沿现有 [Project 接续](references/v2-handoff.md) 和 [专业能力合同](references/capability-integration.md) 返回。

保留的运行修复及源码/测试对应见 [runtime-differences](references/runtime-differences.md)。
纯上游0.25.1程序不等于本地修复版；按安装说明从固定上游加补丁构建，不能沿用旧原生回执。

## 最小离线验收

在统一仓库安装后，使用未存在的私有输出目录：

```sh
export ELECTRONICS_MODULE_ROOT="$ONTOLOGY_ENGINEERING_ROOT/modules/electronics-design"
/bin/sh "$ELECTRONICS_MODULE_ROOT/scripts/check-offline.sh" \
  "$ONTOLOGY_ENGINEERING_ROOT/var/tests/electronics-offline-01"
```

需要 Python 3、Node.js 和 shell。该命令实际运行上游方向/布局正反例及 diff 回归、3D 文件包合成正例和损坏/缺模型反例，并检查 API 桥脚本语法。它不启动桥、daemon、EDA、浏览器、模型下载或板级操作。`RESULT.txt` 仅在全部预期通过时写出；每项 stdout/stderr 保留。跨语言 connector 源码不在本模块内，上游测试明确跳过该项，不计为通过。

API 和 fanout 的本次接入验收限于固定资源/链接/差异与离线语法；实际连接和扇出仍需真实工程上下文。3D 文件摘要/三角形检查不证明 CAD 格式完整、模型来源权利或封装对齐。接线模板是未填实物映射的版式起点，不能用于焊接或上电。原生设计、仿真、保存重开、制造和实测各自需要新任务中的实际证据。

本次离线输出可以由 Agent 按 [ToolResult](../../references/professional-capabilities.md) 登记到实际能力维护 Project；先冻结对象/条件，再执行，不能把本页或旧回执直接当成新产品证据。完整通用结果—决定—memory 回流见 [FIRST-PROJECT](../../docs/FIRST-PROJECT.md)。
