# Altium Boundary

## Format Boundary

Use this rule when explaining status:

- OBJ is source evidence.
- Directly downloaded STEP/STP is the practical generic Altium 3D body import candidate.
- A STEP derived from OBJ is not a primary-source STEP.

Altium public 3D Body documentation lists STEP, SolidWorks, and Parasolid as generic 3D model formats. Do not assume direct OBJ import into Altium unless a real Altium job proves it.

Official reference:

```text
https://www.altium.com/documentation/altium-designer/pcb-3d-body-properties
```

## Release Boundary

Model extraction does not release a PCB. Keep these false until real evidence exists:

```text
import_allowed=false
pcblib_created=false
footprint_match_recorded=false
footprint_alignment_checked=false
formal_fabrication_release_allowed=false
```

## Altium VM Probe Ladder

Use a staged ladder:

1. File visibility probe:
   - Copy OBJ/derived STEP candidates into the VM workspace.
   - Write a report proving files exist.
   - No PcbDoc mutation, no PcbLib, no 3D body.

2. Disposable import probe:
   - Copy the PcbDoc or create a scratch PcbLib/PcbDoc.
   - Try importing one 3D body.
   - Save only scratch artifacts.
   - Prefer uploading the scratch PcbDoc as an Altium job asset into
     `C:\Users\Public\altium_cli` instead of copying the official PcbDoc from
     inside DelphiScript. This keeps official project files out of the write
     path and avoids asynchronous Windows copy/delete races.
   - Write a small `probe.txt` before every risky API call such as
     `PCBObjectFactory`, `ModelFactory_FromFilename`, `SetState_FromModel`, and
     `Board.AddPCBObject`.
   - A successful disposable import may still leave `pcblib_created=false`,
     `footprint_alignment_checked=false`, and `formal_fabrication_release_allowed=false`.

3. Footprint alignment probe:
   - Attach the body to the intended footprint.
   - Check pin-1, pad/hole pattern, seating plane, right-angle direction, and body height.
   - Capture 2D and 3D screenshots.
   - If the official footprint is only available through ODB/PnP/test-point
     evidence, a scratch reconstruction of that footprint is a useful
     intermediate probe. Label it as `scratch_odb_footprint_alignment_checked`
     and keep `pcblib_created=false`,
     `official_pcbdoc_component_alignment_checked=false`, and
     `formal_fabrication_release_allowed=false`.

4. Scratch PcbLib attachment probe:
   - Create or open a scratch `PCBLIB`, not the official project library.
   - Open/create the `PCBLIB` document before checking `PCBServer`.
   - Avoid `PCBServer.PreProcess/PostProcess` if that route has stalled in the
     local Altium version.
   - Add footprint primitives and `IPCB_ComponentBody` to the PcbLib component
     through the PcbLib transfer workaround:
     `TransferAllPrimitivesBackFromBoard`,
     `AddPCBObject`, and `TransferAllPrimitivesOntoBoard`.
   - Write probe markers before `ModelFactory_FromFilename`,
     `SetState_FromModel`, and body registration.
   - A successful scratch PcbLib probe may record `pcblib_created=true` and
     `pcblib_3d_body_attached=true`, but still keep
     `official_project_library_modified=false`,
     `official_pcbdoc_component_alignment_checked=false`, and
     `formal_fabrication_release_allowed=false`.

5. Project integration:
   - Only after alignment evidence is accepted.
   - Still keep formal fabrication release false unless the broader project release gate explicitly opens.

## Common Failure Shield

If an Altium DelphiScript job creates the done marker but not report files, check for asynchronous `RunApplication('cmd /c del ...')` calls. They can delete a report after `TStringList.SaveToFile`. Prefer overwriting report files directly or make deletion synchronous outside Altium before running the script.

If the job runner times out but WireGuard or SSH was unstable, reconnect/fix
the route and inspect `C:\Users\Public\altium_cli` before assuming the Altium
script failed. A real VM job can finish and create the done/report files after
the local SSH transport has timed out.
