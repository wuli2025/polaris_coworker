## 独立审查结果

**审查模式**：packet（无工具）

**材料范围**：R8-rereview-brief、R8-all.patch（含测试）、P5cd-round1-triage.md

---

### 结论：CHANGES_REQUIREED

---

### 范围与缺失材料

**已有材料**：
- `R8-all.patch`（含后端专家同步、前端切换确认、用量价格测试）
- `P5cd-round1-triage.md`（前轮处置记录）
- brief 确认三种构建 0 警告、vitest 65/65

**明确缺失**：
- `experts_sync.rs` 源文件（`experts_recovery_tests.rs` 路径目标）——仅有 patch 中的模块声明和引用，未见实际实现
- `experts_tests.rs` 实际内容（patch 仅有 `#[path = "experts_tests.rs"] mod tests;`）
- brief 提到的 P6a-2 回归修复 patch 未单独提供
- 前端 `useContextFileEditor.ts` 实现未在 patch 中

---

### 问题

#### 1. 【中】`BackendStatus::default()` 与 `BackendStatus::new("pi")` 状态不一致

**文件**：`src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs:9-10`

```rust
#[derive(Debug, Clone, Default, Serialize)]  // 第 10 行 Default 加在 patch 中
pub struct BackendStatus {
    pub experts_synced: Option<bool>,         // None / Some(false) / Some(true)
```

**证据**：
- `BackendStatus::new("pi")`（`:29`）显式设 `experts_synced: Some(false)`（明确"未同步"）
- `BackendStatus::default()`（`:29`）隐式得 `experts_synced: None`（语义暧昧）

**可复现影响**：`expert_state`（experts.rs:128）用 `experts_synced == Some(true)` 判断允许操作：
```rust
anyhow::ensure!(status.experts_synced == Some(true), "ExpertConfigUnsynced: ...")
```
`None` 不等于 `Some(true)`，行为等价于未同步，操作被拒绝。这是**隐式语义混淆**：代码中有三态（`None`/`Some(false)`/`Some(true)`），但调用方只处理两态。

**最小修复建议**：若 `Default` 仅用于测试填充，应仅在 `#[cfg(test)]` 路径派生；生产代码统一用 `BackendStatus::new()` 或显式构造 `BackendStatus { experts_synced: Some(false), .. }`。

---

#### 2. 【低】`expert_state` 回滚路径设置 `experts_synced = Some(false)` 逻辑存疑

**文件**：`src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts.rs:136-138`

```rust
if let Err(rollback) = self.apply_experts(&before) {
    let client = self.client()?;
    let mut status = client.status.lock();
    status.experts_synced = Some(false);  // ← 这里
    status.expert_sync_error = Some(format!("ExpertConfigRollbackFailed: {rollback}"));
```

**分析**：持久化失败意味着磁盘写入未生效，daemon 端配置未变，`experts_synced` 不应降级。将状态改 `Some(false)` 使后续 `expert_state` 被阻止，但 `apply_experts` 本身就可能重试失败，修复方向存疑。

**最小修复建议**：将 `experts_synced = Some(false)` 改为仅记录 `expert_sync_error`；或移除此降级，让调用方决定重试策略。

---

#### 3. 【低】prompt 字节长度校验可能误伤多字节 UTF-8 内容

**文件**：`src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts.rs:62-66`

```rust
pub(super) fn normalized(mut self, limit: usize) -> anyhow::Result<Self> {
    anyhow::ensure!(self.prompt.len() <= limit,   // .len() = 字节数
        "ExpertPromptTooLong: {} bytes exceeds limit {limit}", self.prompt.len());
    self.prompt.truncate(self.prompt.trim_end().len());  // 字符数截断
```

**分析**：`.len()` 是 UTF-8 字节数，`trim_end().len()` 是字符数（前者 ≥ 后者）。对全 ASCII prompt 无问题；对含中文字符的 prompt，字节数可能 3 倍于字符数，可能导致 30 KiB 中文字符 prompt 在 32 KiB 字节限制下被错误拒绝。

**最小修复建议**：注释明确说明限制以字节计；或考虑用 `prompt.chars().count()` 统一用字符数。

---

### 简述无问题项

| 项 | 审查结论 |
|---|---|
| 前端切换项目未保存确认 | `switchProject` hook 已外置，`.vue` 中有 `changeProject` 拦截确认；实测覆盖 |
| 专家 prompt 上限 32768 字节 + 尾部空白规范化 | `normalized()` 实现完整，环境变量可配置 |
| agentd 不可达时专家同步不阻塞后端就绪 | `PiBackend::new` 启动监督但不等待 I/O（`:224`），`BackendStatus` 有 `expertsSynced`/`expertSyncError` |
| 未同步期间 `expert_state` 拒绝操作 | `experts_synced == Some(true)` 检查存在 |
| 恢复重试与用户设置竞态 | `expert_state` 全程持 `expert_lock`，`restore_experts` 在 `Control` channel 串行处理中，不并发 |
| `restore_experts` 在 daemon 恢复时延迟执行 | `pending.reply.is_none()` 条件确保首条 ping 后执行 |
| `BackendStatus` 旁路场景（`polaris-wiki`）| `..BackendStatus::default()` 仅在测试 `Fake` 中，不涉及生产路径 |
| `has_price` 语义与边界测试 | `usage_costs.rs` 新增 `model_price_completeness_is_window_local_and_independent_of_request_order` 覆盖混合请求场景 |

---

### 建议验证步骤

> **区分推断风险与已有证据**

**已有证据支撑**：
1. `experts_sync.rs` 模块声明存在但实现未在 patch 中——需补充源文件确认 `Restore` 结构语义正确
2. `BackendStatus::default()` 的三态不一致——可 grep 全量源码确认无生产路径误用
3. prompt 字节 vs 字符——可构造 UTF-8 多字节 prompt 的 unit test 验证行为

**推断风险**（无直接证据）：
- `experts_sync.rs` 恢复逻辑是否在 daemon 长时间不可达时无限重试或内存积累——需审查 `Restore` 实现
- `experts_recovery_tests::idle_daemon_reopen_restores_experts_before_releasing_requests` 偶发并行失败是否已在本次修复范围——brief 称已另派任务，但 patch 中未见该测试内容
