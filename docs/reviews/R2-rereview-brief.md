# 复审 brief：P2.5 组第 1 轮修复（第 2 轮审查）

上一轮你的意见与主控处置见 `P25-round1-triage.md`（已附），本轮只审修复 diff `R2-diff.patch`。驳回项附有代码证据；如仍认为驳回错误，请给出具体文件与行号。

## Claude 实测（沙箱外）
agentd typecheck 通过、`npm test` 57/57（含子进程端到端）；Rust 两壳 `cargo check` 通过、7 警告同基线；`cargo test -p polaris-kernel --lib --tests` 166 通过，4 失败均为基线 Windows 路径测试；集成测试 4 通过；改动文件全部 ≤500 行。

## 修复要点
- #3：`State::drop` 以 `AgentdSupervisorExited` 回复 deferred/pending；`client.rs` 区分通道关闭与真正超时。
- #4：`session.configure` 先全量预检再应用，失败回滚模型/思考档位/工具，`toolProfile` 不变。
- #2：feature-tree 附件节点记录 TOCTOU 已知限制与威胁模型。
