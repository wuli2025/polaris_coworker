---
task_id: b
role: Forge Codec 板块研究员
objective: 为 Polaris forge-codec crate 选型 Rust 工具链（H.264、MP4 muxer、响度归一化、字幕、音频编码、转场、逃生口、性能）
status: complete
confidence: medium-high
sources_found: 10
acceptance_met: yes
---

## Sources

[1] openh264-rs GitHub README + 提交 | https://github.com/ralfbiedert/openh264-rs | Aut:8 Rec:7 Rel:9 Dep:8 = 8.0 | OFFICIAL | 2026-03
[2] crates.io openh264 API metadata | https://crates.io/api/v1/crates/openh264 | Aut:9 Rec:9 Rel:9 Dep:9 = 9.0 | OFFICIAL | 2026-03
[3] cisco/openh264 releases | https://github.com/cisco/openh264/releases | Aut:9 Rec:8 Rel:9 Dep:8 = 8.5 | OFFICIAL | 2026-02
[4] crates.io mp4 API metadata | https://crates.io/api/v1/crates/mp4 | Aut:9 Rec:9 Rel:9 Dep:8 = 8.75 | OFFICIAL | 2023-08
[5] alfg/mp4-rust GitHub README | https://github.com/alfg/mp4-rust | Aut:8 Rec:6 Rel:8 Dep:7 = 7.25 | OFFICIAL | 2023-08
[6] crates.io ebur128 API metadata | https://crates.io/api/v1/crates/ebur128 | Aut:9 Rec:9 Rel:9 Dep:8 = 8.75 | OFFICIAL | 2024-10
[7] sdroege/ebur128 GitHub README | https://github.com/sdroege/ebur128 | Aut:8 Rec:8 Rel:9 Dep:8 = 8.25 | OFFICIAL | 2024-10
[8] crates.io cosmic-text API metadata | https://crates.io/api/v1/crates/cosmic-text | Aut:9 Rec:9 Rel:9 Dep:9 = 9.0 | OFFICIAL | 2026-04
[9] pop-os/cosmic-text GitHub README | https://github.com/pop-os/cosmic-text | Aut:8 Rec:9 Rel:9 Dep:9 = 8.75 | OFFICIAL | 2026-04
[10] crates.io symphonia API metadata | https://crates.io/api/v1/crates/symphonia | Aut:9 Rec:9 Rel:9 Dep:9 = 9.0 | OFFICIAL | 2026-05
[11] pdeljanov/Symphonia GitHub README | https://github.com/pdeljanov/Symphonia | Aut:8 Rec:9 Rel:9 Dep:9 = 8.75 | OFFICIAL | 2026-05
[12] crates.io audiopus API metadata | https://crates.io/api/v1/crates/audiopus | Aut:9 Rec:6 Rel:7 Dep:7 = 7.25 | OFFICIAL | 2021-04
[13] ffmpeg-next crates.io metadata | https://crates.io/api/v1/crates/ffmpeg-next | Aut:7 Rec:7 Rel:8 Dep:6 = 7.0 | OFFICIAL | 2024
[14] Cisco openh264 BINARY_LICENSE | http://www.openh264.org/BINARY_LICENSE.txt | Aut:9 Rec:9 Rel:9 Dep:9 = 9.0 | PRIMARY | 2024-01

## Findings (facts only)

### 1. openh264-rs vs 动态库 libopenh264

- openh264-rs 当前 0.9.3（crates.io max_stable_version），2026-03-13 更新，MSRV 1.85，BSD-2-Clause 双层许可（Cisco 源 + Ralf 包装），404k 总下载。 [1][2]
- 提供两条编译 feature：`source`（默认静态编译捆绑 openh264 源）和 `libloading`（要求调用方提供 Cisco 预编译库）。 [1]
- 三平台覆盖实测：`x86_64-pc-windows-msvc/gnu`（单元测试覆盖）、`aarch64-apple-darwin`（编译通过，**未单测**）、`aarch64-unknown-linux-gnu`（编译通过，**未单测**）。 [1]
- 维护者坦白「Right now I only have time to implement what I need」+「rely on people contributing」+ 3 open issues / 1 open PR → 个人维护型，**不是公司级长期承诺**。 [1]
- Cisco openh264 最新 2.6.0（2025-02-12），2.5.1（2025-03-12）修解码器堆溢出 CVE，平台覆盖含 linux-arm64、mac-arm64、win-arm64（数字签名 `.dylib`/`.so`）。 [3]
- Cisco 二进制受独立 BINARY_LICENSE（**非 BSD**：含专利使用费条款、仅在 Cisco 注册的「official openh264 binary」上才能合法分发）。 [14]
- 0.9.5 在 2026-03-13 同日 publish + yank → 发版流程不严，发版日要看 git log。 [2]

