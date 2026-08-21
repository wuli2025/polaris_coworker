# Polaris · Docker 版

把 Polaris（原 Tauri 桌面 AI 工作台）跑成**浏览器访问的容器服务**。
核心架构：**保留全部 Rust 引擎，用 axum HTTP/WS 外壳替代 Tauri 桌面外壳**。
桌面版与 Docker 版**共用同一份源码**——这是「Windows 更新后能快速更新 Docker」的根基。

---

## 一、快速开始

```bash
# 1) 准备环境变量
cp .env.server.example .env
#   编辑 .env，至少填一种模型鉴权：
#   - ANTHROPIC_API_KEY=sk-ant-...           （Claude 官方）
#   - 或 ANTHROPIC_BASE_URL + ANTHROPIC_AUTH_TOKEN（兼容端点）
#   真把服务暴露公网时必须设置 POLARIS_AUTH_TOKEN（或 POLARIS_REQUIRE_LOGIN=1），
#   并保持 POLARIS_BIND_IP=127.0.0.1、通过 HTTPS 反代接入。

# 2) 拉取官方多架构镜像并启动
docker compose pull
docker compose up -d

# 3) 浏览器打开
#   http://localhost:8080          ← 默认免口令，直接进
#   若设了口令：http://localhost:8080/?token=<你的口令>
```

官方镜像为 `ghcr.io/wuli2025/polaris_coworker:latest`，支持 `linux/amd64` 与
`linux/arm64`。只有需要从当前源码定制构建时才改用 `docker compose up -d --build`。

健康检查：`curl http://localhost:8080/api/health` → `ok`。

---

## 二、它是怎么接起来的（架构）

```
浏览器 (Vue3 前端，与桌面版同一份)
   │  src/tauri.ts 适配层：非 Tauri 环境自动改走 ↓
   ├── invoke(cmd,args)  ──HTTP──▶  POST /api/invoke   （≈75 个引擎命令分发）
   ├── listen(topic,cb)  ──WS────▶  GET  /ws           （emit 事件广播）
   └── 文件上传          ──multipart▶ POST /api/upload  （替代原生文件对话框）
                                          │
                              polaris-server (Rust · axum)
                                          │  src/host.rs 的 shim AppHandle
                                          │  把 app.emit() 转成 WS 广播
                              ┌───────────┴───────────┐
                              │ kb / chat / conv /     │  ← 桌面版同款 .rs，未改业务逻辑
                              │ provider / skills /... │     仅顶部 import + 命令宏 cfg 门控
                              └───────────┬───────────┘
                                  spawn   │  stdin 喂 prompt，解析 stream-json
                              ┌───────────▼───────────┐
                              │   claude CLI（镜像预装）│──▶ 各 LLM 供应商
                              └───────────────────────┘
```

关键实现（都在 `src-tauri/src/`）：

| 文件 | 作用 |
|---|---|
| `host.rs` | server 模式的 `AppHandle` 替身：`emit→broadcast`、`path().resource_dir()→/app/resources` |
| `server.rs` | axum 服务：`/api/invoke` 分发、`/ws` 推流、`/api/upload`、静态托管、可选口令鉴权 |
| `bin/polaris-server.rs` | server 二进制入口 |
| 各引擎模块 | `use tauri::AppHandle` → `#[cfg]` 门控双导入；`#[tauri::command]` → `#[cfg_attr(feature="desktop", tauri::command)]` |

`Cargo.toml`：`tauri` 等设为 **optional**，`default = ["desktop"]`，新增 `server` feature。
- 桌面构建：`cargo build`（默认 desktop）—— 一切照旧。
- Docker 构建：`cargo build --bin polaris-server --no-default-features --features server` —— 不拉 Tauri，Linux 无需 webkit2gtk。

---

## 三、更新 Docker 版

### 旧架构只迁移一次

没有 Git 目录时直接运行：

```bash
curl -fsSL https://llmwiki.cloud/docker/nas-bootstrap.sh | sudo sh
```

如果原安装目录就是 Polaris Git 仓库，在该目录先快进源码，再运行同一份迁移器：

```bash
git switch main
git pull --ff-only origin main
sudo sh docker/nas-bootstrap.sh
```

迁移脚本会保留旧数据挂载，并把旧容器停止后改名留作恢复；验证新容器失败时会自动恢复旧容器。
拉取新镜像发生在停止旧容器之前；它不会递归改文件属主，也不会询问用户访问口令或 updater token。

### 2.9.2 及后续：Polaris 更新页一键更新

