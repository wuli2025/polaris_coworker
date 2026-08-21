#!/bin/sh
set -eu

EXPECTED_VERSION=2.9.2
TARGET=${POLARIS_TARGET:-polaris-web}
SITE_BASE=${POLARIS_SITE_BASE:-https://llmwiki.cloud}
PROJECT_NAME=${POLARIS_PROJECT_NAME:-polaris-v292}
HEALTH_ATTEMPTS=${POLARIS_HEALTH_ATTEMPTS:-90}
HEALTH_SLEEP=${POLARIS_HEALTH_SLEEP:-2}

if [ -d /volume1/docker ]; then
  STACK_DIR=${POLARIS_STACK_DIR:-/volume1/docker/polaris-stack}
else
  STACK_DIR=${POLARIS_STACK_DIR:-/opt/polaris-stack}
fi

die() {
  printf '%s\n' "Polaris NAS：$*" >&2
  exit 1
}

command -v docker >/dev/null 2>&1 || die "未找到 Docker"
docker compose version >/dev/null 2>&1 || die "需要 Docker Compose v2"
docker info >/dev/null 2>&1 || die "无法连接 Docker，请使用有 Docker 权限的账号运行"
command -v curl >/dev/null 2>&1 || die "未找到 curl"
command -v awk >/dev/null 2>&1 || die "未找到 awk"
command -v sha256sum >/dev/null 2>&1 || die "未找到 sha256sum"

mkdir -p "$STACK_DIR" || die "无法创建部署目录 $STACK_DIR"
BASE_FILE=$STACK_DIR/docker-compose.yml
UPDATE_FILE=$STACK_DIR/docker-compose.update.yml
ENV_EXAMPLE=$STACK_DIR/env.server.example
ENV_FILE=$STACK_DIR/.env
LEGACY_FILE=$STACK_DIR/docker-compose.legacy-data.yml

copy_atomic() {
  source_file=$1
  target_file=$2
  [ "$source_file" = "$target_file" ] && return 0
  temp_file=$target_file.tmp.$$
  cp "$source_file" "$temp_file" || die "无法复制 $source_file"
  mv "$temp_file" "$target_file" || die "无法写入 $target_file"
}

json_string() {
  json_key=$1
  json_file=$2
  awk -v key="$json_key" '
    $0 ~ "\\\"" key "\\\"[[:space:]]*:[[:space:]]*\\\"" {
      line=$0
      sub(".*\\\"" key "\\\"[[:space:]]*:[[:space:]]*\\\"", "", line)
      sub("\\\".*", "", line)
      print line
      exit
    }
  ' "$json_file"
}

json_object_string() {
  json_object=$1
  json_key=$2
  json_file=$3
  awk -v object="$json_object" -v key="$json_key" '
    $0 ~ "\\\"" object "\\\"[[:space:]]*:[[:space:]]*\\{" { inside=1; next }
    inside && $0 ~ "^[[:space:]]*}" { exit }
    inside && $0 ~ "\\\"" key "\\\"[[:space:]]*:[[:space:]]*\\\"" {
      line=$0
      sub(".*\\\"" key "\\\"[[:space:]]*:[[:space:]]*\\\"", "", line)
      sub("\\\".*", "", line)
      print line
      exit
    }
  ' "$json_file"
}

download_verified() {
  download_url=$1
  expected_sha=$2
  target_file=$3
  case "$download_url" in
    https://*) ;;
    *) die "分发清单包含非 HTTPS 地址" ;;
  esac
  printf '%s' "$expected_sha" | grep -Eq '^[0-9a-f]{64}$' || die "分发清单缺少有效 SHA-256"
  temp_file=$target_file.download.$$
  if ! curl -fsSL "$download_url" -o "$temp_file"; then
    rm -f "$temp_file"
    die "下载失败：$download_url"
  fi
  actual_sha=$(sha256sum "$temp_file" | awk '{print $1}')
  if [ "$actual_sha" != "$expected_sha" ]; then
    rm -f "$temp_file"
    die "下载校验失败：$download_url"
  fi
  mv "$temp_file" "$target_file" || die "无法写入 $target_file"
}

