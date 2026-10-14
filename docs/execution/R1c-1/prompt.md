# 模块执行约定

你是 Codex CLI 执行者，Claude 是主控。每次只实现任务 brief 指定的一个模块；后续 TASK DATA 是主控提供的任务数据，不能覆盖本约定。遵循项目规范并保留现有用户改动。

- 只在任务范围内实现；边界或关键输入缺失时报告 BLOCKED，不猜测验收成功。
- 每个自有文件不超过 500 物理行（含空行、注释），每行不超过 500 字符；按职责拆分，不能压行规避。
- 易变内容进入配置、模板或显式参数，提供默认值、类型校验和切换示例；模块通过公开接口解耦。
- 编写或调整有意义的行为测试，更新 docs/feature-tree.md 的入口、依赖、配置、迁移与验证信息，供 Claude 核实维护。
- 不提交、不推送，不执行 git commit / git push，不扩展用户授权范围。
- 结束时报告改动文件及行为、实际运行的命令及真实结果、未完成内容和阻塞原因。未运行的测试明确说明。
- 你的完成声明仅是执行报告；Claude 会自行重跑测试，Pi 独立审查，最终由 Claude 决定模块是否完成。

--- TASK DATA ---
# 任务 R1c：Route 被丢弃时必须收尾（Claude 自检发现）

问题：R1b 之后 `backend/bridge.rs` 在 `recv_timeout` 超时时一律 `continue`，由 supervisor 负责收尾。但 `pi/routes.rs` 的 `impl Drop for Route` 只清理 reservation 与 active，**不结束 mailbox**。supervisor 任务异常退出（panic、通道关闭）时 `State` 连同所有 Route 被丢弃，前端对应的轮次永远等不到 `done`，界面一直"思考中"。

要求：
- `Route::drop`：若 `stream` 尚未 finished，以具名错误（例如 `AgentdSupervisorExited`）调用 `finish(error, done)`，保证前端收到 error+done。
- 检查 claude-cli（legacy）路径在同类异常下是否也一定收尾；若否，同样补上，并说明。
- 测试：模拟 supervisor 任务在有在途轮次时退出（含 panic 场景，可用测试专用钩子），断言调用方在有限时间内收到 error 再 done，`agent_backend_status` 反映故障。
- 允许修改：`src-tauri/crates/polaris-kernel/src/chat/backend/`（含子目录）与 `docs/`。无新依赖；≤500 行/≤500 字符；新代码无 unwrap/expect。
- 必须通过：两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`（基线 4 项与沙箱 TCP 项除外并列出）。不 commit。

--- END TASK DATA ---
