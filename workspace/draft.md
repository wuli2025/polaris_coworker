# Polaris Forge · 自研工业级化方案

> Date: 2026-06-10 | Output: Full report | Sources: 36（registry.md）
> Stakes: High | Evaluation: Pending（轻量自评）
> 接 [[forge-engine-prd]] [[forge-crossplatform-foundation]] [[docker-render-image-size-audit]]，兑现用户原话 "你把能自研的都自研一下，要求工业级的稳定的，分 winmac 和 docker 版本，docker 版本一定要做好，稳定"

---

## Executive Summary

Polaris Forge 的自研工业级化分四条线：(1) capture 用 `chromiumoxide 0.9.1` 持久 CDP 替「每帧起 chromium 进程」，三平台共用 `chrome-headless-shell`；(2) codec 用 `openh264-rs` + 自写最小 mp4 muxer + `ebur128` + `cosmic-text`，二进制增量 3.3-5MB，可全替 `ffmpeg` CLI；(3) forge_pptx / forge_tts / forge_fx 三个已自研件做 a11y/schema 校验/字幕/L1 重试/动效错误隔离等"工业级化"清单（P0+P1 ≈ 9.5 天）；(4) Docker 渲染 flavor 做 userns-remap + seccomp + AppArmor + cap-drop + non-root 五重加固 + 资源精控 + 字体子集，目标 235MB 渲染层。

**核心 trade-off**：浏览器内核不自研（几千万行）→ Docker 必须自带 ~130MB `chrome-headless-shell`；ffmpeg 可全替（许可、二进制、可观测性）但保留为逃生口。

**最大不确定性**：openh264 编码质量比 libx264 medium 差 2-3dB PSNR（同码率略糊），且 aarch64 仅编译未单测——必须落 forge-bench 真机验证 + 保留 ffmpeg CLI 兜底。

**Confidence**：高（capture/codec/已自研件/Docker 各自的单一选型）；中（跨板块集成后端到端不掉一帧）；低（forge-bench 缺独立数字，需 P5 真机测）。

---

## 1. 范围与边界（再确认）

来自 [[forge-engine-prd]] 与桌面 HTML PRD 的不可妥协点：

| 组件 | 自研 / 调外部 | 理由 |
|---|---|---|
| 浏览器内核 | **永远不自研** | Chromium 几千万行；Docker 必须自带（约 130MB headless-shell） |
| CDP 客户端 | 自研路线：`chromiumoxide 0.9.1` 持久连接 | 替"每帧起 chromium 进程"，提速 6-10× |
| H.264 编码 | 自研路线：`openh264-rs` + 自写 mp4 muxer | 替 ffmpeg，二进制 ~3.3-5MB 增量 |
| 响度归一化 | `ebur128 0.1.10`（Sebastian Dröge 维护，EBU 测试过） | 自研无优势 |
| 字幕 | `cosmic-text 0.19`（Pop!_OS COSMIC 桌面生产用） | CJK/RTL 支持 |
| 音频编解码 | `symphonia 0.6` 解 + `audiopus` 编（Opus） | 拒绝 FDK-AAC 商用付费 / 拒绝 LAME 灰区 |
| 视频转场 | **留在浏览器**（fx 板块已自研） | Rust 端像素合成重复造轮 |
| PPTX | `forge_pptx.rs` 已自研（隐形文本层） | 工业级化 |
| TTS | `forge_tts.rs`（MiniMax L0）已自研 | L1 edge-tts Rust 端口全 0 star，改 MiniMax 失败重试链 |
| 动效 | `forge_fx`（__fx.seek 确定性时钟）已自研 | 工业级化（错误隔离 + spring 闭式解） |
| 离线兜底 TTS | Windows SAPI / mac say / Linux espeak | 工业级化新增 |
| 编码逃生口 | ffmpeg CLI（保留） | forge-codec 挂掉时兜底 |

**关键决策推翻**：架构 v2 提的 L1 "edge-tts 免费神经层" —— 经查 `Initsnow/MasterXD/yynag` 三个 Rust 端口**全 0 star**、2024-2026 个人维护，**生产慎用**。改 L1 = MiniMax 失败重试链（同源不同 voice_id）。

**关键决策保留**：架构 v2 提的 `chrome-headless-shell` + 字体子集 + 自写 mp4 muxer —— 全部采纳。

---

## 2. 三平台分层架构

### 2.1 trait 抽象

