

## 审查结果

**审查模式**：packet（无工具）

**结论**：**PASS**

---

### 范围与材料

| 材料 | 内容 |
|------|------|
| Brief | R9-rereview-brief.md：复审指令，重点关注缓存清除期间高风险操作被拒、冷静期删除路径、`BackendStatus::Default` 误用 |
| Triage | P6a-round1-triage.md：第 1 轮 Pi 审查处置，含 R8 收尾 |
| Diff | R9-diff.patch：含 grants.rs 冷静期删除、contract.rs in_cooldown 置 false、revocations.rs 缓存验签失败清除、BackendStatus Default 限定测试、feature-tree.md 更新 |

> 注意：diff 正文已排除 20 个纯搬移文件（H/http.rs 路由拆分），无法验证生产路由完整性。

---

### 问题：无

#### 1. 缓存验签失败 → 清除 + 拒高风险（revocations.rs:761-774）

- `cached()` 在验签失败时执行 `UPDATE ... SET document='' WHERE uid=?1 AND document=?2`
- 条件 `document=?2` 防止并发心跳将成功结果冲掉（若期间有新文档写入则 UPDATE 0 行，旧有效文档保留）
- `require_fresh()`（:748-752）查询已改为 `document<>''`，清除后该查询返回 `None`，`is_some_and` 失败，高风险操作被拒
- 测试 `corrupt_cache_denies_high_risk_until_next_security_heartbeat` 覆盖：损坏缓存→高风险被拒→`require_fresh` 失败→已知吊销仍可用→离线心跳失败→仍被拒→下一心跳成功恢复→高风险放行
- `is_revoked()` 查 `revoked_devices` 表（独立于缓存正文），不受清除影响

**评估**：符合"清除后不误判为无吊销"要求。

#### 2. 冷静期删除（grants.rs + contract.rs）

- `trust_now()` 函数已删除（:74-89 整块移除）
- `in_cooldown()` 在 `contract.rs:389-391` 替换为 `false`
- `fs_gate`（:239）冷静期分支删除，只剩只读拒绝消息
- `exec_gate`（:260）冷静期分支删除，只剩 ask 拒绝消息
- `mesh.rs`（:931-932）前端输出硬编码 `"inCooldown": false`、`"cooldownUntil": 0`，不再读契约数据
- feature-tree.md 更新为"策略 B 替代旧冷静期"

**评估**：冷静期逻辑已清除，无残留路径将新设备当只读或反之。

#### 3. BackendStatus::Default 仅测试可用（mod.rs:864-891）

- `#[derive(Debug, Clone, Default, Serialize)]` 改为 `#[derive(Debug, Clone, Serialize)]` + `#[cfg_attr(test, derive(Default))]`
- `BackendStatus::new()` 显式构造，`experts_synced=Some(false)`（pi）或 `None`（非 pi）
- `wiki/compile/tests.rs` 中替身显式填充所有字段，不再用 `..Default::default()`
- 新增 `status_tests` 验证 `new("pi")` → `experts_synced=Some(false)`、`new("claude-cli")` → `experts_synced=None`

**评估**：`Default` 限定测试，生产代码统一显式构造。

---

### 建议验证步骤（区分推断风险与已有证据）

| 步骤 | 风险等级 | 依据 |
|------|----------|------|
| 1. 确认 `require_fresh` 查询在清除后返回 `None`（现有测试已覆盖关键路径） | 低推断 | `document<>''` 条件语义明确，测试 `corrupt_cache_denies_high_risk_until_next_security_heartbeat` 已有端到端验证 |
| 2. 验证并发心跳不会在清除期间写入新文档后被意外清除（`document=?2` 原子性依赖 SQLite 行锁） | 低推断 | SQLite `UPDATE ... WHERE` 语义保证，但未提供并发测试证据 |
| 3. 确认 `is_revoked` 不依赖缓存正文（现有测试已覆盖） | 无风险 | 已有证据：`corrupt_cache_denies_high_risk_until_next_security_heartbeat` 第 814 行显式断言 |
| 4. 检查 feature-tree.md 未提及的 20 个拆分文件是否仅纯搬移（未提供 diff，无法核实） | 中推断 | Brief 称已排除，建议 Claude 独立核对原函数完整性 |

---

### 缺失材料说明

1. **20 个路由拆分文件**：diff 体积超限已排除，无法核实生产路由 89 条是否完全保留
2. **测试运行日志**：未附 CI 输出，结论依赖 brief 中"实测"声明
3. **并发场景测试**：无并行心跳竞争测试证据（风险低但未覆盖）
