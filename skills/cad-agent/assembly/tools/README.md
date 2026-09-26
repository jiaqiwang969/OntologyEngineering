# assembly/tools — 逐 occurrence 装配片工具链正本（2026-09-03 收进 skill）

来源：X1（重建-20260829，30 个工具）→ Poppy/Fourier → OLSK Large CNC V3（+18 个工具与 v005 表达层/量具修正）。
从 OLSK 起，**工具正本在这里**；项目目录只放 `robot-config.v1.json`、受控记录与产物，不再复制工具。
变更记录见 `CHANGELOG.md`（原 TOOL_LEDGER）。案例适配器（S3 绑定、SOP 搬运）在 `adapters/`，不通用。

## 运行约定

- 每个工具用 `--case-dir <项目目录>` 或环境变量 `REBUILD_CASE_DIR` 定位案例；路径由项目的 `robot-config.v1.json` 的 `paths` 决定
  （模板见 `regress/robot-config.olsk.example.json`）。产物一律写进项目目录，create-only（已存在需显式 `--allow-existing-output`）。
- Python 依赖：numpy、trimesh、python-fcl、scipy、rtree（行走/审计）；Blender 5.2（构建/回读/渲染）；ffmpeg；PIL（合成）。
- 重活（行走、审计、渲染）默认上舰队节点：`farm/farm_launcher.py` 保活式并发 + `pgrep`/`loadavg` 核实（K-GOV-16）。
  `farm/*template*.sh` 是 OLSK v005 的链脚本样板，路径需按项目替换。

## 执行序（与 playbook 一节一致）

```
S0 冻结 → S1 parse_step_assembly_{generic|ap242}.py + s1_xcaf_channel_b.py + s1_crosscheck_compare.py
→ S2 extract_definition_geometry.py / export_definition_stl.py（多体：split_definition_bodies.py + derive_body_view.py + extract_body_geometry.py）
   + build_interface_graph_generic.py + build_plane_interface_graph_generic.py（+gap05）
→ S3 adapters/build_<case>_s3_binding.py（S5 折叠、台架）+ build_glb_step_bridge.py（GLB 网格桥接，可选）
→ S7 sweep_certify_fcl_generic.py / sweep_certify_bvh_generic.py（交叉）+ validate_axis_choice_generic.py
   + sweep_sequence_reverse_generic.py（权威运动来源；--cap 按机器尺度反解，K-PATH-19）→ merge_s7_chunks.py → merge_s7_generic.py
→ S8 build_connection_graph_generic.py → build_play_order_generic.py → order_by_tree.py → audit_tree_conformance.py
→ build_playable_from_seqwalk.py（审计回灌；改道优先，K-PATH-18）→ freeze_film_state_generic.py（create-only 冻结）
→ S10 build_assembly_animation_generic.py（分组机位/材质/虚化/插值显式落盘：v005 参数见 farm/chain_template_v005.sh）
   → reopen_readback_generic.py → render_film_generic.py（--samples）→ compose_assembly_film_generic.py
   → build_score_bed.py + narrate-and-score/assemble.py（配乐）→ media_qc_generic.py → write_review_request.py
→ 验证：audit_path_penetration_generic.py --scene film + audit_sibling_paths_generic.py + visual_overlap_scan.py（独立口径，K-GOV-15）
   → 多智能体逐帧审片（K-DEL-04）→ build_acceptance_packet.py → validator/validate_acceptance.py --profile engineering-film
```

## 量具

`baseline_region.py` 是行走与审计共用的 K-IF-04 量具（区域嫌疑 + 双向绕数含入）。2026-09-03 起含入采样为
表面采样沿法向向内偏移 0.3 mm（K-IF-09）；`inward_eps<=0` 退回顶点法（仅供复现旧结果）。
独立验证口径 `visual_overlap_scan.py` 不豁免基线接触，分 SEATED_OVERLAP / PATH_OVERLAP。

## 回归

`regress/cases.json` 登记案例与关键产物摘要；`regress/regress.py --compile` 编译全部工具，`--case <id>` 比对已登记摘要。
X1/Poppy 零差异回归基线待录入（open-item O-24）。
