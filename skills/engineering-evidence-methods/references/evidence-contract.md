# 证据合同与调用

执行下列命令时位于根 ontology-engineering，使用根 `runtime/.venv/bin/python`。完整源代码入口为 `ontology_engineering/method_evidence.py`，字段类型来自锁定 Semantica 包的 `input-profiles`，本模块不再维护一套语义字段正本。

```bash
runtime/.venv/bin/python scripts/method_evidence.py list
runtime/.venv/bin/python scripts/method_evidence.py project \
  --record /controlled/record.json --evidence-root /controlled \
  --output /controlled/new-projection
```

一个记录只声明一个主张、一个方法和一个范围。可以给同一主张建立多项记录。

```json
{
  "schema": "ontology-engineering.method-evidence/v1",
  "record_id": "inspection-record-1",
  "claim": {
    "id": "inspection-executed-1",
    "statement": "The declared checker ran on the requested revision",
    "subject_revision": "revision-A",
    "scope": "inspection-input-A"
  },
  "method": "check-execution-origin",
  "sources": [{
    "id": "execution-record",
    "path": "execution.json",
    "sha256": "<actual source SHA-256>",
    "media_type": "application/json"
  }],
  "facts": {
    "execution_origin": {"source_id": "execution-record", "pointer": "/execution_origin"},
    "execution_status": {"source_id": "execution-record", "pointer": "/execution_status"}
  }
}
```

此例故意不补全事实。投影可以成功，后续语义检查将因缺少其他必要字段而保持 unknown。没有读到的事实省略；显式空清单表示该次记录确实声明为空。只接受相对证据根的普通 JSON 文件，拒绝路径越界、符号链接、摘要不符、重复 JSON 键、类型不符或失效指针。哈希证明读的是哪些字节，不证明记录的人写的是真话。

`projection.json` 输出 `focus`、`focus_type`、`scope`、query/shape 资产名和 RDF 摘要。把 `evidence.ttl` 的真实摘要与 `text/turtle` 写入当前 task，按根 binding 合同建立项目事实和决策权限，再调用：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py review \
  --binding /controlled/project-binding.json \
  --workspace /controlled/semantica-registry \
  --task /controlled/current-task.json \
  --evidence-file /controlled/new-projection/evidence.ttl \
  --source-id method-evidence --format turtle \
  --scope inspection-input-A \
  --focus urn:semantica:engineering-evidence:claim/inspection-executed-1 \
  --focus-type urn:semantica:engineering-evidence:ReviewClaim \
  --query-asset findings-check-execution-origin \
  --shape-asset shape-check-execution-origin
```

包身份为 `semantica.engineering.evidence-methods`。不要手填某台作者机器的 registry 权限；发行包的冻结运输件不能自动成为接收方已晋升的正式行业包。查看绑定的 registry 是否已经采用该精确版本，再作项目审查。

命名 `query-<method>` 返回三态和原因码；`findings-<method>` 仅返回 violated/unknown，供 DecisionReview 使用。`clear` 表示本次记录没有所查缺口；执行完成、数值正确、证据适用、实物符合、允许采用仍是不同主张。raw solver/checker 记录应另外绑定实际输入与输出；受控 JSON 中的引用名字并不自动证明引用内容已经核对。

`review-coverage` 只核对声明的义务清单和所报结果、版本是否覆盖，不认证清单完整或回执真伪。用于正式汇总前，应从实际 source-locked 运行回执生成列表，并绑定相同主张、范围、版本、package 与 RDF 摘要；不能填一组 PASS 列表后称整个工程已验证。
