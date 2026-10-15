#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
lock
prepare
require_docker
# 公共接口 /api/account/pubkey 调用服务端已有签名密钥生成逻辑。
# 已有库却丢失 key 时禁止自动造出新身份；已有容器也不重建。
dc run --rm -T --no-deps --user 0 --entrypoint sh authority -ec '
  dir=/home/polaris/Polaris/data
  mkdir -p "$dir"
  if [ -e "$dir/collab.db" ] && [ ! -s "$dir/account_authority.key" ]; then
    echo "BLOCKED: 已有数据库但签名私钥丢失，请恢复备份" >&2
    exit 1
  fi
  chown 1000:1000 /home/polaris/Polaris "$dir"
'
dc up -d --no-recreate --wait --wait-timeout 120 authority
dc exec -T authority curl -fsS --max-time 10 http://127.0.0.1:8080/api/account/pubkey \
  | python3 "$SG_ROOT/scripts/probe.py" fingerprint "$(value EXPECTED_FINGERPRINT)"
echo '以上完整指纹请通过可信渠道交给客户端发布负责人，并填入 .env EXPECTED_FINGERPRINT。'
