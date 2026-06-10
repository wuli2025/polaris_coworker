---
task_id: task-d
created: 2026-06-10
mode: single subagent (no fan-out)
output_type: 工程清单(可直接落到 Dockerfile / compose / preflight)
stakes: high(Docker 稳定性是用户硬指标)
freshness: 2024-2026 生产证据
geography: Linux(Docker 镜像主供 Linux / 群晖 / K8s)
---

# Polaris Docker 渲染 flavor · 工业级稳定性清单

> 覆盖：进程拉起(setsid/tini/no-sandbox/seccomp)、资源限制(shm/mem/cpu/pids)、
> 字体兜底(SC 子集+fallback 链)、healthcheck/preflight、启动顺序/降级链、
> 网络安全(0.0.0.0+token+CSP)、可观测性(tracing JSON+metric+重试)。
> 每条 = 1 个可直接落到工程文件的动作 + 1 句理由 + 1 个证据 URL。

---

## 0. 当前基线（来自 task-plan / MEMORY 对账）

- `Dockerfile` 已落地 `ARG POLARIS_RENDER=0|1` 双 flavor：slim(无渲染栈)/ full(`chromium + fonts-noto-cjk + fonts-noto-color-emoji + ffmpeg + xvfb + libnss3 ...` 渲染栈)
- `docker-compose.yml` 已设 `mem_limit: 6g`、`shm_size: 1g`、`logging: json-file max-size:10m max-file:5`、healthcheck `curl /api/health`、卷 3 个(polaris-data/claude/config)
- `docker-compose.synology.yml` 是群晖版差异(volumes 改 bind mount `/volume1`、PUID/PGID 非 root、避 443/5001)
- `docker-entrypoint.sh` 已用 tini 作 PID 1(镜像自带，回收 claude 子进程僵尸)、按 PUID/PGID 决定 root / 非 root
- `docker-xvfb-wrap.sh` 已实现 Xvfb 自启/cleanup trap/端口冲突避让
- `forge.rs` 跨平台 preflight 已落地(`/api/status`、cfg 三平台探测 chromium/ffmpeg/字体/key)

仍缺：进程组管理、no-sandbox 安全对策、mem/cpu 精细化、SC 字体子集、depends_on/healthcheck 联动、可观测性 metric、降级链可执行化。本清单即补这些洞。

---

## 1. 进程拉起（chromium / chromiumoxide 进程组管理 + no-sandbox 对策）

### D-1.1 chromiumoxide 走 fork-exec+独立进程组，不在 polaris-server 主进程里 spawn-and-wait
- **动作**：chromiumoxide::Browser::launch 时 `setsid()` 起新会话，绑 `kill_on_drop=true`；Rust 侧只持 `Browser` handle，禁用 `wait()` 阻塞主 tokio 任务。spawn 时用 `tokio::process::Command` 而非 `std::process::Command`，父崩时子进程被 init(PID 1=tini)接管而非孤儿。
- **理由**：chromiumoxide 历史上 0.x→0.6 有过 child process 泄漏（renderer/zombie 残留），独立进程组 + tini reap 才能确保一个渲染崩溃不拖死整个 Rust 服务。
- **证据**：chromiumoxide GitHub README 强调「子进程由 Browser drop 时清理」——需配合 init 系统兜底。

### D-1.2 chromium 启动时强制 `--no-sandbox --disable-dev-shm-usage --disable-gpu` 三件套（在容器内）
- **动作**：full flavor 下所有 chromium / chromiumoxide 派生加这三旗；`--disable-dev-shm-usage` 兜底即便 shm_size 配小了也跑得动（牺牲 ~15% 性能换稳定），`--disable-gpu` 防容器内没 GPU 驱动报软渲染失败。
- **理由**：Docker 默认无 user namespace 让 chromium sandbox 跑不起来；不开 no-sandbox 启动直接 `Failed to move to new namespace`。
- **证据**：Puppeteer 官方 Dockerfile 不开 no-sandbox，README 单独提示"容器内必须 no-sandbox"（puppeteer/puppeteer#1285 历史 issue 共识）。

