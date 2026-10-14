# R3：P3 第 1 轮修复执行报告（待主控复核）

范围依据 [P3 分流](../reviews/P3-round1-triage.md)：仅 C1、P3a-1/2、P3b-8/9，以及 P3a-3/P3b-6 文档。
P3b-2 配置化、P3b-4/5 留后续；未改变驳回项的授权策略；无新增包依赖、无提交或推送。

## 入口、依赖与行为

- `agentd/src/extensions/permissions.ts`：在路径展开/规整前拒绝任一 `..` 段；存在前缀 realpath，逐层 lstat 拒绝悬空链接，不能将链接当作不存在的尾部。
  POSIX 上 Windows 反斜杠/盘符路径保守回落人工确认；Windows 使用本机分隔符与 realpath。`./a/b`、工作区内绝对路径可通过。
- `agentd/src/extensions/sandbox.ts`：启用沙箱时在任何首次工具调用前发 `probe`；并发共享探测，成功/失败按 TTL 缓存。失败为 `SANDBOX_UNAVAILABLE`，不执行实际工具。
  `remoteBash.exec` 返回后端状态原值；模型输出包含超时/信号标记，非零退出码由 Pi SDK 判失败。
- `K/chat/backend/host_tools.rs`：公开 `SandboxExecutor -> SandboxOutput`，将状态逐字段映射到协议。重复安装执行器记录 JSON warn，不替换已有实例。
- `K/chat/backend/docker_tools.rs`、`sandbox_process.rs`、`sandbox_command.sh`：inspect 和 exec 使用有界子进程等待；并发读取 stdout/stderr；容器内监督执行、截止时终止进程组。
  监督脚本使用镜像已有 BusyBox 工具（sh、setsid、mktemp、sleep、kill）；probe 同时检查工具与当前挂载路由。
  正常退出状态文件区分 `exit 124/137` 与超时/信号；Docker 传输超时无法得知容器最终退出码时返回 null，不捏造 0。
- `agentd/src/sessions/registry.ts`、`K/chat/backend/pi/{protocol,translate}.rs`：授权结束通知透传到父会话；取消时仍可发 resolution，先于轮次终态。
- `src/features/permissions/`：显示服务端截止时间的剩余秒数；到零禁用按钮；按 sessionId/callId 关闭已解决弹窗；保留 done 清理兜底。

`K/` 指 `src-tauri/crates/polaris-kernel/src/`。以上均通过现有公开端口接线，没有修改既有超长前端文件。

## 协议 v4 与迁移

TS `PROTOCOL_VERSION`、Rust `PROTOCOL_VERSION`、ping 握手、假 daemon、共享 JSONL 样例和源文件 SHA256 一起升级。
缺失/旧版本（包括 v3）仍触发 `AgentdProtocolMismatch`；Rust 壳与 agentd 必须一起升级，旧二进制不能混用。

`sandbox.respond` 保留 `sessionId/callId/output/error?`，新增三个必需字段：

| 字段 | 类型 | 语义 |
| --- | --- | --- |
| exitCode | integer 或 null | 正常退出码；信号退出按 shell 约定为 128+signal；未知时 null |
| signal | 正整数或 null | 检测到的终止信号；正常退出/未知为 null |
| timedOut | boolean | 容器命令或 Docker 传输达到截止时间；与普通非零退出分开 |

`output` 保留命令 stdout/stderr。`error` 表示执行设施/协议错误，不用它替代普通命令的非零退出码。
`sandbox.request.operation=probe` 复用请求响应链路，使用独立短超时；成功响应 exitCode=0、signal=null、timedOut=false。

`permission.request` 新增 `expiresAt`（Unix 毫秒时间戳）；新增 `permission.resolved`：
`{targetSessionId, callId, reason: "responded" | "expired" | "cancelled"}`。
请求到期、人工回应、重配置/轮次取消或结束都会清理 pending，并且每个 pending 仅发一次 resolution。
子会话事件沿父轮次透传，但 targetSessionId 保留子会话身份；Rust 转为弹窗 payload 中的 sessionId。