```text
forge-capture (port = crate::capture)
  ├── trait Browser { fn new_page() -> Page; fn launch(cfg) -> Result; }
  │     ├── CdpBrowser (chromiumoxide 持久 CDP)         ← Win/Mac/Docker 主路
  │     ├── CliBrowser (chromium --headless --screenshot) ← 降级 3 档
  │     └── WebViewBrowser (wry + html2canvas)          ← Win 降级 4 档（仅 PPT）

forge-codec (port = crate::codec)
  ├── trait Encoder { fn push_frame(&mut self, FxFrame) -> Result; fn finish(self) -> Vec<u8>; }
  │     ├── H264Encoder (openh264-rs + 自写 BMFF muxer)  ← 主路
  │     └── FfmpegEncoder (ffmpeg CLI wrapper)           ← 逃生口
  ├── trait AudioEncoder { fn encode_pcm(&mut self, &[i16]) -> Result<Vec<u8>>; }
  │     ├── OpusEncoder (audiopus)                        ← 主路
  │     └── FfmpegAudioEncoder                           ← 逃生口
  ├── trait Loudness { fn analyze(pcm) -> LUFS; fn scale(pcm, target_lufs) -> Vec<i16>; }
  │     └── Ebur128Loudness (ebur128 0.1.10)              ← 三平台统一
  └── trait FrameSink (fx → codec 桥)                     ← 见 §3.4

forge-pptx (port = crate::pptx, 已落 forge_pptx.rs)
  └── 纯 Rust OOXML + 隐形文本层 + schema 校验 + 字体子集内嵌

forge-tts (port = crate::tts, 已落 forge_tts.rs)
  ├── L0 MiniMax T2A v2 (主力 zh-CN 神经层)
  ├── L1 MiniMax 失败重试链 (替原 edge-tts 神经层)
  ├── L3 macOS say / Windows SAPI / Linux espeak (离线兜底)
  └── Silent 兜底 (1s 静音 + 字幕)
```

### 2.2 平台后端选型

| 平台 | Browser 后端 | Codec 后端 | TTS 兜底 | 离线 |
|---|---|---|---|---|
| **Windows** | chromiumoxide + chrome-headless-shell；wry+WebView2 降级（仅 PPT） | openh264-rs (source 静态编译) | Windows SAPI 5.4 (ISpVoice) | OK |
| **macOS** | chromiumoxide + chrome-headless-shell（**不**走 WKWebView，Rust crate 太薄） | openh264-rs (Apple Silicon) | macOS `say -v Tingting` | OK |
| **Docker** | chromiumoxide + chrome-headless-shell（瘦形态，~80-130MB vs 完整 chromium 250-300MB） | openh264-rs (linux-gnu)；ffmpeg CLI 保留 | Linux espeak-ng / Silent | slim 无渲染 |

**关键决策**：三平台 capture 路径**统一**走 chromiumoxide + chrome-headless-shell。macOS 不单独维护 WKWebView objc2 binding（首次实现 2-3 天 + 三档系统测试 ROI 太低），复用同一条 CDP 路径最大化代码复用。

### 2.3 编码层二进制体积估算

| crate | 体积增量 | 来源 |
|---|---|---|
| openh264 静态编译 | ~1.2MB (x86) / 1.0MB (aarch64) | [4][5] |
| 自写 mp4 muxer | ~30KB | 自研 |
| ebur128 | ~200KB | [8] |
| cosmic-text + swash + HarfRust | ~800KB-1.2MB | [9] |
| symphonia (MP3+AAC 解码器) | ~600KB-1MB | [10] |
| audiopus + libopus | ~400KB | [11] |
| **合计** | **3.3-5MB** | 匹配「几 MB」目标 |

许可：全链 MIT/BSD-2/Apache-2.0/MPL-2.0，**0 冲突**（拒绝 FDK-AAC 商用付费 + 拒绝 LAME 灰区）。

---

## 3. capture 板块工业级化

### 3.1 选型

