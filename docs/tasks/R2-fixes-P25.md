# 任务 R2：R1c/P2.5/P2.5b 审查意见修复

先读 `docs/reviews/P25-round1-triage.md`，只修"采纳"项（#3、#4），并按 #2 的处置在 `docs/feature-tree.md` 附件节点记录已知限制。驳回项不要改代码。

- #3：`pi/supervisor.rs` 的 `State::drop` 显式以 `AgentdSupervisorExited` 回复 `deferred` 与 `pending` 中的全部请求；`pi/client.rs` 区分"回复通道关闭"（报 supervisor 退出）与"真正超时"（报 `AgentdRequestTimeout`）。测试：supervisor 退出时排队请求立即收到 `AgentdSupervisorExited`，不是超时。
- #4：`agentd/src/sessions/`：`session.configure` 先完整校验所有字段，再应用；应用中任一步失败则回滚已改字段到原值。测试：模拟中途失败（例如工具档位应用失败），会话的模型、思考档位、工具、`toolProfile` 全部保持原值；成功路径不变。

允许修改：`src-tauri/crates/polaris-kernel/src/chat/backend/pi/`、`agentd/src/sessions/`、对应测试、`docs/`。无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect。
必须通过：agentd `npm run typecheck && npm test`；两壳 `cargo check --offline`；`cargo test -p polaris-kernel --lib --tests --offline`（基线与沙箱 TCP 项除外并列出）。不 commit。
