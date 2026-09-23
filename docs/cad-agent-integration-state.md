# CAD Agent 并入工程本体论：本地实施状态

2026-09-22。此记录区分技能入口、运行进程、语义裁定和产品放行；本地打包通过不等于正式 Semantica release 或客户交付批准。

## 已实现的协同关系

`skills/cad-agent/` 是新的 CAD 模块正本，根 `SKILL.md` 与制造工艺模块双向路由。CAD 模块负责 Fusion／NX／AutoCAD 的原生几何和回读证据、对象身份、界面及装配状态；制造模块负责工艺功能、失效路径、检验、成本和交期的因果审查。`cad_process_handoff.py` 校验来源文件 SHA-256、原生回读 JSON 指针、模型内嵌来源摘要和对象依赖，输出项目 ABox、CAD→工艺／工艺→CAD 问题及变更影响清单。证据投影只写确定性的 N-Triples，不运行查询、SHACL 或规则；正式语义只由根控制面中的 source-locked Semantica 执行。

旧 CAD 本体、SPARQL、SHACL 和雨刮示例验证器共 190 个文件已从新模块的活动源码面移出，原字节保存在私有迁移归档及旧 CAD 目录。新模块的 `semantic_query.py capabilities` 只读取静态工具清单，不执行旧本体查询。原 CAD wheel 确曾内含可执行 SHACL/SPARQL 的 MCP 侧车；新模块现只分发从锁定来源提取的 Fusion 执行 wheel，不含 `cad_agent`、`cad_geometry_mcp`、本体资产和 `cad-agent-mcp` 入口。打包前和安装后均核对 wheel 摘要、成员与入口。CAD 本地工具运行所需的动态加载和子进程调用由 `runtime/cad-operational-source-lock.json` 对 31 个确切文件逐一锁定 SHA-256；改动后门禁要求重审。锁不豁免显式本体引擎调用、第二套语义资产或直接导入语义后端。

真实项目首例及其图纸、参数、人物和试跑结果留在仓外受控目录；本模块只保留通用交接合同、适配器与合成反例。该首例用于核对 CAD 来源、必要功能、候选工艺覆盖和下游决定之间的依赖，不能将作者试跑当作产品结论。

## 已验证的本地边界

- CAD、制造和根技能的结构校验通过；新 CAD `doctor.sh --json` 为 `ready=true`（只读预检，未发送 Fusion MCP 请求）。
- CAD 交接的 6 项测试和工艺投影的 3 项测试通过；包含来源篡改、原生回读漂移、对象错引、影响传播及未验证覆盖不能升级的反例。
- Semantica 唯一后端门禁的严格模式为 0 个剩余发现；另有 31 个按文件哈希锁定的非语义 CAD 执行源和独立 wheel 成员审查。门禁的新增反例证明改动锁定文件、篡改 wheel、重新锁定含语义引擎的 wheel 或显式调用本体引擎均会失败。
- `scripts/package_skill.py` 对整根目录的文件闭合、直接个人路径和传输锁检查通过；独立解包复验另记在分发记录中。

## 尚未切换的事实

旧顶层 `cad-agent` 已改名为 `cad-agent-legacy` 技能入口以免与新模块重复命名。旧运行目录暂留给已有会话；路径切换应在无活跃调用时重新检查，并做旧命令与新入口的只读对照。

制造因果关口的 Semantica 后继仍是候选。没有项目已采用的 `ProjectOntologyBinding` 与 fresh task 时，只能形成来源完整性投影和候选作者试跑，不能签发正式项目 review receipt、release 或 promotion。真实产品的物理闭合另需同版结构、代表性截面及适用的检漏、承压等原生证据。
