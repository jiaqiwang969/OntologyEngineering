# 当前六模块的依赖快照

这里仅含依赖版本和发行包完整性记录，不含运行环境、账号或私有案例。REA 与 SmokingGun 的 npm lock 来自实际固定版本安装；两个 Python requirements 与当前已装发行元数据逐项一致。快照 SHA 记录在上一级 tools.lock.json。

为复现这些依赖，先把 `rea/` 或 `smokinggun/` 的 package.json 与 package-lock.json 复制到用户新建的私有工具目录，再在该目录执行 `npm ci`。调用该目录的实际程序入口；不要在本仓库公共目录里留下 node_modules。REA 的 Ghidra/JDK 另按 bindings/WORKFLOW.md 配置。

Jacobian 与 Flameox 分别在独立 Python 3.12 环境中执行 `python -m pip install -r /absolute/对应-requirements.txt`。原环境为 macOS arm64、Python 3.12.13；这些包含原来安装的可选数学后端，不承诺每个平台都有同版本二进制轮子。需要其他平台时按上游支持重新解析并记录差异；不可把不同解析结果写成相同环境。两个 MCP 固定版本不同，不合并环境。

macOS arm64 上锁定 Z3 wheel 的平台元数据问题与保守修正见 [Z3-MACOS.md](Z3-MACOS.md)。新用户必须保留修正来源并运行 pip check 和实际求解，不能忽略原始失败。

GitContribute 与 LeanToken 当前使用固定官方原生发行二进制；这两项按 tools.lock.json 的版本和平台安装，不需要复制维护者的二进制或 Go/Rust 构建缓存。依赖快照不代替新机功能检查或宿主实际连接。
