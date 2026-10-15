#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
[[ $# == 1 ]] || { echo '用法：restore.sh /绝对路径/snapshot.tar[.age|.gpg]' >&2; exit 2; }
SG_ARCHIVE=$(realpath "$1")
lock
prepare
SG_EXPECTED=$(value EXPECTED_FINGERPRINT)
if [[ -z $SG_EXPECTED ]]; then
  if [[ $(value RESTORE_UNPINNED_ACK) != 1 ]]; then
    echo 'BLOCKED: 恢复须填写 EXPECTED_FINGERPRINT；仅人工确认灾难恢复时可设 RESTORE_UNPINNED_ACK=1。' >&2
    exit 1
  fi
  echo '!!! WARNING: RESTORE_UNPINNED_ACK=1，缺少预期指纹，恢复身份未经可信指纹确认 !!!' >&2
fi
SG_RESTORE_PENDING=0
SG_RESTORE_WORK=$(mktemp -d "${TMPDIR:-/tmp}/polaris-restore.XXXXXX")
finish_restore() {
  local status=$?
  trap - EXIT
  if [[ $SG_RESTORE_PENDING == 1 ]]; then
    echo '恢复或公钥核验失败，停止 authority；请核对可信 EXPECTED_FINGERPRINT 与原身份备份，勿直接接受新指纹。' >&2
    dc stop -t 30 authority >/dev/null || {
      echo 'BLOCKED: 自动停止失败，请立即人工停止 authority。' >&2
    }
    status=1
  fi
  python3 "$SG_ROOT/scripts/snapshot.py" clean "$SG_RESTORE_WORK" || status=1
  exit "$status"
}
trap finish_restore EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
# 解密、完整性、路径、SQLite 与私钥格式在停机前验证。
SG_ARCHIVE=$(TMPDIR="$SG_RESTORE_WORK" python3 "$SG_ROOT/scripts/snapshot.py" unpack \
  "$SG_ARCHIVE" "$SG_RESTORE_WORK" "$SG_ENV_FILE")
require_docker
# 先备份现状；空卷恢复允许无现存数据库。
if [[ -z $(dc ps -aq authority) ]]; then
  dc create authority >/dev/null
fi
if dc run --rm -T --no-deps --entrypoint sh authority -ec \
  'test -s /home/polaris/Polaris/data/collab.db'; then
  backup_authority
fi
SG_RESTORE_PENDING=1
stop_authority
# 失败故意保持停止，不能启动半恢复的数据。
dc run --rm -T --no-deps --user 0 --entrypoint sh authority -ec '
  dir=/home/polaris/Polaris/data
  mkdir -p "$dir"
  rm -f "$dir/collab.db-wal" "$dir/collab.db-shm"
  tar -xf - -C "$dir" collab.db account_authority.key
  chown -R 1000:1000 "$dir"
  chmod 600 "$dir/collab.db" "$dir/account_authority.key"
' < "$SG_ARCHIVE"
# 即使原服务停止，也临时启动并从实际恢复的容器取得公钥，再恢复停止状态。
dc start authority >/dev/null
SG_TIMEOUT=$(value RESTORE_VERIFY_TIMEOUT_SECS)
# curl 重试预算不截断最后一次请求；请求上限 min(10, 预算)，总请求等待至多预算加该上限。
SG_REQUEST_TIMEOUT=$((SG_TIMEOUT < 10 ? SG_TIMEOUT : 10))
dc exec -T authority curl -fsS --retry "$SG_TIMEOUT" --retry-connrefused \
  --retry-delay 1 --retry-max-time "$SG_TIMEOUT" --max-time "$SG_REQUEST_TIMEOUT" \
  http://127.0.0.1:8080/api/account/pubkey \
  | python3 "$SG_ROOT/scripts/probe.py" fingerprint "$SG_EXPECTED"
if [[ $SG_WAS_RUNNING == 0 ]]; then dc stop -t 30 authority >/dev/null; fi
SG_RESTORE_PENDING=0
if [[ -n $SG_EXPECTED ]]; then
  echo '已恢复数据库；恢复后公钥与 EXPECTED_FINGERPRINT 一致，原运行状态已保留。'
else
  echo '已按显式确认恢复；以上仅为实际公钥指纹，身份尚未核实，请经可信渠道核对并填写 EXPECTED_FINGERPRINT。'
fi