| 层 | 主选 | 备选 | 决策 |
|---|---|---|---|
| CDP 客户端 | `chromiumoxide 0.9.1` ([1]) | `headless_chrome 1.0.21` ([2]) | chromiumoxide 主，headless_chrome 兜底 |
| 浏览器内核 | `chrome-headless-shell` (Chrome for Testing 分发) | 完整 chromium 禁 | 必须上瘦形态 |
| Windows 兜底 | wry (WebView2) **仅 dev** | — | 生产截图不走 wry |
| macOS 兜底 | chrome-headless-shell on macOS | objc2 WKWebView 不写 | 复用同一条 CDP 路径 |
| 并发 | 1 Browser × 4-8 Page 池 | N Browser 横向（高配可选） | 默认池模式 |
| Docker 加固 | userns-remap + seccomp + AppArmor + cap-drop + non-root | — | 五重叠加必上 |
| 失败链 | chromiumoxide → headless_chrome → CLI → wry → 报错 | Playwright (debug only) | 五档降级 |
| CloakBrowser | 默认浏览器 + 插件宿主 | — | **不进 capture** |

### 3.2 持久 CDP vs 每帧 CLI 性能假设

来自 [1]（chromiumoxide README 性能特征）+ 实测粗估：

- 单帧 CLI：冷启动 ~250-600ms + 导航 50-200ms + 截图 30-80ms = **350-900ms/帧**
- 60s 视频 30fps = 1800 帧 → 单进程串行 = **10-27 分钟**
- 持久 CDP：启动 ~400ms 一次性 + 每帧 50-120ms → **90-220 秒** ≈ **6-10× 提速**
- 4-8 Page 池：再 3-4× → **20-60 秒**

**这些数字待 P5 真机 bench 确认**（G4 缺口）。

### 3.3 工业级必备补丁

- **reconnect-on-stale** wrapper：CDP WebSocket 断了不要 panic，自动重连 ≤3 次
- **target-pool**：4-8 Page 池共享 Browser，避免每帧 `new_page` 触发完整文档加载
- **render-ready gate**：`Page.lifecycleEvent` + `__deckReady` 标记防白屏截图
- **锁版本**：`chromiumoxide = "=0.9.1"`（等号 pin）直到 1.0

### 3.4 fx → codec 桥接口

`pub trait FxFrameSink {
    async fn write_frame(&mut self, f: FxFrame) -> Result<(), String>;
}
struct FxFrame { t_ms: u64, width: u32, height: u32, rgba: Vec<u8>, keyframe: bool }`

- forge-codec 实现 `impl FxFrameSink for H264Sink`（P2-B 落地）
- 关键帧策略：每 60 帧（2s@30fps）强插 + 大色块突变即时插
- 错误恢复：codec 写失败 → 帧先落盘 `/tmp/forge-fx-frames/*.rgba`，codec 故障可重跑

### 3.5 Docker `--no-sandbox` 风险面与五重加固

```yaml
# compose 必加
security_opt:
  - seccomp:/etc/docker/seccomp/chromium.json   # 自定义白名单 [16]
  - apparmor:docker-chromium                    # 自定义 profile
  - no-new-privileges:true
cap_drop: [ALL]
cap_add: [CHOWN, SETUID, SETGID, DAC_OVERRIDE, NET_RAW]
read_only: true
user: "1000:1000"
shm_size: 1g
userns_mode: "host"   # 或 daemon 配 userns-remap
```

参考 [15] Zenika/alpine-chrome 三方案共识 + [16] Jessie Frazelle seccomp profile 事实标准。

---

## 4. codec 板块工业级化

### 4.1 选型决策矩阵

| 板块 | 推荐 | 备选 | 自研边界 |
|---|---|---|---|
| H.264 编码 | `openh264-rs 0.9.3` `source` feature（静态捆绑）| `libloading` 调系统库 | **不推荐**自研 H.264 encoder（>50k 行 C 等价）|
| MP4 muxer | **自写最小 BMFF**（moof/mdat/traf）+ mp4-rust 0.14 验证 box 正确性 | mp4-rust fork 改增量写 | 自写 ~500-800 行可控 |
| 响度归一化 | `ebur128 0.1.10` ([8]) | 自写 K-weighting（~200 行）| ebur128 已交付 EBU 测试 |
| 字幕 | `cosmic-text 0.19` + swash ([9]) | glyph_brush（已停止）| 自研 shaping > 1k 行 |
| 音频解码 | `symphonia 0.6` + MP3/AAC feature ([10]) | minimp3 (仅 mp3) | — |
| 音频编码 | **Opus via audiopus 0.2.0** ([11]) | MP3 via LAME（许可灰）| **拒绝 FDK-AAC**（商用付费）|
| 转场 | **留在浏览器**（fx 板块已自研）| — | 拒绝 Rust 端像素合成 |
| 逃生口 | ffmpeg CLI（jrottenberg/ffmpeg:7-slim, ~30MB）| 静态 ffmpeg 自构建 | — |

