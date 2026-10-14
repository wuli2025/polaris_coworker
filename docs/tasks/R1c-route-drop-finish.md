# 任务 R1c：Route 被丢弃时必须收尾（Claude 自检发现）

问题：R1b 之后 `backend/bridge.rs` 在 `recv_timeout` 超时时一律 `continue`，由 supervisor 负责收尾。但 `pi/routes.rs` 的 `impl Drop for Route` 只清理 reservation 与 active，**不结束 mailbox**。supervisor 任务异常退出（panic、通道关闭）时 `State` 连同所有 Route 被丢弃，前端对应的轮次永远等不到 `done`，界面一直"思考中"。

要求：
- `Route::drop`：若 `stream` 尚未 finished，以具名错误（例如 `AgentdSupervisorExited`）调用 `finish(error, done)`，保证前端收到 error+done。
- 检查 claude-cli（legacy）路径在同类异常下是否也一定收尾；若否，同样补上，并说明。
- 测试：模拟 supervisor 任务在有在途轮次时退出（含 panic 场景，可用测试专用钩子），断言调用方在有限时间内收到 error 再 done，`agent_backend_status` 反映故障。
- 允许修改：`src-tauri/crates/polaris-kernel/src/chat/backend/`（含子目录）与 `docs/`。无新依赖；≤500 行/≤500 字符；新代码无 unwrap/expect。
- 必须通过：两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`（基线 4 项与沙箱 TCP 项除外并列出）。不 commit。
