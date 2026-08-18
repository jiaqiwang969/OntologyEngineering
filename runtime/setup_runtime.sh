#!/usr/bin/env bash
# 工程本体论 skill 的可执行佐证运行时。
# 用途：为 demos/ 里的「书中论断 → 代码执行 → 佐证结论」示例准备 Python 环境。
# 依赖锁定版本，避免 semantica 0.6.x API 变动破坏 demo。
set -euo pipefail

RUNTIME_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV="$RUNTIME_DIR/.venv"

if [ ! -d "$VENV" ]; then
  python3 -m venv "$VENV"
fi
"$VENV/bin/pip" install --quiet --upgrade pip
"$VENV/bin/pip" install --quiet -r "$RUNTIME_DIR/requirements.txt"

echo "runtime ready: $VENV"
echo "run demos with: $VENV/bin/python demos/<demo>.py"
