#!/usr/bin/env bash
# Retired from the NX direct default; no automatic bridge operation.
if [ "${CAD_AGENT_LEGACY_CAD:-}" != "explicit" ]; then
  echo "Retired CAD MCP service route. Use scripts/nx_direct.py; explicit historical use requires CAD_AGENT_LEGACY_CAD=explicit." >&2
  exit 64
fi
# Manage the AutoCAD MCP service on dell-nb from the Mac.
#   autocad_service.sh status      health: task state, relay listener, acad process, log tails
#   autocad_service.sh start       (re)register + start the CadMcpRelay task, start AutoCAD if not running
#   autocad_service.sh stop        stop relay task and relay/server pythons (AutoCAD keeps running)
#   autocad_service.sh restart     stop + start relay
#   autocad_service.sh start-acad  only launch AutoCAD visibly on the desktop (scheduled task CadMcp-StartAcad)
#   autocad_service.sh logs        last 40 lines of relay / server logs
# Env: AUTOCAD_CALL_SSH overrides the fleet-routed helper path.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
SSH="${AUTOCAD_CALL_SSH:-$HERE/dell_ssh.sh}"
[ -x "$SSH" ] || SSH="$HOME/.local/bin/dell-ssh.sh"
BASE='$env:USERPROFILE\work\cadmcp'
PS='powershell -NoProfile -ExecutionPolicy Bypass -File'
UTF8='[Console]::OutputEncoding=[Text.Encoding]::UTF8;'

run() { "$SSH" "$UTF8 $1" | LC_ALL=C tr -d '\r'; }

case "${1:-}" in
  status)     run "$PS \"$BASE\\cadmcp-status.ps1\"" ;;
  start)      run "$PS \"$BASE\\cadmcp-install-task.ps1\"; if (-not (Get-Process acad -ErrorAction SilentlyContinue)) { Start-ScheduledTask -TaskName CadMcp-StartAcad; 'AutoCAD start requested (allow ~60 s)' } else { 'AutoCAD already running' }" ;;
  stop)       run "$PS \"$BASE\\cadmcp-stop.ps1\"" ;;
  restart)    run "$PS \"$BASE\\cadmcp-restart.ps1\"" ;;
  start-acad) run "Start-ScheduledTask -TaskName CadMcp-StartAcad; Start-Sleep 3; Get-Process acad -ErrorAction SilentlyContinue | Select-Object Id,SessionId,Responding | Format-Table -AutoSize | Out-String" ;;
  logs)       run "Get-Content \"$BASE\\logs\\relay.log\" -Tail 40 -ErrorAction SilentlyContinue; '--- server'; Get-Content \"$BASE\\logs\\acad_mcp_server.log\" -Tail 40 -ErrorAction SilentlyContinue; '--- stderr'; Get-Content \"$BASE\\logs\\server_stderr.log\" -Tail 20 -ErrorAction SilentlyContinue" ;;
  *) sed -n '2,9p' "$0"; exit 2 ;;
esac
