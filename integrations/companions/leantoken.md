# LeanToken 接续补充

这是 OntologyEngineering 维护的 companion，适配 `leantoken 0.1.28`，不是上游
Skill 的副本。上游运行程序与其 setup 生成的 Skill 保持原版。

1. 从用户配置取得 `ONTOLOGY_ENGINEERING_ROOT`，读取该根的
   `references/professional-capabilities.md`。绑定实际 Project、对象修订和证据缺口。
2. 由用户任务明确选定待检索仓库。CLI 每次传 `--root /absolute/repository`；不要索引
   用户主目录、系统根目录或包含不相关项目的上级目录。
3. 第一次直接 CLI 检索先运行 `leantoken --root REPO --json index`，再用一个有界
   `context --task TEXT --budget 2200`。有具体缺口时再进行针对性 search/read。
   MCP 模式先检查宿主所绑定的工作区，不把启动目录当作已获授权的索引范围。
4. 保存提交/脏文件身份、索引 generation、覆盖、返回范围和实际读取字节。
   `indexed_generation` 不证明返回字节未变；修改后显式重建索引，回读变更范围。
   0.1.28 的 CLI `context/read` 默认是 `reconcile_working_tree`，不要套用其他入口默认值。
5. 原生输出保存到 Project 私有区，通过 `oev2 project tool-result` 接收。
   源码定位不是测试执行；找到测试不能记成测试通过。过程与实际使用通过
   `oev2 memory` 反馈，召回的经验仍需当前条件核对。

CLI 路径由用户安装决定；缺少 MCP 不表示 CLI 不可用。检索失败、部分覆盖或过期索引
需要照实返回，不要求额外的托管判断服务，也不借用另一用户的索引或配置。

版本、上游来源、安装与验收见 [工具接入说明](../../docs/TOOL-INTEGRATIONS.md)。