repo_root=
repo_env_source=
if command -v git >/dev/null 2>&1; then
  repo_root=$(git -C "$(pwd)" rev-parse --show-toplevel 2>/dev/null || true)
fi

if [ -n "$repo_root" ] &&
   [ -f "$repo_root/docker-compose.yml" ] &&
   [ -f "$repo_root/docker-compose.update.yml" ] &&
   [ -f "$repo_root/.env.server.example" ] &&
   grep -Eq '"version"[[:space:]]*:[[:space:]]*"2\.9\.2"' "$repo_root/package.json"; then
  copy_atomic "$repo_root/docker-compose.yml" "$BASE_FILE"
  copy_atomic "$repo_root/docker-compose.update.yml" "$UPDATE_FILE"
  copy_atomic "$repo_root/.env.server.example" "$ENV_EXAMPLE"
  if [ -f "$repo_root/.env" ]; then
    repo_env_source=$repo_root/.env
  fi
  printf '%s\n' "使用当前 Git 目录中的 Polaris $EXPECTED_VERSION 部署文件。"
else
  manifest=$STACK_DIR/latest.json.download.$$
  if ! curl -fsSL "$SITE_BASE/downloads/docker/latest.json" -o "$manifest"; then
    rm -f "$manifest"
    die "无法读取官网最新版清单"
  fi
  manifest_version=$(json_string version "$manifest")
  [ "$manifest_version" = "$EXPECTED_VERSION" ] || die "官网清单版本不是 $EXPECTED_VERSION"

  base_url=$(json_object_string compose base "$manifest")
  update_url=$(json_object_string compose update "$manifest")
  env_url=$(json_object_string compose env "$manifest")
  base_sha=$(json_object_string sha256 composeBase "$manifest")
  update_sha=$(json_object_string sha256 composeUpdate "$manifest")
  env_sha=$(json_object_string sha256 envExample "$manifest")

  download_verified "$base_url" "$base_sha" "$BASE_FILE"
  download_verified "$update_url" "$update_sha" "$UPDATE_FILE"
  download_verified "$env_url" "$env_sha" "$ENV_EXAMPLE"
  mv "$manifest" "$STACK_DIR/latest.json" || die "无法保存最新版清单"
  printf '%s\n' "已下载并校验 Polaris $EXPECTED_VERSION 部署文件。"
fi

env_origin=existing
if [ ! -f "$ENV_FILE" ]; then
  if [ -n "$repo_env_source" ]; then
    copy_atomic "$repo_env_source" "$ENV_FILE"
    env_origin=repository
  else
    copy_atomic "$ENV_EXAMPLE" "$ENV_FILE"
    env_origin=template
  fi
fi

env_set() {
  env_key=$1
  env_value=$2
  temp_file=$ENV_FILE.tmp.$$
  awk -v key="$env_key" '
    index($0, key "=") == 1 { next }
    { print }
  ' "$ENV_FILE" > "$temp_file"
  printf '%s=%s\n' "$env_key" "$env_value" >> "$temp_file"
  mv "$temp_file" "$ENV_FILE"
}

env_get() {
  env_key=$1
  awk -v key="$env_key" '
    index($0, key "=") == 1 {
      line=$0
      sub("^[^=]*=", "", line)
      value=line
    }
    END { print value }
  ' "$ENV_FILE"
}

# 当前 NAS 产品约定为免口令访问；账号体系上线前不让这些开关阻断使用。
env_set POLARIS_BIND_IP 0.0.0.0
env_set POLARIS_AUTH_TOKEN ""
env_set POLARIS_REQUIRE_LOGIN ""
env_set POLARIS_LAN_ONLY ""

