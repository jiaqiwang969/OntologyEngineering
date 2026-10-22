# 公开的 Agent 与 memory 闭环案例

本例使用 [source.claude.jsonl](source.claude.jsonl) 中明确标注的合成可见消息。原文包含目标、错误声称和纠正；没有设备采样、CAD 执行或其他外部工具结果。先阅读 [AGENT-REVIEW.md](AGENT-REVIEW.md) 的整理依据。运行程序只验证真实安装中的来源追溯与回交流程，不自动替代 Agent 的语义审阅。

在已安装的仓库根目录运行，`--root` 指向该安装根；输出目录必须是尚不存在的 `var/examples/` 子目录：

```sh
rustup run 1.95.0 cargo run --locked -j2 -p engineering-memory-runtime \
  --example agent_memory_loop -- --root "$PWD" \
  --output var/examples/agent-memory-review --prepare-only
```

这一步使用原生来源读取器生成实际字节范围、SHA256、`engineering-memory.experience/2`、Project 与 session 请求。没有调用安装程序，也没有产生工具通过记录。可在输出目录检查原文、完整时间线、纠正关系、条件和未知项。

审阅后，用新的输出目录执行完整回放：

```sh
rustup run 1.95.0 cargo run --locked -j2 -p engineering-memory-runtime \
  --example agent_memory_loop -- --root "$PWD" \
  --output var/examples/agent-memory-run
```

[agent_memory_loop.rs](agent_memory_loop.rs) 是 Rust 示例编排。它校验 `var/install/INSTALLATION.json` 中的安装根和 `oev2` 摘要，然后逐条运行安装中的真实 CLI，保留每步参数、退出码、stdout 和 stderr。失败立即停止，已形成的记录留在该次输出目录；不会删除或覆盖旧记录，也不会调用旧 skill 或 PATH 中另一个程序。

实际命令顺序如下。路径和上下文摘要来自本次生成的文件及真实回执，由示例程序填写：

```text
oev2 project validate --input project-r1.json
oev2 memory --home memory evidence --provider claude --profile public-example --source source.claude.jsonl
oev2 memory --home memory record --input experience.json
oev2 memory --home memory session prepare --input session-first.json
oev2 memory --home memory session activate --id first-context --context-sha256 <本次摘要>
oev2 memory --home memory session handoff --context-id first-context --context-sha256 <本次摘要> --id sampling-boundary --revision 1
oev2 project memory-candidate verify --project project-r1.json --input handoff.json
oev2 project memory-candidate register --project project-r1.json --input handoff.json --revision r2 --output project-r2.json
oev2 project memory-feedback --project project-r2.json --input handoff.json --id sampling-feedback --outcome no_delta --result <实际使用说明> --unknown <未核范围> --output feedback.json
oev2 memory --home memory session feedback --input feedback.json
oev2 memory --home memory session prepare --input session-second.json
oev2 memory --home memory session activate --id second-context --context-sha256 <新的摘要>
oev2 memory --home memory session verify --id second-context
oev2 memory --home memory session handoff --context-id second-context --context-sha256 <新的摘要> --id sampling-boundary --revision 1
```

输出 `RESULT.json` 仅在全部实际调用成功并且独立核对满足下列条件时形成：候选被登记为 Project r2 的来源、决策列表仍为空、新会话保留完整时间线/纠正/未知、第二次回交包含实际提交的反馈。`no_delta` 表示本次未证明收益，不代表 RRSI 改进。这个案例不宣称客户端生命周期 hook 已被真实会话触发，也不替代另行执行的原生 `v2-prepare` / `v2-verify` 接收回归。
