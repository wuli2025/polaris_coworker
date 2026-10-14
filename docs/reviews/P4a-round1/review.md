## 审查结果

**审查模式：** packet（无工具；所有评审仅依据本任务内联材料）

**结论：** CHANGES_REQUIRED

---

### 范围与缺失材料

| 范围 | 说明 |
|------|------|
| P4a-diff.patch | agentd 凭据/模型 + Rust provider 新逻辑；store.rs 纯搬移已排除 |
| 测试证据 | brief 声称沙箱外测试通过（107/107、181/181 等），但测试文件被排除，未在本包中提供 |

---

### 严重问题

#### 1. 锁崩溃残留导致永久阻塞（严重）

**文件：**
- `agentd/src/models/file-credentials.ts` 第 240 行
- `src-tauri/crates/polaris-kernel/src/provider/credential_file.rs` 第 1327 行

**证据（file-credentials.ts）：**

```typescript
// 第 226–241 行
private async locked<T>(work: () => Promise<T>, options?: AuthOperationOptions): Promise<T> {
  await mkdir(dirname(this.path), { recursive: true });
  const lock = `${this.path}.lock`;
  const deadline = Date.now() + this.locks.timeoutMs;
  while (true) {
    options?.signal?.throwIfAborted();
    try { await mkdir(lock); break; }  // ← 持锁
    catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw new Error("CredentialStoreLockFailed");
      if (Date.now() >= deadline) throw new Error("CredentialStoreBusy");  // ← 超时后抛异常，不抢锁
      await delay(this.locks.pollMs, undefined, { signal: options?.signal });
    }
  }
  try { options?.signal?.throwIfAborted(); return await work(); }
  finally { await rm(lock, { recursive: true }); }  // ← 仅在正常/异常退出时清理
}
```

**Rust 侧（credential_file.rs）：**

```rust
impl Drop for FileLock { fn drop(&mut self) { let _ = fs::remove_dir(&self.0); } }
```

**缺陷：** `finally` / `Drop` 只在进程正常退出时执行。若进程在持锁期间崩溃、断电、被 kill -9、或 Promise reject 后未触达 finally，则 `.lock` 目录永久残留。此后所有凭据读写（超时前）均返回 `CredentialStoreBusy`，无法自愈。

**可复现影响：**
1. 进程 A 持锁时卡在网络请求（锁内阻塞调用）
2. 进程 A 被强制终止
3. `.lock` 目录残留
4. 所有后续进程超时等待，人工删除前服务不可用

**最小修复建议：** 在 `mkdir` 前尝试清理同名 `.lock` 目录（检测其修改时间戳或内容判断是否过期），对超时残留进行主动回收；或在锁目录中写入持有者进程 ID，超时后由其他进程验证并回收。

---

#### 2. 刷新后的 token 持久化路径缺失（严重）

**文件：** `agentd/src/models/logins.ts` 第 357–361 行

**证据：**

```typescript
// 第 357–361 行
login.done = this.models.runtime.login(provider, "oauth", { signal: controller.signal, notify, prompt })
  .then((credential) => {
    if (credential.type === "oauth") {
      this.models.redactor.add(credential.access);    // ← 仅注册到脱敏器
      this.models.redactor.add(credential.refresh);
    }
    login.state = { status: "ok" };
  }, () => { login.state = { status: "failed" }; })
  .finally(() => { ... });
// ← 没有调用 credentials.modify() 或 save() 写回磁盘
```

**问题：** `login.done` 中取到的新 credential（含刷新后的 refresh token）仅更新了内存状态，未落 `providers.json`。进程重启后读取磁盘，旧 token 已过期。用户需重新 OAuth。

**与 brief 验收条件 2 的偏差：** brief 明确要求"刷新后的 token 持久化"，但当前实现在 `logins.ts` 中没有触发 `FileCredentials.modify` 或等效持久化调用。Rust 侧的 `pi_login.rs` 是代理调用，也不主动持久化。

**最小修复建议：** 在 `login.done` 的 `.then()` 中，调用 `credentials.modify()` 将新 credential（含更新后的 expires）写回共享存储，并确保原子写路径覆盖。

---

#### 3. `usage-store.ts` 中 `recordUsage` 失败被静默吞没（中等）

**文件：** `agentd/src/events/usage-store.ts` 第 113–134 行 + `agentd/src/sessions/registry.ts` 第 551–554 行

**证据：**

```typescript
// registry.ts 第 551–554 行
if (event.type === "message_end" && event.message.role === "assistant") {
  try { recordUsage(this.config().usageDir, id, run.id, usageOrdinal++, event.message); }
  catch { run.mapper.error("USAGE_PERSIST_FAILED", "Usage record could not be persisted"); }  // ← 仅记录错误，不传播
}
```