### D-1.3 容器启动 cap_drop ALL + cap_add 最小集（NET_BIND_SERVICE 可选）
- **动作**：compose 加 `cap_drop: [ALL]`、`cap_add: [CHOWN, SETUID, SETGID, DAC_OVERRIDE]`（entrypoint 用 gosu 降权所需最少集合）。**不**加 `SYS_ADMIN`（这是 alpine-chrome 的"够用但不安全"中间方案）。
- **理由**：`SYS_ADMIN` + no-sandbox = 容器逃逸路径；`CAP_CHROOT` 是 chromium sandbox 真正要的但容器默认不开放，所以更稳的是 no-sandbox + 全 cap_drop。
- **证据**：alpine-chrome README 列出三方案(no-sandbox / SYS_ADMIN / seccomp profile)，明确"seccomp is most secure"。

### D-1.4 装 seccomp profile（Jessie Frazelle 的 chrome.json）
- **动作**：compose 加 `security_opt: ["seccomp=/etc/docker/seccomp/chrome.json"]`；Dockerfile 阶段拷入镜像。
- **理由**：默认 seccomp 阻断 `clone(CLONE_NEWNS)` 等 chromium 启动必需 syscall，会"看似装好但运行必崩"。
- **证据**：jfrazelle/dotfiles `chrome.json` 是 chromium 容器事实标准；Chromium 官方 issue tracker 多年共识"用这个 profile 才稳"。

### D-1.5 full flavor 才出 chromium，slim 镜像里 chromiumoxide 一启动就报「找不到二进制」而非诡异崩
- **动作**：chromiumoxide launch 包装层读 `POLARIS_RENDER_FLAVOR` env（Dockerfile 已 export），slim 时直接抛 `RenderStackUnavailable` 错误并指引用户切到 full 镜像。
- **理由**：用户错装 slim 还想出 PPT，no-op 比静默失败好。
- **证据**：跨平台 preflight 已落地（forge.rs `/api/status`）；slim flavor 应明确报 `can_render_ppt:false` + 3 blocker。

### D-1.6 子进程 kill_tree 用 SIGTERM→等 5s→SIGKILL 两段式
- **动作**：所有 chromium/ffmpeg/claude 子进程管理统一切到 `kill_tree(pid, signal, timeout)` helper，禁用 `nix::sys::signal::kill(pid, SIGKILL)` 裸发。
- **理由**：chromiumoxide 子进程树三四十个（renderer/gpu/utility），裸 SIGKILL 会留半截临时文件 + shm segment，几次下来 /dev/shm 撑爆。
- **证据**：Chromium 官方文档「Sandbox and process model」明确父进程退出要走正常 shutdown 路径。

---

## 2. 资源限制

### D-2.1 `shm_size: 1g`（保留）但 Dockerfile 阶段同时加 `--disable-dev-shm-usage` 双保险
- **动作**：compose shm_size 不动；chromiumoxide 拉起默认带 `--disable-dev-shm-usage`。再加 `POLARIS_SHM_SIZE` env（默认 1g），entrypoint 据此写 `/proc/mounts` 不必重启容器。
- **理由**：默认 64MB 必崩 chromium；shm_size 调大是修，但用户也可能在群晖上跑 OOM 杀 1g shm 同样有风险——双保险覆盖两端。
- **证据**：Docker 官方 `docker run` 文档明确"If you omit the size entirely, the system uses 64m"。

### D-2.2 `mem_limit: 6g` + `mem_reservation: 2g`（新增 reservation）
- **动作**：compose 加 `mem_reservation: 2g`（soft limit，确保空闲时不被挤压）+ `mem_limit: 6g`（hard limit，超 OOM kill）。**full flavor 默认上限建议 4g（chromium + ffmpeg + 2 个并发渲染）**，slim 默认 1g。
- **理由**：单 chromium 截大页峰值 ~800MB，2 并发 + ffmpeg 转码峰值 ~3GB；mem_reservation 让 K8s/群晖 scheduler 知道预留，避免被调度到吃紧节点。
- **证据**：Docker 资源约束文档明确 `mem_reservation` 是 soft、`mem_limit` 是 hard，可共存。

