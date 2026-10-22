# SolidWorks 原生 Rust worker

`oe-sw-worker` 是 Windows STA COM 的有限执行器，和 NX 并列，复用 `oe-windows-controller` 的文件句柄、文件身份及进程观察。共享请求/观测由 `specs/engineering.toml` 经真实 ggen 生成。Project 和 work 引用仍属于同一工程链，没有第二份工程账本。

`oe-sw-worker run REQUEST.json NEW_DIRECTORY` 使用显式安装文件摘要，先以 `LoadTypeLibEx(REGKIND_NONE)` 核选定 SW2025 的接口、DISPID、参数方向、返回类型及枚举值。`api_check` 只核类型库，不激活 CAD。模型操作包括有限单零件只读 inspect、矩形带可选圆孔的拉伸，以及明确命名特征的厚度改型；单位为 m，体积为 m³。尺寸回读、全实体库存、质量属性和六向极值分别保存，体积一致不证明完整拓扑或工程适用性。

原 `solidworks-native-request/1` 模型动作继续拒绝任何已存在的 SLDWORKS 进程。另版 `/2` 必须显式给 `execution_profile="owned_private_desktop"`：在非服务会话持有全局单写者，观察且不控制外部实例；创建非输入私有桌面中的直接子进程，保留原生进程句柄核 PID、OS 创建时间、映像及会话。仅用 CreateProcess 返回的原主线程做有限桌面就绪等待，并同时核退出和 deadline；正面桌面名、该子进程窗口仅在私有桌面、输入桌面与前台观察通过后，才仅取该 PID 的精确 ROT 名称，再由 GetProcessID 核同一进程。不能从睡眠或另选线程推断隔离。无默认实例附着、没有激活窗口或切换桌面；未知或启动转交另一进程均拒绝，无 fallback。

worker 在第一次 CoInitializeEx 前只关联自身主线程到同一任务私有桌面；它不调用 SwitchDesktop。先建立留在原输入桌面的只读观察线程，再记录关联前后的真实 worker/线程/桌面与原输入前台。进程归属、原主线程、全部窗口和外部实例身份检查不因此减少；COM 释放及 STA 退出先于自身线程桌面恢复。这遵循 [COM 缓存调用方 window station/desktop 的官方合同](https://learn.microsoft.com/en-us/windows/win32/com/registering-a-class-at-installation)，不修改系统注册表或 COM 访问策略。

`oe-sw-session-host run REQUEST NEW_NATIVE_OUTPUT NEW_LAUNCH_DIRECTORY` 是 Rust GUI 子系统的会话入口，可由本任务自己的 Windows InteractiveToken 任务启动；它以 CREATE_NO_WINDOW 运行同安装目录中的 worker，保存原请求、精确命令、安装文件、实际 worker PID/创建时间/session、原始 stdout/stderr 与终态。服务会话拒绝、600 秒后的结果 unknown、任务配置与启动回执各自保留，不终止或自动重试未知 worker。GUI/no-window 编译、调度器成功和 API-only 任务均不能替代 CAD 操作成功。

未证明归属不操作 CAD，失败后不自动重试写入、不杀进程。只有绑定及本任务新文档返回都已观察、该 owned 私有进程仍通过隔离且单文档库存明确时，失败清理才关闭该唯一文档、不保存，并只请求该 owned CAD 退出；原失败不改签。未绑定、库存变化或同步 COM 阻塞继续保留原调用与 outcome unknown；外部监护不能将未知执行重试为新动作。

Project/work 引用以请求文件所在目录为基准，核原字节 SHA；这是来源绑定，不证明工程语义和适用性。原 SLDPRT 只读持有；改型先写本次新目录内的副本。结果保存到新文件，记录 native errors/warnings 和原生路径，释放文档并观察同一 CAD 进程退出。独立重开须另一个真正新 CAD 进程读取所保存的精确文件，不能由同进程 Close/Open 或任意 JSON 声明代替。

安装原件和 COM 元数据提供 API 依据；编译、ABI 单元测试、安装类型库核验与实际建模/独立重开分别验收。当前具体执行情况以本次原始回执为准，不继承第三方 Python MCP 的建模成功。装配、制造图、STEP 往返及完整设计能力仍需继续实现和实际验证。工程证据继续沿 `cad evidence/handoff`、同 Project/work、正式审查和经验流程；已暴露案例是 development_replay，不自动成为设计方法改进收益。

使用部署的独立单链接worker副本；Cargo target目录的硬链接会被既有文件owner拒绝，不能放宽guard绕过。MSVC用锁定sha2正式Rust实现；原macOS/Linux依赖及build依赖按cfg(not(windows))继续显式启用asm。

SW 原生 API 的模型/模板路径使用等价的 DOS/UNC 绝对拼写，限定当前已资格的短路径；Windows 文件 owner 继续持有 canonical 原生句柄、完整文件/目录身份及 SHA。OpenDoc 回读须与原只读输入或本任务复制件的真实文件 ID 一致；SaveAs 回读仍核新目录、完整目标文件 ID、原码、关闭后原字节，不能从路径转换推断打开或保存通过。当前实际保存 error1 的原件保留，新拼写必须另实跑。

原只读输入继续由不可写 PinnedFile 保留。厚度改型先新建本任务复制件、核完整原 SHA 并记录 file ID；SW 读写打开复制件后仅用 READ_ATTRIBUTES 与显式读写共享核同一新目录/文件 ID，不声称此时内容不变。关闭后保存产物再核完整身份/全部字节。原 sharing violation 失败保留，不放宽原始输入 owner。

新 producer 增加可选的完整面库存：原 GetFaces 数量须等于 GetFaceCount，记录平面/柱面原参与柱面完整边中的圆曲线参数。在独立板件意图下，孔径、轴向、中心、两端圆边 z=0/完整厚度与全实体体积共同核验；2mm 残底反例仍拒绝。旧记录缺该库存保持通孔未观察，不从面数、D1 或外包围追签孔功能；这仍是有限板件证据，不是通用完整修剪 BRep 或制造/物理采用。

`oev2 cad solidworks verify-native-record --request var/<request> --observation var/<observation>`核请求原件、过程/单位/输出状态。`verify-native-reopen --saved-request ... --saved-observation ... --reopen-request ... --reopen-observation ...`核独立worker与CAD进程、同一保存文件、配置、原生实体/面数/六向极值和体积。JSON一致性不自行证明producer执行身份；原调用日志、可执行文件、现场进程回执及原生文件仍经现有evidence/handoff登记。有限几何比较不代替孔拓扑、制造图、强度/寿命或工程采用。

控件软件试验保留独立预期和失败身份；同一 benchmark Project 新修订的完整 engineering-basis/1 由冻结意图原值形成，purpose 与 Project 精确一致，再经现有 project::context 和 workflow::verify_project_binding 接 work/3。已有 work/1 软件观察不追签为新 work/3 执行；物料、载荷、制造及物理采用仍保持具体 unknown。

多实例的启动和 PID ROT 方法依据 [Dassault SolidPractices 的 standalone 开发说明](https://3dswym.3dexperience.3ds.com/wiki/solidworks-news-info/getting-started-with-the-solidworks-api-solidpractices_eLpSxUm2THqG6jwQdSrMGQ)，不猜未正式核验的启动参数。线程桌面语义见 [GetThreadDesktop](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getthreaddesktop)，会话登录边界见 [Task Scheduler security contexts](https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks)。这些是方法依据；当前安装实际行为必须另取证。
