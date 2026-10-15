#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
lock
prepare
echo '已校验输入并生成配置；未启动服务。'
if docker compose version >/dev/null 2>&1; then
  dc config --quiet
  echo 'docker compose config: PASS'
else
  echo 'Docker 不可用；请运行 tests/test_static.py，目标环境仍须 compose config。' >&2
fi
