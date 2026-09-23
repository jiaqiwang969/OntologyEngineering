# CAD 与制造工艺双向证据交接

当工艺路线取决于孔、界面、筋、焊缝可达性、装夹或公差，或工艺反过来要求 CAD 核查这些对象时，使用本交接。它把项目 ABox、问题与来源送到工程本体论控制面；不把几何接触解释成已焊合，不把名义封闭水域解释成实物密封。

## 交接对象

`contracts/cad-process-handoff.v1.schema.json` 定义一个产品配置和状态下的来源、CAD 特征、必要／候选功能、工艺路线及操作、待审决定主张、双向问题、模型假设，以及依赖这些主张的检验、费用、交期等下游输出。使用稳定的项目对象 ID；CAD 面编号只可作为同一模型版次内的来源定位。模型、图纸和工艺版次不能悄悄合并。来源路径相对于项目根目录，逐项带 SHA-256；原生几何观察必须指向 native readback JSON 中存在的 JSON Pointer。若回读文件内带原 STEP 的摘要，`embedded_source_hashes` 应将它与原生模型来源绑定。必要功能若尚未定位到具体 CAD 特征，`feature_ids` 保持空数组，并在工艺 → CAD 的开放问题中要求映射，不能为了填表指派一个不相干特征。

CAD → 工艺的问题要指向具体特征与待承担功能，例如“这条通向外界的孔周路径由哪道工序形成连续封水边界”。工艺 → CAD 的问题要指向候选路线及需要核查的几何，例如“指定焊道在该版盖板、流道和夹具状态下是否可达”。两边都写明所需证据。工艺专家的偏好是有来源的候选主张；`intended_function_ids` 只记录已明确提出的覆盖意图，不能根据工序名称自动补齐。试验来源必须对应实际产品状态和验证范围。

从模块根目录运行：

```bash
.venv/bin/python scripts/cad_process_handoff.py \
  --packet /project/output/cad_process/handoff.json \
  --project-root /project \
  --output-dir /project/output/cad_process
```

转换器逐项核对来源文件、嵌入摘要、对象引用和原生回读字段，生成项目 ABox、完整性记录，以及分别发给 CAD 和工艺的原始问题清单。`semantic_execution=not_run`、`engineering_verdict=not_assessed` 是固定的边界；完整性通过只证明输入可追溯。项目语义审查仍需本项目 `ProjectOntologyBinding`、fresh `SemanticTaskEnvelope`、已晋升 package 和 source-locked Semantica `review`。候选 package 的作者测试不能替代项目审查，语义清晰也不能替代接头截面、检漏或承压试验。

制造工艺侧若正在作者评估拟议的 `0.1.3` 因果包，可以在同一交接包上运行[工艺投影脚本](../../manufacturing-process-cost/scripts/cad_handoff_to_q21.py)，把一条明确指定原路线、新路线的整批交付主张投影为 `urn:mfg:loop:` 的 `q21` 输入。它只读取已通过完整性检查的来源，不补造功能清单完备性，也不把候选操作写成 `verified` 覆盖。再由[作者试跑脚本](../../manufacturing-process-cost/scripts/run_q21_authoring_review.py)调用 Semantica 原生 `DecisionReviewRunner.evaluate_manifest`，使用确切拟议 delta 执行作者级查询和 shape；结果标为 `proposed_package_authoring_only`，没有项目 review receipt。旧本地 CAD CQ／shape 只保留历史比较入口。

新旧模型／工艺包同时可用时，加 `--previous-packet`。转换器按明确的来源和对象依赖给出受影响主张、问题和下游输出清单；它仅提示重审范围，不自行宣布旧主张失败或新工艺合格。制造工艺模块据此核对功能覆盖、检验、成本、交期和对外输出是否需要修订，并将实际决定回写项目记录。

对于通孔、板间界面和密封等案例，至少分别核查：几何连通性、实际连接区域、受压／松夹／后加工后的状态、密封试验覆盖，以及流体／热／结构仿真的边界假设。局部 CWA 只在已声明的模型和完备性范围内使用；项目实物主张的未知项保持 OWA。