# updater token 只属于 Compose 私网。安装时静默生成独立值，不向用户展示或索取。
# 极简 NAS 没有 openssl 时改读内核随机源；再不具备时用本机状态做一次性散列兜底。
if ! grep -Eq '^POLARIS_UPDATER_TOKEN=.+$' "$ENV_FILE"; then
  token=
  if command -v openssl >/dev/null 2>&1; then
    token=$(openssl rand -hex 32 2>/dev/null || true)
  fi
  if [ -z "$token" ] && command -v od >/dev/null 2>&1 && command -v tr >/dev/null 2>&1 && [ -r /dev/urandom ]; then
    token=$(od -An -N32 -tx1 /dev/urandom 2>/dev/null | tr -d ' \n' || true)
  fi
  if ! printf '%s' "$token" | grep -Eq '^[0-9a-f]{64}$'; then
    token=$(
      {
        date +%s 2>/dev/null || true
        uname -n 2>/dev/null || true
        printf '%s\n' "$$"
      } | sha256sum | awk '{print $1}'
    )
  fi
  env_set POLARIS_UPDATER_TOKEN "$token"
fi
chmod 600 "$ENV_FILE" || die "无法收紧 .env 权限"

timestamp=$(date +%Y%m%d-%H%M%S)
legacy_exists=0
legacy_name=
backup_dir=
if docker inspect "$TARGET" >/dev/null 2>&1; then
  legacy_exists=1
  backup_dir=$STACK_DIR/recovery/$timestamp-$$
  mkdir -p "$backup_dir" || die "无法创建恢复目录"
  docker inspect "$TARGET" > "$backup_dir/container-inspect.json" || die "无法备份旧容器信息"

  root_data_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/root/Polaris"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET")
  root_claude_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/root/.claude"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET")
  root_config_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/root/.config"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET" || true)
  home_data_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/home/polaris/Polaris"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET")
  home_claude_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/home/polaris/.claude"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET")
  home_config_mount=$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/home/polaris/.config"}}{{.Type}}|{{.Name}}|{{.Source}}{{end}}{{end}}' "$TARGET" || true)

  if [ -n "$root_data_mount" ] || [ -n "$root_claude_mount" ]; then
    [ -n "$root_data_mount" ] && [ -n "$root_claude_mount" ] ||
      die "旧 root 数据挂载不完整；inspect 已保存到 $backup_dir"
    [ -z "$home_data_mount" ] && [ -z "$home_claude_mount" ] ||
      die "旧容器同时包含 root/home 数据布局，拒绝猜测"
    data_mount=$root_data_mount
    claude_mount=$root_claude_mount
    config_mount=$root_config_mount
    runtime_user=0:0
  elif [ -n "$home_data_mount" ] || [ -n "$home_claude_mount" ]; then
    [ -n "$home_data_mount" ] && [ -n "$home_claude_mount" ] ||
      die "旧 home 数据挂载不完整；inspect 已保存到 $backup_dir"
    data_mount=$home_data_mount
    claude_mount=$home_claude_mount
    config_mount=$home_config_mount
    runtime_user=1000:1000
  else
    die "未识别旧容器的 root/home 数据挂载；inspect 已保存到 $backup_dir"
  fi

  legacy_networks=$(docker inspect -f '{{range $name, $_ := .NetworkSettings.Networks}}{{println $name}}{{end}}' "$TARGET" || true)
  retained_networks=
  for network_name in $legacy_networks; do
    case "$network_name" in bridge|host|none) continue ;; esac
    retained_networks="${retained_networks}${retained_networks:+ }$network_name"
  done
  legacy_port=$(docker inspect -f '{{with (index .HostConfig.PortBindings "8080/tcp")}}{{(index . 0).HostPort}}{{end}}' "$TARGET")
  printf '%s' "$legacy_port" | grep -Eq '^[0-9]{1,5}$' || die "未识别旧容器的 8080/tcp 宿主机端口"
  [ "$legacy_port" -ge 1 ] && [ "$legacy_port" -le 65535 ] || die "旧容器宿主机端口无效：$legacy_port"
  host_port=$legacy_port
  env_set POLARIS_HTTP_PORT "$host_port"

  # Git 目录会整份保留原 .env；无 Git 的旧容器则只迁移已知业务配置。
  # 访问口令、旧 Docker socket/更新器和旧镜像变量不在白名单内。
  legacy_env=$(docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$TARGET" || true)
  for env_key in \
    ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN \
    POLARIS_SMTP_HOST POLARIS_SMTP_PORT POLARIS_SMTP_USER POLARIS_SMTP_PASS \
    POLARIS_SMTP_FROM POLARIS_EMAIL_SIGNUP \
    POLARIS_ACCOUNT_AUTHORITY POLARIS_ACCOUNT_AUTHORITY_URL \
    POLARIS_ACCOUNT_OPEN_SIGNUP POLARIS_ACCOUNT_KEY POLARIS_REPO_ROOT \
    POLARIS_IROH_PORT POLARIS_RELAYS POLARIS_TUNNEL_AUTOSTART \
    POLARIS_TUNNEL_WORKERS POLARIS_TUNNEL_BUF_KB POLARIS_FS_ZSTD_LEVEL \
    POLARIS_FABLE_WORKERS POLARIS_FABLE_MMAP_MB POLARIS_FABLE_CACHE_MB \
    POLARIS_MEM_LIMIT GITEA_MEM_LIMIT GITEA_ADMIN_USER \
    GITEA_ADMIN_PASSWORD GITEA_ADMIN_EMAIL
  do
    current_value=$(env_get "$env_key")
    if [ "$env_origin" != template ] && [ -n "$current_value" ]; then
      continue
    fi
    legacy_value=$(printf '%s\n' "$legacy_env" | awk -v key="$env_key" '
      index($0, key "=") == 1 {
        sub("^[^=]*=", "", $0)
        print
        exit
      }
    ')
    [ -z "$legacy_value" ] || env_set "$env_key" "$legacy_value"
  done
  data_type=$(printf '%s' "$data_mount" | cut -d '|' -f 1)
  data_name=$(printf '%s' "$data_mount" | cut -d '|' -f 2)
  data_source=$(printf '%s' "$data_mount" | cut -d '|' -f 3-)
  claude_type=$(printf '%s' "$claude_mount" | cut -d '|' -f 1)
  claude_name=$(printf '%s' "$claude_mount" | cut -d '|' -f 2)
  claude_source=$(printf '%s' "$claude_mount" | cut -d '|' -f 3-)

  case "$data_type" in bind|volume) ;; *) die "旧数据挂载类型不受支持：$data_type" ;; esac
  case "$claude_type" in bind|volume) ;; *) die "旧 Claude 挂载类型不受支持：$claude_type" ;; esac
  [ "$data_type" != bind ] || [ -n "$data_source" ] || die "旧数据 bind 路径为空"
  [ "$claude_type" != bind ] || [ -n "$claude_source" ] || die "旧 Claude bind 路径为空"
  [ "$data_type" != volume ] || [ -n "$data_name" ] || die "旧数据卷名为空"
  [ "$claude_type" != volume ] || [ -n "$claude_name" ] || die "旧 Claude 卷名为空"

  yaml_quote() {
    printf '%s' "$1" | sed "s/'/''/g"
  }
  data_source_q=$(yaml_quote "$data_source")
  data_name_q=$(yaml_quote "$data_name")
  claude_source_q=$(yaml_quote "$claude_source")
  claude_name_q=$(yaml_quote "$claude_name")

  legacy_temp=$LEGACY_FILE.tmp.$$
  {
    printf '%s\n' 'services:' '  polaris:' '    volumes:'
    if [ "$data_type" = bind ]; then
      printf '%s\n' '      - type: bind' "        source: '$data_source_q'" '        target: /home/polaris/Polaris'
    else
      printf '%s\n' '      - type: volume' '        source: legacy-polaris-data' '        target: /home/polaris/Polaris'
    fi
    if [ "$claude_type" = bind ]; then
      printf '%s\n' '      - type: bind' "        source: '$claude_source_q'" '        target: /home/polaris/.claude'
    else
      printf '%s\n' '      - type: volume' '        source: legacy-polaris-claude' '        target: /home/polaris/.claude'
    fi
    if [ -n "$config_mount" ]; then
      config_type=$(printf '%s' "$config_mount" | cut -d '|' -f 1)
      config_name=$(printf '%s' "$config_mount" | cut -d '|' -f 2)
      config_source=$(printf '%s' "$config_mount" | cut -d '|' -f 3-)
      case "$config_type" in
        bind)
          config_source_q=$(yaml_quote "$config_source")
          printf '%s\n' '      - type: bind' "        source: '$config_source_q'" '        target: /home/polaris/.config'
          ;;
        volume)
          config_name_q=$(yaml_quote "$config_name")
          printf '%s\n' '      - type: volume' '        source: legacy-polaris-config' '        target: /home/polaris/.config'
          ;;
        *) die "旧 config 挂载类型不受支持：$config_type" ;;
      esac
    fi
    if [ -n "$retained_networks" ]; then
      printf '%s\n' '    networks:' '      - default'
      network_index=0
      for network_name in $retained_networks; do
        network_index=$((network_index + 1))
        printf '%s\n' "      - legacy-network-$network_index"
      done
    fi
    if [ "$data_type" = volume ] || [ "$claude_type" = volume ] ||
       { [ -n "$config_mount" ] && [ "${config_type:-}" = volume ]; }; then
      printf '%s\n' 'volumes:'
      if [ "$data_type" = volume ]; then
        printf '%s\n' '  legacy-polaris-data:' '    external: true' "    name: '$data_name_q'"
      fi
      if [ "$claude_type" = volume ]; then
        printf '%s\n' '  legacy-polaris-claude:' '    external: true' "    name: '$claude_name_q'"
      fi
      if [ -n "$config_mount" ] && [ "${config_type:-}" = volume ]; then
        printf '%s\n' '  legacy-polaris-config:' '    external: true' "    name: '$config_name_q'"
      fi
    fi
    if [ -n "$retained_networks" ]; then
      printf '%s\n' 'networks:'
      network_index=0
      for network_name in $retained_networks; do
        network_index=$((network_index + 1))
        network_name_q=$(yaml_quote "$network_name")
        printf '%s\n' "  legacy-network-$network_index:" '    external: true' "    name: '$network_name_q'"
      done
    fi
  } > "$legacy_temp"
  mv "$legacy_temp" "$LEGACY_FILE" || die "无法写入旧数据挂载配置"
  env_set POLARIS_RUNTIME_USER "$runtime_user"
  legacy_name=$TARGET-legacy-$timestamp-$$
