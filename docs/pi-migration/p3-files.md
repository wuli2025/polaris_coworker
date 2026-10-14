# P3 改动文件与物理行数

共 56 个文件；不包含已有且未修改的 docs/execution/P3-1/。路径相对仓库根。
每个文件均 ≤500 物理行，每行均 ≤500 字符；未修改依赖清单/锁文件。

| 文件 | 物理行数 | 行为 |
| --- | ---: | --- |
| [agentd/config/agent.default.json](../../agentd/config/agent.default.json) | 124 | 带默认值和校验的可切换配置 |
| [agentd/src/config/schema.ts](../../agentd/src/config/schema.ts) | 62 | 带默认值和校验的可切换配置 |
| [agentd/src/daemon.ts](../../agentd/src/daemon.ts) | 118 | 权限/沙箱响应协议分发 |
| [agentd/src/extensions/index.ts](../../agentd/src/extensions/index.ts) | 6 | 权限、容器工具、联网、子任务、上下文资源扩展 |
| [agentd/src/extensions/permissions.ts](../../agentd/src/extensions/permissions.ts) | 90 | 权限、容器工具、联网、子任务、上下文资源扩展 |
| [agentd/src/extensions/resources.ts](../../agentd/src/extensions/resources.ts) | 20 | 权限、容器工具、联网、子任务、上下文资源扩展 |
| [agentd/src/extensions/sandbox.ts](../../agentd/src/extensions/sandbox.ts) | 64 | 权限、容器工具、联网、子任务、上下文资源扩展 |
| [agentd/src/extensions/subagent.ts](../../agentd/src/extensions/subagent.ts) | 9 | 权限、容器工具、联网、子任务、上下文资源扩展 |
| [agentd/src/extensions/web.ts](../../agentd/src/extensions/web.ts) | 132 | 权限、容器工具、联网、子任务、上下文资源扩展 |
| [agentd/src/protocol/events.ts](../../agentd/src/protocol/events.ts) | 24 | 协议 v3 类型与请求/事件 |
| [agentd/src/protocol/schema.ts](../../agentd/src/protocol/schema.ts) | 52 | 协议 v3 类型与请求/事件 |
| [agentd/src/sessions/create.ts](../../agentd/src/sessions/create.ts) | 64 | 会话策略、资源与子会话生命周期装配 |
| [agentd/src/sessions/options.ts](../../agentd/src/sessions/options.ts) | 89 | 会话策略、资源与子会话生命周期装配 |
| [agentd/src/sessions/registry.ts](../../agentd/src/sessions/registry.ts) | 342 | 会话策略、资源与子会话生命周期装配 |
| [agentd/test/helpers.ts](../../agentd/test/helpers.ts) | 57 | 行为测试、协议 v3 夹具或测试权限设置 |
| [agentd/test/permissions.test.ts](../../agentd/test/permissions.test.ts) | 92 | 行为测试、协议 v3 夹具或测试权限设置 |
| [agentd/test/resources.test.ts](../../agentd/test/resources.test.ts) | 38 | 行为测试、协议 v3 夹具或测试权限设置 |
| [agentd/test/sandbox.test.ts](../../agentd/test/sandbox.test.ts) | 48 | 行为测试、协议 v3 夹具或测试权限设置 |
| [agentd/test/subagents.test.ts](../../agentd/test/subagents.test.ts) | 74 | 行为测试、协议 v3 夹具或测试权限设置 |
| [agentd/test/web.test.ts](../../agentd/test/web.test.ts) | 83 | 行为测试、协议 v3 夹具或测试权限设置 |
| [docs/feature-tree.md](../../docs/feature-tree.md) | 284 | 功能入口、配置/迁移、执行结果与行数记录 |
| [docs/pi-migration/p3-files.md](../../docs/pi-migration/p3-files.md) | 63 | 功能入口、配置/迁移、执行结果与行数记录 |
| [docs/pi-migration/p3-result.md](../../docs/pi-migration/p3-result.md) | 133 | 功能入口、配置/迁移、执行结果与行数记录 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/bridge.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/bridge.rs) | 69 | 后端工具参数、协议往返与流事件接线 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/finish.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/finish.rs) | 87 | 旧流事件显式补齐可选 callId/parentCallId |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/mod.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/mod.rs) | 208 | 旧流事件显式补齐可选 callId/parentCallId |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/parser.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/parser.rs) | 219 | 旧流事件显式补齐可选 callId/parentCallId |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/prepare.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/prepare.rs) | 414 | Pi 上下文/专家独立装载，子任务指令适配 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/process.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/process.rs) | 378 | 旧流事件显式补齐可选 callId/parentCallId |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/spawn.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/spawn.rs) | 202 | 删除未使用的容器启动函数 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/stream.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pipeline/stream.rs) | 208 | 旧流事件显式补齐可选 callId/parentCallId |
| [src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pool/reader.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/claude_cli/pool/reader.rs) | 221 | 旧流事件显式补齐可选 callId/parentCallId |
| [src-tauri/crates/polaris-kernel/src/chat/backend/config.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/config.rs) | 134 | 带默认值和校验的可切换配置 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/docker_tools.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/docker_tools.rs) | 148 | 真实 Docker 工具执行与挂载/凭据边界检查 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/host_tools.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/host_tools.rs) | 78 | 容器执行端口、授权响应命令、专家提示接口 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs) | 152 | 后端工具参数、协议往返与流事件接线 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/extension_tests.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/extension_tests.rs) | 135 | 行为测试、协议 v3 夹具或测试权限设置 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/input.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/input.rs) | 34 | 后端工具参数、协议往返与流事件接线 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs) | 153 | 后端工具参数、协议往返与流事件接线 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/protocol.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/protocol.rs) | 168 | 协议 v3 类型与请求/事件 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/sessions.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/sessions.rs) | 66 | 后端工具参数、协议往返与流事件接线 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/tests.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/tests.rs) | 406 | 行为测试、协议 v3 夹具或测试权限设置 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/translate.rs](../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/translate.rs) | 92 | 后端工具参数、协议往返与流事件接线 |
| [src-tauri/crates/polaris-kernel/src/chat/pipeline.rs](../../src-tauri/crates/polaris-kernel/src/chat/pipeline.rs) | 169 | 流事件新增可选工具调用与父调用标识 |
| [src-tauri/crates/polaris-kernel/src/chat/types.rs](../../src-tauri/crates/polaris-kernel/src/chat/types.rs) | 127 | 流事件新增可选工具调用与父调用标识 |
| [src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs](../../src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs) | 98 | 行为测试、协议 v3 夹具或测试权限设置 |
| [src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl](../../src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl) | 31 | 行为测试、协议 v3 夹具或测试权限设置 |
| [src-tauri/crates/polaris-kernel/tests/fixtures/protocol-schema.sha256](../../src-tauri/crates/polaris-kernel/tests/fixtures/protocol-schema.sha256) | 2 | 行为测试、协议 v3 夹具或测试权限设置 |
| [src-tauri/src/apihub/chat_services.rs](../../src-tauri/src/apihub/chat_services.rs) | 331 | 注册授权响应 IPC，沿用现有认证 |
| [src-tauri/src/apihub/desktop.rs](../../src-tauri/src/apihub/desktop.rs) | 138 | 注册授权响应 IPC，沿用现有认证 |
| [src-tauri/src/desktop_commands.rs](../../src-tauri/src/desktop_commands.rs) | 348 | 注册授权响应 IPC，沿用现有认证 |
| [src-tauri/src/wiring.rs](../../src-tauri/src/wiring.rs) | 112 | 注入容器执行器及受保护目录 |
| [src/features/permissions/PermissionDialog.vue](../../src/features/permissions/PermissionDialog.vue) | 62 | 授权确认弹窗/队列 |
| [src/features/permissions/queue.test.ts](../../src/features/permissions/queue.test.ts) | 21 | 行为测试、协议 v3 夹具或测试权限设置 |
| [src/features/permissions/queue.ts](../../src/features/permissions/queue.ts) | 25 | 授权确认弹窗/队列 |
| [src/main.ts](../../src/main.ts) | 34 | 挂载最小授权弹窗 |