标准 NAS 部署直接叠加更新 overlay：

```bash
docker compose -f docker-compose.yml -f docker-compose.update.yml up -d
```

该 overlay 启动固定版本 `containrrr/watchtower:1.7.1`。Polaris App 容器**没有** Docker CLI，
也**不挂** `/var/run/docker.sock`；它只能携带内部通信密钥调用内网 sidecar 的
`/v1/update`。Watchtower 还同时按容器名 `polaris-web` 与
`com.centurylinklabs.watchtower.enable=true` 标签限定目标，因此不会顺手更新其他容器。

“检查更新”读取当前 tag 的 OCI index、当前 CPU 架构 manifest 和 config labels，以 Git build
revision 判断（即使版本号未变，`latest` 指向新提交也能识别）。运行中的 build 身份读取镜像内
`/app/polaris-build-revision`，不相信 Watchtower 可能从旧容器保留下来的同名环境变量。点更新后 HTTP 请求立即返回
`requestId`，页面继续等待：只有 `/api/build` 同时出现**新 bootId + 目标 revision**，且
`/api/ready` 恢复，才会报告成功并重载页面。Watchtower 空 200/no-op 不算成功；拉取失败、
鉴权错误或 15 分钟仍未观察到替换都会退出转圈并保留明确错误，可修复后直接重试。更新过程
保持数据卷、端口、网络、标签、环境变量与 restart policy。

从不含 `/usr/local/bin/update.sh` 的旧镜像升级时，需要先在宿主机执行一次上面的迁移命令；
这是唯一一次 bootstrap，之后才会出现网页一键更新能力。排错先看：

```bash
docker logs polaris-updater
docker logs polaris-web
```

> Docker socket 仍等同宿主机 root 权限，但现在只进入用途单一的 Watchtower sidecar，
> 不再进入能够运行项目命令的通用 App 容器。不要对外发布 sidecar 的 8080 端口。内部通信
> 密钥由安装脚本或 Compose 私网默认值自动处理，不展示、不询问，也不会卡住用户安装。

本地定制镜像继续使用 `docker compose up -d --build`，不要叠加远程更新 overlay，以免
Watchtower 按配置的 GHCR 标签把本地定制构建替换为官方镜像。

---

## 四、数据持久化

| 卷 | 容器内路径 | 内容 |
|---|---|---|
| `polaris-data` | `/home/polaris/Polaris` | 知识库、对话历史、项目、协作数据库与账号权威密钥 |
| `polaris-claude` | `/home/polaris/.claude` | Claude 凭证、`settings.json`（供应商切换/OAuth 登录态） |
| `gitea-data` | `/data`（可选 Gitea 容器） | 只有启用 `gitea` profile 时使用的仓库存储 |

容器更新/重建不删除命名卷。备份直接备份这些卷即可；不要用 `docker compose down -v`
执行常规升级。

---

## 五、鉴权说明

- **API Key 模式（推荐，最稳）**：`.env` 里设 `ANTHROPIC_API_KEY` 或第三方
  `ANTHROPIC_BASE_URL`+`ANTHROPIC_AUTH_TOKEN`。容器把这些环境变量传给 spawn 的 claude。
- **供应商坞切换**：进入 App 内「供应商」面板切换/新增，会写入 `/home/polaris/.claude/settings.json`（持久化）。
- **OAuth 订阅（Claude Pro / Codex）**：无头容器难走设备码流程。变通：把已登录的
  `~/.claude` 内容拷进 `polaris-claude` 卷复用。本期主推 API Key。
