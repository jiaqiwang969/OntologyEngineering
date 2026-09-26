#!/bin/sh
# v005 收尾：验收包 + 验证器（engineering-film）
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
export REBUILD_CASE_DIR=$(pwd)
.venv/bin/python tools/build_acceptance_packet.py --case-dir . --run FILM_v005 > logs/v005_packet.log 2>&1; tail -3 logs/v005_packet.log
PK=$(ls -t 装配动画/FILM_v005/*acceptance*packet*.json 2>/dev/null | head -1); echo "packet: $PK"
python3 validator/validate_acceptance.py --contract validator/acceptance-contract.v1.1.json --profile engineering-film --root . "$PK" > logs/v005_validate.log 2>&1; echo "validator exit $?"; tail -25 logs/v005_validate.log
echo "V005_FINISH_DONE $(date +%T)"
