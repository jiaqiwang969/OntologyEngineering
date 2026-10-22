# Flameox 接续补充

这是 OntologyEngineering 维护的 companion，适配 `flameox 0.2.9`。该固定上游
版本没有可供追加的 Skill 文件；这份说明不是虚构的上游 diff，运行程序保持原版。

1. 从用户配置取得 `ONTOLOGY_ENGINEERING_ROOT`，读取该根的
   `references/professional-capabilities.md`，绑定 Project、对象修订和待检验主张。
2. 显式已有原件能回答问题时，先选只读 `analyze`。通过 `flameox mcp inspect --tool NAME`
   核对准确 schema。输入使用绝对路径，保存原件摘要与生产者身份。
3. `capture` 会执行程序。仅执行当前任务授权的工作负载，固定 argv、绝对 cwd、提供方、
   输入、环境和行为判据。分别设置分析预算、单次工作负载预算和整个实验截止时间。
   不把任意 shell 文本当作采集目标，不自动安装可选提供方。
4. 保留原始结果、完整/部分/失败状态、覆盖和未知项。Profile、导入的测量和因果判断
   分开；一个观测不证明改进。对比必须保留可比的输入、环境和行为检查。
5. 需要跨进程保存时，在退出前 preserve，并用同一 `FLAMEOX_DATA_DIR` 重开核验。
   用原样 `analysis_sources` 或 `next_page` 继续只读分析；重放不再次 capture。
6. 回交经过 `oev2 project tool-result`，实际过程与使用反馈经过 `oev2 memory`。
   结果导入成功不授予工程采用，也不要求额外的托管判断服务。
7. 记录超时后的部分结果与清理回执，确认本任务子进程退出。只关闭任务拥有的资源，
   保留原件，不添加常驻服务或自动重启任务。

版本、上游来源、安装与验收见 [工具接入说明](../../docs/TOOL-INTEGRATIONS.md)。
