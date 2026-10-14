# 任务 P1：polaris-agentd（Pi SDK 常驻服务）

在 `refactor/pi-agent` 分支、`agentd/` 目录内实现 Polaris 的 Agent 服务进程。先读：
- `docs/plans/2026-10-14-pi-refactor.md`（第 1 节目标架构）
- `docs/pi-migration/agent-events.md`（前端事件契约，必须能映射过去）
- `agentd/test/sdk-contract.test.ts`（**已验证可用**的 Pi SDK 用法，照它写，不要凭记忆猜 API）
- Pi 文档：`agentd/node_modules/@earendil-works/pi-coding-agent/docs/`（sdk.md、rpc.md、extensions.md、sessions.md）与 `dist/**/*.d.ts`

## 要做什么
一个 Node 进程，经 **stdio JSONL** 与 Rust 壳通信，内部托管多个 Pi `AgentSession`。

| 模块（`agentd/src/`） | 职责 |
| --- | --- |
| `config/` | 配置 schema（typebox）、默认值文件 `agentd/config/agent.default.json`、加载与校验（默认值 ← 文件 ← `config.apply` 请求）。可变项：会话目录、最大并发会话数、单会话排队上限、空闲回收时间、单轮超时、单行最大字节、工具白名单、默认模型/思考档位、日志级别、扩展开关。不得在业务代码里写死这些值。 |
| `protocol/` | 请求/响应/事件类型与 schema；JSONL 读写（超过单行上限的输入返回具名错误而不是崩溃；输出逐行 flush）。每个请求带 `id`，响应带同一 `id`；事件带 `sessionId`、`turnId`。 |
| `models/` | 创建 `ModelRuntime`；`CredentialStore` 由宿主注入（先实现内存版 + 接口，P4 再接 Polaris 提供商库）；`models.list`；`config.apply` 可下发提供商与 Key。**Key 永不写日志、永不出现在事件里。** |
| `sessions/` | 会话注册表：`session.open`（新建或按 id 恢复，持久化用 `SessionManager.create/open`，目录来自配置）、`prompt`、`steer`、`followUp`、`abort`、`close`；并发上限、单会话串行、空闲回收、单轮超时（超时即 abort 并发 error 事件）。 |
| `events/` | `AgentSessionEvent` → 协议事件：`turn.started`、`text.delta`、`reasoning.delta`、`tool.started`、`tool.completed`、`usage`、`turn.completed`（含 status）、`error`。映射表要能对上 `agent-events.md` 里的现有前端事件。 |
| `extensions/` | 只留装配点：一个扩展工厂列表（P3 会在这里加权限闸与沙箱）。本任务不实现权限。 |
| `main.ts` | 启动、读 stdin、分发、`ping` 健康检查、SIGTERM/stdin 关闭时优雅退出（abort 所有会话、flush）。结构化日志写 **stderr**（带请求 id/会话 id），stdout 只走协议。 |

## 测试（`agentd/test/`，用 faux 模型，不联网）
至少覆盖：JSONL 分帧（半行、多行一块、超长行、非法 JSON）；事件映射（文本、思考、工具成功/失败、usage）；会话生命周期（open→prompt→close、按 id 恢复后历史还在、并发上限拒绝、同会话第二个 prompt 走 steer/followUp 或明确报错）；abort 与超时；配置两组不同输入都生效、非法配置报具名错误；日志里不出现 Key。再写一个**端到端测试**：用子进程真正启动 `main.ts`，经 stdin/stdout 完成一轮带工具调用的对话。

## 规则
- 只改 `agentd/` 和 `docs/`。不要改 Rust、前端、`package.json` 依赖（依赖已装好且锁定，不允许新增或升级；沙箱无网络）。
- 每个文件 ≤500 行、每行 ≤500 字符；文件按职责拆，不压行。
- 完成前必须运行并通过：`cd agentd && npm run typecheck && npm test`。
- 更新 `docs/feature-tree.md` 加入 agentd 节点（入口、协议、配置入口、测试命令）。
- 结束时输出：新增文件及行数、测试结果原文摘要、未完成或有疑问的点。不要声称做了没做的事。
