---
name: lceda-jlc-3d-model-extraction
description: Extract and audit JLC/LCEDA/EasyEDA component 3D model evidence for PCB work, especially when Codex needs to find or preserve OBJ/STEP/STP model files, inspect LCEDA device/footprint/3D JSON, recover models from BOM/Gerber/pick-place-only packages by reverse-looking-up EasyEDA/LCSC parts, convert OBJ to an Altium STEP import candidate, prepare SHA256 evidence packages, or record that Altium import, PcbLib creation, footprint alignment, and fabrication release are still blocked.
---

# LCEDA/JLC 3D Model Extraction

本文件是 [electronics-design](../../SKILL.md) 的内部专业指南；从总入口按需读取，脚本与资源路径均相对本模块目录。

## v2 工程接续

用于 v2 管理的工程项目时，由 `ontology-engineering-v2` 主导，先按
[electronics-design 接续约定](../../references/v2-handoff.md)
核同一 Project、对象修订、条件、来源和本次证据缺口；以下专业操作保留其实际工具边界。
结果连同原生文件、输入/工具版本、来源摘要、覆盖、失败与未知交回原 Project，
不另建决策账本，不把操作完成或本工具检查通过当成工程采用。
用户当前授权及后台不抢焦点要求优先；需要前台才能完成的项保持未执行并说明限制，
不能依据下文历史操作方法自动激活应用。若单独分发缺少上述接续文件，读取已安装的
`electronics-design` / v2 正本；仍缺失则说明缺项，不声称 v2 接入已完成。

本系统执行限制：浏览器/桌面沿当前用户指定的 Peekaboo 现有后台能力，观察禁网页聚焦回退，
交互禁自动聚焦；不按旧参考自举 Chrome 调试实例、使用 Chrome 插件或 osascript，
不读取登录存储/浏览器凭据，不静默重启浏览器、改设置或重装连接器。
任务临时服务仅在当前任务需要且已授权时启动，保存归属；用完核归属后清退本任务进程，
不停止其他会话或用户已有服务，不设置自启/自动重连。
在 v2 项目内使用裸 JS/API 桥前，先核主工具 typed action 确实未覆盖且当前授权包含该
debug 路径；沿已有授权执行，不重复索要许可，也不通过子 skill 绕过主工具阶段检查。

## Core Rule

Treat LCEDA/JLC/EasyEDA OBJ files as source model evidence when they are tied to device, footprint, 3D-document, datasheet, and hash evidence. If EasyEDA provides a STEP/STP directly, preserve it as a primary downloaded STEP candidate. If STEP is derived from OBJ, label it explicitly as derived. Do not treat a visually correct orientation as footprint alignment; right-angle connectors and vendor STEP files can still need model-origin offsets. Do not claim Altium readiness until a real Altium import, footprint alignment check, and screenshot evidence exist.

Do not read, store, or repeat account credentials, activation files, cookies, private keys, license bypass details, or passwords.

## Workflow

1. Identify the exact component row:
   - MPN, supplier/LCSC/JLC code, role designator, footprint name.
   - Reject near-miss rows unless their footprint and package geometry are proven.
   - If the repo only has fabrication outputs, parse BOM, pick-place, and Gerber package notes first. Use BOM as the source row, but record pick-place conflicts.

2. Capture source evidence:
   - Device JSON, footprint JSON, 3D-document JSON, datasheet/package drawing.
   - If using LCEDA 3D documents, inspect `dataStr.head.uuid`; that internal blob can be the real model payload even when the outer 3D document URL is not directly downloadable.
   - Save the model as OBJ/STEP/STP under a role-specific folder.
   - If only MPNs are available, reverse-lookup EasyEDA/LCSC and require exact MPN/manufacturer/footprint evidence before accepting the model.

3. Audit the model folder:
   - Include `SHA256SUMS`.
   - Run the bundled script:

```bash
python3 "$ELECTRONICS_MODULE_ROOT/modules/lceda-jlc-3d-model-extraction/scripts/audit_model_package.py" /path/to/model-folder
```

4. If only OBJ is available and Altium is the target:
   - Preserve OBJ as the source evidence.
   - Convert OBJ to a `.derived-from-obj.step` import candidate using FreeCAD or another traceable converter.
   - Write a per-file conversion report and include both files in `SHA256SUMS`.
   - Keep the derived STEP label explicit: `derived-from-obj`, not primary-source STEP.

