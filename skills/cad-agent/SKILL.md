---
name: cad-agent
description: Validate native CAD evidence and hand it to manufacturing process decisions; use guarded local Fusion execution when available.
---

# CAD Agent：几何证据与工艺交接

本模块是工程本体论的 CAD 执行模块。先识别零件、特征、装配、界面、版本、产品状态和原生来源，再决定需要何种回读、可达性核查或制造问题。CAD 工具报告的是几何与文档状态；正式 TBox/ABox、CQ、SHACL、规则和项目语义审查由父级 [工程本体论](../../SKILL.md)的 Semantica 控制。

当工艺路线依赖孔、筋、间隙、接触面、流道、焊缝或夹具时，使用 [CAD／工艺双向交接](references/cad-process-integration.md) 和 `scripts/cad_process_handoff.py`。来源均使用项目根目录内的相对路径及 SHA-256；原生回读特征指向实际 JSON Pointer。分别写明要求的功能、候选工序及覆盖机制、验证来源、尚待回答的 CAD→工艺和工艺→CAD 问题。几何接触不能直接升级成连接或密封结论，局部焊点也不能自动代表整条必要密封边界。

## Fusion 本机执行

先运行 `setup.sh`，再阅读 [Fusion 安全调用](references/fusion-execution.md)。所有 Fusion MCP 调用经 `scripts/fusion_call.py` 一次性守卫，不直连端口，不对歧义超时盲目重试。`scripts/fusion_ops.py` 是结果分类的共同入口。调用前运行 `doctor.sh --fusion`；它只预检，不发业务请求。

```bash
python3 scripts/fusion_call.py fusion_mcp_read '{"queryType":"projects"}'
```

一次调用返回 `classification`；进程退出 0 只表示已送达，`delivered_ok` 才是可继续的执行结果。原生保存、重新打开及来源摘要要记录在项目证据里。没有 Fusion 或本机端点时，仍可用来自 STEP/图纸/其他 CAD 的已核验回读 JSON 完成工艺交接；不能声称本模块亲自完成原生回读。

NX/AutoCAD 远程桥、历史装配影片工具、案例经验库和具体工作站配置不在这个可分发核心版中。接收方如需这些能力，应在自己的受控环境中按同一来源、身份、回读和工艺交接合同接入。
