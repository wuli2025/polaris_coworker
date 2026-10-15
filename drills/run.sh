#!/usr/bin/env bash
set -euo pipefail
DRILL_REPO=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if [[ $# -lt 1 ]]; then
  printf '%s\n' '{"drill":"unknown","step":"arguments","status":"blocked","evidence":{"reason":"Usage: bash drills/run.sh D1|D2|D3|D4|D6 [--only STEP] [--keep] [--repeat N]"},"durationMs":0}'
  exit 2
fi
DRILL_NAME=${1^^}
shift
case "$DRILL_NAME" in
  D1|D2|D3|D4|D6) ;;
  *)
    printf '%s\n' '{"drill":"unknown","step":"arguments","status":"blocked","evidence":{"reason":"Unknown drill (D5 deferred)"},"durationMs":0}'
    exit 2
    ;;
esac
if ! command -v node >/dev/null 2>&1; then
  printf '%s\n' '{"drill":"unknown","step":"prerequisites","status":"blocked","evidence":{"reason":"Node >=22.19 required"},"durationMs":0}'
  exit 2
fi
umask 077
exec node "$DRILL_REPO/drills/${DRILL_NAME,,}.mjs" "$@"
