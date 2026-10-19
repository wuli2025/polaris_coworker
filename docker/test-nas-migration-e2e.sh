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
legacy_home_volume=polaris-legacy-home-claude-${GITHUB_RUN_ID:-$$}
legacy_network=polaris-legacy-v28-${GITHUB_RUN_ID:-$$}

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
  down_stack "$test_root/stack" "$project_name"
  down_stack "$test_root/stack-home" "$project_name-home"
  down_stack "$test_root/stack-rollback" "$project_name-rollback"
  docker rm -f polaris-web polaris-updater >/dev/null 2>&1 || true
  remove_matching_recovery_containers
  docker volume rm "$legacy_volume" >/dev/null 2>&1 || true
  docker volume rm "$legacy_home_volume" >/dev/null 2>&1 || true
  docker network rm "$legacy_network" >/dev/null 2>&1 || true
  # The staged Polaris container writes into these disposable bind mounts as
  # UID 1000/root. Return only this mktemp tree to the runner before the host
  # removes it; production legacy data is never chowned by the migration.
  cleanup_uid=$(id -u)
  cleanup_gid=$(id -g)
  docker run --rm -v "$test_root:/cleanup" alpine:3.20 \
    chown -R "$cleanup_uid:$cleanup_gid" /cleanup >/dev/null 2>&1 || true
  rm -rf "$test_root"
}
trap cleanup EXIT HUP INT TERM

down_stack() {
  stack_path=$1
  stack_project=$2
  [ -f "$stack_path/docker-compose.yml" ] || return 0
  if [ -f "$stack_path/docker-compose.legacy-data.yml" ]; then
    docker compose -p "$stack_project" \
      -f "$stack_path/docker-compose.yml" \
      -f "$stack_path/docker-compose.update.yml" \
      -f "$stack_path/docker-compose.legacy-data.yml" down >/dev/null 2>&1 || true
  else
    docker compose -p "$stack_project" \
      -f "$stack_path/docker-compose.yml" \
      -f "$stack_path/docker-compose.update.yml" down >/dev/null 2>&1 || true
  fi
}

mkdir -p "$test_root/legacy-data"
printf '%s\n' 'bind-sentinel' > "$test_root/legacy-data/sentinel.txt"
docker volume create "$legacy_volume" >/dev/null
docker volume create "$legacy_home_volume" >/dev/null
docker network create "$legacy_network" >/dev/null
docker run --rm -v "$legacy_volume:/volume" alpine:3.20 \
  sh -c 'printf "%s\n" volume-sentinel > /volume/sentinel.txt'

docker run -d \
  --name polaris-web \
  -p 127.0.0.1:18080:8080 \
  -e ANTHROPIC_AUTH_TOKEN=e2e-provider-secret \
  -e POLARIS_SMTP_HOST=legacy.smtp.invalid \
  -e POLARIS_FABLE_WORKERS=7 \
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
  POLARIS_BIND_IP=127.0.0.1 \
  POLARIS_HTTP_PORT=19999 \
  POLARIS_AUTH_TOKEN=host-must-not-win \
  POLARIS_REQUIRE_LOGIN=1 \
  POLARIS_LAN_ONLY=1 \
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
   grep -E 'POLARIS_AUTH_TOKEN=(must-not-migrate|host-must-not-win)' >/dev/null; then
  fail "legacy access token leaked into the passwordless deployment"
fi
docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' polaris-web |
  grep -F 'POLARIS_SMTP_HOST=legacy.smtp.invalid' >/dev/null || fail "non-empty template default hid legacy SMTP config"
docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' polaris-web |
  grep -F 'POLARIS_FABLE_WORKERS=7' >/dev/null || fail "legacy performance config was not migrated"

app_mounts=$(docker inspect -f '{{range .Mounts}}{{println .Destination}}{{end}}' polaris-web)
updater_mounts=$(docker inspect -f '{{range .Mounts}}{{println .Destination}}{{end}}' polaris-updater)
printf '%s\n' "$app_mounts" | grep -F '/var/run/docker.sock' >/dev/null && fail "application received docker.sock"
printf '%s\n' "$updater_mounts" | grep -F '/var/run/docker.sock' >/dev/null || fail "updater is missing docker.sock"
[ "$(docker inspect -f '{{(index (index .HostConfig.PortBindings "8080/tcp") 0).HostPort}}' polaris-web)" = 18080 ] ||
  fail "custom host port was not preserved"

