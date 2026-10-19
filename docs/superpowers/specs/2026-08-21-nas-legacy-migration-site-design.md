# NAS 旧版迁移与官网最新版分发设计

**日期：** 2026-08-21
**状态：** 已获用户确认
**目标版本：** Polaris 2.9.2（`latest` 持续按 OCI revision 更新）

## 1. 目标

让现有 NAS 用户有两条清晰且可验证的升级路径：

1. 2.7/2.8 及旧安装架构只做一次宿主机迁移，保留数据与现有端口，进入 2.9.2 的 Compose + 隔离 Watchtower 架构；
2. 迁移后的 2.9.2 及后续版本直接在 Polaris“更新”页面检查并替换容器。

官网 `https://llmwiki.cloud/nas#nas`、安装脚本与机器可读清单必须指向同一套最新镜像身份，不能再出现页面写 2.6.2、R2 清单写 2.7.0、GHCR 已是 2.9.2 的分裂状态。

用户明确要求 NAS 暂时保持完全免口令体验，后续再由账号体系接管。因此不询问、生成或强制配置 `POLARIS_AUTH_TOKEN`，也不要求 `POLARIS_REQUIRE_LOGIN` 或 `POLARIS_LAN_ONLY`。`POLARIS_UPDATER_TOKEN` 只用于 Polaris 容器到 Watchtower sidecar 的内部协议鉴权，不是用户访问口令；安装/迁移脚本可用时静默生成随机值，Compose 同时提供仅在项目私有网络内使用的非空内部默认值。该值不展示、不向用户索取，缺少 `openssl` 也不得卡住安装或更新。

## 2. 已确认的根因

- 官网 NAS 页面仍展示 2.6.2，并继续推荐旧的 R2 镜像体安装方案。
- R2 的 `downloads/docker/polaris-image-manifest.txt` 仍返回 2.7.0，旧容器据此正确地、但错误地认为 2.7.0 是最新版本。
- `https://llmwiki.cloud/downloads/docker` 裸路径回退到了官网 HTML，不是版本清单。
- 旧镜像把数据挂在 `/root/Polaris`、`/root/.claude`、`/root/.config`，默认以 root 运行，并把 Docker socket 直接挂入应用容器。
- 2.9.2 把数据挂在 `/home/polaris/Polaris`、`/home/polaris/.claude`，以 UID/GID 1000 运行，并把 Docker socket 隔离到固定版本的 Watchtower sidecar。
- 因此仅把旧 R2 清单的版本改成 2.9.2 会让旧更新器直接重建新镜像，却仍挂旧目标路径，造成“容器升级成功但数据看似消失”，且不会创建 Watchtower。该做法禁止采用。

## 3. 总体架构

### 3.1 路径 A：有 Git 目录的旧部署

官网展示一组可复制命令，在原 Polaris 仓库目录执行：

1. 切回并快进更新 `main`；
2. 保留已有 `.env`，不存在时从 `.env.server.example` 创建；
3. 缺少 `POLARIS_UPDATER_TOKEN` 时尽量静默生成；生成工具不可用时使用 Compose 内部默认值继续启动；
4. 保持 `POLARIS_AUTH_TOKEN`、`POLARIS_REQUIRE_LOGIN` 与 `POLARIS_LAN_ONLY` 为空，设置 `POLARIS_BIND_IP=0.0.0.0` 供 NAS 所在网络访问；
5. 执行仓库内的迁移准备命令：记录旧容器身份并在 `.env` 写入 `POLARIS_RUNTIME_USER=0:0`，沿用旧版默认的 root 数据属主，但不再给应用容器 Docker socket；
6. 拉取并启动 `docker-compose.yml` 与 `docker-compose.update.yml`；
7. 轮询 `/api/ready`，再读取 `/api/build` 验证版本和 revision。

Compose 项目沿用原目录和卷名，所以命名卷不会被删除。迁移命令不得执行 `down -v`、volume prune 或任何递归删除。

### 3.2 路径 B：没有 Git 目录的旧部署

仓库新增可测试的 `docker/nas-bootstrap.sh`，官网发布为：

- `/docker/nas-bootstrap.sh`：规范地址；
- `/docker/install-r2.sh`：保留历史公开地址，内容与规范脚本一致，避免旧文档失效。

