## 审查结果

**审查模式**：packet（无工具），所有分析基于已内联的 `brief`、`P5b-diff.patch` 与 `P5b-fable.patch`。

---

### 结论：CHANGES_REQUIRED

发现一个可能阻塞验收的问题（done 事件文本格式歧义），以及一个测试覆盖缺口。

---

### 范围

| 文件/模块 | 改动性质 | 行数 |
|---|---|---|
| `kernel/headless.rs` | 重命名 + deprecated 转发 | ±18 |
| `kernel/bridge.rs` | `ToolOptions` 新字段初始化 | ±1 |
| `kernel/mod.rs` | `ToolOptions` 新增字段 | ±4 |
| `kernel/pi/translate.rs` | `detailed_events` 支持 | ±10 |
| `kernel/pi/wiki_tests.rs` | 新增测试 | 50 |
| `kernel/pi/extension_tests.rs` | 测试适配新字段 | ±2 |
| `wiki/compile.rs` | 迁移至 AgentBackend（375→79） | 重构 |
| `wiki/compile/config.rs` | 新增 | 36 |
| `wiki/compile/legacy.rs` | 旧逻辑迁入 | 263 |
| `wiki/compile/progress.rs` | 新增 | 73 |
| `wiki/compile/prompt.rs` | 新增 | 47 |
| `wiki/compile/runner.rs` | 新增 | 111 |
| `wiki/compile/tests.rs` | 新增 | 215 |
| `exec/config.rs` | 白名单配置化 | 51 |
| `exec/defaults.json` | 新增 | 53 |
| `exec/tests.rs` | 拆分 | 201 |
| fable 多文件 | `run_claude_readonly*`→`run_agent_readonly*` | 机械改名 |
| fable `path_tests.rs` / `access_path_tests.rs` | 跨平台拆分 | 新增 |

---

### 问题

#### 1. **[中等]** `done` 事件文本序列化可能产生意外字面量 `"None"`

**文件**：`kernel/pi/translate.rs` 行 167-168
```rust
let status = self.detailed.then(|| format!("{status:?}"));
out.push(self.make("done", status));
```

**证据**：`format!("{status:?}")` 在 `status = None` 时输出字面字符串 `"None"`（Rust `Debug` 实现），而非空字符串或省略。这使 `consume` 中的检查产生歧义：

**文件**：`polaris-wiki/src/wiki/compile/runner.rs` 行 1264
```rust
anyhow::ensure!(!matches!(event.text.as_deref(), Some("Aborted" | "Timeout" | "Error")),
    "WikiCompileInterrupted: {}", event.text.as_deref().unwrap_or_default());
```

- 若 Pi 返回 `None`（成功 done 无状态），前端收到 `"None"` 而非 `None`，`starts_with("ToolFailed: ")` 仍为 false，不被截断，最终 `WikiCompileInterrupted` 错误消息中出现字面 `"None"`。
- 若 Pi 返回 `Some("None")`（状态字面为 `"None"` 字符串），`matches!(..., Some("Aborted" | ...))` 为 false，同上。

**影响**：用户可能在编译成功时收到混淆的错误提示 `"WikiCompileInterrupted: None"`。

**最小修复**：显式处理 `None` 情况：
```rust
let status = status.as_ref().map(|s| s.as_str()).filter(|s| !s.is_empty());
out.push(self.make("done", status.map(String::from)));
```
或在 `runner.rs` 的检查中将 `"None"` 加入排除列表（但这是掩盖而非修复）。

---

#### 2. **[低/建议]** `progress.rs` 路径遍历测试缺口

**文件**：`polaris-wiki/src/wiki/compile/progress.rs`

**证据**：`page_path` 对 `..` 的检查（行 1059）使用 `path.components().any(|c| matches!(c, Component::ParentDir))`，但没有对应测试验证 `a/../b` 类路径被正确拒绝。

`tests.rs` 中的测试用例：
- `../raw/source.md` — 直接 `..`，覆盖
- `abs_path/...` — 绝对路径，覆盖
- `escape/source.md` — 符号链接逃逸，`#[cfg(unix)]` 覆盖

缺失：`"a/../b"`、`"概念/../../etc/passwd"`、`"概念/嵌套/../穿透.md"` 等组合路径。

**可复现影响**：低 — 代码逻辑正确（`components()` 会分解出 `ParentDir`），但无测试保障回归。

**最小修复**：在 `tests.rs` 中补充：
```rust
for path in ["a/../b", "概念/../../etc/passwd"] {
    assert!(progress::page_path(&wiki, path).is_err());
}
```

---

### 确认满足的验收条件

| 条件 | 状态 |
|---|---|
| wiki 走 AgentBackend，cwd=wiki 根，`AutoCurrent` | ✅ `TurnRequest::headless` + `runner.rs` 行 1191-1192 |
| sandbox/tool_profile 沿用户选择 | ✅ `config.sandbox`→`ToolOptions`→`sessions::open`；测试 `wiki_tests.rs:222` 断言 |
| 进度事件走 `kb:compile` | ✅ `runner.rs` 通过 emit 回调推送，progress.rs 过滤 |
| 仅成功写且路径在 wiki 内报"写入页面" | ✅ `progress.rs:1034-1040`，`isError == false` 才报 page |
| 维护锁异常路径释放 | ✅ `Guard` 随线程结束 Drop；测试 `timeout_and_cancel_abort_close...` 行 1434-1453 |
| 取消/超时走 abort | ✅ `runner.rs:1215` + `1217-1218` drain grace；`watchdog` 行 1203-1206 |
| deprecated 转发无遗漏 | ✅ `kernel/headless.rs` 行 245-253；fable `kb/mod.rs` 行 330-331 重导出 |
| exec 白名单可配置，来源与环境变量绑定 | ✅ `config.rs:1802-1806`，仅 `POLARIS_EXEC_CONFIG` 指定路径 |
| exec 校验输入（注入防护） | ✅ `config.rs:1792-1797` 字符集/空值/路径遍历检查 |
| fable 路径测试跨平台，Windows 断言保留 | ✅ `access_path_tests.rs` 行 251-283 `#[cfg(windows)]` |
| 文件行数不超 500 | ✅ 最大的 `legacy.rs` 263 行，`tests.rs` 215 行 |

---

### 建议验证步骤（区分推断与已有证据）

**已有证据支撑**：
1. 单元测试覆盖：路径越界（`../raw/`, 绝对路径, 符号链接逃逸）、维护锁释放、backend guard、配置校验失败、done/abort 语义。无需重新运行。
2. `extension_tests.rs` 中 sandbox/expert_prompt 路径已有既有测试，`wiki_tests.rs` 补充了 `tool_profile` 和 `detailed_events`。

**推断风险（需人工确认）**：
1. `done` 事件文本 `"None"` 问题 — 建议用 mock stream 发送 `done` 事件（text=None）验证 `runner.rs` 的行为与用户看到的最终消息。
2. `backend::selected()` 在 `cancel_compile` 中失败时的错误信息可读性 — 当前错误经 `e.to_string()` 透传，建议抽测。
3. `CompileConfig::load()` 的 `timeout_secs` 从环境变量覆盖（行 714-716），`POLARIS_KB_COMPILE_CAP_SECS` 与 `config.timeout_secs` 的优先级是否有文档说明。
