#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
: > "$tmp/empty.env"

compose() {
  if [ -n "${POLARIS_COMPOSE_BIN:-}" ]; then
    "$POLARIS_COMPOSE_BIN" "$@"
  else
    docker compose "$@"
  fi
}

compose --env-file "$tmp/empty.env" \
  -f docker-compose.yml -f docker-compose.update.yml \
  config --format json > "$tmp/config.json"

node - "$tmp/config.json" <<'NODE'
const fs = require("fs");
const c = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const app = c.services.polaris;
const updater = c.services["polaris-updater"];
if (app.user !== "1000:1000") throw new Error(`unexpected app user ${app.user}`);
if (!app.environment.POLARIS_UPDATER_TOKEN) throw new Error("app updater token is empty");
if (app.environment.POLARIS_UPDATER_TOKEN !== updater.environment.WATCHTOWER_HTTP_API_TOKEN) {
  throw new Error("internal updater tokens differ");
}
if ((app.volumes || []).some((v) => String(v.source || v).includes("docker.sock"))) {
  throw new Error("application container received docker.sock");
}
if (!(updater.volumes || []).some((v) => String(v.source || v).includes("docker.sock"))) {
  throw new Error("updater sidecar is missing docker.sock");
}
NODE

POLARIS_RUNTIME_USER=0:0 compose --env-file "$tmp/empty.env" \
  -f docker-compose.yml -f docker-compose.update.yml \
  config --format json > "$tmp/legacy.json"

node - "$tmp/legacy.json" <<'NODE'
const fs = require("fs");
const c = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
if (c.services.polaris.user !== "0:0") {
  throw new Error(`unexpected legacy app user ${c.services.polaris.user}`);
}
NODE

POLARIS_HTTP_PORT=19092 compose --env-file "$tmp/empty.env" \
  -f docker-compose.yml -f docker-compose.update.yml \
  config --format json > "$tmp/custom-port.json"

node - "$tmp/custom-port.json" <<'NODE'
const fs = require("fs");
const c = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const published = c.services.polaris.ports?.[0]?.published;
if (String(published) !== "19092") throw new Error(`unexpected published HTTP port ${published}`);
NODE

printf '%s\n' "NAS compose contract: ok"
