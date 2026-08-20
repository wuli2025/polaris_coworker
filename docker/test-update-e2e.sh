#!/bin/sh
# Disposable end-to-end test for the Docker Web "click update" path.
# It starts a private registry, Polaris, and the real Watchtower 1.7.1 sidecar;
# no host resources are reused and every container/network/volume is removed on exit.
set -eu

if [ "${POLARIS_E2E:-0}" != "1" ]; then
  echo "Refusing to run outside disposable CI (set POLARIS_E2E=1)." >&2
  exit 64
fi
if [ -z "${BASE_IMAGE:-}" ]; then
  echo "BASE_IMAGE is required" >&2
  exit 64
fi

for tool in docker curl jq; do
  command -v "$tool" >/dev/null 2>&1 || { echo "$tool is required" >&2; exit 64; }
done

RUN_KEY="${GITHUB_RUN_ID:-local}-$$"
NETWORK="polaris-update-e2e-$RUN_KEY"
VOLUME="polaris-update-e2e-data-$RUN_KEY"
REGISTRY="polaris-update-e2e-registry-$RUN_KEY"
APP="polaris-update-e2e-app-$RUN_KEY"
UPDATER="polaris-update-e2e-updater-$RUN_KEY"
TOKEN="e2e-app-$RUN_KEY"
UPDATER_TOKEN="e2e-updater-$RUN_KEY"
WORK="$(mktemp -d)"
FAILED=1

cleanup() {
  code=$?
  trap - EXIT INT TERM
  if [ "$FAILED" = "1" ]; then
    echo "--- Polaris logs ---" >&2
    docker logs "$APP" 2>&1 || true
    echo "--- Watchtower logs ---" >&2
    docker logs "$UPDATER" 2>&1 || true
    echo "--- Registry logs ---" >&2
    docker logs "$REGISTRY" 2>&1 || true
  fi
  docker rm -f "$APP" "$UPDATER" "$REGISTRY" >/dev/null 2>&1 || true
  docker volume rm -f "$VOLUME" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  rm -rf "$WORK"
  exit "$code"
}
trap cleanup EXIT INT TERM

docker network create "$NETWORK" >/dev/null
docker volume create "$VOLUME" >/dev/null
docker run -d --name "$REGISTRY" --network "$NETWORK" --network-alias registry \
  -p 127.0.0.1::5000 registry:2 >/dev/null
REGISTRY_PORT="$(docker port "$REGISTRY" 5000/tcp | sed -n 's/.*://p' | head -n 1)"
[ -n "$REGISTRY_PORT" ] || { echo "registry port mapping missing" >&2; exit 1; }
IMAGE_REPO="127.0.0.1:$REGISTRY_PORT/polaris-e2e"

cat >"$WORK/Dockerfile" <<'DOCKERFILE'
ARG BASE_IMAGE
FROM ${BASE_IMAGE}
ARG BUILD_REVISION
USER root
RUN printf '%s\n' "$BUILD_REVISION" > /app/polaris-build-revision \
    && printf '%s\n' '2.9.0-e2e' > /app/polaris-build-version
USER polaris
ENV POLARIS_BUILD_REVISION=${BUILD_REVISION} \
    POLARIS_BUILD_VERSION=2.9.0-e2e
LABEL org.opencontainers.image.revision=${BUILD_REVISION} \
      io.polaris.app.version=2.9.0-e2e
DOCKERFILE

docker build --build-arg "BASE_IMAGE=$BASE_IMAGE" --build-arg BUILD_REVISION=e2e-source \
  -t "$IMAGE_REPO:source" "$WORK" >/dev/null
docker tag "$IMAGE_REPO:source" "$IMAGE_REPO:latest"
docker push "$IMAGE_REPO:latest" >/dev/null

docker build --build-arg "BASE_IMAGE=$BASE_IMAGE" --build-arg BUILD_REVISION=e2e-target \
  -t "$IMAGE_REPO:target" "$WORK" >/dev/null

echo "Starting source Polaris container"
if ! docker run -d --name "$APP" --network "$NETWORK" --restart unless-stopped \
  -p 127.0.0.1::8080 \
  -v "$VOLUME:/home/polaris/Polaris" \
  --label com.centurylinklabs.watchtower.enable=true \
  -e "POLARIS_AUTH_TOKEN=$TOKEN" \
  -e "POLARIS_UPDATER_URL=http://polaris-updater:8080/v1/update" \
  -e "POLARIS_UPDATER_TOKEN=$UPDATER_TOKEN" \
  -e "POLARIS_IMAGE_REPO=$IMAGE_REPO" \
  -e POLARIS_TAG=latest \
  -e POLARIS_REGISTRY_API_URL=http://registry:5000 \
  -e POLARIS_ALLOW_INSECURE_REGISTRY=1 \
  -e POLARIS_UPDATE_E2E=1 \
  -e POLARIS_UPDATE_DEADLINE_SECONDS=20 \
  -e POLARIS_UPDATE_TRIGGER_DELAY_SECONDS=2 \
  -e POLARIS_E2E_SENTINEL=runtime-kept \
  "$IMAGE_REPO:latest" >/dev/null; then
  echo "failed to start source Polaris container" >&2
  exit 1
