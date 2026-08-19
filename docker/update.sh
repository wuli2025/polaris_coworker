#!/bin/sh
# Polaris Docker self-update helper.
#
# --check: query the same latest.json used by desktop releases; no Docker socket needed.
# default: after a successful check, launch a pinned one-shot Watchtower helper. The helper is
# detached before this process returns, so replacing the current container cannot kill the update.
set -eu

DEFAULT_IMAGE_REPO="ghcr.io/wuli2025/polaris_coworker"
DEFAULT_WATCHTOWER_IMAGE="containrrr/watchtower:1.7.1"
VERSION_FILE="${POLARIS_VERSION_FILE:-/app/package.json}"
IMAGE_REPO="${POLARIS_IMAGE_REPO:-$DEFAULT_IMAGE_REPO}"
TARGET="${POLARIS_CONTAINER_NAME:-polaris-web}"
WATCHTOWER_IMAGE="${POLARIS_WATCHTOWER_IMAGE:-$DEFAULT_WATCHTOWER_IMAGE}"
SOCKET="${POLARIS_DOCKER_SOCKET_PATH:-/var/run/docker.sock}"

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

if [ ! -S "$SOCKET" ]; then
  echo "Docker socket 不存在：$SOCKET" >&2
  exit 3
fi
if [ ! -r "$SOCKET" ] || [ ! -w "$SOCKET" ]; then
  echo "当前用户无权访问 Docker socket：$SOCKET（请在更新 overlay 中设置正确的 DOCKER_GID）" >&2
  exit 4
fi
if ! command -v docker >/dev/null 2>&1; then
  echo "镜像中缺少 docker CLI" >&2
  exit 5
fi
case "$TARGET" in
  *[!A-Za-z0-9_.-]*|'') echo "POLARIS_CONTAINER_NAME 不合法：$TARGET" >&2; exit 6 ;;
esac

running_image=$(docker inspect --format '{{.Config.Image}}' "$TARGET" 2>/dev/null || true)
if [ -z "$running_image" ]; then
  echo "找不到当前容器：$TARGET" >&2
  exit 7
fi
case "$running_image" in
  "$IMAGE_REPO":*) ;;
  *)
    echo "当前容器镜像是 $running_image，不属于远程更新源 $IMAGE_REPO；本地构建请继续用 docker compose up -d --build。" >&2
    exit 8
    ;;
esac

helper="polaris-self-update"
if [ "${POLARIS_DRY_RUN:-0}" = "1" ]; then
  echo "dry_run=1"
  echo "target=$TARGET"
  echo "image=$running_image"
  echo "watchtower=$WATCHTOWER_IMAGE"
  exit 0
fi

# Watchtower is only an ephemeral replacement worker. Pin the version so an upstream `latest`
# change cannot silently alter host-level updater behavior.
docker pull "$WATCHTOWER_IMAGE" >/dev/null
docker rm -f "$helper" >/dev/null 2>&1 || true
helper_id=$(docker run -d --rm \
  --name "$helper" \
  -v "$SOCKET:/var/run/docker.sock" \
  "$WATCHTOWER_IMAGE" \
  --run-once --cleanup --stop-timeout 60s "$TARGET")

printf 'started=1\n'
printf 'helper=%s\n' "$(one_line "$helper_id")"
printf 'target=%s\n' "$(one_line "$TARGET")"
printf 'image=%s\n' "$(one_line "$running_image")"
echo "更新替身已启动；它会拉取新镜像并原样重建当前容器。"
