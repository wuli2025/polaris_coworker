# Source Registry — Polaris Forge 工业级化

> 截止 2026-06-10。所有来源来自 4 份 task notes（task-a/b/c/d），按 crates / GitHub / 官方文档 / 既有记忆 / 工具链 5 类分组。

## A. Rust Crate（核心选型）

[1] chromiumoxide 0.9.1 | https://github.com/mattsse/chromiumoxide | crates.io/api/v1/crates/chromiumoxide | OFFICIAL | Feb 2026 · 1.3k stars · 178 forks · 5 supported target · MIT/Apache-2.0
[2] headless_chrome 1.0.21 | https://github.com/Edu4rdSHL/rust-headless-chrome | crates.io | OFFICIAL | Feb 2026 · Lib.rs 329k monthly · MIT
[3] wry 0.55.1 | https://github.com/tauri-apps/wry | crates.io | OFFICIAL | May 2026 · Lib.rs 2.7M monthly · no headless mention
[4] openh264-rs 0.9.3 | https://github.com/ralfbiedert/openh264-rs | crates.io/api/v1/crates/openh264 | OFFICIAL | Mar 2026 · MSRV 1.85 · BSD-2-Clause · 404k 下载
[5] cisco/openh264 2.6.0 | https://github.com/cisco/openh264/releases | OFFICIAL | Feb 2025 · BINARY_LICENSE 独立
[6] openh264 BINARY_LICENSE | http://www.openh264.org/BINARY_LICENSE.txt | PRIMARY | 2024-01 · 商用分发约束
[7] mp4-rust 0.14.0 | https://github.com/alfg/mp4-rust | crates.io/api/v1/crates/mp4 | OFFICIAL | 2023-08-01 至今未更新 · MIT · 349 stars
[8] ebur128 0.1.10 | https://github.com/sdroege/ebur128 | crates.io | OFFICIAL | 2024-10-26 · MIT · EBU TECH 3341/3342 测试过
[9] cosmic-text 0.19.0 | https://github.com/pop-os/cosmic-text | crates.io | OFFICIAL | Apr 2026 · Apache-2.0/MIT · Pop!_OS 维护
[10] symphonia 0.6.0 | https://github.com/pdeljanov/Symphonia | crates.io | OFFICIAL | May 2026 · MPL-2.0 · 100% safe Rust · 3.3k stars
[11] audiopus 0.2.0 | https://crates.io/crates/audiopus | OFFICIAL | 2021-04 (5 年停滞) · BSD-3
[12] ffmpeg-next | https://crates.io/crates/ffmpeg-next | OFFICIAL | 2024 · MIT
[13] fonttools pyftsubset | https://fonttools.readthedocs.io/en/latest/subset/ | OFFICIAL | webfont 工业做法 · pyftsubset --unicodes --no-hinting --desubroutinize

## B. Chrome / Chromium 工具链

[14] puppeteer/puppeteer Dockerfile | https://github.com/puppeteer/puppeteer/blob/main/docker/Dockerfile | OFFICIAL | base + 字体清单 + useradd 模式
[15] Zenika/alpine-chrome | https://github.com/Zenika/alpine-chrome | OFFICIAL | 三方案 no-sandbox/SYS_ADMIN/seccomp 共识
[16] jfrazelle/dotfiles chrome.json | https://github.com/jfrazelle/dotfiles/blob/master/etc/docker/seccomp/chrome.json | OFFICIAL | chromium seccomp 事实标准
[17] Chromium Linux sandboxing doc | https://chromium.googlesource.com/chromium/src/+/master/docs/linux/sandboxing.md | OFFICIAL | 进程模型与子进程树清理

## C. 容器与运行时

[18] Docker run reference / resource constraints | https://docs.docker.com/engine/containers/resource_constraints/ | OFFICIAL | --shm-size / --memory / --cpuset-cpus 语义
[19] Docker Compose spec | https://docs.docker.com/reference/compose-file/services/ | OFFICIAL | shm_size / cap_drop / cap_add / healthcheck / depends_on.condition
[20] krallin/tini | https://github.com/krallin/tini | OFFICIAL | tini -g 杀进程组 + 1.13+ 内置 init
[21] OpenTelemetry Rust | https://github.com/open-telemetry/opentelemetry-rust | OFFICIAL | OTLP · 0.x GA

## D. TTS / 字幕 / 第三方 Python 工具

[22] microsoft/edge-tts | https://github.com/microsoft/edge-tts | OFFICIAL | 限流 403 行为
[23] Initsnow/edge-tts-rust | https://github.com/Initsnow/edge-tts-rust | OFFICIAL | 0 star · 2026-04
[24] MasterXD123/mini-edge-tts-rust | https://github.com/MasterXD123/mini-edge-tts-rust | OFFICIAL | 0 star · 2026-04
[25] yynag/edge-tts-rust | https://github.com/yynag/edge-tts-rust | OFFICIAL | 0 star · 2024-12
[26] rust_xlsxwriter | https://docs.rs/rust_xlsxwriter | OFFICIAL | 不写 .pptx，仅 .xlsx
[27] pptx-rs | https://crates.io/crates/pptx-rs | OFFICIAL | 2024 之后无大更新
[28] windows-rs / windows-sys | https://github.com/microsoft/windows-rs | OFFICIAL | 暴露 ISpVoice，Win10/11 自带 SAPI 5.4

## E. Polaris 既有记忆 / 代码（不需重外部证据）

[29] MEMORY forge-engine-prd | 仓库内 | ADR-002/003/011 + 六 crate 骨架
[30] MEMORY forge-crossplatform-foundation | 仓库内 | 跨平台 preflight + 17 单测 + slim/full 落地
[31] MEMORY security-hardening-pass-3 | 仓库内 | auto-token + CSP + secrets
[32] MEMORY docker-render-image-size-audit | 仓库内 | 994MB 体积分析
[33] MEMORY deck-export-overlap-noanim-bug | 仓库内 | no-anim 误伤
[34] src-tauri/src/forge_pptx.rs | 仓库内 | 隐形文本层 + 全幅图版式
[35] src-tauri/src/forge_tts.rs | 仓库内 | MiniMax T2A + Mac say
[36] src-tauri/src/forge.rs | 仓库内 | 跨平台 preflight `/api/status`

## F. 知识缺口（Gaps — 后续 P5 验证项）

G1. openh264-rs aarch64 **未单测**（仅编译通过）→ 必须落 forge-bench
G2. chrome-headless-shell 各平台精确二进制大小（Chrome for Testing JSON 不暴露）
G3. chromiumoxide MSRV 数字未抓到（真机 cargo +1.78 check）
G4. 多 Browser 横向并发的真实吞吐（无公开 benchmark）
G5. noto-cjk SC 子集 6763 字的实际命中率（需自有 corpus）
G6. Jessi Frazelle seccomp 与 docker 25+ 兼容性
G7. wry headless mode（0.56+ 跟踪）
G8. chromiumoxide 0.6 进程泄漏 commit 编号
G9. OpenTelemetry Rust 在 Tauri/WebView2 兼容
G10. mp4-rust vs 自写 muxer 写盘性能（无第三方 benchmark）
G11. 1080p openh264 在 aarch64/Apple Silicon 实际 FPS + 功耗
G12. 群晖 Container Manager userns-remap 支持
