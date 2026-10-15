#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
[[ $# == 1 ]] || { echo '用法：restore.sh /绝对路径/snapshot.tar' >&2; exit 2; }
SG_ARCHIVE=$(realpath "$1")
# 在停机前检查完整性、路径、SQLite 与私钥格式。
python3 "$SG_ROOT/scripts/snapshot.py" verify "$SG_ARCHIVE"
lock
prepare
require_docker
# 先备份现状；空卷恢复允许无现存数据库。
if [[ -z $(dc ps -aq authority) ]]; then
  dc create authority >/dev/null
fi
if dc run --rm -T --no-deps --entrypoint sh authority -ec \
  'test -s /home/polaris/Polaris/data/collab.db'; then
  backup_authority
fi
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
restart_original
echo '已恢复数据库与原签名身份。若原容器停止，请先启动，再运行 smoke.sh 核对指纹。'
