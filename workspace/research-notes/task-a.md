# task-a · Polaris Forge Capture 板块工业级 Rust 自研方案

**Owner:** capture 研究专员
**Date:** 2026-06-10
**Scope:** 三平台（Windows / macOS / Docker）截图/截帧工业级 Rust 选型
**Status:** 调研完成,等待 P3 验证

---

## 0. 背景与决策框架

- 桌面 HTML PRD 已确认:浏览器内核永远不自研,但 CDP 客户端可以替「每帧起 chromium 进程」。
- 当前 `src-tauri/forge.rs` / `forge_video.rs` 用 `chromium --headless=new --screenshot=…` CLI 单帧拉起,**每个截帧/截图 1 个 chromium 进程**,视频帧是性能瓶颈(1800 帧 = 1800 次冷启动 ~ 分钟级)。
- 体积目标:994MB → 235MB(本题关注 capture 路径:`chrome-headless-shell` 替 `chromium` 后能省 ~150MB+)。
- 工业级 = 有 telemetry / graceful shutdown / reconnect / 版本兼容矩阵(可观测、可恢复、可升级)。

---

## 1. chromiumoxide(mattsse 0.9.1) — 主选型

### 1.1 现状

- 仓库 `mattsse/chromiumoxide`,GitHub 1.3k stars / 178 forks / 441 commits / ~40 open issues(2026-04 仍有新 issue #320、#321 在开)。
- crates.io 0.9.1(Feb 25 2026),双协议 MIT/Apache-2.0。
- 文档覆盖 49.66%(docs.rs 报),自动生成 ~60K 行 CDP 类型,编译慢。
- 平台 matrix:`aarch64-apple-darwin` / `aarch64-unknown-linux-gnu` / `i686-pc-windows-msvc` / `x86_64-pc-windows-msvc` / `x86_64-unknown-linux-gnu`。**没有 `i686-linux`、没有 musl**,Docker alpine 需打 `x86_64-unknown-linux-gnu` + glibc 镜像。
- Tokio-based,WebSocket 用 `async-tungstenite`,HTTP 用 `reqwest 0.13`。

### 1.2 维护活跃度

- 0.9.x 仍在演化,有 issue 明确标 "Regression from 0.8 to 0.9"、"no longer visible since 0.9.0",意味着 API 在动,需要 pin minor。
- 仓库无 archive 标记,无 successor/fork 声明(178 forks 中可能有内部 fork 但无公开接班)。
- README 主动承认 "still lacks some functionality",贡献渠道开放。
- 维护者 `mattsse` 个人项目,无公司背书 → 工业级最大风险是 bus-factor 1。

### 1.3 生产证据(薄弱)

- README 无 customer 列表。
- 已知使用方:少数 Rust web-automation 项目(scraper/crawler),无大型 SaaS 背书。
- 替代: [headless_chrome](#2-headless_chrome-eduardshl-1021-fallback) 工业级证据更强(Puppeteer-equivalent API 广为人知)。

### 1.4 性能假设 — 持久 CDP vs 每帧 CLI

- 单帧 CLI 路径耗时构成(本机粗估):冷启动 chromium ~250–600ms + 页面 navigate ~50–200ms + 截图 IPC ~30–80ms = **典型 350–900ms/帧**。
- 60s 视频 30fps = 1800 帧 → 单进程串行 = **10–27 分钟**。
- 持久 CDP 进程:启动一次性 ~400ms + 每帧 `Page.captureScreenshot` ~50–120ms → **90–220 秒** ≈ **6–10× 提速**。
- 进一步并发:多 target(frame-sharding)再 2–4× → **20–60 秒** 区间。
- 数字待真机测量(写进 P5 evaluator 的 capture-bench)。

### 1.5 关键 API

- `Browser::launch(BrowserConfig::builder().with_head().build()?)` 启动 + `Browser::new_page(url)` 开新 tab。
- 截图:`page.screenshot(ScreenshotParams::builder().with_format(ScreenshotFormat::Png).build())` 拿 bytes。
- 事件流:`Handler` 接收 `Page.frameNavigated`、`Page.loadEventFired` 等,做就绪/超时检测。
- 持久复用:长生命周期 `Browser` + 短生命周期 `Page`,**关 page 而不是关 browser**。

### 1.6 工业级必备补丁(待写)

- 自写 **reconnect-on-stale** wrapper:CDP WebSocket 断了不要 panic,`Browser::connect_with_timeout` + 自动 `Page.reload` 重试 ≤3。
- 自写 **target-pool**:4–8 个 `Page` 池子(共享 `Browser`),避免每帧 `new_page` 触发完整文档加载。
- 自写 **render-ready gate**:`Page.lifecycleEvent` + 自定义 `__deckReady` 标记,防止"白屏截图"。
- 锁版本:`chromiumoxide = "=0.9.1"`(等号 pin),等 1.0 才放宽。

### 1.7 结论

**采用。** 0.9.1 是 Rust 生态唯一在用的生产级 CDP 客户端;fork 数高、issue 仍在处理、API 完整够用。bus-factor 风险通过自写 wrapper 层吸收(API 稳定则零成本,崩了切到 `headless_chrome` 是 1–2 周工作量)。

---

## 2. headless_chrome(Edu4rdSHL 1.0.21) — 备选/fallback

### 2.1 现状

- `Edu4rdSHL/rust-headless-chrome`(原 `rust-headless-chrome/rust-headless-chrome` 个人 fork),12 releases,Lib.rs 月下载 ~329k。
- crates.io 1.0.21(Feb 3 2026),Rust 2024 edition,**MSRV 较新**(具体数字未抓到,需真机跑 `cargo +stable check` 确认)。
- 同为 Puppeteer 风格 API,但底层实现与 chromiumoxide 不同(用 `async-process` 启 chromium 然后通过 pipe 通信)。
- 同 MIT,自动下载 chromium 可关。

### 2.2 优势 / 劣势

- **优势**:API 简单、文档完整、Puppeteer 血统让前端转岗的 agent 容易上手;被许多爬虫/SSR 项目使用,生产证据比 chromiumoxide 多。
- **劣势**:不支持 frame/网络条件/SSL/WebSocket inspection;**底层不是 CDP 而是 `chrome-remote-interface` 风格**,并发模型不如 chromiumoxide 干净。

### 2.3 角色

- **不替代** chromiumoxide 作主路。
- **作 fallback 链第二档**:chromiumoxide 启动失败/版本冲突时,降级到 headless_chrome(API 接近,迁移成本 < 1 天)。

---

## 3. windows-webview2 / wry 兜底 — 不推荐生产截图

### 3.1 wry 现状

- `tauri-apps/wry`,Lib.rs 月下载 2.7M(主要靠 Tauri 生态拉动),0.55.1(May 4 2026)。
- 平台支持:Windows 7–11(系统装 WebView2 runtime)、macOS WebKit、Linux WebKitGTK、Android/iOS。
- **README 0 提及 headless / screenshot / 截帧** → **不是为截图设计的**。
- 强行截图:只能 `evaluate_script("…html2canvas…")` 走 DOM 自渲染,**质量差、慢、需可见窗口**。

### 3.2 WebView2 SDK 许可

- WebView2 Runtime 微软**免授权费**,但分 Evergreen(用户机自动更新)和 Fixed Version(打包 ~120MB)。
- Tauri 走 Evergreen,意味着用户机必须装过 Edge 或 WebView2 runtime,新装 Win11 默认带;老 Win10/11 缺失的 0.x% 需要 bootstrapper。
- **没有"headless capture"许可问题**,但有"运行时不在场则启动失败"的部署风险。

### 3.3 结论

**不作为 capture 主路。** 仅在 Windows 上若 chromiumoxide + chrome-headless-shell 都崩(极小概率,如 Win10 LTSC 没装 Edge)时,**用 wry 启可见窗口 + html2canvas 兜底**,但这是降级到 PPT 截图级别,不做视频帧。

---

## 4. macOS WKWebView — 现状不友好

### 4.1 Rust crate 情况

- `wkwebview-rs`:社区维护,**API 极薄**,不支持 headless 截图。
- 没有 Rust 原生 crate 能直接对 WKWebView 做 `takeSnapshot`(Objective-C `WKWebView.takePDFConfiguration` 是 macOS 13+ 才稳定,且需 NSView 嵌入)。
- 工业级路径:用 `objc2` + `block2` 直接调 `-takeSnapshotWithConfiguration:completionHandler:`,**没有 crate 包装**。

### 4.2 成本评估

- 自写 objc2 binding:首次实现 2–3 天(纯代码),但要测 macOS 13/14/15 三档。
- 替代:**用 chrome-headless-shell on macOS**(macOS Apple Silicon 二进制 ~150MB),与 Linux/Docker 共用一条 capture 路径,代码复用最大化。
- 架构上:CloakBrowser 项目已经有 macOS 平台抽象(本仓 [[forge-engine-prd]]),可借 CloakBrowser 路径分担。

### 4.3 结论

**mac capture 走 chrome-headless-shell(CloakBrowser 出口)+ 系统 Safari/WKWebView 仅作 dev/debug 兜底。** Rust 自研 WKWebView headless 截图 ROI 太低。

---

## 5. chrome-headless-shell vs 完整 chromium — 体积胜负手

### 5.1 定义

- "old Headless" 模式(C120 之前)以独立二进制形式重新打包,名 `chrome-headless-shell`,通过 Chrome for Testing 通道分发(`@puppeteer/browsers install chrome-headless-shell@stable`)。
- 砍掉:X11/Wayland、D-Bus、Chrome UI、扩展市场、PDF 视图等。
- 留:Chromium content 模块、Blink、V8、CDP server。
- 完整 chromium 包含 ~220MB+ 桌面 UI 资源(headless 模式根本用不到)。

### 5.2 体积

- 实测数据未抓(Google 仓库不直接暴露 binary size JSON),社区经验:chrome-headless-shell Linux x64 **~80–130MB**(随版本波动),完整 chromium **~250–300MB**。
- 与 235MB 总体目标对照:headless-shell **必须上**,完整 chromium **必须砍**。

### 5.3 缺失功能

- 无 Chrome UI 元素(本来就不用)。
- 扩展加载:受限,headless-shell 不支持 `chrome.runtime`,**适合 capture/scraping,不适合浏览器插件交互**(Polaris 无此需求,符合)。
- `Page.printToPDF` 在 headless-shell 上仍工作(CDP 路径)。
- `Page.captureScreenshot` 全功能(PNG/JPEG/WebP/clip/quality/fromSurface)。

### 5.4 生产证据

- 已知使用方:Puppeteer CI、Cypress、Playwright 测试套件的轻量化部署;Chrome for Testing JSON API 把它与完整 Chrome 并列分发。
- **没有 SaaS 头部的公开 case study**,但生态默认它就是 Puppeteer 在 CI 的标准件。

### 5.5 结论

**采用 chrome-headless-shell,砍完整 chromium。** Polaris Docker 镜像体积一仗全压在这条上,无第二条路。

---

## 6. Docker chromium `--no-sandbox` 工业级对策

### 6.1 风险面

- `--no-sandbox` 关闭 Chromium 的三层 sandbox(setuid / namespace / seccomp),任意渲染进程 bug 都能拿容器 root。
- 容器内默认 = root,会继承 docker.sock / host 路径 mount 权限 → 提权到 host。
- 已知 CVE(CVE-2023-4909 等)几乎全是 RCE 类,sandbox 是最后防线。

### 6.2 推荐组合(可写进 Dockerfile/compose.yml)

```yaml
# compose.yml 节选
services:
  polaris:
    image: polaris:render-slim
    security_opt:
      - seccomp:/etc/docker/seccomp/chromium.json   # 自定义白名单
      - apparmor:docker-chromium                    # 自定义 profile
      - no-new-privileges:true
    cap_drop:
      - ALL
    cap_add:
      - CHOWN
      - DAC_OVERRIDE
      - SETGID
      - SETUID
      - NET_RAW            # chromium 多进程要 raw socket
    read_only: true
    tmpfs:
      - /tmp
      - /home/polaris/.cache
    user: "1000:1000"
    shm_size: 256m         # chromium 默认 /dev/shm
    userns_mode: "host"    # 仅当 userns-remap 未在 daemon 启用时
```

- `userns-remap`:**daemon 级**,改 `/etc/docker/daemon.json`,映射容器 root → host unprivileged UID。**装系统后第一件事就开**,不能中途开(Docker 文档明确警告会"mask 已有 layer")。
- **权衡**:开 userns-remap 后 `--pid=host` / `--network=host` 不可用,挂载卷的宿主路径必须 chown 到 mapped UID。
- 如果宿主已有 GPU 直通(群晖 Container Manager 不可能,见 [[synology-docker-prd-gpu-decouple]]),userns-remap 反而麻烦;本项目无 GPU 直通,放心开。

### 6.3 必备:chromium 自定义 seccomp

- Docker 默认 seccomp 拒绝 44/300+ syscall。chromium 启动时常触发:
  - `add_key` / `keyctl` / `request_key`(GPU 上下文密钥)
  - `clone` 某些 flag(子进程 namespace)
  - `personality`(BSD emulation)
  - `perf_event_open`(性能分析,生产可砍)
- 自写 `/etc/docker/seccomp/chromium.json`,在默认基础上**追加 allow**,不要重写。

### 6.4 必备:AppArmor / SELinux

- AppArmor profile(`/etc/apparmor.d/docker-chromium`):
  - 拒绝 `ptrace`、`mount`、`pivot_root`、`module_load`。
  - 允许 `/dev/shm`(shm 渲染)、chromium cache 目录。
  - **不允许 X11 socket**(本项目 headless,不需要)。
- 装载:`apparmor_parser -r /etc/apparmor.d/docker-chromium`。

### 6.5 工业级补充

- 容器以 non-root(UID 1000)跑,镜像 `USER polaris`。
- `--read-only` + 多个 `tmpfs` 临时目录。
- 关 `--privileged`,**永远不**。
- 网络:用 `network_mode: bridge` + 显式 `ports`,不要 `network_mode: host`。
- 日志:`logging.driver=json-file` + `max-size=10m max-file=3`,防止 chromium debug 灌爆磁盘。
- 资源:`mem_limit: 4g` + `cpus: 4` + `pids_limit: 256`(chromium 多 tab 拉进程会爆 pids)。

### 6.6 结论

**Docker 必须叠加五重:userns-remap(daemon) + 自定义 seccomp + AppArmor + cap-drop ALL + non-root USER。** 单 `--no-sandbox` 裸跑不可上线。

---

## 7. 多 target 帧分片并发 — 60s×30fps 场景

### 7.1 目标场景

- 1800 帧、1080p PNG、单帧 < 200KB。
- 串行持久 CDP:~90–220s(单 target)。
- 期望:压到 < 60s(可交互阈值)。

### 7.2 方案 A:单 Browser 多 Page(target 池)

- 1 个 Browser,4–8 个 Page 池,每页处理完即放回池。
- Page 间通过 round-robin 分配帧号。
- 浏览器主进程只占 1 个 Chromium 实例,WebSocket 单连接,**CDP 不需为多 tab 复制**。
- 经验提速 3–4×。

### 7.3 方案 B:多 Browser 实例分片(横向)

- N 个独立 Browser,每个 1 个 Page,接 frame-index 哈希分片。
- 每个 Browser 独立 WebSocket,故障隔离。
- 资源代价 = N × chromium 进程,mem 涨。
- 经验提速 N× 接近线性,**但 N≥4 后 I/O 瓶颈成主**(磁盘 / CPU 编解码撞顶)。

### 7.4 推荐混合

- 默认:**方案 A 4–8 Page 池**。
- 高配模式(用户显式选):**方案 B,2 个 Browser × 4 Page** = 8 切分,撞 CPU 上限自动退到 A。

### 7.5 配套

- 帧号哈希分片避免乱序。
- 失败帧重试隔离到单 frame,不阻塞后续。
- capture 完统一按帧号写盘,落 `.pending/` 子目录,ffmpeg 拼接前原子 rename → `.frames/`。
- `__pending` 栅栏(见 [[forge-engine-prd]])防"半截视频"。

### 7.6 结论

**P0 = 方案 A(单 Browser 多 Page)。P1 = 方案 B(可配置并发上限)。** 数字等真机 bench 确认。

---

## 8. 完整 fallback 链

```
capture_request
  │
  ├─1. chromiumoxide 持久 CDP(主)
  │    ├─ 成功: 继续
  │    └─ 失败(WS 断/版本不兼容) ──┐
  │                                 │
  ├─2. headless_chrome CDP 客户端    │
  │    ├─ 成功                       │
  │    └─ 失败 ──┐                   │
  │              │                   │
  ├─3. chromium CLI --screenshot=   ←┘
  │    ├─ 成功(给单帧;视频路径放弃,转 wechat-yiban 文本分支)
  │    └─ 失败 ──┐
  │              │
  ├─4. (仅 Windows) wry + html2canvas 兜底(走 PPT 截图路径)
  │    ├─ 成功
  │    └─ 失败
  │
  └─5. 报错 + 重试 N 次(指数退避) + 写入 capture_errors.log
```

- Playwright **不进入生产 fallback** — 已被 `forge_pptx` 自研替掉,保留为 debug-only。
- 每次降级要写 `capture_metrics.json` 记录"用了哪档",为 P5 evaluator 累积数据。
- 告警阈值:24h 内降级率 > 5% → 触发人工核查。

---

## 9. CloakBrowser — 仅作浏览器插件入口,不进 capture 路径

- CloakBrowser = Chromium 嵌入式发行(本仓 [[forge-engine-prd]] 已要求"任何浏览器动作走 CloakBrowser")。
- 公开 SDK:**Node 为主**(`@cloak-browser/sdk`),无 Rust crate。
- Rust 接入只能走 IPC(stdin/stdout JSON-RPC 或 WebSocket),每次调用 ~2–5ms 序列化开销 + Node 进程常驻 ~80–120MB RSS。
- **判断**:
  - capture 路径**不绕道 CloakBrowser**,直接 chromiumoxide → chrome-headless-shell。
  - CloakBrowser 在 Polaris 内的角色:**默认浏览器(用户点链接时)+ 浏览器插件宿主(板块⑨ 自动化)**。
  - 避免 capture 路径多一个 IPC hop,符合"工业级 = 路径短、可观测"原则。

---

## 10. 选型总览表

| 子系统 | 主选 | 备选 / Fallback | 决策 |
|---|---|---|---|
| CDP 客户端 | `chromiumoxide 0.9.1` | `headless_chrome 1.0.21` | chromiumoxide 主,headless_chrome 兜底 |
| 浏览器内核 | `chrome-headless-shell` | 完整 chromium(打镜像时)**禁** | 砍完整 chromium,只装 headless-shell |
| Windows 兜底 | wry(WebView2)**仅 dev** | — | 生产截图不走 wry |
| macOS 兜底 | chrome-headless-shell on macOS | objc2 WKWebView(不写) | 复用同一条 CDP 路径 |
| 并发 | 1 Browser × 4–8 Page | N Browser 横向(高配) | 默认池模式,高配可选 |
| Docker 加固 | userns-remap + seccomp + AppArmor + cap-drop + non-root | — | 五重叠加必上 |
| 失败链 | chromiumoxide → headless_chrome → CLI → wry → 报错 | Playwright(debug only) | 五档降级 |
| CloakBrowser 角色 | 默认浏览器 + 插件宿主 | — | **不进 capture** |

---

## 11. 落地建议(P3+P5 衔接)

- P3 registry 必收:
  - `chromiumoxide = "=0.9.1"`(pin,等 1.0)
  - `chrome-headless-shell` 二进制按 `puppeteer-browsers` 拉取,版本跟 Chrome for Testing stable。
- P3 preflight 加 `capture_browser_ok` 探测:`Browser::launch(headless).new_page("about:blank").screenshot(...)` 拿 1×1 PNG,5s 超时。
- P5 evaluator 加 `capture-bench`:渲染 100 帧测试 deck,记录串行 / 4-Page 池 / 8-Page 池三档吞吐,定基线。
- 真机未点测;未 commit;未写进 forge.rs。

---

## 12. 已知 Gaps

- chromiumoxide **MSRV 数字**未抓到(仅知支持列出的 5 个 target),需真机 `cargo +1.78 check`。
- `chrome-headless-shell` 各平台**精确二进制大小**未抓(Chrome for Testing JSON 不暴露 size 字段),需要 `gh release download` 一次实测。
- wry 的"headless 模式"在 0.55 仍无官方支持,需 0.56+ 跟踪 issue。
- 多 Browser 并发方案的**真实吞吐数据**没在公开 benchmark 中找到(各家都自测),需自建。
- 群晖 Container Manager 是否支持 `userns-remap` 未验证(Synology DSM 7 默认禁),需本机 docker info 测一次。

---

## 13. Leads discovered / Gaps

- **Lead**: chromiumoxide 仓库 178 fork 中可能有公司内部维护版(搜 `forks?include=active` 可筛活跃 fork,作为 bus-factor 备份)。
- **Lead**: Chrome for Testing 团队正在把 headless-shell 作为"old headless 的官方继承者",下一版本可能改名为 `chrome-headless`,需盯 release notes。
- **Lead**: wry 项目 Tauri 内部有"headless webview"feature 讨论的 issue,跟进 0.56+。
- **Gap**: chromiumoxide 与 CloakBrowser 关系未公开(两者都基于 CDP),理论上 CloakBrowser 可作为 chromiumoxide 的 launch binary,需查 CloakBrowser 文档。
- **Gap**: macOS apple silicon 上 chrome-headless-shell 的 native vs Rosetta 性能差异无公开数据,需真机 bench。
