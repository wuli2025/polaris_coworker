#!/bin/sh
# Polaris Docker remote-update helper.
#
# --check   : release-manifest diagnostic only; product update availability is decided from OCI revision.
# --trigger : call the isolated Watchtower HTTP API. A 200 means only “the scan returned”; replacement
#             success is proved by the new server boot/revision, never by this script response.
set -eu

DEFAULT_IMAGE_REPO="ghcr.io/wuli2025/polaris_coworker"
DEFAULT_UPDATER_URL="http://polaris-updater:8080/v1/update"
VERSION_FILE="${POLARIS_VERSION_FILE:-/app/package.json}"
IMAGE_REPO="${POLARIS_IMAGE_REPO:-$DEFAULT_IMAGE_REPO}"
IMAGE_TAG="${POLARIS_TAG:-latest}"
UPDATER_URL="${POLARIS_UPDATER_URL:-$DEFAULT_UPDATER_URL}"
UPDATER_TOKEN="${POLARIS_UPDATER_TOKEN:-}"
RESULT_FILE="${POLARIS_UPDATE_RESULT_FILE:-}"

one_line() {
  printf '%s' "$1" | tr '\r\n=' '   '
}

write_result() {
  state=$1
  code=$2
  [ -n "$RESULT_FILE" ] || return 0
  tmp="${RESULT_FILE}.tmp.$$"
  printf '{"state":"%s","exitCode":%s}\n' "$state" "$code" > "$tmp"
  mv -f "$tmp" "$RESULT_FILE"
}

current_version() {
  if [ -n "${POLARIS_BUILD_VERSION:-}" ]; then
    printf '%s' "$POLARIS_BUILD_VERSION"
    return
  fi
  if [ -n "${POLARIS_VERSION:-}" ]; then
    printf '%s' "$POLARIS_VERSION"
    return
  fi
  if [ -r "$VERSION_FILE" ]; then
    sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$VERSION_FILE" | head -n 1
    return
  fi
  printf '%s' "unknown"
}

extract_version() {
  sed -n \
    -e 's/.*"version"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' \
    -e 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' \
    | head -n 1
}

fetch_latest() {
  custom="${POLARIS_UPDATE_MANIFEST_URLS:-}"
  if [ -n "$custom" ]; then
    urls="$custom"
  else
    # llmwiki.cloud/latest.json 当前返回官网 HTML，不能继续把它伪装成版本源。
    urls="https://github.com/wuli2025/polaris_coworker/releases/latest/download/latest.json
https://api.github.com/repos/wuli2025/polaris_coworker/releases/latest
https://gh-proxy.com/https://github.com/wuli2025/polaris_coworker/releases/latest/download/latest.json"
  fi

  old_ifs=$IFS
  IFS='
'
  for url in $urls; do
    [ -n "$url" ] || continue
    body=$(curl -fsSL --connect-timeout 8 --max-time 25 --retry 1 "$url" 2>/dev/null || true)
    version=$(printf '%s' "$body" | extract_version)
    if [ -n "$version" ]; then
      IFS=$old_ifs
      LATEST_VERSION=$version
      LATEST_SOURCE=$url
      return 0
    fi
  done
  IFS=$old_ifs
  return 1
}

is_newer() {
  current=$1
  latest=$2
  [ "$current" != "$latest" ] || return 1
  [ "$current" != "unknown" ] || return 0
  highest=$(printf '%s\n%s\n' "$current" "$latest" | sort -V | tail -n 1)
  [ "$highest" = "$latest" ]
}

check_update() {
  current=$(current_version)
  if fetch_latest; then
    has=0
    if is_newer "$current" "$LATEST_VERSION"; then has=1; fi
    printf 'ok=1\n'
    printf 'current=%s\n' "$(one_line "$current")"
    printf 'latest=%s\n' "$(one_line "$LATEST_VERSION")"
    printf 'has_update=%s\n' "$has"
    printf 'image=%s:%s\n' "$(one_line "$IMAGE_REPO")" "$(one_line "$IMAGE_TAG")"
    printf 'source=%s\n' "$(one_line "$LATEST_SOURCE")"
    return 0
  fi

  printf 'ok=0\n'
  printf 'current=%s\n' "$(one_line "$current")"
  printf 'latest=\n'
  printf 'has_update=0\n'
  printf 'image=%s:%s\n' "$(one_line "$IMAGE_REPO")" "$(one_line "$IMAGE_TAG")"
  printf 'error=版本清单不可用；产品内 Docker 更新会改用 OCI revision 检查\n'
  return 1
}

case "${1:-}" in
  --check)
    check_update
    exit $?
    ;;
  --trigger)
    ;;
  *)
    echo "用法: update.sh --check | --trigger" >&2
    exit 64
    ;;
esac

if [ -z "$UPDATER_TOKEN" ]; then
  write_result failed 3
  echo "隔离更新服务未配置：请通过 docker-compose.update.yml 设置 POLARIS_UPDATER_TOKEN" >&2
  exit 3
fi
if [ "$UPDATER_URL" != "$DEFAULT_UPDATER_URL" ]; then
  write_result failed 4
  echo "POLARIS_UPDATER_URL 只能指向隔离内网端点 $DEFAULT_UPDATER_URL" >&2
  exit 4
fi

if [ "${POLARIS_DRY_RUN:-0}" = "1" ]; then
  write_result watchtower_returned 0
  echo "dry_run=1"
  echo "updater=$(one_line "$UPDATER_URL")"
  exit 0
fi

# 只供 disposable CI 故障注入：让后端先返回 accepted，再关闭临时 registry，
# 从而证明真实 pull 失败会在有限截止时间内离开等待态。生产环境无法启用这段延迟。
if [ "${POLARIS_UPDATE_E2E:-0}" = "1" ]; then
  delay="${POLARIS_UPDATE_TRIGGER_DELAY_SECONDS:-0}"
  case "$delay" in
    ''|*[!0-9]*) delay=0 ;;
  esac
  if [ "$delay" -gt 0 ]; then sleep "$delay"; fi
fi

write_result triggering 0
# Watchtower 1.7.1 会同步执行扫描/拉取/替换。若它真的替换本容器，本 curl 会随旧容器
# 一起结束，但 sidecar 内已开始的 callback 不依赖客户端连接；新容器用 boot/revision 证明结果。
if ! curl -fsS --connect-timeout 5 --max-time 900 \
  -H "Authorization: Bearer $UPDATER_TOKEN" \
  "$UPDATER_URL" >/dev/null; then
  write_result failed 5
  echo "隔离更新服务不可用或拒绝请求；确认 overlay 已启动且两端 token 一致" >&2
  exit 5
fi

# 空 200 可能是 no-op、锁跳过或内部失败；这里只记录“HTTP 返回”，绝不声称镜像已更新。
write_result watchtower_returned 0
printf 'watchtower_returned=1\n'
echo "Watchtower 扫描已返回；是否替换成功由新容器 build revision 继续确认。"
