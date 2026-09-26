#!/bin/sh
# Windows 渲染节点模板（dell-nb：Blender 5.2 portable，PowerShell over ssh，只有 scp）。来源：OLSK v009（2026-09-05）。
# 1) 投放：scp assembly_source.blend / state-chain.json / tools/render_film_generic.py / robot-config.v1.json 到 C:\olsk\case\...（目录结构与本机一致）。
# 2) 启动必须“脱离 ssh 会话”：Start-Process 会随会话结束被杀；用 Win32_Process Create：
#    Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{CommandLine='cmd /c "C:\olsk\bl\blender-5.2.0-windows-x64\blender.exe -b --factory-startup -P C:\olsk\case\tools\render_film_generic.py -- --case-dir C:\olsk\case --run FILM_vXXX --from A --to B --samples 96 > C:\olsk\case\render.log 2>&1"'}
# 3) 进度：Test-Path 末帧；(Get-ChildItem frames_raw | Measure-Object).Count。
# 4) 回传：scp 通配 "jiaqi@host:C:/olsk/case/装配动画/RUN/frames_raw/r_0[89]*.png"，核数 ≥ 段长再 touch logs/dellnb_frames_done（主控只认这个标记）。
cd "${CASE_DIR:?set CASE_DIR to the OLSK work directory}" || exit 1; RUN=$1; LAST=$2; GLOB=$3; NEED=$4
WIN_HOST=$(fleet resolve dell-nb) || exit 1; [ -n "$WIN_HOST" ] || exit 1
SSHNB="ssh -o UserKnownHostsFile=$HOME/.config/fleet/known_hosts -o HostKeyAlias=fleet-dell-nb -i $HOME/.ssh/id_ed25519 -o IdentitiesOnly=yes -o ConnectTimeout=20"
until $SSHNB jiaqi@$WIN_HOST "Test-Path \"C:\\olsk\\case\\装配动画\\$RUN\\frames_raw\\r_$LAST.png\"" 2>/dev/null | grep -q True; do sleep 120; done
scp -q -o UserKnownHostsFile=$HOME/.config/fleet/known_hosts -o HostKeyAlias=fleet-dell-nb -i $HOME/.ssh/id_ed25519 -o IdentitiesOnly=yes "jiaqi@$WIN_HOST:C:/olsk/case/装配动画/$RUN/frames_raw/$GLOB" 装配动画/$RUN/frames_raw/ 2>/dev/null
n=$(ls 装配动画/$RUN/frames_raw/$GLOB 2>/dev/null | wc -l | tr -d ' '); [ "$n" -ge "$NEED" ] && touch logs/dellnb_frames_done || echo PULL_INCOMPLETE
