# 第 2 章：概念与术语

正文见 [chapter.md](chapter.md)。本章保留对象类别、故障—错误—失效、时间量与 ASIL 语义的可读论述、术语、方法和
合成教学案例；机器可执行语义不在本目录。

## 唯一执行绑定

- package id：`semantica.chapter_packages.vol2.ch02`
- primary scenario：`semantica.vol2.ch02.scenario.primary`
- manifest 状态：`partial`
- release 状态：`blocked`
- 唯一资产位置：Semantica package registry

本体、CQ 注册表、SPARQL、SHACL、正反案例、工程规则、exact oracle、manifest、
版本、PROV、receipt 与 release verdict 全部由上述包持有。本章不存在本地
`examples/`、`ontology/`、`eval/`、fixture、runner 或 fallback；旧路径只在
Semantica migration ledger 中作为哈希来源保留。

```bash
: "${SEMANTICA_RUNTIME_COMMIT:?set from the reviewed Semantica source lock}"
: "${SEMANTICA_WHEEL_SHA256:?set from the reviewed Semantica wheel lock}"
semantica package show semantica.chapter_packages.vol2.ch02 --json
semantica package run semantica.chapter_packages.vol2.ch02 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_WHEEL_SHA256" \
  --json
```

运行时必须使用项目 source lock 绑定的 Semantica commit 和 wheel SHA-256。报告时将
书中依据、scenario oracle 与独立 release verdict 分开；不得把 `partial`、
`blocked`、未运行检查或 unsupported 能力改写成通过。规范术语必须回到合法持有的标准原文；工程释义不是官方定义。

ISO 术语或模态需要机器核对时，只能查询
`semantica.chapter_packages.vol2.normative` 中**已登记**的范围；该包目前只承诺
manifest 声明的部分工程释义/教学映射。未登记的分册、条文或表必须报告
unsupported/blocked 并回到合法持有的原文，不得补造；该包也不是官方解释、
合规意见或认证证据。