### 4.2 已知风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| openh264 aarch64 仅编译未单测 | aarch64 平台编码可能错 | 落 forge-bench：编 10s + ffprobe 校验（**P5 必做**） |
| openh264 同码率比 libx264 medium 差 2-3dB PSNR | 视觉略糊 | 默认 1.5-2x 目标码率；保留 ffmpeg+libx264 兜底 |
| Cisco openh264 BINARY_LICENSE 非 BSD | 商分发约束 | 选 `source` feature 静态编译，**不**走 BINARY_LICENSE 路径 |
| mp4-rust 2.7 年未更新 | 维护风险 | 自写 muxer 为主，mp4-rust 仅作 box 正确性对照 |
| audiopus 5 年停滞 | libopus 1.5+ API 变动 | 监控 libopus 上游，准备 fork |
| cosmic-text RTL/Bidirectional 未完工 | 阿拉伯/希伯来文渲染 | 字幕用 `harfbuzz-sys` 兜底（pango-ffi 也可） |

### 4.3 性能基线（待 P5 验证）

业界经验值（无独立基准，列为预期）：

- openh264 x86 单核 H.264 Baseline 1080p@30fps = 0.5-0.8x 实时
- Apple Silicon NEON 加速后 = 0.5-0.7x 实时
- ARM64 (aarch64-linux) 无 NEON = 0.6-0.9x 实时（勉强实时）
- ebur128 旁链 + Opus 编码 1 分钟 PCM ≈ 0.3-0.5s
- cosmic-text 字幕烧录 ≈ 0.5-1s/千字

---

## 5. 已自研件工业级化（forge_pptx / forge_tts / forge_fx）

### 5.1 forge_pptx 工业级化（P0 ≈ 2.5 天）

来自 [34] 现状分析：

| # | 工程动作 | 估时 |
|---|---|---|
| A.4.1 | **流式写**：`images.push` 改成 `let bytes = read; put; drop;` —— 内存峰值从 N 张降到 1 张 | 0.5d |
| A.3.1 | **自写最小 OOXML 校验器**：解压 zip + 校验 `[Content_Types].xml`/`.rels`/`presentation.xml` + media hash —— 零新依赖（zip 已用）| 1d |
| A.1.3 | **隐形文本框双保险**：`alpha=0` + `<a:effectLst><a:noFill/></a:effectLst>` 应对 Keynote16 风险 | 0.5d |
| A.2 | **兼容矩阵报告**：`forge_pptx::compatibility_report()` 静态报告，集成到 `/api/status` 红绿灯 | 0.5d |

### 5.2 forge_tts 工业级化（P0 ≈ 2 天）

来自 [35] + 推翻架构 v2 的 edge-tts L1：

| # | 工程动作 | 估时 |
|---|---|---|
| B.3.1 | **chunk 切分**：`chunk_text(text, max_chars=1800)` 按 `,。?!;` 切，>2000 字偶发截断 | 0.5d |
| B.3.2 | **chunk 间补 0.3s 静音** + 2 并发（`tokio::spawn`） | 0.5d |
| B.5 | **MiniMax 401/403/429 静默降级** + `governor` 2 req/s 节流 | 0.5d |
| B.4 | **Windows SAPI 5.4**（`ISpVoice` via windows-sys）+ **Linux espeak-ng** 兜底 | 0.5d |
| B.1 改 | **L1 = MiniMax 失败重试链**（替 edge-tts 神经层）| 0.5d |

### 5.3 forge_fx 工业级化（P0 ≈ 0.5 天 + P1 ≈ 1 天）

| # | 工程动作 | 估时 |
|---|---|---|
| C.2.1 | **26 个动效 safe_run** 错误隔离 + 健康徽章 + 主时钟不挂 | 0.5d |
| C.4.1 | **fx → codec 接口 FxFrameSink trait** 定义 + capture 端发 channel | 1d（P1） |
| C.1.1 | **`__pending` 5s 超时** + broken 标记 | 1d（P2） |
| C.3.1 | **spring 闭式解** `x(t)=x0·cos(ωt)+v0/ω·sin(ωt)` 跨平台 1e-9 容差 | 1d（P2） |

### 5.4 通用层（P2 约 1 天）

