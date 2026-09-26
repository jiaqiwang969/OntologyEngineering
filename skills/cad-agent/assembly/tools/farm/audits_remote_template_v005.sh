#!/bin/sh
# OLSK v005 remote audit example. Resolve the host on each run; require an explicit local project root.
set -eu
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT"
RUN=${1:-FILM_v005}
case "$RUN" in ''|*[!A-Za-z0-9_.-]*) echo "invalid run name" >&2; exit 2;; esac
PAR=${OLSK_AUDIT_PARALLEL:-90}
case "$PAR" in ''|0|*[!0-9]*) echo "OLSK_AUDIT_PARALLEL must be a positive integer" >&2; exit 2;; esac
HOST=$(fleet resolve dell-7920)
[ -n "$HOST" ] || { echo "fleet could not resolve dell-7920" >&2; exit 2; }
REMOTE="dell@$HOST"
SSHOPT="ssh -o UserKnownHostsFile=$HOME/.config/fleet/known_hosts -o HostKeyAlias=fleet-dell-7920 -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes"
SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
LAUNCHER=${OLSK_AUDIT_LAUNCHER:-$SCRIPT_DIR/audit_launcher_example.py}
if [ -z "${WAIT_ONLY:-}" ]; then
  [ -f "$LAUNCHER" ] || { echo "audit launcher missing: $LAUNCHER" >&2; exit 2; }
  rsync -a -e "$SSHOPT" tools robot-config.v1.json 本体/S7-可播接近距离.v2.json "$REMOTE:~/olsk_film/"
  $SSHOPT "$REMOTE" "mkdir -p ~/olsk_film/装配动画/$RUN ~/olsk_film/本体/S7-穿模审计 ~/olsk_film/logs/audit_$RUN"
  rsync -a -e "$SSHOPT" "装配动画/$RUN/state-chain.json" "装配动画/$RUN/pipeline-state.v1.json" "$REMOTE:~/olsk_film/装配动画/$RUN/"
  .venv/bin/python -c "import json;print('\n'.join(c['op'] for c in json.load(open('装配动画/$RUN/state-chain.json'))['chapters'] if c.get('movers')))" > "logs/audit_ops_$RUN.txt"
  rsync -a -e "$SSHOPT" "logs/audit_ops_$RUN.txt" "$REMOTE:~/olsk_film/logs/"
  rsync -a -e "$SSHOPT" "$LAUNCHER" "$REMOTE:~/olsk_film/audit_launcher.py"
  $SSHOPT "$REMOTE" "cd ~/olsk_film && rm -f logs/audit_$RUN/DONE && (nohup ~/olsk_walk_venv/bin/python audit_launcher.py $RUN $PAR > logs/audit_$RUN.launcher.log 2>&1 &); echo launched"
fi
until $SSHOPT "$REMOTE" "test -f ~/olsk_film/logs/audit_$RUN/DONE"; do sleep 60; done
rsync -a -e "$SSHOPT" "$REMOTE:~/olsk_film/本体/S7-穿模审计/" 本体/S7-穿模审计/ --include="*.$RUN.*" --exclude="*"
mkdir -p "logs/audit_$RUN"
rsync -a -e "$SSHOPT" "$REMOTE:~/olsk_film/logs/audit_$RUN/" "logs/audit_$RUN/"
echo "V005_AUDITS_DONE $(ls 本体/S7-穿模审计/*.$RUN.film.real.json | wc -l) pen, $(ls 本体/S7-穿模审计/*.$RUN.siblings.json | wc -l) sib, $(ls 本体/S7-穿模审计/*.$RUN.visual.json | wc -l) vis $(date +%T)"