- **访问口令：默认没有**（2026-07-25 起）。容器起来谁连上谁能用，不弹口令框、不看来源。

  | 配置 | 效果 |
  |---|---|
  | **什么都不设（默认）** | **免口令**：任何能连上 8080 的来源直接进 |
  | `POLARIS_AUTH_TOKEN=<口令>` | 所有来源一律校验这把口令 |
  | `POLARIS_REQUIRE_LOGIN=1` | 不用口令，改为一律要团队账号登录 |
  | `POLARIS_LAN_ONLY=1` | 免口令，但只放行内网来源（私网/回环/Tailscale `100.64/10`），公网来源拒 |

  设了口令时 `/api/*` 需 `Authorization: Bearer <口令>`、WS 需 `?token=<口令>`；
  前端用 `http://host:8080/?token=<口令>` 打开会自动记住。

  为什么默认不要口令：家用 NAS 挂在路由 NAT 后面，地址由运营商动态分配、还常带 CGNAT，
  没做端口映射时公网压根连不上（真要远程用走的是 P2P/Tailscale，那条自带身份）；而口令
  最容易忘、忘了又没有找回入口——历史上两次把用户锁在自己软件外面，一次是启动时随机
  生成只打在日志里，一次是网页向导让人随手设的。**网页里那个「设置访问口令」向导已经
  撤掉，历史落盘的口令也不再生效**（升级上来即自动解锁）。上锁只剩上表那几个环境变量
  一条明路。`POLARIS_ALLOW_OPEN` 现在就是默认行为，留着不报错、也不必再设。

  `POLARIS_LAN_ONLY=1` 那档的判定按 **TCP 对端地址**，保守优先：拿不到对端地址、或带了
  `X-Forwarded-For`/`X-Real-IP` 但没设 `POLARIS_TRUST_PROXY=1`，一律当公网处理（反代后面
  对端恒是反代自己，按它判会把全世界当内网）。

  **`/api/exec`（远程 shell）不吃免口令这条豁免**：没设 `POLARIS_AUTH_TOKEN`／没开
  `POLARIS_REQUIRE_LOGIN` 时它直接 403，要用必须先给出真凭据。

---

## 六、特性存活矩阵（容器版）

| 板块 | 状态 | 说明 |
|---|---|---|
| 对话 / 流式 / 工具调用 | ✅ 保留 | WS 推流，体验等价 |
| 知识库 KB（扫描/图谱/检索/编译/上传） | ✅ 保留 | 纯逻辑，卷持久化 |
| 技能 / 人格 / CLAUDE.md / 供应商 / 用量 / Codex 代理 | ✅ 保留 | 文件落盘到卷 |
| 文件上传 | ✅ 保留 | 拖拽 → `/api/upload` multipart |
| 产物预览 / 成品编辑器 | ✅ 保留 | `artifact_read` 返回正文/dataUrl，iframe 预览 |
| 飞书 / 企微网关 | ⚠ 可用 | 长连接服务端更合适；OAuth 回调 URL 需公网可达 |
| PPT / 网页 / 视频工坊 | ⚠ 多数保留 | 视频需镜像加 ffmpeg/playwright（按需扩镜像） |
| 可运行项目（一键起前后端） | ⚠ 受限 | 容器内嵌套起服务受限，list/status 可用 |
| Docker 沙箱板块 | ⛔ 降级 | Docker-in-Docker 风险高，返回 stub |
| 环境医生（安装 claude/node） | ⛔ 简化 | 镜像已预装，安装类命令返回提示 |
| 自动更新 | ✅ 可选 | 默认只检查版本；显式叠加 `docker-compose.update.yml` 后 owner 可一键替换容器 |

---

## 七、常用运维

```bash
docker compose logs -f polaris      # 看日志
docker compose restart polaris      # 重启
docker compose down                 # 停（保留卷）
docker compose down -v              # 停并删数据卷（慎用）
docker exec -it polaris-web bash    # 进容器排查（claude --version 等）
```

## 八、稳健性：单轮对话看门狗

容器内偶发：个别极简 prompt 会让 claude 触发子代理（`claude --print`，其 cwd 落在 `/`）
对文件系统做无界扫描而长时间不返回，既拖死本轮、又占住 OAuth 订阅的并发槽拖垮后续消息。

对策：`POLARIS_CHAT_TIMEOUT_SECS`（容器默认 900s，桌面 1800s）。连续空闲超阈且进程树
深检确认静止（0 子孙 + CPU 无推进）才杀整个 claude 进程组，stdout 关闭 → 正常 emit
error+done，系统自愈、释放并发槽。设 0 关闭。静默但在干活（构建/ffmpeg/长脚本）不会被杀。
另有总时长硬顶 `POLARIS_CHAT_HARD_CAP_SECS`（默认 86400s=24h），只兜真失控任务。

> 实测：实质性问题（联网检索、生成 PPT/网页、写文件、KB 取证）均正常；
> 仅「只回复两个字」这类极简多轮 prompt 偶发触发上述扫描，看门狗保证不会无限挂死。

## 九、扩展为「全功能镜像」（媒体/视频）

在 `Dockerfile` 阶段3 的 apt 安装里加 `ffmpeg`，并按需装 Playwright/Chromium
（`npx playwright install --with-deps chromium`），compose 里加 `shm_size: 1gb`。
镜像会增大约 400MB+，故默认做「轻量镜像」，按需开启。