- 统一 `enum ForgeError { Io, Network, Ooxml, Tts, Fx, Codec }` + `thiserror`
- 统一 telemetry：`~/Polaris/data/forge_telemetry.jsonl` + `/api/status` 最近 100 条计数
- 统一熔断 + 降级表：`forge::run_with_strategy(name, retries, backoff, fallback)`
- 单元测试基线：每件 ≥ 8 单测
- cargo features：默认全开，Docker slim `--no-default-features` 关

### 5.5 优先级总表

| 优先级 | 项 | 估时 | 累计 |
|---|---|---|---|
| P0 | A.4.1 + A.3.1 + A.1.3 + A.2 + B.3 + B.4 + B.5 + C.2.1 | 4.5d | 4.5d |
| P1 | A.5 字体子集 + B.1 L1 重试链 + B.2 VTT/SRT + C.4 接口 | 5d | 9.5d |
| P2 | A.1.1 alt-text + B.6 统计 + C.1.1 + C.3 + D 通用层 | 5d | 14.5d |

---

## 6. Docker 工业级稳定性（用户硬指标）

> 详见 task-d.md（50+ 条工程动作），下表为落地子集。

### 6.1 资源与进程加固

```yaml
# compose 增量（详见 task-d §10）
cap_drop: [ALL]
cap_add: [CHOWN, SETUID, SETGID, DAC_OVERRIDE]
security_opt:
  - "no-new-privileges:true"
  - "seccomp=/etc/docker/seccomp/chrome.json"   # 拷入镜像
mem_limit: 4g              # full
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
  start_period: 60s
```

```dockerfile
# Dockerfile 增量（tini -g）
ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/docker-entrypoint.sh"]
```

### 6.2 字体 SC 子集（关键体积杀手）

```dockerfile
FROM python:3.12-slim AS font-subset
RUN pip install --no-cache-dir fonttools brotli
COPY docker/font-subset-chars.txt /chars.txt   # 6763 高频字 + ASCII + emoji
COPY docker/subset_cjk.py /subset_cjk.py
RUN python /subset_cjk.py   # 输出 3 个 weight woff2

# runtime 引用
COPY --from=font-subset /out/ /usr/share/fonts/truetype/noto-cjk-subset/
RUN fc-cache -fv
```

**HTML 字体回退链**：`font-family: "Noto Sans CJK SC", "Noto Sans", "DejaVu Sans", sans-serif;`

### 6.3 启动顺序 / healthcheck

- tini `-g` 杀进程组（详 D-4.1）
- healthcheck 拆 `/api/health`（200 即活）+ `/api/ready`（带渲染栈语义）
- preflight 启动时跑 → 写 `~/.polaris/last_preflight.json` mode 0600
- 启动 banner：`═══════ Polaris {ver} ═══════ render={full|slim} shm=1g mem=4g pids=512 cpus=2.0 ═══════`

### 6.4 网络与安全

- **0.0.0.0:8080** 保留，README 文档化"反代统一 443+TLS"
- **自动 token**：未设 `POLARIS_AUTH_TOKEN` 时 `openssl rand -hex 32` + 写 `~/.polaris/auto_token` + 日志打印完整访问 URL；`POLARIS_NO_AUTH=1` 显式禁用（[31] 已落）
- **CSP 双设**：桌面 `tauri.conf` + Docker `tower-http` 响应头
- **出网白名单**：`POLARIS_EGRESS_ALLOWLIST=api.anthropic.com,api.minimaxi.com,...` 应用层拦截
- **secrets**：compose `secrets:` 段，生产建议挂

### 6.5 优雅降级链

```
chrome-headless-shell 启动失败
  → 退到完整 chromium (单次重试)
  → 退到 chromiumoxide 不行, 退到 headless_chrome
  → 退到 chromium CLI (单帧出片, 视频路径放弃)
  → Windows: wry + html2canvas (仅 PPT)
  → 报错 + 重试 3 次 (指数退避 500ms/1s/2s)
  → 写 capture_errors.log
```

### 6.6 可观测性

- tracing JSON / pretty 双格式（`LOG_FORMAT=json`）
- `metrics-exporter-prometheus` `/metrics` 端口 9090
- 关键 metric：`polaris_boot_duration_seconds` / `polaris_first_frame_seconds{op}` / `polaris_encode_seconds{kind}` / `polaris_chromium_restart_total` / `polaris_preflight_blockers`
- 告警阈值：24h 内降级率 > 5% 触发人工核查

