#!/bin/sh
set -eu

if [ "${POLARIS_E2E:-}" != 1 ]; then
  printf '%s\n' "SKIP: set POLARIS_E2E=1 to run real NAS migration tests"
  exit 0
fi

: "${BASE_IMAGE_REPO:?set BASE_IMAGE_REPO to the staged image repository}"
: "${BASE_IMAGE_TAG:?set BASE_IMAGE_TAG to the staged image tag}"

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
test_root=$(mktemp -d)
project_name=polaris-migration-e2e-${GITHUB_RUN_ID:-$$}
legacy_volume=polaris-legacy-claude-${GITHUB_RUN_ID:-$$}

fail() {
  printf '%s\n' "NAS migration E2E: $*" >&2
  exit 1
}

remove_matching_recovery_containers() {
  docker ps -a --filter 'name=^/polaris-web-legacy-' --format '{{.Names}}' |
    while IFS= read -r container; do
      [ -z "$container" ] || docker rm -f "$container" >/dev/null 2>&1 || true
    done
}

cleanup() {
  trap - EXIT HUP INT TERM
  if [ -f "$test_root/stack/docker-compose.yml" ]; then
    if [ -n "${LEGACY_OVERLAY:-}" ]; then
      docker compose -p "$project_name" \
        -f "$test_root/stack/docker-compose.yml" \
        -f "$test_root/stack/docker-compose.update.yml" \
        -f "$LEGACY_OVERLAY" down >/dev/null 2>&1 || true
    else
      docker compose -p "$project_name" \
        -f "$test_root/stack/docker-compose.yml" \
        -f "$test_root/stack/docker-compose.update.yml" \
        down >/dev/null 2>&1 || true
    fi
  fi
  docker rm -f polaris-web polaris-updater >/dev/null 2>&1 || true
  remove_matching_recovery_containers
  docker volume rm "$legacy_volume" >/dev/null 2>&1 || true
  rm -rf "$test_root"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$test_root/legacy-data"
printf '%s\n' 'bind-sentinel' > "$test_root/legacy-data/sentinel.txt"
docker volume create "$legacy_volume" >/dev/null
docker run --rm -v "$legacy_volume:/volume" alpine:3.20 \
  sh -c 'printf "%s\n" volume-sentinel > /volume/sentinel.txt'

docker run -d \
  --name polaris-web \
  -p 127.0.0.1:18080:8080 \
  -e ANTHROPIC_AUTH_TOKEN=e2e-provider-secret \
  -e POLARIS_AUTH_TOKEN=must-not-migrate \
  -v "$test_root/legacy-data:/root/Polaris" \
  -v "$legacy_volume:/root/.claude" \
  alpine:3.20 sleep 600 >/dev/null

(
  cd "$root"
  POLARIS_STACK_DIR="$test_root/stack" \
  POLARIS_PROJECT_NAME="$project_name" \
  POLARIS_IMAGE_REPO="$BASE_IMAGE_REPO" \
  POLARIS_IMAGE_TAG="$BASE_IMAGE_TAG" \
  POLARIS_HEALTH_ATTEMPTS=90 \
  POLARIS_HEALTH_SLEEP=2 \
  sh docker/nas-bootstrap.sh
)

build_json=$(curl -fsS http://127.0.0.1:18080/api/build)
printf '%s' "$build_json" | grep -Eq '"version"[[:space:]]*:[[:space:]]*"2\.9\.2"' || fail "new build is not 2.9.2"
if [ -n "${GITHUB_SHA:-}" ]; then
  printf '%s' "$build_json" | grep -Eq "\"buildRevision\"[[:space:]]*:[[:space:]]*\"$GITHUB_SHA\"" ||
    fail "new build revision does not match the workflow revision"
fi

docker exec polaris-web test -f /home/polaris/Polaris/sentinel.txt || fail "bind sentinel was not preserved"
docker exec polaris-web test -f /home/polaris/.claude/sentinel.txt || fail "named-volume sentinel was not preserved"
docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' polaris-web |
  grep -F 'ANTHROPIC_AUTH_TOKEN=e2e-provider-secret' >/dev/null || fail "provider configuration was not migrated"
if docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' polaris-web |
   grep -F 'POLARIS_AUTH_TOKEN=must-not-migrate' >/dev/null; then
  fail "legacy access token leaked into the passwordless deployment"
fi

app_mounts=$(docker inspect -f '{{range .Mounts}}{{println .Destination}}{{end}}' polaris-web)
updater_mounts=$(docker inspect -f '{{range .Mounts}}{{println .Destination}}{{end}}' polaris-updater)
printf '%s\n' "$app_mounts" | grep -F '/var/run/docker.sock' >/dev/null && fail "application received docker.sock"
printf '%s\n' "$updater_mounts" | grep -F '/var/run/docker.sock' >/dev/null || fail "updater is missing docker.sock"
[ "$(docker inspect -f '{{(index (index .HostConfig.PortBindings "8080/tcp") 0).HostPort}}' polaris-web)" = 18080 ] ||
  fail "custom host port was not preserved"

recovery=$(docker ps -a --filter 'name=^/polaris-web-legacy-' --format '{{.Names}}' | head -n 1)
[ -n "$recovery" ] || fail "successful migration did not retain a recovery container"
[ "$(docker inspect -f '{{.State.Running}}' "$recovery")" = false ] || fail "recovery container should remain stopped"

LEGACY_OVERLAY=$test_root/stack/docker-compose.legacy-data.yml
docker compose -p "$project_name" \
  -f "$test_root/stack/docker-compose.yml" \
  -f "$test_root/stack/docker-compose.update.yml" \
  -f "$LEGACY_OVERLAY" down >/dev/null
docker rm -f "$recovery" >/dev/null
LEGACY_OVERLAY=

# A broken replacement must restore the old container, its custom port, and its data.
docker run -d \
  --name polaris-web \
  -p 127.0.0.1:18081:8080 \
  -v "$test_root/legacy-data:/root/Polaris" \
  -v "$legacy_volume:/root/.claude" \
  alpine:3.20 sleep 600 >/dev/null

if (
  cd "$root"
  POLARIS_STACK_DIR="$test_root/stack-rollback" \
  POLARIS_PROJECT_NAME="$project_name-rollback" \
  POLARIS_IMAGE_REPO=alpine \
  POLARIS_IMAGE_TAG=3.20 \
  POLARIS_HEALTH_ATTEMPTS=2 \
  POLARIS_HEALTH_SLEEP=1 \
  sh docker/nas-bootstrap.sh
); then
  fail "broken replacement unexpectedly passed health verification"
fi

[ "$(docker inspect -f '{{.State.Running}}' polaris-web)" = true ] || fail "legacy container was not restarted"
[ "$(docker inspect -f '{{(index (index .HostConfig.PortBindings "8080/tcp") 0).HostPort}}' polaris-web)" = 18081 ] ||
  fail "rollback did not restore the legacy port"
docker exec polaris-web test -f /root/Polaris/sentinel.txt || fail "rollback lost legacy data"
[ -z "$(docker ps -a --filter 'name=^/polaris-web-legacy-' --format '{{.Names}}')" ] ||
  fail "rollback left a renamed legacy container behind"

printf '%s\n' "NAS migration E2E: ok"