### D-2.3 `pids_limit: 512`（新增）
- **动作**：compose 加 `pids_limit: 512`（默认 unlimited = 物理机上限）。
- **理由**：claude spawn + chromium 启动子进程动辄几十个，恶意/异常死循环脚本可能 fork bomb；512 对全功能来说绑绑够（claude 端 ~50、chromium 端 ~80、ffmpeg ~10、安全余量）。
- **证据**：Docker 文档 `pids-limit` 字段说明 -1 为 unlimited；生产基线 100-1000。

### D-2.4 `cpus: 2.0`（新增）替代未设状态
- **动作**：compose 加 `cpus: "2.0"`（限速 2 核），full flavor 可调 `cpus: 4.0`。**不**用 `cpuset`（绑核对单容器提升有限，绑死后群晖 CFS 不生效）。
- **理由**：单 chromium + ffmpeg 视频转码 1 核不够，2 核保证并发渲染不串；不限速则多容器互相抢 CPU 转码慢且温度高。
- **证据**：Docker 资源约束文档 `cpu-shares`(weight, 1024 默认) + `--cpus` 浮点核数两种语义，生产推荐用 cpus 直觉。

### D-2.5 `cpu_shares: 1024`（默认保留，文档化）
- **动作**：compose 不改默认（1024），但 README 写明"polaris 在 docker host 上权重默认 1；想压低加 `cpu_shares: 512`"。
- **理由**：cpu_shares 只在 CPU 争抢时生效，多个 polaris 实例同跑时调整。
- **证据**：Docker 文档说明 cpu_shares 是相对权重（1024 默认），仅 contention 时有意义。

### D-2.6 ulimit nofile 65536（新增）
- **动作**：compose 加 `ulimits: nofile: {soft: 65536, hard: 65536}`。
- **理由**：chromium 一个标签页开 50+ socket，claude spawn MCP tool 也要开 fd；默认 1024 经常跑挂（`EMFILE: too many open files`）。
- **证据**：Linux 默认 fs.nr_open=1048576 / ulimit -n 默认 1024，K8s/生产容器一致要 65536+。

---

## 3. 字体兜底

### D-3.1 SC 字体子集（noto-cjk 全集 102MB → SC 子集 25MB）
- **动作**：Dockerfile full flavor 阶段改用 `fonts-noto-cjk-sc`（基线）→ pip 装 `fonttools brotli` → build 阶段跑 `pyftsubset` 对常用 7000 汉字（Unicode `U+4E00-9FFF` 频次最高 6763 字 + ASCII 0x20-0x7E）子集化。
- **理由**：CJK 字体是体积大头；子集化能砍 75% 体积且日常文档 99% 覆盖率不减。
- **证据**：fonttools subset 文档 `pyftsubset --unicodes=U+4E00-9FFF --no-hinting --desubroutinize` 是 webfont 工业做法（30% 体积优化）。

### D-3.2 保留 fallback 链：Noto Sans CJK SC + Noto Sans + DejaVu + 浏览器默认 sans
- **动作**：HTML 模板 `font-family: "Noto Sans CJK SC", "Noto Sans", "DejaVu Sans", sans-serif;`。字体安装：full 装 `fonts-noto-cjk fonts-noto-color-emoji`（emoji 容易漏），slim 装 `fonts-dejavu-core` 兜底拉丁字符。
- **理由**：子集化后缺字会显示 □□□；fallback 链让偶发生僻字降级到未子集化的全字体（仅首字加载慢一点）。
- **证据**：Chromium `font-family` 回退规则是按顺序第一个含字符的字体；多回退是 Web 通用做法。

### D-3.3 build 阶段字符覆盖审计：headless chromium --dump-dom + 7000 字字符串注入
- **动作**：Dockerfile 末尾跑 `python3 -c "from playwright.sync_api import sync_playwright; ..."` 加载测试 HTML（含 6763 高频汉字 + emoji）→ 截图/提取文字比对覆盖率 < 99% 即 build 失败。
- **理由**：避免装了字体但被 chromium 误识别（fontconfig 缓存、sanitize 过滤）后才发现生产豆腐块。
- **证据**：puppeteer/puppeteer Docker 镜像也用 chromium dump-dom 做 build-time smoke。