### 6.7 preflight 工业化

- 拆 `[[bin]] polaris-forge`，CLI 单跑 `polaris-forge preflight --json` 写 `~/.polaris/last_preflight.json` mode 0600
- exit code 语义：0=OK / 1=blocker / 2=warn
- agent 友好：JSON to stdout + `blockers` 数组

### 6.8 Dockerfile 落地三阶段（P0-1）

| 阶段 | 改动 | 验证 |
|---|---|---|
| P0-1 | cap_drop/cap_add/seccomp/tini -g + SC 字体子集 + fc-cache | docker build --build-arg POLARIS_RENDER=1 出 **235MB** 镜像 |
| P0-2 | mem_reservation/pids_limit/cpus/ulimits/security_opt | docker compose up + curl /api/health + /api/ready |
| P1-1 | forge preflight 拆 bin / D-7.1 JSON log / metric / retry | cargo test 绿 + 真机出 PPT 端到端 |
| P1-2 | auto-token / CSP / egress allowlist | 集成测：未设 token → 浏览器跳 token URL；CSP 头存在 |
| P2 | OpenTelemetry + 多服务编排 | K8s staging |

---

## 7. 端到端落地路线图（4 阶段）

### 阶段 0 · 体积瘦身（与引擎路线不冲突，本周可做）

**目标**：Docker full 镜像 994MB → 235MB

| 改动 | 文件 | 估时 | 验证 |
|---|---|---|---|
| 换 chrome-headless-shell | Dockerfile | 0.5d | docker build 出镜像 < 200MB |
| 静态 ffmpeg | Dockerfile | 0.5d | preflight 报 ffmpeg_static_ok:true |
| SC 字体子集 | Dockerfile + font-subset-chars.txt | 1d | 浏览器渲染中文 PPT 无豆腐 |

### 阶段 1 · capture 提速 + 编码后端替身（2-3 周）

**目标**：60s 视频出片 10-27 分钟 → 20-60 秒；forge-codec 替 ffmpeg CLI

| 改动 | 文件 | 估时 | 验证 |
|---|---|---|---|
| 引入 `chromiumoxide = "=0.9.1"` | src-tauri/Cargo.toml | 0.5d | cargo build 绿 |
| 替换 `forge_video.rs` 截图路径 | src-tauri/src/forge_video.rs | 2d | 真机 100 帧 4-Page 池 bench |
| 新建 `crates/polaris-forge-codec` 骨架 | src-tauri/crates/polaris-forge-codec/ | 1d | cargo test 骨架 5 测绿 |
| 集成 `openh264-rs` H.264 编码 | codec 板块 | 2d | 10s 1080p 出 mp4 + ffprobe 校验 |
| 自写最小 BMFF muxer | codec 板块 | 2d | mediainfo 校验 ISO/IEC 14496-12 |
| 集成 `ebur128` 响度归一化 | codec 板块 | 1d | 单测 EBU TECH 3341 测试向量过 |
| 集成 `cosmic-text` 字幕烧录 | codec 板块 | 2d | 1000 字字幕 1s 内烧完 |
| 集成 `symphonia` + `audiopus` 音轨 | codec 板块 | 1d | mp4 含 aac 音轨 + 音量归 -23 LUFS |
| 保留 ffmpeg CLI 兜底 + fallback 链 | forge 板块 | 1d | codec 故意 fail → 自动降 ffmpeg |
| `FxFrameSink` trait + capture 端发 channel | forge 板块 | 1d | cargo test 绿 |

**阶段 1 验证产物**：60s 1080p 视频出片 < 60s + 端到端 mp4 一帧不掉。

### 阶段 2 · 桌面/移动 capture 后端替身（1-2 周）

**目标**：三平台 capture 路径工业级化

| 改动 | 文件 | 估时 | 验证 |
|---|---|---|---|
| Windows wry + WebView2 降级 | forge-capture | 1d | dev 模式真机 + Win10 LTSC 无 Edge 兜底 |
| macOS apple silicon chrome-headless-shell 集成 | forge-capture | 1d | 真机 bench (native vs Rosetta) |
| 多 target 帧分片 2-Browser×4-Page 高配 | forge-capture | 1d | bench 8 target 提速 4x |
| 全部 Docker 工业级加固 (D-1~D-8 50+ 条) | Dockerfile + compose + forge.rs | 2d | docker build < 240MB + 真机 ppt 出片不掉 |
| preflight 拆 bin + JSON log + metric | forge.rs | 1d | /api/ready 报 can_render_ppt 准确 |

