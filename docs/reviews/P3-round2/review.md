# Pi 独立审查报告

**审查模式**: packet（无工具）—— 仅依据内联材料 R3-diff.patch 及附带的 brief、triage 文件

**主控实测摘要**（来源 brief）：
- agentd typecheck + `npm test` 100/100、Rust `cargo check` + tests 174 通过 4 基线失败、`vue-tsc`、前端 vitest 13/13
- 主控复现脚本验证 `link/../escaped.txt` 修复效果

---

## 结论: **PASS**

所审 diff 范围（C1、P3a-1、P3a-2、P3b-8、P3b-9、协议 v4）未发现阻塞问题。以下为各修复项的逐行审查记录及次要观察。

---

## 修复项审查

### C1: 沙箱 bash 真实退出码

| 位置 | 审查结果 |
|---|---|
| `sandbox.ts:231-237` | `remoteBash.exec` 从 `return { exitCode: 0 }` 改为 `return result`，完整返回 `SandboxResult`（含 exitCode/signal/timedOut）。**修复正确**。 |
| `sandbox.ts:235-236` | 超时时打印 `\nSANDBOX_TIMEOUT\n`，信号时打印 `\nSANDBOX_SIGNAL: {signal}\n`；模型可见失败状态。**行为合理**。 |
| `host_tools.rs:517-519` | `SandboxOutput` 结构体含 `output/exit_code/signal/timed_out`，Rust 端正确填充。**设计合理**。 |
| `host_tools.rs:541-543` | `sandbox_response` 将 `SandboxOutput` 各字段展开写入 `SandboxResponse`，保持与 agentd schema 对称。**正确**。 |

### P3a-1: 路径判定（`..` 防护）