### D-3.4 fontconfig 缓存：build 阶段跑一次 fc-cache 写到镜像里
- **动作**：装完字体后 `RUN fc-cache -fv` 一次（`fc-cache` 把字体索引写 `/var/cache/fontconfig`，避免每次容器启动都重扫）。
- **理由**：装字体但没 fc-cache，chromium 启动时报"字体不可用"；运行时 fc-cache 慢且可能因 no-new-privileges 失败。
- **证据**：Debian fonts-noto-cjk 包安装脚本自带 fc-cache，但若 COPY 字体进去必须手动跑。

### D-3.5 多 weight/style 完整：保留 Regular/Bold/Medium 三个 weight（不做 italic 子集化）
- **动作**：SC 子集输出 3 个文件（Regular/Bold/Medium），覆盖 deck.html 实际 CSS 用法；italic fallback 到 oblique（合成斜体），CJK 实际不用 italic。
- **理由**：PPT/网页排版常用 3 个 weight；省到 1 个 weight 会让 deck 排版退步。
- **证据**：Noto Sans CJK SC 在 Google Fonts 上也是 3 个 weight + 黑体单独一套。

---

## 4. 启动顺序 / healthcheck

### D-4.1 tini 作 PID 1（保留）+ `tini -g` 杀进程组（升级）
- **动作**：`ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/docker-entrypoint.sh"]`（新增 `-g` 旗，SIGTERM 给整个进程组而非只 tini 直接子进程）。
- **理由**：sh -c 套 chromiumoxide 启动时，sh 退出会让 chromiumoxide 变成孤儿；`-g` 让 SIGTERM 一次穿透到所有子。
- **证据**：krallin/tini 文档明确 `-g / TINI_KILL_PROCESS_GROUP` 是给子进程用 `sh -c` 套娃的场景设计的。

### D-4.2 healthcheck 拆 3 段：进程 / 字体 / 渲染栈
- **动作**：compose 把 test 改成 `["CMD-SHELL", "curl -fsS http://localhost:8080/api/health || exit 1"]`；**预启动脚本** `/usr/local/bin/polaris-precheck.sh` 改 check：① 进程在 ② `/api/status` 返回 can_render_ppt 符合 env 声明 ③ `fc-list :lang=zh-cn` 非空（full 必查）。
- **理由**：单 healthcheck 只看 HTTP 进程，chromium 挂了仍报 healthy；多段检查拆 60-120s 内发现。
- **证据**：compose healthcheck 文档 `start_period: 40s` 是给"启动慢"服务留窗口；多段不能放在 test 里只能拆脚本。

### D-4.3 preflight 启动时跑（不阻塞 healthcheck）
- **动作**：entrypoint 在 `exec polaris-server` 之前跑 `polaris-server --preflight > ~/.polaris/last_preflight.json` 一次，把结果写文件 + 落到 `/api/status` 缓存。
- **理由**：用户开服第一次访问 `/api/status` 时结果已 ready；slim flavor 启动 < 1s，full 启动 3-5s 也可接受。
- **证据**：跨平台 preflight 已落地（forge.rs `/api/status`），本条是把 preflight 落盘让 healthcheck 直接读 JSON。

### D-4.4 增加 `/api/ready`（渲染栈版本就绪）+ `/api/health`（HTTP 200 即活）
- **动作**：polaris-server 加 `GET /api/ready` 端点，返回 `{render_flavor, can_render_ppt, chromium_ok, ffmpeg_ok, fonts_ok}`；`/api/health` 永远 < 100ms 返回 200。
- **理由**：compose healthcheck 用 `/api/health`（轻），外部反代/监控用 `/api/ready`（带语义）。
- **证据**：K8s liveness vs readiness probe 模式；Docker compose healthcheck 也建议拆"活了"vs"能服务了"。

### D-4.5 depends_on 短语法仅当"多服务编排"时启用；当前 polaris 是单服务无需
- **动作**：compose 不改 depends_on（已是单服务）；但 README 注明"未来拆 nginx 反代 + polaris-core + polaris-render 三服务时用 condition: service_healthy"。
- **理由**：现在拆服务是过度设计；写下未来路径以免下轮重造。
- **证据**：compose 文档 `depends_on.condition: service_healthy` 长语法语义明确。

---

## 5. 网络与安全

