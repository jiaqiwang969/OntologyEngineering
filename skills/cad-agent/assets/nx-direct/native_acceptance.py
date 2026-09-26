"""Bounded native acceptance: parametric solid, repeated assembly, STEP round trip.

run() is called by worker.py inside NX. All writes stay in the fresh job/out.
The inspect action opens staged copies and never saves them.
"""
def run(session, job, params):
    import math
    import NXOpen
    import NXOpen.Features
    import NXOpen.UF
    uf = NXOpen.UF.UFSession.GetUFSession()
    def info(part):
        bodies = list(part.Bodies)
        solids = [b for b in bodies if b.IsSolidBody]
        planar_vertex_bounds = []
        for body in solids:
            faces = list(body.GetFaces())
            if faces and all(f.SolidFaceType == NXOpen.Face.FaceType.Planar for f in faces):
                vertices = [p for e in body.GetEdges() for p in e.GetVertices()]
                xyz = [[p.X,p.Y,p.Z] for p in vertices]
                planar_vertex_bounds.append([min(v[i] for v in xyz) for i in range(3)] + [max(v[i] for v in xyz) for i in range(3)])
            else:
                planar_vertex_bounds.append(None)
        root = part.ComponentAssembly.RootComponent
        components = []
        if root:
            for c in root.GetChildren():
                pt, mat = c.GetPosition()
                components.append({"name": c.Name, "prototype_path": c.Prototype.FullPath, "origin": [pt.X, pt.Y, pt.Z], "matrix": [getattr(mat,k) for k in ('Xx','Xy','Xz','Yx','Yy','Yz','Zx','Zy','Zz')]})
        return {"path": part.FullPath, "units": "mm" if part.PartUnits == NXOpen.BasePart.Units.Millimeters else "inch" if part.PartUnits == NXOpen.BasePart.Units.Inches else "unknown", "solid_count": len(solids), "sheet_count": len([b for b in bodies if b.IsSheetBody]), "bbox": [list(uf.ModlGeneral.AskBoundingBox(b.Tag)) for b in solids], "bbox_role": "UF envelope, may include tolerance padding", "planar_vertex_bounds": planar_vertex_bounds, "expressions": [{"name": e.Name, "rhs": e.RightHandSide, "value": e.Value} for e in part.Expressions if e.Name.startswith('cad_')], "components": components}
    def save(part):
        status = part.Save(NXOpen.BasePart.SaveComponents.TrueValue, NXOpen.BasePart.CloseAfterSave.FalseValue)
        status.Dispose()
    if params.get("action") == "inspect":
        reports = []
        for name in params["parts"]:
            path = (job / "inputs" / name).resolve()
            if path.parent != (job / "inputs").resolve(): raise ValueError("input must be a staged file")
            part, status = session.Parts.OpenBaseDisplay(str(path))
            status.Dispose()
            reports.append(info(part))
        return {"action": "fresh_process_readback", "parts": reports, "engineering_verdict": "not_assessed"}
    out = job / "out"
    if params.get("action") == "import_step":
        source = (job / "inputs" / params["step"]).resolve()
        if source.parent != (job / "inputs").resolve(): raise ValueError("input must be staged")
        target = out / "step_roundtrip.prt"
        importer = session.DexManager.CreateStep214Importer()
        try:
            importer.InputFile = str(source)
            importer.OutputFile = str(target)
            importer.ImportTo = NXOpen.Step214Importer.ImportToOption.NewPart
            importer.FileOpenFlag = False
            importer.ProcessHoldFlag = True
            importer.ObjectTypes.Solids = True
            importer.Commit()
        finally: importer.Destroy()
        if not target.is_file(): raise RuntimeError("translator returned without a PRT")
        imported, status = session.Parts.OpenBaseDisplay(str(target))
        status.Dispose()
        report = info(imported)
        if report["solid_count"] != 1 or report["units"] != "mm": raise RuntimeError("STEP imported representation/units differ")
        save(imported)
        return {"action": "import_step", "part": report, "supplier_identity": "synthetic_fixture_not_supplier_CAD"}
    part = session.Parts.NewBaseDisplay(str(out / "parametric_block.prt"), NXOpen.BasePart.Units.Millimeters)
    exp = part.Expressions.CreateWithUnits("cad_length=40", part.UnitCollection.FindObject("MilliMeter"))
    b = part.Features.CreateBlockFeatureBuilder(NXOpen.Features.Feature.Null)
    try:
        b.Type = NXOpen.Features.BlockFeatureBuilder.Types.OriginAndEdgeLengths
        b.SetOriginAndLengths(NXOpen.Point3d(0.0,0.0,0.0), "cad_length", "20", "10")
        b.CommitFeature()
    finally: b.Destroy()
    before = info(part)
    mark = session.SetUndoMark(NXOpen.Session.MarkVisibility.Invisible, "dimension update")
    part.Expressions.Edit(exp, "55")
    errors = session.UpdateManager.DoUpdate(mark)
    if errors: raise RuntimeError(f"NX update reports {errors} errors")
    after = info(part)
    if not math.isclose(after["bbox"][0][3]-after["bbox"][0][0],55.0,abs_tol=1e-6):
        raise RuntimeError("expression change did not update the native solid")
    save(part)
    assembly = session.Parts.NewBaseDisplay(str(out / "two_instances.prt"), NXOpen.BasePart.Units.Millimeters)
    rotation = NXOpen.Matrix3x3()
    rotation.Xx=rotation.Yy=rotation.Zz=1.0
    for name, x in (("BLOCK_A",0.0),("BLOCK_B",80.0)):
        c, status = assembly.ComponentAssembly.AddComponent(part.FullPath, "Entire Part", name, NXOpen.Point3d(x,0.0,0.0), rotation, -1)
        status.Dispose()
    save(assembly)
    assembly_info = info(assembly)
    if len(assembly_info["components"]) != 2: raise RuntimeError("assembly occurrence count differs")
    # Export the part via a file selection, not the displayed assembly.
    step = session.DexManager.CreateStepCreator()
    try:
        step.ExportAs = NXOpen.StepCreator.ExportAsOption.Ap214
        step.ExportFrom = NXOpen.StepCreator.ExportFromOption.ExistingPart
        step.InputFile = part.FullPath
        step.OutputFile = str(out / "parametric_block.step")
        step.LayerMask = "1-256"
        step.ExportSelectionBlock.SelectionScope = NXOpen.ObjectSelector.Scope.EntirePart
        step.ObjectTypes.Solids = True
        step.FileSaveFlag = False
        step.ProcessHoldFlag = True
        step.Commit()
    finally: step.Destroy()
    step_path = out / "parametric_block.step"
    if not step_path.is_file() or not step_path.read_bytes().startswith(b"ISO-10303-21;"):
        raise RuntimeError("STEP export did not produce an ISO-10303-21 file")
    if b"MANIFOLD_SOLID_BREP" not in step_path.read_bytes():
        raise RuntimeError("STEP export contains no expected solid B-Rep; header-only files are not geometry")
    return {"action": "create", "before": before, "after": after, "assembly": assembly_info, "update_error_count": errors, "joint_constraints": "not_created_or_verified", "contact_and_accuracy": "not_assessed"}
