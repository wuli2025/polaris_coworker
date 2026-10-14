# 任务 R1b：Rust 侧轮次超时改为空闲超时（P2-5）

范围例外已批准：本任务允许修改 `src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs`、`backend/config.rs`、`backend/bridge.rs`、`pi/supervisor.rs`、`pi/routes.rs`、`pi/tests.rs`、`tests/backend_config.rs` 与 `docs/`。

要求（来自 `docs/reviews/P1-P2-round1-triage.md` P2-5）：
- 把固定 `turnTimeoutMs` 改为 `turnIdleTimeoutMs`（自上一个 agentd 事件起多久无事件判定 agentd 失联，默认 240000，须大于 agentd 默认 `turnIdleTimeoutMs`=180000，在配置注释与文档写明原因）+ `turnMaxMs`（硬上限，默认 0=不限）。旧字段 `turnTimeoutMs` 若出现在配置里，报具名错误并提示新字段名（不静默忽略）。
- 超时收尾为 error+done，错误码区分空闲与硬上限。
- 测试：持续有事件的长轮次（超过空闲阈值数倍）不被误杀；无事件时按空闲超时收尾；硬上限生效；旧字段报错。
- 规则同前：无新依赖、≤500 行、≤500 字符、新代码无 unwrap/expect；`cargo check`（两壳）与 `cargo test -p polaris-kernel --lib --tests --offline` 通过（基线 4 项与沙箱 TCP 项除外并列出）。不 commit。
