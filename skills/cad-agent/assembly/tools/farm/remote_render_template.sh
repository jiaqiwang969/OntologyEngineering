#!/bin/sh
# 在 dell-7920（RTX 5880，Blender 5.2 portable）上渲染指定 run 的 assembly_source.blend，帧回传本机。用法：remote_render.sh FILM_v002 [--every N] [--from A --to B]
RUN=$1; shift
LOCAL=${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}
REMOTE=/data/olsk_film   # 用数据盘，不用系统盘（2026-09-05 系统盘被日志占满导致“12 帧”假完成）
HOST=$(fleet resolve dell-7920) || exit 1; [ -n "$HOST" ] || exit 1
SSHOPT="ssh -o UserKnownHostsFile=$HOME/.config/fleet/known_hosts -o HostKeyAlias=fleet-dell-7920 -i $HOME/.ssh/id_ed25519 -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new"
fleet ssh dell-7920 "mkdir -p $REMOTE/装配动画/$RUN $REMOTE/tools" </dev/null
rsync -a -e "$SSHOPT" "$LOCAL/装配动画/$RUN/assembly_source.blend" "$LOCAL/装配动画/$RUN/state-chain.json" "dell@$HOST:$REMOTE/装配动画/$RUN/" </dev/null
rsync -a -e "$SSHOPT" "$LOCAL/tools/render_film_generic.py" "$LOCAL/robot-config.v1.json" "dell@$HOST:$REMOTE/" </dev/null
fleet ssh dell-7920 "cd $REMOTE && mv render_film_generic.py tools/ 2>/dev/null; ~/blender-5.2.0-linux-x64/blender -b --factory-startup -P tools/render_film_generic.py -- --case-dir $REMOTE --run $RUN $* > render_$RUN.log 2>&1; tail -2 render_$RUN.log; ls 装配动画/$RUN/frames_raw | wc -l" </dev/null
mkdir -p "$LOCAL/装配动画/$RUN/frames_raw"
rsync -a -e "$SSHOPT" "dell@$HOST:$REMOTE/装配动画/$RUN/frames_raw/" "$LOCAL/装配动画/$RUN/frames_raw/" </dev/null
echo "REMOTE_RENDER_DONE $RUN $(ls "$LOCAL/装配动画/$RUN/frames_raw" | wc -l) frames $(date +%T)"