**allowAlways 作用域：同一会话、完全相同的工具与参数。**
实现以 `JSON.stringify([tool,args])` 精确匹配，不是对该工具的任意参数授权；参数键序不同也可能重新请求。
关闭会话或重新配置权限会清空记忆；轮次结束不扩大授权；不同会话及子会话各自独立。
弹窗按钮及解释文字同时说明此范围。

## 配置默认、校验与切换

默认 `sandbox.probeTimeoutMs=5000`，`sandbox.probeCacheMs=10000`；两者必须为 1..2147483647 的整数。
配置在会话创建时捕获，修改后重开会话。顶层 patch 的 sandbox 对象是整体替换，因此旧自定义配置需补两个字段。

```json
{
  "sandbox": {
    "probeTimeoutMs": 2000,
    "probeCacheMs": 15000,
    "timeoutMs": 120000,
    "maxOutputBytes": 262144,
    "shell": "sh"
  }
}
```

用 `config.apply` 的 config 传入上例，或放入 agentd 配置文件；`session.open`/空闲 `session.configure` 的 `sandbox:true` 启用沙箱。
沿用 permissions.timeoutMs=60000 作为授权截止时间；前端只展示服务端 expiresAt，不维护第二套授权超时。

## 验证记录

以下为实际运行结果，临时日志在 `/tmp/r3-validation/`。最终验收状态 **BLOCKED**：强制全量测试未全绿，不能据本报告宣告模块完成。