### 阶段 3 · 已自研件工业级化 + 通用层（2 周）

**目标**：forge_pptx / forge_tts / forge_fx "工业级化" + 通用 error/telemetry

P0 4.5d + P1 5d = 9.5d（详 §5.5 表），本周开 P0、下周开 P1。

---

## 8. 风险清单与兜底方案

| 自研件 | 挂掉模式 | 兜底 | 检测 |
|---|---|---|---|
| chromiumoxide | 启动失败 / WS 断 | headless_chrome → CLI → wry → 报错 | chromiumoxide 0.6 进程泄漏已知，wrapper 加重试 |
| chrome-headless-shell | 启动失败 | 单次重试退完整 chromium | preflight 启动 5s 超时 |
| openh264 aarch64 | 编译或编码错 | 退 libx264 + ffmpeg CLI | P5 forge-bench 必做 |
| openh264 质量差 | 视觉糊 | 1.5-2x 码率 + ffmpeg+libx264 兜底 | PSNR 自动测 |
| 自写 muxer | box 错 | mp4-rust 对照 + ffprobe 校验 | ci 每 PR 跑 mediainfo |
| ebur128 | EBU 测试漂移 | 单测 EBU TEST 3341/3342 向量 | CI 必过 |
| cosmic-text RTL | 阿语/希伯来渲染错 | 改用 harfbuzz-sys 或 pango-ffi | 字符覆盖审计 |
| audiopus | libopus 1.5+ 改 API | fork audiopus + 直绑 libopus | 监控上游 release |
| forge_tts L0 MiniMax | 401/403/429 | 失败重试链（不同 voice_id）→ L3 系统 → Silent | tier_downgraded 字段 |
| forge_tts L1 edge-tts | 缺稳态 Rust 实现 | **已不采用** | — |
| forge_pptx 隐形文本 | Keynote16 可见 | 双保险 alpha=0 + effectLst | 兼容矩阵报告 |
| forge_fx 动效 | 抛错污染全局 | safe_run 错误隔离 + 主时钟不挂 | /api/status 健康徽章 |
| Docker `--no-sandbox` | 容器逃逸 | userns-remap + seccomp + AppArmor + cap-drop + non-root | 镜像扫 trivy |
| Docker 字体子集 | 命中率不足 99% | build 阶段字符覆盖审计失败 → 退全语种 | build 阶段 ffprobe 验证 |
| tini 兜底 | 进程组泄漏 | tini -g 杀进程组 + chromiumoxide Browser drop | /api/metrics 报孤儿数 |
| 磁盘满 | preflight 写不出 | try/catch warn 不 fail | /api/health 200 不被拖累 |

---

## 9. 关键决策记录（与 PRD 对账）

| 本方案 | 引用 | 决策 |
|---|---|---|
| 浏览器内核不自研 | [[forge-engine-prd]] ADR-002 | 延续 |
| ffmpeg 可被 openh264 替 | [[forge-engine-prd]] ADR-003 | 采纳 + 保留兜底 |
| 字体子集内嵌 | [[forge-engine-prd]] ADR-011 | 采纳（Docker 阶段 P0-1 落地）|
| `__pending` 栅栏 + 5s 超时 | [[forge-engine-prd]] | 增强 |
| **edge-tts 神经层** | [[forge-engine-prd]] | **推翻** —— Rust 端口全 0 star，改 MiniMax 失败重试链 |
| **macOS WKWebView 自研 objc2 binding** | [[forge-engine-prd]] 备选 | **推翻** —— ROI 太低，mac 走 chrome-headless-shell |
| chrome-headless-shell + 静态 ffmpeg + SC 字体 | [[docker-render-image-size-audit]] P0 | 采纳（阶段 0）|
| tini + shm + 跨平台 preflight | [[forge-crossplatform-foundation]] | 增强（加 tini -g + 五重加固）|
| no-anim 误伤 .slide 叠加 | [[deck-export-overlap-noanim-bug]] | 已知坑已修（详见 §5.3 C.2.3）|
| auto-token + CSP | [[security-hardening-pass-3]] | 延续（Docker + 桌面双设）|
| 不进 capture 的 CloakBrowser | [[use-cloakbrowser-always]] | 延续（默认浏览器 + 插件宿主）|

