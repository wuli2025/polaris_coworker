# Polaris 云服务器部署指南（Docker 一键上云）

> 面向「只会把文件传到服务器」的用户。照着从上到下做即可。
> 适用：一台 Linux 云服务器（Ubuntu 22.04/24.04、Debian 12 等），2 核 4G 起步，建议 4 核 8G。
>
> 官方镜像由 GitHub Actions 同时发布 `linux/amd64` 与 `linux/arm64`；常规部署不需要在服务器上编译 Rust。
> 只有修改了源码或要做定制镜像时才使用本地 `--build` 路径。

## 0. 装 Docker（服务器上执行一次）

```bash
curl -fsSL https://get.docker.com | bash
# 验证
docker --version && docker compose version
```

## 1. 把代码放到服务器

方式 A（推荐，便于以后升级）——git 克隆：

```bash
git clone <你的仓库地址> polaris-app
cd polaris-app
```

方式 B——本地打包上传：

```bash
# 本地（排除垃圾目录，压缩会小很多）
tar --exclude node_modules --exclude src-tauri/target --exclude .git \
    -czf polaris.tgz -C /path/to polaris-app
scp polaris.tgz root@服务器IP:~
# 服务器
tar xzf polaris.tgz && cd polaris-app
```

## 2. 填环境变量

```bash
cp .env.server.example .env
nano .env
```

必填两项：

- `POLARIS_AUTH_TOKEN`：机器级 owner 访问口令，必须使用独立强随机值。
  生成一个：`openssl rand -hex 32`
  （容器**默认是免口令**的——那是给 NAT 后面的家用 NAS 设的。云机对公网可达，
  这一项必须显式设上，否则等于把 owner 接口敞开。）
- `POLARIS_BIND_IP`：保持默认 `127.0.0.1`。这让 8080 只供宿主机上的 HTTPS
  反向代理访问，避免 owner 口令、聊天和文件走公网明文。
- LLM 接入（三选一）：`ANTHROPIC_API_KEY`，或第三方端点的
  `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN`，或都留空、起服务后在
  App 内「供应商」面板登录 Claude 订阅。

## 3. 拉取并启动

```bash
docker compose pull
docker compose up -d
```

默认拉取 `ghcr.io/wuli2025/polaris_coworker:latest`，会按服务器架构自动选择 amd64 或
arm64 镜像。只有需要从当前源码定制构建时才运行：

```bash
docker compose up -d --build
```

本地构建要编译 Rust，视机器 10–30 分钟属正常。看运行日志：

```bash
docker compose logs -f polaris
```

### 可选：启用网页一键更新

基础 compose **不会**挂载 Docker socket；更新页仍可检查版本，但不能自行替换容器。
确认部署已设置 `POLARIS_AUTH_TOKEN`（或 `POLARIS_REQUIRE_LOGIN=1`）并通过 HTTPS 访问后，
才可显式叠加更新配置：

```bash
# Linux / NAS：先把结果写入 .env 的 DOCKER_GID；Docker Desktop 通常保持 0
stat -c %g /var/run/docker.sock

docker compose -f docker-compose.yml -f docker-compose.update.yml up -d
```

此后 owner 可在 Polaris「更新」页点「立即更新容器」。Polaris 会启动固定版本的独立
Watchtower 替身，替身接手拉取和替换，因此当前容器退出不会中断更新；数据卷、端口、环境变量
保持不变，服务通常只短暂断线 1–3 分钟。

> **安全边界：**`/var/run/docker.sock` 等同宿主机 root 权限。不要在匿名、无鉴权或不可信
> 用户可访问的部署上启用。以后手动执行 `compose up/pull` 也要同时带这两个 `-f` 参数，
> 否则基础 compose 会撤掉 socket 挂载。固定到 `POLARIS_IMAGE_TAG=2.9.0` 等版本标签时不会
> 自动跨版本；要远程跟随发布版请保持 `latest`。

## 4. 验证

```bash
curl http://localhost:8080/api/health
# 期望返回 ok
curl -f http://localhost:8080/api/ready
# 期望返回 ready；数据卷、SQLite 或前端入口异常时会返回 503
docker compose ps   # polaris 应为 healthy
```

不要在云安全组开放 8080。先用 Caddy / Nginx / Traefik 把你的 HTTPS 域名反代到
`127.0.0.1:8080`，只开放 TCP 443；证书必须有效，并建议开启 HSTS。以下是最小 Caddyfile
示意（把域名换成已解析到本机的域名）：

