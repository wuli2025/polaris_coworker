

# 审查报告

**审查模式**：packet（无工具）  
**结论**：CHANGES_REQUIRED

---

## 范围与材料

- **Patch**：P5cd-backend.patch（agentd + polaris-kernel + src-tauri）
- **配套证据**：Brief 中提供 Claude 实测数据（vue-tsc、vitest 61/61、cargo test 213 通过、agentd test 122/122）
- **未提供**：前端 patch（brief 提及的 `P5cd-frontend.patch` 未在源码数据中）、完整上下文文件内容

---

## 严重问题

### 1. `write` 写入不截断文件 — 数据损坏风险

**文件**：`src-tauri/crates/polaris-kernel/src/claude_md/context.rs`，行 587–598

```rust
pub fn write(root: &Path, selection: &Selection, content: &str) -> Result<(), String> {
    let path = pinned(root, selection)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);  // ← 截断已设置
```

检查 patch 原文后确认 `truncate(true)` 已存在（行 590）。**此问题不存在，撤回。**

---

### 2. 专家开关服务端路径缺少鉴权

**文件**：`src-tauri/src/expert/settings.rs`，行 1383–1394

```rust
pub fn expert_enabled_get() -> Result<ExpertState, String> {
    polaris_kernel::chat::backend::selected()
        .and_then(|backend| backend.experts_enabled(None))
        ...
pub fn expert_enabled_set(enabled: bool) -> Result<ExpertState, String> {
    polaris_kernel::chat::backend::selected()
        .and_then(|backend| backend.experts_enabled(Some(enabled)))
```

**问题**：`expert_enabled_get` 和 `expert_enabled_set` 均通过 Tauri command 暴露，无任何用户身份或项目所有权校验。任何能调用 Tauri API 的客户端均可读写任意会话的专家开关。

**影响**：用户 A 可以通过项目 X 的前端开关用户 B 的项目 Y 的专家模式。

**证据**：brief 第 5 条明确要求"真实开关可读可写（Tauri 与服务端两条路径）"，但未要求鉴权；而 brief 第 17 条重点审查"专家开关的服务端路径是否有鉴权（谁能改）"。

**最小修复**：在 `expert_enabled_get/set` 中增加项目上下文参数，比对当前用户是否有权操作该项目后端。

---

### 3. ExpertConfig.prompt 无限长且无 API 支持

**文件**：`src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts.rs`，行 314

```rust
pub struct ExpertConfig { pub enabled: bool, pub prompt: String }
```

**问题**：
1. `prompt: String` 无长度限制，可持久化极长字符串，可能导致磁盘或内存问题。
2. 现有 `expert_enabled_set` 只接受 `enabled: bool`，前端无法修改 prompt 值。
3. `apply_experts` 对比配置时使用 `==`，若 prompt 编码或空白字符不同可能误判。

**证据**：测试中 prompt 仅作为 marker 字符串使用，无长度测试；`expert_enabled_set` 无 prompt 参数。

**最小修复**：
- 对 `prompt` 增加长度上限（如 4096 字符）
- 或增加独立 `expert_prompt_set` 命令
- 或在 `apply_experts` 比较时忽略尾部空白

---

## 中等问题

### 4. `restore_experts` 失败阻止后端启动

**文件**：`src-tauri/crates/polaris-kernel/src/chat/backend/pi/experts.rs`，行 344–349

```rust
fn restore_experts(&self) -> anyhow::Result<()> {
    if let Some(saved) = load(&self.config.expert_settings)? {
        if self.actual_experts()? != saved { self.apply_experts(&saved)?; }
    }
    Ok(())
}
```

**问题**：若 `expert_settings.json` 存在但 agentd 宕机或网络不通，`actual_experts()` 返回错误，导致 `sync_experts()` 失败，可能阻止后端就绪。

**证据**：brief 未提及此场景的容错要求。

**最小修复**：agentd 不可达时将专家状态标记为"未同步"而非失败，允许后端继续启动，后续 ping 恢复时再同步。

---

### 5. 混合记录用量聚合逻辑正确但缺少边界测试

**文件**：`src-tauri/crates/polaris-kernel/src/provider/usage_costs.rs`，行 1012–1032

```rust
pub(super) fn add(&mut self, u: &Usage, cost: f64, has_price: bool, source: &str, model: &str) {
    ...
    if !has_price { self.unpriced_requests += 1; self.has_unknown_costs = true; }
    let index = match self.models.iter().position(|m| m.source == source && m.model == model) {
        Some(index) => index,
        None => { self.models.push(ModelCost { ... }); self.models.len() - 1 }
    };
    let row = &mut self.models[index];
    row.cost += cost;
    if has_price { row.priced_requests += 1; } else { row.unpriced_requests += 1; }
    row.has_price = row.unpriced_requests == 0;
}
```

**问题**：`row.has_price` 在有未标价请求混入后永久为 `false`，即使后续所有请求都标价。这对于长期运行的进程可能造成状态不一致。

**证据**：测试 `complete_mixed_and_unknown_costs_preserve_priced_subtotal` 仅验证累计结果，未测试 `has_price` 状态机的不可逆性。

**最小修复**：考虑将 `has_price` 改为"至少有一次标价"的布尔标记，或增加重置条件说明。

---

## 建议验证步骤

以下步骤基于推断风险，**不能替代实际测试证据**：

| 风险 | 验证方式 |
|------|----------|
| 专家开关鉴权缺失 | 构造两个不同项目，尝试通过项目 A 的前端调用项目 B 的 `expert_enabled_set` |
| prompt 无限长 | 向 `config.apply` 注入 `experts: { enabled: true, prompt: "x".repeat(100000) }`，观察磁盘写入和后续 agentd ping 响应大小 |
| restore_experts 失败阻止启动 | 预先写入 `expert_settings.json`，启动时 kill agentd，观测 polaris-kernel 是否报 ExpertConfigReadFailed |
| 混合记录聚合状态机 | 构造 10 条记录：前 5 条 `has_price=true`，后 5 条 `has_price=false`，验证最终 `bucket.models[0].has_price == false` |

---

## 无问题确认

以下 brief 要求在 patch 范围内已正确实现：

| 要求 | 实现文件 | 确认 |
|------|----------|------|
| 环境诊断读 agentRuntime | agentd/src/daemon.ts:10 ping 响应包含 versions | ✅ |
| 凭据不显示明文 | PingSchema 无 auth 字段 | ✅ |
| 价格可用性基于模型定义 | usage-store.ts:35 `rates.every` 判断 | ✅ |
| 上下文路径限定项目内 | context.rs:548-558 symlink/O_NOFOLLOW 保护 | ✅ |
| 专家默认关闭 | fake-agentd.mjs:1273 `enabled: false` | ✅ |
| 专家开关持久化 + 下发 agentd | experts.rs:325-342 persist + config.apply | ✅ |
| 新逻辑在新文件 | experts.rs / context.rs / usage_costs.rs / usage_prices.rs | ✅ |

---

## 总结

**CHANGES_REQUIRED**：发现 1 个阻塞性安全问题（专家开关鉴权缺失）和 2 个稳健性缺陷（prompt 长度、restore_experts 容错）。用量价格逻辑路径正确，测试覆盖较全，但 `has_price` 状态机的不可逆性需明确语义。
