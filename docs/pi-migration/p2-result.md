# P2：Rust 后端接线记录

分支：`refactor/pi-agent`。这是实现进展记录，**P2 尚未达到完整验收**。
没有修改前端或 `agentd/`，没有新增 crate。

## 已实现

- `AgentBackend`：一轮输入、事件流、取消、关闭会话、模型列表、健康状态。
- 默认 Pi，`agent.backend` 与环境覆盖；未知值返回 `InvalidBackend`。
- Claude 管线与池按提示拼装、进程、采样、解析、收尾、池记账拆分；原测试搬移。
- shared Pi：独立 Tokio actor、Node stdio JSONL、请求 ID 关联、session/turn 路由、ping、限长、stderr 日志。
- 有界 stdout、stdin 和事件队列；慢消费暂停 stdout 路由；相邻 delta 合并；终态预留槽位。
- 崩溃/协议错误/超时收尾、指数退避（上限及抖动）、连续失败熔断、空闲进程回收。
- `abort` 取消；等待超时则结束本轮并重启 daemon，终态仍为 done。
- Linux PDEATHSIG + 进程组；Windows Job kill-on-close 实现，但只在 Linux 实测。
- 原 prompt token 估值、产物快照 diff、应用文件夹归并和 assistant 落库继续复用。
- `chat_send` / headless 只读入口走 backend；无 UI 收集 delta 到 done。
- `agent_backend_status` 已在 Tauri 和 HTTP invoke 注册；含 backend/agentdVersion/pid/restarts/circuitOpen/healthy/lastError。
- 假 daemon 无子进程、无模型调用；协议样例覆盖九个请求、响应、所有事件及四种终态。
- 协议测试同时固定两个 TS schema 的 SHA-256；上游修改会强制失败并要求人工审阅 Rust 类型与样例。

## 配置

宿主配置路径：`POLARIS_AGENT_CONFIG`。启动时选定 backend，修改后需重启宿主。
完整默认值和校验见 `src-tauri/crates/polaris-kernel/src/chat/backend/config.rs`。
配置文件顶层是 `agent` / `agentd`，agentd 内字段使用 camelCase。

```json
{
  "agent": { "backend": "pi" },
  "agentd": {
    "node": "/absolute/path/to/node",
    "entry": "/absolute/path/to/agentd/src/main.ts",
    "nodeArgs": ["--experimental-strip-types"],
    "args": ["--config", "/absolute/path/to/agentd-config.json"],
    "isolation": "shared",
    "channelCapacity": 64,
    "maxLineBytes": 1048576,
    "requestTimeoutMs": 15000,
    "turnTimeoutMs": 600000,
    "abortTimeoutMs": 5000,
    "pingIntervalMs": 30000,
    "idleMs": 600000,
    "restartBaseMs": 200,
    "restartMaxMs": 30000,
    "restartJitterMs": 100,
    "failureThreshold": 5,
    "stableResetMs": 60000,
    "deltaBatchMs": 30,
    "writeTools": ["write", "edit"]
  }
}
```

上例路径须由部署填写；Rust 没有硬编码仓库入口路径。Node 参数取决于部署的 Node 版本与 TS 运行方式。
宿主环境覆盖：`POLARIS_AGENT_BACKEND`、`POLARIS_AGENTD_NODE`、`POLARIS_AGENTD_ENTRY`、
`POLARIS_AGENTD_ARGS` / `POLARIS_AGENTD_NODE_ARGS`（JSON 字符串数组）、`POLARIS_AGENTD_ISOLATION`。
`POLARIS_AGENTD_CONFIG` 仍由 agentd 自己解释，宿主配置和 daemon 配置不是同一个文件。
`sessionMap` 是宿主 UUID 映射目录，默认 data/agent-session-map；文件按逻辑会话键 SHA-256 命名。

## 与旧行为的差异

1. 默认由 Claude CLI 改为 Pi；旧分支显式选 `claude-cli`。旧 CLI argv、工具名单、模型环境变量、池 LRU/TTL 与 parser 保留。
2. 旧取消准备阶段过去可能不发事件，现在补 done，避免统一事件流挂起。旧 headless 的 argv/parser 保留，但进程登记到共享 registry，以便统一 cancel/退出回收；stdin 改为可被 kill 打断的后台写入。
3. Pi stderr 只进结构化日志；协议非法 stdout 不作为正文显示，而是 error + done 并触发重启。
4. Pi 写工具在成功 tool.completed 后才发 artifact；失败工具发 error，不报不存在的产物；轮末扫描仍复用旧实现。
5. Pi reasoning/usage 没有新增前端事件；meta 保持估值，不冒充真实 usage；usage 未接旧账本。
6. Pi 会话 UUID 由 agentd 分配，本地保存 conversation/session key → UUID；已删除的 daemon 会话可重新建立。
7. Pi 当前仍使用原提示拼装；SDK 已有会话上下文时仍会收到原历史块，没有实现旧池的 keyed/session 增量去重。
8. Pi 的模型按进程配置；TurnRequest.model 与已打开模型不符时明确拒绝。聊天 providerId 指定供应商时明确拒绝，P4 才做供应商映射。
9. Pi 手动/拒绝档只允许实际 tools 为 readonlyTools 子集；另外两档使用配置好的 daemon 工具面。四档动态权限机制尚不具备。
10. Pi useSandbox、二进制附件明确拒绝，未伪造协议字段。现有前端以 prompt 文本传入的路径仍随 prompt 传递。
11. Pi 预热当前 no-op，首轮惰性启动；Pi 默认本轮硬超时 600000ms 可配置。旧 CLI 继续自己的看门狗，没有套 Pi 的 10 分钟硬顶。
12. CLI 模型列表只返回已配置的模型名，不声称能查询 CLI 的远程模型目录；无 UI legacy 入口仍限定原只读调用。

## 未完成 / 疑问

- **per-account 未实现**：桌面默认 shared；服务端默认 per-account 会返回 AccountContextRequired。
  即使直接 TurnRequest 带 account_id，也返回 UnsupportedIsolation，不会退回 shared。
  HTTP 层已有 AuthCtx，但尚未向后端传递可信账号；headless 后台任务也没有账号归属。原 prompt/KB/产物存储仍是宿主全局。
  仅按账号多开 Node 并设置 cwd/HOME 不能保证工作目录、凭据不可互见，不能把它称为隔离。
  `maxProcesses` 已保留配置和正数校验，但尚无 per-account 进程表及淘汰实现；shared 始终只有一个 daemon。
- **协议缺口**：session.open 只有 sessionId/cwd，prompt 只有 sessionId/text；没有逐会话模型、四档权限、附件字段。
  config.apply 是全进程的；当前只提供进程级 apply 并关闭 reopenRequired 会话，不冒充逐会话设置。
- **版本缺口**：ping 只有 status/sessions；agentdVersion 明确为 null。需要上游协议提供版本才可填写。
- **验证未达标**：完整 kernel 测试除 4 个基线失败，还存在 6 个沙箱禁止创建/绑定 TCP 的失败；不改测试、不扩大 baseline 豁免。
- **未实测**：真实模型、打包后的 Node/agentd 发现、Windows Job 运行、父进程被外部强杀的端到端验证。
- `rustfmt` 组件未安装且环境无网络；已检查行数、字符数和 diff 空白，但没有声称 cargo fmt 通过。

## 验证与文件清单

实际命令与原文摘要见 [验证记录](p2-validation.md)，逐文件行数见 [改动清单](p2-files.md)。
