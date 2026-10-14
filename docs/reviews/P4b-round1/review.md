

## P4b 独立审查报告

### 审查模式
**packet**（无工具），依据 `p4b-diff.patch` 内联显式材料评审。

---

### 结论
**CHANGES_REQUIRED**

---

### 范围与缺失材料
| 类别 | 状态 |
|------|------|
| 协议版本 4→5 | ✅ 本次可见 |
| 新增 `Auth` / `Catalog` 命令 | ✅ 本次可见 |
| Provider selection 逻辑 | ✅ 本次可见 |
| Translator 错误跟踪与 outcome 上报 | ✅ 本次可见 |
| 锁实现与崩溃残留 | ❌ 位于 P4a-diff.patch（已排除） |
| 原子写实现 | ❌ 位于 P4a-diff.patch（已排除） |
| `pi_failure_counts` / `pi_outcome` 具体实现 | ❌ 未提供实现源码 |
| session reopen 旧逻辑移除后的新策略 | ❌ 未提供 agentd 端改动 |

---

### 问题

#### 问题 1：`input.rs` 优先选非首选模型的逻辑缺陷
**严重程度**：中等  
**位置**：`input.rs` 第 29–34 行  
**证据**：
```rust
let preferred = models.iter().find(|m| &m.id == id && m.provider == selected).cloned();
let mut matches = models.into_iter().filter(|m| &m.id == id);
let model = matches.next().ok_or_else(|| anyhow::anyhow!("AgentdModelNotFound: {id}"))?;
anyhow::ensure!(preferred.is_some() || matches.next().is_none(), "AgentdModelAmbiguous: ...");
Some(preferred.unwrap_or(model))
```
**影响**：当请求的 id 同时存在于首选和非首选提供商时，`preferred` 非空且 `matches.next()` 也有值，`ensure!` 通过后 `unwrap_or(model)` 仍返回非首选模型——即 provider-specific 优先逻辑完全失效。多个提供商提供同名模型时行为不可预期。
**最小修复**：改为 `preferred.unwrap_or_else(|| matches.next().unwrap())`。

---

#### 问题 2：session reopen 逻辑移除后无等效保障
**严重程度**：高  
**位置**：`mod.rs` 第 60–70 行（删除了 reopenRequired 处理）  
**证据**：原逻辑遍历 `reopenRequired` 数组逐一 Close；改后直接 `Ok(result)`。  
**影响**：brief 承诺"空闲会话自动重开保留历史"，但当前实现放弃了 agentd 返回的 reopenRequired 列表处理。若 agentd 仍按旧协议返回此字段，Pi 不消费会导致空闲会话持有旧提供商上下文。  
**最小修复**：确认 agentd 侧已移除 `reopenRequired` 返回，或在 Pi 侧保留该处理分支。

---

#### 问题 3：`pi_failure_counts` 和 `pi_outcome` 实现不可审
**严重程度**：中等  
**位置**：`translate.rs` 第 196、205 行  
**证据**：函数调用无实现源码；`thread::spawn` 没有 join handle，错误被静默吞掉。  
**影响**：① `pi_failure_counts` 是否正确维护计数器、阈值多少、是否线程安全——均不可验证；② outcome 上报失败不报错，日志不可诊断；③ brief 要求接入 `polaris-failover`，但本 diff 未展示 failover 触发逻辑，无法确认是否正确联动。  
**最小修复**：提供 `pi_failure_counts` 和 `pi_outcome` 的实现源码；至少用 `spawn_thread` 返回 `JoinHandle` 并在适当层级 join。

---

#### 问题 4：`ProviderMutation` 锁覆盖范围不足
**严重程度**：中等  
**位置**：`mod.rs` 第 95–99 行  
**证据**：
```rust
if crate::provider::credential_file::provider_store_path().is_some() {
    let _selection = crate::provider::PROVIDER_MUTATION.lock();
    let id = crate::provider::current_provider_id();
    self.provider_apply(if id.is_empty() { "claude-official" } else { &id })?;
}
```
**影响**：锁仅保护 `current_provider_id()` 读取和 `provider_apply` 调用；但 `provider_apply` 内部会调用 `client.call()` → 网络通信 → **持锁期间的阻塞 I/O**。若网络慢，持锁时间不确定。  
**最小修复**：锁范围应限于纯内存操作；网络调用应移到锁外，结合 `Arc::swap` 或双检锁避免重复调用。

---

#### 问题 5：`provider_apply` 缓存基于 PID 但未处理进程重启
**严重程度**：低  
**位置**：`mod.rs` 第 81–84 行  
**证据**：`let pid = client.status.lock().pid`；`if last.as_ref() == Some(&(pid, config.clone()))`  
**影响**：Pi 后端进程重启后 PID 变化，缓存失效但 agentd 可能仍持有相同配置，产生冗余 `apply_config` 调用。若 `apply_config` 本身幂等则无功能性影响，但增加进程间通信开销。  
**最小修复**：使用启动时间戳或 UUID 替代 PID 作为缓存 key。

---

### 推断风险（无测试证据）

| 风险 | 推断依据 | 验证建议 |
|------|----------|----------|
| 锁目录崩溃残留 | brief 主控自述；P4a 未提供，无法独立确认 | 需单独审查 P4a-diff.patch 中 `mkdir` 是否带 `O_EXCL`，进程崩溃信号处理路径 |
| Windows rename 覆盖行为 | P4a 未提供 | 需单独审查 P4a 原子写实现 |
| 凭证明文进日志/事件 | 本次 diff 无凭据处理逻辑 | 需审查 P4a `credentials` 字段传递路径 |
| 用量去重键误合并 | 未提供去重实现 | 需审查 P4a 用量落库逻辑 |

---

### 建议验证步骤
1. 补充 `pi_failure_counts` 和 `pi_outcome` 实现源码，核实阈值触发 failover 的逻辑路径。
2. 确认 agentd 侧 `reopenRequired` 字段是否已移除或改为内部处理。
3. 在 P4a 中确认锁的 `mkdir` 使用 `O_EXCL | O_CREAT` 且有崩溃后清理机制。
4. 修复 input.rs 的首选模型选择逻辑并覆盖同名模型跨提供商的测试用例。
5. 将 `provider_apply` 中的 `client.call()` 移出 `PROVIDER_MUTATION` 锁范围。