fi
APP_PORT="$(docker port "$APP" 8080/tcp | sed -n 's/.*://p' | head -n 1)" \
  || { echo "failed to inspect Polaris port mapping" >&2; exit 1; }
[ -n "$APP_PORT" ] || { echo "Polaris port mapping missing" >&2; exit 1; }
echo "Source Polaris published at 127.0.0.1:$APP_PORT"
BASE_URL="http://127.0.0.1:$APP_PORT"
AUTH="Authorization: Bearer $TOKEN"

wait_ready() {
  limit=$1
  i=0
  while [ "$i" -lt "$limit" ]; do
    if curl -fsS "$BASE_URL/api/ready" >/dev/null 2>&1; then return 0; fi
    i=$((i + 1))
    sleep 1
  done
  return 1
}

invoke() {
  cmd=$1
  args=$2
  curl -fsS -H "$AUTH" -H 'Content-Type: application/json' \
    -d "{\"cmd\":\"$cmd\",\"args\":$args}" "$BASE_URL/api/invoke"
}

wait_request_state() {
  request_id=$1
  wanted=$2
  limit=$3
  i=0
  while [ "$i" -lt "$limit" ]; do
    value="$(invoke docker_update_status "{\"requestId\":\"$request_id\"}" 2>/dev/null || true)"
    state="$(printf '%s' "$value" | jq -r '.state // empty' 2>/dev/null || true)"
    if [ "$state" = "$wanted" ]; then return 0; fi
    i=$((i + 1))
    sleep 1
  done
  echo "request $request_id did not reach $wanted" >&2
  return 1
}

wait_request_failure() {
  request_id=$1
  limit=$2
  i=0
  while [ "$i" -lt "$limit" ]; do
    value="$(invoke docker_update_status "{\"requestId\":\"$request_id\"}" 2>/dev/null || true)"
    state="$(printf '%s' "$value" | jq -r '.state // empty' 2>/dev/null || true)"
    if [ "$state" = "failed" ] || [ "$state" = "unconfirmed" ]; then return 0; fi
    i=$((i + 1))
    sleep 1
  done
  echo "request $request_id did not reach a finite failure state" >&2
  return 1
}

wait_ready 90 || { echo "source Polaris never became ready at $BASE_URL" >&2; exit 1; }
docker exec "$APP" sh -c 'printf keep > /home/polaris/Polaris/e2e-sentinel' \
  || { echo "source Polaris data volume is not writable" >&2; exit 1; }
SOURCE_ID="$(docker inspect -f '{{.Id}}' "$APP")"
SOURCE_BUILD="$(curl -fsS "$BASE_URL/api/build")" \
  || { echo "source /api/build is unavailable" >&2; exit 1; }
SOURCE_BOOT="$(printf '%s' "$SOURCE_BUILD" | jq -r '.bootId // empty')"
SOURCE_REVISION="$(printf '%s' "$SOURCE_BUILD" | jq -r '.buildRevision // empty')"
[ -n "$SOURCE_BOOT" ] || { echo "source /api/build has no bootId: $SOURCE_BUILD" >&2; exit 1; }
[ "$SOURCE_REVISION" = "e2e-source" ] \
  || { echo "source marker mismatch: expected e2e-source, got $SOURCE_REVISION; build=$SOURCE_BUILD" >&2; exit 1; }

docker tag "$IMAGE_REPO:target" "$IMAGE_REPO:latest"
docker push "$IMAGE_REPO:latest" >/dev/null
# 不能让目标镜像留在 runner 本地：否则 Watchtower 可能直接复用本地 tag，所谓“拉取失败”
# 和 happy path 都没有真的证明 registry pull。source 容器仍由 source tag/镜像 ID 保留。
docker image rm "$IMAGE_REPO:target" "$IMAGE_REPO:latest" >/dev/null
CHECK_RESULT="$(invoke docker_check_update '{}')" \
  || { echo "docker_check_update request failed" >&2; exit 1; }
printf '%s' "$CHECK_RESULT" | jq -e '.ok and .has_update and .target_revision == "e2e-target"' >/dev/null \
  || { echo "docker_check_update did not resolve the target OCI revision: $CHECK_RESULT" >&2; exit 1; }

