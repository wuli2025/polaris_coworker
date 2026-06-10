---
task_id: plan
created: 2026-06-10
mode: lead + 4 subagents
output_type: Full report (工程决策 + 路线图)
stakes: high
freshness: 必须 2025-2026 生产证据
geography: 全球，但 mac/Win 落地在国内桌面，Docker 镜像主供 Linux
---

# Research Plan — Polaris Forge 自研工业级化

## 用户原话
> 你把能自研的都自研一下，要求工业级的稳定的，分 winmac 和 docker 版本，docker 版本一定要做好，稳定

## 受众
工程团队（单人）+ 决策者（自己拍板）。产物 = 路线图 + 选型表 + Docker 工业级清单，能直接落到 src-tauri/crates/。

## 关键边界（来自桌面 HTML 已读 + memory）
- 浏览器内核 **永远不自研**（Chromium 几千万行）；CDP 客户端 = chromiumoxide 替「每帧起 chromium 进程」
- ffmpeg **可被替**（forge-codec openh264+自写 mp4 muxer+ebur128）
- forge_pptx / forge_tts / forge_fx **已自研落地**，本轮做"工业级化"而非重做
- Docker 是三平台里**唯一没系统 WebView 的**，必须自带浏览器
- 目标尺寸：渲染层降到 ~235MB（headless-shell + 静态 ffmpeg + SC 字体子集），物理下限 ~155MB（去掉 ffmpeg 后）

## 研究子线（4 个，2 串 2 并）
- A · capture：chromiumoxide vs 当前 CLI 进程；WebView2/WKWebView 桌面端兜底；chrome-headless-shell 瘦形态
- B · codec：openh264-rs / 自写 mp4 muxer / ebur128 / cosmic-text / rav1e 备路
- C · 已自研件：forge_pptx（隐形文本层）、forge_tts（edge-tts 免费神经层兜底）、polari-fx（动画可观测性）
- D · Docker 工业级：进程拉起、资源限制、shm/no-sandbox、字体兜底、healthcheck、可观测性、降级链

## 编排
- lead = 主对话（我）
- 4 subagent 各自独立 workspace/research-notes/task-*.md，最后由 lead 合并成 draft.md
- evaluator 价值不如直接动手；用 quality-gates 5 步自评

## 评估标准
- 每个选型 = Rust crate + 至少 1 条生产环境证据
- Docker 措施 = 直接对应一个工程动作（cmd、yaml、env、telemetry）
- 路线图 = 4 阶段，每阶段带可验证产物（cargo test / 出片 / mp4+ppxt 端到端）
- 风险 = 自研件挂掉 → 退路是什么

## 何时停止
- 每个 subagent 拿到 5-8 条带出处的核心证据即停
- 重复搜索超过 50% → 收尾进 registry