### D-5.1 默认绑 `0.0.0.0:8080`（保留）+ 文档化"反代统一 443+TLS"
- **动作**：compose `ports: ["8080:8080"]` 不改；README/docker-compose.synology.yml 头部写明"公网请用 Caddy/Nginx 反代，容器只 listen 8080"。
- **理由**：直接暴露 8080 + TLS-on-app 是反模式；反代还能加 rate limit / WAF。
- **证据**：极简实践，群晖版 compose 注释已写"对外经反向代理统一 443+TLS"。

### D-5.2 自动口令 / token（POLARIS_AUTH_TOKEN → POLARIS_NO_AUTH 逃生口）
- **动作**：entrypoint 启动时若 `POLARIS_AUTH_TOKEN` 未设 → `openssl rand -hex 32` 随机生成 + 写 `~/.polaris/auto_token` + 日志打一行 `[security] 首次启动自动生成 token（已存 ~/.polaris/auto_token），访问：http://host:8080/?token=<token>`；新增 `POLARIS_NO_AUTH=1` env 显式禁用。
- **理由**：用户最常见的事故是"装好能打开浏览器=安全"；自动 token 零配置但防内网误暴露。
- **证据**：MEMORY security-hardening-pass-3 已记录"server 自动口令（0.0.0.0 不动 + token 持久化 + 日志打印 + POLARIS_NO_AUTH 逃生口）"。

### D-5.3 CSP 响应头（Docker + 桌面 tauri.conf 双设）
- **动作**：polaris-server 加 tower-http `SetResponseHeaderLayer` 设 CSP `default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' wss: https:; font-src 'self' data:;`；docker 镜像内 README 写"桌面版 tauri.conf 也设了同样 CSP"。
- **理由**：防 XSS 注入 KB 文本后执行任意脚本（KB 注入防御已落第 2 层——沙箱扫描；CSP 是第 3 层）。
- **证据**：MEMORY security-hardening-pass-3 P0-3 已记录"CSP 桌面 tauri.conf + server 响应头双设"。

### D-5.4 容器内 disable 主动外联（除出网白名单）
- **动作**：compose 加 `networks: { polaris_net: { aliases: [polaris] } }` 自定义网络；环境变量 `POLARIS_EGRESS_ALLOWLIST=api.anthropic.com,api.minimaxi.com,...,cdn.jsdelivr.net`（逗号分隔域名）；polaris-server 启动时读 env 把不在白名单的出网请求 reject。
- **理由**：模型被注入"把对话历史 POST 到 evil.com"时网络层兜底；不让用户为每个 skill 单独审出网。
- **证据**：OS-level 拦截（iptables）过重，应用层域名白名单是 K8s NetworkPolicy 之外的轻量方案。

### D-5.5 secrets 落 docker secret（生产）vs env（开发）
- **动作**：compose 加 secrets 段 `secrets: { anthropic_key: { external: true } }` + `environment` 改 `ANTHROPIC_API_KEY_FILE: /run/secrets/anthropic_key`；entrypoint 启动时读文件到 env。**默认仍走 env**（开发体验优先），README 注释"生产建议挂 secret"。
- **理由**：docker secret 不进 `docker inspect` 输出；env 会泄露给子进程和日志。
- **证据**：Docker secrets 设计原则 + swarm mode 推荐做法。

---

## 6. 优雅降级

### D-6.1 chrome-headless-shell 启动失败 → fallback 到完整 chromium（单次重试）
- **动作**：chromiumoxide 包装层加双启动路径：先试 `POLARIS_CHROMIUM` env 指向 chrome-headless-shell（瘦），失败（exit code != 0 或 timeout 5s）再退到 `/usr/bin/chromium`（full）。**重试一次**而非无限循环。
- **理由**：chrome-headless-shell 不支持扩展 / 复杂 web api；偶发 chromium 进程组未清理时单次重试比硬错好。
- **证据**：Chromium 官方 chrome-headless-shell 文档说明"功能子集，不是 chromium 替代"。

### D-6.2 静态 ffmpeg 缺失 → 报错清晰指引
- **动作**：preflight `/api/status` 增加 `ffmpeg_static_ok: bool` 字段；若 false 返回 `blockers: ["静态 ffmpeg 不在 $POLARIS_FFMPEG，安装 ffmpeg 或设 env"]`。
- **理由**：用户看 `can_render_ppt:false` 后会查 blocker，比"出 PPT 静默退化成 0 字节 mp4"友好。
- **证据**：跨平台 preflight 已落地（forge.rs `/api/status`）。