---

## 10. 引用

[1] chromiumoxide 0.9.1 — https://github.com/mattsse/chromiumoxide
[2] headless_chrome 1.0.21 — https://github.com/Edu4rdSHL/rust-headless-chrome
[3] wry 0.55.1 — https://github.com/tauri-apps/wry
[4] openh264-rs 0.9.3 — https://github.com/ralfbiedert/openh264-rs
[5] cisco/openh264 2.6.0 — https://github.com/cisco/openh264/releases
[6] openh264 BINARY_LICENSE — http://www.openh264.org/BINARY_LICENSE.txt
[7] mp4-rust 0.14.0 — https://github.com/alfg/mp4-rust
[8] ebur128 0.1.10 — https://github.com/sdroege/ebur128
[9] cosmic-text 0.19.0 — https://github.com/pop-os/cosmic-text
[10] symphonia 0.6.0 — https://github.com/pdeljanov/Symphonia
[11] audiopus 0.2.0 — https://crates.io/crates/audiopus
[12] ffmpeg-next — https://crates.io/crates/ffmpeg-next
[13] fonttools pyftsubset — https://fonttools.readthedocs.io/en/latest/subset/
[14] puppeteer/puppeteer Dockerfile — https://github.com/puppeteer/puppeteer/blob/main/docker/Dockerfile
[15] Zenika/alpine-chrome — https://github.com/Zenika/alpine-chrome
[16] jfrazelle/dotfiles chrome.json — https://github.com/jfrazelle/dotfiles/blob/master/etc/docker/seccomp/chrome.json
[17] Chromium Linux sandboxing doc — https://chromium.googlesource.com/chromium/src/+/master/docs/linux/sandboxing.md
[18] Docker resource constraints — https://docs.docker.com/engine/containers/resource_constraints/
[19] Docker Compose spec — https://docs.docker.com/reference/compose-file/services/
[20] krallin/tini — https://github.com/krallin/tini
[21] OpenTelemetry Rust — https://github.com/open-telemetry/opentelemetry-rust
[22] microsoft/edge-tts — https://github.com/microsoft/edge-tts
[23] Initsnow/edge-tts-rust — https://github.com/Initsnow/edge-tts-rust
[24] MasterXD123/mini-edge-tts-rust — https://github.com/MasterXD123/mini-edge-tts-rust
[25] yynag/edge-tts-rust — https://github.com/yynag/edge-tts-rust
[26] rust_xlsxwriter — https://docs.rs/rust_xlsxwriter
[27] pptx-rs — https://crates.io/crates/pptx-rs
[28] windows-rs / windows-sys — https://github.com/microsoft/windows-rs
[29] MEMORY forge-engine-prd — 仓库内
[30] MEMORY forge-crossplatform-foundation — 仓库内
[31] MEMORY security-hardening-pass-3 — 仓库内
[32] MEMORY docker-render-image-size-audit — 仓库内
[33] MEMORY deck-export-overlap-noanim-bug — 仓库内
[34] src-tauri/src/forge_pptx.rs — 仓库内
[35] src-tauri/src/forge_tts.rs — 仓库内
[36] src-tauri/src/forge.rs — 仓库内

---

## Limitations & Trade-offs

- **本报告数字多为经验值**：openh264 各平台 FPS、chromiumoxide 提速倍数、opus 编码耗时都缺独立基准 → P5 forge-bench 必做（G1/G4/G11）
- **mac capture 路径未实测**：apple silicon 上 chrome-headless-shell native vs Rosetta 性能无公开数据（G7 跟踪）
- **自研件 vs 调库的边界靠经验**：mp4-rust 2.7 年未更新 → 自写 muxer，但**没有第三方 benchmark** 证明自写比 mp4-rust fork 更好（G10）
- **edge-tts Rust 端口全 0 star** 是本轮推翻架构 v2 L1 的关键证据，但 Rust 生态变化快，下轮需重新评估
- **OpenTelemetry 在 Tauri/WebView2 路径的兼容未验证**（G9）→ P2
- **群晖 Container Manager 是否支持 userns-remap 未验证**（G12）→ P5 真机测
- **本报告是路线图 + 选型表 + 清单**，**不是已落地的工程**；落地需 4 阶段共 6-8 周（与 [[forge-engine-prd]] P0-P5 多周路线一致）
