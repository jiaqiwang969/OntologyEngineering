# -*- coding: utf-8 -*-
"""Width-change pilot (batch, run_journal.exe). Works ONLY inside a copy folder whose path contains '\\mod\\'.
1. Skeleton part SKEL_W.prt with expressions web_width_ref / web_width / dW / half, added as a component.
2. Every width-driven part: inter-part expression dw_half = SKEL_W::half, then two synchronous Move Face features
   (all faces on the +side / -side of the part's mid-plane along the machine cross axis) driven by dw_half.
3. Every position-driven component: moved by +/-half along the machine cross axis inside its owning sub-assembly
   (side = world-y of the occurrence's geometry centre). Applied delta is tracked in pilot_state.json.
4. 纸路 extrude end limit follows SKEL_W::dW. Update, verify (bounding boxes), save all parts under the copy.
Config via env INSPECT_CFG: {"asm","copy_dir","plan","out","log","delta_w","web_width_ref","do_width","do_position","save"}"""
import json, math, os, time, traceback
import NXOpen
import NXOpen.UF
import NXOpen.Features
import NXOpen.GeometricUtilities
import NXOpen.Assemblies

cfg = json.load(open(os.environ["INSPECT_CFG"], encoding="utf-8"))
ASM, COPY, PLAN, OUT, LOG = cfg["asm"], cfg["copy_dir"], cfg["plan"], cfg["out"], cfg.get("log", cfg["out"] + ".log")
DW, REF = float(cfg.get("delta_w", 100.0)), float(cfg.get("web_width_ref", 550.0))
SKEL = cfg.get("skeleton", "SKEL_W")
T0 = time.time()
assert "\\mod\\" in COPY.lower() or "/mod/" in COPY.lower(), "refusing to run outside a \\mod\\ copy folder"

session = NXOpen.Session.GetSession()
ufs = NXOpen.UF.UFSession.GetUFSession()


def log(msg):
    with open(LOG, "a", encoding="utf-8") as f:
        f.write("[%6.1fs] %s\n" % (time.time() - T0, msg))


def identity():
    m = NXOpen.Matrix3x3()
    m.Xx, m.Xy, m.Xz, m.Yx, m.Yy, m.Yz, m.Zx, m.Zy, m.Zz = 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0
    return m


def part_of(c):
    proto = c.Prototype
    return proto if isinstance(proto, NXOpen.BasePart) else proto.OwningPart


def leaf_of(c):
    return part_of(c).Leaf


def mat_rows(m):
    return [[m.Xx, m.Xy, m.Xz], [m.Yx, m.Yy, m.Yz], [m.Zx, m.Zy, m.Zz]]


def local_axis_for_world_y(m):
    """Which part-local axis (0/1/2) and sign points along world +Y, given the component orientation matrix."""
    R = mat_rows(m)
    # NX Matrix3x3 from GetPosition: rows are the component's local axes expressed in absolute coordinates
    best, bsign, bval = 0, 1.0, -1.0
    for i in range(3):
        v = R[i][1]
        if abs(v) > bval:
            best, bval, bsign = i, abs(v), (1.0 if v >= 0 else -1.0)
    return best, bsign, bval


def world_to_local(m, v):
    R = mat_rows(m)   # local axis i in world = R[i]
    return [R[0][0] * v[0] + R[0][1] * v[1] + R[0][2] * v[2],
            R[1][0] * v[0] + R[1][1] * v[1] + R[1][2] * v[2],
            R[2][0] * v[0] + R[2][1] * v[1] + R[2][2] * v[2]]


def local_to_world(m, v):
    R = mat_rows(m)
    return [R[0][j] * v[0] + R[1][j] * v[1] + R[2][j] * v[2] for j in range(3)]


def face_box(face):
    """Face extents from its edge vertices (UF_MODL face/bbox calls are absent in this Python binding).
    Closed edges carry their seam vertex, so the extent along the part's length axis is reliable."""
    try:
        xs, ys, zs = [], [], []
        for e in face.GetEdges():
            try:
                v1, v2 = e.GetVertices()
            except Exception:
                continue
            for pt in (v1, v2):
                xs.append(pt.X)
                ys.append(pt.Y)
                zs.append(pt.Z)
        if not xs:
            return None
        return [min(xs), min(ys), min(zs), max(xs), max(ys), max(zs)]
    except Exception:
        return None


