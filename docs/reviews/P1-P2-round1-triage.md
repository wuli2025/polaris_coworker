# P1 / P2 第 1 轮 Pi 审查意见处置（Claude）

审查者：Pi 0.78 + minimax/MiniMax-M2.7（packet 模式）。原始报告：`docs/reviews/P1-round1/`、`docs/reviews/P2-round1/`。

## P1 agentd

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 严重 | `queue()` 的 catch 静默吞错 | **有证据驳回** | 实际代码 `registry.ts:143-149` 只有 `try/finally`，没有 catch；错误原样抛给调用方并以 error 响应返回。Pi 引用的代码片段不存在。 |
| 2 中 | Redactor 可被绕过 | **部分采纳（低）** | Unicode 规范化场景是推测（Key 为 ASCII）。但真实缺口：只匹配原文，Key 若以 URL 编码、base64（如 Basic 认证头）、JSON 转义形式出现会漏网。修：登记 Key 时同时登记这些编码变体；加测试。 |
| 3 低 | `opening` 集合不对称 | **有证据驳回** | `opening` 只用于"同一 sessionId 并发恢复"去重；无 id 的 open 生成随机新 id，不可能重复。设计正确。 |

## P2 Rust 后端

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 中 | `active` 表泄漏 | **部分采纳（低，加固）** | 正常路径 `supervisor.rs:141` 在 retain 中已移除，Pi 的"drain 跳过清理"不成立。但 supervisor 线程整体退出（熔断等）时 Route 被整体丢弃，`active` 残留会让 `AgentdConfigBusy`（`mod.rs:38`）永久成立。修：`Route::drop` 同时移除 `active`；supervisor 退出时清空 `active`；加测试。 |
| 2 低 | abort 超时掩盖原始错误 | **采纳** | `routes.rs` `fail()` 在已有终态时用新消息覆盖 error。修：保留首个错误原因，后续原因只进日志。 |
| 3 低 | reasoning 被丢弃 | **采纳为文档** | 现有前端契约没有思考事件，迁移期按设计不显示。加注释说明；"只增不改"的前端事件扩展另列任务。 |
| 4 低 | 写入背压被报成请求超时 | **采纳** | 背压与超时是不同故障，诊断时要区分。修：独立错误码贯穿到 `agent_backend_status.lastError` 与日志。 |
| 5 低（已知） | Rust 侧固定 600s 总超时与 agentd 空闲超时冲突 | **采纳（提为中）** | 长任务会在 10 分钟被切断，抵消 P1-F2。修：Rust 侧改为"无事件空闲超时"（默认略大于 agentd 的 `turnIdleTimeoutMs`，作为 agentd 失联兜底）+ 可选硬上限（默认 0 不限）；测试持续有事件的长轮次不被误杀。 |
