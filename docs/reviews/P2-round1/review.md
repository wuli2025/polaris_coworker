## 审查报告

**审查模式**：packet（无工具）

**结论**：CHANGES_REQUIRED

---

### 范围与缺失材料

**已审查文件**：
- `mod.rs` (130行)
- `config.rs` (113行)
- `stream.rs` (99行)
- `bridge.rs` (67行)
- `pi/mod.rs` (131行)
- `pi/supervisor.rs` (243行)
- `pi/process.rs` (110行)
- `pi/client.rs` (62行)
- `pi/routes.rs` (82行)
- `pi/sessions.rs` (56行)
- `pi/translate.rs` (76行)
- `pi/protocol.rs` (99行)
- `pi/windows_job.rs` (25行)
- `fake-agentd.mjs` 测试固件

**已知问题**（主控已标注，本次独立核实）：
- turnTimeoutMs 与 agentd 空闲超时语义冲突 → 已确认存在（见问题 #6）

**测试证据**：依赖 brief 中声称的 134 通过 / 4 Windows 失败；packet 模式无法验证。

---

### 问题

#### 1. `active` HashMap 泄漏 — 严重性：中
**文件**：`pi/routes.rs`，第 80-81 行 + `pi/mod.rs` 第 119 行

**证据**：
```rust
// routes.rs Drop impl
impl Drop for Route {
    fn drop(&mut self) { self.reservation.0.lock().remove(&self.reservation.1); }
    // ^^^ 未清理 self.active 中的 req 映射
}
```
`drain()`（第 59-77 行）会在正常流程下清理 `active`，但：
- 若 `EventStream` 被丢弃（调用方放弃轮次），`stream.is_finished()` 为真 → `drain()` 跳过 active 清理
- `routes.rs` 的 `Drop` 未清理 `active`
- `close()` 只删会话，不删 active

**可复现影响**：长时间运行中 active 持续增长（内存泄漏 + O(n) 性能退化）。

**最小修复**：`routes.rs` 的 `Drop` 应调用 `self.active.lock().remove(&self.req)`，或在 `EventStream` Drop 时触发 cleanup。

---

#### 2. Abort 超时不覆盖原有 error — 严重性：低
**文件**：`pi/supervisor.rs`，第 147-149 行

**证据**：
```rust
if route.abort_deadline.is_some_and(|d| Instant::now() >= d) {
    route.aborted();
    anyhow::bail!("AgentdAbortTimeout: daemon restarted...");
}
```
若 `fail()` 已触发（daemon 返回 error），`aborted()` 的 no-op 保护不覆盖原有 error，但 `bail!` 仍向 supervisor 报告新错误，掩盖原始原因。

**最小修复**：abort 超时时检查 `route.terminal` 状态，或在 `fail()` 中记录原始错误代码。

---

#### 3. `Event::Reasoning` 丢弃且无缓冲 — 严重性：低
**文件**：`pi/translate.rs`，第 71 行

**证据**：
```rust
Event::Started | Event::Reasoning { .. } | Event::Usage { .. } => {}
```
`Event::Reasoning` 被静默丢弃。协议有 `reasoning.delta` 事件，但前端契约未定义接收方式（delta 还是其他 kind）。

**最小修复**：明确忽略（保持现状加注释）或将其缓冲为前端可消费的 kind。

---

#### 4. Process::write 错误语义模糊 — 严重性：低
**文件**：`pi/process.rs`，第 79 行

**证据**：
```rust
self.writer.try_send(bytes).map_err(|_| anyhow::anyhow!("AgentdWriterBackpressure"))
```
但 `try_send` 失败会被 supervisor 包装为 `AgentdRequestTimeout`（`supervisor.rs` 第 26 行），用户看到"请求超时"而非"内部背压"。

**最小修复**：使用独立错误码 `"AgentdWriterBackpressure"`，或在 `request()` 层区分。

---

#### 5. turnTimeoutMs 默认 600s 语义冲突 — 严重性：低（已知）
**文件**：`config.rs`，第 65 行；`bridge.rs`，第 41 行

**证据**：Rust 侧固定 600s 总时长超时，但 agentd 可能有独立的 idle 超时（`idle_ms`）和硬上限。两者同时生效可能导致意料之外的提前终止。

**最小修复**：提供配置选项让 `turn_timeout_ms` 可选（零值表示不设置 Rust 侧超时），或明确文档化语义。

---

### 正面发现

1. **配置校验完整**：`config.rs` 验证 backend 类型、isolation 值、所有正数配置项，有单元测试
2. **事件回压安全**：`stream.rs` 的 terminal 槽位保留机制确保 error/done 不因队列满而丢失
3. **取消路径正确**：`PiBackend::cancel()` 标记 cancelled，线程内检查 → 保证 `done` 事件
4. **Windows Job 正确**：使用 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` + `AssignProcessToJobObject`
5. **无 unwrap/expect 新增**：所有错误使用 `anyhow::bail!` 或具名错误
6. **进程清理**：Linux PDEATHSIG + Unix killpg + Drop 清理
7. **熔断机制**：指数退避 + jitter + `failure_threshold` 正确实现

---

### 建议验证步骤

| 风险 | 推断/已有证据 | 验证方式 |
|------|-------------|---------|
| active 泄漏 | 推断 | 长时间运行观察内存，或增加活跃会话数量压测 |
| abort 超时覆盖 | 推断 | 模拟 agentd error 后触发 abort timeout |
| 背压错误码 | 推断 | 快速发送大量请求触发 writer 背压 |
| turnTimeoutMs 冲突 | 已知 | 配置短 idle_ms 观察行为 |

---

**CHANGES_REQUIRED** 原因：`active` HashMap 泄漏是明确的功能缺陷，需修复后方可通过审查。