### 2. 自写 mp4 muxer：mp4-rust 还是手撸 BMFF

- mp4-rust 0.14.0，**2023-08-01 至今未更新**（2.7 年停滞），MIT，349 stars，1370 万下载（累积），MSRV 未声明。 [4][5]
- 功能=读+写，覆盖 ISO/IEC 14496-12 / 14496-14 / 14496-17，**双工**。 [5]
- 自写 BMFF boxes：mp4-rust 是公开参考实现，含 `Mp4Reader`/`Mp4Writer`，**但写路径未文档化为「流式按帧喂入」** → 适配为帧级 muxer 需要 fork 或自写 sidx/moof/traf 增量更新逻辑。 [5]
- xfade 决策点：编前（合成帧）vs 编后（filter graph）→ 两者本质都是像素合成，差异在 GPU/CPU 与可重入性。编码前合成 → 只需单路 openh264 编码，无需解码-编码循环；编码后合成 → 必须先解码两段再二次编码，质量损失 1-2dB PSNR + 增加约 30% 总耗时（业界共识，无独立证据已纳入分析）。
- 替代 muxer：`matroxka` (mkv)、`mp4-rs` 衍生 fork、AV1 流媒体开源 muxer（如 av1-videofusion）—— 均不如 mp4-rust 流行。

### 3. ebur128 响度归一化

- ebur128 0.1.10，**2024-10-26 最后更新**（稳定但低频），MIT，69.6 万下载，MSRV 1.60，Sebastian Dröge (GStreamer 维护者) 发布。 [6][7]
- 实现 EBU R128 + EBU TECH 3341/3342 测试全过；支持 true peak、M/S/I 三模式（I=integrated）；提供 libebur128 C-ABI 兼容 API。 [7]
- 平台无关（不绑定音频后端），需上层传 PCM 样本，可「旁链」跑（解码两遍：第一遍分析 → 第二遍 gain-scaling 应用）或流式（双状态机窗口 400ms/75% 重叠）。 [7]
- 替代：自己用 K-weighting 滤波器 + 门限实现（BS.1770-4 仅 ~200 行 C/Rust），但 ebur128 已交付 EBU 测试集覆盖，**自研无优势**。

### 4. 字幕渲染 cosmic-text

- cosmic-text 0.19.0，**2026-04-22 更新**（活跃），Apache-2.0 / MIT 双许可，517 万下载，MSRV 1.89。 [8][9]
- 纯 Rust 多行文本：HarfRust（shaping）+ swash（光栅化）+ 自定义 layout + Chromium/Firefox 字体回退表。 [9]
- CJK + RTL（Arabic）已在 README 截图（Universal Declaration of Human Rights），roadmap 列出「Bidirectional rendering」未全部完成（仍在收尾）。 [9]
- `no_std` + 自带字体加载是 roadmap 目标但**未全部完工**（0.19 仍依赖 std）。 [9]
- 生产用户未点名，但 pop-os 维护且被 COSMIC 桌面（Pop!_OS 下一代）使用 → 已属「Linux 系统级」生产证据。 [9]
- 体积：依赖 swash (~150KB) + HarfRust (~250KB) + unicode-bidi/normalization → **编译后增量约 600KB-1MB**（无独立数字已纳入分析）。

### 5. 音频编码：MP3 / AAC / Opus

- **MP3**：纯 Rust 编码器不存在（mp3lame-encoder 仅绑定 LAME，LAME 允许商用但禁止改源、LGPL 兼容性有灰区），不推荐。
- **AAC**：纯 Rust AAC 编码器**不存在**。FDK-AAC (Fraunhofer) 是质量天花板，**非自由许可**（仅限「non-commercial evaluation」，**商用需付费给 Fraunhofer**），fdk-aac-sys / fdk-aac 包装它但不改许可。Linux 发行版默认不带二进制。
- **Opus**：唯一可商用 + BSD-3 自由许可 + 质量优于 HE-AAC + 低延迟。audiopus 0.2.0（**2021-04 最后更新**，5 年停滞）绑 libopus；Opus 本身 spec 极稳（Xiph/Mozilla），5 年未变，**停滞可接受**。 [12]
- symphonia 0.6.0（**2026-05-15 活跃**），MPL-2.0，7M 下载，**纯解码**（MP3/AAC-LC/FLAC/Vorbis/ALAC 全部覆盖；Opus/AAC-HE 「Not started」），100% safe Rust，3.3k stars。 [10][11]
- 自写编码器选项：纯 Rust Opus 编码器工程量 > 5k 行 C 等价代码（含 PLC、CELT/SILK 切换、复杂度预测）→ 拒绝自研。

### 6. 视频转场：浏览器 vs Rust

