"""NX-owned Python entry. Custom journal exposes run(session, job_dir, params)."""
import hashlib
import json
import os
from pathlib import Path
import runpy
import time
import traceback

job = Path(os.environ["CAD_AGENT_NX_JOB"]).resolve()
spec = json.loads((job / "job.json").read_text(encoding="utf-8"))
result = {"schema": "cad-agent.nx-direct-result/v1", "job_id": spec["job_id"], "pid": os.getpid(), "started_at": time.time(), "journal_sha256": spec["files"]["journal.py"]["sha256"], "status": "error"}
try:
    import NXOpen
    session = NXOpen.Session.GetSession()
    # Each subprocess starts with no customer model; inputs are task-owned copies.
    if list(session.Parts):
        raise RuntimeError("Batch session unexpectedly contains open parts")
    (job / "out").mkdir(exist_ok=False)
    module = runpy.run_path(str(job / "journal.py"), run_name="cad_agent_reviewed_journal")
    data = module["run"](session, job, json.loads((job / "params.json").read_text(encoding="utf-8")))
    result.update(status="ok", readback=data)
except BaseException:
    result["error"] = traceback.format_exc()
finally:
    result["finished_at"] = time.time()
    result["outputs"] = [{"path": str(p.relative_to(job)).replace("\\", "/"), "bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted((job / "out").rglob("*")) if p.is_file()]
    tmp = job / "native-result.json.tmp"
    tmp.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    os.replace(str(tmp), str(job / "native-result.json"))
