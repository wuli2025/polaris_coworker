

## 审查结果

**审查模式**：packet（无工具），仅依据内联 diff 和 brief 评审。

**结论**：CHANGES_REQUIREED

---

### 范围

本轮审查范围：
- `#3` 修复：`State::drop` 回复 deferred/pending，`client.rs` 区分通道关闭与超时
- `#4` 修复：`session.configure` 预检与回滚事务
- `#2` 记录：feature-tree.md TOCTOU 已知限制

---

### 问题

#### 问题 1（低）：回滚逻辑标记语义不精确

**文件**：`agentd/src/sessions/options.ts`，第 42–81 行

**证据**：
```typescript
let modelAttempted = false;
let thinkingAttempted = false;
try {
  if (model) {
    modelAttempted = true;
    // setModel also changes thinking level, including before a possible failure.
    thinkingAttempted = true;   // ← 此处标记与实际业务逻辑不符
    try { await session.setModel(model, { persist: false }); }
    catch { throw DaemonError("MODEL_CONFIGURE_FAILED"); }
  }
  if (thinkingLevel !== undefined) {
    thinkingAttempted = true;   // ← 同上，两个位置标记同一件事
    session.setThinkingLevel(thinkingLevel, { persist: false });
  }
```

**影响**：当 `setModel` 成功后 `thinkingAttempted = true` 实际上是注释中说的"SDK 会改 thinking level"，不是"用户请求改 thinking level"。随后用户显式请求 `setThinkingLevel` 时再设一次 `true`。虽然结果正确（回滚时都会回滚），但 `thinkingAttempted` 语义混淆：它既表示"模型改变导致的 thinking level 被动变更"，又表示"用户显式请求改变 thinking level"。

**最小修复建议**：将两个标记点拆分为 `modelChangedThinking` 和 `thinkingExplicitlyRequested`，回滚 catch 块相应调整判断条件；或至少补充注释说明为何 `setModel` 后就将 `thinkingAttempted` 设为 true。

---

### 已确认正确的部分

| 修复项 | 确认结果 |
|---|---|
| `State::drop` drain deferred/pending 并发送 `SUPERVISOR_EXITED` | ✅ 所有权正确，`drain` 消费元素安全 |
| `client.rs:132-135` 移除 status lock，直接区分 Disconnected/Timeout | ✅ 消除旧版错误地检查 actor 状态掩盖真正原因的 bug |
| `supervisor_tests.rs` 覆盖 panic unwind 场景 | ✅ 验证了 drop 在展开时仍正确回复 |
| `client.rs` 测试三种错误返回值 | ✅ 通道关闭、超时、actor 错误均独立覆盖 |
| configure 预检 schema、模型存在、模型输入类型 | ✅ 全面 |
| 回滚顺序：模型→思考档位→工具 | ✅ 与 brief 要求的 SDK clamp 行为一致 |
| 回滚失败时抛出 `SESSION_CONFIGURE_ROLLBACK_FAILED` | ✅ 不可吞 |
| feature-tree.md 记录 TOCTOU 限制与威胁模型前提 | ✅ 与 R1 triage #2 驳回理由一致 |
| 文件行数 ≤500 | ✅ |

---

### 建议验证步骤（未运行测试）

以下推断基于代码静态分析，需实际运行确认：

1. **#3 运行时验证**：注入 supervisor 崩溃（断控制 mpsc），确认调用方收到 `AgentdSupervisorExited` 而非 `AgentdRequestTimeout`；注入正常超时确认收到 `AgentdRequestTimeout`。
2. **#4 原子性验证**：在 `setModel` 后、`setThinkingLevel` 前注入错误，确认会话状态完全回滚到 `previous` 快照；分别测试模型失败、思考档位失败、工具失败三种场景。
3. **回滚失败验证**：mock SDK `setModel` 成功后第二次调用失败，确认抛出 `SESSION_CONFIGURE_ROLLBACK_FAILED`。

---

### 缺失材料

- 未提供 `routes.rs` 中 `SUPERVISOR_EXITED` 常量的具体定义，无法确认字符串值与 `client.rs` 中引用的名称一致。
- 未提供 `agentd/test/configure-transaction.test.ts` 源码，无法验证测试覆盖了回滚场景。

---

**备注**：问题 1 不阻塞合入，但影响代码可维护性。建议在 `#4` 回滚逻辑前增加注释明确解释 `thinkingAttempted` 双标记的设计意图，或拆分为更精确的变量。
