#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
# 可在外部探测机运行，不要求 Docker；不会生成密钥或改变配置。
python3 "$SG_ROOT/scripts/probe.py" smoke "$SG_ENV_FILE"
