# P3a 审查报告

**审查模式**：packet（无工具），按本次调用指令报告

---

## 结论：CHANGES_REQUIREED

所审范围内存在需要明确或修复的问题。部分逻辑存在边界条件风险，且存在若干需要 Rust 端或测试证据补充才能确认的验收点。

---

## 范围

- `agentd/config/agent.default.json`（配置）
- `agentd/src/config/schema.ts`（配置校验）
- `agentd/src/daemon.ts`（调度）
- `agentd/src/extensions/permissions.ts`（权限引擎）
- `agentd/src/extensions/resources.ts`（上下文文件）
- `agentd/src/extensions/sandbox.ts`（沙箱封装）
- `agentd/src/extensions/subagent.ts`（子代理工厂）
- `agentd/src/extensions/web.ts`（HTTP/SSRF 防护）
- `agentd/src/protocol/events.ts`（事件 Schema）
- `agentd/src/protocol/schema.ts`（协议 Schema v3）
- `agentd/src/sessions/create.ts`（会话创建）
- `agentd/src/sessions/options.ts`（会话配置）
- `agentd/src/sessions/registry.ts`（会话注册与子代理）

**缺失材料**：Rust 后端实现、协议两端对齐证据、符号链接场景的测试用例。

---

## 问题

### 问题 1：`withinWorkspace` 对不存在路径的符号链接解析存在逃逸风险

**严重程度**：中（安全边界）

**文件**：`agentd/src/extensions/permissions.ts`，第 167-173 行

```typescript
async function canonical(path: string): Promise<string> {
  try { return await realpath(path); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT" || dirname(path) === path) throw error;
    return resolve(await canonical(dirname(path)), basename(path));
  }
}
```

**证据**：当路径不存在时，`canonical()` 递归解析父目录的真实路径，然后拼接 basename。这可能导致符号链接在不存在子路径的情况下被跟随。

**场景**：假设工作区为 `/home/user/workspace`，其中存在符号链接 `sym → /`，若用户执行 `write` 操作路径为 `sym/../../../etc/passwd`，则：
1. `canonical("sym/../../../etc/passwd")` → `realpath(dirname)` = `realpath("/home/user/workspace")` → `"/home/user/workspace"`
2. `resolve("/home/user/workspace", "etc/passwd")` → `"/home/user/workspace/etc/passwd"`

→ 逃逸失败，但若 `sym` 本身不存在：
1. `canonical("sym")` → `canonical(dirname)` = `canonical("/home/user/workspace")` → `realpath` 成功 → `"/home/user/workspace"`
2. `resolve("/home/user/workspace", "sym")` → `"/home/user/workspace/sym"`（解析为目录而非链接目标）

→ 逃逸被阻止。但若 `sym` 指向工作区外的路径且 `sym` 不存在，则 `canonical` 会将 `sym` 视为普通目录而非跟随链接，可能绕过 workspace 限制。

**可复现影响**：恶意用户可能通过构造不存在的符号链接路径绕过 `auto-current` 模式的工作区限制。

**最小修复建议**：在 `canonical()` 中对不存在的路径进行特殊处理——要么拒绝（返回 false），要么仅跟随到最近的真实祖先目录而不拼接 basename。具体方案需根据产品策略决定。

---

### 问题 2：沙箱不可用时的错误语义不明确

**严重程度**：低-中（可用性）

**文件**：`agentd/src/extensions/sandbox.ts`，第 296-312 行

```typescript
request(operation: string, args: Record<string, unknown>, signal?: AbortSignal): Promise<string> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new Error("SANDBOX_ABORTED"));
    const callId = randomUUID();
    const abort = () => finish("", "SANDBOX_ABORTED");
    const timer = setTimeout(() => finish("", "SANDBOX_TIMEOUT: no host execution result"), this.config.timeoutMs);
    const finish = (output: string, error?: string) => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      this.pending.delete(callId);
      if (error) reject(new Error(error)); else resolve(output);
    };
    this.pending.set(callId, finish);
    signal?.addEventListener("abort", abort, { once: true });
    try { this.emit({ callId, operation, args: { ...args, cwd: this.cwd, ...this.config } }); }
    catch { finish("", "SANDBOX_UNAVAILABLE"); }
  });
}
```

**证据**：`catch` 块调用 `finish("", "SANDBOX_UNAVAILABLE")`，符合"沙箱不可用时报错"要求。但随后通过超时报错 `SANDBOX_TIMEOUT` 返回给用户，错误消息对用户不够友好，且 120 秒超时在用户感知上可能偏长。

**可复现影响**：沙箱后端启动失败或通信异常时，用户需等待 120 秒才能看到明确错误。

**最小修复建议**：考虑增加快速心跳/ping 机制，在会话建立时验证沙箱可用性；或在超时时提供更清晰的诊断信息。

---

### 问题 3：`allowAlways` 的 key 生成对含非确定性数据的参数可能失效

**严重程度**：低（功能）

**文件**：`agentd/src/extensions/permissions.ts`，第 218 行