5. Before claiming 3D model alignment:
   - Verify pad pattern, pin-1, right-angle direction, board-lock/peg pads, body side, and board seating plane.
   - Compare source footprint pad centers from EasyEDA/LCEDA package JSON (`packageDetail.dataStr.shape`) against the recovered Gerber/KiCad footprint. Do not rely on pick-place rotation alone.
   - Compare OBJ/STEP bounding boxes and origins. If possible, compare against KiCad or vendor official same-part STEP to detect vendor model-origin shifts.
   - If direction is right but pins or posts are not on pads, tune KiCad/Altium 3D body transform with explicit `offset xyz` plus `rotate xyz`; record the exact values.
   - For right-angle connectors, render a close-up from the pad side and check that all visible signal legs/posts sit over the corresponding pads/traces, not merely that the plastic body faces the board edge.
   - Preserve evidence paths: source pad coordinates, model bbox/origin comparison, transform values, close-up screenshot, and DRC/ERC status.
   - Label the result as `visual_alignment_evidence` unless a formal CAD/Altium alignment process has been completed.

6. Before Altium import:
   - Check pad pattern, plated holes, peg/board-lock holes, pin-1, right-angle direction, height, and board seating plane.
   - Record `import_allowed=false` until source evidence, footprint match, and Altium alignment evidence exist.

7. For Altium VM work:
   - Use the project or global Altium VM skill after this skill.
   - First prove the candidate files are visible in the VM workspace.
   - Only then attempt PcbLib/3D Body import and capture 2D/3D screenshots.
   - For Altium PcbLib automation, prefer the verified PcbLib transfer
     workaround: open/create the `PCBLIB` document first, then add pads, tracks,
     text, and `IPCB_ComponentBody` through
     `TransferAllPrimitivesBackFromBoard`, `AddPCBObject`, and
     `TransferAllPrimitivesOntoBoard`. Avoid treating a scratch PcbLib as an
     official library or release artifact.

## Status Vocabulary

Use these terms consistently:

- `downloaded_obj_candidate`: OBJ source model captured with evidence.
- `downloaded_step_candidate`: STEP/STP downloaded directly from EasyEDA/JLC/LCSC evidence.
- `derived_step_candidate`: STEP generated from OBJ for Altium probing.
- `altium_import_status=not_imported`: no real Altium 3D body exists yet.
- `footprint_alignment_checked=false`: no footprint/model alignment proof yet.
- `visual_alignment_evidence`: local KiCad/preview render plus source geometry checks suggest model-to-pad fit, but formal CAD/Altium alignment is still not proven.
- `formal_fabrication_release_allowed=false`: never changed by model capture.

## BOM/Gerber-Only Recovery

When a project has no LCEDA/EasyEDA source but does have BOM/Gerber/pick-place files:

1. Preserve the fabrication package under `source-*/` with hashes.
2. Extract role rows into a manifest with `role`, `designators`, `mpn`, `manufacturer`, and `expected_package`.
3. Use the helper script when appropriate:

```bash
python3 "$ELECTRONICS_MODULE_ROOT/modules/lceda-jlc-3d-model-extraction/scripts/fetch_easyeda_models_from_manifest.py" \
  parts-manifest.json \
  model-sources/<project>-3d-models \
  --source-file path/to/BOM.xlsx \
  --source-file path/to/pick-place.txt \
  --source-file path/to/gerbers.zip
```

4. Re-run `audit_model_package.py` on every accepted role folder and `shasum -a 256 -c SHA256SUMS`.
5. If a query returns a near-miss, try a more exact query before accepting anything. Example: `SM04B-SRSS-TB` may return a different manufacturer, while `SM04B-SRSS-TB(LF)(SN)` can return the exact JST row.

## Evidence boundary

The public module carries no private-board example or historical success receipt.
A new extraction must produce its own source/model identities, conversion record and
alignment checks. Right-angle orientation alone cannot establish pad/post alignment;
check the exact footprint geometry, model origin, dimensions and explicit transform.
The offline package checker is a file/geometry summary helper, not a CAD verifier or
permission to fabricate. An absent, incomplete or stale SHA256 manifest must be
resolved before treating the package as complete.

## References

- Read `references/extraction-flow.md` when reconstructing the exact LCEDA/JLC extraction and evidence workflow.
- Read `references/altium-boundary.md` before making claims about OBJ, STEP, Altium 3D Body import, or release readiness.
