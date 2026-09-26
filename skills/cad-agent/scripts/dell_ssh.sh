#!/usr/bin/env bash
# Route each Dell notebook call through live fleet identity and host-key checks.
# Usage: dell_ssh.sh [remote command...]
set -euo pipefail
if [[ -n "${DELL_NB_IP:-}" ]]; then
  echo "DELL_NB_IP bypasses fleet identity resolution; unset it and use fleet dell-nb" >&2
  exit 2
fi
exec fleet ssh dell-nb "$@"