| 位置 | 审查结果 |
|---|---|
| `permissions.ts:71-73` | `path` 和 `expanded` 均在规范化前检查 `..` 段；命中即 `return false`。**修复正确**，与 triage 描述一致。 |
| `permissions.ts:48-56` | `canonical` 悬挂符号链接处理：先用 `lstat` 捕获 ENOENT 但路径存在的情况，抛出专用错误防止误作新组件。**边界处理完善**。 |
| `permissions.ts:75` | 非 Windows 平台拒绝含 `\` 或 `X:` 的路径，防止 foreign-path 混淆。**POSIX 安全考量合理**。 |
| `permissions.ts:76-78` | `realpath(cwd)` 后对绝对路径做 `canonical` 再 `relative`；最终用 `isAbsolute(rel)` 判断是否出站。**逻辑链正确**。 |

### P3a-2: 快速探测与缓存

| 位置 | 审查结果 |
|---|---|
| `sandbox.ts:157-173` | `available()` 检查 `this.cached`（含 TTL），过期后发起 probe；失败时缓存错误，TTL 内不重探。**修复符合 brief**。 |
| `sandbox.ts:163-169` | probe 失败时抛出 `SANDBOX_UNAVAILABLE: ${cause}`，被 `catch` 后写入 `this.cached.error`。**错误传播链完整**。 |
| `sandbox.ts:207-211` | `tool_call` 钩子中调用 `this.available()`，失败时返回 `{ block: true, reason }`。**沙箱不可用时工具被阻止**。 |
| `schema.ts:10` | 新增 `probeTimeoutMs/probeCacheMs` 配置字段。**与实现配套**。 |

**缓存持久化风险**：brief 提及"沙箱掉线后错误放行"。当前 TTL 设计（`probeCacheMs` 可配置，默认应较短）在缓存有效期内确实不重新探测。但缓存过期后自动重新探测，不构成永久绕过。若 TTL 过长（如数分钟）且沙箱频繁切换可用状态，可能短暂误判；建议确认生产默认值 ≤ 30s。

### P3b-8: 授权过期关闭事件与倒计时

| 位置 | 审查结果 |
|---|---|
| `permissions.ts:108-114` | 超时 `finish` 先调用 `this.resolved({ callId, reason: "expired" })`（在 `try` 中），再用 `finally` 执行 `resolve(decision)`。**时序正确**。 |
| `permissions.ts:110` | `if (!this.pending.has(callId)) return;` 防止 `resolved` 二次触发时重复处理。**防御性检查**。 |
| `events.ts:251-252` | 新增 `permission.resolved` 事件 schema（reason: responded/expired/cancelled）。**协议配套**。 |
| `protocol.rs:606-608` | Rust 端 `PermissionResolution` 枚举与 TS 对称。**版本一致**。 |
| `queue.ts:842-843` | `permission.resolved` 事件按 sessionId+callId 过滤并移除对应队列项；不匹配的 session 不影响队列。**过滤正确**。 |
| `queue.ts:845` | 校验 `expiresAt` 为非负安全整数。**输入验证**。 |
| `PermissionDialog.vue:755,778` | 倒计时每秒更新；剩余 0 时按钮禁用并提示"授权已过期"。**UI 行为符合 brief**。 |

### P3b-9: `set_sandbox_executor` 失败告警

| 位置 | 审查结果 |
|---|---|
| `host_tools.rs:526-530` | `set_sandbox_executor` 失败时写 `stderr` JSON 格式警告日志（组件名+sandbox）。**符合 brief**。 |

### 协议 v4

| 位置 | 审查结果 |
|---|---|
| `schema.ts:265` | `PROTOCOL_VERSION = 4`；`SandboxResultSchema` 暴露 exitCode/signal/timedOut。 |
| `pi/protocol.rs:590` | Rust 端同步为 `4`。 |
| `pi/protocol.rs:598` | `SandboxResponse` 新增字段；**双向对齐**。 |

---

## 次要观察

| # | 描述 | 位置 | 风险评估 |
|---|---|---|---|
| 1 | `this.resolved(...)` 在 `try` 块中，抛出时不会阻止 `finally` 中的 `resolve`。但若事件发送失败（如服务器断连），错误会传播到 Promise executor；调用方若未捕获，程序可能崩溃。 | `permissions.ts:114` | 低——事件发送为"通知"语义，失败不应影响本地决策；建议在 `try` 内加 `catch` 静默化。 |
| 2 | `toolPath` 对 `~` 使用 `resolve(homedir(), ...)`（POSIX）或手写拼接（Windows），未统一处理。Windows 分支先大写盘符再替换 `/` 为 `\` 再拼接 `homedir()`，逻辑较复杂但路径构造结果应等价。 | `permissions.ts:62-63` | 低——Windows 端路径处理略分散，建议统一测试覆盖。 |
| 3 | probe 缓存 TTL 与 `timeoutMs` 无关联；`timeoutMs` 控制单次请求超时，probe 缓存控制可用性检查频率。两者独立设计合理。 | `sandbox.ts:157-173` | 无阻塞风险；配置层面需注意默认值协调。 |

---

## 未覆盖项

本次审查仅基于 diff 内联材料，无法验证：
- 前端 `PermissionDialog` 实际渲染与倒计时交互（依赖运行证据，brief 未提供截图）
- Docker 容器内 `sandbox_command.sh` 在非 BusyBox 环境（Debian/Ubuntu）下的兼容性（`setsid`/`mktemp` POSIX 保证，理论安全）
- Windows 路径（`\`, `X:`, UNC）`withinWorkspace` 的完整行为（代码有防护但无实测证据）

---

## 验证建议（区分推断风险与已有证据）

| 步骤 | 类型 | 说明 |
|---|---|---|
| 1. **已有证据** | 实测 | 主控复现脚本已验证 `link/../escaped.txt` 修复；建议保留该脚本作为回归测试。 |
| 2. **已有证据** | 实测 | Rust `status_tests` 覆盖信号捕获与超时区分；cargo test 可验证。 |
| 3. **推断风险** | 配置 | 确认 `probeCacheMs` 默认值（如 > 60s）不会在沙箱频繁切换场景下导致过长误判窗口。 |
| 4. **推断风险** | 跨平台 | 在 Windows+NTFS 大小写不敏感文件系统上验证 `withinWorkspace` 对 `Link/../Outside` 的判定（无符号链接测试证据）。 |
| 5. **推断风险** | 信号 | 确认 `sandbox_command.sh` 在收到 SIGTERM（15）而非 SIGKILL（9）时行为（测试覆盖 SIGKILL，未覆盖 SIGTERM）。 |
