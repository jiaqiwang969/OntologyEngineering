# 工程证据方法整合

本次是既有工程本体上的方法扩展，公开发布版本仍另行管理。增加共用 `engineering-evidence-methods` 模块，供 CAD、制造工艺／成本、仿真、测量和科研验证交接证据。原工程算法、客户事实、参数、图纸与对话不随模块吸收。

## 改了什么

此前 `analysis-record.v1` 能要求填写假设并核对来源，但自由文字条件不能直接判定当前证据是否适用。现在专业工具或受控审阅把所需事实放入 JSON 快照，薄适配器逐字段绑定来源摘要和 JSON Pointer，Semantica 用命名查询和约束返回适用性原因。

九类方法具体化为 19 项检查：

| 类别 | 检查项 |
| --- | --- |
| 数量和条件 | quantity-reference、condition-domain、parameter-use、effect-accounting |
| 表示与因果 | representation-adequacy、controlled-effect-contrast、drive-capacity-evidence、work-attribution |
| 验证与推导 | validation-lineage、proof-engineering-transfer |
| 观察、工具和取证 | bounded-non-detection、check-execution-origin、checker-capability-coverage、corroboration-independence、acquisition-execution-mode、acquisition-effect |
| 物料与成本 | material-balance、material-cost-basis |
| 声明覆盖 | review-coverage |

每项只判断自己的证据义务。`satisfied`、`violated`、`unknown` 不等于实物合格、不合格和待测；例如反例可以仅说明所用模型不支持该结论，原物理对象仍未判定。显式空清单与未提供清单分开，单位未统一则保持未知，不直接比较数值。

`review-coverage` 核对的是已声明的义务和结果清单。它不会自动识别所有必需义务，也不认证清单引用的回执。资料解释、特征提取、专业条件是否完整、真实证据质量仍需领域工具和工程审阅；来源哈希不是事实真实性的证明。

## 架构和兼容

Semantica 是唯一执行正式规则的后端。本次新增 `semantica.engineering.evidence-methods` 数据包；运行时仍为锁定的 `0.6.5+oe.6`，没有另加语义引擎。OE 的新适配器只检查结构、来源摘要、指针和类型，再投影 ABox。

原 `analysis-record.v1` 与制造包 `0.1.1` 保持可用；自由文字假设不会自动升级为新模块已验证的条件。章节包原有的部分实现状态也没有因本次扩展而被改成完成。两卷书不作修改。

本次同时修复统一 CLI 的输出协议：原生推理进度写入 stderr，stdout 保持单个 JSON。该修复不改变 Semantica 的判定规则。

## 复验

```bash
runtime/.venv/bin/python scripts/method_evidence.py list
runtime/.venv/bin/python scripts/run_methodology_cases.py --run --output /controlled/new-method-replay
runtime/.venv/bin/python -m pytest -q tests/test_method_evidence.py tests/test_semantic_cli_output.py
```

包身份、SHA-256 和资产清单在 `runtime/semantic-bundles.json`。冻结案例验证检查逻辑；正式项目使用还需自己的 exact binding、ABox 和根 `semantic_engagement.py review`。分发包不会把作者的 registry 权限带给接收方。

方法成熟度分别记录为假说、合同可执行、合成反例通过、独立任务试用、真实效果复核。好的想法可以在前两阶段进入候选并被完善，但合成回归不能用来声称真实降本或物理验证已经完成。
