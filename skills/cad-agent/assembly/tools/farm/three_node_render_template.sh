#!/bin/sh
# 三机分帧渲染模板（K-GOV-16 并发核实 + 尾段再平衡）。来源：OLSK v009（2026-09-05）。
# 角色：本机（Mac, ~1 s/帧）渲 1..H 再渲 T0..T1；GPU 节点 dell-7920（~1.5 s/帧）渲 H+1..S 到分界即改渲 T1+1..N；Windows 节点 dell-nb（~2 s/帧）渲 S+1..T0-1，由 windows_node_render_template.sh 回传并 touch logs/<node>_frames_done。
# 分界按速率解方程：各机同时收尾；实测 43/min（dell）与 32/min（本机）时，2948 帧尾段按 2200/748 分。
# 关键教训：不要在“杀远端 blender 触发回传”的链上再挂合成——先把所有帧齐到本机并逐帧核数（缺帧=0）再合成。
# 关键教训 2：远端 pgrep/pkill -f 的模式必须锚定可执行路径（^/home/<remote-user>/.../blender …），否则会匹配到承载它的 ssh shell 自身：v009 的 job2 守卫 `pgrep -f "from 10000 --to 10747"` 自匹配成功，导致第二实例从未启动。
# 关键教训 3：异地时 dell 地址按 LAN→WireGuard 逐个探测（resolve），dell-nb/jx 经 dell 跳板 ProxyCommand（-J 会因跳板主机键别名缺失而失败）；回传与渲染并行（rsync 增量，完成后再最终 rsync 一次修正半写文件）。
cd "$CASE_DIR"; RUN=$1; N=$2; H=$3; S=$4; T0=$5; T1=$6; LOG=logs/${RUN}_master.log
HOST=$(fleet resolve dell-7920) || exit 1; [ -n "$HOST" ] || exit 1
SSHD="ssh -o UserKnownHostsFile=$HOME/.config/fleet/known_hosts -o HostKeyAlias=fleet-dell-7920 -i $HOME/.ssh/id_ed25519 -o IdentitiesOnly=yes -o ConnectTimeout=20"
R=/data/olsk_film/装配动画/$RUN/frames_raw; F=装配动画/$RUN/frames_raw
until $SSHD dell@$HOST "test -f $R/r_$(printf %05d $S).png" 2>/dev/null; do sleep 30; done
$SSHD dell@$HOST "pkill -f '[/]blender-5.2.0-linux-x64/blender'; sleep 3; cd /data/olsk_film && (nohup ~/blender-5.2.0-linux-x64/blender -b --factory-startup -P tools/render_film_generic.py -- --case-dir /data/olsk_film --run $RUN --from $((T1+1)) --to $N --samples 96 > render_${RUN}_tail.log 2>&1 &)" >> $LOG 2>&1
until [ -f $F/r_$(printf %05d $H).png ]; do sleep 30; done
/opt/homebrew/bin/blender -b --factory-startup -P tools/render_film_generic.py -- --case-dir . --run $RUN --from $T0 --to $T1 --samples 96 > logs/${RUN}_render_local2.log 2>&1
until [ -f logs/dellnb_frames_done ]; do sleep 30; done
until $SSHD dell@$HOST "test -f $R/r_$(printf %05d $N).png" 2>/dev/null; do sleep 30; done; sleep 5
rsync -a -e "$SSHD" "dell@$HOST:$R/" $F/
MISSING=$(.venv/bin/python -c "import os;print(sum(1 for i in range(1,$N+1) if not os.path.exists('$F/r_%05d.png'%i)))")
echo "frames check: missing=$MISSING $(date +%T)" >> $LOG; [ "$MISSING" = "0" ] || { echo FRAMES_MISSING >> $LOG; exit 1; }
# 之后：合成→配乐→faststart→媒体 QC→复看请求→验收包→验证器（见 chain_template_v005.sh 合成段与 finish_template.sh）
