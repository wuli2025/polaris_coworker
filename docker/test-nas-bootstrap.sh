#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
test_root=$(mktemp -d)
trap 'rm -rf "$test_root"' EXIT HUP INT TERM

fail() {
  printf '%s\n' "FAIL: $*" >&2
  exit 1
}

assert_file() {
  [ -f "$1" ] || fail "missing file $1"
}

assert_contains() {
  file=$1
  expected=$2
  grep -F -- "$expected" "$file" >/dev/null || fail "$file does not contain: $expected"
}

assert_not_contains() {
  file=$1
  unexpected=$2
  if grep -F -- "$unexpected" "$file" >/dev/null; then
    fail "$file unexpectedly contains: $unexpected"
  fi
}

make_stubs() {
  case_dir=$1
  mkdir -p "$case_dir/bin"

  cat > "$case_dir/bin/docker" <<'STUB'
#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$POLARIS_TEST_CALLS"

if [ "${1:-}" = compose ]; then
  case " $* " in
    *" pull "*)
      [ "$POLARIS_TEST_CASE" != pull_failure ] || exit 44
      ;;
  esac
  exit 0
fi

if [ "${1:-}" = inspect ]; then
  case "$POLARIS_TEST_CASE" in
    fresh_install|git_checkout|no_openssl) exit 1 ;;
  esac
  if [ "${2:-}" = -f ]; then
    format=${3:-}
    case "$format" in
      *'/root/Polaris'*)
        if [ "$POLARIS_TEST_CASE" = legacy_named_volume ]; then
          printf '%s\n' 'volume|old_polaris-data|/var/lib/docker/volumes/old_polaris-data/_data'
        else
          printf '%s\n' 'bind||/volume1/docker/polaris/data'
        fi
        ;;
      *'/root/.claude'*)
        if [ "$POLARIS_TEST_CASE" = legacy_named_volume ]; then
          printf '%s\n' 'volume|old_polaris-claude|/var/lib/docker/volumes/old_polaris-claude/_data'
        else
          printf '%s\n' 'bind||/volume1/docker/polaris/claude'
        fi
        ;;
      *'/root/.config'*) printf '%s\n' '' ;;
      *'PortBindings'*)
        if [ "$POLARIS_TEST_CASE" = custom_port ]; then
          printf '%s\n' '19092'
        else
          printf '%s\n' '8080'
        fi
        ;;
      *'.Config.Env'*)
        printf '%s\n' \
          'ANTHROPIC_AUTH_TOKEN=legacy-provider-secret' \
          'POLARIS_AUTH_TOKEN=must-not-migrate' \
          'POLARIS_DOCKER_SOCKET=1'
        ;;
    esac
  else
    printf '%s\n' '{"Id":"legacy-container"}'
  fi
  exit 0
fi

exit 0
STUB

  cat > "$case_dir/bin/curl" <<'STUB'
#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$POLARIS_TEST_CURL_CALLS"
out=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift 2 ;;
    -*) shift ;;
    *) url=$1; shift ;;
  esac
done

if [ -n "$out" ]; then
  case "$url" in
    */downloads/docker/latest.json)
      base_sha=$(sha256sum "$POLARIS_TEST_ASSETS/docker-compose.yml" | awk '{print $1}')
      update_sha=$(sha256sum "$POLARIS_TEST_ASSETS/docker-compose.update.yml" | awk '{print $1}')
      env_sha=$(sha256sum "$POLARIS_TEST_ASSETS/.env.server.example" | awk '{print $1}')
      printf '%s\n' \
        '{' \
        '  "version": "2.9.2",' \
        '  "compose": {' \
        '    "base": "https://test.invalid/docker/current/docker-compose.yml",' \
        '    "update": "https://test.invalid/docker/current/docker-compose.update.yml",' \
        '    "env": "https://test.invalid/docker/current/env.server.example"' \
        '  },' \
        '  "sha256": {' \
        "    \"composeBase\": \"$base_sha\"," \
        "    \"composeUpdate\": \"$update_sha\"," \
        "    \"envExample\": \"$env_sha\"" \
        '  }' \
        '}' > "$out"
      ;;
    */docker/current/docker-compose.yml) cp "$POLARIS_TEST_ASSETS/docker-compose.yml" "$out" ;;
    */docker/current/docker-compose.update.yml) cp "$POLARIS_TEST_ASSETS/docker-compose.update.yml" "$out" ;;
    */docker/current/env.server.example) cp "$POLARIS_TEST_ASSETS/.env.server.example" "$out" ;;
    *) exit 22 ;;
  esac
  exit 0
fi

case "$url" in
  */api/ready)
    [ "$POLARIS_TEST_CASE" != health_failure ] || exit 22
    printf '%s\n' '{"ready":true}'
    ;;
  */api/build)
    printf '%s\n' '{"version":"2.9.2","buildRevision":"1111111111111111111111111111111111111111"}'
    ;;
  *) exit 22 ;;
esac
STUB

  chmod +x "$case_dir/bin/docker" "$case_dir/bin/curl"
}

