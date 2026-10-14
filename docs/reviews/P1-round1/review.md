

## 审查报告

### 审查模式
**packet**（无工具）。仅依据内联材料评审。

---

### 结论
**CHANGES_REQUIRED**

所审范围发现 3 个需要修复的问题，其中 1 个严重（竞态导致错误被静默吞噬）、2 个中等（Redactor 绕过风险、opening 集合不对称）。

---

### 范围与缺失材料

| 类别 | 范围 |
|------|------|
| 已审文件 | 14 个源码/测试/配置文件（`config/load.ts` ~ `agent.default.json`，`test/sessions.test.ts` ~ `test/timeouts.test.ts`） |
| 未提供 | `src/models/runtime.ts`（仅部分行）、`test/helpers.ts`（测试工具）、端到端测试完整源码 |

---

### 问题

#### 问题 1（严重）
**文件**：`src/sessions/registry.ts` 行 134–149

**证据**：
```typescript
async queue(id: string, text: string, method: "steer" | "followUp") {
  const entry = this.get(id);
  const run = entry.run;
  if (!run || run.stopped) throw new DaemonError("SESSION_NOT_RUNNING", ...);
  // ← 此处检查通过后，abort/close 可能设置 run.stopped
  entry.session.clearQueue(); // ← 只清队列，不停止正在执行的 steer/followUp
  try {
    const disposition = await entry.session[method](text);
    return { turnId: run.id, disposition };
  } catch (error) {
    // ← 错误被静默捕获，用户只收到 abort 的终端事件
  } finally {
    entry.queueReservations--;
    if (entry.run !== run || run.stopped) entry.session.clearQueue();
  }
}
```

**影响**：如果用户在 steer/followUp 执行期间调用 `abort`/`close`，`queue` 中的后续 steer/followUp 调用虽然会抛出 `SESSION_NOT_RUNNING`（因为 `run.stopped` 已被设置），但如果 `queue` 在 `run.stopped` 检查通过后、`entry.session[method]` 调用执行期间收到 abort，则：
1. `clearQueue()` 仅清空排队消息，不中断已开始的 steer/followUp 调用
2. `method` 调用抛出的错误被 catch 块吞没（没有 rethrow 或发出 error 事件）
3. 用户只收到一个 `turn.completed` / `error` 事件，且错误原因不明确

**最小修复**：catch 块应 rethrow 不应静默吞噬，或在 finally 中检测 `run.stopped` 变化并发送错误事件。

---

#### 问题 2（中等）
**文件**：`src/models/credentials.ts` 行 15–19

**证据**：
```typescript
text(value: string): string {
  for (const secret of [...this.secrets].sort((a, b) => b.length - a.length)) {
    value = value.split(secret).join("[REDACTED]");
  }
  return value;
}
```

**绕过路径**：
1. **Unicode 规范化**：`key` vs `k\u0301ey`（组合字符）在 JSON 序列化后可能不匹配 split
2. **分割注入**：`apikey` → `ap" + "ikey` 若出现在日志上下文（JSON 序列化后）则无法匹配原始 `apikey`
3. **零宽字符**：密钥中的零宽字符在 split 前若被 normalize 可能导致不匹配

**影响**：在异常路径（如错误消息、调试日志）中，密钥片段可能以变体编码出现并泄露。

**最小修复**：对密钥进行 Unicode NFC 规范化后再加入 Set；考虑使用正则全局替换并检测边界字符。

---

#### 问题 3（低）
**文件**：`src/sessions/registry.ts` 行 45–46, 63

**证据**：
```typescript
if (sessionId) this.opening.add(sessionId);  // 行 46：只在有 sessionId 时添加
// ...
if (sessionId) this.opening.delete(sessionId); // 行 63：同样只在有 sessionId 时删除
```

**分析**：opening 集合用于检测 `SESSION_ALREADY_OPEN`，其条件 `this.opening.has(sessionId)` 仅在 `sessionId` 非空时为真。因此无 sessionId 参数时 opening 集合不会被使用。不对称是无意的还是设计的需要确认。

**影响**：有限——无 sessionId 时不会触发 duplicate 检测，且 opening 集合始终为空。

**最小修复**：澄清意图；如无意义则移除 opening 集合的相关操作。

---

### 建议验证步骤（推断风险，非已有证据）

| 风险 | 验证方式 |
|------|----------|
| 问题 1（竞态静默吞噬） | 在 steer 执行期间并发 abort，用测试断言验证是否收到 `error` 事件或 `turn.completed` 带 `aborted` 状态 |
| 问题 2（Redactor 绕过） | 注入含 Unicode 组合字符的 API key，触发错误路径，确认日志中无明文泄露 |
| 问题 3（opening 集合） | 确认无 sessionId 时 opening 集合永远为空，不影响功能即可关闭 |

---

### 已有证据与验收条件匹配

| 验收条件 | 证据 |
|----------|------|
| 配置不硬编码 | schema.ts + validate.ts + load.ts 支持运行时 patch ✓ |
| 跨会话并发、同会话串行 | `schedule()` 以 session key 隔离 + pending Set ✓；ping 不阻塞已在 `concurrency.test.ts` 验证 ✓ |
| 超时语义 | `expire()` 设置单终态 + `timeouts.test.ts` 验证 idle/max/active 三模式 ✓ |
| 密钥不进日志 | Redactor 覆盖日志与事件写入 ✓；问题 2 为潜在绕过 |
| 模型错误固定中文提示 | `model-error.ts` 五类映射 ✓ |
| 优雅退出 | `shutdown()` 等待 pending + close 所有 session ✓ |

---

### 无需阻塞的问题（设计已知权衡）

1. **JsonlWriter 无显式背压**：write 失败时 Promise 链保存错误，后续 send 被拒绝；内存可能短暂积累，但不丢数据。

2. **model-error.ts 对无 diagnostics 的适配器返回 `MODEL_ERROR`**：注释已标注为已知限制，非本次修复范围。

3. **turnMaxMs=0 表示不限**：schema 允许 0 值，`max: 2147483647` 覆盖非负整数 ✓。
