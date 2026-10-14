

## 审查结果

**审查模式**：packet（无工具）

**结论**：INCOMPLETE

---

### 范围与缺失材料

本次 diff 包含锁回收机制、provider 异步命令、generation 围栏、用量持久化失败计数、`builtinCatalog` 更新、catalog schema 等改动。**未提供**以下关键材料，无法完成独立评审：

| 缺失项 | 影响 |
|--------|------|
| 测试文件（含锁回收向量、OAuth 持久化验证、generation 代次测试） | 无法确认锁回收竞态、OAuth 写入文件、命令 superseded 场景的测试覆盖 |
| `pi-ai/src/auth/resolve.ts`（P4a-2 刷新写回凭据的实现位置） | 无法验证 OAuth 刷新后 token 是否真的落盘 |
| `agentd/src/models/runtime.ts` 变更前的 `apply` 方法 | 无法确认 `builtinCatalog` 是否在 apply 后同步刷新 |
| `provider/selection.rs` 变更前逻辑及 `provider_apply` 旧版本 | 无法确认 generation 围栏的必要性论证 |
| 沙箱外实测的完整测试输出（含 Node↔Rust 跨语言锁互通测试的具体用例描述） | 无法核实"锁回收测试"覆盖了哪些向量 |

---

### 审查范围内的问题

#### 1. 锁回收逻辑——`owner.json` 被删除但目录仍在时误判为存活

**严重程度**：中  
**位置**：
- `agentd/src/models/credential-lock.ts:105`
- `src-tauri/crates/polaris-kernel/src/provider/credential_lock.rs:692`

**证据**（Node 侧，Rust 侧逻辑相同）：
```ts
// credential-lock.ts:104-109
let info: unknown;
try { info = JSON.parse(await readFile(`${lock}/${token}/owner.json`, "utf8`)); } catch { /* torn metadata */ }
if (!validOwner(info)) {
  const modified = await stat(`${lock}/${token}`).then((s) => s.mtimeMs).catch(() => undefined);
  if (modified !== undefined && Date.now() - modified > maxHoldMs) return retire(lock, token);
  continue;  // ← 目录有效但 owner.json 丢失时，age 也未超则跳过
}
```
```rs
// credential_lock.rs:692-693
let info = fs::read(entry.path().join("owner.json")).ok()
    .and_then(|b| serde_json::from_slice::<Owner>(&b).ok());
let Some(info) = info.filter(...validOwner...) else { /* age check + continue */ };
```

**可复现影响**：持锁进程在写入 `owner.json` 后、删除前崩溃，或文件系统元数据损坏导致文件消失；此时锁目录存在、`acquiredAt` 来自内存未落盘，无法通过时间判断。进程仍在运行时，`processState` 查询会返回"alive"，`reclaimable` 为 false，锁永久残留。**发生概率低但不可忽视**。

**最小修复建议**：在 `acquire()` 中先写 `owner.json` 再原子 rename 到 lock 目录；或 `reclaim` 时检查目录年龄而非仅在 `validOwner` 失败时检查。

---

#### 2. `provider_apply` 中 `selected != id` 时提前返回

**严重程度**：低  
**位置**：`src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs:397-398`

**证据**：
```rs
let (generation, selected) = crate::provider::selection::snapshot();
if selected != id { return Ok(serde_json::json!({"applied":false,"superseded":true})); }
```

**分析**：若 `provider_switch` 已在 pending 状态但还未写入 `STORE.current_id`，此时 `selected` 仍为旧值，`provider_apply("new-id")` 会直接返回 superseded，即使切换即将完成。这在以下时序下发生：
1. UI 触发 `provider_switch("claude-official")`
2. Rust 侧 `changed()` 更新 generation，但 `STORE.current_id` 尚未写入（代码在 `let _mutation = PROVIDER_MUTATION.lock()` 之后才写）
3. 轮次内 `provider_apply("claude-official")` 到来，`snapshot()` 拿到旧 id，返回 superseded
4. 循环重试，成功

实际影响是增加一次 IPC 开销，不阻塞正确性。**低优先级**。

---

#### 3. `assertOwned` 在 Rust 侧 `write` 中的位置

**严重程度**：低  
**位置**：`src-tauri/crates/polaris-kernel/src/provider/credential_file.rs:654`

**证据**：
```rs
_lock.assert_owned()?;
atomic_write(&path, &text).map_err(|_| "CredentialStoreWriteFailed".into())
```

`assert_owned()` 在 `atomic_write` 之前调用。若锁在 `atomic_write` 执行期间被回收（竞态窗口），写入仍会完成但锁已失效。Node 侧在 `modify` 中先 `await assertOwned()` 再 `await save(root)`（`file-credentials.ts:208-209`），同样的窗口存在。

**最小修复建议**：将 `atomic_write` 的目标路径纳入锁的原子性保护范围（例如写入临时文件后再 rename 到锁目录内），使写入本身与锁的生命周期绑定。

---

#### 4. 协议版本升级（5→6）的指纹测试覆盖

**严重程度**：信息  
**位置**：
- `agentd/src/protocol/schema.ts:318` → PROTOCOL_VERSION = 6
- `src-tauri/crates/polaris-kernel/src/chat/backend/pi/protocol.rs:456` → 6

**分析**：Node schema 和 Rust 协议版本同步升级到 6，新增字段 `usagePersistFailures`、`CatalogSchema`。根据 `protocol.rs:455` 注释"fingerprint test gates changes"，需确认指纹测试是否已更新。若无指纹测试约束，版本号升级无法防止未来两侧漂移。

---

### 建议验证步骤（区分推断风险与已有证据）

| 步骤 | 类型 | 说明 |
|------|------|------|
| 补充锁回收向量测试：子进程持锁后被 SIGKILL，另一进程在 maxHoldMs 内回收并完成读写；活着进程不被抢锁；同时回收只有一个成功 | **推断风险** | diff 中未见测试文件，无法确认已有覆盖 |
| 补充 OAuth 持久化测试：使用 `FileCredentials` 登录后断言 `~/.polaris/credentials.json` 内容含新 token；刷新后再次断言文件更新 | **推断风险** | brief 称已补测试，但测试文件未提供 |
| 验证 `provider_apply` 提前返回场景：模拟 UI 切换 pending 期间轮次触发 | **推断风险** | 需要集成测试或日志验证 |
| 确认指纹测试已更新至协议版本 6 | **已有证据（diff）** | 两侧版本同步更新，但需实际测试确认 |

---

### 结论说明

本次 diff 实现了 P4a-1 锁回收机制的核心逻辑（持有者信息、墓碑原子重命名、PID/启动时间复用保护、跨主机超时），Node 与 Rust 两侧语义一致，generation 围栏解决锁内 IPC 阻塞问题。**但由于测试文件、OAuth 刷新写回实现、generation 必要性论证均未提供，独立审查无法确认以下关键项通过**：

1. 锁回收在所有向量下正确（包括 owner.json 丢失但目录仍在的边界情况，见问题 1）
2. OAuth 刷新后 token 真的落盘而非仅在内存中
3. `builtinCatalog` 在 apply 后真的同步刷新
4. generation 代次"最后一次选择生效"的保证有测试佐证

根据审查规则，**缺少必要实现或证据时使用 INCOMPLETE**，不能把未知当通过。
