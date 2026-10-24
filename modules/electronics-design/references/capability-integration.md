# 电子设计专业模块与同一 Project 的接续

电子设计与原六工具共享统一系统的 Project、context、ToolResult、决定和内置 memory。
本文件描述接续方法，并说明电子原生观测如何通过专用 Rust 外层桥进入 Project；它不把
通用 ToolResult 扩展成 PCB 解析器，也不把外部工具结果自动变成工程采用。
唯一电子入口为本模块的 [SKILL](../SKILL.md)，五个内部 GUIDE 按缺口展开。

## 实际安装与结果合同

以 `$ONTOLOGY_ENGINEERING_ROOT/var/install/INSTALLATION.json` 核根及 `oev2` 字节；
外部程序按 [INSTALL](../INSTALL.md) 自行安装。不要调用历史目录的薄适配器、旧 CLI 或旧回执。
专业结果遵循根 [结果合同](../../../references/professional-capabilities.md)；完整通用操作案例见
[FIRST-PROJECT](../../../docs/FIRST-PROJECT.md)。数学案例只说明交接方式，不是电子设计通过。

1. 保留同一 project_id、对象修订、条件、验收项、完整父目标及未决问题；调用前明确待决定事项。
2. 用 `project validate` / `project context` 核合同并冻结实际输入。给物理对象、网表、
   原理图/PCB/模型与工具程序分别登记身份。独立项目显式使用 `--project-home`，原件与结果
   存在该 ProjectStore，摘要和业务相对路径指回实际字节；未选择项目根时沿用安装的私有 `var/`。
3. 执行当前已授权的原生工具，记录实际 argv、版本、时间、退出码、stdout/stderr、
   原生文件、测量、失败和未执行项。后台约束与服务归属沿当前用户要求，不能从指南继承授权。
4. 用独立设计意图、厂商引脚来源、解析预期或独立实验核对结果；同一原生数据经两种显示
   通道输出并不成为两份独立证据。工具成功、连接成功、DRC 通过和电路功能分别判断。
5. 先将外部工具结果整理为严格的 `oev2.electronics-native-observation/1`，运行
   `oev2 project electronics project --request ... --source ... --aux role=path ...`。
   该入口只重读来源、核 SHA、Project context、操作类型和辅助原件，保留 source-observation、
   projection、aux 副本及 receipt；它不启动 EasyEDA，也不执行 DRC、仿真、模型导入或接线卡渲染。
   随后准备 `oev2.tool-result/1`，经 `project tool-result verify/register` 登记，再用
   `project decide` 更新有依据的有限结论。登记不改变结论，decide 不授予 adopted。
   条件、对象或来源变化后用当前 reconcile 合同定位受影响决定，必要时重新专业检查和内核审查。
6. 交付前按路线锚定 /2 核已登记决定与证据，仅交付被验证的范围；缺电子规则或实测仍未知。
7. 用 `project memory-feedback` / `memory session feedback` 保存实际使用、失败和纠正。
   没有可比收益证据时记录 no_delta；历史经验保持待核候选，不能作为支持决定的唯一依据。

`--root` 和 `ONTOLOGY_ENGINEERING_ROOT` 始终选择软件安装，不用工程目录替代。
本机工具 registry 默认来自安装根；项目内副本通过显式绝对 `--registry` 选择。
memory 的 home 独立选择。原件不在所选项目、项目快照已更新或安装身份改变时拒绝当前接续，
核对后重新准备；不自动把旧路径、旧摘要和旧通过改签为当前结果。

## 各项证据的边界

| 范围 | 所需证据 | 不足以推出 |
|---|---|---|
| connector 连通 | 当前窗口、宿主/连接器/CLI 版本与目标只读响应 | 图纸已建立、所有动作已执行 |
| 器件映射 | 精确 MPN、库 UUID、pin→pad、封装与厂商原件 | 仅搜索命中不能证明电气适用或库存 |
| 原理图连接 | 当前页 pin→net 对照独立设计意图 | 同名标签、截图、放件成功不能证明接对 |
| 保存重开 | 实际保存原件、重开后对象/连接/属性 | 立即回读、save=true 不能单独证明持久化 |
| PCB 实现 | 当前器件关联、pad→net、板层/板框/实际铜线 | 网络标签相符不能证明铜连通 |
| ERC/DRC/布局 | 当前规则来源、完整输出和遗留问题 | blocked、超时、不支持不能算零错误；通过不能证明功能 |
| 制造与接线 | 同一修订的原生输出、BOM/坐标、实物证据和审阅 | 文件完整不表示允许下单、焊接或上电 |
| v2 接收 | 当前绑定、来源、决定及电子投影回执的实际检查 | 外层桥不重算缺失的电子规则，不自动采用 |

本次模块迁移的离线测试只覆盖公开合成输入和辅助程序行为；EDA 原生执行、模型下载、
真实 CAD 对齐、制造、接线及实物性能须另行明确范围并实际验收。不得沿用开发机私人案例通过。