```caddyfile
polaris.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

若开 iroh 中继，按需另放行 443 TCP/UDP。只有在完全可信的隔离内网调试时，才可把
`POLARIS_BIND_IP` 改为 `0.0.0.0`；不要用这个设置直接暴露公网。

## 5. 首次打开：建 owner 账号

浏览器访问：

```
https://polaris.example.com/?token=你的POLARIS_AUTH_TOKEN
```

页面读取口令后会立即从地址栏和浏览器历史中清除，并只保留在本次浏览器会话。不要把含
`?token=` 的初始链接发到群聊、工单或截图中。

首次进入会引导创建 **owner（所有者）账号**——这是多人协作的最高权限账号，
账号密码务必记牢。之后队友的账号由 owner 在管理面板发「邀请票据」加入。

协作项目当前只绑定 `POLARIS_REPO_ROOT` 内已经存在的本地 Git 仓库，不接受远程 URL，
也不会自动 clone。Docker 默认目录是 `/home/polaris/Polaris/repos`；请先把仓库放入对应
volume/挂载目录，再由 owner 在界面创建项目。此限制用于阻止成员借 Git 操作触达宿主机任意目录。

## 6.（实验性）Gitea 管理员

compose 带无头 Gitea（仅容器内可访问，未对公网开端口，且已禁自助注册），但当前项目
创建→仓库创建/克隆→成员 ACL 尚未自动编排，生产流程不要依赖这一 profile。
第一次需要创建管理员：

```bash
docker compose exec -u git gitea gitea admin user create \
  --admin --username "$GITEA_ADMIN_USER" \
  --password "$GITEA_ADMIN_PASSWORD" --email "$GITEA_ADMIN_EMAIL"
```

（变量即 .env 里 GITEA_ADMIN_* 三项；也可直接写明文。）

## 7.（实验性，当前未接入产品流程）iroh-relay

该容器只是中继实验组件；当前桌面发布未启用并接通完整的 NodeId 配对/隧道流程，不能靠
它实现“粘贴配对码自动连主机”。生产多人协作请使用上面的 HTTPS Docker 主机。若仅做
协议开发，需要 443 端口空闲并放行 443 TCP+UDP。

```bash
# 1) 生成纯 IP 自签证书（把 IP 换成你的公网 IP）
bash docker/relay/gen-cert.sh 203.0.113.7

# 2) 准备配置
cp docker/relay/config.toml.example docker/relay/config.toml

# 3) 带 relay profile 启动
docker compose --profile relay up -d
```

## 8. 数据备份与恢复

所有数据都在三个 docker volume 里：`polaris-data`（知识库/collab.db 等）、
`polaris-claude`（LLM 登录态）、`gitea-data`。

备份（打成 tar 落在当前目录）：

```bash
for v in polaris-data polaris-claude gitea-data; do
  docker run --rm -v ${PWD%/*}/_:/x -v $(basename $PWD)_$v:/data -v $PWD:/backup \
    debian:bookworm-slim tar czf /backup/$v-$(date +%F).tgz -C /data .
done
```

> 提示：volume 实际名字带 compose 项目前缀，`docker volume ls` 确认后替换。

恢复：新机器先 `docker compose up -d` 生成空卷再停掉，然后：

```bash
docker run --rm -v <卷名>:/data -v $PWD:/backup debian:bookworm-slim \
  bash -c "cd /data && tar xzf /backup/<备份文件>.tgz"
docker compose up -d
```

## 9. 升级

官方镜像部署有两条路径：

1. 已启用 `docker-compose.update.yml`：owner 直接在 Polaris「更新」页检查并一键更新。
2. 未挂 socket（安全默认）：在宿主机执行：

```bash
cd polaris-app
git pull
docker compose pull
docker compose up -d
docker image prune -f             # 可选：清理旧镜像层
```

如果部署的是本地定制源码，则继续使用：

```bash
git pull
docker compose up -d --build
```

数据在命名卷中，以上容器替换都不会删除数据。不要用 `docker compose down -v` 做升级。

## 构建排错

| 症状 | 处理 |
| --- | --- |
| `npm ci` 报 lock 不同步 | 确认上传了根目录 `package-lock.json`；或临时把 Dockerfile 里 `npm ci` 改 `npm install` |
| `vue-tsc` 类型报错卡住前端 | 本地先跑 `npm run build` 确认能过；不能过就先修类型错误 |
| Rust 编译报缺系统库（`*-sys` build failed） | 看报错缺什么，在 Dockerfile stage2 的 apt 行补装（已预装 pkg-config/libssl-dev/cmake/nasm/clang） |
| 报找不到 `polaris-server` bin | 构建命令必须是 `cargo build --release -p polaris-cli --bin polaris-server`（bin 在 crates/polaris-cli，不在主包） |
| 内存不足被 OOM 杀（编译期） | 加 swap：`fallocate -l 4G /swap && chmod 600 /swap && mkswap /swap && swapon /swap` |
| relay 镜像拉不到 | 见 docker-compose.yml relay 服务内注释，改用源码构建或 `cargo install iroh-relay` |
| healthcheck 一直 starting | `docker compose logs polaris` 看启动日志；常见是 .env 没填导致鉴权/供应商初始化告警（不致命）或端口被占 |
