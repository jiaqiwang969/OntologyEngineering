# LCEDA/JLC Extraction Flow

## Evidence To Capture

For each role-specific component folder, prefer this structure:

```text
<ROLE>-<MPN>/
  lceda-device-*.json
  lceda-footprint-*.json
  lceda-3d-doc-*.json
  easyeda-device-*.raweasy.json
  source-evidence.json
  download-report.json
  *.pdf
  *.obj or *.step or *.stp
  *.derived-from-obj.step
  *.step-report.json
  source-evidence-YYYYMMDD.md
  README.md
  SHA256SUMS
```

Do not collapse two electrical roles into one folder even if they use the same physical model. Copy the same source model into separate role folders so each designator has its own trace.

Preserve source fabrication packages separately:

```text
source-<project>/
  BOM.xlsx
  pick-place.txt
  gerbers.zip
  README-or-source-note.md
  SHA256SUMS
```

## LCEDA/EasyEDA Clues

Useful places to inspect:

- Device JSON: MPN, supplier part, device title, symbol/footprint bindings.
- Footprint JSON: pad geometry, holes, pin numbering, board-lock holes.
- 3D-document JSON: model transform and internal model blob id.
- EasyEDA raw component JSON: `dataStr.head.c_para`, `packageDetail.dataStr.shape`, `lcsc.number`, `szlcsc.number`.
- EasyEDA model URLs:
  - OBJ: `https://modules.easyeda.com/3dmodel/<model_uuid>`
  - STEP: `https://modules.easyeda.com/qAxj6KHrDKw4blvCG8QJPs7Y/<model_uuid>`
- Local app cache:
  `~/.config/EasyEDA-Pro/cache.model.2/`
  or
  `~/.config/LCEDA-Pro/cache.model.2/`.

For a 32-character model hash, cache path shape is:

```text
cache.model.2/<hash[0:2]>/<hash[2:4]>/<hash[4:6]>/<hash[6:]>
```

The model cache payload may be compressed. Inspect before assuming it is OBJ text.

## BOM/Gerber-Only Recovery

Use this path when a repo has only fabrication files, not LCEDA/EasyEDA source.

1. Find source truth:
   - BOM rows: designator, quantity, manufacturer, MPN, value, package/footprint.
   - Pick-place rows: footprint, coordinates, rotation, comments.
   - Gerber/NC drill package: board release context only; it usually does not contain model evidence.
2. Convert repeated designators into role rows:
   - Example: `U2,U3,U4,U5` becomes one role folder `U2-U5-<MPN>-<LCSC>`.
   - Keep roles separate even if the model file is identical.
3. Reverse lookup by exact MPN first. If a query returns a near-miss, retry with the full BOM part string before accepting.
4. Accept a result only when the EasyEDA row proves enough of:
   - `Manufacturer Part` or title matches the BOM MPN.
   - Manufacturer is compatible with the BOM row.
   - Package/footprint matches the BOM and pick-place footprint.
   - The model UUID resolves to nonempty OBJ and/or STEP geometry.
5. Record conflicts, do not silently resolve them:
   - If BOM says `RC0402FR-074K7L` but pick-place comment says a different resistor, preserve the conflict in `source-evidence.json`.
   - If a shorter query returns a different manufacturer, reject it or save it only as rejected evidence.

The bundled helper can automate the EasyEDA/LCSC lookup and download after the role rows are prepared:

```bash
python3 "$ELECTRONICS_MODULE_ROOT/modules/lceda-jlc-3d-model-extraction/scripts/fetch_easyeda_models_from_manifest.py" \
  parts-manifest.json \
  model-sources/<project>-3d-models \
  --source-file path/to/BOM.xlsx \
  --source-file path/to/pick-place.txt \
  --source-file path/to/gerbers.zip
```

Manifest shape:

```json
{
  "parts": [
    {
      "role": "J1",
      "designators": "J1",
      "mpn": "SM04B-SRSS-TB(LF)(SN)",
      "manufacturer": "JST",
      "expected_package": "4 pos, 1 mm pitch right-angle SMD header"
    }
  ]
}
```

## OBJ Handling

OBJ source evidence is acceptable when:

- The OBJ is tied to exact device/footprint/3D-document evidence.
- The file has nonzero vertex and face records.
- The bounding box is plausible for the package.
- The folder has SHA256 verification.

OBJ alone is not Altium-ready. It still needs footprint match and Altium import evidence.

## Direct STEP Handling

If EasyEDA/JLC/LCSC provides a STEP/STP directly:

- Preserve it as `*.easyeda-source.step` or another name that clearly says it was downloaded directly.
- Treat it as a `downloaded_step_candidate`.
- Do not create a `.derived-from-obj.step` name for direct STEP files.
- It is still only an Altium import candidate until Altium import and footprint alignment are proven.

## OBJ To STEP

If FreeCAD is available on macOS, a common working execution shape is:

```bash
printf "exec(open('path/to/convert_lceda_obj_to_step_freecad.py').read())\n" \
  | /Applications/FreeCAD.app/Contents/Resources/bin/freecadcmd -c
```

Keep the output name explicit:

```text
<source>.derived-from-obj.step
<source>.derived-from-obj.step-report.json
```

Do not rename a derived STEP as if it came directly from the supplier.

## Verification

After any edit to evidence files:

```bash
python3 "$ELECTRONICS_MODULE_ROOT/modules/lceda-jlc-3d-model-extraction/scripts/audit_model_package.py" <folder>
(cd <folder> && shasum -a 256 -c SHA256SUMS)
```

If documentation files are included in `SHA256SUMS`, update their hash after editing them.