def part_bbox(part):
    bb = None
    for b in part.Bodies:
        try:
            if not b.IsSolidBody:
                continue
        except Exception:
            continue
        for f in b.GetFaces():
            fb = face_box(f)
            if fb is None:
                continue
            bb = fb if bb is None else [min(bb[0], fb[0]), min(bb[1], fb[1]), min(bb[2], fb[2]), max(bb[3], fb[3]), max(bb[4], fb[4]), max(bb[5], fb[5])]
    return bb


def ensure_expression(part, name, rhs):
    mm = part.UnitCollection.FindObject("MilliMeter")
    try:
        e = part.Expressions.FindObject(name)
        if e.RightHandSide.strip() != rhs:
            part.Expressions.EditWithUnits(e, mm, rhs)
        return e, False
    except Exception:
        e = part.Expressions.CreateSystemExpressionWithUnits("%s=%s" % (name, rhs), mm)
        return e, True


def move_faces(part, faces, axis_vec, dist_expr, tag):
    """Synchronous Move Face (NX 2412 Python API: MoveFaceCollector / Type / Direction / Distance)."""
    mfb = part.Features.CreateMoveFaceBuilder(NXOpen.Features.MoveFace.Null)
    try:
        rule = part.ScRuleFactory.CreateRuleFaceDumb(faces)
        mfb.MoveFaceCollector.ReplaceRules([rule], False)
        d = part.Directions.CreateDirection(NXOpen.Point3d(0.0, 0.0, 0.0), NXOpen.Vector3d(float(axis_vec[0]), float(axis_vec[1]), float(axis_vec[2])), NXOpen.SmartObject.UpdateOption.WithinModeling)
        T = NXOpen.Features.MoveFaceBuilder.Types
        names = [n for n in dir(T) if not n.startswith("_")]
        if hasattr(T, "TranslateDirectionAndDistance"):
            mfb.Type = T.TranslateDirectionAndDistance
        elif hasattr(T, "Distance"):
            mfb.Type = T.Distance
        else:
            raise RuntimeError("no translate type in MoveFaceBuilder.Types: %s" % names)
        mfb.Direction = d
        mfb.Distance.RightHandSide = dist_expr
        feat = mfb.Commit()
        try:
            feat.SetName("W_%s" % tag)
        except Exception:
            pass
        return feat
    finally:
        mfb.Destroy()