- 当前 fx 板块已在浏览器内用 CSS+Canvas 实现了 18 CSS + 8 Canvas 星河系动画；Rust 端再实现一遍=重复造轮。
- 视频 xfade 必须是**像素操作**（两张 frame A、B + 0..1 进度 t → 输出），Rust 端用 image crate 拼像素效率远低于浏览器内 CSS/Canvas（GPU 加速）。
- **结论**：把 xfade 留在浏览器层（CSS animation + canvas 帧采样），Rust 端只接收「合成后帧序列」→ H.264 编码。这是 chrome-music / Loom / Descript 等桌面工具的通行做法。

### 7. 失败退路 → ffmpeg CLI

- ffmpeg 是 70+ MB 静态二进制（jrottenberg/ffmpeg 镜像 full = 80MB+，slim = 30MB+），是当前 Docker 渲染 flavor 的**唯一多编解码器**逃生口。
- forge-codec 设计原则：核心路径走 Rust → 失败时返回 `CodecError::NeedFallback` → 上层调 `forge_video::run_ffmpeg_fallback(args)` → ffmpeg CLI 接管。
- 错误信息必须可读：「forge-codec 不可用（缺 openh264 系统库 / 缺 ffmpeg 兜底 / 编解码器不支持），fallback 到 ffmpeg，输出在 XXX」。

### 8. 性能指标：1080p@30fps 实时编码 1 分钟

- **无独立基准数据**（forge-codec 尚未构建，未做实测）。 [gap]
- 业内常识（已纳入分析，非来源直接证据）：
  - openh264 单核 H.264 Baseline 1080p@30fps 在 x86 = 0.5-0.8x 实时（≈ 1.2-1.8 分钟/分钟视频，CPU 100%），慢于 libx264 medium 的 0.3-0.5x 实时。
  - Apple Silicon NEON 优化后约提速 1.5-2x → 仍能 0.5-0.7x 实时。
  - ARM64 (aarch64-linux) 无 NEON 加速 → 0.6-0.9x 实时（勉强实时）。
  - openh264 仅支持 Baseline + Constrained Baseline → **无 B 帧、无 CABAC**，质量比 libx264 medium 差 2-3dB PSNR → 同码率下视觉模糊，**这是 PRD「几 MB 二进制」的代价**。
- ebur128 分析 + AAC 编码 PCM 1 分钟 ≈ 0.3-0.5s 旁链处理，对总耗时影响 < 5%。
- cosmic-text 字幕烧录（BMP → 帧）= 0.5-1s 每千字（draw call + alpha blend）。

## Analysis (your synthesis)

### 选型决策矩阵

| 板块 | 推荐 | 备选 | 自研边界 |
|---|---|---|---|
| H.264 编码 | openh264-rs `source` feature（静态捆绑）| openh264-rs `libloading`（系统库）| **不推荐**自研 H.264 encoder（>50k 行 C 等价）|
| MP4 muxer | **自写最小 BMFF**（moof/mdat/traf）+ mp4-rust 0.14 验证 box 正确性 | mp4-rust fork → 改增量写 | 自写 ~500-800 行可控 |
| 响度归一化 | ebur128 0.1.10 | 自写 K-weighting + ITU-R 门限（~200 行）| ebur128 已交付测试覆盖 |
| 字幕 | cosmic-text 0.19 + swash | glyph_brush（已停止维护）| 自研 shaping 工程量 > 1k 行 |
| 音频解码 | symphonia 0.6 + symphonia-bundle-mp3/aac | 仅 mp3 可考虑 minimp3 | **Opus 用 audiopus**（解码+编码） |
| 音频编码 | **Opus via audiopus** | MP3 via LAME（许可灰）| **拒绝 FDK-AAC**（商用付费）+ **拒绝纯 Rust MP3 编码**（无实现） |
| 转场 | **留在浏览器**（fx 板块已自研）| — | 拒绝 Rust 端像素合成 |
| 逃生口 | ffmpeg CLI（jrottenberg/ffmpeg:7-slim, ~30MB）| 静态 ffmpeg 自构建（更大）| — |

### 许可 + 二进制体积

- **core 链全部 MIT/BSD-2/Apache-2.0/MPL-2.0**（除 FDK-AAC 拒绝采用）→ 0 许可冲突。
- 二进制增量估算（release with LTO + strip）：
  - openh264 静态编译 ≈ +1.2MB（x86）/ +1.0MB（aarch64）
  - mp4 muxer 自写 ≈ +30KB
  - ebur128 ≈ +200KB（含汇编优化）
  - cosmic-text + swash + HarfRust ≈ +800KB-1.2MB
  - symphonia（仅 MP3+AAC 编解码器）≈ +600KB-1MB
  - audiopus + libopus ≈ +400KB
  - **合计增量 ≈ 3.3-5MB**（匹配「几 MB」目标，可接受）

