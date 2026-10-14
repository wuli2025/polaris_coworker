# 复审 brief：P4 第 1 轮修复（第 2 轮审查）

你上一轮的意见与主控处置见 `P4-round1-triage.md`（已附），本轮只审修复 diff `R5-diff.patch`（测试文件已排除）。驳回项附有代码证据，如仍认为驳回错误，请给出具体文件与行号。

## Claude 实测（沙箱外）
agentd typecheck 通过、`npm test` 两次 115/115（含 SIGKILL 持锁子进程后被另一进程回收的测试）；Rust 两壳 `cargo check` 通过（7 警告同基线）；`cargo test -p polaris-kernel --lib --tests` 连续 3 次 184 通过、4 失败均为基线，集成测试含 Node↔Rust 跨语言锁互通测试全部通过；vue-tsc 通过、vitest 25/25；改动文件 ≤500 行；提供商命令已为 `tauri::command(async)`。

## 请重点审查
锁回收的正确性：PID 复用、跨主机共享路径、时钟回拨、墓碑重命名在 Windows 上的语义、回收与正常释放的竞态（持有者正要删除锁时被回收）、持有者信息文件写入一半就崩溃；Node 与 Rust 两侧判定是否完全一致；异步提供商命令的代次号是否保证"最后一次选择生效"；`ping`/状态中新增计数是否泄露敏感信息。
