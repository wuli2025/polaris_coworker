#!/bin/sh
# Polaris Docker remote-update helper.
#
# --check: query the same latest.json used by desktop releases; no Docker access needed.
# default: after a successful check, call the authenticated HTTP API of the isolated Watchtower
# sidecar. This app container never receives docker.sock or a Docker CLI.
set -eu

DEFAULT_IMAGE_REPO="ghcr.io/wuli2025/polaris_coworker"
DEFAULT_UPDATER_URL="http://polaris-updater:8080/v1/update"
VERSION_FILE="${POLARIS_VERSION_FILE:-/app/package.json}"
IMAGE_REPO="${POLARIS_IMAGE_REPO:-$DEFAULT_IMAGE_REPO}"
UPDATER_URL="${POLARIS_UPDATER_URL:-$DEFAULT_UPDATER_URL}"
UPDATER_TOKEN="${POLARIS_UPDATER_TOKEN:-}"

one_line() {
  printf '%s' "$1" | tr '\r\n=' '   '
}

current_version() {
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
  # Accept both Tauri latest.json ("version") and GitHub release API ("tag_name").
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
    urls="https://llmwiki.cloud/latest.json
https://github.com/wuli2025/polaris_coworker/releases/latest/download/latest.json
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
    printf 'image=%s:latest\n' "$(one_line "$IMAGE_REPO")"
    printf 'source=%s\n' "$(one_line "$LATEST_SOURCE")"
    return 0
  fi

  printf 'ok=0\n'
  printf 'current=%s\n' "$(one_line "$current")"
  printf 'latest=\n'
  printf 'has_update=0\n'
  printf 'image=%s:latest\n' "$(one_line "$IMAGE_REPO")"
  printf 'error=四个版本源都不可用，请检查容器网络或设置 POLARIS_UPDATE_MANIFEST_URLS\n'
  return 1
}

if [ "${1:-}" = "--check" ]; then
  check_update
  exit $?
fi

if [ "${POLARIS_FORCE:-0}" != "1" ]; then
  check=$(check_update) || {
    printf '%s\n' "$check"
    echo "版本检查失败；未启动更新。需要跳过检查时使用“强制重装”。" >&2
    exit 2
  }
  printf '%s\n' "$check"
  has=$(printf '%s\n' "$check" | sed -n 's/^has_update=//p' | tail -n 1)
  if [ "$has" != "1" ]; then
    echo "当前已经是版本清单中的最新版；未启动替换。"
    exit 0
  fi
fi

if [ -z "$UPDATER_TOKEN" ]; then
  echo "隔离更新服务未配置：请通过 docker-compose.update.yml 设置 POLARIS_UPDATER_TOKEN" >&2
  exit 3
fi
if [ "$UPDATER_URL" != "$DEFAULT_UPDATER_URL" ]; then
  echo "POLARIS_UPDATER_URL 只能指向隔离内网端点 $DEFAULT_UPDATER_URL" >&2
  exit 4
fi

if [ "${POLARIS_DRY_RUN:-0}" = "1" ]; then
  echo "dry_run=1"
  echo "updater=$(one_line "$UPDATER_URL")"
  exit 0
fi

# Watchtower 的 HTTP API 只有“立即执行一轮更新”这一项能力；socket 留在 sidecar 内，
# Polaris 即使运行项目命令也接触不到 Docker daemon。Bearer token 只放请求头，不写日志。
# Watchtower v1.7.1 会等这一轮更新完成后才回 HTTP；真的替换本容器时，curl 会随旧容器
# 一起退出，而 sidecar 内的更新不受影响。给完整拉镜像留 15 分钟上限，不能用 30 秒误报失败。
response=$(curl -fsS --connect-timeout 5 --max-time 900 \
  -H "Authorization: Bearer $UPDATER_TOKEN" \
  "$UPDATER_URL") || {
    echo "隔离更新服务不可用或拒绝了请求；确认 update overlay 已启动且两端 token 一致" >&2
    exit 5
  }

printf 'started=1\n'
printf 'updater=%s\n' "$(one_line "$UPDATER_URL")"
printf 'response=%s\n' "$(one_line "$response")"
echo "隔离更新服务已接单；它会拉取新镜像并只替换带启用标签的 Polaris 容器。"
