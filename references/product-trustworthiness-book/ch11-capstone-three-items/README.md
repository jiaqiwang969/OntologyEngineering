# 历史总装案例：三相关项冻结包

本目录保存一段历史书稿，用来解释候选包身份、哈希漂移、冻结与只读收口的区别。
它不是现行第 11 章，也不再拥有可执行脚本、manifest 或本体副本；正式第 11 章是
[../ch11-claim-ontology/chapter.md](../ch11-claim-ontology/chapter.md)。

历史 `bundle-manifest.yaml`、`capstone.py` 与 `freeze_bundle.py` 的权威迁移后继
位于 `semantica.chapter_packages.vol2.ch20`，资产分别登记为
`legacy-capstone-bundle-manifest`、`legacy-capstone-boundary-rules`、
`legacy-capstone-queries` 与 `legacy-capstone-missing-inputs`。本目录不得重建或
运行这些旧文件，也没有 fallback。

要复核迁移事实和当前边界，从 ontology-engineering skill 根调用统一入口。它会核验
source lock 并自动注入 runtime commit、版本与 wheel SHA-256；调用者不得手抄身份。
先 `discover`，再按
[`semantic-engagement-contract.md`](../../semantic-engagement-contract.md) 建立绑定：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
runtime/.venv/bin/python scripts/semantic_engagement.py run \
  --binding /path/to/package-binding.json \
  --task /path/to/task-envelope.json \
  --scenario semantica.vol2.ch20.scenario.primary
```

当前 ch20 manifest 为 `partial`、release `blocked`。历史叙事中出现的旧路径、
RDFLib/pySHACL 版本和旧 runner 输出只是在解释一次过去的冻结对象；它们不是今天
可调用的后端，也不能作为当前发布证据。原生 `semantica package ...` 只供底层
runner/manifest 诊断，不是主运行路径。
