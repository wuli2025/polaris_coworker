#!/usr/bin/env bash
# 为 iroh-relay 生成「纯 IP」自签证书（v8 方案）。
# 用法: ./gen-cert.sh <服务器公网IP>
# 产物: docker/relay/certs/relay.crt / relay.key （有效期 10 年）
set -euo pipefail

IP="${1:?用法: $0 <服务器公网IP>，例: $0 203.0.113.7}"
DIR="$(cd "$(dirname "$0")" && pwd)/certs"
mkdir -p "$DIR"

openssl req -x509 -newkey ed25519 -nodes -days 3650 \
  -keyout "$DIR/relay.key" -out "$DIR/relay.crt" \
  -subj "/CN=$IP" \
  -addext "subjectAltName=IP:$IP"

chmod 600 "$DIR/relay.key"
echo "✔ 已生成: $DIR/relay.crt / relay.key (SAN=IP:$IP, 3650 天)"
echo "  下一步: cp config.toml.example config.toml 并核对证书路径，"
echo "  然后 docker compose --profile relay up -d"
