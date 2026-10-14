# 任务 P2：Rust 侧 AgentBackend 抽象与 PiBackend

在 `refactor/pi-agent` 分支实现 Rust 壳对 agentd 的接入。先读：
- `docs/plans/2026-10-14-pi-refactor.md` 第 1 节
- `docs/pi-migration/agent-events.md`（`chat:stream` 六种事件 meta/delta/tool/artifact/error/done 及顺序要求——**前端不改，必须保持这个契约**）
- `docs/pi-migration/call-sites.md`（chat 部分）、`docs/pi-migration/baseline.md`（已知失败，不算回归）
- `agentd/src/protocol/`（**协议的唯一真源**，Rust 侧类型按它写，不要另起一套字段名）、`agentd/README` 或 feature-tree 中的 agentd 节点

## 要做什么

1. **后端抽象**（`polaris-kernel/src/chat/backend/`）
   - `AgentBackend` trait：发起一轮（输入：会话 id、提示词、权限档位、模型、工作目录、附件；输出：事件流）、取消、关闭会话、列模型、健康状态。
   - `ClaudeCliBackend`：把现有 `pipeline.rs` 的 spawn/解析与 `session_pool.rs` **原样搬进**这个适配器（行为不变，是旧后端），不重写逻辑。
   - `PiBackend`：见下。
   - 选择：配置项 `agent.backend = "pi" | "claude-cli"`，默认 `pi`；环境变量可覆盖；未知值报具名错误，不静默回落。
2. **PiBackend**（`polaris-kernel/src/chat/backend/pi/`，按职责拆文件）
   - 进程守护：启动 `node <agentd 入口>`（node 路径、入口路径、参数来自配置/环境，不写死）；`ping` 健康检查；崩溃后指数退避重启（上限可配）；重启后进行中的轮次发 `error`+`done` 收尾，不挂死前端。
   - JSONL 客户端：请求 id ↔ 响应关联；按 sessionId/turnId 路由事件；有界通道 + 回压；单行上限；stdout 只解析协议，stderr 进日志。
   - 事件翻译：agentd 事件 → 现有 `ChatStreamEvent`。text.delta→delta（保留约 30ms 合批）；tool.started→tool；写文件类工具（按 Pi 工具名 write/edit 等，名单进配置）→artifact；error→error；turn.completed→done（失败先 error 再 done）；`meta` 维持现有 prompt token 估算。轮末产物快照 diff 继续复用现有实现。
   - 取消：`chat_cancel` → agentd `abort`；保证最终一定有 `done`。
   - 隔离模式：配置项 `agentd.isolation = "shared" | "per-account"`。桌面默认 `shared`（一个 agentd 多会话）；服务端/NAS 多用户形态默认 `per-account`（每个登录账号一个 agentd 进程，崩溃互不影响、工作目录与凭据互不可见）。进程表有上限与空闲回收，均可配置。
3. **拆分**：本次触及的 `pipeline.rs`（2080 行）、`session_pool.rs`（1170 行）拆到每文件 ≤500 行，按职责分（提示词拼装 / 进程 / 流解析 / 产物 / 收尾）。只搬运与拆分，不顺手改行为。
4. **装配**：`chat_send` 走 `AgentBackend`；`headless.rs` 的只读调用也走同一个后端接口（无 UI 场景用"收集全部 delta 直到 done"的辅助函数）。

## 测试
- 事件翻译单测：每种 agentd 事件 → 期望的 `ChatStreamEvent` 序列（含失败轮次先 error 后 done、工具失败、写文件产物）。
- 守护进程测试：用一个**假 agentd 脚本**（`tests/fixtures/` 下的小 node 脚本，按协议回放事件、可模拟崩溃/超时/非法行）验证重启、收尾、取消、回压。
- 后端选择两组配置都生效；非法值报错。
- 旧后端行为：已有测试全部保持通过。
- 必须运行并通过（失败只允许是 baseline.md 列出的 4 个已知失败）：
  - `cd src-tauri && cargo check -p polaris-app && cargo check -p polaris-cli --bin polaris-server --features collab-net`
  - `cargo test -p polaris-kernel --lib --tests`
  - `cd .. && npx vue-tsc --noEmit`

## 规则
- 只改 `src-tauri/crates/polaris-kernel/`、必要的装配点（`src-tauri/src/` 中调用 chat 的地方）与 `docs/`。不改前端、不改 `agentd/`（发现协议缺口就写进结果的"疑问"里，由我决定）。不新增 crate 依赖，除非现有依赖确实做不到（若加，说明理由）。
- 新写/改动的文件 ≤500 行、每行 ≤500 字符；可变值进配置，不写死。
- 更新 `docs/feature-tree.md`。
- 结束时输出：改动文件及行数、命令结果原文摘要、与旧行为的差异清单、未完成/疑问。不要声称做了没做的事。

## 最佳实践要求（追加）
- **沿用现有风格**：错误用 `anyhow`（对外边界用具名错误枚举 + `Display`）；并发沿用现有 `std::thread` + 通道模式，或 tokio（已是依赖，如需 `process` 特性可开），二选一并在模块注释说明理由；**不新增 crate**。
- 新代码禁止 `unwrap()`/`expect()`（测试除外）；锁不跨 I/O 持有；所有阻塞读写有超时或可取消。
- 进程守护：子进程随父退出（Windows 用 Job Object 或现有 `polaris-runtime` 的进程工具，先查 `polaris-runtime/src/procs.rs` 能否复用）；重启退避有上限与抖动；连续失败超过阈值进入"熔断"并给前端明确错误，而不是无限重启。
- 背压：事件通道有界；前端消费慢时合批 delta，不丢 `error`/`done`。
- 可观测：每轮带 reqId/conversationId/sessionId 的结构化日志（沿用仓库现有日志方式）；提供 `agent_backend_status` 查询（后端类型、agentd 版本、进程 pid、重启次数、熔断状态）。
- 契约测试：从 `agentd/src/protocol` 生成或手写一份 JSON 样例集放 `agentd/test/fixtures/protocol-samples.jsonl`（若不存在则在 Rust 侧 `tests/fixtures/` 建），Rust 解析测试必须逐条通过——两端协议漂移时测试立即失败。

## 环境说明
- 你的沙箱里 `cargo` 可用（`~/.cargo` 可写）、无网络；所有 crate 已在本地缓存，用 `cargo ... --offline` 避免联网失败。
- 沙箱里 Node 不能再派生子进程，但 Rust 的 `std::process::Command` 可以启动 `node <脚本>`；假 agentd 脚本本身不要再派生子进程。
- agentd 的协议以 `agentd/src/protocol/` 为准（P1 已验证，含 F1–F6 修复）：请求 `ping / models.list / session.open / prompt / steer / followUp / abort / close / config.apply`；事件 `turn.started / text.delta / reasoning.delta / tool.started / tool.completed / usage / turn.completed(status) / error(code,message)`；`config.apply` 可能返回 `reopenRequired`。