recovery=$(docker ps -a --filter 'name=^/polaris-web-legacy-' --format '{{.Names}}' | head -n 1)
[ -n "$recovery" ] || fail "successful migration did not retain a recovery container"
[ "$(docker inspect -f '{{.State.Running}}' "$recovery")" = false ] || fail "recovery container should remain stopped"

docker compose -p "$project_name" \
  -f "$test_root/stack/docker-compose.yml" \
  -f "$test_root/stack/docker-compose.update.yml" \
  -f "$test_root/stack/docker-compose.legacy-data.yml" down >/dev/null
docker rm -f "$recovery" >/dev/null

# Official 2.7/2.8 Compose images already used /home/polaris, UID 1000,
# named volumes, and a project network. All four properties must survive.
mkdir -p "$test_root/legacy-home-data"
docker run --rm -v "$test_root/legacy-home-data:/volume" alpine:3.20 \
  sh -c 'printf "%s\n" home-bind-sentinel > /volume/sentinel.txt; chown -R 1000:1000 /volume'
docker run --rm -v "$legacy_home_volume:/volume" alpine:3.20 \
  sh -c 'printf "%s\n" home-volume-sentinel > /volume/sentinel.txt; chown -R 1000:1000 /volume'

docker run -d \
  --name polaris-web \
  --network "$legacy_network" \
  --user 1000:1000 \
  -p 127.0.0.1:18082:8080 \
  -e POLARIS_SMTP_HOST=v28.smtp.invalid \
  -v "$test_root/legacy-home-data:/home/polaris/Polaris" \
  -v "$legacy_home_volume:/home/polaris/.claude" \
  alpine:3.20 sleep 600 >/dev/null

(
  cd "$root"
  POLARIS_STACK_DIR="$test_root/stack-home" \
  POLARIS_PROJECT_NAME="$project_name-home" \
  POLARIS_IMAGE_REPO="$BASE_IMAGE_REPO" \
  POLARIS_IMAGE_TAG="$BASE_IMAGE_TAG" \
  POLARIS_HEALTH_ATTEMPTS=90 \
  POLARIS_HEALTH_SLEEP=2 \
  sh docker/nas-bootstrap.sh
)

curl -fsS http://127.0.0.1:18082/api/build | grep -Eq '"version"[[:space:]]*:[[:space:]]*"2\.9\.2"' ||
  fail "home-layout migration did not start 2.9.2"
[ "$(docker inspect -f '{{.Config.User}}' polaris-web)" = 1000:1000 ] || fail "home-layout runtime user changed"
docker exec polaris-web test -f /home/polaris/Polaris/sentinel.txt || fail "home bind sentinel was not preserved"
docker exec polaris-web test -f /home/polaris/.claude/sentinel.txt || fail "home named-volume sentinel was not preserved"
docker inspect -f '{{json .NetworkSettings.Networks}}' polaris-web |
  grep -F "\"$legacy_network\"" >/dev/null || fail "legacy Compose network was not retained"
docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' polaris-web |
  grep -F 'POLARIS_SMTP_HOST=v28.smtp.invalid' >/dev/null || fail "home-layout business config was not retained"
[ "$(docker inspect -f '{{(index (index .HostConfig.PortBindings "8080/tcp") 0).HostPort}}' polaris-web)" = 18082 ] ||
  fail "home-layout custom port was not retained"

home_recovery=$(docker ps -a --filter 'name=^/polaris-web-legacy-' --format '{{.Names}}' | head -n 1)
[ -n "$home_recovery" ] || fail "home-layout migration did not retain a recovery container"
docker compose -p "$project_name-home" \
  -f "$test_root/stack-home/docker-compose.yml" \
  -f "$test_root/stack-home/docker-compose.update.yml" \
  -f "$test_root/stack-home/docker-compose.legacy-data.yml" down >/dev/null
docker rm -f "$home_recovery" >/dev/null

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
