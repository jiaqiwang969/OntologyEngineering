#!/usr/bin/env python3
"""通用保活式农场启动器（K-GOV-16）。

为什么不用 xargs -P：dell-7920 实测 `xargs -P 96 -I{}` 只维持 ~25 个并发。本脚本自己维持 N 个子进程，
跳过已有日志的作业（可与其它启动器共存、可重启续跑），结束后写 DONE 标记。

用法：
  farm_launcher.py --case-dir <case> --jobs <jobs.txt> --n 100 --log-dir logs/xxx \
      --python /path/python --cmd 'tools/sweep_sequence_reverse_generic.py --case-dir {case} --op {job} --cap 150 --out-dir 本体/S7-序列感知认证-延长150'
  jobs.txt 每行一个作业参数串（如 `WB-56.1 --chunk 3:4`），{job} 原样展开为多个参数；{case} 展开为 case 目录。
  并发核实：另开终端 `pgrep -c -f "<cmd 关键词>"` 与 `cat /proc/loadavg`，不得只信设定值。
"""
import argparse, os, shlex, subprocess, time
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--case-dir", required=True)
ap.add_argument("--jobs", required=True)
ap.add_argument("--n", type=int, default=64)
ap.add_argument("--log-dir", required=True)
ap.add_argument("--python", default="python3")
ap.add_argument("--cmd", required=True, help="命令模板，含 {case} 与 {job}")
ap.add_argument("--reverse", action="store_true", help="逆序取作业（与另一启动器共存时减少碰撞）")
ap.add_argument("--mem-reserve-frac", type=float, default=0.15,
                help="MemAvailable 低于总内存的这个比例时不再起新作业（OLSK 实测：90 个审计进程把 503 GB 吃满，sshd 都不响应）")
a = ap.parse_args()

def mem_ok():
    try:
        info = {}
        for line in open("/proc/meminfo"):
            k, v = line.split(":")[0], line.split(":")[1].strip().split()[0]
            info[k] = int(v)
        return info["MemAvailable"] / info["MemTotal"] > a.mem_reserve_frac
    except Exception:
        return True
CASE = Path(a.case_dir).resolve(); LOGD = Path(a.log_dir); LOGD.mkdir(parents=True, exist_ok=True)
jobs = [l.strip() for l in Path(a.jobs).read_text(encoding="utf-8").splitlines() if l.strip()]
if a.reverse: jobs = jobs[::-1]
env = dict(os.environ, REBUILD_CASE_DIR=str(CASE), OPENBLAS_NUM_THREADS="1", OMP_NUM_THREADS="1")
def logname(j): return LOGD / (j.replace(" ", "_").replace(":", "_").replace("/", "_") + ".log")
running, started = [], 0
while jobs or running:
    running = [p for p in running if p.poll() is None]
    while jobs and len(running) < a.n and mem_ok():
        j = jobs.pop(0)
        if logname(j).exists(): continue
        cmd = [a.python] + shlex.split(a.cmd.replace("{case}", str(CASE)).replace("{job}", j))
        with open(logname(j), "w") as lf:
            running.append(subprocess.Popen(cmd, cwd=str(CASE), env=env, stdout=lf, stderr=subprocess.STDOUT))
        started += 1
    time.sleep(5)
(LOGD / "DONE").write_text(f"FARM_DONE started={started}\n", encoding="utf-8")
print(f"farm done: started {started}")
