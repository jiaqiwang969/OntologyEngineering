import subprocess, os, time, sys
from pathlib import Path
CASE=Path(os.environ.get('REBUILD_CASE_DIR', str(Path.home()/'olsk_film'))); LOGD=CASE/'logs/walk150'; OUTD=CASE/'本体/S7-序列感知认证-延长150'
jobs=[l.strip() for l in (CASE/'jobs.txt').read_text().splitlines() if l.strip()][::-1]
N=int(sys.argv[1]) if len(sys.argv)>1 else 88
running=[]
def logname(j): return LOGD/(j.replace(' ','_').replace(':','_')+'.log')
env=dict(os.environ, REBUILD_CASE_DIR=str(CASE), OPENBLAS_NUM_THREADS='1', OMP_NUM_THREADS='1')
started=0
while jobs or running:
    running=[p for p in running if p.poll() is None]
    while jobs and len(running)<N:
        j=jobs.pop(0)
        if logname(j).exists(): continue   # 已由 xargs 启动/完成
        cmd=[os.environ.get('OLSK_WORKER_PYTHON', str(Path.home()/'olsk_walk_venv/bin/python')),'tools/sweep_sequence_reverse_generic.py','--case-dir',str(CASE),'--op']+j.split()+['--cap','150','--fine-until','20','--coarse-step','1.0','--out-dir','本体/S7-序列感知认证-延长150']
        with open(logname(j),'w') as lf:
            running.append(subprocess.Popen(cmd, cwd=str(CASE), env=env, stdout=lf, stderr=subprocess.STDOUT))
        started+=1
    time.sleep(5)
(LOGD/'DONE2').write_text(f'LAUNCHER_DONE started={started}\n')
