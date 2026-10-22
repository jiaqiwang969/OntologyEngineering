每次 CLI 执行仍先核当前编译源码、engineering 与 semantic 生成合同，以及实际执行文件；binding JSON 的原含义和查询入口保持。

执行记录路径改用 `var/maintenance/executions/{role}/{pid}-{UnixNanos}-{attempt}`。PID 及时间只是名字候选；正式唯一性来自原私有路径守护和独占 mkdir。最多尝试 32 个名字，只重试实际 `AlreadyExists`；非碰撞、链接、路径和权限错误继续拒绝。时间不能取得时具体失败。时间回退或精度有限不会授予复用旧记录的权限。

生产入口不读取、复用、删除或覆盖旧 binding，包括以前的 PID-only 目录。写入仍使用原 `write_new`。完成目录创建后的失败可以留下新空目录，不把它当成功查询或复用资格。

原独立 fork 闸门复现的 COUNT 输入及当前编译版本身份保持。新目录分配的同 PID 历史、相同时间碰撞、有限重试耗尽、路径和链接反例，与当前真实 CLI 的源码/两合同漂移拒绝分别验收。生成区仍由锁定真实 ggen 更新。此修复不改变查询、数值要求或版本合同，不证明外部依赖、Windows/native SDK 或完整迁移资格。
