#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
[[ $# == 0 ]] || { echo '请在 .env 填新 AUTHORITY_IMAGE 后运行；不接受浮动 tag' >&2; exit 2; }
lock
prepare
require_docker
require_pin
# 只升级权威；relay/代理单独维护，避免扩大故障域。
dc pull authority
backup_authority
docker inspect -f '{{.Config.Image}}' "$SG_CONTAINER" > "$SG_ROOT/.generated/previous-authority-image.txt"
dc up -d --no-deps --wait --wait-timeout 180 authority
python3 "$SG_ROOT/scripts/probe.py" smoke "$SG_ENV_FILE" --authority-only
echo '权威检查通过。下一步：升级成员主机，再重新登录需要设备绑定保证的客户端。'
echo '策略依据：docs/interconnect/p6a2-policy-b.md；旧会话不会被本脚本强制清除。'