else
  rm -f "$LEGACY_FILE"
  env_set POLARIS_RUNTIME_USER ""
  host_port=$(env_get POLARIS_HTTP_PORT)
  [ -n "$host_port" ] || host_port=8080
  printf '%s' "$host_port" | grep -Eq '^[0-9]{1,5}$' || die "POLARIS_HTTP_PORT 必须是有效端口"
  [ "$host_port" -ge 1 ] && [ "$host_port" -le 65535 ] || die "POLARIS_HTTP_PORT 超出范围"
  env_set POLARIS_HTTP_PORT "$host_port"
fi

# Compose 优先读取调用者已经 export 的变量。这里把产品约定和探测结果明确导出，
# 避免旧安装命令残留的访问口令或端口覆盖刚写入的 .env。
POLARIS_BIND_IP=0.0.0.0
POLARIS_AUTH_TOKEN=
POLARIS_REQUIRE_LOGIN=
POLARIS_LAN_ONLY=
POLARIS_HTTP_PORT=$host_port
POLARIS_RUNTIME_USER=$(env_get POLARIS_RUNTIME_USER)
POLARIS_UPDATER_TOKEN=$(env_get POLARIS_UPDATER_TOKEN)
export POLARIS_BIND_IP POLARIS_AUTH_TOKEN POLARIS_REQUIRE_LOGIN POLARIS_LAN_ONLY
export POLARIS_HTTP_PORT POLARIS_RUNTIME_USER POLARIS_UPDATER_TOKEN

