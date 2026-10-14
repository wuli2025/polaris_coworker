# 审查 brief：R1c + P2.5 + P2.5b（协议扩展、入口接通、异常退出必收尾）

附件 `P25-diff.patch` 为非测试源码 diff（测试文件已排除以控制体积）。

## 目标与验收
- R1c：后端（Pi 与旧 claude-cli）任何异常退出（supervisor panic/退出、reader/job 异常、一次性 stdout 异常）都必须给前端发 error→done，且终态不重复。
- P2.5：agentd 协议 v2：`session.open` 逐会话 model/thinkingLevel/toolProfile（档位名→工具清单在 agentd 配置里映射）；空闲会话 `session.configure`（运行中 SESSION_BUSY）；`prompt.attachments` 图片（数量/大小/MIME 上限进配置）；`ping` 返回 version/piVersion/protocolVersion，Rust 握手版本不一致立即熔断（AgentdProtocolMismatch）。
- P2.5b：`ChatSendArgs` 新增可选 `image_paths`/`thinking_level`；图片路径**只允许**会话 uploads 目录内（规范化、拒绝 `..`/符号链接逃逸/目录外），校验后 base64 下发；旧前端不传仍可用；prompt 内路径保留以兼容旧后端。

## Claude 实测（沙箱外）
- agentd：typecheck 通过，`npm test` 52/52（含子进程端到端）。
- Rust：两壳 `cargo check` 通过，7 警告与基线相同；`cargo test -p polaris-kernel --lib --tests` 164 通过，4 失败均为基线 Windows 路径测试；集成测试 4 通过。
- 前端：`vue-tsc` 通过；`vitest run src/lib/chatImages.test.ts` 2/2。
- 行数：新增/改动文件 ≤500 行（`feishu.rs` 1157 行为既有超长，仅机械 +2 行；`src/tauri.ts`、`src/stores/chat.ts` 既有超长，各 +2 行）。

## 请重点审查
1. 路径校验能否被绕过（符号链接、TOCTOU：校验后到读取前文件被替换、大小写/Unicode、Windows 路径分隔与盘符、硬链接）。
2. 附件上限是否在**读取前**就能挡住超大文件（避免先读进内存再判断）。
3. 协议版本握手与熔断：是否所有路径（首次启动、重启后）都检查；不一致时在途请求是否都收尾。
4. `session.configure` 与正在进行的 prompt/abort/close 的竞态。
5. R1c：finish 在 panic 展开期间是否可能二次 panic 或死锁（锁顺序）；终态是否可能重复。