脚本自动区分首次安装和旧容器迁移：

- 首次安装：下载经 SHA-256 固定的 2.9.2 Compose 三件套到独立 stack 目录，创建 `.env`，启动 Polaris 与 updater；
- 发现 `polaris-web` 旧容器：先保存完整 `docker inspect`，读取旧数据/Claude 卷的 mount 类型与 source，生成只覆盖目标路径的 `docker-compose.legacy-data.yml`，把旧 source 分别挂到新版 `/home/polaris/Polaris` 与 `/home/polaris/.claude`；
- 先拉新镜像再停旧容器；旧容器只停止并改名为带时间戳的恢复容器，不立即删除；
- 新容器通过 `/api/ready` 与 `/api/build` 后才报告成功；失败时停止新容器，把旧容器恢复原名并启动；
- 旧部署默认由 root 写入数据。迁移栈写入 `POLARIS_RUNTIME_USER=0:0`，让新版容器继续以原权限读取同一数据；不做递归 `chown`，避免对大知识库、NAS ACL 或嵌套 mount 造成不可逆变化。新安装仍使用镜像默认的 UID/GID 1000。应用容器即使以 root 运行也不再持有 Docker socket。

脚本允许通过环境变量覆盖 stack 目录和容器名，但默认无交互、不会询问任何 token。
基础 `docker-compose.yml` 增加可选的 `POLARIS_RUNTIME_USER` 映射；未设置时保持镜像默认用户，只有检测到旧 root 数据布局的迁移脚本才写入 `0:0`。

### 3.3 路径 C：2.9.2 及后续页面更新

`POLARIS_UPDATER_TOKEN` 继续保护内部 Watchtower HTTP API；该端口不发布到宿主机。Compose overlay 保证两端总能拿到同一个非空内部值；脚本生成的随机值优先，没有生成工具时退回项目内固定默认值。只要固定的 updater 内网地址和内部值已配置，应用侧就启用远程更新能力，不再要求用户访问口令、账号登录或 LAN-only 开关。

这意味着当前任何能够打开该 NAS Polaris 页面的人都可以触发一次官方镜像检查/替换，符合现阶段“NAS 本身只在可达网络内使用”的产品约定。触发方不能指定镜像、容器、命令或 Docker API：应用只能请求固定内网 Watchtower 地址，Watchtower 仍只更新带 enable 标签且名为 `polaris-web` 的官方镜像。未来账号体系上线后，再把触发入口收敛到账号 owner 权限，本次不提前加入用户口令。

NAS 默认 `.env` 为：

```dotenv
POLARIS_BIND_IP=0.0.0.0
POLARIS_AUTH_TOKEN=
POLARIS_REQUIRE_LOGIN=
# POLARIS_LAN_ONLY 留空；网络可达范围由 NAS/路由器自身决定
# POLARIS_UPDATER_TOKEN 可由脚本静默追加；省略时 Compose 使用内部默认值
POLARIS_IMAGE_REPO=ghcr.io/wuli2025/polaris_coworker
POLARIS_IMAGE_TAG=latest
# 仅旧版数据迁移写入；全新安装省略并使用镜像默认 UID/GID 1000
POLARIS_RUNTIME_USER=0:0
```

## 4. 官网与分发契约

### 4.1 NAS 页面

`/nas#nas` 的 NAS 区域改成两个主卡片：

1. “我现在是 2.7/2.8：一次迁移到 2.9.2”，分别给有 Git 与无 Git 两种命令；
2. “我已经是 2.9.2：以后在页面点更新”，说明检查、替换、短暂断线和 `/api/build` 验证。

页面删除 2.6.2 tar、旧 `ghcr.io/wuli2025/polaris`、应用容器直挂 Docker socket、旧 `POLARIS_UPDATE_URL` 等说明。页面显示当前稳定版本、完整镜像名、发布 revision 短 SHA，并明确 2.9.2 同版本下仍以 OCI revision 判断是否需要更新。

### 4.2 机器可读清单

新增静态 `/downloads/docker/latest.json`。发布程序写入实际值，不保留模板变量。字段契约为：

