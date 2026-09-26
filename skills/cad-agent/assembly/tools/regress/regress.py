#!/usr/bin/env python3
"""工具链回归：--compile 编译全部工具；--case <id> 比对 cases.json 里登记的产物摘要；--record <id> 录入当前摘要。"""
import argparse, hashlib, json, os, py_compile, sys
from pathlib import Path
HERE = Path(__file__).resolve().parent; TOOLS = HERE.parent
ap = argparse.ArgumentParser(); ap.add_argument("--compile", action="store_true"); ap.add_argument("--case"); ap.add_argument("--record")
ap.add_argument("--artifacts", nargs="*", default=["本体/S7-可播接近距离.v1.json", "本体/S8-连接关系图.v1.json", "本体/S3-工序实例绑定.v1.json"])
a = ap.parse_args(); rc = 0
if a.compile:
    bad = []
    for p in sorted(TOOLS.rglob("*.py")):
        if ".venv" in p.parts or "__pycache__" in p.parts: continue
        try: py_compile.compile(str(p), doraise=True)
        except Exception as e: bad.append(f"{p.relative_to(TOOLS)}: {e}")
    print(f"compiled {len(list(TOOLS.rglob('*.py')))} files, errors {len(bad)}"); [print("  ", b) for b in bad]; rc |= bool(bad)
reg = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
if a.record or a.case:
    cid = a.record or a.case; c = next((x for x in reg["cases"] if x["id"] == cid), None)
    if c is None: print("unknown case", cid); sys.exit(2)
    case_root = os.environ.get("CAD_REGRESSION_CASE_ROOT")
    if not case_root:
        print("set CAD_REGRESSION_CASE_ROOT to the directory containing the registered cases")
        sys.exit(2)
    cd = Path(case_root).expanduser().resolve() / c["case_dir"]
    cur = {art: (sha(cd / art) if (cd / art).is_file() else None) for art in a.artifacts}
    if a.record:
        c["artifacts"] = {k: v for k, v in cur.items() if v}; (HERE / "cases.json").write_text(json.dumps(reg, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"); print("recorded", c["artifacts"])
    else:
        exp = c.get("artifacts", {})
        if not exp: print("no baseline recorded for", cid, "(open-item O-24)"); sys.exit(3)
        diff = {k: (exp[k], cur.get(k)) for k in exp if exp[k] != cur.get(k)}
        print("regression", "PASS" if not diff else "FAIL", diff); rc |= bool(diff)
sys.exit(rc)