`recordUsage` 内部校验失败（`UsageInvalid`）或文件系统错误时，错误被 try/catch 吞没，只向当前 run 的 mapper 报告 `USAGE_PERSIST_FAILED`。但如果该 turn 中途失败（例如 token 耗尽），`message_end` 事件可能不触发，用量记录丢失，且无独立日志/告警。

**最小修复建议：** 在 daemon 级别维护一个失败的用量记录计数器或队列，供运维诊断；或至少在日志（`daemon.logger.warn`）中输出一次。

---

#### 4. Pi 后端故障切换不触发（设计限制未明确标注）

**文件：** `src-tauri/crates/polaris-kernel/src/provider/store/failover.rs` 第 1768–1810 行

**证据：** `note_provider_failure` 接入点是"本地路由（codex_proxy/代理）错误分类点"（第 1764–1767 行注释）。但 brief 验收条件 6 要求 Pi 错误分类接入 `polaris-failover`。`pi_outcome` 函数（failover.rs 第 1833–1837 行）虽定义了，但调用路径未在 diff 中出现。

**问题：** Pi 后端（`models.runtime`）的请求错误不经本地路由，`note_provider_failure` 永远不触发。`polaris_failover` 对 Pi 后端实质无效。

**最小修复建议：** 在 `pi_outcome` 的调用方（agentd 的模型调用拦截或 SDK 回调）中接入 `note_provider_failure`，或在 brief 中明确此为已知限制。

---

### 设计问题（低严重性）

#### 5. `models.builtinCatalog` 在 `runtime` 更新后可能过期

**文件：** `agentd/src/models/runtime.ts` 第 391 行

`builtinCatalog` 在 `Models.create` 时从旧 `runtime` 快照中计算（第 397–399 行）。`apply` 更新 `this.runtime` 时未同步更新 `builtinCatalog`。若 `models.catalog` 被用于模型选择，可能用到过期模型列表。

#### 6. `models.catalog` 未在 schema 中定义输出结构

**文件：** `agentd/src/protocol/schema.ts` 第 446 行

`models.catalog` 请求被注册（第 446 行），但 `catalog` 响应的结构（包含 `api`、`baseUrl`、`cost` 等）未在 schema 中定义，与 `models.list` 的返回结构不一致。

---

### 已验证为正确的设计

| 方面 | 结论 |
|------|------|
| 凭据单一来源 | agentd 与 Rust 共享 `providers.json`，锁路径一致（`${path}.lock`） |
| 原子写（Unix） | TypeScript 和 Rust 均用 `open(wx)` + `fsync` + `rename`；Rust 额外用进程级 SEQ 防止并发 tmp 冲突 |
| Windows rename | Rust 注释确认 `MoveFileExW + REPLACE_EXISTING` 语义覆盖，代码逻辑未显式处理但 rename 覆盖行为满足需求 |
| 明文防护 | `usage-store.ts` 不记录 prompts/headers；`FileCredentials.register` 将凭据加入脱敏器；`provider_list` 打码路径在 `secrets.rs` |
| 用量去重键 | `legacyMessageId` 走 `request:{date}:{id}` 路径（usage.ts 第 2309 行 + pi_usage.rs 第 1629 行），与 Pi 的 `pi:{date}:{provider}:{request_id}` 路径不冲突 |
| 会话历史 | `rebuild()` 传入 `existingManager`，`createSession` 复用 `sessionManager`（含历史），`previous.dispose()` 仅清理本次会话资源 |
| OAuth 并发 | `locked()` 串行化所有凭据读写，两进程同时刷新时后者读前者已写回的新 token |
| 锁内无阻塞 | TypeScript `locked()` 中 `work()` 包含 `this.root()`（读文件）和 `this.save()`（写文件），均为本地 I/O，非网络调用 |

---

### 建议验证步骤

> 以下步骤依赖 repo 模式工具（read/grep），packet 模式下推断存在风险；未运行测试，不能声称测试通过。

1. **崩溃残留锁（推断高风险）：** 模拟 agentd 进程在持锁期间被 kill -9，验证 `~/.polaris/providers.json.lock` 是否残留且阻塞后续启动；对比 brief 第 17 条主控已发现的现象。

2. **刷新 token 持久化（需补充证据）：** OAuth 完成后检查 `providers.json.credentials[anthropic]` 的 `refresh` 和 `expires` 字段是否被更新；需在 `logins.ts` 中追踪 `credential` 对象是否触发 `credentials.modify()`。

3. **Pi 后端 failover（需补充证据）：** 在 Pi 模式下触发 MODEL_AUTH 错误，用 grep 查找 `note_provider_failure` 或 `FAILOVER.record_failure` 的调用点，验证是否实际接入。

4. **`builtinCatalog` 过期（推断风险）：** 修改 provider 配置后通过 `models.catalog` 端点查询，验证返回的 `baseUrl` 是否为最新值。

5. **锁超时配置（需实际压测）：** 两个 agentd 进程并发持锁，第一个持锁进程在超时阈值附近完成，验证第二个进程的 `CredentialStoreBusy` 是否正确抛出。