```typescript
const key = JSON.stringify([tool, args]);
```

**证据**：若 `args` 中包含时间戳、随机数、动态生成的内容等，每次调用的 key 均不同，导致 `allowAlways` 无法实现跨调用记忆。

**场景**：Claude 执行 `bash({ command: "echo $RANDOM" })`，每次命令相同但参数中包含随机元素（假设通过环境变量注入），导致每次 key 不同，无法真正"记住"允许。

**最小修复建议**：若需要真正的"始终允许"，应仅基于工具名（和可选的路径参数白名单）生成 key，或对 `args` 进行标准化处理（排除时间戳、摘要化长输出）。但这取决于产品对 `allowAlways` 的实际定位。

---

### 问题 4：子代理 `abort` 信号竞态条件

**严重程度**：低（健壮性）

**文件**：`agentd/src/sessions/registry.ts`，第 792-826 行

```typescript
async subagent(id: string, callId: string, task: string, signal?: AbortSignal): Promise<string> {
  // ...
  const abort = () => { if (childId) void this.abort(childId).catch(() => undefined); };
  signal?.addEventListener("abort", abort, { once: true });
  try {
    // ...
  } finally {
    signal?.removeEventListener("abort", abort);
    if (childId) await this.close(childId).catch(() => undefined);
    this.runningChildren--;
  }
}
```

**证据**：当父会话 abort 触发时，事件触发顺序为：
1. `signal.aborted` 变为 true
2. `abort()` 注册的监听器被调用 → `this.abort(childId)` 执行
3. 子会话的 `abort()` 中设置 `run.stopped = "aborted"`，调用 `cancelTools()` 和 `stopChildren()`
4. `finally` 块执行 `await this.close(childId)`

这个顺序是正确的，但存在边界情况：若 `signal.aborted` 在 `await this.wait(childId)` 返回后、`finally` 执行前变为 true，`abort()` 监听器可能错过触发时机。不过 `finally` 中的 `this.close(childId)` 会兜底，不算严重问题。

**建议验证步骤**：需要通过实际测试验证竞态条件是否完全消除。

---

## 无阻塞问题但需注意的设计点

### 协议 v3 完整性

Schema 已升级到 v3（`PROTOCOL_VERSION = 3`），新增了 `permission.respond`、`sandbox.respond`、新事件类型和会话选项。但前端/SDK 是否同步更新无法通过此 diff 确认。建议验证 Claude brief 中的"协议 v3 两端一致性"是否有测试或对齐文档。

### Rust 后端缺失

`web.ts` 中的 `boundedGet` 发起 HTTP 请求，`sandbox.ts` 中的 `request` 发送协议消息，但对应的 Rust 端实现不在本包范围内。若 Rust 端未正确实现，协议会超时而非优雅失败。

### 子代理继承的 model 可能为 undefined

```typescript
model: entry.session.model && { provider: entry.session.model.provider, id: entry.session.model.id },
```

`entry.session.model` 为 optional，若父会话无模型，子会话也无模型。是否合理取决于产品设计——若无模型则无法执行，此行为可接受。

### 专家系统提示拼接顺序

```typescript
return { systemPrompt: [event.systemPrompt, config.experts.prompt, prompt].filter(Boolean).join("\n\n") };
```

拼接顺序为：Agent 默认系统提示 → experts.prompt → expertPrompt（会话级）。这意味着会话级 expertPrompt 会覆盖全局 expert prompt。是否符合预期需产品确认。

---

## 验收条件匹配度

| 验收条件 | 匹配状态 | 说明 |
|----------|----------|------|
| 四档权限 | ⚠️ 部分 | 逻辑正确，`withinWorkspace` 存在符号链接逃逸风险 |
| 真沙箱 | ⚠️ 部分 | TypeScript 端正确报错/拒绝，Rust 端缺失 |
| web_fetch SSRF 防护 | ✓ 良好 | BlockList + DNS pin + 重定向重判 |
| web_search 无 Key 报错 | ✓ 良好 | `WEB_SEARCH_CREDENTIAL_REQUIRED` |
| subagent 继承权限 | ✓ 良好 | `permissionMode`, `sandbox` 均继承 |
| subagent 级联中止 | ✓ 良好 | `stopChildren` + `cancelTools` |
| 上下文文件优先级 | ✓ 良好 | AGENTS.md 优先于 CLAUDE.md |
| 专家默认关闭 | ✓ 良好 | `enabled: false` |

---

## 建议验证步骤

以下验证需要 repo 模式或补充材料，无法通过当前 packet 推断：

1. **符号链接逃逸测试**：创建工作区内符号链接指向外部目录，验证 `auto-current` 模式下 `withinWorkspace` 返回值
2. **沙箱不可用测试**：模拟 Rust 后端无响应，验证 TypeScript 端超时行为和错误消息
3. **协议 v3 对齐验证**：检查前端/SDK 是否已支持新协议消息类型
4. **子代理竞态测试**：在父会话 abort 时并发检查子会话状态转换
