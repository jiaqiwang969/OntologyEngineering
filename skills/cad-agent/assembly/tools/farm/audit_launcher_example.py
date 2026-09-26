import subprocess, os, sys, time
from pathlib import Path
CASE=Path(os.environ.get('REBUILD_CASE_DIR', str(Path.home()/'olsk_film'))); RUN=sys.argv[1]; N=int(sys.argv[2])
ops=[l.strip() for l in (CASE/f'logs/audit_ops_{RUN}.txt').read_text().splitlines() if l.strip()]
LOGD=CASE/f'logs/audit_{RUN}'; LOGD.mkdir(parents=True, exist_ok=True)
env=dict(os.environ, REBUILD_CASE_DIR=str(CASE), OPENBLAS_NUM_THREADS='1', OMP_NUM_THREADS='1')
PY=os.environ.get('OLSK_WORKER_PYTHON', str(Path.home()/'olsk_walk_venv/bin/python'))
jobs=[]
for op in ops:
    jobs.append(('pen', op, [PY,'tools/audit_path_penetration_generic.py','--op',op,'--run',RUN,'--scene','film','--geom','real']))
    jobs.append(('sib', op, [PY,'tools/audit_sibling_paths_generic.py','--op',op,'--run',RUN]))
    jobs.append(('vis', op, [PY,'tools/visual_overlap_scan.py','--op',op,'--run',RUN]))
running=[]
while jobs or running:
    running=[p for p in running if p.poll() is None]
    while jobs and len(running)<N:
        kind,op,cmd=jobs.pop(0)
        lf=open(LOGD/f'{kind}_{op}.log','w')
        running.append(subprocess.Popen(cmd, cwd=str(CASE), env=env, stdout=lf, stderr=subprocess.STDOUT))
    time.sleep(3)
(LOGD/'DONE').write_text('AUDITS_DONE\n')