def main():
    open(LOG, "w", encoding="utf-8").close()
    plan = json.load(open(PLAN, encoding="utf-8"))
    state_path = os.path.join(COPY, "pilot_state.json")
    state = json.load(open(state_path, encoding="utf-8")) if os.path.exists(state_path) else {"applied_dw": 0.0, "moved": {}, "stretched": []}
    report = {"asm": ASM, "delta_w": DW, "web_width_ref": REF, "skeleton": {}, "width": [], "position": [], "errors": [], "paper_path": None}
    lo = session.Parts.LoadOptions
    lo.ComponentsToLoad = NXOpen.LoadOptions.LoadComponents.All
    lo.UsePartialLoading = False
    lo.AbortOnFailure = False
    asm, st = session.Parts.OpenBaseDisplay(ASM)
    try:
        st.Dispose()
    except Exception:
        pass
    asm = session.Parts.Display
    log("opened %s" % asm.Leaf)
    mark = session.SetUndoMark(NXOpen.Session.MarkVisibility.Invisible, "pilot")
    mm = None

    # ---------- 1. skeleton
    skel_path = os.path.join(COPY, SKEL + ".prt")
    skel = None
    for p in session.Parts:
        if p.Leaf == SKEL:
            skel = p
    if skel is None and os.path.exists(skel_path):
        skel, st2 = session.Parts.Open(skel_path)
        try:
            st2.Dispose()
        except Exception:
            pass
    created = False
    if skel is None:
        skel = session.Parts.NewBaseDisplay(skel_path, NXOpen.BasePart.Units.Millimeters)
        created = True
        session.Parts.SetDisplay(asm, False, True)
        log("skeleton created %s" % skel_path)
    ensure_expression(skel, "web_width_ref", "%g" % REF)
    ensure_expression(skel, "web_width", "%g" % (REF + DW))
    ensure_expression(skel, "dW", "web_width-web_width_ref")
    ensure_expression(skel, "half", "dW/2")
    if created:
        skel.Save(NXOpen.BasePart.SaveComponents.TrueValue, NXOpen.BasePart.CloseAfterSave.FalseValue)
        try:
            comp, st3 = asm.ComponentAssembly.AddComponent(skel_path, "MODEL", SKEL, NXOpen.Point3d(0.0, 0.0, 0.0), identity(), -1)
            try:
                st3.Dispose()
            except Exception:
                pass
            log("skeleton added as component")
        except Exception as e:
            log("AddComponent failed: %s" % e)
            report["errors"].append("AddComponent: %s" % e)
    report["skeleton"] = {"path": skel_path, "web_width": REF + DW, "dW": DW, "half": DW / 2}

    touched = {asm.Tag: asm, skel.Tag: skel}
    root = asm.ComponentAssembly.RootComponent
    occ_by_leaf = {}
    pd_leaves = {it["part"] for it in plan.get("position_driven", [])}

    def walk(c):
        for ch in c.GetChildren():
            try:
                occ_by_leaf.setdefault(leaf_of(ch), []).append(ch)
            except Exception:
                pass
            walk(ch)
    walk(root)
    log("occurrences indexed: %d leaves" % len(occ_by_leaf))

    # ---------- 2. width-driven parts
    if cfg.get("do_width", True):
        done = set(state.get("stretched", []))
        for item in plan["width_driven"]:
            leaf = item["part"]
            rec = {"part": leaf, "instances": item.get("instances")}
            try:
                occs = occ_by_leaf.get(leaf)
                if not occs:
                    rec["status"] = "no occurrence"
                    report["width"].append(rec)
                    continue
                part = part_of(occs[0])
                origin, m = occs[0].GetPosition()
                axis, sign, conf = local_axis_for_world_y(m)
                rec["axis"] = "xyz"[axis]
                rec["axis_confidence"] = round(conf, 3)
                if leaf in done:
                    rec["status"] = "already stretched (expression-driven)"
                    rec["bbox_after"] = part_bbox(part)
                    report["width"].append(rec)
                    continue
                session.Parts.SetWork(part)
                pmark = session.SetUndoMark(NXOpen.Session.MarkVisibility.Invisible, "pilot-part")
                bb = part_bbox(part)
                rec["bbox_before"] = bb
                if bb is None:
                    rec["status"] = "no solid faces"
                    report["width"].append(rec)
                    continue
                mid = (bb[axis] + bb[axis + 3]) / 2.0
                e = [0.0, 0.0, 0.0]
                e[axis] = 1.0
                ensure_expression(part, "dw_half", "%s::half" % SKEL)
                bodies = [b for b in part.Bodies if b.IsSolidBody]
                n_ok, n_fb, errs, ftot = 0, 0, [], {"plus": 0, "minus": 0, "spanning": 0}
                for b in bodies:
                    faces = list(b.GetFaces())
                    boxes = {}
                    for f in faces:
                        boxes[f.Tag] = face_box(f)
                    plus, minus = [], []
                    for f in faces:
                        fb = boxes[f.Tag]
                        if fb is None:
                            continue
                        lo_, hi_ = fb[axis], fb[axis + 3]
                        if hi_ <= mid + 0.5:
                            minus.append(f)
                        elif lo_ >= mid - 0.5:
                            plus.append(f)
                        else:
                            ftot["spanning"] += 1
                    ftot["plus"] += len(plus)
                    ftot["minus"] += len(minus)
                    bmark = session.SetUndoMark(NXOpen.Session.MarkVisibility.Invisible, "pilot-body")
                    try:
                        if plus:
                            move_faces(part, plus, e, "dw_half", "plus")
                        if minus:
                            move_faces(part, minus, [-v for v in e], "dw_half", "minus")
                        n_ok += 1
                    except Exception as ex1:
                        try:
                            session.UndoToMark(bmark, None)   # drop the half-applied first attempt before the fallback
                        except Exception:
                            pass
                        # fallback: only the faces touching the two extremes of this body along the axis
                        bmax = max(v[axis + 3] for v in boxes.values() if v)
                        bmin = min(v[axis] for v in boxes.values() if v)
                        ends_plus = [f for f in faces if boxes[f.Tag] and boxes[f.Tag][axis + 3] >= bmax - 0.5 and boxes[f.Tag][axis] >= mid - 0.5]
                        ends_minus = [f for f in faces if boxes[f.Tag] and boxes[f.Tag][axis] <= bmin + 0.5 and boxes[f.Tag][axis + 3] <= mid + 0.5]
                        try:
                            if ends_plus:
                                move_faces(part, ends_plus, e, "dw_half", "plus_end")
                            if ends_minus:
                                move_faces(part, ends_minus, [-v for v in e], "dw_half", "minus_end")
                            n_ok += 1
                            n_fb += 1
                        except Exception as ex2:
                            errs.append("%s | fallback: %s" % (str(ex1)[:70], str(ex2)[:70]))
                rec["faces"] = ftot
                rec["bodies"] = len(bodies)
                rec["bodies_ok"] = n_ok
                rec["bodies_fallback"] = n_fb
                if errs:
                    rec["body_errors"] = errs[:3]
                session.UpdateManager.DoUpdate(mark)
                rec["bbox_after"] = part_bbox(part)
                if rec["bbox_after"]:
                    rec["length_before"] = round(bb[axis + 3] - bb[axis], 2)
                    rec["length_after"] = round(rec["bbox_after"][axis + 3] - rec["bbox_after"][axis], 2)
                if n_ok == len(bodies):
                    rec["status"] = "stretched" + (" (fallback on %d bodies)" % n_fb if n_fb else "")
                    done.add(leaf)
                    touched[part.Tag] = part
                else:
                    try:
                        session.UndoToMark(pmark, None)
                        rec["rolled_back"] = True
                    except Exception as ex3:
                        rec["rolled_back"] = "undo failed: %s" % str(ex3)[:60]
                    rec["status"] = "error (rolled back, manual): " + (errs[0] if errs else "no bodies")
                log("stretched %s axis %s +%d/-%d faces: %s -> %s" % (leaf, "xyz"[axis], len(plus), len(minus), rec.get("length_before"), rec.get("length_after")))
            except Exception as ex:
                rec["status"] = "error: %s" % str(ex)[:160]
                log("ERROR %s: %s" % (leaf, traceback.format_exc()[-400:]))
            report["width"].append(rec)
        state["stretched"] = sorted(done)
        session.Parts.SetWork(asm)

    # ---------- 3. one-sided components move as whole units (highest one-sided ancestor), plan["moves"]
    if cfg.get("do_position", True):
        applied = float(state.get("applied_dw", 0.0))
        step_half = (DW - applied) / 2.0
        moved_pairs = set()
        n_moved = n_dup = n_miss = n_err = 0
        occ_all = {}
        for lf, lst in occ_by_leaf.items():
            occ_all[lf] = []
            for o in lst:
                try:
                    po_, pm_ = o.GetPosition()
                    occ_all[lf].append((o, (po_.X, po_.Y, po_.Z)))
                except Exception:
                    pass
        for mv in plan.get("moves", []):
            leaf = mv["part"]
            want = mv.get("origin")
            cands = [o for o, org in occ_all.get(leaf, []) if want is None or (abs(org[0] - want[0]) + abs(org[1] - want[1]) + abs(org[2] - want[2]) < 1.5)]
            if not cands:
                n_miss += 1
                report["position"].append({"part": leaf, "status": "no occurrence at planned origin", "origin": want})
                continue
            for occ in cands:
                rec = {"part": leaf, "component": occ.DisplayName, "side": mv["side"], "basis": mv.get("basis", "bbox")}
                try:
                    parent = occ.Parent
                    owner = asm if (parent is None or parent.Tag == root.Tag) else part_of(parent)
                    origin, m = occ.GetPosition()
                    wv = [0.0, mv["side"] * step_half, 0.0]
                    if owner is asm:
                        lv, target = wv, occ
                    else:
                        po, pm_ = parent.GetPosition()
                        lv = world_to_local(pm_, wv)
                        dvec = [origin.X - po.X, origin.Y - po.Y, origin.Z - po.Z]
                        lo = world_to_local(pm_, dvec)
                        best, bestd = None, 1e9
                        for x in owner.ComponentAssembly.RootComponent.GetChildren():
                            try:
                                if leaf_of(x) != leaf:
                                    continue
                                xo, xm = x.GetPosition()
                                dd = ((xo.X - lo[0]) ** 2 + (xo.Y - lo[1]) ** 2 + (xo.Z - lo[2]) ** 2) ** 0.5
                                if dd < bestd:
                                    best, bestd = x, dd
                            except Exception:
                                continue
                        target = best
                        if target is None or bestd > 2.0:
                            rec["status"] = "not found in owner context"
                            n_err += 1
                            report["position"].append(rec)
                            continue
                    key = (owner.Tag, target.Tag)
                    if key in moved_pairs:
                        n_dup += 1
                        rec["status"] = "shared instance, already moved"
                        report["position"].append(rec)
                        continue
                    if abs(step_half) > 1e-6:
                        owner.ComponentAssembly.MoveComponent(target, NXOpen.Vector3d(float(lv[0]), float(lv[1]), float(lv[2])), identity())
                    moved_pairs.add(key)
                    touched[owner.Tag] = owner
                    rec["owner"] = owner.Leaf
                    rec["status"] = "moved"
                    n_moved += 1
                except Exception as ex:
                    n_err += 1
                    rec["status"] = "error: %s" % str(ex)[:160]
                    log("ERROR move %s: %s" % (leaf, str(ex)[:200]))
                report["position"].append(rec)
        state["applied_dw"] = DW
        log("moves: %d moved, %d shared-dup, %d missing, %d errors" % (n_moved, n_dup, n_miss, n_err))

    # ---------- 4. paper path band follows dW
    try:
        for p in session.Parts:
            if p.Leaf == "纸路":
                session.Parts.SetWork(p)
                e = p.Expressions.FindObject("p61")
                mmu = p.UnitCollection.FindObject("MilliMeter")
                p.Expressions.EditWithUnits(e, mmu, "650+%s::half" % SKEL)
                e0 = p.Expressions.FindObject("p60")
                p.Expressions.EditWithUnits(e0, mmu, "100-%s::half" % SKEL)
                touched[p.Tag] = p
                report["paper_path"] = "p60 = 100-%s::half ; p61 = 650+%s::half (band stays centred)" % (SKEL, SKEL)
                session.Parts.SetWork(asm)
                break
    except Exception as ex:
        report["errors"].append("paper path: %s" % str(ex)[:120])
        session.Parts.SetWork(asm)

    session.UpdateManager.DoUpdate(mark)
    log("update done")
    # ---------- 5. save everything under the copy folder
    if cfg.get("save", True):
        saved, failed = 0, 0
        for p in list(session.Parts):
            try:
                if p.FullPath.lower().startswith(COPY.lower()) and (p.IsModified or p.Tag in touched):
                    p.Save(NXOpen.BasePart.SaveComponents.FalseValue, NXOpen.BasePart.CloseAfterSave.FalseValue)
                    saved += 1
            except Exception as ex:
                failed += 1
                log("save failed %s: %s" % (p.Leaf, str(ex)[:120]))
        try:
            asm.Save(NXOpen.BasePart.SaveComponents.TrueValue, NXOpen.BasePart.CloseAfterSave.FalseValue)
        except Exception as ex:
            log("asm save: %s" % str(ex)[:120])
        report["saved"], report["save_failed"] = saved, failed
        log("saved %d parts (%d failed)" % (saved, failed))
        with open(state_path, "w", encoding="utf-8") as f:
            json.dump(state, f, ensure_ascii=False, indent=1)
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(report, f, ensure_ascii=False, indent=1, default=str)
    log("report written")
    try:
        session.Parts.CloseAll(NXOpen.BasePart.CloseModified.CloseModified, None)
    except Exception:
        pass
    log("done")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        tb = traceback.format_exc()
        log("FATAL " + tb)
        try:
            with open(OUT + ".FATAL.txt", "w", encoding="utf-8") as f:
                f.write(tb)
        except Exception:
            pass
        print(tb)