- `version`：固定为当前产品版本 `2.9.2`；
- `buildRevision`：本次实际发布 commit 的完整 40 位 Git SHA；
- `image`：`ghcr.io/wuli2025/polaris_coworker:latest`；
- `digest`：该 tag 实际指向的 OCI index digest；
- `bootstrap`：`https://llmwiki.cloud/docker/nas-bootstrap.sh`；
- `compose.base`、`compose.update`、`compose.env`：官网 `docker/current/` 下三份已校验部署文件的绝对 URL；
- `sha256`：上述脚本和三份 Compose 文件各自的 SHA-256，bootstrap 下载后据此校验。

`/downloads/docker` 裸路径改为重定向到该 JSON，不再回退 HTML。JSON 与 Compose 文件使用 `no-store`；版本化脚本或不可变文件可使用短缓存并通过 SHA-256 校验。

旧 `polaris-image-manifest.txt` 保留为 legacy，只服务仍在 2.7 架构中的旧检查器。它不能伪装成 2.9.2 镜像清单。官网明确旧页面按钮不能跨架构，必须先走路径 A 或 B。

### 4.3 单一真相源

- 应用版本从仓库 `package.json` 读取；
- build revision 从实际推送到 `main` 的 commit 读取；
- image digest 从 GHCR 发布结果读取；
- 官网 JSON、页面与脚本在镜像发布成功后用上述三个值生成/更新；
- 部署前检查这些值互相一致，禁止手工复制旧版本号。

## 5. 错误处理与恢复

- 缺 Docker、Compose v2、curl 或必要权限时，在改变容器前退出；
- 无法识别旧数据 mount 时不猜路径，输出 inspect 备份位置并停止；
- 新镜像或 Watchtower 拉取失败时旧容器保持运行；
- 切换后的 readiness/build 验证失败时自动恢复旧容器；
- `POLARIS_UPDATER_TOKEN` 不写 stdout；脚本成功生成时只写 mode 600 的 `.env`，未生成时直接使用 Compose 内部默认值；
- 网站端点返回 HTML、JSON 字段缺失、版本/revision/digest 不一致时部署验证失败。

## 6. 测试与验收

### 6.1 单元/契约测试

- Rust：未设置用户访问口令、账号登录和 LAN-only 时，只要内部 updater 配置完整就启用远程更新；内部 URL 或 token 缺失时禁用；触发目标仍固定为官方镜像和 `polaris-web`；
- Shell：用 stub Docker/Compose 覆盖首次安装、有 Git、旧 bind mount、旧 named volume、旧 root runtime、拉取失败、健康失败回滚、token 已存在等分支；
- 静态站点：断言 NAS 区域只出现 2.9.2 新路径、不再出现旧 Docker 镜像说明，机器清单 schema 完整，脚本和 Compose 校验和匹配。

### 6.2 集成验证

- 用旧版卷布局创建测试容器与 sentinel 文件，运行迁移后确认 sentinel 在新版 `/home/polaris/Polaris` 可见；
- 确认应用容器没有 Docker socket，只有 updater sidecar 持有；
- 触发一次真实 Watchtower 更新，只有观测到新 boot ID、目标 OCI revision 与 readiness 恢复才算成功；
- 访问生产地址验证：
  - `/nas#nas` 展示两条路径和当前版本；
  - `/downloads/docker` 不再返回 HTML；
  - `/downloads/docker/latest.json` 为 2.9.2 且 revision/digest 与 GHCR 一致；
  - `/docker/install-r2.sh` 与 `/docker/nas-bootstrap.sh` 都返回 shell 内容而非回退页。

## 7. 发布顺序

1. 在应用仓库实现 LAN-only 更新授权、迁移脚本、测试与文档；
2. 推送 `main`，等待多架构 GHCR 与真实 Watchtower E2E 成功；
3. 读取新 revision 与 digest，生成官网 JSON 和页面内容；
4. 部署 Cloudflare Pages；
5. 对生产域名执行内容类型、正文、版本、revision、digest 和脚本语法终验。

本次仍使用产品版本 `2.9.2`，但发布后的 build revision 会是包含迁移能力的新 commit，不再是先前的 `ca6141a...`。同版本更新由 OCI revision 正确识别。
