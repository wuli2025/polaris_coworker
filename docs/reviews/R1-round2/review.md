

## 审查报告

### 审查模式
**packet（无工具）** — 仅依据内联编号的显式材料评审，未使用任何读写或执行工具。

### 结论
**PASS** — 所审范围内的修复逻辑和测试覆盖未发现阻塞问题。

---

### 范围与缺失材料

| 项目 | 状态 |
|------|------|
| 审查范围 | R1-diff.patch 内 9 个文件；brief、triage 作为上下文 |
| 已有证据 | Claude 实测（沙箱外）：`cargo test` 145 通过；`npm test` 46/46；行数合规 |
| 缺失材料 | 未提供完整源码文件，仅有 diff；无法验证 diff 以外代码的污染或隐藏依赖 |

---

### 各修复项审查

#### P1-2：Redactor 编码变体
**证据**：credentials.ts:8-16  
```typescript
for (const variant of [secret, encodeURIComponent(secret),
  Buffer.from(secret, "utf8").toString("base64"), JSON.stringify(secret).slice(1, -1)]) {
  this.secrets.add(variant);
}
```
✅ **通过** — 注册了原文、URL 编码、base64、JSON 转义 4 种变体；时序调整（runtime.ts:29-32）确保在 `Models.build()` 前注册，防止竞态。

---

#### P2-1：`active` 表泄漏
**证据**：
- routes.rs:341-343：`Route::drop` 移除自身 `active` 条目
- supervisor.rs:478-480：`impl Drop for State` 清空整个 `active`
- supervisor.rs:439-445：插入 Route 时同时登记 `active`

✅ **通过** — 双层防护：单个 Route drop 清理 + supervisor 线程退出时全局清理。测试 `supervisor_exit_clears_orphan_active_entries`（tests.rs:579-592）和 `config_apply_is_not_busy_after_circuit_or_thread_exit`（tests.rs:535-559）覆盖。

---

#### P2-2：保留首个错误原因
**证据**：routes.rs:316-326  
```rust
let error = if self.first_error.is_some() {
    eprintln!(...);  // 仅记日志
    None
} else {
    self.first_error = Some(message.into());
    Some(...)
};
```
✅ **通过** — 首个 `fail()` 设置 `first_error` 并写入 terminal；后续 `fail()` 只打日志。测试 `first_failure_survives_abort_timeout`（routes.rs:371-387）覆盖。

---

#### P2-4：背压独立错误码
**证据**：
- process.rs:169：`pub const WRITER_BACKPRESSURE: &str = "AgentdWriterBackpressure"`
- process.rs:214-221：`try_send` 失败时区分 `Full` vs `Closed`，返回不同错误字符串
- process.rs:223：`write_error()` 对外暴露写入状态
- supervisor.rs:455：`deadlines()` 检查 `process.write_error()` 并将背压转为终态错误

✅ **通过** — 写入背压有独立错误码，与请求超时完全解耦。测试 `stalled_stdin_reports_backpressure_to_caller_and_status`（tests.rs:562-576）覆盖。

---

#### P2-5：Rust 侧超时改为空闲超时
**证据**：
- config.rs:69-73：新增 `turn_idle_timeout_ms`（默认 240s）和 `turn_max_ms`（默认 0）
- config.rs:91-95：`reject_turn_timeout` 反序列化守卫拒绝旧字段
- config.rs:104-105：`ensure` 改为检查 `turn_idle_timeout_ms > 0`
- routes.rs:274-277：Route 存储 `registered_at`、`last_event_at`、`idle_timeout`、`max_duration`
- routes.rs:297-306：`timeout_error()` 区分 idle 和 max 超时
- supervisor.rs:443-444：`Route` 构造时使用 `turn_idle_timeout_ms` 和请求 timeout（作为 max）
- supervisor.rs:464-467：deadline 检查使用新 `timeout_error()`

✅ **通过** — idle timeout 随每次 `accept()` 事件重置（routes.rs:288），hidden 事件（Reasoning/Usage）也计入（routes.rs:289 `let fatal = ...` 确保 fatal 事件后才记录 error）。测试覆盖：
- `idle_turn_times_out_even_while_ping_responses_arrive`（tests.rs:655-662）：ping 存活但 idle 超时
- `hidden_agentd_events_keep_long_turn_alive`（tests.rs:647-653）：reasoning/usage 保持 turn 活跃
- `hard_limit_stops_active_turn_and_caps_request_override`（tests.rs:664-677）：硬上限生效且限制请求级 timeout
- `explicit_request_limit_remains_a_hard_limit`（tests.rs:679-686）：显式请求 timeout 仍是硬限制

---

### 需关注点（非阻塞）

1. **bridge.rs:55 `legacy` 条件移除**
   - 旧代码：`Err(...) if legacy && matches!(...) => continue`
   - 新代码：`Err(...) if matches!(...) => continue`
   - **含义**：非 legacy 模式下超时不继续而直接返回错误。新行为与注释一致（supervisor 接管 deadline），但属行为变更。若 legacy 路径有未测到的边缘情况可能受影响。

2. **tests.rs:525 `turn_and_ping_timeouts_are_bounded` 被删除**
   - 原测试验证 "turn timeout 产生包含 Timeout 的 error"
   - 新测试只覆盖 ping timeout；turn timeout 逻辑已重构为 idle/max 超时，不再单独测旧逻辑
   - **风险**：若有人恢复使用旧字段 `turnTimeoutMs`（现已拒绝），无回归保护
   - **缓解**：`reject_turn_timeout` 确保旧字段必定报错

---

### 建议验证步骤（推断风险，非已有证据）

| 风险 | 验证方式 |
|------|----------|
| bridge.rs legacy 条件移除影响非 legacy 路径的边缘行为 | 在非 legacy 配置下注入超时事件，确认 supervisor 接管而非立即返回 |
| `reject_turn_timeout` 反序列化路径未覆盖序列化方向 | 确认 `serde(serialize_with)` 正确跳过旧字段；检查配置热更新路径 |
| `Client::wait` 的 2x 超时放大与 agentd 端 `turnIdleTimeoutMs`（180s）叠加可能拖长故障检测 | 在 agentd 无响应但 supervisor 存活场景下测量端到端错误上报延迟 |

---

### 总结
5 项修复的代码逻辑均与 triage 处置意见一致，未引入明显的假进度或假成功。测试覆盖了新 idle/max 超时模型、背压错误码、active 清理和首个错误保留。已知变更点（legacy 条件移除、turn timeout 测试删除）有合理理由但建议在集成测试中确认。