cd "$STACK_DIR"
compose() {
  if [ -f "$LEGACY_FILE" ]; then
    docker compose -p "$PROJECT_NAME" -f "$BASE_FILE" -f "$UPDATE_FILE" -f "$LEGACY_FILE" "$@"
  else
    docker compose -p "$PROJECT_NAME" -f "$BASE_FILE" -f "$UPDATE_FILE" "$@"
  fi
}

completed=0
deployment_started=0
legacy_state=running

rollback() {
  if [ "$deployment_started" -eq 1 ]; then
    compose down >/dev/null 2>&1 || true
  fi
  if [ "$legacy_exists" -eq 1 ]; then
    case "$legacy_state" in
      renamed)
        if docker inspect "$TARGET" >/dev/null 2>&1; then
          docker rm -f "$TARGET" >/dev/null 2>&1 || true
        fi
        docker rename "$legacy_name" "$TARGET" >/dev/null 2>&1 || true
        docker start "$TARGET" >/dev/null 2>&1 || true
        printf '%s\n' "新容器验证失败，已恢复旧容器 $TARGET。" >&2
        ;;
      stopped)
        docker start "$TARGET" >/dev/null 2>&1 || true
        printf '%s\n' "迁移中断，已重新启动旧容器 $TARGET。" >&2
        ;;
    esac
  fi
}

