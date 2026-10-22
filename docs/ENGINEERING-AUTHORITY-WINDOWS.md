# Windows 工程采用权限接续

本增量接通既有 `artifacts::private_policy` 的 Windows 实现。此前非 Unix 平台一律返回 `authority_policy_backend_not_verified`；这项扩展须取得自己的实际 Windows 回执，不能继承 Mac 的采用通过。

政策仍由用户或明确授权的管理过程提供。生产读取复用 `oe-core::private_file` 的原生 owner/DACL、文件及父目录身份、reparse/硬链接拒绝、完整有界读取与再次核验；不会自行创建政策、修改权限或批准主张。当前用户、SYSTEM 和 Administrators 为该既有 Windows owner 的可信主体；未知主体及未支持 ACE 拒绝。严格 JSON、1 MiB 限制和 `var/` 路径边界保持，Unix 原 mode/uid 检查不变。

文件可读只说明政策输入满足本地读取合同。工程采用仍须原对象版本、条件、全部必要检查、来源字节、精确规则归档、不同批准者及当前审查全部满足，写入新的项目副本，原件不改。共享该政策 reader 的版本 registry 等消费者仍须各自的平台锁、事务和实际使用验收。

原测试的临时目录现在先保证私有父目录存在，再按 SkillRoot 的规范路径拼写构造；Windows 普通盘符与扩展盘符不会混用。来源路径函数保持原严格词法边界，原失败、四项采用测试及十项 Windows 复核测试的断言均保留。新增真实权限放宽测试要求先完成有效采用，再证明新采用及旧记录复核都拒绝，不能用一个始终拒绝的后端通过负例。

规范位于 `specs/modules/private-supplier-file.toml`；新旧测试、锁定 ggen、当前构建与双平台接收分别留痕。软件实现不抵扣当前原生 NX、保存独立重开、Project 全来源、制造计量、完整 Semantica 或 RRSI 收益义务。整体继续按当前维护回执判定。
