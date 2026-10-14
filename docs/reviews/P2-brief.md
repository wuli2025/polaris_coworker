# 审查 brief：P2 Rust 侧 AgentBackend 与 PiBackend

## 目标
Polaris（Tauri + Rust）把 Agent 底座从 claude CLI 换成 Pi SDK。P2 在 Rust 内核加 `AgentBackend` 抽象：`PiBackend`（默认）通过 stdio JSONL 守护并调用 Node 进程 agentd；旧 claude CLI 逻辑原样搬进 `ClaudeCliBackend`。前端事件 `chat:stream`（meta/delta/tool/artifact/error/done）必须保持不变。

## 验收条件
- 后端可配置切换（pi / claude-cli），非法值报具名错误。
- agentd 崩溃、超时、协议错误时，进行中的轮次一定以 error + done 收尾，前端不挂死；重启指数退避+抖动，连续失败熔断。
- 事件通道有界、回压不丢 error/done；取消一定产生 done。
- 子进程随父退出；新代码无 unwrap/expect；不新增 crate；文件 ≤500 行。

## Claude 实测结果（沙箱外，2026-10-14）
- `cargo check -p polaris-app`：通过，7 警告（与基线相同）
- `cargo check -p polaris-cli --bin polaris-server --features collab-net`：通过，7 警告（与基线相同）
- `cargo test -p polaris-kernel --lib --tests`：134 通过、4 失败、2 忽略；4 个失败均为重构前已存在的 Windows 路径测试（基线记录）。集成测试 2 通过。
- `npx vue-tsc --noEmit`：通过。行数检查：本次新建/改动文件全部 ≤500 行。

## 已知/主控已发现的问题（请独立判断，不必复述）
- Rust 侧有固定 `turnTimeoutMs`（默认 600000）总时长超时，可能与 agentd 的"空闲超时+可选硬上限"语义冲突。
- per-account 隔离尚未实现（明确拒绝）；逐会话模型/权限/附件存在协议缺口，留待后续。

## 请重点审查
supervisor 的重启/熔断/收尾正确性与竞态；事件翻译是否符合前端契约（尤其 error/done 顺序、delta 合批、artifact）；回压与有界队列是否可能死锁或丢终态；取消路径；Windows Job 实现是否正确；配置校验。
