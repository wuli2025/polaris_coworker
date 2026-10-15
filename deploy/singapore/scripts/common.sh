#!/usr/bin/env bash
set -euo pipefail
SG_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SG_ENV_FILE=${SG_ENV_FILE:-$SG_ROOT/.env}
export SG_ENV_FILE
umask 077
value() { python3 "$SG_ROOT/scripts/config.py" value "$SG_ENV_FILE" "$1"; }
dc() {
  python3 "$SG_ROOT/scripts/config.py" compose "$SG_ENV_FILE" "$@"
}
prepare() { python3 "$SG_ROOT/scripts/config.py" prepare "$SG_ENV_FILE"; }
lock() {
  mkdir -p "$SG_ROOT/.generated"
  exec 9>"$SG_ROOT/.generated/operation.lock"
  flock -n 9 || { echo 'BLOCKED: 另一维护操作正在执行' >&2; exit 1; }
}
require_docker() { docker compose version >/dev/null; dc config --quiet; }
require_pin() {
  [[ -n $(value EXPECTED_FINGERPRINT) ]] || { echo 'BLOCKED: 先填写 EXPECTED_FINGERPRINT' >&2; exit 1; }
}
# 维护过程中只恢复原来运行的权威，不启动原来停止的服务。
restart_original() {
  if [[ ${SG_WAS_RUNNING:-0} == 1 ]]; then dc start authority >/dev/null; fi
}
stop_authority() {
  SG_CONTAINER=$(dc ps -aq authority)
  [[ -n $SG_CONTAINER ]] || { echo 'BLOCKED: 尚无 authority 容器' >&2; return 1; }
  SG_WAS_RUNNING=0
  if [[ $(docker inspect -f '{{.State.Running}}' "$SG_CONTAINER") == true ]]; then
    SG_WAS_RUNNING=1
  fi
  dc stop -t 30 authority >/dev/null
  [[ $(docker inspect -f '{{.State.Running}}' "$SG_CONTAINER") == false ]]
}
backup_authority() {
  local target scratch
  target=$(value BACKUP_DIR)
  [[ $target = /* ]] || target="$SG_ROOT/$target"
  mkdir -p "$target"
  target=$(cd "$target" && pwd)
  scratch=$(mktemp -d "$target/.snapshot.XXXXXX")
  # 停服务后复制，SQLite backup API 归并 WAL；失败不产出成功标记。
  stop_authority
  trap 'restart_original' EXIT
  if ! docker cp "$SG_CONTAINER:/home/polaris/Polaris/data/." "$scratch/"; then
    echo "备份读取失败；临时目录含敏感数据：$scratch" >&2
    return 1
  fi
  python3 "$SG_ROOT/scripts/snapshot.py" pack "$scratch" "$target" \
    "$(docker inspect -f '{{.Config.Image}}' "$SG_CONTAINER")"
  rm -rf -- "$scratch"
  restart_original
  SG_WAS_RUNNING=0
  trap - EXIT
  echo '备份含未加密签名私钥；请立即转存加密介质，勿上传明文。' >&2
}
