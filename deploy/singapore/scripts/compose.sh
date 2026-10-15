#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
[[ $# -gt 0 ]] || { echo '用法：compose.sh <compose 子命令及参数>' >&2; exit 2; }
lock
prepare
dc "$@"