run_case() {
  name=$1
  cwd_mode=${2:-temporary}
  case_dir="$test_root/$name"
  stack="$case_dir/stack"
  calls="$case_dir/calls.log"
  curl_calls="$case_dir/curl-calls.log"
  output="$case_dir/output.log"
  mkdir -p "$stack"
  : > "$calls"
  : > "$curl_calls"
  make_stubs "$case_dir"

  if [ "$name" = no_openssl ]; then
    cat > "$case_dir/bin/openssl" <<'STUB'
#!/bin/sh
exit 1
STUB
    chmod +x "$case_dir/bin/openssl"
  fi

  if [ "$name" = legacy_named_volume ]; then
    printf '%s\n' 'POLARIS_UPDATER_TOKEN=already-internal' > "$stack/.env"
  fi

  if [ "$cwd_mode" = repository ]; then
    source_dir=$case_dir/source
    mkdir -p "$source_dir"
    cp "$root/docker-compose.yml" "$source_dir/docker-compose.yml"
    cp "$root/docker-compose.update.yml" "$source_dir/docker-compose.update.yml"
    cp "$root/.env.server.example" "$source_dir/.env.server.example"
    cp "$root/package.json" "$source_dir/package.json"
    printf '%s\n' \
      'ANTHROPIC_API_KEY=git-provider-secret' \
      'POLARIS_BIND_IP=127.0.0.1' > "$source_dir/.env"
    git -C "$source_dir" init -q
    work_dir=$source_dir
  else
    work_dir=$case_dir
  fi

  if (
    cd "$work_dir"
    PATH="$case_dir/bin:$PATH" \
      POLARIS_STACK_DIR="$stack" \
      POLARIS_SITE_BASE=https://test.invalid \
      POLARIS_HEALTH_ATTEMPTS=2 \
      POLARIS_HEALTH_SLEEP=0 \
      POLARIS_TEST_CASE="$name" \
      POLARIS_TEST_CALLS="$calls" \
      POLARIS_TEST_CURL_CALLS="$curl_calls" \
      POLARIS_TEST_ASSETS="$root" \
      sh "$root/docker/nas-bootstrap.sh"
  ) > "$output" 2>&1; then
    result=0
  else
    result=$?
  fi
}

run_case fresh_install
[ "$result" -eq 0 ] || fail "fresh install failed"
assert_file "$stack/.env"
assert_contains "$calls" "compose -p polaris-v292 -f $stack/docker-compose.yml -f $stack/docker-compose.update.yml pull polaris polaris-updater"
assert_not_contains "$output" "POLARIS_UPDATER_TOKEN="

run_case no_openssl
[ "$result" -eq 0 ] || fail "install without openssl failed"
grep -Eq '^POLARIS_UPDATER_TOKEN=[0-9a-f]{64}$' "$stack/.env" ||
  fail "install without openssl did not generate an invisible internal token"
assert_not_contains "$output" "POLARIS_UPDATER_TOKEN="

run_case git_checkout repository
[ "$result" -eq 0 ] || fail "git checkout install failed"
assert_file "$stack/docker-compose.yml"
assert_contains "$stack/.env" "ANTHROPIC_API_KEY=git-provider-secret"
assert_not_contains "$curl_calls" "downloads/docker/latest.json"

run_case legacy_bind_mount
[ "$result" -eq 0 ] || fail "legacy bind migration failed"
assert_contains "$stack/docker-compose.legacy-data.yml" "source: '/volume1/docker/polaris/data'"
assert_contains "$stack/docker-compose.legacy-data.yml" "target: /home/polaris/Polaris"
assert_contains "$stack/.env" "POLARIS_RUNTIME_USER=0:0"
assert_contains "$stack/.env" "ANTHROPIC_AUTH_TOKEN=legacy-provider-secret"
assert_contains "$stack/.env" "POLARIS_AUTH_TOKEN="
assert_not_contains "$stack/.env" "must-not-migrate"
assert_contains "$calls" "stop polaris-web"
assert_contains "$calls" "rename polaris-web polaris-web-legacy-"
assert_not_contains "$output" "POLARIS_UPDATER_TOKEN="

run_case legacy_named_volume
[ "$result" -eq 0 ] || fail "legacy named-volume migration failed"
assert_contains "$stack/docker-compose.legacy-data.yml" "external: true"
assert_contains "$stack/docker-compose.legacy-data.yml" "name: 'old_polaris-data'"
assert_contains "$stack/.env" "POLARIS_UPDATER_TOKEN=already-internal"
assert_not_contains "$output" "already-internal"

run_case custom_port
[ "$result" -eq 0 ] || fail "custom-port migration failed"
assert_contains "$stack/.env" "POLARIS_HTTP_PORT=19092"
assert_contains "$curl_calls" "http://127.0.0.1:19092/api/ready"
assert_contains "$curl_calls" "http://127.0.0.1:19092/api/build"

run_case pull_failure
[ "$result" -ne 0 ] || fail "pull failure unexpectedly succeeded"
assert_not_contains "$calls" "stop polaris-web"

run_case health_failure
[ "$result" -ne 0 ] || fail "health failure unexpectedly succeeded"
assert_contains "$calls" "rename polaris-web polaris-web-legacy-"
assert_contains "$calls" "rename polaris-web-legacy-"
assert_contains "$calls" "start polaris-web"
assert_not_contains "$calls" "down -v"

printf '%s\n' "NAS bootstrap tests: ok"