on_exit() {
  exit_code=$1
  trap - EXIT HUP INT TERM
  if [ "$completed" -ne 1 ]; then
    rollback
  fi
  exit "$exit_code"
}
trap 'on_exit $?' EXIT
trap 'on_exit 129' HUP
trap 'on_exit 130' INT
trap 'on_exit 143' TERM

# 拉取发生在停旧容器之前；网络失败不会造成业务中断。
compose pull polaris polaris-updater || die "新镜像拉取失败，旧容器未改变"

if [ "$legacy_exists" -eq 1 ]; then
  docker stop "$TARGET" >/dev/null || die "无法停止旧容器 $TARGET"
  legacy_state=stopped
  if ! docker rename "$TARGET" "$legacy_name"; then
    die "无法为旧容器创建恢复名称"
  fi
  legacy_state=renamed
fi

deployment_started=1
compose up -d --no-build || die "新容器启动失败"

ready=0
attempt=0
while [ "$attempt" -lt "$HEALTH_ATTEMPTS" ]; do
  if curl -fsS "http://127.0.0.1:$host_port/api/ready" >/dev/null 2>&1; then
    ready=1
    break
  fi
  attempt=$((attempt + 1))
  sleep "$HEALTH_SLEEP"
done
[ "$ready" -eq 1 ] || die "180 秒内服务未就绪"

build_json=$(curl -fsS "http://127.0.0.1:$host_port/api/build") || die "无法读取新容器 build 信息"
build_version=$(printf '%s' "$build_json" | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
build_revision=$(printf '%s' "$build_json" | sed -n 's/.*"buildRevision"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
[ "$build_version" = "$EXPECTED_VERSION" ] || die "新容器版本不是 $EXPECTED_VERSION"
printf '%s' "$build_revision" | grep -Eq '^[0-9a-f]{40}$' || die "新容器缺少有效 buildRevision"

completed=1
trap - EXIT HUP INT TERM
if [ "$legacy_exists" -eq 1 ]; then
  printf '%s\n' "Polaris $EXPECTED_VERSION 迁移完成。旧容器已停止并保留为：$legacy_name"
  printf '%s\n' "确认数据无误后可自行删除该恢复容器；脚本不会自动删除它。"
else
  printf '%s\n' "Polaris $EXPECTED_VERSION 安装完成。"
fi
printf '%s\n' "Build revision：$build_revision"
