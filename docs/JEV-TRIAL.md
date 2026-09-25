# 带实验密钥的完整体验包

本页保留 v0.5.8 已发布体验包及后续明确授权包装的规则。v0.5.9 普通分发包不附
凭据，默认路由及自有凭据配置见[使用说明](USAGE.md)。旧体验包的声明绑定其原
`PORTABLE-MANIFEST.json`，不要将它直接拼到新的 skill 目录冒充新版本体验包。

仓库所有者已明确授权，将自己的临时实验密钥随体验包公开提供，方便接收者直接试用 Jev。
它是共享试用凭据，可能被撤销；撤销后换用自己的凭据即可，项目本体和既有记录仍留在本地。

解压后保留这些文件的相对位置：

```text
解压目录/
  api-jev.md
  JEV-TRIAL-MANIFEST.json
  TRY-JEV.md
  ontology-engineering/
    SKILL.md
    docs/
    runtime/
    ...
```

先在 `ontology-engineering/` 内按 `docs/PORTABLE-DISTRIBUTION.md` 安装运行时，再在助手中
加载根 skill。v0.5.9 根入口默认准备当前情景并调用 Jev 路由；资料批量判断另按需使用。
员工不必申请密钥或复制密钥内容。初始化和图纸/CAD 工具的安装由项目维护者处理。

凭据选用顺序：明确提供的 `--credential-file`，其次是操作者原有的本地
`~/.codex/api-jev.md`，最后是本体验包附带的 `api-jev.md`。使用自有密钥失败时不会
悄悄换成共享密钥；既有错误按原配置处理。读取体验密钥前核对配套声明和文件摘要，
并将该文件权限收紧到 `0600`。不要只复制 `ontology-engineering/` 而丢掉旁边的试用文件。

要更换密钥，把新凭据保存为仅本人可读写的文件，调用时传入 `--credential-file` 即可。
自己的密钥申请说明见 `https://docs.typesafe.ai/`；共享密钥失效时，助手应说明 Jev 当前
不可用，继续不依赖 Jev 的工程整理和语义审查，不把服务失败说成资料没有问题。

Jev 仍只提供候选初判。读取图片、图纸与 CAD 需要相应工具，工程结论仍需来源和适用检查。
试用密钥不改变项目资料向外部服务传输的既有授权范围。客户项目本体、对话、真实参数与
报价保存在各自私有工作区，不能随这个通用体验包分享。

维护者从已校验的普通完整包生成体验包：

```bash
python3 scripts/package_jev_trial.py \
  --skill-archive /path/to/full-package.zip \
  --credential-file /path/to/authorized-public-trial-key.md \
  --output /path/to/new-full-trial-package.zip
```

普通包和源码保持不含凭据；体验包装仅新增外层试用文件，原 skill 内的文件及清单逐字节
保留。公开密钥这一例外只适用于明确授权的体验包，不放宽源码或客户资料的隐私检查。