| 工作目录 | 实际命令 | 结果 |
| --- | --- | --- |
| agentd | `npm run typecheck && npm test` | typecheck 通过；test 退出 1，18 个测试文件通过、2 个失败（e2e/web），见下方 |
| src-tauri | `cargo check --offline` | 退出 0，桌面壳通过，有既有警告 |
| src-tauri | `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 退出 0，实际服务端入口通过，有既有警告 |
| src-tauri | `cargo check --offline --no-default-features --features server` | 退出 0，额外 server feature 检查通过 |
| src-tauri | `cargo test -p polaris-kernel --lib --tests --offline` | 退出 101；168 通过、10 失败、2 忽略；库失败导致该次未执行集成测试 |
| src-tauri | `cargo test -p polaris-kernel --test backend_config --offline` | 单独补跑集成测试：4 通过 |
| src-tauri | `cargo test -p polaris-kernel --lib --tests --offline chat::backend` | 最终回归：68 通过；后端退出状态、协议指纹、v4 握手和授权翻译全通过 |
| 仓库根 | `npx vue-tsc --noEmit` | 退出 0 |
| 仓库根 | `npx vitest run src` | 退出 0，4 文件、13 测试通过 |
| agentd | `node --import tsx test/sandbox-status.test.ts` | 6 通过：非零/信号/超时及探测/cache |
| agentd | `node --import tsx test/permissions.test.ts` | 24 通过：矩阵、路径、授权结束 |
| agentd | `node --import tsx test/protocol-extension.test.ts` | 6 通过：含共享 request/event 样例 |
| 仓库根 | `git diff --check`；Python 按改动文件扫描物理行/单行字符数 | 通过；所有 31 个自有改动文件 ≤500 行、每行 ≤500 字符 |

全量 agentd 的失败单独诊断：

- `node --import tsx test/web.test.ts`：2 通过、1 失败；TCP 测试 `listen EPERM: operation not permitted 127.0.0.1`，属于当前沙箱端口限制。
- `node --import tsx test/e2e.test.ts`：1 失败；首次 ping 等待超时，子进程 exit=0、stderr 空。属于子进程 E2E 链路失败，根因未确认，不将其宣称为通过或已证明的环境豁免。
- 曾尝试 `node --import tsx --test --test-isolation=none ...` 获取展开诊断；Node 22.19 不支持该选项，退出 9；随后采用上述直接执行测试文件方式获取实际失败详情。

内核 10 项失败：

- 4 项与 [既有基线](baseline.md#已知失败重构前就存在) 一致：`doctor::path::tests::{claude_dir_from_path_picks_parent_or_npm_prefix,prime_and_prepend_behaviour}`、`doctor::verify::tests::{install_root_groups_npm_entries_together,two_installs_with_different_versions_report_both_conflicts}`。
- 6 项本地端口相关：`integrations::codex_proxy::tests::{e2e_anthropic_passthrough_over_real_tcp,e2e_openai_route_over_real_tcp}`（bind PermissionDenied）。
- `provider::claude_login::claude_oauth_tests::{loopback_run_end_to_end_local,loopback_run_rejects_wrong_state}`（loopback bind Operation not permitted）。
- `provider::store::local_route_tests::{openai_provider_routes_via_local_router,route_all_switch_reroutes_anthropic_provider}`（无法绑定 8765–8789）。这些不扩大基线豁免范围。

未完成/未运行：

- 真实 Docker 容器端到端未运行：`docker --version` 返回本 WSL 无可用 Docker。监督脚本用本地 sh/bash 行为测试验证，不能替代目标 BusyBox 镜像联调。
- Windows 原生路径测试、浏览器实际点击与双机时钟差场景未运行；当前仅 Linux 路径/Windows 字符串分隔符测试及前端类型/队列单测。
- e2e 子进程失败根因及上述强制全量失败需主控在可用环境重跑/处置。P3b 延后项按 brief 未实现。
- 保留原有未跟踪目录 `docs/execution/R3-1/`，未写入；没有执行 git commit / git push。

## 改动文件清单

| 文件（仓库相对路径） | 行为 |
| --- | --- |
| `agentd/config/agent.default.json` | 探测默认超时和缓存时长 |
| `agentd/src/config/schema.ts` | 配置/协议 v4 类型校验 |
| `agentd/src/daemon.ts` | 传递完整沙箱回包 |
| `agentd/src/extensions/permissions.ts` | 路径包含判断及授权结束通知 |
| `agentd/src/extensions/sandbox.ts` | 探测、缓存、真实 bash 状态 |
| `agentd/src/protocol/events.ts` | 授权 deadline/resolved 协议 |
| `agentd/src/protocol/schema.ts` | 配置/协议 v4 类型校验 |
| `agentd/src/sessions/registry.ts` | 会话/子会话事件与回包接线 |
| `agentd/test/permissions.test.ts` | 路径逃逸与授权结束行为测试 |
| `agentd/test/protocol-extension.test.ts` | 共享协议样例校验 |
| `agentd/test/sandbox.test.ts` | v4 沙箱响应测试 |
| `agentd/test/subagents.test.ts` | 探测继承和子授权结束透传测试 |
| `docs/feature-tree.md` | 入口、依赖、配置、迁移、验证索引 |
| `docs/pi-migration/agent-events.md` | v4 生命周期和 allowAlways 作用域 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/docker_tools.rs` | 容器探测、退出状态解码和监督脚本行为测试 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/host_tools.rs` | SandboxOutput 公开端口、回包和安装失败警告 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs` | 注册有界 Docker 子进程模块 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/extension_tests.rs` | v3 拒绝及授权结束 Rust 翻译回归 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/protocol.rs` | Rust v4 类型 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/tests.rs` | 假 daemon 握手版本 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/translate.rs` | 授权 deadline/resolution 转前端事件 |
| `src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs` | v4 假 daemon 与授权生命周期 |
| `src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl` | v4 正常/失败/信号/超时/授权样例 |
| `src-tauri/crates/polaris-kernel/tests/fixtures/protocol-schema.sha256` | 同步 TS 协议源指纹 |
| `src/features/permissions/PermissionDialog.vue` | 倒计时、过期按钮和作用域文案 |
| `src/features/permissions/queue.test.ts` | 按身份关闭及倒计时测试 |
| `src/features/permissions/queue.ts` | 消费 resolution 与剩余秒数接口 |
| `agentd/test/sandbox-status.test.ts` | 非零/信号/超时、探测/cache 行为测试 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/sandbox_process.rs` | Docker 子进程有界等待与输出读取 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/sandbox_command.sh` | 容器内命令/看门狗、状态与超时标记 |
| `docs/pi-migration/r3-result.md` | 本执行报告 |
