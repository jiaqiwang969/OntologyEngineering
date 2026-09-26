#!/usr/bin/env bash
# Retired from the NX direct default; no automatic bridge operation.
if [ "${CAD_AGENT_LEGACY_CAD:-}" != "explicit" ]; then
  echo "Retired CAD MCP service route. Use scripts/nx_direct.py; explicit historical use requires CAD_AGENT_LEGACY_CAD=explicit." >&2
  exit 64
fi
# Drive the NX_MCP bridge (headless NX 2412 session) on dell-nb through scripts/dell_ssh.sh (or NX_CALL_SSH).
#
#   nx_bridge.sh start [journal.py]   start the batch bridge (default journal: NX_MCP\examples\start_nx_bridge.py;
#                                     pass the Dell user's work\nxmcp\start_nx_bridge_gui.py to run the GUI variant in batch)
#   nx_bridge.sh stop                 write the stop flag and wait for the NX session to exit
#   nx_bridge.sh status               descriptor, loopback port check, run_journal/ugraf processes
#   nx_bridge.sh restart              stop then start
#   nx_bridge.sh log [n]              last n lines (default 40) of bridge.log
#
# The bundled SSH helper resolves the notebook through fleet on each call.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
SSH="${NX_CALL_SSH:-$HERE/dell_ssh.sh}"
[ -x "$SSH" ] || SSH="$HOME/.local/bin/dell-ssh.sh"
NXMCP='$env:USERPROFILE\work\nxmcp'
PS='powershell -NoProfile -ExecutionPolicy Bypass -File'
decode() { iconv -f GBK -t UTF-8 -c | LC_ALL=C tr -d '\r'; }

case "${1:-}" in
  start)
    journal="${2:-}"
    if [ -n "$journal" ]; then
      "$SSH" "$PS \"$NXMCP\\start-bridge.ps1\" '$journal'" | decode
    else
      "$SSH" "$PS \"$NXMCP\\start-bridge.ps1\"" | decode
    fi ;;
  stop)    "$SSH" "$PS \"$NXMCP\\stop-bridge.ps1\"" | decode ;;
  status)  "$SSH" "$PS \"$NXMCP\\status.ps1\"" | LC_ALL=C tr -d '\r' ;;
  restart) "$0" stop; "$0" start "${2:-}" ;;
  log)     n="${2:-40}"; "$SSH" "Get-Content \"$NXMCP\\bridge.log\" -Tail $n" | decode ;;
  *) sed -n '2,12p' "$0"; exit 1 ;;
esac