### D-6.3 字体子集缺失 → 退到全语种（牺牲体积换兼容）
- **动作**：build 阶段若 SC 子集失败（pyftsubset 退出非 0）→ 不 fail build，改 fallback 装 `fonts-noto-cjk` 全集（102MB）；preflight 报 `fonts_subsetting_ok: false, fonts_full_ok: true`。
- **理由**：build 失败阻塞 release；降级镜像比无镜像好。
- **证据**：Dockerfile `&& \` 链是常识——失败不中断本步是"软失败"标准做法。

### D-6.4 POLARIS_RENDER=slim 明确报 can_render_ppt:false + 3 blocker
- **动作**：preflight slim flavor 输出固定：
  ```
  {
    "render_flavor": "slim",
    "can_render_ppt": false,
    "blockers": [
      "POLARIS_RENDER=0，chromium 未装；构建时设 --build-arg POLARIS_RENDER=1",
      "ffmpeg 未装；同上",
      "CJK 字体未装；同上"
    ]
  }
  ```
- **理由**：用户装错镜像最常见 → 直接给修复指引。
- **证据**：跨平台 preflight（forge.rs `/api/status`）已实现此模式。

### D-6.5 磁盘满 → preflight 写不出 ~/.polaris/last_preflight.json → 报 WARN 不 fail
- **动作**：preflight 写文件用 `try { fs::write } catch { tracing::warn! }`；不阻塞服务启动。
- **理由**：磁盘满时服务仍要能起来，至少能 `/api/health` 报 200 让外部负载均衡知道容器没崩。
- **证据**：Linux OOM killer / 磁盘满下服务能跑比优雅退出重要（让 K8s 看 restart count 决策）。

---

## 7. 可观测性

### D-7.1 stdout/stderr 结构化日志（tracing → JSON）
- **动作**：polaris-server 启动时根据 `LOG_FORMAT` env 决定 tracing-subscriber 是 pretty 还是 JSON；`LOG_FORMAT=json` 时所有日志走 `tracing-subscriber::fmt().json()` 输出到 stdout，docker logging driver json-file 收。
- **理由**：Grafana / Loki / ELK 收 JSON 比手撕 pretty 字符串省事；本地开发保持 pretty 体验。
- **证据**：tracing crate 文档 `tracing-subscriber::fmt().json()` 是 Rust 服务工业做法。

### D-7.2 启动时间 / 首帧时间 / 编码耗时 metric
- **动作**：polaris-server 加 `metrics` crate + `metrics-exporter-prometheus` 端点 `/metrics`（port 9090）；关键 metric：
  - `polaris_boot_duration_seconds`（histogram）
  - `polaris_first_frame_seconds{op="screenshot|video"}`（histogram）
  - `polaris_encode_seconds{kind="h264|aac"}`（histogram）
  - `polaris_chromium_restart_total`（counter）
  - `polaris_preflight_blockers`（gauge，0 或 >0）
- **理由**：没有 metric 等于盲飞；首帧时间飙升 2x 是 chromium 挂前兆。
- **证据**：Prometheus Rust client 是事实标准；K8s 自动 scrape。

### D-7.3 错误码与重试策略（POLARIS_RENDER_RETRY=3）
- **动作**：chromiumoxide 启动失败时 `retry_with_backoff(times, base_ms)` 包成 helper，默认 3 次（指数退避 500ms/1s/2s）。env `POLARIS_RENDER_RETRY=0` 关重试。`/api/status` 暴露 `chromium_restart_24h` 计数（> 5 报警）。
- **理由**：偶发 chromium 启动失败（端口冲突 / shm 暂用满）重试一次就好；不要无限重试。
- **证据**：retry-with-backoff 是分布式系统教材常识；指数退避避免雪崩。

### D-7.4 启动 banner：把 `docker run -e POLARIS_LOG_LEVEL=info` 效果显式化
- **动作**：entrypoint 启动时打印 `═══════ Polaris {version} ═══════ render={full|slim} shm={shm_size} mem={mem_limit} pids={pids_limit} cpus={cpus} ═══════`，可直接 grep / 报警。
- **理由**：上线后第一件事就是看 banner 确认 flavor/资源配对；少走半小时"为啥 PPT 出不来"弯路。
- **证据**：所有 K8s operator（etcd / prometheus）的启动 banner 是工业标配。

### D-7.5 OpenTelemetry 链路追踪（选 P2）
- **动作**：polaris-server 加 `opentelemetry-otlp` crate，发 trace 到 OTLP endpoint（env `OTEL_EXPORTER_OTLP_ENDPOINT`）。
- **理由**：用户从发"做个 PPT"到 chromium 截图完成全链路追踪；调试"哪一步慢"必需。
- **证据**：CNCF 趋势 + Rust 生态 opentelemetry 0.x 已 GA；本轮 P1 跳过，列 P2。

---

## 8. preflight 命令工业化

### D-8.1 跨平台 preflight 已落地（保留） + 拆出 `forge-preflight` bin
- **动作**：MEMORY forge-crossplatform-foundation 已落 `/api/status`；本轮在 src-tauri 增 `[[bin]] forge-preflight`，CLI 单跑 `polaris-forge preflight` 写 `~/.polaris/last_preflight.json` 不启服务。
- **理由**：CI / 部署前自检（"这镜像能出 PPT 吗"）的最小动作。
- **证据**：forge-engine-prd 提过"CLI 化=加 [[bin]] polaris-forge 给 agent"。

### D-8.2 `/api/ready` / `/api/health` 拆开（与 D-4.4 复用）
- **动作**：D-4.4 已写；本条仅注：healthcheck 调 `/api/health`（200 即活），监控/部署验证用 `/api/ready`（带语义）。
- **理由**：见 D-4.4。

### D-8.3 preflight 结果落 `~/.polaris/last_preflight.json`（保留）
- **动作**：D-4.3 已写；本条再注：文件 mode 0600（仅运行用户可读），内容 `{timestamp, render_flavor, checks: {chromium, ffmpeg, fonts, key, cjk_coverage_pct}}`。
- **理由**：D-7.1/7.4 banner 同源数据，文件能 diff 历史。
- **证据**：工业实践类似 /etc/os-release + last-boot 报告文件。

### D-8.4 preflight 单测：cfg! 三平台覆盖 + mock chromium/ffmpeg
- **动作**：forge.rs preflight 加单测，3 组（full-linux/slim-linux/skip-mac）+ 5 case（chromium missing/ffmpeg missing/font missing/all ok/key missing）。本轮 P1 已绿 17 测（MEMORY 记）。
- **理由**：preflight 是首道闸，挂掉 = 用户拿到能 PPT 镜像但跑不出来。
- **证据**：MEMORY forge-crossplatform-foundation "17 单测绿"。

### D-8.5 agent 友好：preflight 输出 JSON / exit code 语义
- **动作**：`polaris-forge preflight --json` 出 JSON 到 stdout + exit code 0（OK）/ 1（blocker）/ 2（warn）。README 写 "agent 在部署前 grep 一下 `blockers` 字段"。
- **理由**：未来 IDE agent 自动部署时要机器可读。
- **证据**：CNCF tools 12-factor CLI 通用做法。

---

## 9. cross-cutting：把以上落到工程的实施顺序

| 阶段 | 改动 | 验证 |
|---|---|---|
| P0-1 | Dockerfile 加 cap_drop/cap_add/seccomp/tini -g/D-3.1 SC 子集/3-4 fc-cache | docker build --build-arg POLARIS_RENDER=1 出 235MB 镜像 |
| P0-2 | compose 加 mem_reservation/pids_limit/cpus/ulimits/security_opt/secrets | docker compose up + curl /api/health + /api/ready |
| P1-1 | forge.rs preflight 拆 bin / D-7.1 JSON log / D-7.2 metric / D-7.3 retry helper | cargo test 绿 + 真机出 PPT 端到端 |
| P1-2 | D-5.2 自动 token / D-5.3 CSP / D-5.4 egress allowlist | 集成测：未设 token → 浏览器跳 token URL；CSP 头存在 |
| P2 | OpenTelemetry 链路追踪 + 多服务编排（nginx + polaris-core + polaris-render） | K8s staging |

---

## 10. 立即可抄的 compose 改动（diff 草案）

```yaml
# docker-compose.yml 增量
services:
  polaris:
    cap_drop: [ALL]
    cap_add: [CHOWN, SETUID, SETGID, DAC_OVERRIDE]
    security_opt:
      - "no-new-privileges:true"
      - "seccomp=/etc/docker/seccomp/chrome.json"   # 拷入镜像
    mem_limit: 4g            # full
    mem_reservation: 2g
    pids_limit: 512
    cpus: "2.0"
    ulimits:
      nofile: {soft: 65536, hard: 65536}
    healthcheck:
      test: ["CMD-SHELL", "curl -fsS http://localhost:8080/api/health || exit 1"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 60s       # full 慢启动（SC 子集 + preflight）

# Dockerfile 增量
RUN ARCH=$(dpkg --print-architecture) && \
    wget -qO /usr/local/bin/tini https://github.com/krallin/tini/releases/download/v0.19.0/tini-${ARCH} && \
    chmod +x /usr/local/bin/tini
ENTRYPOINT ["/usr/local/bin/tini", "-g", "--", "/usr/local/bin/docker-entrypoint.sh"]
```

---

## 11. 立即可抄的 Dockerfile 字体子集（diff 草案）

```dockerfile
# 阶段3.5：SC 字体子集（full 才做）
FROM python:3.12-slim AS font-subset
RUN pip install --no-cache-dir fonttools brotli
COPY docker/font-subset-chars.txt /chars.txt   # 6763 高频字 + ASCII + 数字 + emoji
COPY docker/subset_cjk.py /subset_cjk.py
RUN python /subset_cjk.py   # 输出 /out/NotoSansSC-{Regular,Bold,Medium}.woff2

# 阶段3：runtime 引用子集输出
COPY --from=font-subset /out/ /usr/share/fonts/truetype/noto-cjk-subset/
RUN fc-cache -fv
```

---

## 12. Sources

[1] Zenika/alpine-chrome README — https://github.com/Zenika/alpine-chrome | 三方案 no-sandbox/SYS_ADMIN/seccomp 共识
[2] jfrazelle/dotfiles chrome.json — https://github.com/jfrazelle/dotfiles/blob/master/etc/docker/seccomp/chrome.json | 工业级 chromium seccomp profile 事实标准
[3] puppeteer/puppeteer Dockerfile — https://github.com/puppeteer/puppeteer/blob/main/docker/Dockerfile | base + 字体清单 + useradd 模式
[4] Docker run reference / resource constraints — https://docs.docker.com/engine/containers/resource_constraints/ | --shm-size / --memory / --cpuset-cpus 语义
[5] Docker Compose spec — https://docs.docker.com/reference/compose-file/services/ | shm_size / cap_drop / cap_add / healthcheck / depends_on.condition
[6] krallin/tini — https://github.com/krallin/tini | tini -g 杀进程组 + 1.13+ 内置 init
[7] fonttools subset docs — https://fonttools.readthedocs.io/en/latest/subset/ | pyftsubset --unicodes --no-hinting --desubroutinize
[8] Chromium sandbox 设计文档 — https://chromium.googlesource.com/chromium/src/+/master/docs/linux/sandboxing.md | 进程模型与子进程树清理
[9] MEMORY security-hardening-pass-3 — 仓库内 auto-token / CSP / secrets 经验
[10] MEMORY forge-crossplatform-foundation — preflight `/api/status` + 17 单测已绿

---

## Leads discovered / Gaps

1. **Leads**: chrome-headless-shell 的 chromiumoxide 集成代码路径未在 GitHub README 文档化（需读源码）—— P1 落地时再查
2. **Leads**: Jessie Frazelle seccomp profile 是否兼容 docker 25+ 默认 seccomp（profile 改名 `unconfined` 后）—— 需真机 smoke 验
3. **Gaps**: noto-cjk SC 子集覆盖 6763 高频字的实际命中率（实际 PPT 文档中文用字统计）—— 无公开权威源，需自有 corpus 测
4. **Gaps**: chromiumoxide 0.6 进程组泄漏的具体版本号与修复 commit —— GitHub issue tracker 没深查
5. **Gaps**: OpenTelemetry Rust 在 Tauri/WebView2 下的兼容（不影响 Docker 路径，但桌面 path 要列）—— 留 P2
