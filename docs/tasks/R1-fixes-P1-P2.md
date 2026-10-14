# 任务 R1：P1/P2 审查意见修复

先读 `docs/reviews/P1-P2-round1-triage.md`。只修其中标为"采纳/部分采纳"的条目，驳回项不要动。

| 条目 | 位置 | 修复要求 |
| --- | --- | --- |
| P1-2 | `agentd/src/models/credentials.ts`、`runtime.ts` | 登记 Key 时同时登记 URL 编码、base64（含 `user:key` 形式不强求，至少 Key 本身的 base64）、JSON 转义后的变体；测试每种变体在日志与事件中都被替换。 |
| P2-1 | `polaris-kernel/src/chat/backend/pi/routes.rs`、`supervisor.rs` | `Route::drop` 移除 `active` 对应项；supervisor 线程退出路径清空 `active`；测试：模拟熔断/线程退出后 `config.apply` 不再报 `AgentdConfigBusy`。 |
| P2-2 | `routes.rs` `fail()` | 已有终态时保留首个错误原因，后续原因只写日志；测试：先 daemon error 再 abort 超时，前端收到的是首个错误。 |
| P2-3 | `translate.rs` | 对丢弃 reasoning/usage 加注释，说明前端契约暂无对应事件、后续以可选字段扩展。 |
| P2-4 | `process.rs`、`supervisor.rs`、`client.rs` | 写入背压使用独立错误码，贯穿到 `agent_backend_status.lastError` 与日志，不再显示为请求超时；测试。 |
| P2-5 | `backend/config.rs`、`bridge.rs` | 把固定 `turnTimeoutMs` 改为 `turnIdleTimeoutMs`（无事件多久判 agentd 失联，默认 240000，须大于 agentd 默认 180000 并在文档写明原因）+ `turnMaxMs`（默认 0=不限）；测试：持续有事件的长轮次不被误杀、无事件时按空闲超时收尾为 error+done。 |

## 规则
- 只改上表文件及其测试、`docs/`。不新增依赖。每文件 ≤500 行、每行 ≤500 字符；新 Rust 代码无 unwrap/expect（测试除外）。
- 必须通过：`cd agentd && npm run typecheck && npm test`（沙箱内 e2e 失败可说明）；`cd src-tauri && cargo check -p polaris-app --offline && cargo check -p polaris-cli --bin polaris-server --features collab-net --offline && cargo test -p polaris-kernel --lib --tests --offline`（只允许 baseline 的 4 个已知失败；沙箱禁止 TCP 导致的失败请单独列出）。
- 不 commit。结束时输出：每条处理结果、改动文件与行数、命令结果摘要。