# Wrong updater Bearer token must become an explicit failed state, not an endless spinner.
docker run -d --name "$UPDATER" --network "$NETWORK" --network-alias polaris-updater \
  -e WATCHTOWER_HTTP_API_TOKEN=definitely-wrong \
  -v /var/run/docker.sock:/var/run/docker.sock \
  containrrr/watchtower:1.7.1 --http-api-update --label-enable --cleanup \
  --stop-timeout 60s "$APP" >/dev/null
sleep 2
BAD_REQUEST="$(invoke docker_update '{"confirm":true}')"
printf '%s' "$BAD_REQUEST" | jq -e '.accepted and (.requestId | length > 0)' >/dev/null
wait_request_state "$(printf '%s' "$BAD_REQUEST" | jq -r .requestId)" failed 30
docker rm -f "$UPDATER" >/dev/null

# Correct Watchtower, but unavailable registry: accepted first, then bounded unconfirmed.
docker run -d --name "$UPDATER" --network "$NETWORK" --network-alias polaris-updater \
  -e "WATCHTOWER_HTTP_API_TOKEN=$UPDATER_TOKEN" \
  -v /var/run/docker.sock:/var/run/docker.sock \
  containrrr/watchtower:1.7.1 --http-api-update --label-enable --cleanup \
  --stop-timeout 60s "$APP" >/dev/null
sleep 2
PULL_REQUEST="$(invoke docker_update '{"confirm":true}')"
PULL_ID="$(printf '%s' "$PULL_REQUEST" | jq -r .requestId)"
[ -n "$PULL_ID" ]
docker stop "$REGISTRY" >/dev/null
wait_request_failure "$PULL_ID" 45
docker start "$REGISTRY" >/dev/null
docker rm -f "$UPDATER" >/dev/null
docker run -d --name "$UPDATER" --network "$NETWORK" --network-alias polaris-updater \
  -e "WATCHTOWER_HTTP_API_TOKEN=$UPDATER_TOKEN" \
  -v /var/run/docker.sock:/var/run/docker.sock \
  containrrr/watchtower:1.7.1 --http-api-update --label-enable --cleanup \
  --stop-timeout 60s "$APP" >/dev/null
sleep 2

# Happy path: API responds quickly; only new boot + exact target revision counts as success.
start="$(date +%s)"
GOOD_REQUEST="$(invoke docker_update '{"confirm":true}')"
elapsed=$(( $(date +%s) - start ))
[ "$elapsed" -lt 10 ] || { echo "docker_update blocked for ${elapsed}s" >&2; exit 1; }
GOOD_ID="$(printf '%s' "$GOOD_REQUEST" | jq -r .requestId)"
printf '%s' "$GOOD_REQUEST" | jq -e '.accepted and .targetRevision == "e2e-target"' >/dev/null

i=0
while [ "$i" -lt 120 ]; do
  build="$(curl -fsS "$BASE_URL/api/build" 2>/dev/null || true)"
  boot="$(printf '%s' "$build" | jq -r '.bootId // empty' 2>/dev/null || true)"
  revision="$(printf '%s' "$build" | jq -r '.buildRevision // empty' 2>/dev/null || true)"
  if [ "$revision" = "e2e-target" ] && [ -n "$boot" ] && [ "$boot" != "$SOURCE_BOOT" ] \
    && curl -fsS "$BASE_URL/api/ready" >/dev/null 2>&1; then
    break
  fi
  i=$((i + 1))
  sleep 1
done
[ "$i" -lt 120 ] || { echo "target container never became ready" >&2; exit 1; }

TARGET_ID="$(docker inspect -f '{{.Id}}' "$APP")"
[ "$TARGET_ID" != "$SOURCE_ID" ]
[ "$(docker exec "$APP" cat /home/polaris/Polaris/e2e-sentinel)" = "keep" ]
[ "$(docker exec "$APP" sh -c 'printf %s "$POLARIS_E2E_SENTINEL"')" = "runtime-kept" ]
[ "$(docker inspect -f '{{.HostConfig.RestartPolicy.Name}}' "$APP")" = "unless-stopped" ]
[ "$(docker inspect -f '{{index .Config.Labels "com.centurylinklabs.watchtower.enable"}}' "$APP")" = "true" ]
docker inspect "$APP" | jq -e --arg network "$NETWORK" '.[0].NetworkSettings.Networks[$network] != null' >/dev/null
docker inspect "$APP" | jq -e '.[0].NetworkSettings.Ports["8080/tcp"][0].HostPort | length > 0' >/dev/null
wait_request_state "$GOOD_ID" succeeded 20
invoke docker_update '{"confirm":true}' | jq -e '.accepted == false and .upToDate == true' >/dev/null

FAILED=0
echo "Docker update E2E passed: wrong token, pull failure deadline, replacement proof, readiness, and preserved runtime config."