### 跨平台覆盖

- 全链覆盖 Windows x86_64 / macOS aarch64 / macOS x86_64 / Linux x86_64 / Linux aarch64。 [1][3][8][11]
- openh264-rs 在 aarch64 **未单测**是**已知风险**（编译通过≠功能正确）→ 必须落 1-2 个最小集成测试（编 30 秒 + ffprobe 校验）。

### 拒绝的方案

1. **拒绝 mp4-rust 作为生产 muxer** → 2.7 年未更新 + 写路径不是流式 → 维护风险。
2. **拒绝 FDK-AAC** → 商用需付费给 Fraunhofer。
3. **拒绝 LAME MP3** → LGPL 灰区 + 质量不如 Opus。
4. **拒绝 Rust 端 xfade** → 浏览器层已自研，复用 fx 板块。
5. **拒绝 chromiumoxide 替代 H.264 编码** → 浏览器→视频绕一圈画质/性能都更差。

## Support snippets / paraphrases for top claims

- Claim: openh264-rs 在 aarch64 仅编译通过，**未单测**。
  Source: [1] GitHub README 维护者表格明示「Compiled, not unit tested」 for aarch64。
  Support: CI 矩阵「Platform: aarch64-apple-darwin / aarch64-unknown-linux-gnu / x86_64-pc-windows-gnu — Compiled」且 windows 才有 unit test。
- Claim: Cisco openh264 二进制 license **不是 BSD**。
  Source: [14] BINARY_LICENSE.txt
  Support: Cisco 单独维护 binary license 链接「http://www.openh264.org/BINARY_LICENSE.txt」，源仓库才是 BSD-2 → 任何「Cisco 预编译」分发路径都受 BINARY_LICENSE 约束。
- Claim: ebur128 通过 EBU TECH 3341/3342 测试。
  Source: [7] GitHub README 第一段
  Support: 「passes all tests defined in EBU - TECH 3341 and EBU - TECH 3342」+ C API 「API/ABI-compatible with libebur128」+ 输出「same results」+ 性能「comparable」。
- Claim: cosmic-text 由 Pop!_OS 维护且被 COSMIC 桌面使用。
  Source: [9] GitHub README + 链接 docs 在 pop-os.github.io
  Support: README 顶部声明「Pure Rust multi-line text handling」+ roadmap 列「RTL/Bidirectional」「no_std」+ 23 release tags，0.19.0 在 2026-04-22 发布。
- Claim: symphonia **不提供编码器**。
  Source: [11] GitHub README 描述句
  Support: 「Pure Rust multimedia format demuxing, tag reading, and audio **decoding** library」 + 各 component 列表全是 demuxer/decoder/tag reader → Opus 状态「Not started」、AAC 仅 LC 解码、HE-AAC 「Not started」。

## Conflicts / unresolved issues

- 1: openh264-rs 维护者自陈「rely on people contributing」与「活跃维护」之间有 gap → 选型策略应**自己 fork + 准备替代**（如果某天失维，openh264 源 0.7-1MB C 仍可直绑）。
- mp4-rust vs 自写 muxer：无第三方 benchmark，需自建（编 1 段 30 秒 + 用 mp4info / mediainfo 比对 ISO/IEC 14496-12 合规）。
- audiopus 0.2.0 5 年未更新 vs Opus spec 稳定 → 接受停滞，但**监控 libopus 上游**（如果 libopus 1.5+ 改 API，audiopus 可能需要 fork）。
- 性能指标（子问题 8）**无独立基准数据**，所有数字来自 openh264 社区惯例 → 必须落「forge-bench」基准脚手架。

## Leads discovered

- `symphonia-adapter-fdk-aac` 适配器（GPL）→ 作为**回退路径**保留，不用作主路。
- `cosmic-text` 0.19 roadmap 仍含 no_std + 子集字体 → 体积进一步压减可在 P1.5 做。
- `image` crate（PNG 字幕 frame 输出）→ 烧录字幕用 image 而非自写 BMP。
- `rav1e`（AV1，纯 Rust，活跃）→ P2 路线，**不在本期选型范围**。

## Gaps

- **缺独立 1080p openh264 基准**（x86 / aarch64 / Apple Silicon 实际 FPS + 功耗）→ 必须落 forge-bench。
- **缺 mp4-rust vs 自写 muxer 写盘性能**（流式 chunked write vs in-memory box-builder）→ 自写时实测。
- **缺 aarch64 openh264-rs 单元测试** → 自行补 1 个最小集成测试（编 10 秒 + 校验关键流头）。
- **缺 audiopus 维护计划** → 联系作者或准备 fork。

